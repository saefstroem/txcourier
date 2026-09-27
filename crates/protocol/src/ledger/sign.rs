use crate::ledger::apdu::MAX_DATA_LEN;
use crate::ledger::error::Error;
use crate::ledger::path::encode as encode_path;
use crate::ledger::result::Result;
use crate::transaction::{SIGNATURE_WORD_LEN, Signature};

// app-ethereum 1.22.3 src/apdu_constants.h: CLA is 0xE0. main.c rejects every other class.
pub const CLASS: u8 = 0xE0;
// app-ethereum 1.22.3 src/apdu_constants.h: INS_SIGN, SIGN ETH TRANSACTION, is 0x04.
pub const INSTRUCTION_SIGN: u8 = 0x04;
// app-ethereum 1.22.3 src/apdu_constants.h: PROVIDE ERC 20 TOKEN INFORMATION is INS 0x0A.
pub const INSTRUCTION_TOKEN_INFO: u8 = 0x0A;
// app-ethereum ethapp.adoc: INS 0x0A requires P1 = 00.
pub const NO_PARAMETER: u8 = 0x00;
// app-ethereum 1.22.3 apdu_constants.h: P1_FIRST, 0x00, marks the first SIGN chunk.
pub const FIRST_CHUNK: u8 = 0x00;
// app-ethereum 1.22.3 apdu_constants.h: P1_MORE, 0x80, marks every later SIGN chunk.
pub const LATER_CHUNK: u8 = 0x80;
// The largest Lc a one-byte length carries, and the most app-ethereum accepts per chunk
// (ethapp.adoc). Chunks are cut at that ceiling.
pub const CHUNK_MAX: usize = MAX_DATA_LEN;

// app-ethereum 1.22.3 ethapp.adoc: the SIGN reply is v 1 byte, r 32, s 32. This is v.
const RECOVERY_LEN: usize = 1;
// 1 + 2 * 32 = 65: app-ethereum 1.22.3 shared_context.h ECDSA_SIGNATURE_LENGTH, v, r and s.
const SIGNATURE_LEN: usize = RECOVERY_LEN + 2 * SIGNATURE_WORD_LEN;

/// Container for the transaction which iterator constructs the needed chunks
/// as APDU
pub struct Chunks<'a> {
    path: &'a [u32],
    transaction: &'a [u8],
    sent: usize,
    started: bool,
}

impl<'a> Chunks<'a> {
    pub const fn new(path: &'a [u32], transaction: &'a [u8]) -> Self {
        Self {
            path,
            transaction,
            sent: 0,
            started: false,
        }
    }

    pub const fn finished(&self) -> bool {
        self.started && self.sent >= self.transaction.len()
    }

    pub fn next_into(&mut self, out: &mut [u8]) -> Option<Result<(u8, usize)>> {
        // If we have already sent everything there is nothing more to iterate on.
        if self.started && self.sent >= self.transaction.len() {
            return None;
        }

        let first = !self.started;
        self.started = true;

        let mut written = 0;

        // If this is the first we need to encode the derviation path
        // we need to sign for
        if first {
            match encode_path(self.path, out) {
                Ok(len) => written = len,
                Err(error) => return Some(Err(error)),
            }
        }

        // Need to account for when we write the serialized path
        let room = CHUNK_MAX - written;
        let taken = room.min(self.transaction.len() - self.sent);

        // Extract the mutable slice 
        let Some(slot) = out.get_mut(written..written + taken) else {
            return Some(Err(Error::OutputTooSmall {
                needed: written + taken,
                found: out.len(),
            }));
        };

        // extract next part of the transaction
        let Some(source) = self.transaction.get(self.sent..self.sent + taken) else {
            return Some(Err(Error::OutputTooSmall {
                needed: self.sent + taken,
                found: self.transaction.len(),
            }));
        };

        // Copy the next part into the mutable slot
        slot.copy_from_slice(source);

        self.sent += taken;
        written += taken;

        // return written 
        Some(Ok((if first { FIRST_CHUNK } else { LATER_CHUNK }, written)))
    }
}


// Reconstructs the signature from received payload
pub fn signature(payload: &[u8]) -> Result<Signature> {
    if payload.len() != SIGNATURE_LEN {
        return Err(Error::ResponseTruncated);
    }

    // Extract v,r,s
    let (Some(&recovery), Some(r), Some(s)) = (
        payload.first(),
        payload.get(RECOVERY_LEN..RECOVERY_LEN + SIGNATURE_WORD_LEN),
        payload.get(RECOVERY_LEN + SIGNATURE_WORD_LEN..),
    ) else {
        return Err(Error::ResponseTruncated);
    };

    // Return it simply
    let (Ok(r), Ok(s)) = (
        <[u8; SIGNATURE_WORD_LEN]>::try_from(r),
        <[u8; SIGNATURE_WORD_LEN]>::try_from(s),
    ) else {
        return Err(Error::ResponseTruncated);
    };

    Ok(Signature::from_ledger(recovery, r, s)?)
}

#[cfg(test)]
mod tests {
    use super::CHUNK_MAX;
    use super::Chunks;
    use super::Error;
    use super::FIRST_CHUNK;
    use super::LATER_CHUNK;
    use super::signature;
    use crate::ledger::path::DEFAULT_ACCOUNT;
    use crate::ledger::path::PATH_LEN;
    use crate::transaction;

