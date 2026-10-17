// USB 2.0 section 8.3.5.1: the CRC5 covers the 11 bits of ADDR and ENDP, or of the SOF frame
// number.
const TOKEN_FIELD_BITS: u32 = 11;

// USB 2.0 section 8.3.5.1: G(X) = X^5 + X^2 + 1, 00101B, bit-reversed for the LSb-first loop.
const CRC5_POLYNOMIAL_REFLECTED: u8 = 0x14;
// USB 2.0 section 8.3.5: the generator is seeded with all ones, five bits for a token.
const CRC5_INITIAL: u8 = 0x1F;
// USB 2.0 section 8.3.5: the CRC is inverted before it is sent, five bits for a token.
const CRC5_FINAL_XOR: u8 = 0x1F;

// USB 2.0 section 8.3.5: the generator is seeded with all ones, 16 bits for data.
pub const CRC16_INITIAL: u16 = 0xFFFF;
// USB 2.0 section 8.3.5: the CRC is inverted before it is sent, 16 bits for data.
pub const CRC16_FINAL_XOR: u16 = 0xFFFF;

// Four bits are folded per step, through the 16-entry table below.
const NIBBLE_BITS: u32 = 4;
// (1 << NIBBLE_BITS) - 1: the index range of the fold table.
const NIBBLE_MASK: u16 = 0x0F;

// Each entry is its index folded four times through 0xA001, the bit reverse of USB 2.0
// section 8.3.5.2's X^16 + X^15 + X^2 + 1.
pub const CRC16_NIBBLE_TABLE: [u16; 16] = [
    0x0000, 0xCC01, 0xD801, 0x1400, 0xF001, 0x3C00, 0x2800, 0xE401, 0xA001, 0x6C00, 0x7800, 0xB401,
    0x5000, 0x9C01, 0x8801, 0x4400,
];

pub fn crc5(field: u16) -> u8 {
    // start with all ones
    let mut crc = CRC5_INITIAL;

    // walk the 11 bits, lowest first
    for position in 0..TOKEN_FIELD_BITS {
        // take the next bit of the field
        let bit = field >> position & 1 == 1;
        // compare it with the bit about to fall out
        crc = if (crc & 1 == 1) != bit {
            // they differ, shift and xor in the polynomial
            crc >> 1 ^ CRC5_POLYNOMIAL_REFLECTED
        } else {
            // they match, just shift
            crc >> 1
        };
    }

    // flip all bits at the end
    crc ^ CRC5_FINAL_XOR
}

pub fn crc16(bytes: &[u8]) -> u16 {
    // start with all ones
    let mut crc = CRC16_INITIAL;

    // fold each byte into the crc
    for byte in bytes {
        crc = crc16_fold(crc, *byte);
    }

    // flip all bits at the end
    crc ^ CRC16_FINAL_XOR
}

fn crc16_fold(crc: u16, byte: u8) -> u16 {
    // fold the low nibble first
    let low = fold_nibble(crc, u16::from(byte) & NIBBLE_MASK);
    // then the high nibble
    fold_nibble(low, u16::from(byte) >> NIBBLE_BITS & NIBBLE_MASK)
}

fn fold_nibble(crc: u16, nibble: u16) -> u16 {
    // pick the table entry for these 4 bits
    let index = usize::from((crc ^ nibble) & NIBBLE_MASK);
    // shift out 4 bits and xor in the table entry
    crc >> NIBBLE_BITS ^ CRC16_NIBBLE_TABLE.get(index).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::CRC16_NIBBLE_TABLE;
    use super::NIBBLE_BITS;
    use super::crc5;
    use super::crc16;

    const CRC16_POLYNOMIAL_REFLECTED: u16 = 0xA001;

    #[test]
    fn the_all_zero_token_field_has_the_crc_the_reference_encoder_expects() {
        assert_eq!(crc5(0x000), 0x02);
    }

    #[test]
    fn token_field_vectors_match_an_independent_implementation() {
        assert_eq!(crc5(0x2A5), 0x17);
        assert_eq!(crc5(0x710), 0x05);
        assert_eq!(crc5(0x53A), 0x07);
    }

    #[test]
    fn bits_above_the_eleventh_are_ignored() {
        assert_eq!(crc5(0x0800), crc5(0x0000));
        assert_eq!(crc5(0xF2A5), crc5(0x02A5));
    }

    #[test]
    fn the_catalogue_check_value_for_crc16_usb_is_reproduced() {
        assert_eq!(crc16(b"123456789"), 0xB4C8);
    }

    #[test]
    fn the_nibble_table_is_the_polynomial_shifted_four_times() {
        for (nibble, entry) in CRC16_NIBBLE_TABLE.iter().enumerate() {
            let mut crc = nibble as u16;
            for _ in 0..NIBBLE_BITS {
                crc = if crc & 1 == 1 {
                    crc >> 1 ^ CRC16_POLYNOMIAL_REFLECTED
                } else {
                    crc >> 1
                };
            }
            assert_eq!(crc, *entry, "nibble {nibble:#x}");
        }
    }

    #[test]
    fn the_get_descriptor_setup_packet_has_the_crc_the_reference_encoder_expects() {
        let setup = [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x08, 0x00];
        assert_eq!(crc16(&setup), 0x94EB);
    }

    #[test]
    fn the_descriptor_reply_from_the_ledger_has_the_crc_it_carried() {
        let payload = [0x12, 0x01, 0x10, 0x02, 0x00, 0x00, 0x00, 0x40];
        assert_eq!(crc16(&payload).to_le_bytes(), [0x55, 0x41]);
    }

    #[test]
    fn an_empty_payload_has_a_zero_crc() {
        assert_eq!(crc16(&[]), 0x0000);
    }

    #[test]
    fn a_single_bit_flip_changes_the_crc() {
        let original = [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x08, 0x00];
        for index in 0..original.len() {
            for bit in 0..8 {
                let mut flipped = original;
                flipped[index] ^= 1 << bit;
                assert_ne!(crc16(&flipped), crc16(&original));
            }
        }
    }
}
