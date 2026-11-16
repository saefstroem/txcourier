use crate::bitcoin::error::Error;
use crate::bitcoin::put;
use crate::bitcoin::result::Result;
use crate::bitcoin::{PUBLIC_KEY_LEN, SIGNATURE_MAX};

pub const SCRIPT_LEN: usize = 22; // length of a P2WPKH output script, 0x0014 and a 20-byte key hash, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0141.mediawiki?plain=1#L168-L169
pub const TXID_LEN: usize = 32; // length of a transaction ID, see https://developer.bitcoin.org/reference/transactions.html#outpoint
pub const RECORD_LEN: usize = ADDRESS_LEN + 8 + 8 + TXID_LEN + 4 + 8 + ADDRESS_LEN; // length of the bitcoin record the app sends, txcourier's own layout with no outside spec: sender, amount, fee rate, txid, output index, coin amount, recipient, as read in Draft::decode below

const ADDRESS_LEN: usize = 42; // length of a bc1q address, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0173.mediawiki?plain=1#L233 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0173.mediawiki?plain=1#L64
const PREFIX: &[u8] = b"bc1q"; // mainnet "bc", separator "1" and witness version 0 written as "q", see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0173.mediawiki?plain=1#L207-L212 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0173.mediawiki?plain=1#L81 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0173.mediawiki?plain=1#L111-L112
const CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l"; // bech32 alphabet, a character's position is its 5-bit value, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0173.mediawiki?plain=1#L99-L122
const CHECKSUM_LEN: usize = 6; // bech32 checksum characters at the end of an address, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0173.mediawiki?plain=1#L127-L128
const OP_0_PUSH_20: [u8; 2] = [0x00, 0x14]; // start of a P2WPKH script: witness version 0, then push 20 bytes, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0141.mediawiki?plain=1#L168-L171
const WHOLE_COIN: u64 = u64::MAX;
const TRANSACTION_BASE: usize = 4 + 1 + 41 + 1 + 4; // base size of a transaction record, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0144.mediawiki?plain=1#L26-L76
const OUTPUT: usize = 8 + 1 + SCRIPT_LEN; // size of an output in the transaction, see https://developer.bitcoin.org/reference/transactions.html#txout https://developer.bitcoin.org/reference/transactions.html#compactsize-unsigned-integers https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0141.mediawiki?plain=1#L168-L169
const WITNESS: usize = 2 + 1 + 1 + SIGNATURE_MAX + 1 + PUBLIC_KEY_LEN; // size of the witness data in the transaction, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0141.mediawiki?plain=1#L49-L57 https://developer.bitcoin.org/reference/transactions.html#compactsize-unsigned-integers https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0141.mediawiki?plain=1#L166-L171 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0066.mediawiki?plain=1#L42-L56 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0143.mediawiki?plain=1#L131
const DUST: u64 = 294; // minimum output value considered non-dust, see https://github.com/bitcoin/bitcoin/blob/v32.1/src/policy/policy.cpp#L38-L42

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Represents a draft of a Bitcoin transaction,
///  including the sender, recipient, amounts, and transaction details.
pub struct Draft {
    /// Amount of sats being sent
    pub amount: u64,
    /// Amount of change in sats being returned to the sender
    pub change: u64,
    /// Transaction ID of the previous output being spent
    pub txid: [u8; TXID_LEN],
    /// Index of the output in the previous transaction being spent
    pub vout: u32,
    /// Total input amount in sats for the transaction
    pub input_amount: u64,
    /// Sender's address script
    sender: [u8; SCRIPT_LEN],
    /// Recipient's address script
    recipient: [u8; SCRIPT_LEN],
}

