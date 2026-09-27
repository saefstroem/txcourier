use crate::rlp::error::Error;

pub type Result<T> = core::result::Result<T, Error>;
