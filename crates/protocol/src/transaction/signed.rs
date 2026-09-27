use crate::rlp::Items;
use crate::transaction::error::Error;
use crate::transaction::result::Result;
use crate::transaction::unsigned::{
    Unsigned, decode_fields, envelope_encoder, envelope_fields, quantity_field, unsigned_field,
};

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
/// A standard eth signature
pub struct Signature {
    pub y_parity: u8,
    pub r: [u8; SIGNATURE_WORD_LEN],
    pub s: [u8; SIGNATURE_WORD_LEN],
}

impl Signature {
    /// Extract signature from a ledger signature `v`,`r`,`s`
    pub fn from_ledger(
        v: u8,
        r: [u8; SIGNATURE_WORD_LEN],
        s: [u8; SIGNATURE_WORD_LEN],
    ) -> Result<Self> {
        // compute y parity
        let y_parity = match v {
            PARITY_EVEN | PARITY_ODD => v, // already parity keep it
            LEGACY_PARITY_BASE => PARITY_EVEN, // even parity
            v if v == LEGACY_PARITY_BASE + 1 => PARITY_ODD, // odd parity
            other => return Err(Error::BadParity(other)), // reject anything else
        };

        Ok(Self { y_parity, r, s })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signed<'a> {
    pub unsigned: Unsigned<'a>,
    pub signature: Signature,
}

impl<'a> Signed<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<Self> {
        let mut fields = envelope_fields(bytes)?;
        let unsigned = decode_fields(&mut fields)?;
        let y_parity = parity_field(&mut fields)?;
        let r = word_field(&mut fields)?;
        let s = word_field(&mut fields)?;

        if fields.next().is_some() {
            return Err(Error::TooManyFields);
        }

        Ok(Self {
            unsigned,
            signature: Signature { y_parity, r, s },
        })
    }
}

fn parity_field(fields: &mut Items<'_>) -> Result<u8> {
    match u8::try_from(unsigned_field(fields)?) {
        Ok(parity @ (PARITY_EVEN | PARITY_ODD)) => Ok(parity),
        Ok(other) => Err(Error::BadParity(other)),
        Err(_) => Err(Error::IntegerTooLarge),
    }
}

fn word_field(fields: &mut Items<'_>) -> Result<[u8; SIGNATURE_WORD_LEN]> {
    let bytes = quantity_field(fields)?;
    let Some(start) = SIGNATURE_WORD_LEN.checked_sub(bytes.len()) else {
        return Err(Error::IntegerTooLarge);
    };

    let mut word = [0u8; SIGNATURE_WORD_LEN];
    if let Some(tail) = word.get_mut(start..) {
        tail.copy_from_slice(bytes);
    }
    Ok(word)
}

