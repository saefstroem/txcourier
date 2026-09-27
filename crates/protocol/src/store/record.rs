use crate::crc32::checksum;
use crate::store::error::Error;
use crate::store::result::Result;

// A u32 sequence, a kind byte, a reserved byte and a u16 payload length.
pub const HEADER_LEN: usize = 8;
// The width of the CRC-32 crc32::checksum returns.
pub const CHECKSUM_LEN: usize = 4;

// The exact complement of the tombstone byte. Programming only clears bits, so no later
// write can turn one kind into the other.
const KIND_LIVE: u8 = 0x5A;
// The exact complement of the live byte, eight bits away. An unprogrammed 0xFF is not a
// valid kind.
const KIND_TOMBSTONE: u8 = 0xA5;
// Erased flash reads 0xFF, so a header only half programmed fails this check on its own.
const RESERVED: u8 = 0x00;
// W25Q16JV datasheet: a sector erase leaves every cell at 1, so an untouched slot reads 0xFF.
const ERASED: u8 = 0xFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Live = KIND_LIVE,
    Tombstone = KIND_TOMBSTONE,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record<'a> {
    pub sequence: u32,
    pub kind: Kind,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot<'a> {
    Erased,
    Corrupt,
    Written(Record<'a>),
}

impl Kind {
    const fn to_byte(self) -> u8 {
        self as u8
    }

    const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            KIND_LIVE => Some(Self::Live),
            KIND_TOMBSTONE => Some(Self::Tombstone),
            _ => None,
        }
    }
}

pub const fn payload_capacity(record_len: usize) -> usize {
    record_len.saturating_sub(HEADER_LEN + CHECKSUM_LEN)
}

pub fn encode(record: &Record<'_>, out: &mut [u8]) -> Result<()> {
    if out.len() < HEADER_LEN + CHECKSUM_LEN {
        return Err(Error::RecordTooSmall(out.len()));
    }

    let capacity = payload_capacity(out.len());
    if record.payload.len() > capacity {
        return Err(Error::PayloadTooLarge {
            len: record.payload.len(),
            capacity,
        });
    }

    out.fill(0);

    let Some([sequence @ .., kind, reserved, length_low, length_high]) =
        out.first_chunk_mut::<HEADER_LEN>()
    else {
        return Err(Error::RecordTooSmall(out.len()));
    };
    *sequence = record.sequence.to_le_bytes();
    *kind = record.kind.to_byte();
    *reserved = RESERVED;
    [*length_low, *length_high] = (record.payload.len() as u16).to_le_bytes();

    let Some(body) = out.get_mut(HEADER_LEN..HEADER_LEN + record.payload.len()) else {
        return Err(Error::RecordTooSmall(out.len()));
    };
    body.copy_from_slice(record.payload);

    let checked = out.len() - CHECKSUM_LEN;
    let Some(covered) = out.get(..checked) else {
        return Err(Error::RecordTooSmall(out.len()));
    };
    let crc = checksum(covered).to_le_bytes();

    let Some(tail) = out.get_mut(checked..) else {
        return Err(Error::RecordTooSmall(out.len()));
    };
    tail.copy_from_slice(&crc);

    Ok(())
}

