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

mod device;
mod error;
mod hex;
mod menu;
mod result;
mod tokens;

use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inquire::error::InquireResult;
use inquire::validator::Validation;
use inquire::{Confirm, InquireError, Select, Text};
use rp2350wallet_protocol::DeviceState;
use rp2350wallet_protocol::token::encode as encode_tokens;
use rp2350wallet_protocol::wire::{
    PROVISIONING_CAPACITY, Provisioning, Request, Response, Slot, TRANSACTION_CAPACITY, Transaction,
};

use crate::error::{Error, explain};
use crate::menu::Action;
use crate::result::Result;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rp2350wallet: {error}");
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

        match perform(&path, action) {
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
            let chosen =
                answer(Select::new("Several couriers (or smth else) are connected. Which one?", names).prompt())?;
            Ok(chosen.map(PathBuf::from))
        }
    }
}

fn perform(path: &Path, action: Action) -> Result<()> {
    match action {
        Action::Status => status(path),
        Action::Push => push(path),
        Action::Show(slot) => show(path, slot),
        Action::Clear(slot) => clear(path, slot),
        Action::Provision => provision(path),
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

fn push(path: &Path) -> Result<()> {
    let Some(typed) = answer(
        Text::new("Unsigned transaction (hex):")
            .with_help_message("an EIP-1559 transaction; spaces and a leading 0x are fine")
            .with_validator(|input: &str| {
                Ok(match hex::parse(input) {
                    None => Validation::Invalid("not an even-length hex string".into()),
                    Some(bytes) if bytes.len() > TRANSACTION_CAPACITY => Validation::Invalid(
                        format!(
                            "{} bytes; a slot holds at most {TRANSACTION_CAPACITY}",
                            bytes.len()
                        )
                        .into(),
                    ),
                    Some(_) => Validation::Valid,
                })
            })
            .prompt(),
    )?
    else {
        return Ok(());
    };

    let Some(bytes) = hex::parse(&typed) else {
        return Err(Error::NotHex(typed));
    };

    acknowledged(path, Request::Put(Transaction::from_slice(&bytes)?))?;

    let count = bytes.len();
    println!("✓ the courier accepted {count} bytes and is holding them");

    Ok(())
}

fn show(path: &Path, slot: Slot) -> Result<()> {
    let transaction = match device::ask(path, Request::Get(slot))? {
        Response::Slot { transaction } => transaction,
        Response::Refused { refusal, detail } => return Err(Error::Refused { refusal, detail }),
        other => return Err(device::Error::UnexpectedResponse(other.opcode()).into()),
    };

    if transaction.as_slice().is_empty() {
        println!("the {} slot is empty", slot.name());
    } else {
        println!("{}", hex::of(transaction.as_slice()));
    }

    Ok(())
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

fn provision(path: &Path) -> Result<()> {
    let Some(typed) = answer(
        Text::new("Token table file:")
            .with_help_message("lines of <chain-id> <contract-address> <ledger-descriptor-hex>")
            .with_validator(|input: &str| {
                Ok(if Path::new(input).is_file() {
                    Validation::Valid
                } else {
                    Validation::Invalid("no such file".into())
                })
            })
            .prompt(),
    )?
    else {
        return Ok(());
    };

    let table = PathBuf::from(typed);
    let text = read_to_string(&table).map_err(|source| Error::Unreadable {
        path: table.display().to_string(),
        reason: source.to_string(),
    })?;

    let tokens = tokens::parse(&text)?;

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
