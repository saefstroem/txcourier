use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("not a configuration descriptor")]
    NotAConfiguration,
    #[error("not an eighteen-byte device descriptor")]
    NotADevice,
    #[error("configuration descriptor ends before the APDU interface is complete")]
    Truncated,
    #[error("configuration declares {0} interfaces")]
    UnexpectedInterfaceCount(u8),
    #[error("the first interface is not the Ledger APDU HID interface")]
    NotApduInterface,
    #[error("the APDU interface is not followed by a HID class descriptor")]
    MissingHidDescriptor,
    #[error("endpoint {0:#04x} is not the expected APDU endpoint")]
    UnexpectedEndpoint(u8),
    #[error("endpoint {0:#04x} is not an interrupt endpoint")]
    NotInterruptEndpoint(u8),
    #[error("endpoint packet size {0} does not match the APDU report size")]
    UnexpectedPacketSize(u16),
}
