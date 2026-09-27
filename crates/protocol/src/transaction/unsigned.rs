use crate::rlp::{Encoder, Item, Items, decode_exact};
use crate::transaction::error::Error;
use crate::transaction::result::Result;

// EIP-1559: TransactionType 2 in the EIP-2718 envelope.
pub const TYPE_EIP1559: u8 = 0x02;
// Yellow Paper: an account address is 160 bits.
pub const ADDRESS_LEN: usize = 20;
// Yellow Paper: value and both fee fields are 256-bit scalars.
pub const QUANTITY_MAX_LEN: usize = 32;

// Defined by this crate: Unsigned holds chain id, nonce and gas limit as u64, so anything
// wider is refused. The Yellow Paper allows a nonce or gas limit up to 2^256.
const U64_MAX_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsigned<'a> {
    pub chain_id: u64,
    pub nonce: u64,
    pub max_priority_fee: &'a [u8],
    pub max_fee: &'a [u8],
    pub gas_limit: u64,
    pub to: Option<&'a [u8; ADDRESS_LEN]>,
    pub value: &'a [u8],
    pub data: &'a [u8],
}

impl<'a> Unsigned<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<Self> {
        let Some((kind, envelope)) = bytes.split_first() else {
            return Err(Error::Empty);
        };
        if *kind != TYPE_EIP1559 {
            return Err(Error::UnsupportedType(*kind));
        }

        let mut fields = decode_exact(envelope)?.list()?;

        let chain_id = unsigned_field(&mut fields)?;
        let nonce = unsigned_field(&mut fields)?;
        let max_priority_fee = quantity_field(&mut fields)?;
        let max_fee = quantity_field(&mut fields)?;
        let gas_limit = unsigned_field(&mut fields)?;
        let to = address_field(&mut fields)?;
        let value = quantity_field(&mut fields)?;
        let data = next_field(&mut fields)?.bytes()?;

        let mut access_list = next_field(&mut fields)?.list()?;
        if access_list.next().is_some() {
            return Err(Error::AccessListNotEmpty);
        }

        if fields.next().is_some() {
            return Err(Error::TooManyFields);
        }

        Ok(Self {
            chain_id,
            nonce,
            max_priority_fee,
            max_fee,
            gas_limit,
            to,
            value,
            data,
        })
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<usize> {
        let mut encoder = envelope_encoder(out)?;
        encoder.list(|fields| self.encode_fields(fields))?;
        Ok(1 + encoder.finish())
    }

    pub fn encode_fields(&self, fields: &mut Encoder<'_>) -> crate::rlp::Result<()> {
        fields.unsigned(self.chain_id)?;
        fields.unsigned(self.nonce)?;
        fields.quantity(self.max_priority_fee)?;
        fields.quantity(self.max_fee)?;
        fields.unsigned(self.gas_limit)?;
        fields.bytes(self.to.map(|address| &address[..]).unwrap_or(&[]))?;
        fields.quantity(self.value)?;
        fields.bytes(self.data)?;
        fields.list(|_| Ok(()))
    }
}

pub fn envelope_encoder(out: &mut [u8]) -> Result<Encoder<'_>> {
    let Some((kind, envelope)) = out.split_first_mut() else {
        return Err(Error::Rlp(crate::rlp::Error::OutputTooSmall {
            needed: 1,
            found: 0,
        }));
    };
    *kind = TYPE_EIP1559;
    Ok(Encoder::new(envelope))
}

fn next_field<'a>(fields: &mut Items<'a>) -> Result<Item<'a>> {
    match fields.next() {
        Some(item) => Ok(item?),
        None => Err(Error::TooFewFields),
    }
}

fn unsigned_field(fields: &mut Items<'_>) -> Result<u64> {
    let bytes = canonical_integer(next_field(fields)?.bytes()?)?;
    if bytes.len() > U64_MAX_LEN {
        return Err(Error::IntegerTooLarge);
    }

    let mut value = 0u64;
    for byte in bytes {
        value = value << 8 | u64::from(*byte);
    }
    Ok(value)
}

