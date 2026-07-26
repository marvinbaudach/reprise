use std::io::Read;

pub(crate) const MAX_JSON_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoundedReadError {
    Read,
    TooLarge,
}

pub(crate) fn read_bounded_string(reader: impl Read) -> Result<String, BoundedReadError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_JSON_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| BoundedReadError::Read)?;
    if bytes.len() as u64 > MAX_JSON_RESPONSE_BYTES {
        return Err(BoundedReadError::TooLarge);
    }
    String::from_utf8(bytes).map_err(|_| BoundedReadError::Read)
}
