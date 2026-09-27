use crate::wire::error::Error;
use crate::wire::result::Result;

// ASCII "RW", the frame start marker. The byte stream has no other framing, so a frame
// picked up mid-stream fails rather than decodes.
const MAGIC: [u8; 2] = [0x52, 0x57];
// The envelope version. There is one value and no negotiation, so an unknown version is
// rejected and host and firmware must be built from the same commit.
const VERSION: u8 = 0x01;

// After the two magic bytes.
const VERSION_AT: usize = 2;
// After the magic and the version byte.
const OPCODE_AT: usize = 3;
// After the magic, the version and the opcode.
const LENGTH_AT: usize = 4;

// Magic 2, version 1, opcode 1, length 2.
pub const HEADER_LEN: usize = 6;
// A round ceiling just above PROVISIONING_CAPACITY, 2036. It must fit u16, since encode
// casts the payload length with `as u16`.
pub const MAX_PAYLOAD_LEN: usize = 2048;
// The header plus the largest payload. Every frame buffer on both sides is sized from this.
pub const MAX_FRAME_LEN: usize = HEADER_LEN + MAX_PAYLOAD_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Decoded instruction
pub struct Decoded<'a> {
    pub opcode: u8,
    pub payload: &'a [u8],
}

/// Encode instruction and payload to some mut buffer
pub fn encode(opcode: u8, payload: &[u8], out: &mut [u8]) -> Result<usize> {
    if payload.len() > MAX_PAYLOAD_LEN {
        return Err(Error::PayloadTooLarge(payload.len()));
    }
    // validate header size
    let total = HEADER_LEN + payload.len();
    if out.len() < total {
        return Err(Error::OutputTooSmall {
            needed: total,
            found: out.len(),
        });
    }

    let length = payload.len() as u16;

    /// put magiv, version, opcode, length, and payload
    put(out, 0, &MAGIC)?;
    put(out, VERSION_AT, &[VERSION])?;
    put(out, OPCODE_AT, &[opcode])?;
    put(out, LENGTH_AT, &length.to_le_bytes())?;
    put(out, HEADER_LEN, payload)?;

    Ok(total)
}

/// Extract payload length from header
pub fn payload_len(header: &[u8]) -> Result<usize> {
    if header.len() < HEADER_LEN {
        return Err(Error::TooShort {
            needed: HEADER_LEN,
            found: header.len(),
        });
    }

    // extract magic
    if take(header, 0, MAGIC.len())? != MAGIC {
        return Err(Error::BadMagic);
    }

    // then take version
    let version = byte_at(header, VERSION_AT)?;
    if version != VERSION {
        return Err(Error::UnsupportedVersion(version));
    }

    // get length of payload, its right after one another
    let low = byte_at(header, LENGTH_AT)?;
    let high = byte_at(header, LENGTH_AT + 1)?;
    let length = usize::from(u16::from_le_bytes([low, high]));

    // validate payload not too large
    if length > MAX_PAYLOAD_LEN {
        return Err(Error::PayloadTooLarge(length));
    }

    Ok(length)
}

/// Frame length is the header + size of payload
pub fn frame_len(header: &[u8]) -> Result<usize> {
    Ok(HEADER_LEN + payload_len(header)?)
}

/// Decode a full serialized frame into semi structured decoded instructions
pub fn decode(frame: &[u8]) -> Result<Decoded<'_>> {
    let length = payload_len(frame)?;
    let total = HEADER_LEN + length;

    if frame.len() < total {
        return Err(Error::TooShort {
            needed: total,
            found: frame.len(),
        });
    }

    if frame.len() > total {
        return Err(Error::TrailingBytes(frame.len() - total));
    }

    Ok(Decoded {
        opcode: byte_at(frame, OPCODE_AT)?,
        payload: take(frame, HEADER_LEN, length)?,
    })
}

/// Put some bytes at position
fn put(out: &mut [u8], at: usize, bytes: &[u8]) -> Result<()> {
    let end = at + bytes.len();

    // validate and extract mutable slot
    let Some(slot) = out.get_mut(at..end) else {
        return Err(Error::OutputTooSmall {
            needed: end,
            found: out.len(),
        });
    };

    // copy it into the buffer
    slot.copy_from_slice(bytes);
    Ok(())
}

/// Read some bytes off of it
fn take(bytes: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    let end = at + len;
    bytes.get(at..end).ok_or(Error::TooShort {
        needed: end,
        found: bytes.len(),
    })
}

