use std::sync::Mutex;

use rusqlite::Connection;

use crate::db::Db;

use super::ScanError;

pub trait ScanWriter {
    /// Runs `work` with the writer connection, then gives it back. The scanner
    /// opens and commits one transaction inside every lease; a lease is never
    /// held across two batches.
    fn lease(
        &self,
        work: &mut dyn FnMut(&Connection) -> Result<(), ScanError>,
    ) -> Result<(), ScanError>;
}

impl ScanWriter for Db {
    fn lease(
        &self,
        work: &mut dyn FnMut(&Connection) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        work(self.conn())
    }
}

impl ScanWriter for Connection {
    fn lease(
        &self,
        work: &mut dyn FnMut(&Connection) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        work(self)
    }
}

impl ScanWriter for Mutex<Db> {
    fn lease(
        &self,
        work: &mut dyn FnMut(&Connection) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        let database = self.lock().map_err(|_| ScanError::WriterPoisoned)?;
        work(database.conn())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::db::Db;

    use super::{ScanError, ScanWriter};

    #[test]
    fn mutex_writer_is_released_between_leases() {
        let writer = Arc::new(Mutex::new(Db::open_in_memory().unwrap()));
        writer
            .lease(&mut |_| Ok(()))
            .expect("the first lease succeeds");

        let other_thread = Arc::clone(&writer);
        let available = std::thread::spawn(move || other_thread.try_lock().is_ok())
            .join()
            .unwrap();
        assert!(available, "the completed lease must give the mutex back");

        writer
            .lease(&mut |_| Ok(()))
            .expect("a later lease can reacquire the writer");
    }

    #[test]
    fn poisoned_mutex_is_a_scan_error() {
        let writer = Arc::new(Mutex::new(Db::open_in_memory().unwrap()));
        let poison = Arc::clone(&writer);
        let _ = std::thread::spawn(move || {
            let _guard = poison.lock().unwrap();
            panic!("poison the fixture writer");
        })
        .join();

        assert!(matches!(
            writer.lease(&mut |_| Ok(())),
            Err(ScanError::WriterPoisoned)
        ));
    }
}
