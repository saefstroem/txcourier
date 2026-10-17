use crate::wire::bytes::Bytes;

// USB 2.0 section 8.2: an eight-bit SYNC precedes every low- and full-speed packet.
const SYNC_LEN: usize = 1;
// USB 2.0 section 8.3.1: a PID is four type bits and their four complement bits.
const PID_LEN: usize = 1;
// USB 2.0: a full-speed bulk or interrupt data payload is at most 64 bytes, which is what
// the Nano S Plus's HID endpoints report.
const MAX_DATA_LEN: usize = 64;
// USB 2.0 section 8.3.5.2: a data packet carries a 16-bit CRC over the payload.
const CRC16_LEN: usize = 2;

// SYNC, PID, a full 64-byte payload and CRC16: the longest full-speed data packet.
pub const PACKET_CAPACITY: usize = SYNC_LEN + PID_LEN + MAX_DATA_LEN + CRC16_LEN;

pub type Packet = Bytes<PACKET_CAPACITY>;

#[cfg(test)]
mod tests {
    use super::Packet;

    #[test]
    fn the_capacity_holds_the_longest_full_speed_data_packet() {
        assert_eq!(Packet::CAPACITY, 68);
    }

    #[test]
    fn a_packet_at_capacity_is_allowed() {
        let bytes = vec![0xA5u8; Packet::CAPACITY];
        let packet = Packet::from_slice(&bytes).expect("fits");
        assert_eq!(packet.as_slice(), bytes.as_slice());
    }

    #[test]
    fn one_byte_over_capacity_is_refused() {
        assert!(Packet::from_slice(&[0; 69]).is_err());
    }
}
