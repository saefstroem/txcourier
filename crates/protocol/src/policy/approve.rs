use crate::policy::error::Error;
use crate::policy::result::Result;
use crate::transaction::{SIGNED_ENVELOPE_GROWTH, Unsigned};
use crate::transfer::classify;
use crate::wire::TRANSACTION_CAPACITY;

/// Verify an unsigned transaction to be correct in accordance with policy
pub fn approve(unsigned: &[u8]) -> Result<()> {
    // Ensure its within the size limits
    if unsigned.len() + SIGNED_ENVELOPE_GROWTH > TRANSACTION_CAPACITY {
        return Err(Error::TooLongToSign {
            len: unsigned.len(),
            capacity: TRANSACTION_CAPACITY,
        });
    }

    // Decode unsigned payload into a structured unsigned transaction 
    let unsigned = Unsigned::decode(unsigned)?;
    
    // Classify and verify the transaction to be correct
    // where correct is by the definition of policy
    classify(&unsigned)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::approve;
    use crate::policy::error::Error;
    use crate::rlp;
    use crate::rlp::Encoder;
    use crate::transaction;
    use crate::transaction::{Signature, Unsigned, encode_signed};
    use crate::transfer;
    use crate::transfer::encode_transfer;
    use crate::wire::TRANSACTION_CAPACITY;

    const QUANTITY_MAX_LEN: usize = 32;
    const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

    const RECIPIENT: [u8; 20] = [
        0xd8, 0xda, 0x6b, 0xf2, 0x69, 0x64, 0xaf, 0x9d, 0x7e, 0xed, 0x9e, 0x03, 0xe5, 0x34, 0x15,
        0xd3, 0x7a, 0xa9, 0x60, 0x45,
    ];
    const CONTRACT: [u8; 20] = [
        0xa0, 0xb8, 0x69, 0x91, 0xc6, 0x21, 0x8b, 0x36, 0xc1, 0xd1, 0x9d, 0x4a, 0x2e, 0x9e, 0xb0,
        0xce, 0x36, 0x06, 0xeb, 0x48,
    ];
    const STRANGER: [u8; 20] = [0x99; 20];
    const AMOUNT: [u8; 32] = [0x01; 32];
    const ONE_ETHER: [u8; 8] = [0x0d, 0xe0, 0xb6, 0xb3, 0xa7, 0x64, 0x00, 0x00];
    const ONE_GWEI: [u8; 4] = [0x3b, 0x9a, 0xca, 0x00];
    const TWENTY_GWEI: [u8; 5] = [0x04, 0xa8, 0x17, 0xc8, 0x00];

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
            .collect()
    }

    fn native_reference() -> Vec<u8> {
        hex(
            "02f00180843b9aca008504a817c80082520894d8da6bf26964af9d7eed9e03e53415d37aa96045880de0b6b3a764000080c0",
        )
    }

    fn token_reference() -> Vec<u8> {
        hex(
            "02f86d01078459682f008506fc23ac0082fde894a0b86991c6218b36c1d19d4a2e9eb0ce3606eb4880b844a9059cbb000000000000000000000000d8da6bf26964af9d7eed9e03e53415d37aa960450000000000000000000000000000000000000000000000000000000000bc614ec0",
        )
    }

    fn envelope(fill: impl FnOnce(&mut Encoder<'_>) -> rlp::Result<()>) -> Vec<u8> {
        let mut buffer = [0u8; 256];
        let len = {
            let mut encoder = Encoder::new(&mut buffer[1..]);
            encoder.list(fill).expect("fits");
            encoder.finish()
        };
        buffer[0] = 0x02;
        buffer[..1 + len].to_vec()
    }

    fn fields(encoder: &mut Encoder<'_>, to: &[u8], value: &[u8], data: &[u8]) -> rlp::Result<()> {
        encoder.unsigned(1)?;
        encoder.unsigned(0)?;
        encoder.quantity(&ONE_GWEI)?;
        encoder.quantity(&TWENTY_GWEI)?;
        encoder.unsigned(21_000)?;
        encoder.bytes(to)?;
        encoder.quantity(value)?;
        encoder.bytes(data)?;
        encoder.list(|_| Ok(()))
    }

    #[test]
    fn a_plain_value_transfer_is_approved() {
        assert_eq!(approve(&native_reference()), Ok(()));
    }

    #[test]
    fn an_erc20_transfer_is_approved() {
        assert_eq!(approve(&token_reference()), Ok(()));
    }

    #[test]
    fn any_recipient_is_allowed_now_that_the_device_holds_no_allowlist() {
        let native = envelope(|encoder| fields(encoder, &STRANGER, &ONE_ETHER, &[]));
        assert_eq!(approve(&native), Ok(()));

        let calldata = encode_transfer(&STRANGER, &AMOUNT);
        let token = envelope(|encoder| fields(encoder, &STRANGER, &[], &calldata));
        assert_eq!(approve(&token), Ok(()));
    }

    #[test]
    fn only_an_eip1559_envelope_is_accepted() {
        let mut bytes = native_reference();
        bytes[0] = 0x01;
        assert_eq!(
            approve(&bytes),
            Err(Error::Transaction(transaction::Error::UnsupportedType(
                0x01
            )))
        );
        assert_eq!(
            approve(&[]),
            Err(Error::Transaction(transaction::Error::Empty))
        );
    }

    #[test]
    fn the_envelope_must_carry_exactly_nine_items() {
        let short = envelope(|encoder| {
            encoder.unsigned(1)?;
            encoder.unsigned(0)?;
            encoder.quantity(&ONE_GWEI)
        });
        assert_eq!(
            approve(&short),
            Err(Error::Transaction(transaction::Error::TooFewFields))
        );

        let long = envelope(|encoder| {
            fields(encoder, &RECIPIENT, &ONE_ETHER, &[])?;
            encoder.unsigned(1)
        });
        assert_eq!(
            approve(&long),
            Err(Error::Transaction(transaction::Error::TooManyFields))
        );
    }

    #[test]
    fn an_access_list_with_an_entry_is_refused() {
        let bytes = envelope(|encoder| {
            encoder.unsigned(1)?;
            encoder.unsigned(0)?;
            encoder.quantity(&ONE_GWEI)?;
            encoder.quantity(&TWENTY_GWEI)?;
            encoder.unsigned(21_000)?;
            encoder.bytes(&RECIPIENT)?;
            encoder.quantity(&ONE_ETHER)?;
            encoder.bytes(&[])?;
            encoder.list(|entries| entries.list(|entry| entry.bytes(&RECIPIENT)))
        });
        assert_eq!(
            approve(&bytes),
            Err(Error::Transaction(transaction::Error::AccessListNotEmpty))
        );
    }

    #[test]
    fn a_transaction_without_a_recipient_creates_a_contract_and_is_refused() {
        let bytes = envelope(|encoder| fields(encoder, &[], &ONE_ETHER, &[]));
        assert_eq!(
            approve(&bytes),
            Err(Error::Transfer(transfer::Error::ContractCreation))
        );
    }

    #[test]
    fn a_recipient_that_is_not_twenty_bytes_is_refused() {
        let bytes = envelope(|encoder| fields(encoder, &[0x11; 19], &ONE_ETHER, &[]));
        assert_eq!(
            approve(&bytes),
            Err(Error::Transaction(transaction::Error::BadAddressLength(19)))
        );
    }

    #[test]
    fn an_erc20_transfer_carrying_value_is_refused() {
        let calldata = encode_transfer(&RECIPIENT, &AMOUNT);
        let bytes = envelope(|encoder| fields(encoder, &CONTRACT, &ONE_ETHER, &calldata));
        assert_eq!(
            approve(&bytes),
            Err(Error::Transfer(transfer::Error::ValueWithTokenTransfer))
        );
    }

    #[test]
    fn calldata_that_is_not_sixty_eight_bytes_is_refused() {
        let calldata = encode_transfer(&RECIPIENT, &AMOUNT);
        let bytes = envelope(|encoder| fields(encoder, &CONTRACT, &[], &calldata[..67]));
        assert_eq!(
            approve(&bytes),
            Err(Error::Transfer(transfer::Error::BadCalldataLength(67)))
        );

        let bytes = envelope(|encoder| fields(encoder, &CONTRACT, &[], &[0xa9, 0x05, 0x9c, 0xbb]));
        assert_eq!(
            approve(&bytes),
            Err(Error::Transfer(transfer::Error::BadCalldataLength(4)))
        );
    }

    #[test]
    fn any_selector_other_than_transfer_is_refused() {
        let mut calldata = encode_transfer(&RECIPIENT, &AMOUNT).to_vec();
        calldata[0] = 0x09;
        calldata[1] = 0x5e;
        calldata[2] = 0xa7;
        calldata[3] = 0xb3;
        let bytes = envelope(|encoder| fields(encoder, &CONTRACT, &[], &calldata));
        assert_eq!(
            approve(&bytes),
            Err(Error::Transfer(transfer::Error::UnsupportedSelector([
                0x09, 0x5e, 0xa7, 0xb3
            ])))
        );
    }

    #[test]
    fn a_recipient_word_that_is_not_left_padded_is_refused() {
        let mut calldata = encode_transfer(&RECIPIENT, &AMOUNT).to_vec();
        calldata[4] = 0x01;
        let bytes = envelope(|encoder| fields(encoder, &CONTRACT, &[], &calldata));
        assert_eq!(
            approve(&bytes),
            Err(Error::Transfer(transfer::Error::BadAddressPadding))
        );
    }

    #[test]
    fn anything_approve_accepts_still_re_encodes_once_the_ledger_has_signed_it() {
        let signature = Signature {
            y_parity: 1,
            r: [0x11; 32],
            s: [0x22; 32],
        };

        for width in 1..=QUANTITY_MAX_LEN {
            let fee = vec![0xEE; width];
            let mut calldata = [0u8; 68];
            calldata[..4].copy_from_slice(&TRANSFER_SELECTOR);
            calldata[16..36].copy_from_slice(&RECIPIENT);
            calldata[36..].copy_from_slice(&AMOUNT);

            let unsigned = Unsigned {
                chain_id: u64::MAX,
                nonce: u64::MAX,
                max_priority_fee: &fee,
                max_fee: &fee,
                gas_limit: u64::MAX,
                to: Some(&CONTRACT),
                value: &[],
                data: &calldata,
            };

            let mut raw = [0u8; 512];
            let len = unsigned.encode(&mut raw).expect("encodes");

            if approve(&raw[..len]).is_err() {
                continue;
            }

            let mut signed = [0u8; TRANSACTION_CAPACITY];
            assert!(
                encode_signed(&unsigned, &signature, &mut signed).is_ok(),
                "approve accepted {len} unsigned bytes but the signed form does not fit \
                 {TRANSACTION_CAPACITY}: the Ledger would sign it and the signature be discarded"
            );
        }
    }

    #[test]
    fn a_transaction_too_long_to_carry_a_signature_is_refused_before_it_is_stored() {
        let fee = [0xEE; QUANTITY_MAX_LEN];
        let mut calldata = [0u8; 68];
        calldata[..4].copy_from_slice(&TRANSFER_SELECTOR);
        calldata[16..36].copy_from_slice(&RECIPIENT);
        calldata[36..].copy_from_slice(&AMOUNT);

        let unsigned = Unsigned {
            chain_id: u64::MAX,
            nonce: u64::MAX,
            max_priority_fee: &fee,
            max_fee: &fee,
            gas_limit: u64::MAX,
            to: Some(&CONTRACT),
            value: &[],
            data: &calldata,
        };

        let mut raw = [0u8; 512];
        let len = unsigned.encode(&mut raw).expect("encodes");

        assert_eq!(
            approve(&raw[..len]),
            Err(Error::TooLongToSign {
                len,
                capacity: TRANSACTION_CAPACITY,
            })
        );
    }
}
