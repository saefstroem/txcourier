use crate::error::Error;
use crate::result::Result;
use crate::transmit::ADDRESS_DRIVE_J as SYMBOL_DRIVE_J;
use crate::transmit::ADDRESS_DRIVE_K as SYMBOL_DRIVE_K;
use crate::transmit::ADDRESS_END_OF_PACKET as SYMBOL_END_OF_PACKET;
use crate::transmit::ADDRESS_RELEASE_BUS as SYMBOL_RELEASE_BUS;

// BITS_PER_BYTE / BITS_PER_SYMBOL: four symbols pack into one byte of the DMA stream.
const SYMBOLS_PER_BYTE: usize = 4;
// Matches out pc, 2 in the transmit program: a symbol is a two-bit jump target.
const BITS_PER_SYMBOL: u32 = 2;
// USB 2.0 section 8.1: bytes are sent least significant bit first.
const BITS_PER_BYTE: usize = 8;

// USB 2.0 section 7.1.9: a zero is inserted after six consecutive ones, before NRZI, to
// force a transition.
pub const ONES_BEFORE_STUFFING: u8 = 6;

// One symbol per packet. Address 0 expands it into two SE0 bit times and the trailing J.
const END_OF_PACKET_SYMBOLS: usize = 1;
// One release symbol ends the burst, tri-stating the drivers one bit time after the SE0.
const RELEASE_SYMBOLS: usize = 1;
// USB 2.0 section 7.1.18.1: at least two bit times of J between an EOP and the next packet.
// The EOP drives one; three more is margin above the minimum.
const INTER_PACKET_IDLE_SYMBOLS: usize = 3;

// SYNC and ACK pre-encoded, so a handshake costs one exec and one DMA start. The two leading
// idle-J symbols and the pad to eight bytes are defined by this crate.
pub const ACKNOWLEDGE_ENCODED: [u8; 8] = [0x5D, 0xDD, 0xF5, 0xD7, 0xF2, 0x55, 0x55, 0x55];

pub const fn encoded_len(raw_len: usize) -> usize {
    encoded_sequence_len(raw_len, 1)
}

/// Compute the encoded sequence length of raw_len packets and number of packets
pub const fn encoded_sequence_len(raw_len: usize, packets: usize) -> usize {
    let gaps = packets.saturating_sub(1) * INTER_PACKET_IDLE_SYMBOLS; // compute total gap size
    let ends = packets * END_OF_PACKET_SYMBOLS; // total EOP

    // compute worst case amout of symbols
    let worst_case_symbols = raw_len * BITS_PER_BYTE // every bit becoems j or k
        + raw_len * BITS_PER_BYTE / ONES_BEFORE_STUFFING as usize // worst case is all 1's
        + ends
        + gaps
        + RELEASE_SYMBOLS; // the release that hands the bus back

    worst_case_symbols.div_ceil(SYMBOLS_PER_BYTE) // each symbol is 2 bits
    // so this is the worst case estimate for the packet
}


pub fn encode(raw: &[u8], out: &mut [u8]) -> Result<usize> {
    encode_sequence(&[raw], out)
}

/// Encode a number of packets 
pub fn encode_sequence(packets: &[&[u8]], out: &mut [u8]) -> Result<usize> {
    // compute worst case size of the packet
    let needed = sequence_len(packets);

    // we need to ensure the allocated buffer is enough for the worst case size
    if out.len() < needed {
        return Err(Error::OutputTooSmall {
            needed,
            found: out.len(),
        });
    }

    let mut writer = Writer::new(out);

    // for each packet
    for (index, packet) in packets.iter().enumerate() {
        // encode the packet
        encode_packet(packet, &mut writer)?;

        // after each packet add the triple J gap
        if index + 1 < packets.len() {
            for _ in 0..INTER_PACKET_IDLE_SYMBOLS {
                writer.push(SYMBOL_DRIVE_J)?;
            }
        }
    }

    // we push the signal that we are done transmitting
    writer.push(SYMBOL_RELEASE_BUS)?;

    // push harmless J if we are unaligned, it will be discarded
    // by reader
    while !writer.is_byte_aligned() {
        writer.push(SYMBOL_DRIVE_J)?;
    }

    Ok(writer.finish())
}

