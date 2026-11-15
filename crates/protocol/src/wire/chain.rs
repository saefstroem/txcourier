use crate::bitcoin::put;

pub const IDENTIFIER_LEN: usize = 1;

/// Chain identifiers for different blockchain networks
const ETHEREUM: u8 = 0x37;
const BITCOIN: u8 = 0x87;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chain {
    Ethereum,
    Bitcoin,
}

impl Chain {
    pub const fn identifier(self) -> u8 {
        match self {
            Self::Ethereum => ETHEREUM,
            Self::Bitcoin => BITCOIN,
        }
    }

    /// Tag a payload with the chain identifier at the beginning
    pub fn tag(self, payload: &[u8], out: &mut [u8]) -> Option<usize> {
        put(out, &[&[self.identifier()], payload])
    }

    /// Split a stored payload into its chain identifier and the remaining payload
    pub fn split(stored: &[u8]) -> Option<(Self, &[u8])> {
        match stored.split_first()? {
            (&ETHEREUM, payload) => Some((Self::Ethereum, payload)),
            (&BITCOIN, payload) => Some((Self::Bitcoin, payload)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Chain;

    #[test]
    fn the_identifier_leads_the_stored_payload_and_its_bytes_are_pinned() {
        let mut out = [0u8; 3];
        for (chain, identifier) in [(Chain::Ethereum, 0x37), (Chain::Bitcoin, 0x87)] {
            assert_eq!(chain.tag(&[0xAA, 0xBB], &mut out), Some(3));
            assert_eq!(out, [identifier, 0xAA, 0xBB]);
            assert_eq!(Chain::split(&out), Some((chain, &[0xAA, 0xBB][..])));
        }
        assert_eq!(Chain::Bitcoin.tag(&[0xAA; 3], &mut out), None);
    }

    #[test]
    fn every_other_first_byte_is_refused() {
        for byte in 0u8..=0xFF {
            let known = [0x37, 0x87].contains(&byte);
            assert_eq!(Chain::split(&[byte, 0xAA]).is_some(), known, "{byte:#04x}");
        }
        assert_eq!(Chain::split(&[]), None);
    }
}
