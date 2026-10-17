use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("the token table is full")]
    TooManyTokens,
    #[error("a token descriptor must not be empty")]
    DescriptorMissing,
    #[error("token descriptor of {0} bytes exceeds the maximum")]
    DescriptorTooLong(usize),
    #[error("the token descriptor is not well formed")]
    DescriptorMalformed,
    #[error("the token descriptor names a different contract")]
    DescriptorAddressMismatch,
    #[error("the token descriptor is for a different chain")]
    DescriptorChainMismatch,
    #[error("record does not start with the token table magic")]
    BadMagic,
    #[error("token table record version {0} is not supported")]
    UnsupportedVersion(u8),
    #[error("token table record ends early")]
    Truncated,
    #[error("{0} unexpected bytes after the token table record")]
    TrailingBytes(usize),
    #[error("token table record checksum does not match")]
    ChecksumMismatch,
    #[error("token table record needs {needed} bytes, buffer holds {found}")]
    OutputTooSmall { needed: usize, found: usize },
}
