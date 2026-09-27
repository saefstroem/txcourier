use crate::wire::error::Error;
use crate::wire::result::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bytes<const CAPACITY: usize> {
    bytes: [u8; CAPACITY],
    len: u16,
}

impl<const CAPACITY: usize> Bytes<CAPACITY> {
    // Re-exports the generic parameter so callers can size buffers without naming it.
    pub const CAPACITY: usize = CAPACITY;

    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let Ok(len) = u16::try_from(bytes.len()) else {
            return Err(Error::PayloadTooLarge(bytes.len()));
        };

        let mut bounded = Self {
            bytes: [0; CAPACITY],
            len,
        };

        let Some(destination) = bounded.bytes.get_mut(..bytes.len()) else {
            return Err(Error::PayloadTooLarge(bytes.len()));
        };
        destination.copy_from_slice(bytes);

        Ok(bounded)
    }

    pub fn as_slice(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::Bytes;
    use super::Error;

    type Small = Bytes<4>;

    #[test]
    fn bytes_round_trip() {
        let bounded = Small::from_slice(&[1, 2, 3]).expect("fits");
        assert_eq!(bounded.as_slice(), &[1, 2, 3]);
    }

    #[test]
    fn empty_and_full_are_both_allowed() {
        assert!(Small::from_slice(&[]).expect("fits").as_slice().is_empty());
        assert_eq!(
            Small::from_slice(&[9; 4]).expect("fits").as_slice(),
            &[9; 4]
        );
    }

    #[test]
    fn one_byte_over_capacity_is_refused() {
        assert_eq!(Small::from_slice(&[0; 5]), Err(Error::PayloadTooLarge(5)));
    }

    #[test]
    fn values_compare_by_content_not_by_padding() {
        let a = Small::from_slice(&[1, 2]).expect("fits");
        let b = Small::from_slice(&[1, 2]).expect("fits");
        let c = Small::from_slice(&[1]).expect("fits");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
