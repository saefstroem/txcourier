use crate::draft::RECORD_LEN;
use crate::wire::bytes::Bytes;

pub type Record = Bytes<RECORD_LEN>;

#[cfg(test)]
mod tests {
    use super::Record;
    use crate::draft::RECORD_LEN;

    #[test]
    fn a_record_fits_and_one_byte_more_does_not() {
        assert!(Record::from_slice(&[0u8; RECORD_LEN]).is_ok());
        assert!(Record::from_slice(&[0u8; RECORD_LEN + 1]).is_err());
    }
}
