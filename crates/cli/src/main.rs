#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        dead_code,
        unused_must_use
    )
)]

mod amount;
mod chain;
mod device;
mod draft;
mod error;
#[cfg(test)]
mod fake_node;
mod fees;
mod hex;
mod inspect;
mod menu;
mod publish;
mod result;
mod rpc;
mod tokens;

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use alloy::primitives::keccak256;
use inquire::error::InquireResult;
use inquire::validator::Validation;
use inquire::{Confirm, CustomType, InquireError, Select, Text};
use txcourier_protocol::DeviceState;
use txcourier_protocol::policy::approve;
use txcourier_protocol::token::{Summary, Tokens, encode as encode_tokens, summarize};
use txcourier_protocol::transaction::{ADDRESS_LEN, Signed, Unsigned};
use txcourier_protocol::transfer::{Transfer, classify};
use txcourier_protocol::wire::{
    PROVISIONING_CAPACITY, Provisioning, Request, Response, Slot, TRANSACTION_CAPACITY, Transaction,
};

use crate::amount::{GWEI_DECIMALS, MAX_DECIMALS, NATIVE_DECIMALS};
use crate::draft::{Draft, Fees, Payment};
use crate::error::{Error, explain};
use crate::fees::Suggestion;
use crate::inspect::Denomination;
use crate::menu::Action;
use crate::publish::Outcome;
use crate::result::Result;

const NATIVE_GAS_LIMIT: u64 = 21_000;
const TOKEN_GAS_LIMIT: u64 = 100_000;
const MINIMUM_GAS_LIMIT: u64 = 21_000;
const ZERO_ADDRESS: [u8; ADDRESS_LEN] = [0; ADDRESS_LEN];

struct TokenChoice {
    ticker: String,
    contract: [u8; ADDRESS_LEN],
    decimals: u32,
    chain_id: u64,
}

impl From<Summary<'_>> for TokenChoice {
    fn from(summary: Summary<'_>) -> Self {
        Self {
            ticker: String::from_utf8_lossy(summary.ticker).into_owned(),
            contract: summary.contract,
            decimals: summary.decimals,
            chain_id: summary.chain_id,
        }
    }
}

impl TokenChoice {
    fn denomination(&self) -> Denomination<'_> {
        Denomination {
            symbol: &self.ticker,
            decimals: self.decimals,
        }
    }
}

impl Display for TokenChoice {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "{} on {}  {}",
            self.ticker,
            chain::describe(self.chain_id),
            hex::address(&self.contract)
        )
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("txcourier: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let path = match choose_port() {
        Ok(Some(path)) => path,
        Ok(None) | Err(Error::Interrupted) => return Ok(()),
        Err(error) => return Err(error),
    };

    println!("courier on {}\n", path.display());

    let mut last_table = None;

    loop {
        let action = match answer(
            Select::new("What would you like to do?", Action::ALL.to_vec())
                .with_page_size(Action::ALL.len())
                .prompt(),
        ) {
            Ok(Some(Action::Quit) | None) | Err(Error::Interrupted) => return Ok(()),
            Ok(Some(action)) => action,
            Err(error) => return Err(error),
        };

        match perform(&path, action, &mut last_table) {
            Ok(()) => {}
            Err(Error::Interrupted) => return Ok(()),
            Err(error) => eprintln!("x {error}"),
        }

        println!();
    }
}

fn choose_port() -> Result<Option<PathBuf>> {
    let found = device::candidates()?;

    match found.as_slice() {
        [] => Err(device::Error::NoPortFound.into()),
        [only] => Ok(Some(only.clone())),
        several => {
            let names = several
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            let chosen = answer(
                Select::new(
                    "Several couriers (or smth else) are connected. Which one?",
                    names,
                )
                .prompt(),
            )?;
            Ok(chosen.map(PathBuf::from))
        }
    }
}

