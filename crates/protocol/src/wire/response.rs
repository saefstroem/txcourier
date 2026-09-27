use crate::wire::error::Error;
use crate::wire::frame;
use crate::wire::refusal::Refusal;
use crate::wire::result::Result;
use crate::wire::transaction::Transaction;

const STATUS: u8 = 0x81;
const SLOT: u8 = 0x87;
const ACKNOWLEDGED: u8 = 0x88;
const REFUSED: u8 = 0x8E;

// No Refusal is numbered 0
const NO_REFUSAL: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    Status {
        state: u8,
        provisioned: bool,
        refusal: Option<Refusal>,
        detail: u16,
    },
    Slot {
        transaction: Transaction,
    },
    Acknowledged,
    Refused {
        refusal: Refusal,
        detail: u16,
    },
}

impl Response {
    pub const fn opcode(self) -> u8 {
        match self {
            Self::Status { .. } => STATUS,
            Self::Slot { .. } => SLOT,
            Self::Acknowledged => ACKNOWLEDGED,
            Self::Refused { .. } => REFUSED,
        }
    }

    pub fn encode(self, out: &mut [u8]) -> Result<usize> {
        match self {
            Self::Status {
                state,
                provisioned,
                refusal,
                detail,
            } => {
                let reason = match refusal {
                    Some(refusal) => refusal.to_byte(),
                    None => NO_REFUSAL,
                };
                let [high, low] = detail.to_be_bytes();
                frame::encode(
                    STATUS,
                    &[state, u8::from(provisioned), reason, high, low],
                    out,
                )
            }
            Self::Slot { transaction } => frame::encode(SLOT, transaction.as_slice(), out),
            Self::Acknowledged => frame::encode(ACKNOWLEDGED, &[], out),
            Self::Refused { refusal, detail } => {
                let [high, low] = detail.to_be_bytes();
                frame::encode(REFUSED, &[refusal.to_byte(), high, low], out)
            }
        }
    }

    pub fn decode(frame: &[u8]) -> Result<Self> {
        let decoded = frame::decode(frame)?;

        match decoded.opcode {
            STATUS => {
                let &[state, provisioned, reason, high, low] = decoded.payload else {
                    return Err(Error::MalformedPayload);
                };
                if provisioned > 1 {
                    return Err(Error::MalformedPayload);
                }
                let refusal = match reason {
                    NO_REFUSAL => None,
                    byte => match Refusal::from_byte(byte) {
                        Some(refusal) => Some(refusal),
                        None => return Err(Error::MalformedPayload),
                    },
                };
                Ok(Self::Status {
                    state,
                    provisioned: provisioned == 1,
                    refusal,
                    detail: u16::from_be_bytes([high, low]),
                })
            }
            SLOT => Transaction::from_slice(decoded.payload)
                .map(|transaction| Self::Slot { transaction }),
            ACKNOWLEDGED if decoded.payload.is_empty() => Ok(Self::Acknowledged),
            ACKNOWLEDGED => Err(Error::MalformedPayload),
            REFUSED => {
                let &[byte, high, low] = decoded.payload else {
                    return Err(Error::MalformedPayload);
                };
                let Some(refusal) = Refusal::from_byte(byte) else {
                    return Err(Error::MalformedPayload);
                };
                Ok(Self::Refused {
                    refusal,
                    detail: u16::from_be_bytes([high, low]),
                })
            }
            other => Err(Error::UnknownOpcode(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ACKNOWLEDGED;
    use super::Error;
    use super::REFUSED;
    use super::Refusal;
    use super::Response;
    use super::SLOT;
    use super::STATUS;
    use super::Transaction;
    use crate::state::DeviceState;
    use crate::wire::frame;

    fn encoded(response: Response) -> Vec<u8> {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let written = response.encode(&mut buffer).expect("encode");
        buffer[..written].to_vec()
    }

    fn every_response() -> Vec<Response> {
        vec![
            Response::Status {
                state: DeviceState::Idle.to_byte(),
                provisioned: true,
                refusal: None,
                detail: 0,
            },
            Response::Slot {
                transaction: Transaction::from_slice(&[0x02, 0xF0]).expect("fits"),
            },
            Response::Acknowledged,
            Response::Refused {
                refusal: Refusal::LedgerDeclined,
                detail: 0x6985,
            },
        ]
    }

    #[test]
    fn every_response_round_trips() {
        for response in every_response() {
            assert_eq!(Response::decode(&encoded(response)), Ok(response));
        }
    }

    #[test]
    fn the_encoded_bytes_of_every_response_are_pinned() {
        let expected: [(Response, Vec<u8>); 4] = [
            (
                Response::Status {
                    state: DeviceState::Idle.to_byte(),
                    provisioned: true,
                    refusal: Some(Refusal::LedgerDeclined),
                    detail: 0x6985,
                },
                vec![
                    0x52, 0x57, 0x01, 0x81, 0x05, 0x00, 0xA1, 0x01, 0x0E, 0x69, 0x85,
                ],
            ),
            (
                Response::Slot {
                    transaction: Transaction::from_slice(&[0x02, 0xF0]).expect("fits"),
                },
                vec![0x52, 0x57, 0x01, 0x87, 0x02, 0x00, 0x02, 0xF0],
            ),
            (
                Response::Acknowledged,
                vec![0x52, 0x57, 0x01, 0x88, 0x00, 0x00],
            ),
            (
                Response::Refused {
                    refusal: Refusal::LedgerDeclined,
                    detail: 0x6985,
                },
                vec![0x52, 0x57, 0x01, 0x8E, 0x03, 0x00, 0x0E, 0x69, 0x85],
            ),
        ];

        for (response, bytes) in expected {
            assert_eq!(encoded(response), bytes, "{response:?}");
        }
    }

    #[test]
    fn no_single_flipped_bit_turns_one_opcode_into_another() {
        let opcodes = [STATUS, SLOT, ACKNOWLEDGED, REFUSED];

        for (index, first) in opcodes.iter().enumerate() {
            for second in opcodes.iter().skip(index + 1) {
                assert!(
                    (first ^ second).count_ones() >= 2,
                    "opcodes {first:#04x} and {second:#04x} are one bit apart: the frame carries \
                     no checksum, so a single flipped bit would silently change the answer"
                );
            }
        }
    }
    #[test]
    fn the_surviving_opcodes_keep_their_wire_values() {
        assert_eq!(
            Response::Status {
                state: 0,
                provisioned: false,
                refusal: None,
                detail: 0,
            }
            .opcode(),
            0x81
        );
        assert_eq!(
            Response::Slot {
                transaction: Transaction::from_slice(&[]).expect("fits"),
            }
            .opcode(),
            0x87
        );
        assert_eq!(Response::Acknowledged.opcode(), 0x88);
        assert_eq!(
            Response::Refused {
                refusal: Refusal::LedgerDeclined,
                detail: 0,
            }
            .opcode(),
            0x8E
        );
    }

    #[test]
    fn the_retired_diagnostic_opcodes_are_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        for opcode in [0x82u8, 0x84, 0x85, 0x86] {
            let written = frame::encode(opcode, &[], &mut buffer).expect("encode");
            assert_eq!(
                Response::decode(&buffer[..written]),
                Err(Error::UnknownOpcode(opcode)),
                "{opcode:#04x}"
            );
        }
    }

    #[test]
    fn every_device_state_round_trips() {
        let states = [
            DeviceState::Idle,
            DeviceState::PendingStored,
            DeviceState::SignedStored,
        ];

        for state in states {
            let response = Response::Status {
                state: state.to_byte(),
                provisioned: true,
                refusal: Some(Refusal::LedgerDeclined),
                detail: 0x6985,
            };
            assert_eq!(Response::decode(&encoded(response)), Ok(response));
        }
    }

    #[test]
    fn an_unrecognised_state_byte_still_round_trips_so_the_host_can_report_it() {
        let response = Response::Status {
            state: 0x00,
            provisioned: false,
            refusal: None,
            detail: 0,
        };
        assert_eq!(Response::decode(&encoded(response)), Ok(response));
    }

    #[test]
    fn an_empty_status_payload_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let written = frame::encode(STATUS, &[], &mut buffer).expect("encode");
        assert_eq!(
            Response::decode(&buffer[..written]),
            Err(Error::MalformedPayload)
        );
    }

    #[test]
    fn an_overlong_status_payload_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let written = frame::encode(STATUS, &[0xA1, 0x00, 0x00, 0x00, 0x00, 0x00], &mut buffer)
            .expect("encode");
        assert_eq!(
            Response::decode(&buffer[..written]),
            Err(Error::MalformedPayload)
        );
    }

    #[test]
    fn an_unknown_refusal_byte_in_a_status_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let written =
            frame::encode(STATUS, &[0xA1, 0x00, 0xFE, 0x00, 0x00], &mut buffer).expect("encode");
        assert_eq!(
            Response::decode(&buffer[..written]),
            Err(Error::MalformedPayload)
        );
    }