/// Compute the sequence length of packets
fn sequence_len(packets: &[&[u8]]) -> usize {
    let raw_len: usize = packets.iter().map(|packet| packet.len()).sum();
    encoded_sequence_len(raw_len, packets.len())
}

/// converts bits to J/K signals 
fn encode_packet(raw: &[u8], writer: &mut Writer<'_>) -> Result<()> {
    let mut line = SYMBOL_DRIVE_J;
    let mut ones_remaining = ONES_BEFORE_STUFFING;

    // go over each bytes
    for byte in raw {
        // for each bits in the byte 
        for position in 0..BITS_PER_BYTE {
            // shift by position to get is to MSB and mask it to check if 1
            let is_one = byte >> position & 1 == 1;

            // if its one we keep J
            if is_one {
                writer.push(line)?;
                ones_remaining -= 1; // decrement number of ones

                // if one remaining i.e. 6 have passed we need to stuff fake bit
                if ones_remaining == 0 {
                    line = opposite(line);
                    writer.push(line)?;
                    ones_remaining = ONES_BEFORE_STUFFING;
                }
            } else {
                // either just push zero but keep same amount of ones remaining
                line = opposite(line);
                writer.push(line)?;
                ones_remaining = ONES_BEFORE_STUFFING;
            }
        }
    }

    // push end of packet
    writer.push(SYMBOL_END_OF_PACKET)
}

// inverts line
const fn opposite(line: u8) -> u8 {
    if line == SYMBOL_DRIVE_J {
        SYMBOL_DRIVE_K
    } else {
        SYMBOL_DRIVE_J
    }
}

struct Writer<'a> {
    out: &'a mut [u8],
    symbols: usize,
}

impl<'a> Writer<'a> {
    const fn new(out: &'a mut [u8]) -> Self {
        Self { out, symbols: 0 }
    }

    const fn is_byte_aligned(&self) -> bool {
        self.symbols.is_multiple_of(SYMBOLS_PER_BYTE)
    }

    const fn finish(&self) -> usize {
        // returns how many bytes of out we have written symbols into
        self.symbols.div_ceil(SYMBOLS_PER_BYTE)
    }

