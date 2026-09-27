use crate::crc::crc5;
use crate::pid;

// USB 2.0 section 7.1.10: the full-speed SYNC is KJKJKJKK. Sent LSb first, this byte encodes
// it.
pub const SYNC: u8 = 0x80;

// USB 2.0 section 8.3.2.1: the function address is seven bits, ADDR<6:0>.
const ADDRESS_MASK: u16 = 0x7F;
// USB 2.0 section 8.3.2.2: the endpoint field is four bits.
const ENDPOINT_MASK: u16 = 0x0F;
// USB 2.0 Figure 8-5: ENDP directly follows the seven-bit ADDR in the LSb-first token.
const ENDPOINT_SHIFT: u32 = 7;
// USB 2.0 section 8.3.3: the frame number is 11 bits and rolls over at 7FFH.
const FRAME_MASK: u16 = 0x7FF;
// The CRC5 starts at bit 11 of the ADDR and ENDP field, three bits into its high byte.
const CRC_SHIFT: u32 = 3;

/// tokens open every transaction and explain who it is for
pub fn token(pid: u8, address: u8, endpoint: u8) -> [u8; 4] {
    let endpoint_field = (u16::from(endpoint) & ENDPOINT_MASK) << ENDPOINT_SHIFT;
    let address_field = u16::from(address) & ADDRESS_MASK;
    // essentially joining addr and endp bits
    packet(pid, endpoint_field | address_field)
}

/// used for keep alive
pub fn start_of_frame(frame: u16) -> [u8; 4] {
    packet(pid::SOF, frame & FRAME_MASK)
}

/// Serializes the packet with sync and crc
fn packet(pid: u8, field: u16) -> [u8; 4] {
    let [low, high] = field.to_le_bytes();
    [SYNC, pid, low, crc5(field) << CRC_SHIFT | high]
}

#[cfg(test)]
mod tests {
    use super::start_of_frame;
    use super::token;
    use crate::pid;

    #[test]
    fn an_in_token_to_the_default_address_matches_the_reference_encoder() {
        assert_eq!(token(pid::IN, 0, 0), [0x80, 0x69, 0x00, 0x10]);
    }

    #[test]
    fn a_setup_token_to_the_default_address_matches_the_reference_encoder() {
        assert_eq!(token(pid::SETUP, 0, 0), [0x80, 0x2D, 0x00, 0x10]);
    }

    #[test]
    fn frame_zero_matches_the_reference_encoder() {
        assert_eq!(start_of_frame(0), [0x80, 0xA5, 0x00, 0x10]);
    }

    #[test]
    fn a_frame_number_spans_the_low_byte_and_three_high_bits() {
        assert_eq!(start_of_frame(0x2A5), [0x80, 0xA5, 0xA5, 0xBA]);
    }

    #[test]
    fn the_endpoint_sits_above_the_address() {
        let [_, _, low, high] = token(pid::IN, 0x7F, 0x0F);
        assert_eq!(low, 0xFF);
        assert_eq!(high & 0x07, 0x07);
    }

    #[test]
    fn bits_that_do_not_fit_the_fields_are_dropped() {
        assert_eq!(token(pid::IN, 0x80, 0x10), token(pid::IN, 0, 0));
        assert_eq!(start_of_frame(0x800), start_of_frame(0));
    }
}
