use crate::bitcoin::RECORD_LEN as BITCOIN_RECORD_LEN;
use crate::evm::RECORD_LEN;
use crate::wire::bytes::Bytes;

pub type EVMRecord = Bytes<RECORD_LEN>;

pub type BitcoinRecord = Bytes<BITCOIN_RECORD_LEN>;

#[cfg(test)]
mod tests {
    use super::EVMRecord;
    use crate::evm::RECORD_LEN;

    #[test]
    fn a_record_fits_and_one_byte_more_does_not() {
        assert!(EVMRecord::from_slice(&[0u8; RECORD_LEN]).is_ok());
        assert!(EVMRecord::from_slice(&[0u8; RECORD_LEN + 1]).is_err());
    }
}
