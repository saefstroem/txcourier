use crate::crc::crc16;
use crate::error::Error;
use crate::pid;
use crate::result::Result;
use crate::token::SYNC;

// USB 2.0 section 8.2: the SYNC field is eight bits at full speed.
const SYNC_LEN: usize = 1;
// USB 2.0 section 8.3.1: a PID is a four-bit type field and its four-bit complement.
const PID_LEN: usize = 1;
// USB 2.0 section 8.3.5.2: the data CRC is 16 bits over the data field.
const CRC_LEN: usize = 2;
// The non-payload bytes of a data packet. SYNC is in the buffer because the transmitter
// sends it and the decoder does not strip it.
const OVERHEAD: usize = SYNC_LEN + PID_LEN + CRC_LEN;

/// Constructs a valid USB data packet
pub fn data_packet(pid: u8, payload: &[u8], out: &mut [u8]) -> Result<usize> {
    let needed = payload.len() + OVERHEAD;

    let Some(packet) = out.get_mut(..needed) else {
        // fail if payload is too small for sync and pid, crc and payload
        return Err(Error::OutputTooSmall {
            needed,
            found: out.len(),
        });
    };
    // split payload into header and rest
    let Some((head, rest)) = packet.split_at_mut_checked(SYNC_LEN + PID_LEN) else {
        return Err(Error::PacketTooShort(needed));
    };

    // copy sync and pid into header
    head.copy_from_slice(&[SYNC, pid]);

    // split rest into payload and checksum
    let Some((body, checksum)) = rest.split_at_mut_checked(payload.len()) else {
        return Err(Error::PacketTooShort(needed));
    };

    // copy payload into body and compute checksum
    body.copy_from_slice(payload);

    // technically we should not be computing checksum for non-data packets,
    // but reader doesnt check em so whatever
    checksum.copy_from_slice(&crc16(payload).to_le_bytes());

    Ok(needed)
}

/// Parses a USB data packet, returning the pid and payload if valid
pub fn data_payload(packet: &[u8]) -> Result<(u8, &[u8])> {
    // check that packet is long enough to contain sync, pid, and checksum
    // this function only parses data packets, so checksum needs to be checkeed
    let Some(payload_len) = packet.len().checked_sub(OVERHEAD) else {
        return Err(Error::PacketTooShort(packet.len()));
    };

    // split packet into sync, pid, payload, and checksum
    let Some((&sync, rest)) = packet.split_first() else {
        return Err(Error::PacketTooShort(packet.len()));
    };

    // check for valid sync
    if sync != SYNC {
        return Err(Error::BadSync(sync));
    }

    // split rest into pid and payload+checksum
    let Some((&identifier, rest)) = rest.split_first() else {
        return Err(Error::PacketTooShort(packet.len()));
    };
    if identifier != pid::DATA0 && identifier != pid::DATA1 {
        return Err(Error::NotData(identifier));
    }

    let Some((payload, checksum)) = rest.split_at_checked(payload_len) else {
        return Err(Error::PacketTooShort(packet.len()));
    };

    // check that the checksum matches the payload
    let declared = match checksum {
        [low, high] => u16::from_le_bytes([*low, *high]),
        _ => return Err(Error::PacketTooShort(packet.len())),
    };
    // compute the checksum of the payload and compare it to the declared checksum
    let computed = crc16(payload);
    if declared != computed {
        return Err(Error::ChecksumMismatch { declared, computed });
    }

    Ok((identifier, payload))
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::data_packet;
    use super::data_payload;
    use crate::pid;

    const GET_DESCRIPTOR_SETUP: [u8; 8] = [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x08, 0x00];
    const GET_DESCRIPTOR_DATA0: [u8; 12] = [
        0x80, 0xC3, 0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x08, 0x00, 0xEB, 0x94,
    ];

    fn packet(pid: u8, payload: &[u8]) -> Vec<u8> {
        let mut buffer = vec![0u8; payload.len() + 4];
        let written = data_packet(pid, payload, &mut buffer).expect("fits");
        buffer.truncate(written);
        buffer
    }

    #[test]
    fn the_get_descriptor_setup_packet_matches_the_reference_encoder() {
        assert_eq!(
            packet(pid::DATA0, &GET_DESCRIPTOR_SETUP),
            GET_DESCRIPTOR_DATA0
        );
    }

    #[test]
    fn an_empty_data1_packet_carries_a_zero_checksum() {
        assert_eq!(packet(pid::DATA1, &[]), [0x80, 0x4B, 0x00, 0x00]);
    }

    #[test]
    fn a_packet_round_trips_through_the_parser() {
        for payload in [&[][..], &GET_DESCRIPTOR_SETUP[..], &[0xFF; 64][..]] {
            for pid in [pid::DATA0, pid::DATA1] {
                let bytes = packet(pid, payload);
                assert_eq!(data_payload(&bytes), Ok((pid, payload)));
            }
        }
    }

    #[test]
    fn a_corrupted_checksum_is_rejected() {
        let mut bytes = GET_DESCRIPTOR_DATA0;
        bytes[10] ^= 0x01;
        assert!(matches!(
            data_payload(&bytes),
            Err(Error::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn a_corrupted_payload_is_rejected() {
        let mut bytes = GET_DESCRIPTOR_DATA0;
        bytes[5] ^= 0x80;
        assert!(matches!(
            data_payload(&bytes),
            Err(Error::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn a_handshake_is_not_a_data_packet() {
        assert_eq!(data_payload(&[0x80, 0xD2]), Err(Error::PacketTooShort(2)));
        assert_eq!(
            data_payload(&[0x80, 0xD2, 0x00, 0x00]),
            Err(Error::NotData(0xD2))
        );
    }

    #[test]
    fn anything_shorter_than_the_overhead_is_too_short() {
        for bytes in [&[][..], &[0x80][..], &[0x80, 0xC3, 0x00][..]] {
            assert_eq!(data_payload(bytes), Err(Error::PacketTooShort(bytes.len())));
        }
    }

    #[test]
    fn a_packet_without_sync_is_rejected() {
        assert_eq!(
            data_payload(&[0x00, 0x4B, 0x00, 0x00]),
            Err(Error::BadSync(0x00))
        );
    }

    #[test]
    fn a_short_buffer_is_refused() {
        let mut buffer = [0u8; 3];
        assert!(data_packet(pid::DATA0, &[], &mut buffer).is_err());
    }
}
