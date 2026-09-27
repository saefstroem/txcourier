use usb_device::UsbError;
use usb_device::prelude::BuilderError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Error {
    Endpoint(UsbError),
    Builder(BuilderError),
}

impl From<UsbError> for Error {
    fn from(error: UsbError) -> Self {
        Self::Endpoint(error)
    }
}

impl From<BuilderError> for Error {
    fn from(error: BuilderError) -> Self {
        Self::Builder(error)
    }
}
