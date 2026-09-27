use crate::rlp::error::Error;
use crate::rlp::result::Result;

// Yellow Paper Appendix B: a single byte below 128 is its own encoding.
const SINGLE_BYTE_MAX: u8 = 0x7F;
// Yellow Paper Appendix B, R_b: a byte array shorter than 56 bytes is prefixed by 128 plus
// its length.
const SHORT_BYTES_BASE: u8 = 0x80;
// Yellow Paper Appendix B, R_b: a longer byte array is prefixed by 183 plus the length of
// its length.
const LONG_BYTES_BASE: u8 = 0xB7;
// Yellow Paper Appendix B, R_l: a list payload shorter than 56 bytes is prefixed by 192 plus
// its length.
const SHORT_LIST_BASE: u8 = 0xC0;
// Yellow Paper Appendix B, R_l: a longer list payload is prefixed by 247 plus the length of
// its length.
const LONG_LIST_BASE: u8 = 0xF7;
// Yellow Paper Appendix B: the short forms cover lengths below 56.
const SHORT_MAX: usize = 55;
// A tag byte plus the four length bytes header_for writes, since it narrows the length to
// u32. RLP itself allows eight.
const HEADER_MAX: usize = 1 + 4;

pub struct Encoder<'a> {
    out: &'a mut [u8],
    len: usize,
}

impl<'a> Encoder<'a> {
    pub fn new(out: &'a mut [u8]) -> Self {
        Self { out, len: 0 }
    }

    pub fn finish(self) -> usize {
        self.len
    }

    /// RLP encode some bytes
    pub fn bytes(&mut self, value: &[u8]) -> Result<()> {
        // if its a single byte simply encode this byte without header
        if let [only] = value
            && *only <= SINGLE_BYTE_MAX
        {
            return self.push(&[*only]);
        }

        let mut header = [0u8; HEADER_MAX];
        // compute header length in accordance with the value
        let header_len = header_for(value.len(), SHORT_BYTES_BASE, LONG_BYTES_BASE, &mut header);
        // Push the encoded value and header
        self.push(header.get(..header_len).unwrap_or(&[]))?;
        self.push(value)
    }

    /// Writes a number that exists in the payload
    pub fn quantity(&mut self, value: &[u8]) -> Result<()> {
        // extract the leading zeros as RLP requires no leading zero
        let leading_zeros = value.iter().take_while(|byte| **byte == 0).count();
        // rlp encode quantity
        self.bytes(value.get(leading_zeros..).unwrap_or(&[]))
    }

    /// RLP encode a u64 into the payload
    pub fn unsigned(&mut self, value: u64) -> Result<()> {
        self.quantity(&value.to_be_bytes())
    }

    /// Writes an RLP list
    pub fn list(&mut self, fill: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        let start = self.len;
        // fill the list
        // for example adding unsigned or bytes
        fill(self)?;
        let payload_len = self.len - start;

        // compute appropriate header size
        let mut header = [0u8; HEADER_MAX];
        let header_len = header_for(payload_len, SHORT_LIST_BASE, LONG_LIST_BASE, &mut header);

        let needed = self.len + header_len;
        if self.out.len() < needed {
            return Err(Error::OutputTooSmall {
                needed,
                found: self.out.len(),
            });
        }

        // shift the items rightby the header length
        self.out.copy_within(start..self.len, start + header_len);
        
        // then copy in the header
        if let Some(slot) = self.out.get_mut(start..start + header_len) {
            slot.copy_from_slice(header.get(..header_len).unwrap_or(&[]));
        }

        // update length
        self.len = needed;

        Ok(())
    }

    /// Pushes raw bytes into the RLP encoded serialized bytes
    fn push(&mut self, bytes: &[u8]) -> Result<()> {
        let needed = self.len + bytes.len();
        // get the mutable slot
        let Some(slot) = self.out.get_mut(self.len..needed) else {
            return Err(Error::OutputTooSmall {
                needed,
                found: self.out.len(),
            });
        };
        // copy into it an update length
        slot.copy_from_slice(bytes);
        self.len = needed;
        Ok(())
    }
}

/// builds the header for RLP encoded data
fn header_for(len: usize, short_base: u8, long_base: u8, header: &mut [u8; HEADER_MAX]) -> usize {
    // the tag alone holds the length
    if len <= SHORT_MAX {
        header[0] = short_base + len as u8;
        return 1;
    }

    // otherwise compute length bytes
    let length_bytes = (len as u32).to_be_bytes();
    let leading_zeros = length_bytes.iter().take_while(|byte| **byte == 0).count();
    // keep only non leading zeros
    let significant = length_bytes.get(leading_zeros..).unwrap_or(&[]);

    // tag says how many bytes follow which represent the length
    header[0] = long_base + significant.len() as u8;

    // retrieve mutable slot
    if let Some(slot) = header.get_mut(1..1 + significant.len()) {
        // write the length inside
        slot.copy_from_slice(significant);
    }

// how many header bytes were written: the tag plus the length bytes
    1 + significant.len()
}

