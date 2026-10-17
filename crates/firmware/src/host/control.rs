use txcourier_pio_usb::{
    PID_ACK, PID_DATA0, PID_DATA1, PID_IN, PID_NAK, PID_OUT, PID_SETUP, PID_STALL, SETUP_LEN, SYNC,
    Request, data_packet, data_payload, token,
};

use crate::host::bus::{Bus, Reception};
use crate::host::error::Error;
use crate::host::result::Result;

// USB 2.0 section 5.3.1: every device implements a default control method on endpoint
// zero. It is the only endpoint reachable before SetAddress and SetConfiguration.
const CONTROL_ENDPOINT: u8 = 0;
// USB 2.0 sections 8.2 and 8.4.4: a data packet is SYNC, PID, payload, CRC16. Four bytes
// surround the payload.
const DATA_OVERHEAD: usize = 4;
// USB 2.0 section 9.2.6.4: a device has 500 ms to return each data packet of a control
// transfer. One retry is paced per 1 ms frame.
const NAK_RETRY_FRAMES: u32 = 500;
// USB 2.0 sections 9.2.6.3 and 9.2.6.4: a status stage must complete within 50 ms. One
// retry is paced per 1 ms frame.
const STATUS_RETRY_FRAMES: u32 = 50;

// USB 2.0 section 8.5.3: a control status stage is a zero-length DATA1. CRC16 over an
// empty data field is 0x0000.
const ZERO_LENGTH_DATA1: [u8; 4] = [SYNC, PID_DATA1, 0x00, 0x00];

/// Performs control transfer
pub fn control(
    bus: &mut Bus,
    address: u8,
    request: Request,
    max_packet: usize,
    out: &mut [u8],
    reply: &mut [u8],
) -> Result<usize> {
    // usb control transfer first stage
    let setup_stage = |bus: &mut Bus, reply: &mut [u8]| -> Result<()> {
        // send the setup packet and expect an ACK handshake
        let setup = request.encode();
        let mut data = [0u8; SETUP_LEN + DATA_OVERHEAD];
        let data_len = data_packet(PID_DATA0, &setup, &mut data)?;
        let data = data.get(..data_len).unwrap_or(&[]);

        // transact and read the reply
        let reception = bus.transact(
            &[&token(PID_SETUP, address, CONTROL_ENDPOINT), data],
            reply,
            false,
        )?;
        expect_handshake(reply, &reception, PID_ACK)
    };

    setup_stage(bus, reply)?;

    match usize::from(request.response_length()) {
        0 => {
            // if its setaddress or setconfig then we dont expect any data
            status_in_stage(bus, address, reply)?;
            Ok(0)
        }
        wanted => {
            // Getdescriptor should return data
            let received = data_stage(bus, address, wanted, max_packet, out, reply)?;
            status_stage(bus, address, reply)?;
            Ok(received)
        }
    }
}

fn data_stage(
    bus: &mut Bus,
    address: u8,
    requested: usize,
    max_packet: usize,
    out: &mut [u8],
    reply: &mut [u8],
) -> Result<usize> {
    let mut total = 0;
    let mut expected = PID_DATA1;

    loop {
        let Some(free) = out.get_mut(total..) else {
            return Err(Error::ReplyTooLong);
        };

        let payload_len = in_transaction(bus, address, CONTROL_ENDPOINT, expected, free, reply)?;
        total += payload_len;
        expected = toggle(expected);

        if payload_len < max_packet || total >= requested {
            break;
        }
    }

    // return read bytes but not more than requested
    Ok(total)
}

fn status_stage(bus: &mut Bus, address: u8, reply: &mut [u8]) -> Result<()> {
    for _ in 0..STATUS_RETRY_FRAMES {
        // after reading expected data send a zero length data1 packet and expect an ACK handshake
        let reception = bus.transact(
            &[
                &token(PID_OUT, address, CONTROL_ENDPOINT),
                &ZERO_LENGTH_DATA1,
            ],
            reply,
            false,
        )?;

        // check that the reply is an ACK handshake
        // we want to ensure the device has completed the request
        match expect_handshake(reply, &reception, PID_ACK) {
            Err(Error::NegativeAcknowledge) => bus.keep_alive()?,
            outcome => return outcome,
        }
    }

    Err(Error::NegativeAcknowledge)
}

