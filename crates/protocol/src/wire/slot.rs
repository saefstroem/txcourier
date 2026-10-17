use crate::wire::error::Error;
use crate::wire::result::Result;

// The pending slot selector. One bit from SIGNED
const PENDING: u8 = 0x00;
// The signed slot selector.
const SIGNED: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Pending,
    Signed,
}

impl Slot {
    // Every variant, so the round-trip test fails if a third slot is added.
    pub const ALL: [Self; 2] = [Self::Pending, Self::Signed];

    pub const fn to_byte(self) -> u8 {
        match self {
            Self::Pending => PENDING,
            Self::Signed => SIGNED,
        }
    }

    pub const fn from_byte(byte: u8) -> Result<Self> {
        match byte {
            PENDING => Ok(Self::Pending),
            SIGNED => Ok(Self::Signed),
            _ => Err(Error::MalformedPayload),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Signed => "signed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Slot;

    #[test]
    fn byte_encoding_round_trips() {
        for slot in Slot::ALL {
            assert_eq!(Slot::from_byte(slot.to_byte()), Ok(slot));
        }
    }

    #[test]
    fn the_selector_bytes_are_pinned() {
        assert_eq!(Slot::Pending.to_byte(), 0x00);
        assert_eq!(Slot::Signed.to_byte(), 0x01);
    }

    #[test]
    fn every_other_byte_is_rejected() {
        for byte in 0u8..=0xFF {
            let expected = [0x00, 0x01].contains(&byte);
            assert_eq!(Slot::from_byte(byte).is_ok(), expected, "{byte:#04x}");
        }
    }

    #[test]
    fn the_two_slots_are_named_for_the_command_line() {
        assert_eq!(Slot::Pending.name(), "pending");
        assert_eq!(Slot::Signed.name(), "signed");
    }
}
