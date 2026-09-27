// USB 2.0 Table 9-3: GetDescriptor bmRequestType is 10000000B, device-to-host, standard,
// device.
const DIRECTION_DEVICE_TO_HOST: u8 = 0x80;
// USB 2.0 Table 9-3: SetAddress and SetConfiguration bmRequestType is 00000000B,
// host-to-device.
const DIRECTION_HOST_TO_DEVICE: u8 = 0x00;
// USB 2.0 Table 9-4: SET_ADDRESS is bRequest 5.
const REQUEST_SET_ADDRESS: u8 = 0x05;
// USB 2.0 Table 9-4: GET_DESCRIPTOR is bRequest 6.
const REQUEST_GET_DESCRIPTOR: u8 = 0x06;
// USB 2.0 Table 9-4: SET_CONFIGURATION is bRequest 9.
const REQUEST_SET_CONFIGURATION: u8 = 0x09;
// USB 2.0 Table 9-3: wIndex is zero for SetAddress and SetConfiguration. Section 9.4.3: it is
// zero for every descriptor type but string, which this host never reads.
const NO_INDEX: [u8; 2] = [0x00, 0x00];
// USB 2.0 Table 9-3: wLength is zero for SetAddress and SetConfiguration, which have no data
// stage.
const NO_LENGTH: [u8; 2] = [0x00, 0x00];
// USB 2.0 sections 9.4.6 and 9.4.7: the high byte of wValue is zero for SetAddress, whose
// addresses end at 127, and reserved for SetConfiguration.
const NO_VALUE_HIGH: u8 = 0x00;

// USB 2.0 Table 9-5: DEVICE is descriptor type 1.
pub const DESCRIPTOR_DEVICE: u8 = 0x01;
// USB 2.0 Table 9-5: CONFIGURATION is descriptor type 2.
pub const DESCRIPTOR_CONFIGURATION: u8 = 0x02;
// USB 2.0 section 9.3: every Setup packet is eight bytes.
pub const SETUP_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Used in the setup packet sent from the usb host to the ledger
/// in beginning of every control transfer
pub enum Request {
    GetDescriptor { kind: u8, index: u8, length: u16 },
    SetAddress(u8),
    SetConfiguration(u8),
}

impl Request {
    pub const fn encode(self) -> [u8; SETUP_LEN] {
        let [index_low, index_high] = NO_INDEX;

        match self {
            Self::GetDescriptor {
                kind,
                index,
                length,
            } => {
                let [length_low, length_high] = length.to_le_bytes();
                [
                    DIRECTION_DEVICE_TO_HOST,
                    REQUEST_GET_DESCRIPTOR,
                    index,
                    kind,
                    index_low,
                    index_high,
                    length_low,
                    length_high,
                ]
            }
            Self::SetAddress(address) => {
                let [length_low, length_high] = NO_LENGTH;
                [
                    DIRECTION_HOST_TO_DEVICE,
                    REQUEST_SET_ADDRESS,
                    address,
                    NO_VALUE_HIGH,
                    index_low,
                    index_high,
                    length_low,
                    length_high,
                ]
            }
            Self::SetConfiguration(value) => {
                let [length_low, length_high] = NO_LENGTH;
                [
                    DIRECTION_HOST_TO_DEVICE,
                    REQUEST_SET_CONFIGURATION,
                    value,
                    NO_VALUE_HIGH,
                    index_low,
                    index_high,
                    length_low,
                    length_high,
                ]
            }
        }
    }

    pub const fn response_length(self) -> u16 {
        match self {
            Self::GetDescriptor { length, .. } => length,
            Self::SetAddress(_) | Self::SetConfiguration(_) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DESCRIPTOR_CONFIGURATION;
    use super::DESCRIPTOR_DEVICE;
    use super::Request;

    #[test]
    fn the_first_eight_bytes_of_the_device_descriptor_match_the_reference_request() {
        assert_eq!(
            Request::GetDescriptor {
                kind: DESCRIPTOR_DEVICE,
                index: 0,
                length: 8
            }
            .encode(),
            [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x08, 0x00]
        );
    }

    #[test]
    fn the_length_is_little_endian() {
        let request = Request::GetDescriptor {
            kind: DESCRIPTOR_DEVICE,
            index: 0,
            length: 0x0112,
        };
        assert_eq!(request.encode().get(6..), Some(&[0x12, 0x01][..]));
        assert_eq!(request.response_length(), 0x0112);
    }

    #[test]
    fn the_descriptor_type_sits_above_the_index() {
        let setup = Request::GetDescriptor {
            kind: DESCRIPTOR_CONFIGURATION,
            index: 0x03,
            length: 9,
        }
        .encode();
        assert_eq!(setup.get(2), Some(&0x03));
        assert_eq!(setup.get(3), Some(&0x02));
    }

    #[test]
    fn set_address_carries_the_address_and_no_data() {
        let request = Request::SetAddress(1);
        assert_eq!(
            request.encode(),
            [0x00, 0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(request.response_length(), 0);
    }

    #[test]
    fn set_configuration_carries_the_value_and_no_data() {
        let request = Request::SetConfiguration(1);
        assert_eq!(
            request.encode(),
            [0x00, 0x09, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(request.response_length(), 0);
    }

    #[test]
    fn only_a_descriptor_read_expects_a_data_stage() {
        for request in [Request::SetAddress(7), Request::SetConfiguration(1)] {
            assert_eq!(request.response_length(), 0);
            assert_eq!(request.encode().first(), Some(&0x00));
        }

        let read = Request::GetDescriptor {
            kind: DESCRIPTOR_DEVICE,
            index: 0,
            length: 18,
        };
        assert_ne!(read.response_length(), 0);
        assert_eq!(read.encode().first(), Some(&0x80));
    }

    #[test]
    fn the_declared_length_is_the_one_encoded_into_the_packet() {
        for length in [0u16, 1, 8, 9, 18, 0x0112, u16::MAX] {
            let request = Request::GetDescriptor {
                kind: DESCRIPTOR_DEVICE,
                index: 0,
                length,
            };
            let setup = request.encode();
            let declared = match setup.get(6..8) {
                Some([low, high]) => u16::from_le_bytes([*low, *high]),
                _ => unreachable!(),
            };
            assert_eq!(declared, request.response_length());
        }
    }
}
