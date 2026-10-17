use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("a record of {0} bytes cannot hold its own header")]
    RecordTooSmall(usize),
    #[error("payload of {len} bytes exceeds the {capacity} a record holds")]
    PayloadTooLarge { len: usize, capacity: usize },
}
