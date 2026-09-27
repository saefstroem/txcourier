use crate::transaction::{ADDRESS_LEN, Unsigned};
use crate::transfer::calldata::{AMOUNT_LEN, decode_transfer};
use crate::transfer::error::Error;
use crate::transfer::result::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer<'a> {
    Native {
        chain_id: u64,
        recipient: &'a [u8; ADDRESS_LEN],
        value: &'a [u8],
    },
    Token {
        chain_id: u64,
        contract: &'a [u8; ADDRESS_LEN],
        recipient: &'a [u8; ADDRESS_LEN],
        amount: &'a [u8; AMOUNT_LEN],
    },
}

pub fn classify<'a>(unsigned: &Unsigned<'a>) -> Result<Transfer<'a>> {
    let Some(to) = unsigned.to else {
        return Err(Error::ContractCreation);
    };

    if unsigned.data.is_empty() {
        return Ok(Transfer::Native {
            chain_id: unsigned.chain_id,
            recipient: to,
            value: unsigned.value,
        });
    }

    if !unsigned.value.is_empty() {
        return Err(Error::ValueWithTokenTransfer);
    }

    let (recipient, amount) = decode_transfer(unsigned.data)?;

    Ok(Transfer::Token {
        chain_id: unsigned.chain_id,
        contract: to,
        recipient,
        amount,
    })
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::Transfer;
    use super::classify;
    use crate::transaction::Unsigned;
    use crate::transfer::calldata::encode_transfer;

    const RECIPIENT: [u8; 20] = [0x11; 20];
    const CONTRACT: [u8; 20] = [0x22; 20];
    const VALUE: [u8; 2] = [0x27, 0x10];

    fn unsigned<'a>(to: Option<&'a [u8; 20]>, value: &'a [u8], data: &'a [u8]) -> Unsigned<'a> {
        Unsigned {
            chain_id: 8453,
            nonce: 3,
            max_priority_fee: &[0x01],
            max_fee: &[0x02],
            gas_limit: 21_000,
            to,
            value,
            data,
        }
    }

    #[test]
    fn empty_calldata_is_a_native_transfer() {
        assert_eq!(
            classify(&unsigned(Some(&RECIPIENT), &VALUE, &[])),
            Ok(Transfer::Native {
                chain_id: 8453,
                recipient: &RECIPIENT,
                value: &VALUE,
            })
        );
    }

    #[test]
    fn a_zero_value_native_transfer_is_still_a_native_transfer() {
        assert!(matches!(
            classify(&unsigned(Some(&RECIPIENT), &[], &[])),
            Ok(Transfer::Native { value: &[], .. })
        ));
    }

    #[test]
    fn transfer_calldata_without_value_is_a_token_transfer() {
        let amount = [0x05; 32];
        let calldata = encode_transfer(&RECIPIENT, &amount);
        assert_eq!(
            classify(&unsigned(Some(&CONTRACT), &[], &calldata)),
            Ok(Transfer::Token {
                chain_id: 8453,
                contract: &CONTRACT,
                recipient: &RECIPIENT,
                amount: &amount,
            })
        );
    }

    #[test]
    fn contract_creation_is_rejected() {
        assert_eq!(
            classify(&unsigned(None, &VALUE, &[])),
            Err(Error::ContractCreation)
        );
    }

    #[test]
    fn a_token_transfer_carrying_value_is_rejected() {
        let calldata = encode_transfer(&RECIPIENT, &[0x05; 32]);
        assert_eq!(
            classify(&unsigned(Some(&CONTRACT), &VALUE, &calldata)),
            Err(Error::ValueWithTokenTransfer)
        );
    }

    #[test]
    fn any_other_call_is_rejected() {
        assert_eq!(
            classify(&unsigned(Some(&CONTRACT), &[], &[0xa9, 0x05, 0x9c, 0xbb])),
            Err(Error::BadCalldataLength(4))
        );
        let mut calldata = encode_transfer(&RECIPIENT, &[0x05; 32]);
        calldata[0] = 0x00;
        assert!(matches!(
            classify(&unsigned(Some(&CONTRACT), &[], &calldata)),
            Err(Error::UnsupportedSelector(_))
        ));
    }
}
