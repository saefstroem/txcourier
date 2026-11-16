use txcourier_protocol::transaction::{ADDRESS_LEN, Unsigned};
use txcourier_protocol::transfer::encode_transfer;

use crate::result::Result;

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
pub struct Draft {
    pub chain_id: u64,
    pub nonce: u64,
    pub fees: Fees,
    pub payment: Payment,
}

impl Draft {
    pub fn encode(&self, out: &mut [u8]) -> Result<usize> {
        let max_priority_fee = self.fees.max_priority_fee.to_be_bytes();
        let max_fee = self.fees.max_fee.to_be_bytes();

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

        Ok(unsigned.encode(out)?)
    }
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
    use txcourier_protocol::policy::approve;
    use txcourier_protocol::transaction::Unsigned;
    use txcourier_protocol::transfer::{Transfer, classify};
    use txcourier_protocol::wire::TRANSACTION_CAPACITY;

    use super::{Draft, Fees, Payment, word};

    const RECIPIENT: [u8; 20] = [
        0xd8, 0xda, 0x6b, 0xf2, 0x69, 0x64, 0xaf, 0x9d, 0x7e, 0xed, 0x9e, 0x03, 0xe5, 0x34, 0x15,
        0xd3, 0x7a, 0xa9, 0x60, 0x45,
    ];
    const USDT: [u8; 20] = [
        0xda, 0xc1, 0x7f, 0x95, 0x8d, 0x2e, 0xe5, 0x23, 0xa2, 0x20, 0x62, 0x06, 0x99, 0x45, 0x97,
        0xc1, 0x3d, 0x83, 0x1e, 0xc7,
    ];
    const ONE_ETHER: u128 = 1_000_000_000_000_000_000;
    const ONE_GWEI: u128 = 1_000_000_000;

    const NATIVE_REFERENCE: &str = "02f00180843b9aca008504a817c80082520894d8da6bf26964af9d7eed9e03e53415d37aa96045880de0b6b3a764000080c0";

    fn fees(gas_limit: u64) -> Fees {
        Fees {
            max_priority_fee: ONE_GWEI,
            max_fee: 20 * ONE_GWEI,
            gas_limit,
        }
    }

    fn native() -> Draft {
        Draft {
            chain_id: 1,
            nonce: 0,
            fees: fees(21_000),
            payment: Payment::Native {
                recipient: RECIPIENT,
                value: ONE_ETHER,
            },
        }
    }

    fn token() -> Draft {
        Draft {
            chain_id: 1,
            nonce: 7,
            fees: fees(100_000),
            payment: Payment::Token {
                contract: USDT,
                recipient: RECIPIENT,
                amount: 12_500_000,
            },
        }
    }

    fn encoded(draft: &Draft) -> Vec<u8> {
        let mut buffer = [0u8; TRANSACTION_CAPACITY];
        let len = draft.encode(&mut buffer).expect("fits");
        buffer[..len].to_vec()
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("hex"))
            .collect()
    }

    #[test]
    fn a_native_draft_encodes_to_the_protocol_reference_bytes() {
        assert_eq!(encoded(&native()), hex(NATIVE_REFERENCE));
    }

    #[test]
    fn a_native_draft_is_a_native_transfer_the_courier_accepts() {
        let bytes = encoded(&native());
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
            Transfer::Token { .. } => panic!("a native draft classified as a token transfer"),
        }
    }

    #[test]
    fn a_token_draft_is_a_token_transfer_the_courier_accepts() {
        let bytes = encoded(&token());
        assert_eq!(approve(&bytes), Ok(()));

        let unsigned = Unsigned::decode(&bytes).expect("decodes");
        assert_eq!(unsigned.nonce, 7);
        assert_eq!(unsigned.gas_limit, 100_000);
        assert_eq!(unsigned.to, Some(&USDT));
        assert!(unsigned.value.is_empty());

        match classify(&unsigned).expect("classifies") {
            Transfer::Token {
                chain_id,
                contract,
                recipient,
                amount,
            } => {
                assert_eq!(chain_id, 1);
                assert_eq!(contract, &USDT);
                assert_eq!(recipient, &RECIPIENT);
                assert_eq!(amount, &word(12_500_000));
            }
            Transfer::Native { .. } => panic!("a token draft classified as a native transfer"),
        }
    }

    #[test]
    fn an_amount_sits_at_the_end_of_its_word() {
        let encoded = word(0x0102);
        assert_eq!(encoded[..30], [0u8; 30]);
        assert_eq!(encoded[30..], [0x01, 0x02]);
        assert_eq!(word(u128::MAX)[..16], [0u8; 16]);
    }
}
