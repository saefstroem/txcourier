#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        dead_code,
        unused_must_use
    )
)]

mod bus;
mod crc32;
mod lamp;

mod state;

pub mod bitcoin;
pub mod evm;
pub mod ledger;
pub mod policy;
pub mod rlp;
pub mod slots;
pub mod store;
pub mod token;
pub mod transaction;
pub mod transfer;
pub mod usb;
pub mod wire;

pub use bus::BUS_SAMPLES;
pub use lamp::lamp_color;
pub use state::DeviceState;
pub use state::FAULT_BYTE;
