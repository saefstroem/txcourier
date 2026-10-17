use txcourier_protocol::wire::Refusal;
use txcourier_protocol::{slots, store};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Slots(slots::Error),
    Record(store::Error),
    RegionUnsupported(usize),
}

impl From<slots::Error> for Error {
    fn from(error: slots::Error) -> Self {
        Self::Slots(error)
    }
}

impl From<store::Error> for Error {
    fn from(error: store::Error) -> Self {
        Self::Record(error)
    }
}

impl From<Error> for Refusal {
    fn from(error: Error) -> Self {
        match error {
            Error::Slots(error) => Self::from(error),
            Error::Record(error) => Self::from(error),
            Error::RegionUnsupported(_) => Self::StorageFull,
        }
    }
}