impl Draft {
    pub fn decode(record: &[u8]) -> Result<Self> {
        // Create a ref to the remaining bytes of the record for parsing each field
        let mut rest = record;
        // take 42 bytes (string representation of the address) and convert to script
        let sender = script(take(&mut rest)?)?;
        // take 8 bytes and convert to amount
        let amount = u64::from_be_bytes(take(&mut rest)?);
        // take 8 bytes and convert to sats per kvB
        let sats_per_kvb = u64::from_be_bytes(take(&mut rest)?);
        // take TXID_LEN bytes and convert to txid
        let mut txid: [u8; TXID_LEN] = take(&mut rest)?;
        // reverse the txid to match the expected endianness
        txid.reverse();
        // take 4 bytes and convert to vout
        let vout = u32::from_be_bytes(take(&mut rest)?);
        // take 8 bytes and convert to input amount
        let input_amount = u64::from_be_bytes(take(&mut rest)?);
        // take 22 bytes and convert to recipient script
        let recipient = script(take(&mut rest)?)?;
        // check for any remaining bytes or invalid amounts
        if !rest.is_empty() || amount == 0 || sats_per_kvb == 0 {
            return Err(Error::Malformed);
        }

        // calculate the final amount and change after accounting for fees
        let (amount, change) = paid(amount, sats_per_kvb, input_amount).ok_or(Error::Short)?;
        // ensure the final amount is above the dust threshold
        if amount < DUST {
            return Err(Error::Dust);
        }

        Ok(Self {
            amount,
            change,
            txid,
            vout,
            input_amount,
            sender,
            recipient,
        })
    }

    pub fn sender(&self) -> &[u8] {
        &self.sender
    }

    pub fn recipient(&self) -> &[u8] {
        &self.recipient
    }
}

/// Calculate the amount sent and change for a given payment amount, fee rate, and input coin.
fn paid(amount: u64, sats_per_kvb: u64, coin: u64) -> Option<(u64, u64)> {
    // if we marked to send the whole coin
    if amount == WHOLE_COIN {
        // calculate the amount to send as the whole coin minus the fee
        let sent = coin.checked_sub(fee(1, sats_per_kvb)?)?;
        // return only if the sent amount is greater than 0
        // and we return sent amount and 0 as the change.
        return (sent > 0).then_some((sent, 0));
    }
    // compute how many sats is left after sending the desired amount
    let left = coin.checked_sub(amount)?;
    // calculate the change after accounting for the fee for 2 outputs (recipient + change)
    let change = left.checked_sub(fee(2, sats_per_kvb)?)?;
    // if the change is below the dust threshold, it will be considered as 0 and added to the fee
    Some((amount, if change < DUST { 0 } else { change }))
}

/// Calculate the transaction fee for a given number of outputs and sats per kvB.
/// Returns `None` if the calculation overflows.
fn fee(outputs: usize, sats_per_kvb: u64) -> Option<u64> {
    let vsize = (4 * (TRANSACTION_BASE + outputs * OUTPUT) + WITNESS).div_ceil(4);
    let fee = (vsize as u64).checked_mul(sats_per_kvb)?;
    Some(fee.div_ceil(1000))
}

fn script(address: [u8; ADDRESS_LEN]) -> Result<[u8; SCRIPT_LEN]> {
    // remove the bc1g prefix from address which indicates its a mainnet address
    let Some(data) = address.strip_prefix(PREFIX) else {
        return Err(Error::Malformed);
    };
    // Prepare buffers:
    // script size 22, length is 2 to make room for header we add at the end
    // 
    let (mut script, mut len, mut accumulator, mut bits) = ([0u8; SCRIPT_LEN], 2, 0u32, 0u32);
    for character in data.iter().take(data.len() - CHECKSUM_LEN) {
        // convert character to the representing value in the bech32 charset (5-bit)
        let value = CHARSET.iter().position(|known| known == character);
        // Shift the accumulator by 5 bits to make room for the new 5-bit value
        // and then OR it in.
        accumulator = (accumulator << 5) | value.ok_or(Error::Malformed)? as u32;
        
        // increase the counter that stores how many bits have been set
        bits += 5;
        if bits >= 8 { // when more than 8 bits have been set
            bits -= 8; // reduce the counter by 8 bits to account for the bytes we are about to extract
            // essentially the above subtraction decreases the span that we will now right shift remove
            // get mut ref at the current length position in the script buffer
            // then do right shift on the accumulator to extract the left most 8 bits as a bytes and set it 
            *script.get_mut(len).ok_or(Error::Malformed)? = (accumulator >> bits) as u8;
            len += 1; // increment the position
        }
    }
    // write the two header bytes for the script
    put(&mut script, &[&OP_0_PUSH_20]);
    Ok(script)
}

