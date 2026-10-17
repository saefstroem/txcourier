use thiserror::Error;

use crate::wire;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("unknown device state byte {0:#04x}")]
    UnknownStateByte(u8),
    #[error(transparent)]
    Wire(#[from] wire::Error),
}
