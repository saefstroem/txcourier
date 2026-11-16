use crate::ledger::error::Error;
use crate::ledger::result::Result;

// BIP-32: a hardened index is i + 2^31. SLIP-44 writes it as 0x80000000 + n.
const HARDENED: u32 = 0x8000_0000;
// BIP-44: purpose is the constant 44', hardened.
const PURPOSE: u32 = 44;
// SLIP-44 registered coin types: 60 is ETH. 61 is ETC.
const COIN_ETHEREUM: u32 = 60;

// m/44'/60'/0'/0/0: BIP-44 first account, external chain, first address, and Ledger's
// documented example path. There is no account selector.
pub const DEFAULT_ACCOUNT: [u32; 5] =
    [HARDENED | PURPOSE, HARDENED | COIN_ETHEREUM, HARDENED, 0, 0];

// app-ethereum ethapp.adoc: a path is one byte of component count, at most 10, then four
// big-endian bytes per component. 1 + 5 * 4.
pub const PATH_LEN: usize = 1 + DEFAULT_ACCOUNT.len() * 4;

/// Encodes derivation path into proper serialized bytes
/// for the ethereum app.
pub fn encode(path: &[u32], out: &mut [u8]) -> Result<usize> {
    let needed = 1 + path.len() * 4;

    // ensure it is sufficient size
    let Some(slot) = out.get_mut(..needed) else {
        return Err(Error::OutputTooSmall {
            needed,
            found: out.len(),
        });
    };

    // split the size and payload
    let Some((count, body)) = slot.split_first_mut() else {
        return Err(Error::OutputTooSmall {
            needed,
            found: out.len(),
        });
    };
    *count = path.len() as u8;

    let (places, _) = body.as_chunks_mut::<4>();
    for (component, place) in path.iter().zip(places) {
        // serialize the path into the "place"
        *place = component.to_be_bytes();
    }

    Ok(needed)
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_ACCOUNT;
    use super::Error;
    use super::PATH_LEN;
    use super::encode;

    #[test]
    fn the_default_account_is_the_standard_first_ethereum_address() {
        assert_eq!(
            DEFAULT_ACCOUNT,
            [0x8000_002C, 0x8000_003C, 0x8000_0000, 0, 0]
        );
    }

    #[test]
    fn the_default_account_encodes_to_the_reference_bytes() {
        let mut buffer = [0u8; PATH_LEN];
        let len = encode(&DEFAULT_ACCOUNT, &mut buffer).expect("fits");
        assert_eq!(len, PATH_LEN);
        assert_eq!(
            buffer,
            [
                0x05, 0x80, 0x00, 0x00, 0x2C, 0x80, 0x00, 0x00, 0x3C, 0x80, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }

    #[test]
    fn the_length_prefix_counts_components_not_bytes() {
        let mut buffer = [0u8; 9];
        let len = encode(&[1, 2], &mut buffer).expect("fits");
        assert_eq!(len, 9);
        assert_eq!(buffer[0], 2);
    }

    #[test]
    fn components_are_big_endian() {
        let mut buffer = [0u8; 5];
        encode(&[0x0102_0304], &mut buffer).expect("fits");
        assert_eq!(&buffer[1..], &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn a_short_buffer_is_refused() {
        let mut buffer = [0u8; PATH_LEN - 1];
        assert!(matches!(
            encode(&DEFAULT_ACCOUNT, &mut buffer),
            Err(Error::OutputTooSmall { .. })
        ));
    }
}
