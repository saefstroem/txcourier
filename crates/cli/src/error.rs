use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

use rp2350wallet_protocol::wire::Refusal;
use rp2350wallet_protocol::{token, wire};
use thiserror::Error;

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
    #[error("{0} is not an even-length hex string")]
    NotHex(String),
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
    use rp2350wallet_protocol::wire::Refusal;

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
