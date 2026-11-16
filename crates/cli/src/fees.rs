use alloy::eips::BlockNumberOrTag;
use alloy::providers::Provider;

use crate::error::Error;
use crate::result::Result;
use crate::rpc::ask;

const HISTORY_BLOCKS: u64 = 1;
const BASE_FEE_HEADROOM: u128 = 2;
const PRIORITY_FEE_PERCENT: u128 = 150;
const WHOLE_PERCENT: u128 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Suggestion {
    pub base_fee: u128,
    pub quoted_priority_fee: u128,
}

impl Suggestion {
    pub fn priority_fee(&self) -> u128 {
        self.quoted_priority_fee
            .checked_mul(PRIORITY_FEE_PERCENT)
            .map_or(self.quoted_priority_fee, |scaled| {
                scaled.div_ceil(WHOLE_PERCENT)
            })
    }

    pub const fn max_fee(&self, priority_fee: u128) -> Option<u128> {
        match self.base_fee.checked_mul(BASE_FEE_HEADROOM) {
            Some(headroom) => headroom.checked_add(priority_fee),
            None => None,
        }
    }
}

pub fn suggest(url: &str) -> Result<Suggestion> {
    let (quoted_priority_fee, base_fee) = ask(url, |node| async move {
        let quoted_priority_fee = node.get_max_priority_fee_per_gas().await?;
        let history = node
            .get_fee_history(HISTORY_BLOCKS, BlockNumberOrTag::Latest, &[])
            .await?;
        Ok((quoted_priority_fee, history.next_block_base_fee()))
    })?;

    let Some(base_fee) = base_fee else {
        return Err(Error::NoBaseFee);
    };

    Ok(Suggestion {
        base_fee,
        quoted_priority_fee,
    })
}

#[cfg(test)]
mod tests {
    use super::{Suggestion, suggest};
    use crate::amount::{GWEI_DECIMALS, format};
    use crate::error::Error;
    use crate::fake_node::{Reply, serve};

