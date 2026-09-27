use core::fmt::Formatter;
use core::fmt::Result as FmtResult;

use thiserror::Error;

use crate::transaction;

// ISO 7816-4 Table 6: '6985' is "Conditions of use not satisfied". app-ethereum returns it
// from io_seproxyhal_touch_tx_cancel(), so here it means the user pressed reject.
pub const STATUS_DENIED: u16 = 0x6985;
// ISO 7816-4 Table 6: '6A80' is "Incorrect parameters in the command data field".
// app-ethereum returns it for an unparseable transaction or a badly signed token descriptor.
const STATUS_BAD_DATA: u16 = 0x6A80;
// No source. Absent from ledger-secure-sdk and from @ledgerhq/errors, whose locked-device
// word is 0x5515. '6BXX' is a proprietary range in ISO 7816-4.
const STATUS_LOCKED: u16 = 0x6B0C;
// ISO 7816-4 Table 5: '6D00' is "Instruction code not supported or invalid". app-ethereum
// src/main.c returns it for an unknown INS. This crate reads it as the wrong app being open.
const STATUS_UNKNOWN_INSTRUCTION: u16 = 0x6D00;
// ISO 7816-4 Table 5: '6E00' is "Class not supported". app-ethereum src/main.c returns it
// for any class byte other than 0xE0.
const STATUS_UNKNOWN_CLASS: u16 = 0x6E00;

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
    #[error(fmt = describe_status)]
    Status(u16),
    #[error(transparent)]
    Signature(#[from] transaction::Error),
}

fn describe_status(status: &u16, f: &mut Formatter<'_>) -> FmtResult {
    match *status {
        STATUS_DENIED => write!(f, "you declined the transaction on the device"),
        STATUS_LOCKED => write!(f, "the device is locked"),
        STATUS_BAD_DATA => write!(f, "the device rejected the transaction data"),
        STATUS_UNKNOWN_INSTRUCTION => write!(
            f,
            "the device does not know that command: is the Ethereum app open?"
        ),
        STATUS_UNKNOWN_CLASS => write!(f, "the device does not know that command class"),
        other => write!(f, "the device answered {other:#06x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn the_statuses_a_user_will_actually_hit_read_as_plain_english() {
        assert!(Error::Status(0x6985).to_string().contains("declined"));
        assert!(Error::Status(0x6B0C).to_string().contains("locked"));
        assert!(Error::Status(0x6D00).to_string().contains("Ethereum app"));
    }

    #[test]
    fn the_remaining_named_statuses_say_what_went_wrong() {
        assert!(
            Error::Status(0x6A80)
                .to_string()
                .contains("rejected the transaction data")
        );
        assert!(Error::Status(0x6E00).to_string().contains("command class"));
    }

    #[test]
    fn an_unrecognised_status_still_reports_its_number() {
        assert!(Error::Status(0x1234).to_string().contains("0x1234"));
    }
}
