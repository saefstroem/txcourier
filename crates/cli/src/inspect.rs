use txcourier_protocol::transaction::Unsigned;
use txcourier_protocol::transfer::Transfer;

use crate::amount::{GWEI_DECIMALS, NATIVE_DECIMALS};
use crate::{amount, chain, hex};

const U128_BYTES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Denomination<'a> {
    pub symbol: &'a str,
    pub decimals: u32,
}

pub fn lines(
    unsigned: &Unsigned<'_>,
    transfer: &Transfer<'_>,
    token: Option<Denomination<'_>>,
) -> Vec<String> {
    let native = Denomination {
        symbol: chain::symbol(unsigned.chain_id),
        decimals: NATIVE_DECIMALS,
    };
    let mut lines = Vec::new();

    match transfer {
        Transfer::Native {
            recipient, value, ..
        } => {
            lines.push(format!("send     {}", amount_in(value, native)));
            lines.push(format!("to       {}", hex::address(*recipient)));
        }
        Transfer::Token {
            contract,
            recipient,
            amount,
            ..
        } => {
            let sent = match token {
                Some(token) => amount_in(*amount, token),
                None => format!("{} base units of an unidentified token", raw(*amount)),
            };
            lines.push(format!("send     {sent}"));
            lines.push(format!("to       {}", hex::address(*recipient)));
            lines.push(format!("token    {}", hex::address(*contract)));
        }
    }

    lines.push(format!("chain    {}", chain::describe(unsigned.chain_id)));
    lines.push(format!("nonce    {}", unsigned.nonce));
    lines.push(format!(
        "gas      limit {}, max fee {}, priority fee {}",
        unsigned.gas_limit,
        gwei(unsigned.max_fee),
        gwei(unsigned.max_priority_fee)
    ));

    let worst_case = number(unsigned.max_fee)
        .and_then(|max_fee| max_fee.checked_mul(u128::from(unsigned.gas_limit)));
    if let Some(fee) = worst_case {
        lines.push(format!(
            "fees     at most {} {}",
            amount::format(fee, NATIVE_DECIMALS),
            native.symbol
        ));
    }

    lines
}

fn amount_in(bytes: &[u8], denomination: Denomination<'_>) -> String {
    match number(bytes) {
        Some(value) => format!(
            "{} {}",
            amount::format(value, denomination.decimals),
            denomination.symbol
        ),
        None => format!(
            "{} base units of {}",
            hex::prefixed(bytes),
            denomination.symbol
        ),
    }
}

fn gwei(bytes: &[u8]) -> String {
    match number(bytes) {
        Some(value) => format!("{} gwei", amount::format(value, GWEI_DECIMALS)),
        None => format!("{} wei", hex::prefixed(bytes)),
    }
}

fn raw(bytes: &[u8]) -> String {
    number(bytes).map_or_else(|| hex::prefixed(bytes), |value| value.to_string())
}

fn number(bytes: &[u8]) -> Option<u128> {
    let start = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());
    let significant = bytes.get(start..)?;

    (significant.len() <= U128_BYTES).then(|| {
        significant
            .iter()
            .fold(0, |value, byte| value << 8 | u128::from(*byte))
    })
}

#[cfg(test)]
mod tests {
    use txcourier_protocol::transaction::{Signed, Unsigned};
    use txcourier_protocol::transfer::{Transfer, classify, encode_transfer};

    use super::{Denomination, lines, number};
    use crate::hex;

