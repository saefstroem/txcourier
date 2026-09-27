use rp2350wallet_protocol::ledger::STATUS_DENIED;
use rp2350wallet_protocol::transaction::Unsigned;
use rp2350wallet_protocol::transfer::{Transfer, classify};
use rp2350wallet_protocol::wire::{Refusal, TRANSACTION_CAPACITY};
use rp2350wallet_protocol::{BUS_SAMPLES, ledger};

use crate::error::Error;
use crate::host::Bus;
use crate::host::Error as HostError;
use crate::host::sign;
use crate::result::Result;
use crate::storage::Vault;

// Not fixed by any specification: 100 ms between attachment polls.
const POLL_INTERVAL_US: u64 = 100_000;
// Not fixed by any specification: three consecutive attached polls, at least 200 ms, before
// the first attempt.
const SETTLE_POLLS: u8 = 3;
// Not fixed by any specification: 2 s before the first retry after a failed attempt. Not
// applied after STATUS_DENIED, which waits for a physical detach instead.
const BACKOFF_FIRST_US: u64 = 2_000_000;
// Not fixed by any specification: the wait doubles from 2 s and is capped at 30 s.
const BACKOFF_LAST_US: u64 = 30_000_000;

/// Attempts to sign via ledger if its connected
/// and we have the correct state
pub struct SigningAttempts {
    settled: u8,
    next_poll_at: u64,
    backoff_us: u64,
    awaiting_detach: bool,
    refusal: Option<Refusal>,
    detail: u16,
}

impl SigningAttempts {
    pub const fn new() -> Self {
        Self {
            settled: 0,
            next_poll_at: 0,
            backoff_us: BACKOFF_FIRST_US,
            awaiting_detach: false,
            refusal: None,
            detail: 0,
        }
    }

    pub const fn refusal(&self) -> Option<Refusal> {
        self.refusal
    }

    pub const fn detail(&self) -> u16 {
        self.detail
    }

    pub fn attend(&mut self, bus: &mut Bus, vault: &mut Vault, now_us: u64) {
        // If we are not waiting for signature we exit early
        if !waiting_for_a_signature(vault) {
            self.settled = 0;
            self.awaiting_detach = false;
            return;
        }

        // Dont spam too hard
        if now_us < self.next_poll_at {
            return;
        }

        // Compute next time we should poll
        self.next_poll_at = now_us + POLL_INTERVAL_US;

        // If we are not attached exit early
        if !attached(bus) {
            self.settled = 0;
            self.awaiting_detach = false;
            return;
        }

        // If we are awaiting user to detach it, we exit
        if self.awaiting_detach {
            return;
        }

        // We wait for when the ledger has been plugged in for at least three polls
        // before we start talking to it
        self.settled = self.settled.saturating_add(1);
        if self.settled < SETTLE_POLLS {
            return;
        }
        self.settled = 0;

        // Exeute the main loop
        match self.run(bus, vault) {
            Ok(()) => {
                self.backoff_us = BACKOFF_FIRST_US;
                self.refusal = None;
                self.detail = 0;
            }
            Err(error) => self.remember(error, now_us),
        }
    }

    fn run(&mut self, bus: &mut Bus, vault: &mut Vault) -> Result<()> {
        let mut unsigned = [0u8; TRANSACTION_CAPACITY];
        let unsigned_len = vault.read_pending(&mut unsigned);
        // read the unsigned transaction
        let raw = unsigned.get(..unsigned_len).unwrap_or(&[]);
        let token = token_for(raw, vault)?;

        let mut signed = [0u8; TRANSACTION_CAPACITY];

        // attempt to get the signed transaction
        let signed_len = sign(bus, raw, token, &mut signed)?;

        // store the signature: tombstone pending, then append signed
        vault.replace_pending_with_signed(signed.get(..signed_len).unwrap_or(&[]))?;

        Ok(())
    }

    fn remember(&mut self, error: Error, now_us: u64) {
        self.refusal = Some(refusal_for(error));
        self.detail = status_of(error);

        // If the ledger denied we stop retrying this one and await for it to be 
        // unplugged we wont retry again
        if self.detail == STATUS_DENIED {
            self.awaiting_detach = true;
            return;
        }

        self.next_poll_at = now_us + self.backoff_us;
        self.backoff_us = (self.backoff_us * 2).min(BACKOFF_LAST_US);
    }
}

fn token_for<'a>(raw: &[u8], vault: &'a Vault) -> Result<&'a [u8]> {
    let Ok(unsigned) = Unsigned::decode(raw) else {
        return Err(Error::Host(HostError::NoTokenDescriptor));
    };

    let (chain_id, contract) = match classify(&unsigned) {
        Ok(Transfer::Native { .. }) => return Ok(&[]),
        Ok(Transfer::Token {
            chain_id, contract, ..
        }) => (chain_id, contract),
        Err(_) => return Err(Error::Host(HostError::NoTokenDescriptor)),
    };

    vault
        .tokens()
        .and_then(|tokens| tokens.descriptor(chain_id, contract))
        .ok_or(Error::Host(HostError::NoTokenDescriptor))
}

fn waiting_for_a_signature(vault: &Vault) -> bool {
    vault.occupancy().may_store_signed().is_ok()
}

fn attached(bus: &Bus) -> bool {
    let (data_plus_high, data_minus_high) = bus.sample();
    data_plus_high == BUS_SAMPLES && data_minus_high == 0
}

fn refusal_for(error: Error) -> Refusal {
    match error {
        Error::Host(error) => Refusal::from(error),
        Error::Storage(error) => Refusal::from(error),
        _ => Refusal::LedgerFailed,
    }
}

const fn status_of(error: Error) -> u16 {
    match error {
        Error::Host(HostError::Ledger(ledger::Error::Status(status))) => status,
        _ => 0,
    }
}
