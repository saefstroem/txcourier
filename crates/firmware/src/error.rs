use rp235x_hal as hal;

use hal::clocks::ClockError;
use hal::pll::Error as PllError;
use hal::xosc::Error as XoscError;
use rp2350wallet_protocol::wire;

use crate::{host, storage, usb};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Error {
    PeripheralsUnavailable,
    CrystalOscillator,
    PhaseLockedLoop,
    ClockConfiguration,
    Host(host::Error),
    Storage(storage::Error),
    Usb(usb::Error),
    Wire(wire::Error),
}

impl From<host::Error> for Error {
    fn from(error: host::Error) -> Self {
        Self::Host(error)
    }
}

impl From<storage::Error> for Error {
    fn from(error: storage::Error) -> Self {
        Self::Storage(error)
    }
}

impl From<usb::Error> for Error {
    fn from(error: usb::Error) -> Self {
        Self::Usb(error)
    }
}

impl From<wire::Error> for Error {
    fn from(error: wire::Error) -> Self {
        Self::Wire(error)
    }
}

impl From<XoscError> for Error {
    fn from(_: XoscError) -> Self {
        Self::CrystalOscillator
    }
}

impl From<PllError> for Error {
    fn from(_: PllError) -> Self {
        Self::PhaseLockedLoop
    }
}

impl From<ClockError> for Error {
    fn from(_: ClockError) -> Self {
        Self::ClockConfiguration
    }
}
