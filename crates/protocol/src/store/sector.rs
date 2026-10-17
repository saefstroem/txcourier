use crate::store::record::{Kind, Slot, decode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A scan abstraction which contains
/// the live index of where the slot exists
/// where the next one should be written
/// and what is the next sequence and if its full
pub struct Scan {
    pub live_index: Option<usize>,
    pub next_index: usize,
    pub next_sequence: u32,
    pub full: bool,
}
/// Scan a memory sector for some record length
pub fn scan(sector: &[u8], record_len: usize) -> Scan {
    let mut newest: Option<(u32, usize, Kind)> = None;
    let mut written_through = 0usize;
    let mut count = 0usize;

    // If its empty simply return the genesis scan
    if record_len == 0 {
        return Scan {
            live_index: None,
            next_index: 0,
            next_sequence: 0,
            full: true,
        };
    }

    // Go over the sector in chunks of the record length
    for (index, bytes) in sector.chunks_exact(record_len).enumerate() {
        count = index + 1;

        match decode(bytes) {
            Slot::Erased => {}
            Slot::Corrupt => written_through = index + 1,
            Slot::Written(record) => {
                // extract the record and its metadata
                written_through = index + 1;
                let newer = match newest {
                    Some((sequence, _, _)) => record.sequence > sequence,
                    None => true,
                };
                if newer {
                    newest = Some((record.sequence, index, record.kind));
                }
            }
        }
    }

    // We can guarantee we have the newest
    // if its live then this is the live index, otherwise there is none
    let live_index = newest.and_then(|(_, index, kind)| match kind {
        Kind::Live => Some(index),
        Kind::Tombstone => None,
    });

    // compute next sequence number for the memory
    let next_sequence = newest
        .map(|(sequence, _, _)| sequence.wrapping_add(1))
        .unwrap_or(0);

    Scan {
        live_index,
        next_index: written_through,
        next_sequence,
        full: written_through >= count,
    }
}

#[cfg(test)]
mod tests {
    use super::Scan;
    use super::scan;
    use crate::store::record::Kind;
    use crate::store::record::Record;
    use crate::store::record::encode;

    const RECORD_LEN: usize = 256;
    const RECORDS: usize = 16;
    const SECTOR_LEN: usize = RECORD_LEN * RECORDS;

    fn erased() -> Vec<u8> {
        vec![0xFFu8; SECTOR_LEN]
    }

    fn write(sector: &mut [u8], index: usize, sequence: u32, kind: Kind, payload: &[u8]) {
        let start = index * RECORD_LEN;
        encode(
            &Record {
                sequence,
                kind,
                payload,
            },
            &mut sector[start..start + RECORD_LEN],
        )
        .expect("fits");
    }

    #[test]
    fn an_erased_sector_has_nothing_live_and_starts_at_zero() {
        assert_eq!(
            scan(&erased(), RECORD_LEN),
            Scan {
                live_index: None,
                next_index: 0,
                next_sequence: 0,
                full: false,
            }
        );
    }

    #[test]
    fn the_newest_live_record_wins() {
        let mut sector = erased();
        write(&mut sector, 0, 0, Kind::Live, b"first");
        write(&mut sector, 1, 1, Kind::Live, b"second");

        let found = scan(&sector, RECORD_LEN);
        assert_eq!(found.live_index, Some(1));
        assert_eq!(found.next_index, 2);
        assert_eq!(found.next_sequence, 2);
        assert!(!found.full);
    }

    #[test]
    fn a_tombstone_over_a_live_record_leaves_nothing_live() {
        let mut sector = erased();
        write(&mut sector, 0, 0, Kind::Live, b"pending");
        write(&mut sector, 1, 1, Kind::Tombstone, &[]);

        let found = scan(&sector, RECORD_LEN);
        assert_eq!(found.live_index, None);
        assert_eq!(found.next_index, 2);
        assert_eq!(found.next_sequence, 2);
    }

    #[test]
    fn a_live_record_after_a_tombstone_is_live_again() {
        let mut sector = erased();
        write(&mut sector, 0, 0, Kind::Live, b"first");
        write(&mut sector, 1, 1, Kind::Tombstone, &[]);
        write(&mut sector, 2, 2, Kind::Live, b"second");

        assert_eq!(scan(&sector, RECORD_LEN).live_index, Some(2));
    }

    #[test]
    fn the_highest_sequence_wins_even_when_it_sits_earlier() {
        let mut sector = erased();
        write(&mut sector, 0, 9, Kind::Live, b"newest");
        write(&mut sector, 1, 2, Kind::Live, b"older");

        let found = scan(&sector, RECORD_LEN);
        assert_eq!(found.live_index, Some(0));
        assert_eq!(found.next_sequence, 10);
    }

    #[test]
    fn a_corrupt_record_is_skipped_but_still_consumes_its_place() {
        let mut sector = erased();
        write(&mut sector, 0, 0, Kind::Live, b"good");
        write(&mut sector, 1, 1, Kind::Live, b"half written");
        sector[RECORD_LEN + 20] ^= 0x01;

        let found = scan(&sector, RECORD_LEN);
        assert_eq!(found.live_index, Some(0));
        assert_eq!(found.next_index, 2);
        assert_eq!(found.next_sequence, 1);
    }

    #[test]
    fn a_sector_whose_last_record_is_written_is_full() {
        let mut sector = erased();
        for index in 0..RECORDS {
            write(&mut sector, index, index as u32, Kind::Live, b"x");
        }

        let found = scan(&sector, RECORD_LEN);
        assert_eq!(found.live_index, Some(RECORDS - 1));
        assert_eq!(found.next_index, RECORDS);
        assert!(found.full);
    }

    #[test]
    fn a_gap_before_the_last_written_record_does_not_get_reused() {
        let mut sector = erased();
        write(&mut sector, 3, 3, Kind::Live, b"only");

        let found = scan(&sector, RECORD_LEN);
        assert_eq!(found.live_index, Some(3));
        assert_eq!(found.next_index, 4);
    }

    #[test]
    fn a_larger_record_size_scans_fewer_records() {
        let mut sector = vec![0xFFu8; 4096];
        encode(
            &Record {
                sequence: 5,
                kind: Kind::Live,
                payload: b"config",
            },
            &mut sector[..2048],
        )
        .expect("fits");

        let found = scan(&sector, 2048);
        assert_eq!(found.live_index, Some(0));
        assert_eq!(found.next_index, 1);
        assert_eq!(found.next_sequence, 6);
        assert!(!found.full);
    }

    #[test]
    fn a_zero_record_length_is_reported_full_rather_than_looping() {
        assert_eq!(
            scan(&erased(), 0),
            Scan {
                live_index: None,
                next_index: 0,
                next_sequence: 0,
                full: true,
            }
        );
    }
}
