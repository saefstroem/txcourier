use crate::transaction::ADDRESS_LEN;
use crate::transfer::error::Error;
use crate::transfer::result::Result;

// The first four bytes of keccak256("transfer(address,uint256)"), the ABI function selector.
pub const TRANSFER_SELECTOR: [u8; SELECTOR_LEN] = [0xA9, 0x05, 0x9C, 0xBB];
// ERC-20 transfer takes a uint256, one 32-byte ABI word.
pub const AMOUNT_LEN: usize = 32;
// The selector and the two ABI words of transfer(address,uint256): 4 + 32 + 32.
pub const CALLDATA_LEN: usize = SELECTOR_LEN + WORD_LEN + AMOUNT_LEN;

// Solidity ABI: call data opens with the first four bytes of the signature hash.
const SELECTOR_LEN: usize = 4;
// Solidity ABI: every static argument is padded to a 32-byte word.
const WORD_LEN: usize = 32;
// Solidity ABI: an address is a uint160, left-padded in its 32-byte word. 32 - 20.
const PADDING_LEN: usize = WORD_LEN - ADDRESS_LEN;

/// Encodes a transfer to calldata based on recipient and amount
pub fn encode_transfer(
    recipient: &[u8; ADDRESS_LEN],
    amount: &[u8; AMOUNT_LEN],
) -> [u8; CALLDATA_LEN] {
    let mut calldata = [0u8; CALLDATA_LEN];

    // Get the slot for the transfer selector 
    if let Some(slot) = calldata.get_mut(..SELECTOR_LEN) {
        slot.copy_from_slice(&TRANSFER_SELECTOR); //write transfer
    }

    // Get the slot for the recipient
    if let Some(slot) = calldata.get_mut(SELECTOR_LEN + PADDING_LEN..SELECTOR_LEN + WORD_LEN) {
        slot.copy_from_slice(recipient); //write recipient
    }

    // Get the slot for amount
    if let Some(slot) = calldata.get_mut(SELECTOR_LEN + WORD_LEN..) {
        slot.copy_from_slice(amount); //write amount
    }

    calldata
}

/// Decode some data into a structured tuple of recipient and amount
pub fn decode_transfer(data: &[u8]) -> Result<(&[u8; ADDRESS_LEN], &[u8; AMOUNT_LEN])> {
    // We always expect a fixed size
    if data.len() != CALLDATA_LEN {
        return Err(Error::BadCalldataLength(data.len()));
    }

    // Get the selector then the rest
    let Some((selector, words)) = data.split_at_checked(SELECTOR_LEN) else {
        return Err(Error::BadCalldataLength(data.len()));
    };

    // Reject invalid selector
    if selector != TRANSFER_SELECTOR {
        let mut found = [0u8; SELECTOR_LEN];
        found.copy_from_slice(selector);
        return Err(Error::UnsupportedSelector(found));
    }

    // Get padding and rest
    let Some((padding, rest)) = words.split_at_checked(PADDING_LEN) else {
        return Err(Error::BadCalldataLength(data.len()));
    };

    // Validate padding correctness
    if padding.iter().any(|byte| *byte != 0) {
        return Err(Error::BadAddressPadding);
    }

    // Extract recipient and amount
    let Some((recipient, amount)) = rest.split_at_checked(ADDRESS_LEN) else {
        return Err(Error::BadCalldataLength(data.len()));
    };

    // Validate rcpt and amount
    let (Ok(recipient), Ok(amount)) = (
        <&[u8; ADDRESS_LEN]>::try_from(recipient),
        <&[u8; AMOUNT_LEN]>::try_from(amount),
    ) else {
        return Err(Error::BadCalldataLength(data.len()));
    };

    Ok((recipient, amount))
}

#[cfg(test)]
mod tests {
    use super::CALLDATA_LEN;
    use super::Error;
    use super::decode_transfer;
    use super::encode_transfer;

    const RECIPIENT: [u8; 20] = [
        0xd8, 0xda, 0x6b, 0xf2, 0x69, 0x64, 0xaf, 0x9d, 0x7e, 0xed, 0x9e, 0x03, 0xe5, 0x34, 0x15,
        0xd3, 0x7a, 0xa9, 0x60, 0x45,
    ];

    fn amount() -> [u8; 32] {
        let mut amount = [0u8; 32];
        amount[29..].copy_from_slice(&[0xbc, 0x61, 0x4e]);
        amount
    }

    #[test]
    fn the_reference_calldata_is_produced() {
        let calldata = encode_transfer(&RECIPIENT, &amount());
        assert_eq!(&calldata[..4], &[0xa9, 0x05, 0x9c, 0xbb]);
        assert_eq!(&calldata[4..16], &[0u8; 12]);
        assert_eq!(&calldata[16..36], &RECIPIENT);
        assert_eq!(&calldata[36..], &amount());
        assert_eq!(calldata.len(), CALLDATA_LEN);
    }

    #[test]
    fn calldata_round_trips() {
        let calldata = encode_transfer(&RECIPIENT, &amount());
        assert_eq!(decode_transfer(&calldata), Ok((&RECIPIENT, &amount())));
    }

    #[test]
    fn any_other_length_is_rejected() {
        let calldata = encode_transfer(&RECIPIENT, &amount());
        assert_eq!(
            decode_transfer(&calldata[..67]),
            Err(Error::BadCalldataLength(67))
        );
        let mut longer = calldata.to_vec();
        longer.push(0);
        assert_eq!(decode_transfer(&longer), Err(Error::BadCalldataLength(69)));
        assert_eq!(decode_transfer(&[]), Err(Error::BadCalldataLength(0)));
    }

    #[test]
    fn approve_and_transfer_from_are_not_transfer() {
        let mut calldata = encode_transfer(&RECIPIENT, &amount());
        calldata[..4].copy_from_slice(&[0x09, 0x5e, 0xa7, 0xb3]);
        assert_eq!(
            decode_transfer(&calldata),
            Err(Error::UnsupportedSelector([0x09, 0x5e, 0xa7, 0xb3]))
        );
        calldata[..4].copy_from_slice(&[0x23, 0xb8, 0x72, 0xdd]);
        assert_eq!(
            decode_transfer(&calldata),
            Err(Error::UnsupportedSelector([0x23, 0xb8, 0x72, 0xdd]))
        );
    }

    #[test]
    fn a_dirty_address_padding_is_rejected() {
        let mut calldata = encode_transfer(&RECIPIENT, &amount());
        calldata[10] = 0x01;
        assert_eq!(decode_transfer(&calldata), Err(Error::BadAddressPadding));
    }
}
