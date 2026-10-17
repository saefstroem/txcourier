use crate::ledger::error::Error;
use crate::ledger::result::Result;

// Ledger transport documentation: HID chunks are 64 bytes. ledger-secure-sdk
// usbd_ledger_hid.c LEDGER_HID_EPIN_SIZE is 0x40.
pub const REPORT_LEN: usize = 64;

// Ledger transport documentation recommends channel 0101. The device echoes the field
// unchecked and ledgerjs sends a random u16. The only constraint is no leading 00 byte.
const CHANNEL: u16 = 0x0101;
// ledger-secure-sdk ledger_protocol.c: TAG_APDU is 0x05. TAG_PING, 0x02, and the other tags
// are rejected here.
const TAG_APDU: u8 = 0x05;
// ledger-secure-sdk ledger_protocol.c protocol_header_t, on every frame: channel 2, tag 1,
// sequence 2.
const HEAD_LEN: usize = 5;
// Ledger transport documentation: frame 0's payload opens with a two-byte big-endian APDU
// length, so the body starts at 7 in frame 0 and at 5 in every later frame.
const LENGTH_LEN: usize = 2;

/// A report is 64 byte pieces of an APDU
/// This is per Ledger's HID endpoint
pub struct Reports<'a> {
    apdu: &'a [u8],
    sequence: u16,
    sent: usize,
}

/// This puts it back together into an APDU
pub struct Reassembly {
    expected: Option<usize>,
    filled: usize,
    sequence: u16,
}

impl<'a> Reports<'a> {
    pub const fn new(apdu: &'a [u8]) -> Self {
        Self {
            apdu,
            sequence: 0,
            sent: 0,
        }
    }
}

impl Iterator for Reports<'_> {
    type Item = [u8; REPORT_LEN];

    fn next(&mut self) -> Option<Self::Item> {
        if self.sequence > 0 && self.sent >= self.apdu.len() {
            return None;
        }

        let mut report = [0u8; REPORT_LEN];
        let [channel_high, channel_low] = CHANNEL.to_be_bytes();
        let [sequence_high, sequence_low] = self.sequence.to_be_bytes();

        let head: [u8; HEAD_LEN] = [
            channel_high,
            channel_low,
            TAG_APDU,
            sequence_high,
            sequence_low,
        ];
        if let Some(slot) = report.get_mut(..HEAD_LEN) {
            // copy the header into the slot
            slot.copy_from_slice(&head);
        }

        // compute where the body starts
        let body_at = if self.sequence == 0 {
            let declared = (self.apdu.len() as u16).to_be_bytes();
            if let Some(slot) = report.get_mut(HEAD_LEN..HEAD_LEN + LENGTH_LEN) {
                slot.copy_from_slice(&declared);
            }
            HEAD_LEN + LENGTH_LEN
        } else {
            HEAD_LEN
        };

        // how much is left
        let room = REPORT_LEN - body_at;
        // how much is taken
        let taken = room.min(self.apdu.len() - self.sent);

        // extract how much has been sent and serialize the next
        // part of the payload into the report mutable bytes
        if let (Some(slot), Some(source)) = (
            report.get_mut(body_at..body_at + taken),
            self.apdu.get(self.sent..self.sent + taken),
        ) {
            slot.copy_from_slice(source);
        }

        // increment counters
        self.sent += taken;
        self.sequence += 1;

        // return the finalized report ready to be sent
        Some(report)
    }
}

impl Default for Reassembly {
    fn default() -> Self {
        Self::new()
    }
}

impl Reassembly {
    pub const fn new() -> Self {
        Self {
            expected: None,
            filled: 0,
            sequence: 0,
        }
    }

    /// Recomputes and pushes each 64 byte section onto the mutable bytes
    pub fn push(&mut self, report: &[u8], out: &mut [u8]) -> Result<Option<usize>> {
        // Ensure the size is correct
        if report.len() != REPORT_LEN {
            return Err(Error::ReportMalformed);
        }

        // Extract key values
        let (Some(channel), Some(tag), Some(sequence)) = (
            report.get(..2),
            report.get(2).copied(),
            report.get(3..HEAD_LEN),
        ) else {
            return Err(Error::ReportMalformed);
        };

        let (Ok(channel), Ok(sequence)) =
            (<[u8; 2]>::try_from(channel), <[u8; 2]>::try_from(sequence))
        else {
            return Err(Error::ReportMalformed);
        };

        let channel = u16::from_be_bytes(channel);
        if channel != CHANNEL {
            return Err(Error::WrongChannel(channel));
        }
        if tag != TAG_APDU {
            return Err(Error::WrongTag(tag));
        }

        let sequence = u16::from_be_bytes(sequence);
        if sequence != self.sequence {
            return Err(Error::OutOfSequence {
                expected: self.sequence,
                found: sequence,
            });
        }


        let body_at = if sequence == 0 {
            let Some(declared) = report.get(HEAD_LEN..HEAD_LEN + LENGTH_LEN) else {
                return Err(Error::ReportMalformed);
            };
            let Ok(declared) = <[u8; 2]>::try_from(declared) else {
                return Err(Error::ReportMalformed);
            };
            let declared = usize::from(u16::from_be_bytes(declared));
            if declared > out.len() {
                return Err(Error::ResponseTooLong(declared));
            }
            self.expected = Some(declared);
            HEAD_LEN + LENGTH_LEN // the first report carries total length
        } else {
            HEAD_LEN // all the other reports carry data at 5
        };

        let Some(expected) = self.expected else {
            return Err(Error::ResponseTruncated);
        };

        let room = REPORT_LEN - body_at;
        let taken = room.min(expected - self.filled);

        if let (Some(slot), Some(source)) = (
            out.get_mut(self.filled..self.filled + taken),
            report.get(body_at..body_at + taken),
        ) {
            slot.copy_from_slice(source);
        }

        self.filled += taken;
        self.sequence += 1;

        if self.filled >= expected {
            Ok(Some(expected))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::HEAD_LEN;
    use super::LENGTH_LEN;
    use super::REPORT_LEN;
    use super::Reassembly;
    use super::Reports;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
            .collect()
    }

    fn round_trip(apdu: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; 4096];
        let mut assembly = Reassembly::new();
        let mut total = 0;

        for report in Reports::new(apdu) {
            if let Some(len) = assembly.push(&report, &mut out).expect("valid") {
                total = len;
            }
        }

        out.truncate(total);
        out
    }