fn perform(path: &Path, action: Action, last_table: &mut Option<PathBuf>) -> Result<()> {
    match action {
        Action::Status => status(path),
        Action::SendNative => send_native(path),
        Action::SendToken => send_token(path, last_table),
        Action::Show(slot) => show(path, slot, last_table),
        Action::Clear(slot) => clear(path, slot),
        Action::Provision => provision(path, last_table),
        Action::Quit => Ok(()),
    }
}

fn answer<T>(outcome: InquireResult<T>) -> Result<Option<T>> {
    match outcome {
        Ok(value) => Ok(Some(value)),
        Err(InquireError::OperationCanceled) => Ok(None),
        Err(InquireError::OperationInterrupted) => Err(Error::Interrupted),
        Err(other) => Err(Error::Prompt(other.to_string())),
    }
}

fn confirmed(question: &str) -> Result<bool> {
    Ok(answer(Confirm::new(question).with_default(false).prompt())?.unwrap_or(false))
}

fn status(path: &Path) -> Result<()> {
    let answer = device::ask(path, Request::Status)?;

    let Response::Status {
        state,
        provisioned,
        refusal,
        detail,
    } = answer
    else {
        return Err(device::Error::UnexpectedResponse(answer.opcode()).into());
    };

    match DeviceState::from_byte(state) {
        Ok(recognised) => println!("state   {}", recognised.name()),
        Err(_) => println!("state   fault ({state:#04x})"),
    }

    println!("accepts an EIP-1559 ETH or ERC-20 transfer to any recipient");

    let tokens = if provisioned {
        "a token table is stored: which means an ERC-20 signs only if its contract is listed"
    } else {
        "no token table: ERC-20 transfer is refused at signing"
    };
    println!("tokens  {tokens}");

    match refusal {
        None => println!("signing no failed attempt recorded"),
        Some(refusal) => println!("signing {}", explain(refusal, detail)),
    }

    Ok(())
}

fn send_native(path: &Path) -> Result<()> {
    let Some(chain_id) = ask_chain()? else {
        return Ok(());
    };
    let symbol = chain::symbol(chain_id);

    let Some(recipient) = ask_address("Recipient address:", None)? else {
        return Ok(());
    };
    let Some(value) = ask_amount(&format!("Amount ({symbol}):"), NATIVE_DECIMALS)? else {
        return Ok(());
    };
    let Some((nonce, fees)) = ask_nonce_and_fees(chain_id, NATIVE_GAS_LIMIT)? else {
        return Ok(());
    };

    let draft = Draft {
        chain_id,
        nonce,
        fees,
        payment: Payment::Native { recipient, value },
    };

    review_and_hand_over(path, &draft, None)
}

fn send_token(path: &Path, last_table: &mut Option<PathBuf>) -> Result<()> {
    let Some((table, tokens)) = ask_token_table(last_table.as_deref())? else {
        return Ok(());
    };
    *last_table = Some(table);

    let listed: Vec<TokenChoice> = tokens
        .iter()
        .filter_map(|descriptor| summarize(descriptor).ok())
        .filter(|summary| chain::named(summary.chain_id).is_some())
        .filter(|summary| summary.decimals <= MAX_DECIMALS)
        .map(TokenChoice::from)
        .collect();

    if listed.is_empty() {
        println!("that token table lists no tokens on Ethereum or BNB Smart Chain");
        return Ok(());
    }

    let Some(token) = answer(Select::new("Token:", listed).prompt())? else {
        return Ok(());
    };

    let Some(recipient) = ask_address("Recipient address:", Some(token.contract))? else {
        return Ok(());
    };
    let Some(units) = ask_amount(&format!("Amount ({}):", token.ticker), token.decimals)? else {
        return Ok(());
    };
    let Some((nonce, fees)) = ask_nonce_and_fees(token.chain_id, TOKEN_GAS_LIMIT)? else {
        return Ok(());
    };

    let draft = Draft {
        chain_id: token.chain_id,
        nonce,
        fees,
        payment: Payment::Token {
            contract: token.contract,
            recipient,
            amount: units,
        },
    };

    review_and_hand_over(path, &draft, Some(token.denomination()))
}

