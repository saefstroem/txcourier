use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("rlp item needs {needed} bytes, only {found} present")]
    Truncated { needed: usize, found: usize },
    #[error("rlp encoding is not the canonical one")]
    NonCanonical,
    #[error("rlp length does not fit")]
    LengthOverflow,
    #[error("{0} bytes follow the rlp item")]
    TrailingBytes(usize),
    #[error("expected an rlp byte string, found a list")]
    ExpectedBytes,
    #[error("expected an rlp list, found a byte string")]
    ExpectedList,
    #[error("rlp output needs {needed} bytes, buffer holds {found}")]
    OutputTooSmall { needed: usize, found: usize },
}