    #[test]
    fn a_get_address_command_frames_into_the_reference_report() {
        let apdu = hex("e002000015058000002c8000003c800000000000000000000000");
        let reports: Vec<_> = Reports::new(&apdu).collect();
        assert_eq!(reports.len(), 1);
        assert_eq!(
            reports[0].to_vec(),
            hex(
                "0101050000001ae002000015058000002c8000003c80000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
            )
        );
    }

    #[test]
    fn a_sign_command_frames_into_the_two_reference_reports() {
        let apdu = hex(
            "e004000047058000002c8000003c80000000000000000000000002f00180843b9aca008504a817c80082520894d8da6bf26964af9d7eed9e03e53415d37aa96045880de0b6b3a764000080c0",
        );
        let reports: Vec<_> = Reports::new(&apdu).collect();
        assert_eq!(reports.len(), 2);
        assert_eq!(
            reports[0].to_vec(),
            hex(
                "0101050000004ce004000047058000002c8000003c80000000000000000000000002f00180843b9aca008504a817c80082520894d8da6bf26964af9d7eed9e03"
            )
        );
        assert_eq!(
            reports[1].to_vec(),
            hex(
                "0101050001e53415d37aa96045880de0b6b3a764000080c000000000000000000000000000000000000000000000000000000000000000000000000000000000"
            )
        );
    }

    #[test]
    fn a_signature_reply_reassembles_from_its_two_reports() {
        let first = hex(
            "010105000000431c1111111111111111111111111111111111111111111111111111111111111111222222222222222222222222222222222222222222222222",
        );
        let second = hex(
            "01010500012222222222222222900000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        );

        let mut out = [0u8; 128];
        let mut assembly = Reassembly::new();
        assert_eq!(assembly.push(&first, &mut out).expect("valid"), None);
        let len = assembly
            .push(&second, &mut out)
            .expect("valid")
            .expect("complete");

        assert_eq!(len, 67);
        assert_eq!(out[0], 0x1C);
        assert_eq!(&out[1..33], &[0x11; 32]);
        assert_eq!(&out[33..65], &[0x22; 32]);
        assert_eq!(&out[65..67], &[0x90, 0x00]);
    }

    #[test]
    fn every_length_round_trips_across_the_frame_boundaries() {
        let first = REPORT_LEN - HEAD_LEN - LENGTH_LEN;
        let later = REPORT_LEN - HEAD_LEN;

        for len in [
            0usize,
            1,
            first - 1,
            first,
            first + 1,
            first + later,
            first + later + 1,
            500,
        ] {
            let apdu: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            assert_eq!(round_trip(&apdu), apdu, "length {len}");
        }
    }

    #[test]
    fn an_empty_apdu_still_produces_one_report() {
        let reports: Vec<_> = Reports::new(&[]).collect();
        assert_eq!(reports.len(), 1);
        assert_eq!(
            &reports[0][..7],
            &[0x01, 0x01, 0x05, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn a_report_for_another_channel_or_tag_is_rejected() {
        let mut report = [0u8; REPORT_LEN];
        report[..7].copy_from_slice(&[0x02, 0x02, 0x05, 0x00, 0x00, 0x00, 0x01]);
        let mut out = [0u8; 64];
        assert_eq!(
            Reassembly::new().push(&report, &mut out),
            Err(Error::WrongChannel(0x0202))
        );

        report[..7].copy_from_slice(&[0x01, 0x01, 0x06, 0x00, 0x00, 0x00, 0x01]);
        assert_eq!(
            Reassembly::new().push(&report, &mut out),
            Err(Error::WrongTag(0x06))
        );
    }

    #[test]
    fn reports_arriving_out_of_order_are_rejected() {
        let mut report = [0u8; REPORT_LEN];
        report[..7].copy_from_slice(&[0x01, 0x01, 0x05, 0x00, 0x07, 0x00, 0x01]);
        let mut out = [0u8; 64];
        assert_eq!(
            Reassembly::new().push(&report, &mut out),
            Err(Error::OutOfSequence {
                expected: 0,
                found: 7
            })
        );
    }

    #[test]
    fn a_reply_larger_than_the_buffer_is_refused_before_it_overruns() {
        let mut report = [0u8; REPORT_LEN];
        report[..7].copy_from_slice(&[0x01, 0x01, 0x05, 0x00, 0x00, 0x10, 0x00]);
        let mut out = [0u8; 64];
        assert_eq!(
            Reassembly::new().push(&report, &mut out),
            Err(Error::ResponseTooLong(0x1000))
        );
    }

    #[test]
    fn a_report_of_the_wrong_size_is_rejected() {
        let mut out = [0u8; 64];
        assert_eq!(
            Reassembly::new().push(&[0u8; 32], &mut out),
            Err(Error::ReportMalformed)
        );
    }
}
