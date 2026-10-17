use thiserror::Error;

use crate::{transaction, transfer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error(transparent)]
    Transaction(#[from] transaction::Error),
    #[error(transparent)]
    Transfer(#[from] transfer::Error),
    #[error("{len} unsigned bytes cannot be re-encoded with a signature inside {capacity}")]
    TooLongToSign { len: usize, capacity: usize },
}