/// matches a reply to the appropriate pattern
fn expect_handshake(reply: &[u8], reception: &Reception, expected: u8) -> Result<()> {
    if !reception.start_seen {
        return Err(Error::NoReply);
    }
    if !reception.end_seen {
        return Err(Error::CorruptReply);
    }

    match reply.get(..reception.len).unwrap_or(&[]) {
        [SYNC, identifier] if *identifier == expected => Ok(()),
        [SYNC, PID_NAK] => Err(Error::NegativeAcknowledge),
        [SYNC, PID_STALL] => Err(Error::Stall),
        _ => Err(Error::UnexpectedReply),
    }
}

fn status_in_stage(bus: &mut Bus, address: u8, reply: &mut [u8]) -> Result<()> {
    for _ in 0..STATUS_RETRY_FRAMES {
        let reception = bus.transact(&[&token(PID_IN, address, CONTROL_ENDPOINT)], reply, true)?;

        if !reception.start_seen {
            return Err(Error::NoReply);
        }

        // ensure that this is a zero length data packet that we got back
        match reply.get(..reception.len).unwrap_or(&[]) {
            [SYNC, PID_NAK] => {
                bus.keep_alive()?;
                continue;
            }
            [SYNC, PID_STALL] => return Err(Error::Stall),
            _ => {}
        }

        // ensure we have acknowledged the reply
        if !reception.acknowledged {
            return Err(Error::CorruptReply);
        }

        let bytes = reply.get(..reception.len).unwrap_or(&[]);
        let (_, payload) = data_payload(bytes).map_err(|_| Error::CorruptReply)?;
        
        // it shouldnt be empty
        if !payload.is_empty() {
            return Err(Error::UnexpectedReply);
        }

        return Ok(());
    }

    Err(Error::NegativeAcknowledge)
}

/// fetches one packet payload
fn in_transaction(
    bus: &mut Bus,
    address: u8,
    endpoint: u8,
    expected: u8,
    out: &mut [u8],
    reply: &mut [u8],
) -> Result<usize> {
    // allowed up until 64 times
    for _ in 0..NAK_RETRY_FRAMES {
        // read the reception
        let reception = bus.transact(&[&token(PID_IN, address, endpoint)], reply, true)?;

        // ensure we have gotten initial reply
        if !reception.start_seen {
            return Err(Error::NoReply);
        }

        // ensure reply is not matching any of the nak or stall packets
        match reply.get(..reception.len).unwrap_or(&[]) {
            [SYNC, PID_NAK] => {
                bus.keep_alive()?;
                continue;
            }
            [SYNC, PID_STALL] => return Err(Error::Stall),
            _ => {}
        }

        // we must acknowledge the reply
        if !reception.acknowledged {
            return Err(Error::CorruptReply);
        }

        // read the bytes from the receiption
        let bytes = reply.get(..reception.len).unwrap_or(&[]);

        // read the data
        let (identifier, payload) = data_payload(bytes).map_err(|_| Error::CorruptReply)?;

        // we might receive a duplicate so there is no reason to error here
        if identifier != expected {
            continue;
        }

        // read payload
        let Some(slot) = out.get_mut(..payload.len()) else {
            return Err(Error::ReplyTooLong);
        };
        // copy it into slot
        slot.copy_from_slice(payload);

        return Ok(payload.len());
    }

    Err(Error::NegativeAcknowledge)
}

/// usb toggles between 1 and 0
const fn toggle(identifier: u8) -> u8 {
    if identifier == PID_DATA1 {
        PID_DATA0
    } else {
        PID_DATA1
    }
}
