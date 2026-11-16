use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("the Bitcoin draft is not one coin paying a 42-character bc1q address")]
    Malformed,
    #[error("the coin does not cover the amount and the fee")]
    Short,
    #[error("the payment is too small for nodes to relay")]
    Dust,
    #[error("needs more than the {0} bytes the buffer holds")]
    OutputTooSmall(usize),
}
