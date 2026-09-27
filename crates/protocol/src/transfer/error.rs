use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("transaction creates a contract")]
    ContractCreation,
    #[error("a token transfer must not carry native value")]
    ValueWithTokenTransfer,
    #[error("calldata is {0} bytes, not the 68 of transfer(address,uint256)")]
    BadCalldataLength(usize),
    #[error(
        "selector {:02x}{:02x}{:02x}{:02x} is not transfer(address,uint256)",
        .0[0], .0[1], .0[2], .0[3]
    )]
    UnsupportedSelector([u8; 4]),
    #[error("the recipient word is not a left-padded address")]
    BadAddressPadding,
}