    #[test]
    fn a_status_provisioned_flag_outside_a_boolean_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let written =
            frame::encode(STATUS, &[0xA1, 0x02, 0x00, 0x00, 0x00], &mut buffer).expect("encode");
        assert_eq!(
            Response::decode(&buffer[..written]),
            Err(Error::MalformedPayload)
        );
    }

    #[test]
    fn the_storage_responses_round_trip() {
        for bytes in [
            vec![],
            vec![0x02, 0xF0],
            vec![0x5Au8; Transaction::CAPACITY],
        ] {
            let response = Response::Slot {
                transaction: Transaction::from_slice(&bytes).expect("fits"),
            };
            assert_eq!(Response::decode(&encoded(response)), Ok(response));
        }

        assert_eq!(
            Response::decode(&encoded(Response::Acknowledged)),
            Ok(Response::Acknowledged)
        );

        for refusal in Refusal::ALL {
            for detail in [0u16, 0x6985, 0xFFFF] {
                let response = Response::Refused { refusal, detail };
                assert_eq!(Response::decode(&encoded(response)), Ok(response));
            }
        }
    }

    #[test]
    fn an_unknown_refusal_byte_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        for payload in [
            &[][..],
            &[0, 0, 0][..],
            &[0xFF, 0, 0][..],
            &[1][..],
            &[1, 0, 0, 0][..],
        ] {
            let written = frame::encode(REFUSED, payload, &mut buffer).expect("encode");
            assert_eq!(
                Response::decode(&buffer[..written]),
                Err(Error::MalformedPayload),
                "{payload:?}"
            );
        }
    }

    #[test]
    fn an_unknown_opcode_is_rejected() {
        let mut buffer = [0u8; frame::MAX_FRAME_LEN];
        let written = frame::encode(0x7F, &[0xA1], &mut buffer).expect("encode");
        assert_eq!(
            Response::decode(&buffer[..written]),
            Err(Error::UnknownOpcode(0x7F))
        );
    }
}
