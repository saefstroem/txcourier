use crate::wire::error::Error;
use crate::wire::frame;
use crate::wire::record::Record;
use crate::wire::result::Result;
use crate::wire::slot::Slot;

const STATUS: u8 = 0x01;
const PUT: u8 = 0x0E;
// One opcode for both slots; the slot selector is in the payload.
const GET: u8 = 0x08;
const CLEAR: u8 = 0x0D;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Status,
    Put(Record),
    Get(Slot),
    Clear(Slot),
}

impl Request {
    pub const fn opcode(self) -> u8 {
        match self {
            Self::Status => STATUS,
            Self::Put(_) => PUT,
            Self::Get(_) => GET,
            Self::Clear(_) => CLEAR,
        }
    }

    pub fn encode(self, out: &mut [u8]) -> Result<usize> {
        match self {
            Self::Status => frame::encode(STATUS, &[], out),
            Self::Put(record) => frame::encode(PUT, record.as_slice(), out),
            Self::Get(slot) => frame::encode(GET, &[slot.to_byte()], out),
            Self::Clear(slot) => frame::encode(CLEAR, &[slot.to_byte()], out),
        }
    }

    pub fn decode(frame: &[u8]) -> Result<Self> {
        let decoded = frame::decode(frame)?;

        match decoded.opcode {
            STATUS if decoded.payload.is_empty() => Ok(Self::Status),
            STATUS => Err(Error::MalformedPayload),
            PUT => Record::from_slice(decoded.payload).map(Self::Put),
            GET => selected(decoded.payload).map(Self::Get),
            CLEAR => selected(decoded.payload).map(Self::Clear),
            other => Err(Error::UnknownOpcode(other)),
        }
    }
}

fn selected(payload: &[u8]) -> Result<Slot> {
    let &[byte] = payload else {
        return Err(Error::MalformedPayload);
    };
    Slot::from_byte(byte)
}

#[cfg(test)]
mod tests {
    use super::CLEAR;
    use super::Error;
    use super::GET;
    use super::PUT;
    use super::Record;
    use super::Request;
    use super::STATUS;
    use super::Slot;
    use crate::wire::frame;

    fn encoded(request: Request) -> Vec<u8> {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let written = request.encode(&mut buffer).expect("encode");
        buffer[..written].to_vec()
    }

    fn every_request() -> Vec<Request> {
        vec![
            Request::Status,
            Request::Put(Record::from_slice(&[0x02, 0xF0]).expect("fits")),
            Request::Get(Slot::Pending),
            Request::Get(Slot::Signed),
            Request::Clear(Slot::Pending),
            Request::Clear(Slot::Signed),
        ]
    }

    #[test]
    fn every_request_round_trips() {
        for request in every_request() {
            assert_eq!(Request::decode(&encoded(request)), Ok(request));
        }
    }

    #[test]
    fn the_encoded_bytes_of_every_request_are_pinned() {
        let expected: [(Request, Vec<u8>); 6] = [
            (Request::Status, vec![0x52, 0x57, 0x01, 0x01, 0x00, 0x00]),
            (
                Request::Put(Record::from_slice(&[0x02, 0xF0]).expect("fits")),
                vec![0x52, 0x57, 0x01, 0x0E, 0x02, 0x00, 0x02, 0xF0],
            ),
            (
                Request::Get(Slot::Pending),
                vec![0x52, 0x57, 0x01, 0x08, 0x01, 0x00, 0x00],
            ),
            (
                Request::Get(Slot::Signed),
                vec![0x52, 0x57, 0x01, 0x08, 0x01, 0x00, 0x01],
            ),
            (
                Request::Clear(Slot::Pending),
                vec![0x52, 0x57, 0x01, 0x0D, 0x01, 0x00, 0x00],
            ),
            (
                Request::Clear(Slot::Signed),
                vec![0x52, 0x57, 0x01, 0x0D, 0x01, 0x00, 0x01],
            ),
        ];

        for (request, bytes) in expected {
            assert_eq!(encoded(request), bytes, "{request:?}");
        }
    }

    #[test]
    fn no_single_flipped_bit_turns_one_opcode_into_another() {
        let opcodes = [STATUS, PUT, GET, CLEAR];

        for (index, first) in opcodes.iter().enumerate() {
            for second in opcodes.iter().skip(index + 1) {
                assert!(
                    (first ^ second).count_ones() >= 2,
                    "opcodes {first:#04x} and {second:#04x} are one bit apart: the frame carries \
                     no checksum, so a single flipped bit would silently change the request"
                );
            }
        }
    }
    #[test]
    fn the_surviving_opcodes_keep_their_wire_values() {
        assert_eq!(Request::Status.opcode(), 0x01);
    }

    #[test]
    fn the_merged_opcodes_keep_the_pending_half_of_each_pair() {
        assert_eq!(Request::Get(Slot::Pending).opcode(), 0x08);
        assert_eq!(Request::Get(Slot::Signed).opcode(), 0x08);
        assert_eq!(Request::Clear(Slot::Pending).opcode(), 0x0D);
        assert_eq!(Request::Clear(Slot::Signed).opcode(), 0x0D);
    }

    #[test]
    fn the_retired_opcodes_are_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        for opcode in [0x02u8, 0x04, 0x05, 0x06, 0x07, 0x09, 0x0B, 0x0C] {
            let written = frame::encode(opcode, &[], &mut buffer).expect("encode");
            assert_eq!(
                Request::decode(&buffer[..written]),
                Err(Error::UnknownOpcode(opcode)),
                "{opcode:#04x}"
            );
        }
    }

    #[test]
    fn an_unknown_opcode_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let written = frame::encode(0x7F, &[], &mut buffer).expect("encode");
        assert_eq!(
            Request::decode(&buffer[..written]),
            Err(Error::UnknownOpcode(0x7F))
        );
    }

    #[test]
    fn a_status_request_carrying_a_payload_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let written = frame::encode(STATUS, b"unexpected", &mut buffer).expect("encode");
        assert_eq!(
            Request::decode(&buffer[..written]),
            Err(Error::MalformedPayload)
        );
    }

    #[test]
    fn a_slot_request_without_exactly_one_selector_byte_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        for opcode in [super::GET, super::CLEAR] {
            for payload in [&[][..], &[0x00, 0x00][..], &b"unexpected"[..]] {
                let written = frame::encode(opcode, payload, &mut buffer).expect("encode");
                assert_eq!(
                    Request::decode(&buffer[..written]),
                    Err(Error::MalformedPayload),
                    "{opcode:#04x} {payload:?}"
                );
            }
        }
    }

    #[test]
    fn a_slot_request_naming_a_slot_that_does_not_exist_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        for opcode in [super::GET, super::CLEAR] {
            for byte in [0x02u8, 0x7F, 0xFF] {
                let written = frame::encode(opcode, &[byte], &mut buffer).expect("encode");
                assert_eq!(
                    Request::decode(&buffer[..written]),
                    Err(Error::MalformedPayload),
                    "{opcode:#04x} {byte:#04x}"
                );
            }
        }
    }

    #[test]
    fn a_put_carries_a_draft_record_under_its_own_opcode() {
        assert_eq!(
            Request::Put(Record::from_slice(&[]).expect("fits")).opcode(),
            0x0E
        );
    }

    #[test]
    fn a_record_larger_than_a_draft_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let payload = vec![0xA5u8; Record::CAPACITY + 1];
        let written = frame::encode(super::PUT, &payload, &mut buffer).expect("encode");
        assert_eq!(
            Request::decode(&buffer[..written]),
            Err(Error::PayloadTooLarge(Record::CAPACITY + 1))
        );
    }
}
