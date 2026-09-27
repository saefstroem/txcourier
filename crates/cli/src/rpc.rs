use std::future::Future;
use std::time::Duration;

use alloy::providers::RootProvider;
use alloy::transports::{TransportError, TransportResult};
use tokio::runtime::Builder;
use tokio::time::timeout;

use crate::error::Error;
use crate::result::Result;

const ANSWER_WITHIN: Duration = Duration::from_secs(5);
const PRINTABLE_MAX: usize = 300;
const BIDI_CONTROLS: [char; 9] = [
    '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}', '\u{2068}',
    '\u{2069}',
];

pub fn ask<T, Request, Answer>(url: &str, request: Request) -> Result<T>
where
    Request: FnOnce(RootProvider) -> Answer,
    Answer: Future<Output = TransportResult<T>>,
{
    ask_within(ANSWER_WITHIN, url, request)
}

fn ask_within<T, Request, Answer>(limit: Duration, url: &str, request: Request) -> Result<T>
where
    Request: FnOnce(RootProvider) -> Answer,
    Answer: Future<Output = TransportResult<T>>,
{
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Error::Runtime(printable(&error.to_string())))?;

    let answer = runtime.block_on(async {
        timeout(limit, async {
            let node = RootProvider::connect(url).await?;
            request(node).await
        })
        .await
    });

    match answer {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(failure(&error)),
        Err(_) => Err(Error::NoAnswer {
            millis: limit.as_millis(),
        }),
    }
}

fn failure(error: &TransportError) -> Error {
    match error.as_error_resp() {
        Some(payload) => Error::NodeRefused(printable(&format!(
            "{} (code {})",
            payload.message, payload.code
        ))),
        None => Error::Transport(printable(&error.to_string())),
    }
}

fn printable(text: &str) -> String {
    text.chars()
        .filter(|glyph| !glyph.is_control() && !BIDI_CONTROLS.contains(glyph))
        .take(PRINTABLE_MAX)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::time::Duration;

    use alloy::providers::Provider;

    use super::{PRINTABLE_MAX, ask, ask_within, printable};
    use crate::error::Error;
    use crate::fake_node::{Reply, serve};

    #[test]
    fn a_node_answer_comes_back_as_a_value() {
        let (url, requests) = serve(vec![Reply::Answer(
            r#"{"jsonrpc":"2.0","id":ID,"result":"0x3b9aca00"}"#,
        )]);

        let fee = ask(&url, |node| async move {
            node.get_max_priority_fee_per_gas().await
        });

        assert_eq!(fee, Ok(1_000_000_000));
        assert!(
            requests
                .recv()
                .expect("one request")
                .contains("eth_maxPriorityFeePerGas")
        );
    }

    #[test]
    fn a_node_error_is_a_refusal_carrying_its_message_and_code() {
        let (url, _requests) = serve(vec![Reply::Answer(
            r#"{"jsonrpc":"2.0","id":ID,"error":{"code":-32000,"message":"nonce too low"}}"#,
        )]);

        let answer = ask(&url, |node| async move {
            node.get_max_priority_fee_per_gas().await
        });

        assert_eq!(
            answer,
            Err(Error::NodeRefused("nonce too low (code -32000)".to_owned()))
        );
    }

    #[test]
    fn escape_sequences_from_a_node_never_reach_the_terminal() {
        let (url, _requests) = serve(vec![Reply::Answer(
            r#"{"jsonrpc":"2.0","id":ID,"error":{"code":-32000,"message":"\u001b[1A\u001b[2K:) published\u202e\r\n"}}"#,
        )]);

        let answer = ask(&url, |node| async move {
            node.get_max_priority_fee_per_gas().await
        });

        assert_eq!(
            answer,
            Err(Error::NodeRefused(
                "[1A[2K:) published (code -32000)".to_owned()
            ))
        );
    }

    #[test]
    fn printable_text_is_capped() {
        assert_eq!(printable(&"a".repeat(1000)).len(), PRINTABLE_MAX);
        assert_eq!(printable("plain text"), "plain text");
        assert_eq!(printable("\u{1b}]52;c;cGF5\u{7}x"), "]52;c;cGF5x");
    }

    #[test]
    fn a_node_that_hangs_up_is_a_transport_failure() {
        let (url, _requests) = serve(vec![Reply::HangUp]);

        let answer = ask(&url, |node| async move {
            node.get_max_priority_fee_per_gas().await
        });

        assert!(matches!(answer, Err(Error::Transport(_))), "{answer:?}");
    }

    #[test]
    fn a_node_that_never_answers_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
        let url = format!("http://{}", listener.local_addr().expect("has an address"));

        let answer = ask_within(Duration::from_millis(200), &url, |node| async move {
            node.get_max_priority_fee_per_gas().await
        });

        assert_eq!(answer, Err(Error::NoAnswer { millis: 200 }));
        drop(listener);
    }

    #[test]
    fn a_node_that_refuses_the_connection_is_a_transport_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
        let url = format!("http://{}", listener.local_addr().expect("has an address"));
        drop(listener);

        let answer = ask(&url, |node| async move {
            node.get_max_priority_fee_per_gas().await
        });

        assert!(matches!(answer, Err(Error::Transport(_))), "{answer:?}");
    }

    #[test]
    fn an_address_that_is_not_a_url_is_a_transport_failure() {
        let answer = ask("not a url", |node| async move {
            node.get_max_priority_fee_per_gas().await
        });

        assert!(matches!(answer, Err(Error::Transport(_))), "{answer:?}");
    }
}