fn review_and_hand_over(path: &Path, draft: &Draft, token: Option<Denomination<'_>>) -> Result<()> {
    let mut buffer = [0u8; TRANSACTION_CAPACITY];
    let len = draft.encode(&mut buffer)?;
    let bytes = buffer.get(..len).unwrap_or(&[]);

    let unsigned = Unsigned::decode(bytes)?;
    let transfer = classify(&unsigned)?;

    println!();
    println!("{}", inspect::lines(&unsigned, &transfer, token).join("\n"));
    println!();

    approve(bytes)?;

    if !confirmed("Hand this transaction to the courier?")? {
        println!("nothing was sent to the courier");
        return Ok(());
    }

    acknowledged(path, Request::Put(Transaction::from_slice(bytes)?))?;

    println!(":) the courier is holding the transaction, plug in the Ledger to sign it");

    Ok(())
}

fn ask_chain() -> Result<Option<u64>> {
    let chains = chain::KNOWN.to_vec();
    Ok(answer(Select::new("Chain:", chains).prompt())?.map(|chain| chain.id))
}

fn ask_address(
    message: &str,
    avoid: Option<[u8; ADDRESS_LEN]>,
) -> Result<Option<[u8; ADDRESS_LEN]>> {
    let Some(typed) = answer(
        Text::new(message)
            .with_help_message("0x and 40 hex digits. make sure to double check")
            .with_validator(move |input: &str| {
                Ok(match hex::parse_address(input) {
                    None => Validation::Invalid("not a 20-byte hex address".into()),
                    Some(ZERO_ADDRESS) => {
                        Validation::Invalid("the zero address would burn what is sent".into())
                    }
                    Some(address) if avoid == Some(address) => Validation::Invalid(
                        "that is the token's own contract, and tokens sent there are usually lost"
                            .into(),
                    ),
                    Some(_) => Validation::Valid,
                })
            })
            .prompt(),
    )?
    else {
        return Ok(None);
    };

    Ok(hex::parse_address(&typed))
}

fn ask_amount(message: &str, decimals: u32) -> Result<Option<u128>> {
    let help = format!("up to {decimals} decimal places");

    let Some(typed) = answer(
        Text::new(message)
            .with_help_message(&help)
            .with_validator(move |input: &str| {
                Ok(match amount::parse(input, decimals) {
                    None => Validation::Invalid(
                        format!("not an amount with at most {decimals} decimal places").into(),
                    ),
                    Some(0) => Validation::Invalid("must be more than zero".into()),
                    Some(_) => Validation::Valid,
                })
            })
            .prompt(),
    )?
    else {
        return Ok(None);
    };

    Ok(amount::parse(&typed, decimals))
}

fn ask_nonce_and_fees(chain_id: u64, default_gas_limit: u64) -> Result<Option<(u64, Fees)>> {
    let Some(nonce) = answer(
        CustomType::<u64>::new("Nonce:")
            .with_help_message(
                "the sending account's transaction count, e.g. from a block explorer",
            )
            .with_error_message("a whole number")
            .prompt(),
    )?
    else {
        return Ok(None);
    };

    let suggestion = suggested_fees(chain_id);

    let Some(max_priority_fee) = ask_gwei(
        "Priority fee (gwei):",
        "the tip to the block producer; the suggestion is the node's quote plus 50% for faster inclusion",
        None,
        suggestion.map(|suggestion| suggestion.priority_fee()),
    )?
    else {
        return Ok(None);
    };

    let Some(max_fee) = ask_gwei(
        "Max fee (gwei):",
        "the most paid per unit of gas, tip included; the suggestion is twice the base fee plus the tip",
        Some(max_priority_fee),
        suggestion.and_then(|suggestion| suggestion.max_fee(max_priority_fee)),
    )?
    else {
        return Ok(None);
    };

    let Some(gas_limit) = answer(
        CustomType::<u64>::new("Gas limit:")
            .with_default(default_gas_limit)
            .with_help_message("gas left unused is not charged")
            .with_error_message("a whole number")
            .with_validator(|limit: &u64| {
                Ok(if *limit >= MINIMUM_GAS_LIMIT {
                    Validation::Valid
                } else {
                    Validation::Invalid(format!("at least {MINIMUM_GAS_LIMIT}").into())
                })
            })
            .prompt(),
    )?
    else {
        return Ok(None);
    };

    Ok(Some((
        nonce,
        Fees {
            max_priority_fee,
            max_fee,
            gas_limit,
        },
    )))
}

