use crate::transaction::error::Error;
use crate::transaction::result::Result;
use crate::transaction::unsigned::{Unsigned, envelope_encoder};

// r and s are scalars below secp256k1's 256-bit order. app-ethereum returns 32 bytes each.
pub const SIGNATURE_WORD_LEN: usize = 32;

// The most appending y_parity, r and s can add: 1 + 33 + 33, plus one if the list header
// widens.
pub const SIGNED_ENVELOPE_GROWTH: usize = 68;

// Yellow Paper: a type-2 transaction carries yParity, 0 or 1.
const PARITY_EVEN: u8 = 0;
// app-ethereum writes 1 into the type-2 v byte when the recovered y is odd.
const PARITY_ODD: u8 = 1;
// Yellow Paper T_w: legacy v is 27 plus yParity. app-ethereum sends a bare parity for type
// 2, so this branch never fires on the courier's own path.
const LEGACY_PARITY_BASE: u8 = 27;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    pub y_parity: u8,
    pub r: [u8; SIGNATURE_WORD_LEN],
    pub s: [u8; SIGNATURE_WORD_LEN],
}

impl Signature {
    pub fn from_ledger(
        v: u8,
        r: [u8; SIGNATURE_WORD_LEN],
        s: [u8; SIGNATURE_WORD_LEN],
    ) -> Result<Self> {
        let y_parity = match v {
            PARITY_EVEN | PARITY_ODD => v,
            LEGACY_PARITY_BASE => PARITY_EVEN,
            v if v == LEGACY_PARITY_BASE + 1 => PARITY_ODD,
            other => return Err(Error::BadParity(other)),
        };

        Ok(Self { y_parity, r, s })
    }
}

pub fn encode_signed(
    unsigned: &Unsigned<'_>,
    signature: &Signature,
    out: &mut [u8],
) -> Result<usize> {
    let mut encoder = envelope_encoder(out)?;
    encoder.list(|fields| {
        unsigned.encode_fields(fields)?;
        fields.unsigned(u64::from(signature.y_parity))?;
        fields.quantity(&signature.r)?;
        fields.quantity(&signature.s)
    })?;
    Ok(1 + encoder.finish())
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::Signature;
    use super::encode_signed;
    use crate::transaction::unsigned::Unsigned;

    const RECIPIENT: [u8; 20] = [
        0xd8, 0xda, 0x6b, 0xf2, 0x69, 0x64, 0xaf, 0x9d, 0x7e, 0xed, 0x9e, 0x03, 0xe5, 0x34, 0x15,
        0xd3, 0x7a, 0xa9, 0x60, 0x45,
    ];
    const ONE_ETHER: [u8; 8] = [0x0d, 0xe0, 0xb6, 0xb3, 0xa7, 0x64, 0x00, 0x00];
    const ONE_GWEI: [u8; 4] = [0x3b, 0x9a, 0xca, 0x00];
    const TWENTY_GWEI: [u8; 5] = [0x04, 0xa8, 0x17, 0xc8, 0x00];

    fn native() -> Unsigned<'static> {
        Unsigned {
            chain_id: 1,
            nonce: 0,
            max_priority_fee: &ONE_GWEI,
            max_fee: &TWENTY_GWEI,
            gas_limit: 21_000,
            to: Some(&RECIPIENT),
            value: &ONE_ETHER,
            data: &[],
        }
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
            .collect()
    }

    #[test]
    fn a_signed_native_transfer_matches_the_reference_bytes() {
        let signature = Signature {
            y_parity: 1,
            r: [0x11; 32],
            s: [0x22; 32],
        };
        let mut buffer = [0u8; 256];
        let len = encode_signed(&native(), &signature, &mut buffer).expect("fits");
        assert_eq!(
            buffer[..len],
            hex(
                "02f8730180843b9aca008504a817c80082520894d8da6bf26964af9d7eed9e03e53415d37aa96045880de0b6b3a764000080c001a01111111111111111111111111111111111111111111111111111111111111111a02222222222222222222222222222222222222222222222222222222222222222"
            )[..]
        );
    }

    #[test]
    fn a_zero_parity_encodes_as_the_empty_quantity() {
        let signature = Signature {
            y_parity: 0,
            r: [0x11; 32],
            s: [0x22; 32],
        };
        let mut buffer = [0u8; 256];
        let len = encode_signed(&native(), &signature, &mut buffer).expect("fits");
        let parity_index = len - 2 * 33 - 1;
        assert_eq!(buffer[parity_index], 0x80);
    }

    #[test]
    fn signature_words_with_leading_zeros_shrink_as_quantities() {
        let mut r = [0u8; 32];
        r[31] = 0x05;
        let signature = Signature {
            y_parity: 0,
            r,
            s: [0x22; 32],
        };
        let mut buffer = [0u8; 256];
        let len = encode_signed(&native(), &signature, &mut buffer).expect("fits");
        assert_eq!(buffer[len - 34], 0x05);
    }

    #[test]
    fn the_ledger_recovery_byte_maps_to_a_parity() {
        for (v, parity) in [(0u8, 0u8), (1, 1), (27, 0), (28, 1)] {
            let signature = Signature::from_ledger(v, [0; 32], [0; 32]).expect("valid");
            assert_eq!(signature.y_parity, parity);
        }
        assert_eq!(
            Signature::from_ledger(2, [0; 32], [0; 32]),
            Err(Error::BadParity(2))
        );
        assert_eq!(
            Signature::from_ledger(37, [0; 32], [0; 32]),
            Err(Error::BadParity(37))
        );
    }

    #[test]
    fn a_short_buffer_is_refused() {
        let signature = Signature {
            y_parity: 1,
            r: [0x11; 32],
            s: [0x22; 32],
        };
        let mut buffer = [0u8; 64];
        assert!(encode_signed(&native(), &signature, &mut buffer).is_err());
    }
}
