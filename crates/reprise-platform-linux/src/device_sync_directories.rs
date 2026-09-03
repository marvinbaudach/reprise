use std::time::Duration;

use gio::prelude::*;
use reprise_core::device_sync::fold_path;

use super::DeviceIoError;

const ENUMERATE_ATTRIBUTES: &str = "standard::name,standard::type,standard::size";
const ENUMERATE_BATCH_SIZE: i32 = 64;
/// How long a directory creation waits before its one retry. Long enough for
/// the device to finish whatever it was busy with, short enough that a run
/// meeting a genuinely broken target still ends in reasonable time.
const DIRECTORY_RETRY_DELAY: Duration = Duration::from_millis(250);

/// `<parent>/<components…>`, following the spellings the device reported.
pub(super) fn child_of(storage: &gio::File, components: &[String]) -> gio::File {
    components
        .iter()
        .fold(storage.clone(), |parent, component| parent.child(component))
}

/// Creates one directory under `parent` and returns the name it ends up having
/// there, which is not always the one that was asked for.
///
/// Only `G_IO_ERROR_EXISTS` used to be tolerated here. Android's emulated
/// storage is case-insensitive, so creating `Speaker of the Dead` next to a
/// resident `Speaker Of The Dead` is not a new folder at all — and libmtp
/// answers that with `Could not send object info` rather than `EXISTS`, which
/// killed the whole track. Every other failure is the device being flaky on its
/// own; those heal on a repeat.
pub(super) async fn ensure_directory(
    parent: &gio::File,
    desired: String,
    cancellable: Option<&gio::Cancellable>,
) -> Result<String, DeviceIoError> {
    let error = match make_directory(parent.child(&desired), cancellable).await {
        Ok(()) => return Ok(desired),
        Err(error) if error.matches(gio::IOErrorEnum::Exists) => return Ok(desired),
        Err(error) => error,
    };
    if let Some(resident) = resident_fold_equal_directory(parent, &desired, cancellable, true).await
    {
        // Also worth a line when the resident name *is* the desired one: the
        // device answered an error for a directory that is demonstrably there,
        // and that is the whole reason this rescue exists.
        tracing::warn!(
            desired = %desired,
            resident = %resident,
            first_error = %error,
            "device sync: creating this directory failed, but a fold-equal one is already on the device"
        );
        return Ok(resident);
    }
    check_cancelled(cancellable)?;
    gio::glib::timeout_future(DIRECTORY_RETRY_DELAY).await;
    check_cancelled(cancellable)?;
    match make_directory(parent.child(&desired), cancellable).await {
        Ok(()) => {}
        Err(retry) if retry.matches(gio::IOErrorEnum::Exists) => {}
        // The retry's error says nothing the first one did not; the caller is
        // told what actually went wrong the first time.
        Err(_) => return Err(error.into()),
    }
    tracing::warn!(
        desired = %desired,
        first_error = %error,
        "device sync: the device refused this directory once and accepted it on the retry"
    );
    Ok(desired)
}

/// Resolves one existing directory without creating it. If no fold-equal
/// resident can be observed, the caller keeps using the desired spelling and
/// lets its read or delete operation report absence in the usual way.
pub(super) async fn resolve_directory(parent: &gio::File, desired: String) -> String {
    resident_fold_equal_directory(parent, &desired, None, false)
        .await
        .unwrap_or(desired)
}

/// The name a fold-equal directory already carries under `parent`, if exactly
/// one does.
///
/// This has to read a listing: querying the desired name would miss the
/// collision, because that name is precisely the one that is not there. Two
/// resident spellings side by side leave the choice open — the device really
/// does report both as separate MTP folders — and inventing one is the call
/// `device_case` refuses to make as well.
async fn resident_fold_equal_directory(
    parent: &gio::File,
    desired: &str,
    cancellable: Option<&gio::Cancellable>,
    warn_on_unreadable: bool,
) -> Option<String> {
    let enumerator = match enumerate_directories(parent, cancellable).await {
        Ok(enumerator) => enumerator,
        Err(error) => {
            if warn_on_unreadable {
                warn_unreadable_listing(desired, &error);
            }
            return None;
        }
    };
    let folded = fold_path(desired);
    let mut resident = Vec::new();
    loop {
        let batch = match next_files(&enumerator, cancellable).await {
            Ok(batch) => batch,
            // A remote enumerator can disappear between batches, and then a
            // resident spelling that is really there looks like an absent one.
            Err(error) => {
                if warn_on_unreadable {
                    warn_unreadable_listing(desired, &error);
                }
                return None;
            }
        };
        if batch.is_empty() {
            break;
        }
        for info in batch {
            if info.file_type() != gio::FileType::Directory {
                continue;
            }
            let name = info.name().to_string_lossy().into_owned();
            if name == desired {
                return Some(name);
            }
            if fold_path(&name) == folded {
                resident.push(name);
            }
        }
    }
    match resident.as_slice() {
        [single] => Some(single.clone()),
        [first, second, ..] => {
            tracing::warn!(
                desired,
                first,
                second,
                "device sync: refused to choose between fold-equal resident directories"
            );
            None
        }
        [] => None,
    }
}

async fn make_directory(
    directory: gio::File,
    cancellable: Option<&gio::Cancellable>,
) -> Result<(), gio::glib::Error> {
    let (sender, receiver) = async_channel::bounded(1);
    directory.make_directory_async(gio::glib::Priority::DEFAULT, cancellable, move |result| {
        let _ = sender.try_send(result);
    });
    receiver
        .recv()
        .await
        .expect("GIO directory creation callback dropped")
}

async fn enumerate_directories(
    parent: &gio::File,
    cancellable: Option<&gio::Cancellable>,
) -> Result<gio::FileEnumerator, gio::glib::Error> {
    let (sender, receiver) = async_channel::bounded(1);
    parent.enumerate_children_async(
        ENUMERATE_ATTRIBUTES,
        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
        gio::glib::Priority::DEFAULT,
        cancellable,
        move |result| {
            let _ = sender.try_send(result);
        },
    );
    receiver
        .recv()
        .await
        .expect("GIO directory enumeration callback dropped")
}

async fn next_files(
    enumerator: &gio::FileEnumerator,
    cancellable: Option<&gio::Cancellable>,
) -> Result<Vec<gio::FileInfo>, gio::glib::Error> {
    let (sender, receiver) = async_channel::bounded(1);
    enumerator.next_files_async(
        ENUMERATE_BATCH_SIZE,
        gio::glib::Priority::DEFAULT,
        cancellable,
        move |result| {
            let _ = sender.try_send(result);
        },
    );
    receiver
        .recv()
        .await
        .expect("GIO directory batch callback dropped")
}

fn check_cancelled(cancellable: Option<&gio::Cancellable>) -> Result<(), DeviceIoError> {
    if cancellable.is_some_and(gio::Cancellable::is_cancelled) {
        return Err(
            gio::glib::Error::new(gio::IOErrorEnum::Cancelled, "Operation cancelled").into(),
        );
    }
    Ok(())
}

/// Says why a directory could not be adopted, so a run that ends in the
/// original creation error still names the step that gave up on rescuing it.
fn warn_unreadable_listing(desired: &str, error: &gio::glib::Error) {
    tracing::warn!(
        desired = %desired,
        %error,
        "device sync: could not list the parent directory, so no resident spelling was adopted"
    );
}
