use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("the token descriptor is not well formed")]
    Malformed,
    #[error("the token descriptor names a different contract")]
    AddressMismatch,
    #[error("the token descriptor is for a different chain")]
    ChainMismatch,
}