pub fn decode(bytes: &[u8]) -> Slot<'_> {
    if bytes.len() < HEADER_LEN + CHECKSUM_LEN {
        return Slot::Corrupt;
    }

    if bytes.iter().all(|byte| *byte == ERASED) {
        return Slot::Erased;
    }

    let checked = bytes.len() - CHECKSUM_LEN;
    let (Some(covered), Some(declared)) = (bytes.get(..checked), bytes.get(checked..)) else {
        return Slot::Corrupt;
    };
    if declared != checksum(covered).to_le_bytes() {
        return Slot::Corrupt;
    }

    let Some(&[sequence @ .., kind, RESERVED, length_low, length_high]) =
        bytes.first_chunk::<HEADER_LEN>()
    else {
        return Slot::Corrupt;
    };

    let Some(kind) = Kind::from_byte(kind) else {
        return Slot::Corrupt;
    };

    let length = usize::from(u16::from_le_bytes([length_low, length_high]));
    let Some(payload) = bytes.get(HEADER_LEN..HEADER_LEN + length) else {
        return Slot::Corrupt;
    };

    Slot::Written(Record {
        sequence: u32::from_le_bytes(sequence),
        kind,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use crate::crc32::checksum;

    use super::Error;
    use super::Kind;
    use super::Record;
    use super::Slot;
    use super::decode;
    use super::encode;
    use super::payload_capacity;

    const RECORD_LEN: usize = 256;

    fn written(payload: &[u8], kind: Kind, sequence: u32) -> [u8; RECORD_LEN] {
        let mut bytes = [0u8; RECORD_LEN];
        encode(
            &Record {
                sequence,
                kind,
                payload,
            },
            &mut bytes,
        )
        .expect("fits");
        bytes
    }

    #[test]
    fn the_header_is_the_eight_bytes_already_written_to_flash() {
        assert_eq!(
            &written(b"ab", Kind::Live, 0x0403_0201)[..8],
            &[0x01, 0x02, 0x03, 0x04, 0x5A, 0x00, 0x02, 0x00]
        );
        assert_eq!(
            &written(&[], Kind::Tombstone, 1)[..8],
            &[0x01, 0x00, 0x00, 0x00, 0xA5, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn a_record_round_trips() {
        let bytes = written(b"unsigned transaction", Kind::Live, 7);
        assert_eq!(
            decode(&bytes),
            Slot::Written(Record {
                sequence: 7,
                kind: Kind::Live,
                payload: b"unsigned transaction",
            })
        );
    }

    #[test]
    fn a_tombstone_round_trips_and_carries_no_payload() {
        let bytes = written(&[], Kind::Tombstone, 9);
        assert_eq!(
            decode(&bytes),
            Slot::Written(Record {
                sequence: 9,
                kind: Kind::Tombstone,
                payload: &[],
            })
        );
    }

    #[test]
    fn an_erased_slot_is_recognised() {
        assert_eq!(decode(&[0xFF; RECORD_LEN]), Slot::Erased);
    }

    #[test]
    fn a_payload_at_capacity_round_trips_and_one_more_is_refused() {
        let capacity = payload_capacity(RECORD_LEN);
        assert_eq!(capacity, 244);

        let payload = vec![0xA5u8; capacity];
        let bytes = written(&payload, Kind::Live, 0);
        assert_eq!(
            decode(&bytes),
            Slot::Written(Record {
                sequence: 0,
                kind: Kind::Live,
                payload: &payload,
            })
        );

        let mut buffer = [0u8; RECORD_LEN];
        assert_eq!(
            encode(
                &Record {
                    sequence: 0,
                    kind: Kind::Live,
                    payload: &vec![0u8; capacity + 1],
                },
                &mut buffer
            ),
            Err(Error::PayloadTooLarge {
                len: capacity + 1,
                capacity
            })
        );
    }

    #[test]
    fn every_single_bit_flip_is_detected() {
        let original = written(b"pending", Kind::Live, 3);
        for index in 0..original.len() {
            for bit in 0..8 {
                let mut corrupted = original;
                corrupted[index] ^= 1 << bit;
                assert_ne!(
                    decode(&corrupted),
                    decode(&original),
                    "byte {index} bit {bit}"
                );
            }
        }
    }

    #[test]
    fn a_partially_written_record_is_corrupt_not_erased() {
        let mut bytes = [0xFFu8; RECORD_LEN];
        bytes[0] = 0x01;
        assert_eq!(decode(&bytes), Slot::Corrupt);
    }

    #[test]
    fn an_unknown_kind_or_dirty_reserved_byte_is_corrupt() {
        let mut bytes = written(b"x", Kind::Live, 1);
        bytes[4] = 0x11;
        let checked = bytes.len() - 4;
        let crc = checksum(&bytes[..checked]).to_le_bytes();
        bytes[checked..].copy_from_slice(&crc);
        assert_eq!(decode(&bytes), Slot::Corrupt);

        let mut bytes = written(b"x", Kind::Live, 1);
        bytes[5] = 0x11;
        let crc = checksum(&bytes[..checked]).to_le_bytes();
        bytes[checked..].copy_from_slice(&crc);
        assert_eq!(decode(&bytes), Slot::Corrupt);
    }

    #[test]
    fn a_length_beyond_the_record_is_corrupt() {
        let mut bytes = written(b"x", Kind::Live, 1);
        bytes[6..8].copy_from_slice(&(u16::MAX).to_le_bytes());
        let checked = bytes.len() - 4;
        let crc = checksum(&bytes[..checked]).to_le_bytes();
        bytes[checked..].copy_from_slice(&crc);
        assert_eq!(decode(&bytes), Slot::Corrupt);
    }

    #[test]
    fn a_record_too_small_for_its_header_is_refused_and_reads_as_corrupt() {
        let mut buffer = [0u8; 8];
        assert_eq!(
            encode(
                &Record {
                    sequence: 0,
                    kind: Kind::Live,
                    payload: &[]
                },
                &mut buffer
            ),
            Err(Error::RecordTooSmall(8))
        );
        assert_eq!(decode(&buffer), Slot::Corrupt);
        assert_eq!(decode(&[]), Slot::Corrupt);
    }

    #[test]
    fn a_larger_record_size_gives_a_larger_capacity() {
        assert_eq!(payload_capacity(2048), 2036);
        let payload = vec![0x5Au8; 2036];
        let mut bytes = [0u8; 2048];
        encode(
            &Record {
                sequence: 1,
                kind: Kind::Live,
                payload: &payload,
            },
            &mut bytes,
        )
        .expect("fits");
        assert_eq!(
            decode(&bytes),
            Slot::Written(Record {
                sequence: 1,
                kind: Kind::Live,
                payload: &payload,
            })
        );
    }
}
