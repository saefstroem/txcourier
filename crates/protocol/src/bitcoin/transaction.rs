use crate::bitcoin::draft::{Draft, SCRIPT_LEN};
use crate::bitcoin::error::Error;
use crate::bitcoin::result::Result;

pub const PUBLIC_KEY_LEN: usize = 33;
pub const SIGNATURE_MAX: usize = 73;


const VERSION: u32 = 2; // transaction version, see https://developer.bitcoin.org/reference/transactions.html#raw-transaction-format https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0370.mediawiki?plain=1#L59-L64 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0143.mediawiki?plain=1#L29
const SEGWIT: [u8; 2] = [0x00, 0x01]; // segwit marker and flag after the version, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0141.mediawiki?plain=1#L49-L55
const ONE_INPUT: u8 = 1; // input count, one coin per payment, a one-byte var_int, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0144.mediawiki?plain=1#L47-L50 https://developer.bitcoin.org/reference/transactions.html#compactsize-unsigned-integers
const EMPTY_SCRIPT: u8 = 0x00; // length of the input's signature script, empty because segwit carries the proof in the witness, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0141.mediawiki?plain=1#L166-L167 https://developer.bitcoin.org/reference/transactions.html#txin
const SEQUENCE: u32 = 0xFFFF_FFFF; // final sequence: no relative lock time, and what the Ledger assumes when the PSBT names none, see https://developer.bitcoin.org/reference/transactions.html#txin https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0068.mediawiki?plain=1#L30 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0370.mediawiki?plain=1#L148 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0143.mediawiki?plain=1#L35
const LOCKTIME: u32 = 0; // no lock time so valid at once, and what the Ledger assumes when the PSBT names none, see https://github.com/bitcoin/bitcoin/blob/v32.1/src/consensus/tx_verify.cpp#L23-L26 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0370.mediawiki?plain=1#L212 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0143.mediawiki?plain=1#L37
const WITNESS_ITEMS: u8 = 2; // witness items of a P2WPKH input: signature and public key, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0141.mediawiki?plain=1#L166-L171
const CHANGE_OUTPUT_LEN: usize = 8 + 1 + SCRIPT_LEN; // size of the change output: amount, script length, script, see https://developer.bitcoin.org/reference/transactions.html#txout https://developer.bitcoin.org/reference/transactions.html#compactsize-unsigned-integers https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0141.mediawiki?plain=1#L168-L169

/// Constructs a signed Bitcoin transaction from a draft, public key, and signature.
/// The resulting transaction is written to the provided output buffer.
/// Returns the number of bytes written on success.
pub fn signed(
    draft: &Draft, // the draft of the Bitcoin transaction containing sender, recipient, amounts, and transaction details
    public_key: &[u8; PUBLIC_KEY_LEN], // the public key corresponding to the sender's address
    signature: &[u8], // the signature proving ownership of the input being spent
    out: &mut [u8], // the output buffer where the constructed transaction will be written
) -> Result<usize> {
    // Prepare the change output if there is any change to return to the sender.
    let recipient = draft.recipient();
    // get sender and change amount for the change output
    let (sender, amount) = (draft.sender(), draft.change.to_le_bytes());
    let mut change = [0u8; CHANGE_OUTPUT_LEN];
    // construct the change output: amount, script length, script
    put(&mut change, &[&amount, &[SCRIPT_LEN as u8], sender]);
    let paid = draft.change > 0;
    let change: &[u8] = if paid { &change } else { &[] };

    let found = out.len();
    // segwit serialization: version, marker and flag, the input, the outputs, the witness, lock time, numbers little-endian, 
    // see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0144.mediawiki?plain=1#L26-L76 
    // https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0141.mediawiki?plain=1#L49-L57 
    // https://developer.bitcoin.org/reference/transactions.html#txin 
    // https://developer.bitcoin.org/reference/transactions.html#txout 
    // https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0143.mediawiki?plain=1#L28-L38
    put(
        out,
        &[
            &VERSION.to_le_bytes(),
            &SEGWIT,
            &[ONE_INPUT],
            &draft.txid, // Outpoint: The Specific Part Of A Specific Output¶ 
            &draft.vout.to_le_bytes(), // Outpoint: The Specific Part Of A Specific Output¶
            &[EMPTY_SCRIPT], // since this is a p2wpkh input, the scriptSig is empty
            &SEQUENCE.to_le_bytes(), // default sequence
            &[1 + u8::from(paid)], // at least 1 + maybe change
            &draft.amount.to_le_bytes(), // amount for the recipient output
            &[recipient.len() as u8], // length of the recipient script
            recipient, // recipient script
            change, // the change output
            &[WITNESS_ITEMS, signature.len() as u8], // A witness field starts with a var_int to indicate the number of stack items for the txin.
            signature, // witness item 1
            &[PUBLIC_KEY_LEN as u8], // witness item 2
            public_key, // witness item 2
            &LOCKTIME.to_le_bytes(), // lock time
        ],
    )
    .ok_or(Error::OutputTooSmall(found))
}

