use thiserror::Error;

use crate::{bitcoin, transaction};

// ISO 7816-4 Table 6: '6985' is "Conditions of use not satisfied". app-ethereum returns it
// from io_seproxyhal_touch_tx_cancel(), so here it means the user pressed reject.
pub const STATUS_DENIED: u16 = 0x6985;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("an apdu carries at most 255 bytes, not {0}")]
    DataTooLong(usize),
    #[error("needs {needed} bytes, buffer holds {found}")]
    OutputTooSmall { needed: usize, found: usize },
    #[error("the report is not 64 bytes of ledger framing")]
    ReportMalformed,
    #[error("the report is for channel {0:#06x}, not ours")]
    WrongChannel(u16),
    #[error("report tag {0:#04x} is not an apdu frame")]
    WrongTag(u8),
    #[error("expected report {expected} of the reply, got {found}")]
    OutOfSequence { expected: u16, found: u16 },
    #[error("the reply of {0} bytes exceeds the receive buffer")]
    ResponseTooLong(usize),
    #[error("the reply ended before its declared length")]
    ResponseTruncated,
    #[error("the Ledger's fingerprint or account key is not in the expected form")]
    MalformedKey,
    #[error("the Ledger asked for data this wallet does not commit to")]
    UnknownCommitment,
    #[error("the Ledger asked for client command {0:#04x}")]
    UnexpectedClientCommand(u8),
    #[error("the Ledger did not sign every input")]
    MissingSignature,
    #[error(transparent)]
    Bitcoin(#[from] bitcoin::Error),
    #[error("the Ledger answered {0:#06x}")]
    Status(u16),
    #[error(transparent)]
    Signature(#[from] transaction::Error),
}