    fn push(&mut self, symbol: u8) -> Result<()> {
        // compute which byte we will write to
        let index = self.symbols / SYMBOLS_PER_BYTE;
        // since each symbol is 2 bits and we write one bit
        // at a time we compute which position in the byte we set
        let slot = self.symbols % SYMBOLS_PER_BYTE;

        // get mutable byte at computed index
        let Some(byte) = self.out.get_mut(index) else {
            return Err(Error::OutputTooSmall {
                needed: index + 1,
                found: self.out.len(),
            });
        };

        // the bus reuses same buffer so if we are starting
        // a new byte we should clear it first
        if slot == 0 {
            *byte = 0;
        }

        // the shift is 4-1 to get the index then subtracting the slot to compute
        // bit position
        let shift = (SYMBOLS_PER_BYTE - 1 - slot) as u32 * BITS_PER_SYMBOL;
        *byte |= symbol << shift;

        // increment number of symbols
        self.symbols += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ACKNOWLEDGE_ENCODED;
    use super::INTER_PACKET_IDLE_SYMBOLS;
    use super::encode;
    use super::encode_sequence;
    use super::encoded_len;
    use super::encoded_sequence_len;

    const ACKNOWLEDGE_LEADING_IDLE_SYMBOLS: usize = 2;

    const SETUP_TOKEN: [u8; 4] = [0x80, 0x2D, 0x00, 0x10];
    const GET_DESCRIPTOR_DATA0: [u8; 12] = [
        0x80, 0xC3, 0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x08, 0x00, 0xEB, 0x94,
    ];

    fn encoded(raw: &[u8]) -> Vec<u8> {
        let mut buffer = vec![0u8; encoded_len(raw.len())];
        let written = encode(raw, &mut buffer).expect("encode");
        buffer.truncate(written);
        buffer
    }

    fn encoded_sequence(packets: &[&[u8]]) -> Vec<u8> {
        let mut buffer = vec![0u8; 256];
        let written = encode_sequence(packets, &mut buffer).expect("encode");
        buffer.truncate(written);
        buffer
    }

    #[test]
    fn an_empty_packet_is_a_bare_end_of_packet() {
        assert_eq!(encoded(&[]), [0x25]);
    }

    #[test]
    fn the_sync_field_alone_matches_the_reference_encoder() {
        assert_eq!(encoded(&[0x80]), [0xDD, 0xDF, 0x25]);
    }

    #[test]
    fn an_acknowledge_packet_matches_the_reference_encoder() {
        assert_eq!(encoded(&[0x80, 0xD2]), [0xDD, 0xDF, 0x5D, 0x7F, 0x25]);
    }

    #[test]
    fn the_pre_encoded_acknowledge_leads_with_idle_bits_then_the_encoder_output() {
        let mut expected = vec![1u8; ACKNOWLEDGE_LEADING_IDLE_SYMBOLS];
        expected.extend(symbols(&encoded(&[0x80, 0xD2])));
        let line = symbols(&ACKNOWLEDGE_ENCODED);
        assert_eq!(&line[..expected.len()], expected.as_slice());
        for padding in &line[expected.len()..] {
            assert_eq!(*padding, 1);
        }
    }

    #[test]
    fn a_negative_acknowledge_packet_matches_the_reference_encoder() {
        assert_eq!(encoded(&[0x80, 0x5A]), [0xDD, 0xDF, 0x5F, 0xD7, 0x25]);
    }

    #[test]
    fn a_setup_token_matches_the_reference_encoder() {
        assert_eq!(
            encoded(&SETUP_TOKEN),
            [0xDD, 0xDF, 0xD5, 0xF7, 0x77, 0x77, 0x77, 0xDD, 0x25]
        );
    }

    #[test]
    fn an_in_token_matches_the_reference_encoder() {
        assert_eq!(
            encoded(&[0x80, 0x69, 0x00, 0x10]),
            [0xDD, 0xDF, 0xDF, 0x57, 0x77, 0x77, 0x77, 0xDD, 0x25]
        );
    }

    #[test]
    fn the_get_descriptor_data_packet_matches_the_reference_encoder() {
        assert_eq!(
            encoded(&GET_DESCRIPTOR_DATA0),
            [
                0xDD, 0xDF, 0xF7, 0x7F, 0x77, 0x75, 0xFD, 0xDD, 0xDD, 0xDD, 0x77, 0x77, 0x77, 0x77,
                0x77, 0x77, 0x75, 0xDD, 0xDD, 0xDD, 0x5F, 0x55, 0xD7, 0xDF, 0x25
            ]
        );
    }

    #[test]
    fn a_single_packet_sequence_is_the_plain_encoding() {
        for raw in [&[][..], &[0x80, 0xD2][..], &SETUP_TOKEN[..]] {
            assert_eq!(encoded_sequence(&[raw]), encoded(raw));
        }
    }

    #[test]
    fn a_sequence_joins_packets_with_idle_bits_and_releases_once() {
        let line = symbols(&encoded_sequence(&[&SETUP_TOKEN, &GET_DESCRIPTOR_DATA0]));

        let token = symbols(&encoded(&SETUP_TOKEN));
        let token_end = token.iter().position(|s| *s == 0).expect("eop");
        let data = symbols(&encoded(&GET_DESCRIPTOR_DATA0));
        let data_end = data.iter().position(|s| *s == 0).expect("eop");

        let mut expected = token[..=token_end].to_vec();
        expected.extend(std::iter::repeat_n(1u8, INTER_PACKET_IDLE_SYMBOLS));
        expected.extend_from_slice(&data[..=data_end]);
        expected.push(2);

        assert_eq!(&line[..expected.len()], expected.as_slice());
        for padding in &line[expected.len()..] {
            assert_eq!(*padding, 1);
        }
    }

    #[test]
    fn six_consecutive_ones_insert_exactly_one_stuff_symbol() {
        assert_eq!(data_symbols(&[0x00]), 8);
        assert_eq!(data_symbols(&[0xFF]), 9);
        assert_eq!(data_symbols(&[0xFF, 0xFF]), 18);
    }

    #[test]
    fn the_line_never_holds_still_for_more_than_seven_bit_times() {
        for raw in [
            vec![0xFF],
            vec![0xFF, 0xFF],
            vec![0xFF, 0xFF, 0xFF, 0xFF],
            vec![0x80, 0xFF, 0xFF],
        ] {
            let line = symbols(&encoded(&raw));
            let mut run = 0usize;
            let mut previous = None;

            for symbol in line.iter().take_while(|s| **s == 1 || **s == 3) {
                if Some(*symbol) == previous {
                    run += 1;
                } else {
                    run = 1;
                    previous = Some(*symbol);
                }
                assert!(run <= 7, "{run} identical symbols in a row for {raw:02x?}");
            }
        }
    }

    #[test]
    fn every_packet_ends_with_end_of_packet_then_release() {
        for raw in [vec![], vec![0x80], vec![0x80, 0xD2], vec![0xFF; 16]] {
            let line = symbols(&encoded(&raw));
            let end = line
                .iter()
                .position(|symbol| *symbol == 0)
                .expect("an end-of-packet symbol");
            assert_eq!(line.get(end + 1), Some(&2), "release must follow the EOP");
            for padding in line.iter().skip(end + 2) {
                assert_eq!(*padding, 1, "padding must be idle-J symbols");
            }
        }
    }

    #[test]
    fn a_token_followed_by_a_full_report_fits_its_sequence_estimate() {
        let token = [0x80u8, 0xE1, 0x01, 0x28];
        let report = vec![0xFFu8; 68];
        let estimate = encoded_sequence_len(token.len() + report.len(), 2);

        let mut buffer = vec![0u8; estimate];
        let written = encode_sequence(&[&token, &report], &mut buffer).expect("fits");
        assert!(written <= estimate);
        assert!(estimate > encoded_len(token.len() + report.len()));
    }

    #[test]
    fn the_sequence_estimate_is_never_exceeded() {
        for first in [0usize, 4, 12] {
            for second in [0usize, 1, 68, 100] {
                for fill in [0x00u8, 0xFF, 0xA5] {
                    let a = vec![fill; first];
                    let b = vec![fill; second];
                    let estimate = encoded_sequence_len(first + second, 2);
                    let mut buffer = vec![0u8; estimate];
                    let written = encode_sequence(&[&a, &b], &mut buffer).expect("fits");
                    assert!(
                        written <= estimate,
                        "{first}+{second} of {fill:#04x} needed {written}, estimate {estimate}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_length_estimate_is_never_exceeded() {
        for length in 0..128usize {
            for fill in [0x00u8, 0xFF, 0xA5, 0x5A] {
                let raw = vec![fill; length];
                let mut buffer = vec![0u8; encoded_len(length)];
                let written = encode(&raw, &mut buffer).expect("encode");
                assert!(
                    written <= encoded_len(length),
                    "{length} bytes of {fill:#04x} needed {written}, estimate was {}",
                    encoded_len(length)
                );
            }
        }
    }

    #[test]
    fn a_short_buffer_is_refused_rather_than_truncated() {
        let mut buffer = [0u8; 1];
        assert!(encode(&[0x80, 0xD2], &mut buffer).is_err());
        assert!(encode_sequence(&[&[0x80], &[0x80]], &mut buffer).is_err());
    }

    fn data_symbols(raw: &[u8]) -> usize {
        symbols(&encoded(raw))
            .iter()
            .position(|symbol| *symbol == 0)
            .expect("an end-of-packet symbol")
    }

    fn symbols(encoded: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for byte in encoded {
            for slot in 0..4 {
                out.push(byte >> ((3 - slot) * 2) & 0b11);
            }
        }
        out
    }
}
