use txcourier_protocol::ledger::{CHUNK_MAX, REPORT_LEN, Reassembly, Reports, STATUS_LEN, command};
use txcourier_protocol::wire::Packet;

use crate::host::bus::Bus;
use crate::host::device::Ledger;
use crate::host::error::Error;
use crate::host::result::Result;
use crate::host::transport::{Endpoint, Progress};

// A five-byte APDU header followed by one data chunk of CHUNK_MAX.
pub(super) const APDU_MAX: usize = 5 + CHUNK_MAX;
// 255 bytes of response data followed by the two status bytes of ISO 7816-4.
pub(super) const RESPONSE_MAX: usize = 255 + STATUS_LEN;

// Not fixed by any specification: 2000 frames of 1 ms, about 2 s, for a reply that needs no
// button press.
pub(super) const BRISK_FRAMES: u32 = 2_000;
// Not fixed by any specification: 300 s for the final chunk, which waits on a person at the
// Ledger screen.
pub(super) const PATIENT_FRAMES: u32 = 300_000;

pub(super) fn ask(
    bus: &mut Bus,
    ledger: &mut Ledger,
    header: [u8; 4],
    data: &[u8],
    response: &mut [u8],
    patience: u32,
) -> Result<usize> {
    let [class, instruction, first, second] = header;
    let mut apdu = [0u8; APDU_MAX];
    let apdu_len = command(class, instruction, first, second, data, &mut apdu)?;
    let Some(apdu) = apdu.get(..apdu_len) else {
        return Err(Error::ReplyTooLong);
    };
    exchange(bus, ledger, apdu, response, patience)
}

/// sends an apdu command and then polls for reply
pub(super) fn exchange(
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
