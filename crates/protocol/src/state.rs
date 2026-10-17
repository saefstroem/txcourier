use crate::error::Error;
use crate::result::Result;

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

    pub const fn from_byte(byte: u8) -> Result<Self> {
        match byte {
            0xA1 => Ok(Self::Idle),
            0xB2 => Ok(Self::PendingStored),
            0xC3 => Ok(Self::SignedStored),
            _ => Err(Error::UnknownStateByte(byte)),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::PendingStored => "pending transaction stored",
            Self::SignedStored => "signed transaction stored",
        }
    }
}

// The value of zeroed RAM, and not a DeviceState, so a lamp nothing has published is dark.
pub const FAULT_BYTE: u8 = 0x00;

#[cfg(test)]
mod tests {
    use super::DeviceState;
    use super::FAULT_BYTE;
    use crate::error::Error;

    const ALL: [DeviceState; 3] = [
        DeviceState::Idle,
        DeviceState::PendingStored,
        DeviceState::SignedStored,
    ];

    const VALID_BYTES: [u8; 3] = [0xA1, 0xB2, 0xC3];

    #[test]
    fn byte_encoding_round_trips() {
        for state in ALL {
            assert_eq!(DeviceState::from_byte(state.to_byte()), Ok(state));
        }
    }

    #[test]
    fn every_other_byte_is_rejected() {
        for byte in 0u8..=0xFF {
            if !VALID_BYTES.contains(&byte) {
                assert_eq!(
                    DeviceState::from_byte(byte),
                    Err(Error::UnknownStateByte(byte))
                );
            }
        }
    }

    #[test]
    fn fault_byte_is_not_a_state() {
        assert!(DeviceState::from_byte(FAULT_BYTE).is_err());
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

    #[test]
    fn names_are_distinct() {
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(a.name(), b.name());
            }
        }
    }
}