/// Create a signed transaction from unsigned structured and parsed sig.
pub fn encode_signed(
    unsigned: &Unsigned<'_>,
    signature: &Signature,
    out: &mut [u8],
) -> Result<usize> {
    // get envelope encoder
    let mut encoder = envelope_encoder(out)?;
    encoder.list(|fields| {
        // Encode the unsigned tx fields
        unsigned.encode_fields(fields)?;
        // Encode unsigned parity as u64
        fields.unsigned(u64::from(signature.y_parity))?;
        // Encode r and s as numbers using our rlp encoder
        fields.quantity(&signature.r)?;
        fields.quantity(&signature.s)
    })?;
    // 1 + finish represents the tx type and the bytes written
    // by encoder
    Ok(1 + encoder.finish())
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::Signature;
    use super::Signed;
    use super::encode_signed;
    use crate::rlp;
    use crate::transaction::unsigned::Unsigned;
    use crate::transfer::{Transfer, classify};

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

    const BSC_NATIVE: &str = "02f87338831a2ec5840393870084039387008261a894445a0d5b48fcad5af240a72f925609e498ec306a860d48cdb5090080c080a07a6c345139a2c0b62d1d39b1c44fe1871f5e17d5eaddfd19e06ab33e46618fb7a05f7baad2066db1ba75177726c2a5cea918caca4d305548228ae47617d313ca18";
    const BSC_TOKEN: &str = "02f8b138818184047498308404d7c6d08301151794333d05767067ff4c5167cf1a03baa6782de3333380b844a9059cbb000000000000000000000000333d05767067ff4c5167cf1a03baa6782de333330000000000000000000000000000000000000000000000000000000000000000c001a01b588c3288bc3815135c2592cf4c551b4c5ac87939df68d86551860deaf9c35ba065f82f7fbb06a4d9895ecb2d3c38a9ed1268f5620f44707ffbfff3ddcc0ef348";

    fn word(text: &str) -> [u8; 32] {
        hex(text).try_into().expect("32 bytes")
    }

    fn signed(tail: &[&[u8]]) -> Vec<u8> {
        let mut buffer = [0u8; 256];
        let mut encoder = rlp::Encoder::new(&mut buffer[1..]);
        encoder
            .list(|fields| {
                native().encode_fields(fields)?;
                tail.iter().try_for_each(|field| fields.bytes(field))
            })
            .expect("fits");
        let len = encoder.finish();
        buffer[0] = 0x02;
        buffer[..1 + len].to_vec()
    }

    #[test]
    fn what_encode_signed_writes_decodes_back_unchanged() {
        for y_parity in [0, 1] {
            let mut r = [0x11; 32];
            r[0] = 0;
            let signature = Signature {
                y_parity,
                r,
                s: [0x22; 32],
            };
            let mut buffer = [0u8; 256];
            let len = encode_signed(&native(), &signature, &mut buffer).expect("fits");
            assert_eq!(
                Signed::decode(&buffer[..len]),
                Ok(Signed {
                    unsigned: native(),
                    signature,
                })
            );
        }
    }

    #[test]
    fn a_real_bsc_native_transfer_decodes() {
        let bytes = hex(BSC_NATIVE);
        let decoded = Signed::decode(&bytes).expect("decodes");
        assert_eq!(decoded.unsigned.chain_id, 56);
        assert_eq!(decoded.unsigned.nonce, 0x1a2ec5);
        assert_eq!(decoded.unsigned.max_priority_fee, &hex("03938700")[..]);
        assert_eq!(decoded.unsigned.max_fee, &hex("03938700")[..]);
        assert_eq!(decoded.unsigned.gas_limit, 0x61a8);
        let to: [u8; 20] = hex("445a0d5b48fcad5af240a72f925609e498ec306a")
            .try_into()
            .expect("20 bytes");
        assert_eq!(decoded.unsigned.to, Some(&to));
        assert_eq!(decoded.unsigned.value, &hex("0d48cdb50900")[..]);
        assert_eq!(decoded.unsigned.data, &[] as &[u8]);
        assert_eq!(
            decoded.signature,
            Signature {
                y_parity: 0,
                r: word("7a6c345139a2c0b62d1d39b1c44fe1871f5e17d5eaddfd19e06ab33e46618fb7"),
                s: word("5f7baad2066db1ba75177726c2a5cea918caca4d305548228ae47617d313ca18"),
            }
        );
    }

    #[test]
    fn a_real_bsc_token_transfer_decodes_and_classifies() {
        let bytes = hex(BSC_TOKEN);
        let decoded = Signed::decode(&bytes).expect("decodes");
        assert_eq!(decoded.unsigned.chain_id, 56);
        assert_eq!(decoded.unsigned.nonce, 0x81);
        assert_eq!(decoded.unsigned.gas_limit, 0x11517);
        assert_eq!(decoded.signature.y_parity, 1);
        assert!(matches!(
            classify(&decoded.unsigned),
            Ok(Transfer::Token {
                chain_id: 56,
                amount,
                ..
            }) if *amount == [0; 32]
        ));
    }

    #[test]
    fn an_unsigned_transaction_is_not_a_signed_one() {
        let mut buffer = [0u8; 128];
        let len = native().encode(&mut buffer).expect("fits");
        assert_eq!(Signed::decode(&buffer[..len]), Err(Error::TooFewFields));
    }

    #[test]
    fn a_signature_missing_a_word_is_refused() {
        assert!(Signed::decode(&signed(&[&[0x01], &[0x11; 32], &[0x22; 32]])).is_ok());
        assert_eq!(
            Signed::decode(&signed(&[&[0x01], &[0x11; 32]])),
            Err(Error::TooFewFields)
        );
        assert_eq!(
            Signed::decode(&signed(&[&[0x01]])),
            Err(Error::TooFewFields)
        );
    }

    #[test]
    fn a_cut_off_transaction_is_refused() {
        let bytes = hex(BSC_NATIVE);
        assert!(Signed::decode(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn a_field_after_the_signature_is_refused() {
        let bytes = signed(&[&[0x01], &[0x11; 32], &[0x22; 32], &[0x33]]);
        assert_eq!(Signed::decode(&bytes), Err(Error::TooManyFields));
    }

    #[test]
    fn a_parity_other_than_zero_or_one_is_refused() {
        for (parity, expected) in [
            (&[0x02][..], Error::BadParity(2)),
            (&[0x1b][..], Error::BadParity(27)),
            (&[0x01, 0x00][..], Error::IntegerTooLarge),
        ] {
            let bytes = signed(&[parity, &[0x11; 32], &[0x22; 32]]);
            assert_eq!(Signed::decode(&bytes), Err(expected), "{parity:?}");
        }
    }

    #[test]
    fn a_zero_parity_written_as_a_zero_byte_is_not_canonical() {
        let bytes = signed(&[&[0x00], &[0x11; 32], &[0x22; 32]]);
        assert_eq!(Signed::decode(&bytes), Err(Error::NonCanonicalInteger));
    }

    #[test]
    fn a_signature_word_wider_than_32_bytes_or_with_a_leading_zero_is_refused() {
        let bytes = signed(&[&[0x01], &[0x11; 33], &[0x22; 32]]);
        assert_eq!(Signed::decode(&bytes), Err(Error::IntegerTooLarge));
        let mut padded = [0x22; 32];
        padded[0] = 0;
        let bytes = signed(&[&[0x01], &[0x11; 32], &padded]);
        assert_eq!(Signed::decode(&bytes), Err(Error::NonCanonicalInteger));
    }

    #[test]
    fn a_short_signature_word_is_left_padded() {
        let bytes = signed(&[&[0x01], &[0x05], &[0x22; 32]]);
        let mut r = [0u8; 32];
        r[31] = 0x05;
        assert_eq!(
            Signed::decode(&bytes).map(|decoded| decoded.signature.r),
            Ok(r)
        );
    }
}