#[cfg(test)]
mod tests {
    use super::Encoder;
    use super::Error;
    use crate::rlp::decode::Item;
    use crate::rlp::decode::decode_exact;

    fn encoded(fill: impl FnOnce(&mut Encoder<'_>) -> super::Result<()>) -> Vec<u8> {
        let mut buffer = [0u8; 512];
        let mut encoder = Encoder::new(&mut buffer);
        fill(&mut encoder).expect("fits");
        let len = encoder.finish();
        buffer[..len].to_vec()
    }

    #[test]
    fn the_wiki_vectors_encode() {
        assert_eq!(encoded(|e| e.bytes(b"dog")), b"\x83dog");
        assert_eq!(encoded(|e| e.bytes(b"")), [0x80]);
        assert_eq!(encoded(|e| e.bytes(&[0x0F])), [0x0F]);
        assert_eq!(encoded(|e| e.bytes(&[0x00])), [0x00]);
        assert_eq!(encoded(|e| e.unsigned(1024)), [0x82, 0x04, 0x00]);
        assert_eq!(
            encoded(|e| e.list(|e| {
                e.bytes(b"cat")?;
                e.bytes(b"dog")
            })),
            b"\xc8\x83cat\x83dog"
        );
        assert_eq!(encoded(|e| e.list(|_| Ok(()))), [0xC0]);
    }

    #[test]
    fn the_set_theoretic_representation_of_three_encodes() {
        let bytes = encoded(|e| {
            e.list(|e| {
                e.list(|_| Ok(()))?;
                e.list(|e| e.list(|_| Ok(())))?;
                e.list(|e| {
                    e.list(|_| Ok(()))?;
                    e.list(|e| e.list(|_| Ok(())))
                })
            })
        });
        assert_eq!(bytes, [0xC7, 0xC0, 0xC1, 0xC0, 0xC3, 0xC0, 0xC1, 0xC0]);
    }

    #[test]
    fn a_fifty_six_byte_string_uses_the_long_form() {
        let bytes = encoded(|e| e.bytes(&[b'L'; 56]));
        assert_eq!(&bytes[..2], &[0xB8, 0x38]);
        assert_eq!(bytes.len(), 58);
    }

    #[test]
    fn a_list_longer_than_fifty_five_bytes_uses_the_long_form() {
        let bytes = encoded(|e| e.list(|e| e.bytes(&[b'L'; 60])));
        assert_eq!(&bytes[..2], &[0xF8, 62]);
        assert_eq!(decode_exact(&bytes).map(|_| ()), Ok(()));
    }

    #[test]
    fn a_quantity_drops_leading_zeros_and_zero_is_the_empty_string() {
        assert_eq!(
            encoded(|e| e.quantity(&[0, 0, 0x04, 0x00])),
            [0x82, 0x04, 0x00]
        );
        assert_eq!(encoded(|e| e.quantity(&[0, 0, 0])), [0x80]);
        assert_eq!(encoded(|e| e.unsigned(0)), [0x80]);
        assert_eq!(encoded(|e| e.unsigned(0x7F)), [0x7F]);
        assert_eq!(encoded(|e| e.unsigned(0x80)), [0x81, 0x80]);
    }

    #[test]
    fn everything_encoded_decodes_back() {
        for value in [
            &[][..],
            &[0x00][..],
            &[0x7F][..],
            &[0x80][..],
            &[b'x'; 55][..],
            &[b'y'; 300][..],
        ] {
            let bytes = encoded(|e| e.bytes(value));
            assert_eq!(decode_exact(&bytes), Ok(Item::Bytes(value)));
        }
    }

    #[test]
    fn a_short_buffer_is_refused_for_strings_and_for_list_headers() {
        let mut buffer = [0u8; 3];
        let mut encoder = Encoder::new(&mut buffer);
        assert!(matches!(
            encoder.bytes(b"dog"),
            Err(Error::OutputTooSmall { .. })
        ));

        let mut buffer = [0u8; 4];
        let mut encoder = Encoder::new(&mut buffer);
        assert!(matches!(
            encoder.list(|e| e.bytes(b"dog")),
            Err(Error::OutputTooSmall { .. })
        ));
    }
}
