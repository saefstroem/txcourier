use crate::ledger::error::Error;

pub type Result<T> = core::result::Result<T, Error>;