/// Writes the given parts sequentially into the output buffer.
/// Returns the total number of bytes written, or `None` if the buffer is too small.
pub(crate) fn put(out: &mut [u8], parts: &[&[u8]]) -> Option<usize> {
    let mut len = 0;
    // iterate over each part and copy it into the output buffer, updating the length
    for part in parts {
        // check if there is enough space in the output buffer for this part
        out.get_mut(len..len + part.len())?.copy_from_slice(part);
        len += part.len();
    }
    Some(len)
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::signed;
    use crate::bitcoin::draft::Draft;
    use crate::wire::TRANSACTION_CAPACITY;

    const RECORD: &str = "626331716372387465346b723630396763617775746d727a61306a34787638306a79387a333036667975000000000001d4c000000000000013881f1e1d1c1b1a191817161514131211100f0e0d0c0b0a090807060504030201000000000100000000000249f06263317177353038643671656a7874646734793572337a6172766172793063357877376b763866337434";
    const PUBLIC_KEY: &str = "0330d54fd0dd420a6e5f8d3624f5f3482cae350f79d5f0753bf5beef9c2d91af3c";
    const SIGNED: &str = "02000000000101000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f0100000000ffffffff02c0d4010000000000160014751e76e8199196d454941c45d1b3a323f1433bd66f72000000000000160014c0cebcd6c3d3ca8c75dc5ec62ebe55330ef910e2024730440220aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0220bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb01210330d54fd0dd420a6e5f8d3624f5f3482cae350f79d5f0753bf5beef9c2d91af3c00000000";

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex"))
            .collect()
    }

    fn signature() -> Vec<u8> {
        [
            &[0x30, 0x44, 0x02, 0x20][..],
            &[0xAA; 32],
            &[0x02, 0x20],
            &[0xBB; 32],
            &[0x01],
        ]
        .concat()
    }

    fn public_key() -> [u8; 33] {
        bytes(PUBLIC_KEY).try_into().expect("33 bytes")
    }

    #[test]
    fn the_reference_draft_serializes_to_the_reference_segwit_transaction() {
        let draft = Draft::decode(&bytes(RECORD)).expect("valid");
        let mut out = [0u8; TRANSACTION_CAPACITY];
        let len = signed(&draft, &public_key(), &signature(), &mut out).expect("fits");
        assert_eq!(out[..len].to_vec(), bytes(SIGNED));
        let longest = signed(&draft, &public_key(), &[0x30; 73], &mut out).expect("fits");
        assert_eq!(longest, 224, "the largest signature still fits a slot");
        let mut short = [0u8; 100];
        let too_long = signed(&draft, &public_key(), &signature(), &mut short);
        assert_eq!(too_long, Err(Error::OutputTooSmall(100)));
    }

    #[test]
    fn spending_everything_leaves_out_the_change_output() {
        let mut record = bytes(RECORD);
        record[42..50].copy_from_slice(&u64::MAX.to_be_bytes());
        let draft = Draft::decode(&record).expect("valid");
        let mut out = [0u8; TRANSACTION_CAPACITY];
        let len = signed(&draft, &public_key(), &signature(), &mut out).expect("fits");
        assert_eq!((len, out[4 + 2 + 1 + 41]), (bytes(SIGNED).len() - 31, 1));
    }
}
