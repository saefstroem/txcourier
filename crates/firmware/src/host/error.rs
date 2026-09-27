use rp235x_hal as hal;

use hal::pio::InstallError;
use txcourier_pio_usb::Error as PacketError;
use txcourier_protocol::ledger::STATUS_DENIED;
use txcourier_protocol::wire::Refusal;
use txcourier_protocol::{ledger, transaction};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Error {
    ProgramInstall,
    PacketTooLong(usize),
    TransmitterUnavailable,
    TransmitStuck,
    TransmitUnderrun,
    ReceiverUnavailable,
    Packet(PacketError),
    NoReply,
    NegativeAcknowledge,
    Stall,
    CorruptReply,
    UnexpectedReply,
    ReplyTooLong,
    NoVendorInterface,
    UnexpectedVendor(u16),
    Ledger(ledger::Error),
    Transaction(transaction::Error),
    LedgerSendFailed,
    LedgerSilent,
    NoTokenDescriptor,
}

impl From<InstallError> for Error {
    fn from(_: InstallError) -> Self {
        Self::ProgramInstall
    }
}

impl From<PacketError> for Error {
    fn from(error: PacketError) -> Self {
        Self::Packet(error)
    }
}

impl From<ledger::Error> for Error {
    fn from(error: ledger::Error) -> Self {
        Self::Ledger(error)
    }
}

impl From<transaction::Error> for Error {
    fn from(error: transaction::Error) -> Self {
        Self::Transaction(error)
    }
}

impl From<Error> for Refusal {
    fn from(error: Error) -> Self {
        match error {
            Error::Ledger(ledger::Error::Status(STATUS_DENIED)) => Self::LedgerDeclined,
            Error::Ledger(_) | Error::Transaction(_) | Error::Stall => Self::LedgerFailed,
            Error::NegativeAcknowledge | Error::LedgerSilent => Self::SigningTimedOut,
            Error::LedgerSendFailed => Self::NoLedger,
            Error::NoTokenDescriptor => Self::NoTokenDescriptor,
            Error::NoReply | Error::CorruptReply | Error::UnexpectedReply => Self::NoLedger,
            Error::NoVendorInterface | Error::UnexpectedVendor(_) => Self::NoLedger,
            _ => Self::LedgerFailed,
        }
    }
}
