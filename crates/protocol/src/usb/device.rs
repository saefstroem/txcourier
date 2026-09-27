use crate::usb::error::Error;
use crate::usb::result::Result;

// USB 2.0 Table 9-8: the device descriptor is 18 bytes, fields at offsets 0 through 17.
const DEVICE_LEN: usize = 18;

// USB 2.0 Table 9-5: DEVICE is descriptor type 1.
const DEVICE_DESCRIPTOR_TYPE: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device {
    pub max_packet_zero: u8,
    pub vendor: u16,
    pub product: u16,
}

impl Device {
    pub fn read(bytes: &[u8]) -> Result<Self> {
        let [
            length,
            DEVICE_DESCRIPTOR_TYPE,
            _specification_low,
            _specification_high,
            _class,
            _subclass,
            _protocol,
            max_packet_zero,
            vendor_low,
            vendor_high,
            product_low,
            product_high,
            ..,
        ] = bytes
        else {
            return Err(Error::NotADevice);
        };

        if usize::from(*length) != DEVICE_LEN || bytes.len() < DEVICE_LEN {
            return Err(Error::NotADevice);
        }

        Ok(Self {
            max_packet_zero: *max_packet_zero,
            vendor: u16::from_le_bytes([*vendor_low, *vendor_high]),
            product: u16::from_le_bytes([*product_low, *product_high]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Device;
    use crate::usb::error::Error;

    const NANO_S_PLUS: [u8; 18] = [
        0x12, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x40, 0x97, 0x2C, 0x00, 0x50, 0x01, 0x02, 0x01,
        0x02, 0x03, 0x01,
    ];

    #[test]
    fn the_nano_s_plus_descriptor_yields_its_vendor_product_and_packet_size() {
        assert_eq!(
            Device::read(&NANO_S_PLUS),
            Ok(Device {
                max_packet_zero: 64,
                vendor: 0x2C97,
                product: 0x5000,
            })
        );
    }

    #[test]
    fn the_identifiers_are_little_endian() {
        let device = Device::read(&NANO_S_PLUS).expect("a valid descriptor");
        assert_eq!(NANO_S_PLUS.get(8..10), Some(&[0x97, 0x2C][..]));
        assert_eq!(device.vendor, 0x2C97);
        assert_eq!(NANO_S_PLUS.get(10..12), Some(&[0x00, 0x50][..]));
        assert_eq!(device.product, 0x5000);
    }

    #[test]
    fn a_configuration_descriptor_is_not_a_device_descriptor() {
        let mut bytes = NANO_S_PLUS;
        bytes[1] = 0x02;
        assert_eq!(Device::read(&bytes), Err(Error::NotADevice));
    }

    #[test]
    fn a_descriptor_declaring_the_wrong_length_is_rejected() {
        let mut bytes = NANO_S_PLUS;
        bytes[0] = 0x09;
        assert_eq!(Device::read(&bytes), Err(Error::NotADevice));
    }

    #[test]
    fn the_eight_byte_read_is_too_short_to_parse() {
        assert_eq!(
            Device::read(NANO_S_PLUS.get(..8).expect("eight bytes")),
            Err(Error::NotADevice)
        );
    }

    #[test]
    fn a_descriptor_cut_between_the_identifiers_is_rejected() {
        for len in 9..18 {
            assert_eq!(
                Device::read(NANO_S_PLUS.get(..len).expect("a prefix")),
                Err(Error::NotADevice),
                "{len} bytes parsed"
            );
        }
    }
}
