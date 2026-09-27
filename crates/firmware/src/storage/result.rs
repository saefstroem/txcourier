use crate::storage::error::Error;

pub type Result<T> = core::result::Result<T, Error>;
