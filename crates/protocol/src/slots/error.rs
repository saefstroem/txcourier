use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("both slots are occupied, which must never happen")]
    BothSlotsOccupied,
    #[error("a pending transaction is already stored")]
    PendingAlreadyStored,
    #[error("a signed transaction is waiting to be collected")]
    SignedNotCollected,
    #[error("there is no pending transaction to sign")]
    NothingToSign,
    #[error("there is no signed transaction to clear")]
    NothingSigned,
}
