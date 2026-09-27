// for the usb coes the high nibble is the low nibble with every bit flipped
// USB 2.0 Table 8-1: OUT is 0001B. 
pub const OUT: u8 = 0xE1;
// USB 2.0 Table 8-1: IN is 1001B. 
pub const IN: u8 = 0x69;
// USB 2.0 Table 8-1: SOF is 0101B. 
pub const SOF: u8 = 0xA5;
// USB 2.0 Table 8-1: SETUP is 1101B. 
pub const SETUP: u8 = 0x2D;
// USB 2.0 Table 8-1: DATA0 is 0011B. 
pub const DATA0: u8 = 0xC3;
// USB 2.0 Table 8-1: DATA1 is 1011B. 
pub const DATA1: u8 = 0x4B;
// USB 2.0 Table 8-1: ACK is 0010B. 
pub const ACK: u8 = 0xD2;
// USB 2.0 Table 8-1: NAK is 1010B. 
pub const NAK: u8 = 0x5A;
// USB 2.0 Table 8-1: STALL is 1110B. 
pub const STALL: u8 = 0x1E;

// mask for masking lower nibbles
const NIBBLE_MASK: u8 = 0x0F;
// USB 2.0 section 8.3.1: a PID is a four-bit type field followed by its four-bit complement.
const NIBBLE_BITS: u32 = 4;

// mask the nibble and ensure its the same as inversion
// moved right 4 bits
pub const fn is_well_formed(pid: u8) -> bool {
    pid & NIBBLE_MASK == !pid >> NIBBLE_BITS
}

#[cfg(test)]
mod tests {
    use super::ACK;
    use super::DATA0;
    use super::DATA1;
    use super::IN;
    use super::NAK;
    use super::OUT;
    use super::SETUP;
    use super::SOF;
    use super::STALL;
    use super::is_well_formed;

    const EVERY_PID: [u8; 9] = [OUT, IN, SOF, SETUP, DATA0, DATA1, ACK, NAK, STALL];

    #[test]
    fn every_identifier_carries_its_own_complement() {
        for pid in EVERY_PID {
            assert!(is_well_formed(pid), "{pid:#04x}");
        }
    }

    #[test]
    fn identifiers_are_distinct() {
        for (i, a) in EVERY_PID.iter().enumerate() {
            for b in EVERY_PID.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn a_corrupted_identifier_is_rejected() {
        for pid in EVERY_PID {
            for bit in 0..8 {
                assert!(!is_well_formed(pid ^ 1 << bit), "{pid:#04x} bit {bit}");
            }
        }
    }
}
