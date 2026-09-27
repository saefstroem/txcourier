use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("encoded packet needs {needed} bytes, buffer holds {found}")]
    OutputTooSmall { needed: usize, found: usize },
    #[error("packet of {0} bytes is too short")]
    PacketTooShort(usize),
    #[error("packet starts with {0:#04x}, not SYNC")]
    BadSync(u8),
    #[error("packet identifier {0:#04x} is not DATA0 or DATA1")]
    NotData(u8),
    #[error("crc16 {declared:#06x} does not match {computed:#06x}")]
    ChecksumMismatch { declared: u16, computed: u16 },
}
