use txcourier_protocol::ledger::{
    CHUNK_MAX, CLASS, Chunks, DEFAULT_ACCOUNT, INSTRUCTION_SIGN, INSTRUCTION_TOKEN_INFO,
    REPORT_LEN, Reassembly, Reports, STATUS_LEN, command, require_ok, signature,
};
use txcourier_protocol::transaction::{Unsigned, encode_signed};
use txcourier_protocol::wire::Packet;

use crate::host::bus::Bus;
use crate::host::device;
use crate::host::device::Ledger;
use crate::host::error::Error;
use crate::host::result::Result;
use crate::host::transport::{Endpoint, Progress};

// Not fixed by any specification: scratch for the configuration descriptor read during
// enumeration. The Nano S Plus descriptor is 96 bytes.
const DESCRIPTOR_CAPACITY: usize = 255;
// A five-byte APDU header followed by one data chunk of CHUNK_MAX.
const APDU_MAX: usize = 5 + CHUNK_MAX;
// 255 bytes of response data followed by the two status bytes of ISO 7816-4.
const RESPONSE_MAX: usize = 255 + STATUS_LEN;
// app-ethereum ethapp.adoc: INS 0x04 with P2 = 00 starts the signing flow, and INS 0x0A
// requires P2 = 00.
const SECOND_PARAMETER: u8 = 0x00;
// app-ethereum ethapp.adoc: INS 0x0A requires P1 = 00.
const NO_PARAMETER: u8 = 0x00;

// Not fixed by any specification: 2000 frames of 1 ms, about 2 s, for a reply that needs no
// button press.
const BRISK_FRAMES: u32 = 2_000;
// Not fixed by any specification: 300 s for the final chunk, which waits on a person at the
// Ledger screen.
const PATIENT_FRAMES: u32 = 300_000;

pub fn sign(bus: &mut Bus, unsigned: &[u8], token: &[u8], out: &mut [u8]) -> Result<usize> {
    // create containers for the answer
    let mut reply = [0u8; Packet::CAPACITY];
    let mut scratch = [0u8; DESCRIPTOR_CAPACITY];

    // create a ledger instance by attaching to it logically
    let mut ledger = device::attach(bus, &mut reply, &mut scratch)?;

    // if we are requesting an erc20 transfer we need to provide the token
    if !token.is_empty() {
        provide_token(bus, &mut ledger, token)?;
    }

    // Compute a bunch of chunks from the transactions, this is because the
    // data cap for APDU tx is 255 bytes
    let mut chunks = Chunks::new(&DEFAULT_ACCOUNT, unsigned);
    let mut data = [0u8; CHUNK_MAX];
    let mut response = [0u8; RESPONSE_MAX];
    let mut answered = 0;

    // for each chunk
    while let Some(step) = chunks.next_into(&mut data) {
        let (marker, len) = step?;
        let Some(body) = data.get(..len) else {
            return Err(Error::ReplyTooLong);
        };
        // transmit the data
        let mut apdu = [0u8; APDU_MAX];
        let apdu_len = command(
            CLASS,
            INSTRUCTION_SIGN,
            marker,
            SECOND_PARAMETER,
            body,
            &mut apdu,
        )?;
        let Some(apdu) = apdu.get(..apdu_len) else {
            return Err(Error::ReplyTooLong);
        };

        let patience = if chunks.finished() {
            PATIENT_FRAMES
        } else {
            BRISK_FRAMES
        };
        // after then entire transaction has been transmitted, the ledger
        // automatically knows when to actually sign, this is becasue it read the
        // tx own rlp length header. 
        // user has 5 minutes to sign the tx via patient frames.
        answered = exchange(bus, &mut ledger, apdu, &mut response, patience)?;
    }


    let Some(body) = response.get(..answered) else {
        return Err(Error::ReplyTooLong);
    };

    // Ensure status is OK and extract the signature
    let signature = signature(require_ok(body)?)?;

    // decode the unsigned transaction
    let parsed = Unsigned::decode(unsigned)?;
    Ok(encode_signed(&parsed, &signature, out)?) // then encode back this time with sig
}

fn provide_token(bus: &mut Bus, ledger: &mut Ledger, token: &[u8]) -> Result<()> {
    let mut apdu = [0u8; APDU_MAX];
    // transmit signed token data to ledger
    let apdu_len = command(
        CLASS,
        INSTRUCTION_TOKEN_INFO,
        NO_PARAMETER,
        SECOND_PARAMETER,
        token,
        &mut apdu,
    )?;
    let Some(apdu) = apdu.get(..apdu_len) else {
        return Err(Error::ReplyTooLong);
    };

    let mut response = [0u8; RESPONSE_MAX];
    let answered = exchange(bus, ledger, apdu, &mut response, BRISK_FRAMES)?;
    // ensure we get status ok meaning ledger verified the signature
    require_ok(response.get(..answered).unwrap_or(&[]))?;

    Ok(())
}

/// sends an apdu command and then polls for reply
fn exchange(
    bus: &mut Bus,
    ledger: &mut Ledger,
    apdu: &[u8],
    response: &mut [u8],
    patience: u32,
) -> Result<usize> {
    let mut reply = [0u8; Packet::CAPACITY];


    for report in Reports::new(apdu) {
        // The report cuts the apdu into 64 bytes reports with ledger formatting
        deliver(bus, &mut ledger.interrupt_out, &report, &mut reply)?;
    }

    let mut assembly = Reassembly::new();
    let mut report = [0u8; REPORT_LEN];

    // Depending on brisk or patient
    for _ in 0..patience {
        // read and wait for the ledger to reply with its current status
        match ledger
            .interrupt_in
            .read_once(bus, &mut report, &mut reply)?
        {
            Progress::Busy => {
                // if its busy lets keep it alive
                bus.keep_alive()?;
                continue;
            }
            Progress::Done(len) => {
                // if its done lets ask for its progress,
                // the ledger is assumed to have queued up messages for us,
                // in apdu format which we reassemble
                let Some(received) = report.get(..len) else {
                    return Err(Error::ReplyTooLong);
                };
                if let Some(total) = assembly.push(received, response)? {
                    return Ok(total);
                }
            }
        }
    }

    Err(Error::LedgerSilent)
}

/// ping the ledger until it accepts our request for transmission
fn deliver(
    bus: &mut Bus,
    endpoint: &mut Endpoint,
    report: &[u8; REPORT_LEN],
    reply: &mut [u8],
) -> Result<()> {
    for _ in 0..BRISK_FRAMES {
        // if it marks as done it means it received it
        match endpoint.write_once(bus, report, reply)? {
            Progress::Done(_) => return Ok(()),
            Progress::Busy => bus.keep_alive()?,
        }
    }

    // ledger tx failed
    Err(Error::LedgerSendFailed)
}
