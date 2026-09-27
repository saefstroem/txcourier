use rp2350wallet_pio_usb::{
    PID_ACK, PID_DATA0, PID_DATA1, PID_IN, PID_NAK, PID_OUT, PID_STALL, SYNC, data_packet,
    data_payload, token,
};
use rp2350wallet_protocol::ledger::REPORT_LEN;

use crate::host::bus::Bus;
use crate::host::error::Error;
use crate::host::result::Result;

// USB 2.0 sections 8.2 and 8.4.4: a data packet is SYNC, PID, payload, CRC16. Four bytes
// surround the payload.
const DATA_OVERHEAD: usize = 4;
// One 64-byte HID report carried as a single data packet.
const REPORT_PACKET_LEN: usize = REPORT_LEN + DATA_OVERHEAD;

pub enum Progress {
    Done(usize),
    Busy,
}

// one direction of one endpoint on the ledger
pub struct Endpoint {
    address: u8,
    number: u8,
    toggle: u8,
}

impl Endpoint {
    pub const fn new(address: u8, number: u8) -> Self {
        Self {
            address,
            number,
            toggle: PID_DATA0,
        }
    }

    /// This reads once from the ledger, its intended to be
    /// called multiple times for a full read
    pub fn read_once(
        &mut self,
        bus: &mut Bus,
        out: &mut [u8],
        reply: &mut [u8],
    ) -> Result<Progress> {
        // Transmit that we want to read
        let reception = bus.transact(&[&token(PID_IN, self.address, self.number)], reply, true)?;

        if !reception.start_seen {
            return Err(Error::NoReply);
        }

        // We want to ensure that the ledger is not busy or has stalled
        match reply.get(..reception.len).unwrap_or(&[]) {
            [SYNC, PID_NAK] => return Ok(Progress::Busy),
            [SYNC, PID_STALL] => return Err(Error::Stall),
            _ => {}
        }

        // ensure that we have acked the transmission from ledger
        if !reception.acknowledged {
            return Err(Error::CorruptReply);
        }

        // read the data bytes
        let bytes = reply.get(..reception.len).unwrap_or(&[]);

        // extract identifeir and payload
        let (identifier, payload) = data_payload(bytes).map_err(|_| Error::CorruptReply)?;

        // a mismatched toggle means the ledger resent a packet we already have, because it
        // missed our ack. the bus has acked it again, so drop it and poll for the next one
        if identifier != self.toggle {
            return Ok(Progress::Busy);
        }

        // extract the data from the payload
        let Some(slot) = out.get_mut(..payload.len()) else {
            return Err(Error::ReplyTooLong);
        };
        slot.copy_from_slice(payload);

        // packet accepted: the next new packet from the ledger will carry the other data pid.
        // the ledger flips its own toggle when it hears the ack the bus already sent
        self.toggle = flipped(self.toggle);

        Ok(Progress::Done(payload.len()))
    }

    /// writes data from host to the ledger
    pub fn write_once(&mut self, bus: &mut Bus, data: &[u8], reply: &mut [u8]) -> Result<Progress> {
        let mut packet = [0u8; REPORT_PACKET_LEN];
        // create packet
        let packet_len = data_packet(self.toggle, data, &mut packet)?;

        // ensure packet is not too small
        let Some(packet) = packet.get(..packet_len) else {
            return Err(Error::ReplyTooLong);
        };

        // transmit the packet
        let reception = bus.transact(
            &[&token(PID_OUT, self.address, self.number), packet],
            reply,
            false,
        )?;

        // ensure we have gotten a reply back after transaction
        if !reception.start_seen {
            return Err(Error::NoReply);
        }

        // ensure the reply is done, this means that the ledger accepted
        // our data
        match reply.get(..reception.len).unwrap_or(&[]) {
            [SYNC, PID_ACK] => {
                self.toggle = flipped(self.toggle);
                Ok(Progress::Done(data.len()))
            }
            [SYNC, PID_NAK] => Ok(Progress::Busy),
            [SYNC, PID_STALL] => Err(Error::Stall),
            _ => Err(Error::UnexpectedReply),
        }
    }
}

/// toggle the identifier to another one
const fn flipped(identifier: u8) -> u8 {
    if identifier == PID_DATA1 {
        PID_DATA0
    } else {
        PID_DATA1
    }
}
