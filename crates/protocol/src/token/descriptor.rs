use crate::token::error::Error;
use crate::token::result::Result;
use crate::transaction::ADDRESS_LEN;

// Defined by this crate: a round ceiling. Ledger's signed descriptors are about 104 bytes,
// and the one-byte length in the table record caps any descriptor at 255.
pub const DESCRIPTOR_MAX: usize = 128;

// Ledger's token information payload opens with a one-byte ticker length.
const TICKER_LEN_AT: usize = 0;
// The contract address follows the ticker, which follows the length byte.
const ADDRESS_AFTER_TICKER: usize = 1;
// Ledger encodes the decimals as a big-endian u32.
const DECIMALS_LEN: usize = 4;
// Ledger encodes the chain id as a big-endian u32, so a chain id above 2^32 can never match
// a descriptor.
const CHAIN_ID_LEN: usize = 4;
// The DER identifier octet for a constructed SEQUENCE. An ECDSA signature is a SEQUENCE of
// two INTEGERs, r and s.
const DER_SEQUENCE: u8 = 0x30;
// Tag plus one length octet. DER's short form covers lengths 0 to 127, and a secp256k1
// signature is 70 to 72 bytes.
const DER_HEADER_LEN: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    bytes: [u8; DESCRIPTOR_MAX],
    len: u8,
}

impl Descriptor {
    // An unused table slot. Zero length, so describes() never matches it.
    pub const NONE: Self = Self {
        bytes: [0; DESCRIPTOR_MAX],
        len: 0,
    };

    pub fn new(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            return Err(Error::DescriptorMissing);
        }

        if bytes.len() > DESCRIPTOR_MAX {
            return Err(Error::DescriptorTooLong(bytes.len()));
        }

        let mut descriptor = Self::NONE;
        let Some(slot) = descriptor.bytes.get_mut(..bytes.len()) else {
            return Err(Error::DescriptorTooLong(bytes.len()));
        };
        slot.copy_from_slice(bytes);
        descriptor.len = bytes.len() as u8;

        Ok(descriptor)
    }

    pub fn as_slice(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).unwrap_or(&[])
    }
}

pub fn describes(descriptor: &[u8], address: &[u8; ADDRESS_LEN], chain_id: u64) -> Result<()> {
    let Some(&ticker_len) = descriptor.get(TICKER_LEN_AT) else {
        return Err(Error::DescriptorMalformed);
    };
    let ticker_len = usize::from(ticker_len);

    let address_at = ADDRESS_AFTER_TICKER + ticker_len;
    let decimals_at = address_at + ADDRESS_LEN;
    let chain_at = decimals_at + DECIMALS_LEN;
    let signature_at = chain_at + CHAIN_ID_LEN;

    let (Some(&tag), Some(&declared)) = (
        descriptor.get(signature_at),
        descriptor.get(signature_at + 1),
    ) else {
        return Err(Error::DescriptorMalformed);
    };

    if tag != DER_SEQUENCE {
        return Err(Error::DescriptorMalformed);
    }

    if descriptor.len() != signature_at + DER_HEADER_LEN + usize::from(declared) {
        return Err(Error::DescriptorMalformed);
    }

    let Some(found) = descriptor.get(address_at..decimals_at) else {
        return Err(Error::DescriptorMalformed);
    };
    if found != address {
        return Err(Error::DescriptorAddressMismatch);
    }

    let Some(chain) = descriptor.get(chain_at..signature_at) else {
        return Err(Error::DescriptorMalformed);
    };
    let Ok(chain) = <[u8; CHAIN_ID_LEN]>::try_from(chain) else {
        return Err(Error::DescriptorMalformed);
    };
    if u64::from(u32::from_be_bytes(chain)) != chain_id {
        return Err(Error::DescriptorChainMismatch);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::DESCRIPTOR_MAX;
    use super::Descriptor;
    use super::Error;
    use super::describes;

    const USDT: [u8; 20] = [
        0xda, 0xc1, 0x7f, 0x95, 0x8d, 0x2e, 0xe5, 0x23, 0xa2, 0x20, 0x62, 0x06, 0x99, 0x45, 0x97,
        0xc1, 0x3d, 0x83, 0x1e, 0xc7,
    ];

    fn usdt_descriptor() -> Vec<u8> {
        let mut bytes = vec![0x04];
        bytes.extend_from_slice(b"USDT");
        bytes.extend_from_slice(&USDT);
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&1u32.to_be_bytes());
        let mut signature = vec![0x30, 68];
        signature.extend_from_slice(&[0xAB; 68]);
        bytes.extend_from_slice(&signature);
        bytes
    }

    #[test]
    fn a_descriptor_round_trips() {
        let bytes = usdt_descriptor();
        let descriptor = Descriptor::new(&bytes).expect("fits");
        assert_eq!(descriptor.as_slice(), bytes.as_slice());
    }

    #[test]
    fn an_empty_descriptor_is_refused() {
        assert_eq!(Descriptor::new(&[]), Err(Error::DescriptorMissing));
        assert!(Descriptor::NONE.as_slice().is_empty());
    }

    #[test]
    fn one_byte_over_the_maximum_is_refused() {
        assert_eq!(
            Descriptor::new(&[0u8; DESCRIPTOR_MAX + 1]),
            Err(Error::DescriptorTooLong(DESCRIPTOR_MAX + 1))
        );
        assert!(Descriptor::new(&[0u8; DESCRIPTOR_MAX]).is_ok());
    }

    #[test]
    fn a_descriptor_must_name_the_contract_it_is_stored_against() {
        assert_eq!(describes(&usdt_descriptor(), &USDT, 1), Ok(()));
        assert_eq!(
            describes(&usdt_descriptor(), &[0x11; 20], 1),
            Err(Error::DescriptorAddressMismatch)
        );
    }

    #[test]
    fn a_descriptor_for_another_chain_is_refused() {
        assert_eq!(
            describes(&usdt_descriptor(), &USDT, 8453),
            Err(Error::DescriptorChainMismatch)
        );
    }

    #[test]
    fn a_truncated_descriptor_is_refused_rather_than_read_past_its_end() {
        let bytes = usdt_descriptor();
        for len in 0..bytes.len() {
            assert_eq!(
                describes(&bytes[..len], &USDT, 1),
                Err(Error::DescriptorMalformed),
                "length {len}"
            );
        }
        assert_eq!(describes(&bytes, &USDT, 1), Ok(()));
    }

    #[test]
    fn trailing_bytes_after_the_signature_are_refused() {
        let mut bytes = usdt_descriptor();
        bytes.push(0x00);
        assert_eq!(describes(&bytes, &USDT, 1), Err(Error::DescriptorMalformed));
    }

    #[test]
    fn a_signature_that_is_not_a_der_sequence_is_refused() {
        let mut bytes = usdt_descriptor();
        bytes[33] = 0x31;
        assert_eq!(describes(&bytes, &USDT, 1), Err(Error::DescriptorMalformed));
    }

    #[test]
    fn a_ticker_length_that_runs_off_the_end_is_refused() {
        let mut bytes = usdt_descriptor();
        bytes[0] = 0xFF;
        assert_eq!(describes(&bytes, &USDT, 1), Err(Error::DescriptorMalformed));
    }
}
