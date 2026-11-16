#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeviceState {
    Idle = 0xA1,
    PendingStored = 0xB2,
    SignedStored = 0xC3,
}

impl DeviceState {
    pub const fn to_byte(self) -> u8 {
        self as u8
    }
}

// The value of zeroed RAM, and not a DeviceState, so a lamp nothing has published is dark.
pub const FAULT_BYTE: u8 = 0x00;

#[cfg(test)]
mod tests {
    use super::DeviceState;
    use super::FAULT_BYTE;

    const ALL: [DeviceState; 3] = [
        DeviceState::Idle,
        DeviceState::PendingStored,
        DeviceState::SignedStored,
    ];

    #[test]
    fn the_state_bytes_are_pinned_and_the_fault_byte_is_none_of_them() {
        assert_eq!(ALL.map(DeviceState::to_byte), [0xA1, 0xB2, 0xC3]);
        assert!(ALL.iter().all(|state| state.to_byte() != FAULT_BYTE));
    }

    #[test]
    fn states_are_at_least_three_bit_flips_apart() {
        let encodings: [u8; 5] = [0xA1, 0xB2, 0xC3, 0x00, 0xFF];
        for (i, a) in encodings.iter().enumerate() {
            for b in &encodings[i + 1..] {
                let distance = (a ^ b).count_ones();
                assert!(
                    distance >= 3,
                    "{a:#04x} and {b:#04x} differ by {distance} bits"
                );
            }
        }
    }
}
