use crate::ledger::error::Error;
use crate::ledger::result::Result;

// ledger-secure-sdk lib_standard_app/offsets.h: OFFSET_CDATA is 5, after CLA, INS, P1, P2
// and Lc. ISO 7816-4 fixes the four-byte header; Lc is always sent here, even as 0x00.
pub const HEADER_LEN: usize = 5;
// ISO 7816-4 section 5.1: a one-byte Lc encodes Nc from 1 to 255.
pub const MAX_DATA_LEN: usize = 255;
// ISO 7816-4 Table 1: the status bytes SW1-SW2 are two bytes.
pub const STATUS_LEN: usize = 2;
// ISO 7816-4 Table 5: '9000' is normal processing. ledger-secure-sdk names it SWO_SUCCESS.
// '61XX' is also normal processing; require_ok() accepts only '9000'.
pub const STATUS_OK: u16 = 0x9000;

/// builds one smartcard command to the Ledger
pub fn command(
    class: u8,
    instruction: u8,
    first: u8,
    second: u8,
    data: &[u8],
    out: &mut [u8],
) -> Result<usize> {
    // ensure within limits
    if data.len() > MAX_DATA_LEN {
        return Err(Error::DataTooLong(data.len()));
    }

    //compute needed data
    let needed = HEADER_LEN + data.len();
    let Some(slot) = out.get_mut(..needed) else {
        return Err(Error::OutputTooSmall {
            needed,
            found: out.len(),
        });
    };

    // split the payload slot at header and body
    let Some((header, body)) = slot.split_at_mut_checked(HEADER_LEN) else {
        return Err(Error::OutputTooSmall {
            needed,
            found: out.len(),
        });
    };
    // copy the structured data into the arranged slots
    header.copy_from_slice(&[class, instruction, first, second, data.len() as u8]);
    body.copy_from_slice(data);

    Ok(needed)
}

/// Ensures the ledger responds with an OK
pub fn require_ok(response: &[u8]) -> Result<&[u8]> {
    let Some(split) = response.len().checked_sub(STATUS_LEN) else {
        return Err(Error::ResponseTruncated);
    };

    let (Some(body), Some(status)) = (response.get(..split), response.get(split..)) else {
        return Err(Error::ResponseTruncated);
    };

    let [high, low] = status else {
        return Err(Error::ResponseTruncated);
    };
    let status = u16::from_be_bytes([*high, *low]);

    if status != STATUS_OK {
        return Err(Error::Status(status));
    }

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::MAX_DATA_LEN;
    use super::command;
    use super::require_ok;

    const GET_ADDRESS_PATH: [u8; 21] = [
        0x05, 0x80, 0x00, 0x00, 0x2C, 0x80, 0x00, 0x00, 0x3C, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    fn built(class: u8, instruction: u8, first: u8, data: &[u8]) -> Vec<u8> {
        let mut buffer = [0u8; 300];
        let len = command(class, instruction, first, 0x00, data, &mut buffer).expect("fits");
        buffer[..len].to_vec()
    }

    #[test]
    fn the_get_address_command_matches_the_reference_bytes() {
        assert_eq!(
            built(0xE0, 0x02, 0x00, &GET_ADDRESS_PATH),
            [
                0xE0, 0x02, 0x00, 0x00, 0x15, 0x05, 0x80, 0x00, 0x00, 0x2C, 0x80, 0x00, 0x00, 0x3C,
                0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }

    #[test]
    fn the_length_byte_is_the_data_length() {
        assert_eq!(built(0xE0, 0x04, 0x80, &[1, 2, 3])[4], 3);
        assert_eq!(built(0xE0, 0x06, 0x00, &[])[4], 0);
        assert_eq!(built(0xE0, 0x04, 0x00, &[0u8; 255])[4], 255);
    }

    #[test]
    fn data_beyond_a_single_apdu_is_refused() {
        let mut buffer = [0u8; 300];
        assert_eq!(
            command(
                0xE0,
                0x04,
                0x00,
                0x00,
                &[0u8; MAX_DATA_LEN + 1],
                &mut buffer
            ),
            Err(Error::DataTooLong(MAX_DATA_LEN + 1))
        );
    }

    #[test]
    fn a_short_buffer_is_refused() {
        let mut buffer = [0u8; 6];
        assert!(matches!(
            command(0xE0, 0x04, 0x00, 0x00, &[1, 2, 3], &mut buffer),
            Err(Error::OutputTooSmall { .. })
        ));
    }

    #[test]
    fn a_successful_response_yields_its_body_without_the_status() {
        assert_eq!(require_ok(&[0xAA, 0xBB, 0x90, 0x00]), Ok(&[0xAA, 0xBB][..]));
        assert_eq!(require_ok(&[0x90, 0x00]), Ok(&[][..]));
    }

    #[test]
    fn a_declined_signature_surfaces_its_status() {
        assert_eq!(require_ok(&[0x69, 0x85]), Err(Error::Status(0x6985)));
        assert_eq!(require_ok(&[0xAA, 0x6A, 0x80]), Err(Error::Status(0x6A80)));
    }

    #[test]
    fn a_response_shorter_than_a_status_word_is_truncated() {
        assert_eq!(require_ok(&[]), Err(Error::ResponseTruncated));
        assert_eq!(require_ok(&[0x90]), Err(Error::ResponseTruncated));
    }
}
