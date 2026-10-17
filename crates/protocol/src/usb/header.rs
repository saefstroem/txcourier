use crate::usb::error::Error;
use crate::usb::result::Result;

// USB 2.0 Table 9-10: the configuration descriptor is nine bytes, and the walk starts after
// it. USB allows an interface association descriptor first; this parser does not.
pub const HEADER_LEN: usize = 9;

// USB 2.0 Table 9-5: CONFIGURATION is descriptor type 2.
const CONFIGURATION_DESCRIPTOR_TYPE: u8 = 0x02;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub total_length: u16,
    pub interfaces: u8,
    pub configuration_value: u8,
}

impl Header {
    pub fn read(bytes: &[u8]) -> Result<Self> {
        let [
            length,
            CONFIGURATION_DESCRIPTOR_TYPE,
            total_low,
            total_high,
            interfaces,
            configuration_value,
            ..,
        ] = bytes
        else {
            return Err(Error::NotAConfiguration);
        };

        if usize::from(*length) != HEADER_LEN {
            return Err(Error::NotAConfiguration);
        }

        Ok(Self {
            total_length: u16::from_le_bytes([*total_low, *total_high]),
            interfaces: *interfaces,
            configuration_value: *configuration_value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Header;
    use crate::usb::error::Error;

    const LEDGER_HEADER: [u8; 9] = [0x09, 0x02, 0x60, 0x00, 0x03, 0x01, 0x00, 0xA0, 0x32];

    #[test]
    fn the_ledger_header_yields_its_total_length_interfaces_and_value() {
        assert_eq!(
            Header::read(&LEDGER_HEADER),
            Ok(Header {
                total_length: 96,
                interfaces: 3,
                configuration_value: 1,
            })
        );
    }

    #[test]
    fn a_header_longer_than_nine_bytes_reads_only_its_own_fields() {
        let mut bytes = LEDGER_HEADER.to_vec();
        bytes.extend_from_slice(&[0x09, 0x04, 0x00, 0x00, 0x02, 0x03, 0x00, 0x00, 0x04]);
        assert_eq!(
            Header::read(&bytes).map(|header| header.total_length),
            Ok(96)
        );
    }

    #[test]
    fn a_descriptor_of_another_type_is_refused() {
        let mut bytes = LEDGER_HEADER;
        bytes[1] = 0x01;
        assert_eq!(Header::read(&bytes), Err(Error::NotAConfiguration));
    }

    #[test]
    fn a_header_claiming_the_wrong_length_is_refused() {
        let mut bytes = LEDGER_HEADER;
        bytes[0] = 0x0A;
        assert_eq!(Header::read(&bytes), Err(Error::NotAConfiguration));
    }

    #[test]
    fn a_short_or_empty_header_is_refused() {
        assert_eq!(
            Header::read(&LEDGER_HEADER[..5]),
            Err(Error::NotAConfiguration)
        );
        assert_eq!(Header::read(&[]), Err(Error::NotAConfiguration));
    }
}
