use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

use alloy::primitives::B256;
use thiserror::Error;
use txcourier_protocol::wire::Refusal;
use txcourier_protocol::{policy, token, transaction, transfer, wire};

use crate::device;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    #[error(transparent)]
    Device(#[from] device::Error),
    #[error(transparent)]
    Wire(#[from] wire::Error),
    #[error(fmt = describe_refusal)]
    Refused { refusal: Refusal, detail: u16 },
    #[error(transparent)]
    Token(#[from] token::Error),
    #[error("line {number} of the token table: {reason}")]
    TokenLine { number: usize, reason: String },
    #[error("cannot read {path}: {reason}")]
    Unreadable { path: String, reason: String },
    #[error(transparent)]
    Transaction(#[from] transaction::Error),
    #[error(transparent)]
    Transfer(#[from] transfer::Error),
    #[error("the courier would refuse this transaction: {0}")]
    Policy(#[from] policy::Error),
    #[error("cannot start the network runtime: {0}")]
    Runtime(String),
    #[error("the node refused: {0}")]
    NodeRefused(String),
    #[error("{0}")]
    Transport(String),
    #[error("no answer within {millis} ms")]
    NoAnswer { millis: u128 },
    #[error("the node answered with hash {answered}, but the transaction hashes to {expected}")]
    HashMismatch { answered: B256, expected: B256 },
    #[error("the fee history has no base fee")]
    NoBaseFee,
    #[error("no node is known for chain {0}, so it cannot be published from here")]
    NoNode(u64),
    #[error("no node accepted the transaction")]
    NotPublished,
    #[error(
        "no node confirmed the transaction, but it may have gone out: check {0} before signing a replacement; publishing this one again is safe"
    )]
    Unconfirmed(String),
    #[error("the prompt failed: {0}")]
    Prompt(String),
    #[error("interrupted")]
    Interrupted,
}

pub fn explain(refusal: Refusal, detail: u16) -> String {
    match detail {
        0 => refusal.describe().to_owned(),
        detail => format!("{} (the Ledger answered {detail:#06x})", refusal.describe()),
    }
}

fn describe_refusal(refusal: &Refusal, detail: &u16, f: &mut Formatter<'_>) -> FmtResult {
    write!(f, "the courier refused: {}", explain(*refusal, *detail))
}

#[cfg(test)]
mod tests {
    use txcourier_protocol::wire::Refusal;

    use super::explain;

    #[test]
    fn a_refusal_the_status_line_prints_carries_no_error_prefix() {
        assert_eq!(
            explain(Refusal::NothingToSign, 0),
            Refusal::NothingToSign.describe()
        );
        assert_eq!(
            explain(Refusal::LedgerFailed, 0x6A80),
            format!(
                "{} (the Ledger answered 0x6a80)",
                Refusal::LedgerFailed.describe()
            )
        );
    }
}
