// CRC-32/ISO-HDLC, as in zlib, gzip and PNG: 0x04C11DB7 bit-reversed for the LSb-first loop.
const POLYNOMIAL: u32 = 0xEDB8_8320;
// CRC-32/ISO-HDLC preconditions the register to all ones; the final complement undoes it.
const SEED: u32 = 0xFFFF_FFFF;

pub fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = SEED;

    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (POLYNOMIAL & mask);
        }
    }

    !crc
}

#[cfg(test)]
mod tests {
    use super::checksum;

    #[test]
    fn the_empty_input_is_zero() {
        assert_eq!(checksum(&[]), 0);
    }

    #[test]
    fn the_published_check_vector_matches() {
        assert_eq!(checksum(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn a_single_zero_byte_matches_the_published_value() {
        assert_eq!(checksum(&[0x00]), 0xD202_EF8D);
    }

    #[test]
    fn a_flipped_bit_changes_the_checksum() {
        let original = checksum(b"pending transaction");
        let mut corrupted = *b"pending transaction";
        corrupted[0] ^= 0x01;
        assert_ne!(checksum(&corrupted), original);
    }

    #[test]
    fn a_truncated_input_changes_the_checksum() {
        assert_ne!(checksum(b"abcdef"), checksum(b"abcde"));
    }

    #[test]
    fn reordering_bytes_changes_the_checksum() {
        assert_ne!(checksum(b"ab"), checksum(b"ba"));
    }

    #[test]
    fn leading_zeros_are_not_ignored() {
        assert_ne!(checksum(&[0x00, 0x01]), checksum(&[0x01]));
    }
}
