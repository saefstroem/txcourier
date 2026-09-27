use rp2350wallet_protocol::wire;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    #[error(
        "no courier found: expected a /dev/cu.usbmodem* device. Is it plugged in and running the firmware?"
    )]
    NoPortFound,
    #[error("cannot list /dev: {0}")]
    DeviceDirectoryUnreadable(String),
    #[error("cannot open {path}: {reason}")]
    PortUnopenable { path: String, reason: String },
    #[error("cannot read the port's terminal settings")]
    TerminalSettingsUnreadable,
    #[error("cannot put the port into raw mode")]
    TerminalSettingsUnwritable,
    #[error("write to the courier failed: {0}")]
    WriteFailed(String),
    #[error("read from the courier failed: {0}")]
    ReadFailed(String),
    #[error("the courier did not reply: wanted {expected} bytes, got {received}")]
    Timeout { expected: usize, received: usize },
    #[error("frame exceeds the protocol maximum")]
    FrameTooLarge,
    #[error("the courier answered with opcode {0:#04x}, which does not match the request")]
    UnexpectedResponse(u8),
    #[error(transparent)]
    Wire(#[from] wire::Error),
}