    fn collected(transaction: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let mut chunks = Chunks::new(&DEFAULT_ACCOUNT, transaction);
        let mut buffer = [0u8; CHUNK_MAX];
        let mut out = Vec::new();

        while let Some(step) = chunks.next_into(&mut buffer) {
            let (marker, len) = step.expect("chunk");
            out.push((marker, buffer[..len].to_vec()));
        }

        out
    }

    #[test]
    fn a_short_transaction_is_one_chunk_carrying_the_path_first() {
        let transaction = [0x02, 0xF0, 0x01];
        let chunks = collected(&transaction);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, FIRST_CHUNK);
        assert_eq!(chunks[0].1.len(), PATH_LEN + transaction.len());
        assert_eq!(chunks[0].1[0], 5);
        assert_eq!(&chunks[0].1[PATH_LEN..], &transaction);
    }

    #[test]
    fn the_reference_native_transfer_still_fits_one_chunk() {
        let transaction = vec![0xAAu8; 50];
        let chunks = collected(&transaction);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].1.len(), 71);
    }

    #[test]
    fn a_long_transaction_splits_and_only_the_first_chunk_carries_the_path() {
        let transaction: Vec<u8> = (0..600).map(|i| (i % 251) as u8).collect();
        let chunks = collected(&transaction);

        assert!(chunks.len() > 1);
        assert_eq!(chunks[0].0, FIRST_CHUNK);
        assert_eq!(chunks[0].1.len(), CHUNK_MAX);
        for chunk in chunks.iter().skip(1) {
            assert_eq!(chunk.0, LATER_CHUNK);
            assert!(chunk.1.len() <= CHUNK_MAX);
        }

        let mut rebuilt = chunks[0].1[PATH_LEN..].to_vec();
        for chunk in chunks.iter().skip(1) {
            rebuilt.extend_from_slice(&chunk.1);
        }
        assert_eq!(rebuilt, transaction);
    }

    #[test]
    fn no_chunk_ever_exceeds_one_apdu() {
        for len in [0usize, 1, 233, 234, 235, 255, 468, 469, 1000] {
            let transaction = vec![0x5Au8; len];
            for (_, data) in collected(&transaction) {
                assert!(data.len() <= CHUNK_MAX, "length {len}");
            }
        }
    }

    #[test]
    fn every_transaction_length_round_trips_through_the_chunks() {
        for len in [0usize, 1, 50, 233, 234, 470, 999] {
            let transaction: Vec<u8> = (0..len).map(|i| (i % 253) as u8).collect();
            let chunks = collected(&transaction);
            let mut rebuilt = chunks[0].1[PATH_LEN..].to_vec();
            for chunk in chunks.iter().skip(1) {
                rebuilt.extend_from_slice(&chunk.1);
            }
            assert_eq!(rebuilt, transaction, "length {len}");
        }
    }

    #[test]
    fn finished_reports_true_only_after_the_last_chunk() {
        let transaction: Vec<u8> = (0..600).map(|i| (i % 251) as u8).collect();
        let mut chunks = Chunks::new(&DEFAULT_ACCOUNT, &transaction);
        let mut buffer = [0u8; CHUNK_MAX];

        assert!(!chunks.finished());

        let mut seen = 0;
        while let Some(step) = chunks.next_into(&mut buffer) {
            step.expect("chunk");
            seen += 1;
            if chunks.finished() {
                break;
            }
        }

        assert!(chunks.finished());
        assert!(seen > 1);
        assert!(chunks.next_into(&mut buffer).is_none());
    }

    #[test]
    fn a_single_chunk_transaction_is_finished_after_one_step() {
        let mut chunks = Chunks::new(&DEFAULT_ACCOUNT, &[0x02, 0xF0]);
        let mut buffer = [0u8; CHUNK_MAX];
        chunks.next_into(&mut buffer).expect("one").expect("chunk");
        assert!(chunks.finished());
    }

    #[test]
    fn a_reference_signature_reply_parses() {
        let mut payload = vec![0x1C];
        payload.extend_from_slice(&[0x11; 32]);
        payload.extend_from_slice(&[0x22; 32]);

        let parsed = signature(&payload).expect("valid");
        assert_eq!(parsed.y_parity, 1);
        assert_eq!(parsed.r, [0x11; 32]);
        assert_eq!(parsed.s, [0x22; 32]);
    }

    #[test]
    fn both_recovery_conventions_are_accepted() {
        for (recovery, parity) in [(0u8, 0u8), (1, 1), (27, 0), (28, 1)] {
            let mut payload = vec![recovery];
            payload.extend_from_slice(&[0x01; 64]);
            assert_eq!(signature(&payload).expect("valid").y_parity, parity);
        }
    }

    #[test]
    fn a_nonsense_recovery_byte_is_refused() {
        let mut payload = vec![9];
        payload.extend_from_slice(&[0x01; 64]);
        assert_eq!(
            signature(&payload),
            Err(Error::Signature(transaction::Error::BadParity(9)))
        );
    }

    #[test]
    fn a_reply_of_the_wrong_length_is_refused() {
        assert_eq!(signature(&[]), Err(Error::ResponseTruncated));
        assert_eq!(signature(&[0x1C; 64]), Err(Error::ResponseTruncated));
        assert_eq!(signature(&[0x1C; 66]), Err(Error::ResponseTruncated));
    }
}