// Helper function to take the first N bytes from a slice and update the slice to the remaining bytes.
fn take<const N: usize>(rest: &mut &[u8]) -> Result<[u8; N]> {
    let Some((head, tail)) = rest.split_first_chunk::<N>() else {
        return Err(Error::Malformed);
    };
    *rest = tail;
    Ok(*head)
}

#[cfg(test)]
mod tests {
    use super::{Draft, Error};

    const RECORD: &str = "626331716372387465346b723630396763617775746d727a61306a34787638306a79387a333036667975000000000001d4c000000000000013881f1e1d1c1b1a191817161514131211100f0e0d0c0b0a090807060504030201000000000100000000000249f06263317177353038643671656a7874646734793572337a6172766172793063357877376b763866337434";
    const FROM: &str = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
    const TO: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex"))
            .collect()
    }

    fn record(from: &str, to: &str, amount: u64, rate: u64) -> Vec<u8> {
        let txid: Vec<u8> = (0u8..32).rev().collect();
        let coin = [&txid[..], &1u32.to_be_bytes(), &150_000u64.to_be_bytes()].concat();
        let numbers = [amount.to_be_bytes(), rate.to_be_bytes()].concat();
        [from.as_bytes(), &numbers, &coin, to.as_bytes()].concat()
    }

    #[test]
    fn the_reference_record_reads_back_field_by_field() {
        assert_eq!(record(FROM, TO, 120_000, 5_000), bytes(RECORD));
        let draft = Draft::decode(&bytes(RECORD)).expect("valid");
        let sender = bytes("0014c0cebcd6c3d3ca8c75dc5ec62ebe55330ef910e2");
        assert_eq!(draft.sender().to_vec(), sender);
        let fields = (draft.amount, draft.change, draft.input_amount);
        assert_eq!(fields, (120_000, 29_295, 150_000));
        assert_eq!((draft.txid.to_vec(), draft.vout), ((0u8..32).collect(), 1));
    }

    #[test]
    fn the_courier_works_out_the_fee_and_the_change() {
        let paid = |amount, rate| {
            let draft = Draft::decode(&record(FROM, TO, amount, rate)).expect("valid");
            (draft.amount, draft.change)
        };
        assert_eq!(paid(120_000, 2_500), (120_000, 29_647));
        assert_eq!(paid(149_200, 5_000), (149_200, 0));
        assert_eq!(paid(u64::MAX, 5_000), (149_450, 0));
        assert_eq!(paid(294, 1_000), (294, 149_565));
    }

    #[test]
    fn a_bc1q_address_becomes_its_script_and_its_checksum_is_ignored() {
        let script = bytes("0014751e76e8199196d454941c45d1b3a323f1433bd6");
        for to in [TO, "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5"] {
            let draft = Draft::decode(&record(FROM, to, 149_900, 100)).expect("valid");
            assert_eq!(draft.recipient().to_vec(), script);
        }
    }

    #[test]
    fn anything_else_is_refused() {
        for to in [
            "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
            "1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2",
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7bv8f3t4",
            "BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4",
            "bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3",
            "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0",
            "bc1q",
        ] {
            let bad = record(FROM, to, 149_900, 100);
            assert_eq!(Draft::decode(&bad), Err(Error::Malformed), "{to}");
        }
        let pay = |amount, rate| record(FROM, TO, amount, rate);
        let foreign = record("bc1pcr8te4kr609gcawutmrza0j4xv80jy8z306fyu", TO, 9, 1);
        for (bad, error) in [
            (foreign, Error::Malformed),
            (bytes(RECORD)[..60].to_vec(), Error::Malformed),
            (pay(0, 5_000), Error::Malformed),
            (pay(120_000, 0), Error::Malformed),
            (pay(150_000, 1_000), Error::Short),
            (pay(u64::MAX, 2_000_000), Error::Short),
            (pay(1, u64::MAX), Error::Short),
            (pay(293, 1_000), Error::Dust),
            (pay(u64::MAX, 1_360_973), Error::Dust),
        ] {
            assert_eq!(Draft::decode(&bad), Err(error), "{bad:02x?}");
        }
    }
}
