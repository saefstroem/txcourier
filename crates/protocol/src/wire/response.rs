use crate::wire::frame;
use crate::wire::record::Address;
use crate::wire::refusal::Refusal;
use crate::wire::result::Result;
use crate::wire::transaction::Transaction;

const STATUS: u8 = 0x81;
const SLOT: u8 = 0x87;
const ACKNOWLEDGED: u8 = 0x88;
const REFUSED: u8 = 0x8E;
const ADDRESS: u8 = 0x90;

// No Refusal is numbered 0
const NO_REFUSAL: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    Status {
        state: u8,
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
    Address {
        address: Address,
    },
}

impl Response {
    pub fn encode(self, out: &mut [u8]) -> Result<usize> {
        match self {
            Self::Status {
                state,
                refusal,
                detail,
            } => {
                let reason = match refusal {
                    Some(refusal) => refusal.to_byte(),
                    None => NO_REFUSAL,
                };
                let [high, low] = detail.to_be_bytes();
                frame::encode(STATUS, &[state, reason, high, low], out)
            }
            Self::Slot { transaction } => frame::encode(SLOT, transaction.as_slice(), out),
            Self::Acknowledged => frame::encode(ACKNOWLEDGED, &[], out),
            Self::Refused { refusal, detail } => {
                let [high, low] = detail.to_be_bytes();
                frame::encode(REFUSED, &[refusal.to_byte(), high, low], out)
            }
            Self::Address { address } => frame::encode(ADDRESS, address.as_slice(), out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ACKNOWLEDGED;
    use super::ADDRESS;
    use super::Address;
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

    #[test]
    fn the_encoded_bytes_of_every_response_are_pinned() {
        let expected: [(Response, Vec<u8>); 5] = [
            (
                Response::Status {
                    state: DeviceState::Idle.to_byte(),
                    refusal: Some(Refusal::LedgerDeclined),
                    detail: 0x6985,
                },
                vec![0x52, 0x57, 0x01, 0x81, 0x04, 0x00, 0xA1, 0x0E, 0x69, 0x85],
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
            (
                Response::Address {
                    address: Address::from_slice(b"0x").expect("fits"),
                },
                vec![0x52, 0x57, 0x01, 0x90, 0x02, 0x00, 0x30, 0x78],
            ),
        ];

        for (response, bytes) in expected {
            assert_eq!(encoded(response), bytes, "{response:?}");
        }
    }

    #[test]
    fn no_single_flipped_bit_turns_one_opcode_into_another() {
        let opcodes = [STATUS, SLOT, ACKNOWLEDGED, REFUSED, ADDRESS];

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
}
