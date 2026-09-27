use crate::store::{SLOT_RECORD_LEN, payload_capacity};
use crate::wire::bytes::Bytes;

// What one 256-byte flash slot record holds after its 8-byte header and 4-byte CRC32.
pub const TRANSACTION_CAPACITY: usize = 244;

const _: () = assert!(TRANSACTION_CAPACITY == payload_capacity(SLOT_RECORD_LEN));

pub type Transaction = Bytes<TRANSACTION_CAPACITY>;

#[cfg(test)]
mod tests {
    use super::TRANSACTION_CAPACITY;
    use super::Transaction;

    #[test]
    fn a_transaction_at_capacity_fits_and_one_more_does_not() {
        assert!(Transaction::from_slice(&[0u8; TRANSACTION_CAPACITY]).is_ok());
        assert!(Transaction::from_slice(&[0u8; TRANSACTION_CAPACITY + 1]).is_err());
    }
}