fn suggested_fees(chain_id: u64) -> Option<Suggestion> {
    let endpoints = chain::named(chain_id)?.rpc;
    let mut failure = None;

    println!("fetching current fees…");

    for url in endpoints {
        match fees::suggest(url) {
            Ok(suggestion) => {
                println!(
                    "base fee {} gwei, priority fee {} gwei, from {url}",
                    amount::format(suggestion.base_fee, GWEI_DECIMALS),
                    amount::format(suggestion.quoted_priority_fee, GWEI_DECIMALS)
                );
                return Some(suggestion);
            }
            Err(error) => failure = Some(error),
        }
    }

    if let Some(error) = failure {
        eprintln!("x could not fetch fees ({error}), enter them by hand");
    }

    None
}

fn ask_gwei(
    message: &str,
    help: &str,
    priority_fee: Option<u128>,
    suggested: Option<u128>,
) -> Result<Option<u128>> {
    let suggested = suggested.map(|fee| amount::format(fee, GWEI_DECIMALS));

    let mut prompt =
        Text::new(message)
            .with_help_message(help)
            .with_validator(move |input: &str| {
                Ok(match amount::parse(input, GWEI_DECIMALS) {
                    None => Validation::Invalid(
                        "not an amount of gwei, with at most 9 decimal places".into(),
                    ),
                    Some(0) if priority_fee.is_some() => {
                        Validation::Invalid("must be more than zero".into())
                    }
                    Some(fee) if priority_fee.is_some_and(|tip| fee < tip) => Validation::Invalid(
                        format!(
                            "at least the priority fee, {} gwei",
                            amount::format(priority_fee.unwrap_or(0), GWEI_DECIMALS)
                        )
                        .into(),
                    ),
                    Some(_) => Validation::Valid,
                })
            });
    if let Some(default) = &suggested {
        prompt = prompt.with_default(default);
    }

    let Some(typed) = answer(prompt.prompt())? else {
        return Ok(None);
    };

    Ok(amount::parse(&typed, GWEI_DECIMALS))
}

fn ask_token_table(remembered: Option<&Path>) -> Result<Option<(PathBuf, Tokens)>> {
    let remembered = remembered.map(|path| path.display().to_string());

    let mut prompt = Text::new("Token table file:")
        .with_help_message("lines of <chain-id> <contract-address> <ledger-descriptor-hex>")
        .with_validator(|input: &str| {
            Ok(if Path::new(input).is_file() {
                Validation::Valid
            } else {
                Validation::Invalid("no such file".into())
            })
        });
    if let Some(path) = &remembered {
        prompt = prompt.with_default(path);
    }

    let Some(typed) = answer(prompt.prompt())? else {
        return Ok(None);
    };

    let table = PathBuf::from(typed);
    let text = read_to_string(&table).map_err(|source| Error::Unreadable {
        path: table.display().to_string(),
        reason: source.to_string(),
    })?;

    let tokens = tokens::parse(&text)?;

    Ok(Some((table, tokens)))
}

fn show(path: &Path, slot: Slot, last_table: &mut Option<PathBuf>) -> Result<()> {
    let transaction = match device::ask(path, Request::Get(slot))? {
        Response::Slot { transaction } => transaction,
        Response::Refused { refusal, detail } => return Err(Error::Refused { refusal, detail }),
        other => return Err(device::Error::UnexpectedResponse(other.opcode()).into()),
    };
    let bytes = transaction.as_slice();

    if bytes.is_empty() {
        println!("the {} slot is empty", slot.name());
        return Ok(());
    }

    match slot {
        Slot::Pending => {
            println!("{}", hex::of(bytes));
            Ok(())
        }
        Slot::Signed => {
            println!("{}", hex::prefixed(bytes));
            inspect_and_publish(bytes, last_table)
        }
    }
}