    const BSC_NATIVE: &str = "02f87338831a2ec5840393870084039387008261a894445a0d5b48fcad5af240a72f925609e498ec306a860d48cdb5090080c080a07a6c345139a2c0b62d1d39b1c44fe1871f5e17d5eaddfd19e06ab33e46618fb7a05f7baad2066db1ba75177726c2a5cea918caca4d305548228ae47617d313ca18";
    const RECIPIENT: [u8; 20] = [0x11; 20];
    const CONTRACT: [u8; 20] = [0x22; 20];
    const USDT: Denomination<'static> = Denomination {
        symbol: "USDT",
        decimals: 6,
    };

    fn token_transfer(chain_id: u64, calldata: &[u8]) -> Unsigned<'_> {
        Unsigned {
            chain_id,
            nonce: 7,
            max_priority_fee: &[0x3b, 0x9a, 0xca, 0x00],
            max_fee: &[0x04, 0xa8, 0x17, 0xc8, 0x00],
            gas_limit: 100_000,
            to: Some(&CONTRACT),
            value: &[],
            data: calldata,
        }
    }

    fn report(unsigned: &Unsigned<'_>, token: Option<Denomination<'_>>) -> Vec<String> {
        let transfer = classify(unsigned).expect("a transfer");
        lines(unsigned, &transfer, token)
    }

    #[test]
    fn a_real_signed_native_transfer_reads_in_whole_units() {
        let bytes = hex::parse(BSC_NATIVE).expect("hex");
        let signed = Signed::decode(&bytes).expect("decodes");

        assert_eq!(
            report(&signed.unsigned, None),
            [
                "send     0.00001460634 BNB",
                "to       0x445a0d5b48fcad5af240a72f925609e498ec306a",
                "chain    BNB Smart Chain (56)",
                "nonce    1715909",
                "gas      limit 25000, max fee 0.06 gwei, priority fee 0.06 gwei",
                "fees     at most 0.0000015 BNB",
            ]
        );
    }

    #[test]
    fn a_listed_token_transfer_reads_in_the_tokens_units() {
        let mut amount = [0u8; 32];
        amount[28..].copy_from_slice(&12_500_000u32.to_be_bytes());
        let calldata = encode_transfer(&RECIPIENT, &amount);

        assert_eq!(
            report(&token_transfer(1, &calldata), Some(USDT)),
            [
                "send     12.5 USDT",
                "to       0x1111111111111111111111111111111111111111",
                "token    0x2222222222222222222222222222222222222222",
                "chain    Ethereum (1)",
                "nonce    7",
                "gas      limit 100000, max fee 20 gwei, priority fee 1 gwei",
                "fees     at most 0.002 ETH",
            ]
        );
    }

    #[test]
    fn an_unidentified_token_reads_in_base_units() {
        let mut amount = [0u8; 32];
        amount[31] = 42;
        let calldata = encode_transfer(&RECIPIENT, &amount);

        let report = report(&token_transfer(56, &calldata), None);
        assert_eq!(
            report.first().map(String::as_str),
            Some("send     42 base units of an unidentified token")
        );
        assert!(report.contains(&"fees     at most 0.002 BNB".to_owned()));
    }

    #[test]
    fn an_amount_too_wide_for_u128_is_shown_in_hex() {
        let amount = [0xff; 32];
        let calldata = encode_transfer(&RECIPIENT, &amount);
        let unsigned = token_transfer(1, &calldata);
        let transfer = classify(&unsigned).expect("a transfer");
        assert!(matches!(transfer, Transfer::Token { .. }));

        assert_eq!(
            lines(&unsigned, &transfer, Some(USDT))
                .first()
                .map(String::as_str),
            Some(format!("send     {} base units of USDT", hex::prefixed(&amount)).as_str())
        );
    }

    #[test]
    fn a_chain_other_than_the_known_two_is_still_described() {
        let calldata = encode_transfer(&RECIPIENT, &[0x01; 32]);
        let report = report(&token_transfer(8453, &calldata), None);
        assert!(report.contains(&"chain    chain 8453".to_owned()));
        assert!(report.contains(&"fees     at most 0.002 native units".to_owned()));
    }

    #[test]
    fn big_endian_bytes_become_a_number_whatever_their_leading_zeros() {
        assert_eq!(number(&[]), Some(0));
        assert_eq!(number(&[0, 0, 0]), Some(0));
        assert_eq!(number(&[0x01, 0x00]), Some(256));
        assert_eq!(number(&[0, 0, 0x01, 0x00]), Some(256));
        assert_eq!(number(&[0xff; 16]), Some(u128::MAX));
        assert_eq!(number(&[0x01; 17]), None);

        let mut word = [0u8; 32];
        word[16..].copy_from_slice(&[0xff; 16]);
        assert_eq!(number(&word), Some(u128::MAX));
    }
}
