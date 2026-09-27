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
mod result;
mod tokens;

use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use rp2350wallet_protocol::DeviceState;
use rp2350wallet_protocol::token::encode as encode_tokens;
use rp2350wallet_protocol::wire::{
    PROVISIONING_CAPACITY, Provisioning, Request, Response, Slot, Transaction,
};

use crate::error::{Error, explain};
use crate::result::Result;

#[derive(Parser)]
#[command(version, about = "Talk to the RP2350 air-gapped transaction courier")]
struct Arguments {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Serial device to use instead of searching /dev"
    )]
    port: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Report what the courier is currently holding")]
    Status,

    #[command(about = "Replace the courier's table of Ledger token descriptors")]
    Provision {
        #[arg(
            value_name = "PATH",
            help = "File of <chain-id> <contract-address> <ledger-descriptor-hex> lines"
        )]
        path: PathBuf,
    },

    #[command(about = "Store an unsigned transaction, if its shape is one the courier signs")]
    Push {
        #[arg(value_name = "HEX", help = "The unsigned EIP-1559 transaction, as hex")]
        transaction: String,
    },

    #[command(about = "Print the transaction held in one of the two slots")]
    Slot { which: Which },

    #[command(about = "Discard the transaction held in one of the two slots")]
    Clear { which: Which },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Which {
    Pending,
    Signed,
}

impl Which {
    fn slot(self) -> Slot {
        match self {
            Self::Pending => Slot::Pending,
            Self::Signed => Slot::Signed,
        }
    }
}

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
    let arguments = Arguments::parse();

    let path = match arguments.port {
        Some(path) => path,
        None => device::locate()?,
    };

    match arguments.command {
        Command::Status => status(&path),
        Command::Provision { path: table } => provision(&path, &table),
        Command::Push { transaction } => push(&path, &transaction),
        Command::Slot { which } => slot(&path, which),
        Command::Clear { which } => clear(&path, which),
    }
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

    println!("port    {}", path.display());

    match DeviceState::from_byte(state) {
        Ok(recognised) => println!("state   {}", recognised.name()),
        Err(_) => println!("state   fault ({state:#04x})"),
    }

    println!("accepts an EIP-1559 ETH or ERC-20 transfer to any recipient, and nothing else");

    let tokens = if provisioned {
        "a token table is stored: an ERC-20 signs only if its contract is listed, and the Ledger names it on screen"
    } else {
        "no token table: ETH transfers still sign, but every ERC-20 transfer is refused at signing"
    };
    println!("tokens  {tokens}");

    match refusal {
        None => println!("signing no failed attempt recorded"),
        Some(refusal) => println!("signing {}", explain(refusal, detail)),
    }

    Ok(())
}

fn provision(path: &Path, table: &Path) -> Result<()> {
    let text = read_to_string(table).map_err(|source| Error::Unreadable {
        path: table.display().to_string(),
        reason: source.to_string(),
    })?;

    let tokens = tokens::parse(&text)?;

    let mut blob = [0u8; PROVISIONING_CAPACITY];
    let len = encode_tokens(&tokens, &mut blob)?;
    let payload = Provisioning::from_slice(blob.get(..len).unwrap_or(&[]))?;

    acknowledged(path, Request::Provision(payload))?;

    let count = tokens.len();
    println!("stored {count} token descriptors on {}", path.display());

    Ok(())
}

fn push(path: &Path, transaction: &str) -> Result<()> {
    let Some(bytes) = hex::parse(transaction.trim()) else {
        return Err(Error::NotHex(transaction.to_owned()));
    };

    acknowledged(path, Request::Put(Transaction::from_slice(&bytes)?))?;

    let count = bytes.len();
    println!("the courier accepted {count} bytes and is holding them");

    Ok(())
}

fn slot(path: &Path, which: Which) -> Result<()> {
    let slot = which.slot();

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

fn clear(path: &Path, which: Which) -> Result<()> {
    let slot = which.slot();
    acknowledged(path, Request::Clear(slot))?;

    println!("the {} slot is now empty", slot.name());

    Ok(())
}

fn acknowledged(path: &Path, request: Request) -> Result<()> {
    match device::ask(path, request)? {
        Response::Acknowledged => Ok(()),
        Response::Refused { refusal, detail } => Err(Error::Refused { refusal, detail }),
        other => Err(device::Error::UnexpectedResponse(other.opcode()).into()),
    }
}
