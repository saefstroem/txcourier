use thiserror::Error;

use crate::transaction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("a draft record is {0} bytes, not 101")]
    WrongLength(usize),
    #[error("that asset is not built in for that chain")]
    UnknownAsset,
    #[error("the amount has more decimal places than its asset")]
    TooPrecise,
    #[error("the amount is more than 128 bits of base units")]
    AmountTooLarge,
    #[error("the recipient is the token's own contract")]
    RecipientIsContract,
    #[error(transparent)]
    Transaction(#[from] transaction::Error),
}
