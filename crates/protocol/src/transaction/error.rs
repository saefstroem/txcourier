use thiserror::Error;

use crate::rlp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error(transparent)]
    Rlp(#[from] rlp::Error),
    #[error("transaction is empty")]
    Empty,
    #[error("transaction type {0:#04x} is not EIP-1559")]
    UnsupportedType(u8),
    #[error("transaction has too few fields")]
    TooFewFields,
    #[error("transaction has too many fields")]
    TooManyFields,
    #[error("an integer field has a leading zero")]
    NonCanonicalInteger,
    #[error("an integer field is wider than allowed")]
    IntegerTooLarge,
    #[error("address is {0} bytes, not 20")]
    BadAddressLength(usize),
    #[error("access list is not empty")]
    AccessListNotEmpty,
    #[error("signature recovery byte {0} is not a parity")]
    BadParity(u8),
}
