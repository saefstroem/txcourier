use crate::draft::error::Error;
use crate::draft::result::Result;
use crate::token::{Kind, find};
use crate::transaction::{ADDRESS_LEN, Unsigned};
use crate::transfer::encode_transfer;

pub const RECORD_LEN: usize = 101;

const AMOUNT_WORD_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fees {
    pub max_priority_fee: u128,
    pub max_fee: u128,
    pub gas_limit: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payment {
    Native {
        recipient: [u8; ADDRESS_LEN],
        value: u128,
    },
    Token {
        contract: [u8; ADDRESS_LEN],
        recipient: [u8; ADDRESS_LEN],
        amount: u128,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A Draft transaction that is sent to the courier for
/// conversion to RLP encoded transaction
pub struct Draft {
    pub chain_id: u64,
    pub nonce: u64,
    pub fees: Fees,
    pub payment: Payment,
}

impl Draft {
    pub fn decode(record: &[u8]) -> Result<Self> {
        if record.len() != RECORD_LEN {
            return Err(Error::WrongLength(record.len()));
        }

        let mut rest = record;
        let chain_id = u64::from_be_bytes(take(&mut rest)?);
        let nonce = u64::from_be_bytes(take(&mut rest)?);
        let max_priority_fee = u128::from_be_bytes(take(&mut rest)?);
        let max_fee = u128::from_be_bytes(take(&mut rest)?);
        let gas_limit = u64::from_be_bytes(take(&mut rest)?);
        let ticker: [u8; 8] = take(&mut rest)?;
        let recipient: [u8; 20] = take(&mut rest)?;
        let digits = u128::from_be_bytes(take(&mut rest)?);
        let [places] = take::<1>(&mut rest)?;

        let asset = find(chain_id, &ticker).ok_or(Error::UnknownAsset)?;
        
        // Scale the amount by the number of decimals the asset has
        let amount = scaled(digits, places, asset.decimals)?;

        // Compute payment type
        let payment = match asset.kind {
            Kind::Coin => Payment::Native {
                recipient,
                value: amount,
            },
            Kind::Token { contract, .. } if contract == recipient => {
                return Err(Error::RecipientIsContract);
            }
            Kind::Token { contract, .. } => Payment::Token {
                contract,
                recipient,
                amount,
            },
        };

        Ok(Self {
            chain_id,
            nonce,
            fees: Fees {
                max_priority_fee,
                max_fee,
                gas_limit,
            },
            payment,
        })
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<usize> {
        let max_priority_fee = self.fees.max_priority_fee.to_be_bytes();
        let max_fee = self.fees.max_fee.to_be_bytes();

        // Compute optional calldata, for token transfers
        // this is a transfer call
        let calldata = match self.payment {
            Payment::Native { .. } => None,
            Payment::Token {
                recipient, amount, ..
            } => Some(encode_transfer(&recipient, &word(amount))),
        };

        let (to, value) = match &self.payment {
            Payment::Native { recipient, value } => (recipient, *value),
            Payment::Token { contract, .. } => (contract, 0),
        };
        let value = value.to_be_bytes();

        // Create a typed unsigned transaction
        let unsigned = Unsigned {
            chain_id: self.chain_id,
            nonce: self.nonce,
            max_priority_fee: &max_priority_fee,
            max_fee: &max_fee,
            gas_limit: self.fees.gas_limit,
            to: Some(to),
            value: &value,
            data: calldata.as_ref().map_or(&[], |data| data.as_slice()),
        };

        // RLP Encode the unsigned transaction
        Ok(unsigned.encode(out)?)
    }
}

fn scaled(digits: u128, places: u8, decimals: u8) -> Result<u128> {
    let shift = decimals.checked_sub(places).ok_or(Error::TooPrecise)?;
    10u128
        .checked_pow(u32::from(shift))
        .and_then(|unit| digits.checked_mul(unit))
        .ok_or(Error::AmountTooLarge)
}

fn take<const N: usize>(rest: &mut &[u8]) -> Result<[u8; N]> {
    let Some((head, tail)) = rest.split_first_chunk::<N>() else {
        return Err(Error::WrongLength(RECORD_LEN));
    };
    *rest = tail;
    Ok(*head)
}

fn word(amount: u128) -> [u8; AMOUNT_WORD_LEN] {
    let mut word = [0u8; AMOUNT_WORD_LEN];
    let bytes = amount.to_be_bytes();

    if let Some(slot) = word.get_mut(AMOUNT_WORD_LEN - bytes.len()..) {
        slot.copy_from_slice(&bytes);
    }

    word
}

#[cfg(test)]
mod tests {
    use super::{Draft, Error, Fees, Payment, RECORD_LEN, word};
    use crate::policy::approve;
    use crate::transaction::Unsigned;
    use crate::transfer::{Transfer, classify};
    use crate::wire::TRANSACTION_CAPACITY;

    const RECIPIENT: [u8; 20] = [
        0xd8, 0xda, 0x6b, 0xf2, 0x69, 0x64, 0xaf, 0x9d, 0x7e, 0xed, 0x9e, 0x03, 0xe5, 0x34, 0x15,
        0xd3, 0x7a, 0xa9, 0x60, 0x45,
    ];
    const USDC: [u8; 20] = [
        0xa0, 0xb8, 0x69, 0x91, 0xc6, 0x21, 0x8b, 0x36, 0xc1, 0xd1, 0x9d, 0x4a, 0x2e, 0x9e, 0xb0,
        0xce, 0x36, 0x06, 0xeb, 0x48,
    ];
    const ONE_ETHER: u128 = 1_000_000_000_000_000_000;
    const ONE_GWEI: u128 = 1_000_000_000;

    const NATIVE_RECORD: &str = "000000000000000100000000000000000000000000000000000000003b9aca00000000000000000000000004a817c80000000000000052084554480000000000d8da6bf26964af9d7eed9e03e53415d37aa960450000000000000000000000000000000100";
    const TOKEN_RECORD: &str = "0000000000000001000000000000000700000000000000000000000059682f00000000000000000000000006fc23ac00000000000000fde85553444300000000d8da6bf26964af9d7eed9e03e53415d37aa9604500000000000000000000000000bc614e06";
    const NATIVE_REFERENCE: &str = "02f00180843b9aca008504a817c80082520894d8da6bf26964af9d7eed9e03e53415d37aa96045880de0b6b3a764000080c0";
    const TOKEN_REFERENCE: &str = "02f86d01078459682f008506fc23ac0082fde894a0b86991c6218b36c1d19d4a2e9eb0ce3606eb4880b844a9059cbb000000000000000000000000d8da6bf26964af9d7eed9e03e53415d37aa960450000000000000000000000000000000000000000000000000000000000bc614ec0";

    const NONCE_AT: usize = 8;
    const TICKER_AT: usize = 56;
    const RECIPIENT_AT: usize = 64;
    const DIGITS_AT: usize = 84;
    const PLACES_AT: usize = 100;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("hex"))
            .collect()
    }

    fn encoded(draft: &Draft) -> Vec<u8> {
        let mut buffer = [0u8; TRANSACTION_CAPACITY];
        let len = draft.encode(&mut buffer).expect("fits");
        buffer[..len].to_vec()
    }

    fn with(record: &str, chain_id: u64, ticker: &[u8], digits: u128, places: u8) -> Vec<u8> {
        let mut record = hex(record);
        record[..NONCE_AT].copy_from_slice(&chain_id.to_be_bytes());
        record[TICKER_AT..RECIPIENT_AT].fill(0);
        record[TICKER_AT..TICKER_AT + ticker.len()].copy_from_slice(ticker);
        record[DIGITS_AT..PLACES_AT].copy_from_slice(&digits.to_be_bytes());
        record[PLACES_AT] = places;
        record
    }

    fn paid(record: &[u8]) -> u128 {
        match Draft::decode(record).expect("decodes").payment {
            Payment::Native { value, .. } => value,
            Payment::Token { amount, .. } => amount,
        }
    }

    #[test]
    fn a_native_record_decodes_to_its_fields() {
        assert_eq!(
            Draft::decode(&hex(NATIVE_RECORD)),
            Ok(Draft {
                chain_id: 1,
                nonce: 0,
                fees: Fees {
                    max_priority_fee: ONE_GWEI,
                    max_fee: 20 * ONE_GWEI,
                    gas_limit: 21_000,
                },
                payment: Payment::Native {
                    recipient: RECIPIENT,
                    value: ONE_ETHER,
                },
            })
        );
    }

    #[test]
    fn a_native_record_builds_the_reference_transaction_the_courier_accepts() {
        let bytes = encoded(&Draft::decode(&hex(NATIVE_RECORD)).expect("decodes"));
        assert_eq!(bytes, hex(NATIVE_REFERENCE));
        assert_eq!(approve(&bytes), Ok(()));

        let unsigned = Unsigned::decode(&bytes).expect("decodes");
        match classify(&unsigned).expect("classifies") {
            Transfer::Native {
                chain_id,
                recipient,
                value,
            } => {
                assert_eq!(chain_id, 1);
                assert_eq!(recipient, &RECIPIENT);
                assert_eq!(value, &ONE_ETHER.to_be_bytes()[8..]);
            }
            Transfer::Token { .. } => panic!("a coin record classified as a token transfer"),
        }
    }

    #[test]
    fn a_token_record_builds_the_reference_transaction_the_courier_accepts() {
        let bytes = encoded(&Draft::decode(&hex(TOKEN_RECORD)).expect("decodes"));
        assert_eq!(bytes, hex(TOKEN_REFERENCE));
        assert_eq!(approve(&bytes), Ok(()));

        let unsigned = Unsigned::decode(&bytes).expect("decodes");
        assert_eq!(unsigned.nonce, 7);
        assert_eq!(unsigned.gas_limit, 65_000);
        assert_eq!(unsigned.to, Some(&USDC));
        assert!(unsigned.value.is_empty());

        match classify(&unsigned).expect("classifies") {
            Transfer::Token {
                chain_id,
                contract,
                recipient,
                amount,
            } => {
                assert_eq!(chain_id, 1);
                assert_eq!(contract, &USDC);
                assert_eq!(recipient, &RECIPIENT);
                assert_eq!(amount, &word(12_345_678));
            }
            Transfer::Native { .. } => panic!("a token record classified as a coin transfer"),
        }
    }

    #[test]
    fn an_amount_is_scaled_to_the_base_units_of_its_asset() {
        assert_eq!(
            paid(&with(NATIVE_RECORD, 1, b"ETH", 15, 1)),
            15 * ONE_ETHER / 10
        );
        assert_eq!(paid(&with(NATIVE_RECORD, 56, b"BNB", 1, 18)), 1);
        assert_eq!(paid(&with(TOKEN_RECORD, 1, b"USDC", 1, 0)), 1_000_000);
        assert_eq!(paid(&with(TOKEN_RECORD, 56, b"USDC", 1, 0)), ONE_ETHER);
        assert_eq!(paid(&with(TOKEN_RECORD, 1, b"PAXG", 25, 2)), ONE_ETHER / 4);
    }

    #[test]
    fn the_largest_fields_still_build_a_transaction_the_courier_accepts() {
        let mut record = hex(TOKEN_RECORD);
        record[NONCE_AT..TICKER_AT].fill(0xFF);
        record[RECIPIENT_AT..PLACES_AT].fill(0xFF);
        assert_eq!(paid(&record), u128::MAX);
        let bytes = encoded(&Draft::decode(&record).expect("decodes"));
        assert_eq!(approve(&bytes), Ok(()));
    }

    #[test]
    fn a_record_of_any_other_length_is_refused() {
        for len in [0, 1, RECORD_LEN - 1, RECORD_LEN + 1, 2 * RECORD_LEN] {
            assert_eq!(
                Draft::decode(&vec![0u8; len]),
                Err(Error::WrongLength(len)),
                "{len}"
            );
        }
    }

    #[test]
    fn an_asset_that_is_not_built_in_on_that_chain_is_refused() {
        let unknown = [
            with(NATIVE_RECORD, 1, b"BNB", 1, 0),
            with(NATIVE_RECORD, 56, b"ETH", 1, 0),
            with(NATIVE_RECORD, 8453, b"ETH", 1, 0),
            with(NATIVE_RECORD, 1, b"eth", 1, 0),
            with(NATIVE_RECORD, 1, b"DAI", 1, 0),
            with(NATIVE_RECORD, 1, b"ETH\0\0\0\0\x01", 1, 0),
            with(NATIVE_RECORD, 1, b"", 1, 0),
        ];
        for record in unknown {
            assert_eq!(
                Draft::decode(&record),
                Err(Error::UnknownAsset),
                "{:?}",
                &record[TICKER_AT..RECIPIENT_AT]
            );
        }
    }

    #[test]
    fn an_amount_finer_than_its_asset_is_refused() {
        assert_eq!(
            Draft::decode(&with(TOKEN_RECORD, 1, b"USDC", 1, 7)),
            Err(Error::TooPrecise)
        );
        assert_eq!(
            Draft::decode(&with(NATIVE_RECORD, 1, b"ETH", 1, 19)),
            Err(Error::TooPrecise)
        );
        assert_eq!(
            Draft::decode(&with(NATIVE_RECORD, 1, b"ETH", 1, u8::MAX)),
            Err(Error::TooPrecise)
        );
        assert_eq!(paid(&with(TOKEN_RECORD, 1, b"USDC", 1, 6)), 1);
    }

    #[test]
    fn an_amount_beyond_128_bits_of_base_units_is_refused() {
        let most = u128::MAX / ONE_ETHER;
        assert_eq!(
            paid(&with(NATIVE_RECORD, 1, b"ETH", most, 0)),
            most * ONE_ETHER
        );
        assert_eq!(
            Draft::decode(&with(NATIVE_RECORD, 1, b"ETH", most + 1, 0)),
            Err(Error::AmountTooLarge)
        );
        assert_eq!(
            Draft::decode(&with(NATIVE_RECORD, 1, b"ETH", u128::MAX, 0)),
            Err(Error::AmountTooLarge)
        );
    }

    #[test]
    fn a_token_sent_to_its_own_contract_is_refused() {
        let mut record = hex(TOKEN_RECORD);
        record[RECIPIENT_AT..DIGITS_AT].copy_from_slice(&USDC);
        assert_eq!(Draft::decode(&record), Err(Error::RecipientIsContract));

        let mut coin = hex(NATIVE_RECORD);
        coin[RECIPIENT_AT..DIGITS_AT].copy_from_slice(&USDC);
        assert!(Draft::decode(&coin).is_ok());
    }

    #[test]
    fn a_draft_that_does_not_fit_the_buffer_is_an_error() {
        let draft = Draft::decode(&hex(TOKEN_RECORD)).expect("decodes");
        let mut small = [0u8; 16];
        assert!(matches!(
            draft.encode(&mut small),
            Err(Error::Transaction(_))
        ));
    }

    #[test]
    fn an_amount_sits_at_the_end_of_its_word() {
        let encoded = word(0x0102);
        assert_eq!(encoded[..30], [0u8; 30]);
        assert_eq!(encoded[30..], [0x01, 0x02]);
        assert_eq!(word(u128::MAX)[..16], [0u8; 16]);
    }
}
