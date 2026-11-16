use crate::wire::chain::Chain;
use crate::wire::error::Error;
use crate::wire::frame;
use crate::wire::record::{BitcoinRecord, EVMRecord};
use crate::wire::result::Result;
use crate::wire::slot::Slot;

const STATUS: u8 = 0x01;
const PUT_EVM: u8 = 0x0E;
// One opcode for both slots; the slot selector is in the payload.
const GET: u8 = 0x08;
const CLEAR: u8 = 0x0D;
const ADDRESS: u8 = 0x10;
const PUT_BITCOIN: u8 = 0x13;
const QUIETLY: u8 = 0x00;
const ON_SCREEN: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Status,
    PutEVM(EVMRecord),
    Get(Slot),
    Clear(Slot),
    Address { chain: Chain, show: bool },
    PutBitcoin(BitcoinRecord),
}

impl Request {
    pub fn decode(frame: &[u8]) -> Result<Self> {
        let decoded = frame::decode(frame)?;

        match decoded.opcode {
            STATUS if decoded.payload.is_empty() => Ok(Self::Status),
            STATUS => Err(Error::MalformedPayload),
            PUT_EVM => EVMRecord::from_slice(decoded.payload).map(Self::PutEVM),
            GET => selected(decoded.payload).map(Self::Get),
            CLEAR => selected(decoded.payload).map(Self::Clear),
            ADDRESS => addressed(decoded.payload),
            PUT_BITCOIN => BitcoinRecord::from_slice(decoded.payload).map(Self::PutBitcoin),
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

fn addressed(payload: &[u8]) -> Result<Request> {
    match Chain::split(payload) {
        Some((chain, &[show @ (QUIETLY | ON_SCREEN)])) => Ok(Request::Address {
            chain,
            show: show == ON_SCREEN,
        }),
        _ => Err(Error::MalformedPayload),
    }
}

#[cfg(test)]
mod tests {
    use super::ADDRESS;
    use super::BitcoinRecord;
    use super::CLEAR;
    use super::Chain;
    use super::EVMRecord;
    use super::Error;
    use super::GET;
    use super::PUT_BITCOIN;
    use super::PUT_EVM;
    use super::Request;
    use super::STATUS;
    use super::Slot;
    use crate::wire::frame;

    #[test]
    fn every_pinned_frame_decodes_to_its_request() {
        let expected: [(Request, Vec<u8>); 9] = [
            (Request::Status, vec![0x52, 0x57, 0x01, 0x01, 0x00, 0x00]),
            (
                Request::PutEVM(EVMRecord::from_slice(&[0x02, 0xF0]).expect("fits")),
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
            (
                Request::Address {
                    chain: Chain::Ethereum,
                    show: false,
                },
                vec![0x52, 0x57, 0x01, 0x10, 0x02, 0x00, 0x37, 0x00],
            ),
            (
                Request::Address {
                    chain: Chain::Bitcoin,
                    show: true,
                },
                vec![0x52, 0x57, 0x01, 0x10, 0x02, 0x00, 0x87, 0x01],
            ),
            (
                Request::PutBitcoin(BitcoinRecord::from_slice(&[0xB7, 0x01]).expect("fits")),
                vec![0x52, 0x57, 0x01, 0x13, 0x02, 0x00, 0xB7, 0x01],
            ),
        ];

        for (request, bytes) in expected {
            assert_eq!(Request::decode(&bytes), Ok(request), "{request:?}");
        }
    }

    #[test]
    fn no_single_flipped_bit_turns_one_opcode_into_another() {
        let opcodes = [STATUS, PUT_EVM, GET, CLEAR, ADDRESS, PUT_BITCOIN];

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
    fn a_record_larger_than_a_draft_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let payload = vec![0xA5u8; EVMRecord::CAPACITY + 1];
        let written = frame::encode(super::PUT_EVM, &payload, &mut buffer).expect("encode");
        assert_eq!(
            Request::decode(&buffer[..written]),
            Err(Error::PayloadTooLarge(EVMRecord::CAPACITY + 1))
        );
    }

    #[test]
    fn an_address_request_names_a_known_account_and_a_display_choice() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        for payload in [
            &[][..],
            &[0x00][..],
            &[0x00, 0x00, 0x00][..],
            &[0x02, 0x00][..],
            &[0x00, 0x02][..],
            &[0x01, 0xFF][..],
        ] {
            let written = frame::encode(ADDRESS, payload, &mut buffer).expect("encode");
            assert_eq!(
                Request::decode(&buffer[..written]),
                Err(Error::MalformedPayload),
                "{payload:?}"
            );
        }
    }
}
