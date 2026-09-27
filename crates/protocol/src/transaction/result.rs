use crate::transaction::error::Error;

pub type Result<T> = core::result::Result<T, Error>;