    const ONE_GWEI: u128 = 1_000_000_000;
    const LARGEST_RAISABLE: u128 = u128::MAX / 150;
    const BASE_FEES: [u128; 4] = [0, ONE_GWEI, 7 * ONE_GWEI, 300 * ONE_GWEI];
    const ONE_GWEI_TIP: Reply = Reply::Answer(r#"{"jsonrpc":"2.0","id":ID,"result":"0x3b9aca00"}"#);
    const ETHEREUM_HISTORY: Reply = Reply::Answer(
        r#"{"jsonrpc":"2.0","id":ID,"result":{"baseFeePerGas":["0x1c9e8b53d","0x1c15dcc0a"],"gasUsedRatio":[0.4253771333333333],"oldestBlock":"0x1"}}"#,
    );
    const BSC_HISTORY: Reply = Reply::Answer(
        r#"{"jsonrpc":"2.0","id":ID,"result":{"oldestBlock":"0x76974ac","baseFeePerGas":["0x0","0x0"],"gasUsedRatio":[0.39962674285714284]}}"#,
    );
    const EMPTY_HISTORY: Reply = Reply::Answer(
        r#"{"jsonrpc":"2.0","id":ID,"result":{"oldestBlock":"0x1","baseFeePerGas":[],"gasUsedRatio":[]}}"#,
    );

    #[test]
    fn the_suggestion_is_the_nodes_tip_and_the_next_blocks_base_fee() {
        let (url, requests) = serve(vec![ONE_GWEI_TIP, ETHEREUM_HISTORY]);

        assert_eq!(
            suggest(&url),
            Ok(Suggestion {
                base_fee: 0x1c15dcc0a,
                quoted_priority_fee: ONE_GWEI,
            })
        );
        assert!(
            requests
                .recv()
                .expect("a tip request")
                .contains("eth_maxPriorityFeePerGas")
        );
        let history = requests.recv().expect("a history request");
        assert!(history.contains("eth_feeHistory"), "{history}");
        assert!(history.contains(r#"["0x1","latest",[]]"#), "{history}");
    }

    #[test]
    fn bnb_smart_chain_reports_a_zero_base_fee() {
        let (url, _requests) = serve(vec![ONE_GWEI_TIP, BSC_HISTORY]);

        assert_eq!(suggest(&url).map(|suggestion| suggestion.base_fee), Ok(0));
    }

    #[test]
    fn a_history_without_base_fees_suggests_nothing() {
        let (url, _requests) = serve(vec![ONE_GWEI_TIP, EMPTY_HISTORY]);

        assert_eq!(suggest(&url), Err(Error::NoBaseFee));
    }

    #[test]
    fn the_max_fee_is_twice_the_base_fee_plus_the_tip() {
        let suggestion = Suggestion {
            base_fee: 7 * ONE_GWEI,
            quoted_priority_fee: ONE_GWEI,
        };
        assert_eq!(suggestion.max_fee(ONE_GWEI), Some(15 * ONE_GWEI));
        assert_eq!(suggestion.max_fee(3 * ONE_GWEI), Some(17 * ONE_GWEI));
    }

    #[test]
    fn a_zero_base_fee_leaves_only_the_tip() {
        let suggestion = Suggestion {
            base_fee: 0,
            quoted_priority_fee: ONE_GWEI / 20,
        };
        assert_eq!(suggestion.max_fee(ONE_GWEI / 20), Some(ONE_GWEI / 20));
    }

    #[test]
    fn a_max_fee_too_large_to_hold_suggests_nothing() {
        let suggestion = Suggestion {
            base_fee: u128::MAX,
            quoted_priority_fee: 0,
        };
        assert_eq!(suggestion.max_fee(0), None);
    }

    fn quoting(quoted_priority_fee: u128) -> Suggestion {
        Suggestion {
            base_fee: 7 * ONE_GWEI,
            quoted_priority_fee,
        }
    }

    #[test]
    fn the_suggested_tip_is_half_again_the_nodes_quote() {
        assert_eq!(quoting(ONE_GWEI).priority_fee(), 3 * ONE_GWEI / 2);
        assert_eq!(quoting(1_000_000).priority_fee(), 1_500_000);
        assert_eq!(quoting(ONE_GWEI / 20).priority_fee(), 75_000_000);
        assert_eq!(quoting(0).priority_fee(), 0);
    }

    #[test]
    fn a_tip_that_does_not_halve_evenly_rounds_up_to_the_next_wei() {
        assert_eq!(quoting(1).priority_fee(), 2);
        assert_eq!(quoting(3).priority_fee(), 5);
        assert_eq!(quoting(4).priority_fee(), 6);
    }

    #[test]
    fn a_quote_too_large_to_raise_is_kept_as_it_is() {
        assert_eq!(quoting(u128::MAX).priority_fee(), u128::MAX);
    }

    #[test]
    fn the_suggested_max_fee_carries_the_raised_tip() {
        let suggestion = quoting(ONE_GWEI);
        assert_eq!(
            suggestion.max_fee(suggestion.priority_fee()),
            Some(15 * ONE_GWEI + ONE_GWEI / 2)
        );
    }

    #[test]
    fn the_nodes_quote_is_raised_once_it_is_fetched() {
        let (url, _requests) = serve(vec![ONE_GWEI_TIP, ETHEREUM_HISTORY]);

        assert_eq!(
            suggest(&url).map(|suggestion| suggestion.priority_fee()),
            Ok(3 * ONE_GWEI / 2)
        );
    }

    fn half_again(quote: u128) -> u128 {
        quote + quote.div_ceil(2)
    }

    fn sample_quotes() -> Vec<u128> {
        let mut quotes: Vec<u128> = (0..=10_000).collect();
        quotes.extend((0..=38).map(|exponent| 10u128.pow(exponent)));
        quotes.extend((0..128).map(|bit| 1u128 << bit));
        quotes.extend((1..128).map(|bit| (1u128 << bit) - 1));
        quotes.extend([
            1_000_000,
            50_000_000,
            1_234_567_891,
            LARGEST_RAISABLE - 1,
            LARGEST_RAISABLE,
        ]);

        let mut state: u128 = 0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C834;
        for _ in 0..20_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            quotes.push(state >> (state % 128));
        }

        quotes.retain(|quote| *quote <= LARGEST_RAISABLE);
        quotes
    }

    #[test]
    fn every_quote_is_raised_to_exactly_half_again_rounded_up() {
        for quote in sample_quotes() {
            assert_eq!(quoting(quote).priority_fee(), half_again(quote), "{quote}");
        }
    }

    #[test]
    fn the_raised_tip_is_the_smallest_whole_wei_at_or_above_one_and_a_half_times_the_quote() {
        for quote in sample_quotes() {
            let raised = quoting(quote).priority_fee();

            assert!(
                2 * raised >= 3 * quote,
                "{quote} raised to {raised} is short"
            );
            if raised > 0 {
                assert!(
                    2 * (raised - 1) < 3 * quote,
                    "{quote} raised to {raised} overshoots"
                );
            }
        }
    }

    #[test]
    fn the_raised_tip_is_never_more_than_one_wei_above_one_and_a_half_times_the_quote() {
        for quote in sample_quotes() {
            let raised = quoting(quote).priority_fee();
            let exact_doubled = 3 * quote;

            assert!(
                2 * raised - exact_doubled <= 1,
                "{quote} raised to {raised}"
            );
            assert_eq!(2 * raised == exact_doubled, quote % 2 == 0, "{quote}");
        }
    }

    #[test]
    fn the_max_fee_rises_by_exactly_the_extra_tip() {
        for base_fee in BASE_FEES {
            for quote in sample_quotes().into_iter().step_by(97) {
                let suggestion = Suggestion {
                    base_fee,
                    quoted_priority_fee: quote,
                };
                let raised = suggestion.priority_fee();

                assert_eq!(
                    suggestion.max_fee(raised),
                    Some(2 * base_fee + half_again(quote)),
                    "base {base_fee}, quote {quote}"
                );
                assert_eq!(
                    suggestion
                        .max_fee(raised)
                        .zip(suggestion.max_fee(quote))
                        .map(|(raised_cap, quoted_cap)| raised_cap - quoted_cap),
                    Some(quote.div_ceil(2)),
                    "base {base_fee}, quote {quote}"
                );
            }
        }
    }

    #[test]
    fn quotes_from_the_edge_of_the_range_up_are_kept_rather_than_overflowing() {
        assert_eq!(
            quoting(LARGEST_RAISABLE).priority_fee(),
            half_again(LARGEST_RAISABLE)
        );
        assert_eq!(
            quoting(LARGEST_RAISABLE + 1).priority_fee(),
            LARGEST_RAISABLE + 1
        );
    }

    #[test]
    fn real_quotes_read_as_half_again_in_gwei() {
        for (quote, quoted_gwei, raised_gwei) in [
            (1_000_000, "0.001", "0.0015"),
            (50_000_000, "0.05", "0.075"),
            (ONE_GWEI, "1", "1.5"),
            (2 * ONE_GWEI, "2", "3"),
            (1_234_567_891, "1.234567891", "1.851851837"),
        ] {
            let raised = quoting(quote).priority_fee();
            assert_eq!(format(quote, GWEI_DECIMALS), quoted_gwei);
            assert_eq!(format(raised, GWEI_DECIMALS), raised_gwei, "{quote}");
        }
    }

    #[test]
    fn every_quote_a_node_sends_comes_back_raised_by_half() {
        for quote in [
            0,
            1,
            3,
            1_000_000,
            50_000_000,
            1_234_567_891,
            100 * ONE_GWEI,
        ] {
            let reply: &'static str = Box::leak(
                format!(r#"{{"jsonrpc":"2.0","id":ID,"result":"{quote:#x}"}}"#).into_boxed_str(),
            );
            let (url, _requests) = serve(vec![Reply::Answer(reply), ETHEREUM_HISTORY]);

            let suggestion = suggest(&url).expect("the node answers");
            assert_eq!(suggestion.quoted_priority_fee, quote);
            assert_eq!(suggestion.priority_fee(), half_again(quote), "{quote}");
        }
    }
}