/// Read byte at an index
fn byte_at(bytes: &[u8], at: usize) -> Result<u8> {
    bytes.get(at).copied().ok_or(Error::TooShort {
        needed: at + 1,
        found: bytes.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::HEADER_LEN;
    use super::MAX_FRAME_LEN;
    use super::MAX_PAYLOAD_LEN;
    use super::decode;
    use super::encode;
    use super::frame_len;
    use super::payload_len;

    fn round_trip(opcode: u8, payload: &[u8]) -> Vec<u8> {
        let mut buffer = [0u8; MAX_FRAME_LEN];
        let written = encode(opcode, payload, &mut buffer).expect("encode");
        buffer[..written].to_vec()
    }

    #[test]
    fn a_frame_is_a_header_followed_by_its_payload() {
        assert_eq!(
            round_trip(0x07, &[0x02, 0xF0]),
            vec![0x52, 0x57, 0x01, 0x07, 0x02, 0x00, 0x02, 0xF0]
        );
    }

    #[test]
    fn an_empty_payload_round_trips() {
        let frame = round_trip(0x01, &[]);
        assert_eq!(frame.len(), HEADER_LEN);
        let decoded = decode(&frame).expect("decode");
        assert_eq!(decoded.opcode, 0x01);
        assert!(decoded.payload.is_empty());
    }

    #[test]
    fn a_payload_round_trips() {
        let payload = b"pending transaction";
        let frame = round_trip(0x42, payload);
        let decoded = decode(&frame).expect("decode");
        assert_eq!(decoded.opcode, 0x42);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn the_largest_payload_round_trips() {
        let payload = [0xABu8; MAX_PAYLOAD_LEN];
        let frame = round_trip(0x01, &payload);
        assert_eq!(frame.len(), MAX_FRAME_LEN);
        assert_eq!(decode(&frame).expect("decode").payload, payload);
    }

    #[test]
    fn every_byte_value_survives_a_round_trip() {
        let payload: Vec<u8> = (0u8..=0xFF).collect();
        let frame = round_trip(0x01, &payload);
        assert_eq!(decode(&frame).expect("decode").payload, payload.as_slice());
    }

    #[test]
    fn an_oversized_payload_is_refused() {
        let payload = [0u8; MAX_PAYLOAD_LEN + 1];
        let mut buffer = [0u8; MAX_FRAME_LEN + 16];
        assert_eq!(
            encode(0x01, &payload, &mut buffer),
            Err(Error::PayloadTooLarge(MAX_PAYLOAD_LEN + 1))
        );
    }

    #[test]
    fn a_short_output_buffer_is_refused() {
        let mut buffer = [0u8; HEADER_LEN];
        assert_eq!(
            encode(0x01, b"too long", &mut buffer),
            Err(Error::OutputTooSmall {
                needed: HEADER_LEN + 8,
                found: HEADER_LEN,
            })
        );
    }

    #[test]
    fn an_output_buffer_of_exactly_the_frame_length_is_enough() {
        let mut buffer = [0u8; HEADER_LEN + 8];
        assert_eq!(encode(0x01, b"transfer", &mut buffer), Ok(HEADER_LEN + 8));
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut frame = round_trip(0x01, &[]);
        frame[0] = 0x00;
        assert_eq!(decode(&frame), Err(Error::BadMagic));
    }

    #[test]
    fn an_unsupported_version_is_rejected() {
        let mut frame = round_trip(0x01, &[]);
        frame[2] = 0x99;
        assert_eq!(decode(&frame), Err(Error::UnsupportedVersion(0x99)));
    }

    #[test]
    fn a_truncated_frame_is_rejected() {
        let frame = round_trip(0x01, b"transfer");
        for keep in 0..frame.len() {
            assert!(decode(&frame[..keep]).is_err(), "accepted {keep} bytes");
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut frame = round_trip(0x01, b"transfer");
        frame.push(0x00);
        assert_eq!(decode(&frame), Err(Error::TrailingBytes(1)));
    }

    #[test]
    fn a_length_beyond_the_maximum_payload_is_rejected() {
        let mut header = [0x52, 0x57, 0x01, 0x01, 0x00, 0x00];
        let overlong = (MAX_PAYLOAD_LEN + 1) as u16;
        header[4..6].copy_from_slice(&overlong.to_le_bytes());
        assert_eq!(
            payload_len(&header),
            Err(Error::PayloadTooLarge(MAX_PAYLOAD_LEN + 1))
        );
    }

    #[test]
    fn the_header_declares_the_payload_and_frame_length() {
        let frame = round_trip(0x01, b"transfer");
        assert_eq!(payload_len(&frame[..HEADER_LEN]).expect("len"), 8);
        assert_eq!(
            frame_len(&frame[..HEADER_LEN]).expect("len"),
            HEADER_LEN + 8
        );
        assert_eq!(frame_len(&frame[..HEADER_LEN]).expect("len"), frame.len());
    }

    #[test]
    fn a_short_header_cannot_declare_a_length() {
        assert!(matches!(
            payload_len(&[0x52, 0x57]),
            Err(Error::TooShort { .. })
        ));
    }
}
