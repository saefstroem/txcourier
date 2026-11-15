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
    pub const fn from_byte(byte: u8) -> Result<Self> {
        match byte {
            PENDING => Ok(Self::Pending),
            SIGNED => Ok(Self::Signed),
            _ => Err(Error::MalformedPayload),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Slot;

    #[test]
    fn the_selector_bytes_are_pinned() {
        assert_eq!(Slot::from_byte(0x00), Ok(Slot::Pending));
        assert_eq!(Slot::from_byte(0x01), Ok(Slot::Signed));
    }

    #[test]
    fn every_other_byte_is_rejected() {
        for byte in 0u8..=0xFF {
            let expected = [0x00, 0x01].contains(&byte);
            assert_eq!(Slot::from_byte(byte).is_ok(), expected, "{byte:#04x}");
        }
    }
}
