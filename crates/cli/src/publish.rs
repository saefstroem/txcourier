use alloy::primitives::{B256, keccak256};
use alloy::providers::Provider;

use crate::chain::Chain;
use crate::error::Error;
use crate::result::Result;
use crate::rpc::ask;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Accepted,
    AlreadyKnown,
}

pub fn broadcast(
    chain: &Chain,
    signed: &[u8],
    mut report: impl FnMut(&str, &Error),
) -> Result<Outcome> {
    let hash = keccak256(signed);
    let mut doubt = false;

    for url in chain.rpc {
        match send(url, signed, hash) {
            Ok(()) => return Ok(Outcome::Accepted),
            Err(error) => {
                doubt |= leaves_doubt(&error);
                report(url, &error);
            }
        }
    }

    for url in chain.rpc {
        match known(url, hash) {
            Ok(true) => return Ok(Outcome::AlreadyKnown),
            Ok(false) => {}
            Err(_) => doubt = true,
        }
    }

    if doubt {
        Err(Error::Unconfirmed(format!("{}{hash}", chain.explorer)))
    } else {
        Err(Error::NotPublished)
    }
}

fn send(url: &str, signed: &[u8], expected: B256) -> Result<()> {
    let answered = ask(url, |node| async move {
        node.send_raw_transaction(signed)
            .await
            .map(|pending| *pending.tx_hash())
    })?;

    if answered == expected {
        Ok(())
    } else {
        Err(Error::HashMismatch { answered, expected })
    }
}

fn known(url: &str, hash: B256) -> Result<bool> {
    ask(url, |node| async move {
        node.get_transaction_by_hash(hash)
            .await
            .map(|found| found.is_some())
    })
}

const fn leaves_doubt(error: &Error) -> bool {
    matches!(
        error,
        Error::NoAnswer { .. } | Error::Transport(_) | Error::HashMismatch { .. }
    )
}

#[cfg(test)]
mod tests {
    use alloy::primitives::keccak256;

    use super::{Outcome, broadcast, leaves_doubt};
    use crate::chain::Chain;
    use crate::error::Error;
    use crate::fake_node::{Reply, serve};
    use crate::hex;

    const BSC_NATIVE: &str = "02f87338831a2ec5840393870084039387008261a894445a0d5b48fcad5af240a72f925609e498ec306a860d48cdb5090080c080a07a6c345139a2c0b62d1d39b1c44fe1871f5e17d5eaddfd19e06ab33e46618fb7a05f7baad2066db1ba75177726c2a5cea918caca4d305548228ae47617d313ca18";
    const BSC_NATIVE_HASH: &str =
        "0xfad9f10a64b130c46a907415fe0edb9d564d0c932bd1c2625566a3696bc3a9c1";

