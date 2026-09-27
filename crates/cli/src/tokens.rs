use rp2350wallet_protocol::token::{Tokens, describes};
use rp2350wallet_protocol::transaction::ADDRESS_LEN;

use crate::error::Error;
use crate::hex;
use crate::result::Result;

// Defined by this parser. A # cannot begin a chain id, an address or a hex blob.
const COMMENT: char = '#';
// Defined by this parser: the three fields descriptor() splits a line into.
const SHAPE: &str = "expected: <chain-id> <contract-address> <ledger-descriptor-hex>";

pub fn parse(text: &str) -> Result<Tokens> {
    let mut tokens = Tokens::EMPTY;

    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let line = line.split(COMMENT).next().unwrap_or("").trim();

        if !line.is_empty() {
            let bytes = descriptor(line, number)?;
            tokens
                .push(&bytes)
                .map_err(|error| fault(number, error.to_string()))?;
        }
    }

    Ok(tokens)
}

fn descriptor(line: &str, number: usize) -> Result<Vec<u8>> {
    let mut fields = line.split_whitespace();

    let (Some(chain), Some(address), Some(blob), None) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return Err(fault(number, SHAPE.to_owned()));
    };

    let chain_id = chain
        .parse()
        .map_err(|_| fault(number, format!("{chain} is not a chain id")))?;

    let digits =
        hex::parse(address).ok_or_else(|| fault(number, format!("{address} is not hex")))?;
    let contract = <[u8; ADDRESS_LEN]>::try_from(digits.as_slice())
        .map_err(|_| fault(number, format!("{address} is not 20 bytes")))?;

    let signed = hex::parse(blob)
        .ok_or_else(|| fault(number, format!("{blob} is not a descriptor in hex")))?;
    describes(&signed, &contract, chain_id).map_err(|error| fault(number, error.to_string()))?;

    Ok(signed)
}

fn fault(number: usize, reason: String) -> Error {
    Error::TokenLine { number, reason }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::error::Error;

    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const USDC_BYTES: [u8; 20] = [
        0xa0, 0xb8, 0x69, 0x91, 0xc6, 0x21, 0x8b, 0x36, 0xc1, 0xd1, 0x9d, 0x4a, 0x2e, 0x9e, 0xb0,
        0xce, 0x36, 0x06, 0xeb, 0x48,
    ];
    const USDC_DESCRIPTOR: &str = "0455534443a0b86991c6218b36c1d19d4a2e9eb0ce3606eb4800000006000000013045022100b2e358726e4e6a6752cf344017c0e9d45b9a904120758d45f61b2804f9ad5299022015161ef28d8c4481bd9432c13562def9cce688bcfec896ef244c9a213f106cdd";

    fn line(chain: &str, address: &str) -> String {
        format!("{chain} {address} {USDC_DESCRIPTOR}")
    }

    fn usdc() -> String {
        line("1", USDC)
    }

    #[test]
    fn a_file_of_descriptors_parses_with_comments_and_blank_lines_ignored() {
        let text = format!("\n# the token table pushed to the courier\n{}\n", usdc());
        let tokens = parse(&text).expect("parses");

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens.iter().next().expect("one token").len(), 104);
    }

    #[test]
    fn a_stored_descriptor_is_found_by_the_contract_and_chain_it_names() {
        let tokens = parse(&usdc()).expect("parses");

        assert!(tokens.descriptor(1, &USDC_BYTES).is_some());
        assert!(tokens.descriptor(8453, &USDC_BYTES).is_none());
    }

    #[test]
    fn an_empty_file_is_an_empty_table() {
        assert!(parse("").expect("parses").is_empty());
        assert!(
            parse("# nothing but a comment\n")
                .expect("parses")
                .is_empty()
        );
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        assert_eq!(
            parse(&format!("   {}   ", usdc())).expect("parses").len(),
            1
        );
    }

    #[test]
    fn a_missing_field_names_its_line_and_the_shape_it_wanted() {
        let error = parse(&format!("1 {USDC}")).expect_err("refused");

        assert!(matches!(error, Error::TokenLine { number: 1, .. }));
        assert!(error.to_string().contains("line 1"));
        assert!(error.to_string().contains("expected:"));
    }

    #[test]
    fn a_fourth_field_is_refused_now_that_labels_are_gone() {
        let error = parse(&format!("{} USDC", usdc())).expect_err("refused");

        assert!(error.to_string().contains("expected:"));
    }

    #[test]
    fn a_bad_chain_id_is_refused() {
        let error = parse(&line("mainnet", USDC)).expect_err("refused");

        assert!(error.to_string().contains("mainnet is not a chain id"));
    }

    #[test]
    fn a_bad_contract_address_is_refused() {
        assert!(
            parse(&line("1", "0x1111"))
                .expect_err("refused")
                .to_string()
                .contains("20 bytes")
        );
        assert!(
            parse(&line("1", "nothex"))
                .expect_err("refused")
                .to_string()
                .contains("is not hex")
        );
    }

    #[test]
    fn a_descriptor_that_is_not_hex_is_refused() {
        let error = parse(&format!("1 {USDC} nothex")).expect_err("refused");

        assert!(error.to_string().contains("not a descriptor in hex"));
    }

    #[test]
    fn a_descriptor_for_a_different_contract_is_refused() {
        let error =
            parse(&line("1", "0x1111111111111111111111111111111111111111")).expect_err("refused");

        assert!(error.to_string().contains("different contract"));
    }

    #[test]
    fn a_descriptor_for_a_different_chain_is_refused() {
        let error = parse(&line("8453", USDC)).expect_err("refused");

        assert!(error.to_string().contains("different chain"));
    }

    #[test]
    fn one_token_past_the_table_capacity_names_its_line() {
        let text = vec![usdc(); 16].join("\n");
        let error = parse(&text).expect_err("refused");

        assert!(matches!(error, Error::TokenLine { number: 16, .. }));
        assert!(error.to_string().contains("full"));
    }

    #[test]
    fn the_line_number_counts_comments_and_blanks() {
        let error = parse(&format!("# comment\n\n{}", line("1", "bad"))).expect_err("refused");

        assert!(matches!(error, Error::TokenLine { number: 3, .. }));
    }
}
