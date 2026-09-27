use crate::token::descriptor::{Descriptor, describes};
use crate::token::error::Error;
use crate::token::result::Result;
use crate::transaction::ADDRESS_LEN;

// The most descriptors whose record fits one provisioning frame. Sixteen would put BLOB_MAX
// at 2074 against a capacity of 2036.
pub const MAX_TOKENS: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tokens {
    descriptors: [Descriptor; MAX_TOKENS],
    count: usize,
}

impl Tokens {
    // The state with nothing provisioned, and what decode starts from.
    pub const EMPTY: Self = Self {
        descriptors: [Descriptor::NONE; MAX_TOKENS],
        count: 0,
    };

    pub fn push(&mut self, descriptor: &[u8]) -> Result<()> {
        let descriptor = Descriptor::new(descriptor)?;

        let Some(slot) = self.descriptors.get_mut(self.count) else {
            return Err(Error::TooManyTokens);
        };
        *slot = descriptor;
        self.count += 1;

        Ok(())
    }

    pub fn descriptor(&self, chain_id: u64, contract: &[u8; ADDRESS_LEN]) -> Option<&[u8]> {
        self.iter()
            .find(|descriptor| describes(descriptor, contract, chain_id).is_ok())
    }

    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        self.descriptors
            .iter()
            .take(self.count)
            .map(Descriptor::as_slice)
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::MAX_TOKENS;
    use super::Tokens;

    const USDT: [u8; 20] = [
        0xda, 0xc1, 0x7f, 0x95, 0x8d, 0x2e, 0xe5, 0x23, 0xa2, 0x20, 0x62, 0x06, 0x99, 0x45, 0x97,
        0xc1, 0x3d, 0x83, 0x1e, 0xc7,
    ];
    const USDC: [u8; 20] = [
        0xa0, 0xb8, 0x69, 0x91, 0xc6, 0x21, 0x8b, 0x36, 0xc1, 0xd1, 0x9d, 0x4a, 0x2e, 0x9e, 0xb0,
        0xce, 0x36, 0x06, 0xeb, 0x48,
    ];

    fn descriptor(ticker: &[u8], address: &[u8; 20], chain_id: u32) -> Vec<u8> {
        let mut bytes = vec![ticker.len() as u8];
        bytes.extend_from_slice(ticker);
        bytes.extend_from_slice(address);
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&chain_id.to_be_bytes());
        bytes.extend_from_slice(&[0x30, 8]);
        bytes.extend_from_slice(&[0xAB; 8]);
        bytes
    }

    #[test]
    fn a_descriptor_is_found_by_the_contract_and_chain_it_names() {
        let mut tokens = Tokens::EMPTY;
        tokens.push(&descriptor(b"USDT", &USDT, 1)).expect("room");
        tokens.push(&descriptor(b"USDC", &USDC, 1)).expect("room");

        assert_eq!(
            tokens.descriptor(1, &USDT),
            Some(descriptor(b"USDT", &USDT, 1).as_slice())
        );
        assert_eq!(
            tokens.descriptor(1, &USDC),
            Some(descriptor(b"USDC", &USDC, 1).as_slice())
        );
        assert_eq!(tokens.len(), 2);
        assert!(!tokens.is_empty());
    }

    #[test]
    fn a_descriptor_from_another_chain_is_not_returned() {
        let mut tokens = Tokens::EMPTY;
        tokens.push(&descriptor(b"USDT", &USDT, 1)).expect("room");

        assert_eq!(tokens.descriptor(8453, &USDT), None);
        assert_eq!(tokens.descriptor(1, &USDC), None);
    }

    #[test]
    fn the_same_contract_on_two_chains_keeps_its_own_descriptor() {
        let mut tokens = Tokens::EMPTY;
        tokens.push(&descriptor(b"USDT", &USDT, 1)).expect("room");
        tokens.push(&descriptor(b"USDT", &USDT, 10)).expect("room");

        assert_eq!(
            tokens.descriptor(10, &USDT),
            Some(descriptor(b"USDT", &USDT, 10).as_slice())
        );
    }

    #[test]
    fn a_malformed_descriptor_is_stored_but_never_matches() {
        let mut tokens = Tokens::EMPTY;
        tokens.push(&[0xFF; 32]).expect("room");

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens.descriptor(1, &USDT), None);
    }

    #[test]
    fn an_empty_descriptor_is_never_stored() {
        let mut tokens = Tokens::EMPTY;
        assert_eq!(tokens.push(&[]), Err(Error::DescriptorMissing));
        assert!(tokens.is_empty());
    }

    #[test]
    fn the_table_refuses_one_token_past_its_capacity() {
        let mut tokens = Tokens::EMPTY;
        for fill in 0..MAX_TOKENS {
            tokens
                .push(&descriptor(b"TKN", &[fill as u8; 20], 1))
                .expect("room");
        }
        assert_eq!(
            tokens.push(&descriptor(b"TKN", &[0xFF; 20], 1)),
            Err(Error::TooManyTokens)
        );
        assert_eq!(tokens.len(), MAX_TOKENS);
    }

    #[test]
    fn an_empty_table_finds_nothing() {
        assert!(Tokens::EMPTY.is_empty());
        assert_eq!(Tokens::EMPTY.len(), 0);
        assert_eq!(Tokens::EMPTY.descriptor(1, &USDT), None);
        assert_eq!(Tokens::EMPTY.iter().count(), 0);
    }
}
