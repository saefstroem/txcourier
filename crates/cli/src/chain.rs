use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chain {
    pub id: u64,
    pub name: &'static str,
    pub symbol: &'static str,
    pub rpc: &'static [&'static str],
    pub explorer: &'static str,
}

pub const KNOWN: [Chain; 2] = [
    Chain {
        id: 1,
        name: "Ethereum",
        symbol: "ETH",
        rpc: &[
            "https://1rpc.io/eth",
            "https://eth.drpc.org",
            "https://rpc.flashbots.net",
        ],
        explorer: "https://etherscan.io/tx/",
    },
    Chain {
        id: 56,
        name: "BNB Smart Chain",
        symbol: "BNB",
        rpc: &[
            "https://1rpc.io/bnb",
            "https://bsc-dataseed.bnbchain.org",
            "https://bsc-rpc.publicnode.com",
        ],
        explorer: "https://bscscan.com/tx/",
    },
];

const UNKNOWN_SYMBOL: &str = "native units";

pub fn named(id: u64) -> Option<Chain> {
    KNOWN.into_iter().find(|chain| chain.id == id)
}

pub fn describe(id: u64) -> String {
    match named(id) {
        Some(chain) => chain.to_string(),
        None => format!("chain {id}"),
    }
}

pub fn symbol(id: u64) -> &'static str {
    named(id).map_or(UNKNOWN_SYMBOL, |chain| chain.symbol)
}

impl Display for Chain {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{} ({})", self.name, self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::{KNOWN, describe, named, symbol};

    #[test]
    fn no_two_known_chains_share_an_id() {
        for (index, chain) in KNOWN.iter().enumerate() {
            assert!(
                !KNOWN[index + 1..].iter().any(|other| other.id == chain.id),
                "chain id {} is listed twice",
                chain.id
            );
        }
    }

    #[test]
    fn only_ethereum_and_bnb_smart_chain_are_known() {
        let ids: Vec<u64> = KNOWN.iter().map(|chain| chain.id).collect();
        assert_eq!(ids, [1, 56]);
    }

    #[test]
    fn every_known_chain_has_https_endpoints_to_ask_for_fees() {
        for chain in KNOWN {
            assert!(!chain.rpc.is_empty(), "{} has no endpoint", chain.name);
            for url in chain.rpc {
                assert!(url.starts_with("https://"), "{url} is not https");
            }
        }
    }

    #[test]
    fn every_known_chain_links_a_transaction_to_its_explorer() {
        for chain in KNOWN {
            assert!(chain.explorer.starts_with("https://"), "{}", chain.name);
            assert!(chain.explorer.ends_with("/tx/"), "{}", chain.name);
        }
    }

    #[test]
    fn a_known_chain_is_found_by_its_id() {
        assert_eq!(named(1).map(|chain| chain.name), Some("Ethereum"));
        assert_eq!(named(56).map(|chain| chain.symbol), Some("BNB"));
        assert_eq!(named(8453), None);
    }

    #[test]
    fn an_unknown_chain_is_still_described() {
        assert_eq!(describe(56), "BNB Smart Chain (56)");
        assert_eq!(describe(8453), "chain 8453");
        assert_eq!(symbol(8453), "native units");
    }
}