fn quantity_field<'a>(fields: &mut Items<'a>) -> Result<&'a [u8]> {
    let bytes = canonical_integer(next_field(fields)?.bytes()?)?;
    if bytes.len() > QUANTITY_MAX_LEN {
        return Err(Error::IntegerTooLarge);
    }
    Ok(bytes)
}

fn address_field<'a>(fields: &mut Items<'a>) -> Result<Option<&'a [u8; ADDRESS_LEN]>> {
    let bytes = next_field(fields)?.bytes()?;
    if bytes.is_empty() {
        return Ok(None);
    }
    match <&[u8; ADDRESS_LEN]>::try_from(bytes) {
        Ok(address) => Ok(Some(address)),
        Err(_) => Err(Error::BadAddressLength(bytes.len())),
    }
}

fn canonical_integer(bytes: &[u8]) -> Result<&[u8]> {
    if bytes.first() == Some(&0) {
        return Err(Error::NonCanonicalInteger);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::Unsigned;
    use crate::rlp;

    pub const RECIPIENT: [u8; 20] = [
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

    const NATIVE_REFERENCE: &str = "02f00180843b9aca008504a817c80082520894d8da6bf26964af9d7eed9e03e53415d37aa96045880de0b6b3a764000080c0";

    #[test]
    fn a_native_transfer_encodes_to_the_reference_bytes() {
        let mut buffer = [0u8; 128];
        let len = native().encode(&mut buffer).expect("fits");
        assert_eq!(buffer[..len], hex(NATIVE_REFERENCE)[..]);
    }

    #[test]
    fn the_reference_bytes_decode_to_the_same_fields() {
        let bytes = hex(NATIVE_REFERENCE);
        assert_eq!(Unsigned::decode(&bytes), Ok(native()));
    }

    #[test]
    fn a_token_transfer_reference_decodes_with_its_calldata() {
        let bytes = hex(
            "02f86d01078459682f008506fc23ac0082fde894a0b86991c6218b36c1d19d4a2e9eb0ce3606eb4880b844a9059cbb000000000000000000000000d8da6bf26964af9d7eed9e03e53415d37aa960450000000000000000000000000000000000000000000000000000000000bc614ec0",
        );
        let unsigned = Unsigned::decode(&bytes).expect("decodes");
        assert_eq!(unsigned.chain_id, 1);
        assert_eq!(unsigned.nonce, 7);
        assert_eq!(unsigned.gas_limit, 65_000);
        assert_eq!(unsigned.value, &[]);
        assert_eq!(unsigned.data.len(), 68);
        assert_eq!(&unsigned.data[..4], &[0xa9, 0x05, 0x9c, 0xbb]);
        assert_eq!(unsigned.to.map(|to| to[0]), Some(0xa0));
    }

    #[test]
    fn contract_creation_has_no_recipient_and_still_round_trips() {
        let creation = Unsigned {
            to: None,
            ..native()
        };
        let mut buffer = [0u8; 128];
        let len = creation.encode(&mut buffer).expect("fits");
        assert_eq!(Unsigned::decode(&buffer[..len]), Ok(creation));
    }

    #[test]
    fn other_transaction_types_are_rejected() {
        let mut bytes = hex(NATIVE_REFERENCE);
        bytes[0] = 0x01;
        assert_eq!(Unsigned::decode(&bytes), Err(Error::UnsupportedType(0x01)));
        assert_eq!(Unsigned::decode(&[]), Err(Error::Empty));
    }

    #[test]
    fn a_non_empty_access_list_is_rejected() {
        let mut buffer = [0u8; 128];
        let mut encoder = super::envelope_encoder(&mut buffer).expect("fits");
        encoder
            .list(|fields| {
                fields.unsigned(1)?;
                fields.unsigned(0)?;
                fields.quantity(&ONE_GWEI)?;
                fields.quantity(&TWENTY_GWEI)?;
                fields.unsigned(21_000)?;
                fields.bytes(&RECIPIENT)?;
                fields.quantity(&ONE_ETHER)?;
                fields.bytes(&[])?;
                fields.list(|entries| entries.list(|entry| entry.bytes(&RECIPIENT)))
            })
            .expect("fits");
        let len = 1 + encoder.finish();
        assert_eq!(
            Unsigned::decode(&buffer[..len]),
            Err(Error::AccessListNotEmpty)
        );
    }

    #[test]
    fn the_field_count_must_be_exactly_nine() {
        let mut buffer = [0u8; 128];
        let mut encoder = super::envelope_encoder(&mut buffer).expect("fits");
        encoder
            .list(|fields| {
                fields.unsigned(1)?;
                fields.unsigned(0)?;
                fields.quantity(&ONE_GWEI)
            })
            .expect("fits");
        let len = 1 + encoder.finish();
        assert_eq!(Unsigned::decode(&buffer[..len]), Err(Error::TooFewFields));

        let mut buffer = [0u8; 128];
        let mut encoder = super::envelope_encoder(&mut buffer).expect("fits");
        encoder
            .list(|fields| {
                native().encode_fields(fields)?;
                fields.unsigned(1)
            })
            .expect("fits");
        let len = 1 + encoder.finish();
        assert_eq!(Unsigned::decode(&buffer[..len]), Err(Error::TooManyFields));
    }

    #[test]
    fn integers_with_leading_zeros_are_rejected() {
        let mut buffer = [0u8; 128];
        let mut encoder = super::envelope_encoder(&mut buffer).expect("fits");
        encoder
            .list(|fields| {
                fields.bytes(&[0x00, 0x01])?;
                fields.unsigned(0)?;
                fields.quantity(&ONE_GWEI)?;
                fields.quantity(&TWENTY_GWEI)?;
                fields.unsigned(21_000)?;
                fields.bytes(&RECIPIENT)?;
                fields.quantity(&ONE_ETHER)?;
                fields.bytes(&[])?;
                fields.list(|_| Ok(()))
            })
            .expect("fits");
        let len = 1 + encoder.finish();
        assert_eq!(
            Unsigned::decode(&buffer[..len]),
            Err(Error::NonCanonicalInteger)
        );
    }

    #[test]
    fn integers_wider_than_their_field_are_rejected() {
        let mut buffer = [0u8; 128];
        let mut encoder = super::envelope_encoder(&mut buffer).expect("fits");
        encoder
            .list(|fields| {
                fields.bytes(&[0x01; 9])?;
                fields.unsigned(0)?;
                fields.quantity(&ONE_GWEI)?;
                fields.quantity(&TWENTY_GWEI)?;
                fields.unsigned(21_000)?;
                fields.bytes(&RECIPIENT)?;
                fields.quantity(&ONE_ETHER)?;
                fields.bytes(&[])?;
                fields.list(|_| Ok(()))
            })
            .expect("fits");
        let len = 1 + encoder.finish();
        assert_eq!(
            Unsigned::decode(&buffer[..len]),
            Err(Error::IntegerTooLarge)
        );
    }

    #[test]
    fn an_address_of_the_wrong_length_is_rejected() {
        let mut buffer = [0u8; 128];
        let mut encoder = super::envelope_encoder(&mut buffer).expect("fits");
        encoder
            .list(|fields| {
                fields.unsigned(1)?;
                fields.unsigned(0)?;
                fields.quantity(&ONE_GWEI)?;
                fields.quantity(&TWENTY_GWEI)?;
                fields.unsigned(21_000)?;
                fields.bytes(&RECIPIENT[..19])?;
                fields.quantity(&ONE_ETHER)?;
                fields.bytes(&[])?;
                fields.list(|_| Ok(()))
            })
            .expect("fits");
        let len = 1 + encoder.finish();
        assert_eq!(
            Unsigned::decode(&buffer[..len]),
            Err(Error::BadAddressLength(19))
        );
    }

    #[test]
    fn trailing_bytes_after_the_envelope_are_rejected() {
        let mut bytes = hex(NATIVE_REFERENCE);
        bytes.push(0x00);
        assert_eq!(
            Unsigned::decode(&bytes),
            Err(Error::Rlp(rlp::Error::TrailingBytes(1)))
        );
    }

    #[test]
    fn a_short_buffer_is_refused_when_encoding() {
        let mut buffer = [0u8; 10];
        assert!(native().encode(&mut buffer).is_err());
        let mut empty = [0u8; 0];
        assert!(native().encode(&mut empty).is_err());
    }
}
