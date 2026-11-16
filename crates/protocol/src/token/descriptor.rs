use crate::token::error::Error;
use crate::token::result::Result;
use crate::transaction::ADDRESS_LEN;

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

/// What a token descriptor says: its ticker, contract, decimals and chain
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary<'a> {
    pub ticker: &'a [u8],
    pub contract: [u8; ADDRESS_LEN],
    pub decimals: u32,
    pub chain_id: u64,
}

/// Read a token descriptor's fields, after checking it is shaped like one
pub fn summarize(descriptor: &[u8]) -> Result<Summary<'_>> {
    let Some(&ticker_len) = descriptor.get(TICKER_LEN_AT) else {
        return Err(Error::Malformed);
    };

    // Compute offsets for all
    let ticker_len = usize::from(ticker_len);

    let address_at = ADDRESS_AFTER_TICKER + ticker_len;
    let decimals_at = address_at + ADDRESS_LEN;
    let chain_at = decimals_at + DECIMALS_LEN;
    let signature_at = chain_at + CHAIN_ID_LEN;

    // The ledger signs all the tokens
    let (Some(&tag), Some(&declared)) = (
        descriptor.get(signature_at),
        descriptor.get(signature_at + 1),
    ) else {
        return Err(Error::Malformed);
    };

    // Ensure proper start of an ECDSA signature
    if tag != DER_SEQUENCE {
        return Err(Error::Malformed);
    }

    // Ensure the descriptor is the length its claiming based on contents
    if descriptor.len() != signature_at + DER_HEADER_LEN + usize::from(declared) {
        return Err(Error::Malformed);
    }

    let (Some(ticker), Some(contract), Some(decimals), Some(chain)) = (
        descriptor.get(TICKER_LEN_AT + 1..address_at),
        descriptor.get(address_at..decimals_at),
        descriptor.get(decimals_at..chain_at),
        descriptor.get(chain_at..signature_at),
    ) else {
        return Err(Error::Malformed);
    };

    let (Ok(contract), Ok(decimals), Ok(chain)) = (
        <[u8; ADDRESS_LEN]>::try_from(contract),
        <[u8; DECIMALS_LEN]>::try_from(decimals),
        <[u8; CHAIN_ID_LEN]>::try_from(chain),
    ) else {
        return Err(Error::Malformed);
    };

    Ok(Summary {
        ticker,
        contract,
        decimals: u32::from_be_bytes(decimals),
        chain_id: u64::from(u32::from_be_bytes(chain)),
    })
}

/// Validate whether a token descriptor describes an address at a certain chain id
pub fn describes(descriptor: &[u8], address: &[u8; ADDRESS_LEN], chain_id: u64) -> Result<()> {
    let summary = summarize(descriptor)?;

    // Ensure the address matches
    if summary.contract != *address {
        return Err(Error::AddressMismatch);
    }

    // Ensure the chain matches
    if summary.chain_id != chain_id {
        return Err(Error::ChainMismatch);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::Summary;
    use super::describes;
    use super::summarize;

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
    fn a_descriptor_must_name_the_contract_it_is_stored_against() {
        assert_eq!(describes(&usdt_descriptor(), &USDT, 1), Ok(()));
        assert_eq!(
            describes(&usdt_descriptor(), &[0x11; 20], 1),
            Err(Error::AddressMismatch)
        );
    }

    #[test]
    fn a_descriptor_for_another_chain_is_refused() {
        assert_eq!(
            describes(&usdt_descriptor(), &USDT, 8453),
            Err(Error::ChainMismatch)
        );
    }

    #[test]
    fn a_truncated_descriptor_is_refused_rather_than_read_past_its_end() {
        let bytes = usdt_descriptor();
        for len in 0..bytes.len() {
            assert_eq!(
                describes(&bytes[..len], &USDT, 1),
                Err(Error::Malformed),
                "length {len}"
            );
        }
        assert_eq!(describes(&bytes, &USDT, 1), Ok(()));
    }

    #[test]
    fn trailing_bytes_after_the_signature_are_refused() {
        let mut bytes = usdt_descriptor();
        bytes.push(0x00);
        assert_eq!(describes(&bytes, &USDT, 1), Err(Error::Malformed));
    }

    #[test]
    fn a_signature_that_is_not_a_der_sequence_is_refused() {
        let mut bytes = usdt_descriptor();
        bytes[33] = 0x31;
        assert_eq!(describes(&bytes, &USDT, 1), Err(Error::Malformed));
    }

    #[test]
    fn a_ticker_length_that_runs_off_the_end_is_refused() {
        let mut bytes = usdt_descriptor();
        bytes[0] = 0xFF;
        assert_eq!(describes(&bytes, &USDT, 1), Err(Error::Malformed));
    }

    #[test]
    fn a_descriptor_summarizes_to_its_ticker_contract_decimals_and_chain() {
        let descriptor = usdt_descriptor();
        assert_eq!(
            summarize(&descriptor),
            Ok(Summary {
                ticker: b"USDT",
                contract: USDT,
                decimals: 6,
                chain_id: 1,
            })
        );
    }

    #[test]
    fn a_misshapen_descriptor_does_not_summarize() {
        let mut too_long = usdt_descriptor();
        too_long.push(0x00);
        assert_eq!(summarize(&too_long), Err(Error::Malformed));

        let short = usdt_descriptor();
        assert_eq!(summarize(&short[..short.len() - 1]), Err(Error::Malformed));
        assert_eq!(summarize(&[]), Err(Error::Malformed));
    }

    #[test]
    fn describing_agrees_with_the_summary() {
        let descriptor = usdt_descriptor();
        let summary = summarize(&descriptor).expect("well formed");
        assert_eq!(
            describes(&descriptor, &summary.contract, summary.chain_id),
            Ok(())
        );
    }
}
