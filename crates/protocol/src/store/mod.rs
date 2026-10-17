mod error;
mod layout;
mod record;
mod result;
mod sector;

pub use error::Error;
pub use layout::SECTOR_LEN;
pub use layout::SLOT_RECORD_LEN;
pub use record::Kind;
pub use record::Record;
pub use record::Slot;
pub use record::decode;
pub use record::encode;
pub use record::payload_capacity;
pub use result::Result;
pub use sector::Scan;
pub use sector::scan;
