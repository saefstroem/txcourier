use txcourier_protocol::ledger::{
    CHUNK_MAX, CLASS, Chunks, DEFAULT_ACCOUNT, INSTRUCTION_ADDRESS, INSTRUCTION_SIGN,
    INSTRUCTION_TOKEN_INFO, PATH_LEN, command, encode_path, ethereum_address, require_ok,
    signature,
};
use txcourier_protocol::transaction::{Unsigned, encode_signed};
use txcourier_protocol::wire::{Address, Packet};

use crate::host::apdu::{APDU_MAX, BRISK_FRAMES, PATIENT_FRAMES, RESPONSE_MAX, ask, exchange};
use crate::host::bus::Bus;
use crate::host::device;
use crate::host::device::{DESCRIPTOR_CAPACITY, Ledger};
use crate::host::error::Error;
use crate::host::result::Result;

// app-ethereum ethapp.adoc: INS 0x04 with P2 = 00 starts the signing flow, and INS 0x0A
// requires P2 = 00.
const SECOND_PARAMETER: u8 = 0x00;
// app-ethereum ethapp.adoc: INS 0x0A requires P1 = 00.
const NO_PARAMETER: u8 = 0x00;
const NO_CHAIN_CODE: u8 = 0x00;

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

pub fn address(bus: &mut Bus, show: bool) -> Result<Address> {
    let mut reply = [0u8; Packet::CAPACITY];
    let mut scratch = [0u8; DESCRIPTOR_CAPACITY];
    let ledger = &mut device::attach(bus, &mut reply, &mut scratch)?;
    let patience = if show { PATIENT_FRAMES } else { BRISK_FRAMES };
    let mut path = [0u8; PATH_LEN];
    let path_len = encode_path(&DEFAULT_ACCOUNT, &mut path)?;
    let path = path.get(..path_len).unwrap_or(&[]);
    let header = [CLASS, INSTRUCTION_ADDRESS, u8::from(show), NO_CHAIN_CODE];
    let mut response = [0u8; RESPONSE_MAX];
    let len = ask(bus, ledger, header, path, &mut response, patience)?;
    let address = ethereum_address(require_ok(response.get(..len).unwrap_or(&[]))?)?;
    Address::from_slice(address).map_err(|_| Error::ReplyTooLong)
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
