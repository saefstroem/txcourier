use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("frame is {found} bytes, needs at least {needed}")]
    TooShort { needed: usize, found: usize },
    #[error("frame does not start with the protocol magic")]
    BadMagic,
    #[error("unsupported protocol version {0:#04x}")]
    UnsupportedVersion(u8),
    #[error("payload of {0} bytes exceeds the maximum")]
    PayloadTooLarge(usize),
    #[error("{0} unexpected bytes after the frame")]
    TrailingBytes(usize),
    #[error("unknown opcode {0:#04x}")]
    UnknownOpcode(u8),
    #[error("output buffer is {found} bytes, needs {needed}")]
    OutputTooSmall { needed: usize, found: usize },
    #[error("payload does not match its opcode")]
    MalformedPayload,
}
