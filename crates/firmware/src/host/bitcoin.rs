use txcourier_protocol::bitcoin::{Draft, PUBLIC_KEY_LEN};
use txcourier_protocol::ledger::{
    self, ACCOUNT_REQUEST, CONTINUE, Client, EXTENDED_PUBKEY, FINGERPRINT, KEY_REQUEST,
    MAX_DATA_LEN, SIGN_PSBT, STATUS_INTERRUPTED, public_key, require_ok,
};
use txcourier_protocol::wire::Packet;

use crate::host::apdu::{BRISK_FRAMES, PATIENT_FRAMES, RESPONSE_MAX, ask};
use crate::host::bus::Bus;
use crate::host::device;
use crate::host::device::{DESCRIPTOR_CAPACITY, Ledger};
use crate::host::error::Error;
use crate::host::result::Result;

const MOST_ROUNDS: usize = 4096;

/// Signs a Bitcoin transaction draft using the attached Ledger device and returns the signed transaction.
pub fn sign(bus: &mut Bus, record: &[u8], out: &mut [u8]) -> Result<usize> {
    // Decode the transaction draft from the provided record.
    let draft = Draft::decode(record).map_err(ledger::Error::Bitcoin)?;
    // Prepare the communication buffers and attach to the Ledger device.
    let mut reply = [0u8; Packet::CAPACITY];
    // Allocate scratch space for the Ledger device descriptor.
    let mut scratch = [0u8; DESCRIPTOR_CAPACITY];
    // Attach to the Ledger device using the prepared buffers.
    let ledger = &mut device::attach(bus, &mut reply, &mut scratch)?;
    let mut key = [0u8; RESPONSE_MAX];
    // Request the extended public key from the Ledger device.
    let key = answered(bus, ledger, EXTENDED_PUBKEY, &KEY_REQUEST, &mut key)?;
    // Initialize the bitcoin client with the draft and the obtained public key.
    let mut client = client(bus, ledger, Some((draft, public_key(key)?)))?;
    // Generate the partially signed Bitcoin transaction (PSBT) request.
    let psbt = client.sign_request()?;
    // Discuss with the Ledger and interactively validate and retrieve signature for PSBT
    converse(bus, ledger, &mut client, SIGN_PSBT, &psbt, PATIENT_FRAMES)?;
    Ok(client.signed(out)?)
}

/// Initializes the Bitcoin client with the provided Ledger device and optional signing capabilities.
/// Retrieves the device fingerprint and extended public key, then constructs the client.
fn client(
    bus: &mut Bus,
    ledger: &mut Ledger,
    signing: Option<(Draft, [u8; PUBLIC_KEY_LEN])>,
) -> Result<Client> {
    // Request the device fingerprint from the Ledger device.
    let mut fingerprint = [0u8; RESPONSE_MAX];
    let mut xpub = [0u8; RESPONSE_MAX];
    let fingerprint = answered(bus, ledger, FINGERPRINT, &[], &mut fingerprint)?;
    // Request the extended public key from the Ledger device.
    let xpub = answered(bus, ledger, EXTENDED_PUBKEY, &ACCOUNT_REQUEST, &mut xpub)?;
    Ok(Client::new(fingerprint, xpub, signing)?)
}

/// Sends a request to the Ledger device and waits for a response, ensuring it is properly answered.
fn answered<'a>(
    bus: &mut Bus,
    ledger: &mut Ledger,
    header: [u8; 4],
    data: &[u8],
    response: &'a mut [u8],
) -> Result<&'a [u8]> {
    // Send the request to the Ledger device and wait for the response.
    let len = ask(bus, ledger, header, data, response, BRISK_FRAMES)?;
    // Ensure the response is properly answered and return it.
    Ok(require_ok(response.get(..len).unwrap_or(&[]))?)
}

/// Perform the interactive conversation "ceremony"
fn converse(
    bus: &mut Bus,
    ledger: &mut Ledger,
    client: &mut Client,
    header: [u8; 4],
    data: &[u8],
    patience: u32,
) -> Result<()> {
    let mut response = [0u8; RESPONSE_MAX];
    let mut len = ask(bus, ledger, header, data, &mut response, patience)?;
    for _ in 0..MOST_ROUNDS {
        let answered = response.get(..len).unwrap_or(&[]);
        let Some((command, &STATUS_INTERRUPTED)) = answered.split_last_chunk() else {
            require_ok(answered)?;
            return Ok(());
        };
        let mut reply = [0u8; MAX_DATA_LEN];
        let reply_len = client.answer(command, &mut reply)?;
        let reply = reply.get(..reply_len).unwrap_or(&[]);
        len = ask(bus, ledger, CONTINUE, reply, &mut response, patience)?;
    }

    Err(Error::TooManyRounds)
}