fn inspect_and_publish(bytes: &[u8], last_table: &mut Option<PathBuf>) -> Result<()> {
    if !confirmed("Inspect it?")? {
        return Ok(());
    }

    let signed = Signed::decode(bytes)?;
    let transfer = classify(&signed.unsigned)?;
    let token = match transfer {
        Transfer::Token {
            chain_id, contract, ..
        } => listed_token(chain_id, contract, last_table)?,
        Transfer::Native { .. } => None,
    };
    let denomination = token.as_ref().map(TokenChoice::denomination);

    println!();
    println!(
        "{}",
        inspect::lines(&signed.unsigned, &transfer, denomination).join("\n")
    );
    println!("hash     {}", keccak256(bytes));
    println!();

    let chain_id = signed.unsigned.chain_id;
    let Some(chain) = chain::named(chain_id) else {
        return Err(Error::NoNode(chain_id));
    };

    if !confirmed(&format!("Publish it on {chain}? This cannot be undone."))? {
        println!("nothing was published");
        return Ok(());
    }

    println!("publishing…");

    let outcome = publish::broadcast(&chain, bytes, |url, error| {
        eprintln!("x {url}: {error}");
    })?;
    let link = format!("{}{}", chain.explorer, keccak256(bytes));

    match outcome {
        Outcome::Accepted => println!(":) published, {link}"),
        Outcome::AlreadyKnown => println!(":) the network already has it, {link}"),
    }

    Ok(())
}

fn listed_token(
    chain_id: u64,
    contract: &[u8; ADDRESS_LEN],
    last_table: &mut Option<PathBuf>,
) -> Result<Option<TokenChoice>> {
    let Some((table, tokens)) = ask_token_table(last_table.as_deref())? else {
        return Ok(None);
    };

    let listed = tokens
        .descriptor(chain_id, contract)
        .and_then(|descriptor| summarize(descriptor).ok())
        .map(TokenChoice::from);

    if listed.is_none() {
        eprintln!(
            "x {} does not list the token {} on {}",
            table.display(),
            hex::address(contract),
            chain::describe(chain_id)
        );
    }

    *last_table = Some(table);

    Ok(listed)
}

fn clear(path: &Path, slot: Slot) -> Result<()> {
    let question = format!("Discard the {} transaction?", slot.name());
    if !confirmed(&question)? {
        println!("nothing was cleared");
        return Ok(());
    }

    acknowledged(path, Request::Clear(slot))?;

    println!(":) the {} slot is now empty", slot.name());

    Ok(())
}

fn provision(path: &Path, last_table: &mut Option<PathBuf>) -> Result<()> {
    let Some((table, tokens)) = ask_token_table(last_table.as_deref())? else {
        return Ok(());
    };
    *last_table = Some(table);

    let count = tokens.len();
    let question = format!("Replace the courier's token table with these {count} descriptors?");
    if !confirmed(&question)? {
        println!("the token table was left as it was");
        return Ok(());
    }

    let mut blob = [0u8; PROVISIONING_CAPACITY];
    let len = encode_tokens(&tokens, &mut blob)?;
    let payload = Provisioning::from_slice(blob.get(..len).unwrap_or(&[]))?;

    acknowledged(path, Request::Provision(payload))?;

    println!(":) stored {count} token descriptors");

    Ok(())
}

fn acknowledged(path: &Path, request: Request) -> Result<()> {
    match device::ask(path, request)? {
        Response::Acknowledged => Ok(()),
        Response::Refused { refusal, detail } => Err(Error::Refused { refusal, detail }),
        other => Err(device::Error::UnexpectedResponse(other.opcode()).into()),
    }
}