    const ACCEPTED: Reply = Reply::Answer(
        r#"{"jsonrpc":"2.0","id":ID,"result":"0xfad9f10a64b130c46a907415fe0edb9d564d0c932bd1c2625566a3696bc3a9c1"}"#,
    );
    const OTHER_HASH: Reply = Reply::Answer(
        r#"{"jsonrpc":"2.0","id":ID,"result":"0x1111111111111111111111111111111111111111111111111111111111111111"}"#,
    );
    const ALREADY_KNOWN: Reply = Reply::Answer(
        r#"{"jsonrpc":"2.0","id":ID,"error":{"code":-32000,"message":"already known"}}"#,
    );
    const NO_FUNDS: Reply = Reply::Answer(
        r#"{"jsonrpc":"2.0","id":ID,"error":{"code":-32000,"message":"insufficient funds for gas * price + value"}}"#,
    );
    const NOT_FOUND: Reply = Reply::Answer(r#"{"jsonrpc":"2.0","id":ID,"result":null}"#);
    const FOUND: Reply = Reply::Answer(
        r#"{"jsonrpc":"2.0","id":ID,"result":{"blockHash":"0xd46afed688a98025495042b4a115d21db71aa09782589e298033566191189c49","blockNumber":"0x7697d1f","from":"0xb01caea8c6c47bbf4f4b4c5080ca642043359c2e","gas":"0x61a8","gasPrice":"0x3938700","maxFeePerGas":"0x3938700","maxPriorityFeePerGas":"0x3938700","hash":"0xfad9f10a64b130c46a907415fe0edb9d564d0c932bd1c2625566a3696bc3a9c1","input":"0x","nonce":"0x1a2ec5","to":"0x445a0d5b48fcad5af240a72f925609e498ec306a","transactionIndex":"0xd","value":"0xd48cdb50900","type":"0x2","accessList":[],"chainId":"0x38","v":"0x0","r":"0x7a6c345139a2c0b62d1d39b1c44fe1871f5e17d5eaddfd19e06ab33e46618fb7","s":"0x5f7baad2066db1ba75177726c2a5cea918caca4d305548228ae47617d313ca18","yParity":"0x0"}}"#,
    );

    fn chain(urls: Vec<String>) -> Chain {
        let rpc: Vec<&'static str> = urls
            .into_iter()
            .map(|url| &*Box::leak(url.into_boxed_str()))
            .collect();

        Chain {
            id: 56,
            name: "BNB Smart Chain",
            symbol: "BNB",
            rpc: Box::leak(rpc.into_boxed_slice()),
            explorer: "https://bscscan.com/tx/",
        }
    }

    fn broadcast_to(nodes: Vec<Vec<Reply>>) -> (Result<Outcome, Error>, Vec<String>) {
        let urls = nodes.into_iter().map(|replies| serve(replies).0).collect();
        let bytes = hex::parse(BSC_NATIVE).expect("hex");
        let mut reported = Vec::new();

        let outcome = broadcast(&chain(urls), &bytes, |url, error| {
            reported.push(format!("{url}: {error}"));
        });

        (outcome, reported)
    }

    #[test]
    fn the_transaction_hash_is_the_keccak_of_its_bytes() {
        let bytes = hex::parse(BSC_NATIVE).expect("hex");
        assert_eq!(keccak256(&bytes).to_string(), BSC_NATIVE_HASH);
    }

    #[test]
    fn the_first_node_that_answers_the_right_hash_publishes_it() {
        let (url, requests) = serve(vec![ACCEPTED]);
        let bytes = hex::parse(BSC_NATIVE).expect("hex");

        let outcome = broadcast(&chain(vec![url]), &bytes, |_, error| {
            panic!("nothing should fail: {error}")
        });

        assert_eq!(outcome, Ok(Outcome::Accepted));
        let request = requests.recv().expect("one request");
        assert!(request.contains("eth_sendRawTransaction"));
        assert!(request.contains(&format!("0x{BSC_NATIVE}")));
    }

    #[test]
    fn a_dead_node_is_reported_and_the_next_one_is_tried() {
        let (outcome, reported) = broadcast_to(vec![vec![Reply::HangUp], vec![ACCEPTED]]);

        assert_eq!(outcome, Ok(Outcome::Accepted));
        assert_eq!(reported.len(), 1);
    }

    #[test]
    fn a_transaction_the_network_already_has_is_found_by_its_hash() {
        let (outcome, reported) = broadcast_to(vec![
            vec![ALREADY_KNOWN, FOUND],
            vec![ALREADY_KNOWN, NOT_FOUND],
        ]);

        assert_eq!(outcome, Ok(Outcome::AlreadyKnown));
        assert_eq!(reported.len(), 2);
        assert!(reported[0].contains("already known"), "{reported:?}");
    }

    #[test]
    fn clean_refusals_from_every_node_mean_it_was_not_published() {
        let (outcome, reported) =
            broadcast_to(vec![vec![NO_FUNDS, NOT_FOUND], vec![NO_FUNDS, NOT_FOUND]]);

        assert_eq!(outcome, Err(Error::NotPublished));
        assert!(
            reported
                .iter()
                .all(|line| line.contains("insufficient funds"))
        );
    }

    #[test]
    fn a_node_that_hangs_up_after_the_request_leaves_the_outcome_unknown() {
        let (outcome, _) = broadcast_to(vec![
            vec![Reply::HangUp, NOT_FOUND],
            vec![NO_FUNDS, NOT_FOUND],
        ]);

        assert_eq!(
            outcome,
            Err(Error::Unconfirmed(format!(
                "https://bscscan.com/tx/{BSC_NATIVE_HASH}"
            )))
        );
    }

    #[test]
    fn a_node_answering_another_hash_is_not_believed() {
        let (outcome, reported) = broadcast_to(vec![vec![OTHER_HASH, NOT_FOUND]]);

        assert!(matches!(outcome, Err(Error::Unconfirmed(_))), "{outcome:?}");
        assert_eq!(reported.len(), 1);
        assert!(reported[0].contains(BSC_NATIVE_HASH), "{reported:?}");
        assert!(reported[0].contains(&format!("0x{}", "11".repeat(32))));
    }

    #[test]
    fn a_failed_lookup_also_leaves_the_outcome_unknown() {
        let (outcome, _) = broadcast_to(vec![vec![NO_FUNDS, Reply::HangUp]]);

        assert!(matches!(outcome, Err(Error::Unconfirmed(_))), "{outcome:?}");
    }

    #[test]
    fn only_failures_after_which_the_node_may_hold_the_transaction_leave_doubt() {
        assert!(leaves_doubt(&Error::NoAnswer { millis: 5000 }));
        assert!(leaves_doubt(&Error::Transport("reset".to_owned())));
        assert!(!leaves_doubt(&Error::NodeRefused(
            "nonce too low".to_owned()
        )));
        assert!(!leaves_doubt(&Error::Runtime("no runtime".to_owned())));
    }
}
