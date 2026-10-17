use crate::ledger::REPORT_LEN;
use crate::usb::error::Error;
use crate::usb::header::{HEADER_LEN, Header};
use crate::usb::result::Result;

// USB 2.0 Table 9-5: INTERFACE is descriptor type 4.
const INTERFACE_DESCRIPTOR_TYPE: u8 = 0x04;
// HID 1.11 section 7.1: the HID class descriptor is type 0x21. Report is 0x22, physical
// 0x23.
const HID_DESCRIPTOR_TYPE: u8 = 0x21;
// USB 2.0 Table 9-5: ENDPOINT is descriptor type 5.
const ENDPOINT_DESCRIPTOR_TYPE: u8 = 0x05;

// USB 2.0 Table 9-12: the interface descriptor is nine bytes.
const INTERFACE_LENGTH: u8 = 9;
// USB 2.0 Table 9-13: the endpoint descriptor is seven bytes.
const ENDPOINT_LENGTH: u8 = 7;
// HID 1.11 section 6.2.1: the HID descriptor has at least one subordinate descriptor entry,
// at offsets 6 to 8.
const SHORTEST_HID_LENGTH: u8 = 9;

// The Nano S Plus presents three interfaces: APDU HID, WebUSB and a boot keyboard. Ledger
// composes the count from the classes enabled at build time; this is what the device reports.
const LEDGER_INTERFACES: u8 = 3;
// ledger-secure-sdk: HID_INTF, the APDU HID interface, is 0. This parser also requires it to
// come first in the configuration.
const APDU_INTERFACE_NUMBER: u8 = 0;
// USB 2.0 section 9.6.5: an interface's default is alternate setting zero.
const APDU_ALTERNATE_SETTING: u8 = 0;
// HID 1.11 section 4.4: the interrupt OUT endpoint is optional. Ledger declares one rather
// than take output reports over the control pipe, so this interface has two endpoints.
const APDU_ENDPOINT_COUNT: u8 = 2;
// HID 1.11 section 4.1: bInterfaceClass is 3 for HID.
const APDU_CLASS_HID: u8 = 0x03;
// HID 1.11 section 4.2: bInterfaceSubClass is 0 or 1. Ledger's APDU interface is not a boot
// device, which distinguishes it from the Nano S Plus's own boot keyboard interface.
const APDU_SUBCLASS_NONE: u8 = 0x00;
// HID 1.11 section 4.3: bInterfaceProtocol 0 is None. It has meaning only for a boot
// interface.
const APDU_PROTOCOL_NONE: u8 = 0x00;
// ledger-secure-sdk: HID_EPIN_ADDR is 0x82. USB 2.0 Table 9-13: bit 7 set is IN.
const APDU_ENDPOINT_IN: u8 = 0x82;
// ledger-secure-sdk: HID_EPOUT_ADDR is 0x02. USB 2.0 Table 9-13: bit 7 clear is OUT.
const APDU_ENDPOINT_OUT: u8 = 0x02;

// USB 2.0 Table 9-13: bmAttributes bits 1..0 are the transfer type.
const TRANSFER_TYPE_MASK: u8 = 0x03;
// USB 2.0 Table 9-13: transfer type 11B is interrupt.
const TRANSFER_TYPE_INTERRUPT: u8 = 0x03;

// The eight fixed bytes of Ledger's APDU HID interface descriptor. iInterface is a build
// detail and is left free.
const APDU_INTERFACE_DESCRIPTOR: [u8; 8] = [
    INTERFACE_LENGTH,
    INTERFACE_DESCRIPTOR_TYPE,
    APDU_INTERFACE_NUMBER,
    APDU_ALTERNATE_SETTING,
    APDU_ENDPOINT_COUNT,
    APDU_CLASS_HID,
    APDU_SUBCLASS_NONE,
    APDU_PROTOCOL_NONE,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Interface of the APDU
pub struct ApduInterface {
    pub configuration_value: u8,
    pub endpoint_in: u8,
    pub endpoint_out: u8,
}

/// Verify that the APDU interface is Ledger APDU interface so that we dont communicate
/// with other unexpected device
pub fn verify_apdu_interface(descriptor: &[u8]) -> Result<ApduInterface> {
    // extract header
    let header = Header::read(descriptor)?;

    if usize::from(header.total_length) != descriptor.len() {
        return Err(Error::Truncated);
    }

    // Ensure there is expected number of interfaces
    if header.interfaces != LEDGER_INTERFACES {
        return Err(Error::UnexpectedInterfaceCount(header.interfaces));
    }

    // Get the body and compute the endpoints at which we should communicate
    // on.
    let body = descriptor.get(HEADER_LEN..).ok_or(Error::Truncated)?;
    let after_interface = apdu_interface(body)?;
    let after_hid = hid_class_descriptor(after_interface)?;
    let (endpoint_in, after_in) = interrupt_endpoint(after_hid, APDU_ENDPOINT_IN)?;
    let (endpoint_out, _) = interrupt_endpoint(after_in, APDU_ENDPOINT_OUT)?;

    Ok(ApduInterface {
        configuration_value: header.configuration_value,
        endpoint_in,
        endpoint_out,
    })
}

/// Extracts APDU interface from some bytes
fn apdu_interface(bytes: &[u8]) -> Result<&[u8]> {
    if !bytes.starts_with(&APDU_INTERFACE_DESCRIPTOR) {
        return Err(Error::NotApduInterface);
    }

    bytes
        .get(usize::from(INTERFACE_LENGTH)..)
        .ok_or(Error::Truncated)
}

/// Extract hid class descriptor from bytes
fn hid_class_descriptor(bytes: &[u8]) -> Result<&[u8]> {
    let [length, HID_DESCRIPTOR_TYPE, ..] = bytes else {
        return Err(Error::MissingHidDescriptor);
    };

    if *length < SHORTEST_HID_LENGTH {
        return Err(Error::MissingHidDescriptor);
    }

    bytes.get(usize::from(*length)..).ok_or(Error::Truncated)
}

/// Computes endpoint from bytes
fn interrupt_endpoint(bytes: &[u8], expected: u8) -> Result<(u8, &[u8])> {
    let [
        length,
        ENDPOINT_DESCRIPTOR_TYPE,
        address,
        attributes,
        packet_low,
        packet_high,
        _,
        rest @ ..,
    ] = bytes
    else {
        return Err(Error::Truncated);
    };

    // Ensure it is expected, i.e. it matches what we have encoded
    if *length != ENDPOINT_LENGTH || *address != expected {
        return Err(Error::UnexpectedEndpoint(*address));
    }

    // Ensure correct transfer type
    if *attributes & TRANSFER_TYPE_MASK != TRANSFER_TYPE_INTERRUPT {
        return Err(Error::NotInterruptEndpoint(*address));
    }

    // ensure packet size is expected apdu packet size (64)
    let packet = u16::from_le_bytes([*packet_low, *packet_high]);
    if usize::from(packet) != REPORT_LEN {
        return Err(Error::UnexpectedPacketSize(packet));
    }

    Ok((*address, rest))
}

#[cfg(test)]
mod tests {
    use super::{ApduInterface, verify_apdu_interface};
    use crate::usb::error::Error;

    const HEADER_AT: usize = 0;
    const APDU_INTERFACE_AT: usize = 9;
    const HID_AT: usize = 18;
    const ENDPOINT_IN_AT: usize = 27;
    const ENDPOINT_OUT_AT: usize = 34;

    fn ledger() -> Vec<u8> {
        let mut bytes = vec![0x09, 0x02, 0x00, 0x00, 0x03, 0x01, 0x00, 0xA0, 0x32];
        bytes.extend_from_slice(&[0x09, 0x04, 0x00, 0x00, 0x02, 0x03, 0x00, 0x00, 0x04]);
        bytes.extend_from_slice(&[0x09, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, 0x22, 0x00]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x82, 0x03, 0x40, 0x00, 0x01]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x02, 0x03, 0x40, 0x00, 0x01]);
        bytes.extend_from_slice(&[0x09, 0x04, 0x01, 0x00, 0x02, 0xFF, 0xFF, 0xFF, 0x05]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x83, 0x02, 0x40, 0x00, 0x00]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x03, 0x02, 0x40, 0x00, 0x00]);
        bytes.extend_from_slice(&[0x09, 0x04, 0x02, 0x00, 0x02, 0x03, 0x01, 0x01, 0x06]);
        bytes.extend_from_slice(&[0x09, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, 0x3F, 0x00]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x84, 0x03, 0x08, 0x00, 0x01]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x04, 0x03, 0x08, 0x00, 0x01]);
        restated(bytes)
    }

    fn restated(mut bytes: Vec<u8>) -> Vec<u8> {
        let total = bytes.len() as u16;
        bytes[2..4].copy_from_slice(&total.to_le_bytes());
        bytes
    }

    #[test]
    fn the_ledger_configuration_descriptor_is_accepted() {
        assert_eq!(
            verify_apdu_interface(&ledger()),
            Ok(ApduInterface {
                configuration_value: 1,
                endpoint_in: 0x82,
                endpoint_out: 0x02,
            })
        );
    }

    #[test]
    fn the_endpoints_come_from_the_descriptor_and_not_from_the_caller() {
        let mut bytes = ledger();
        bytes[ENDPOINT_IN_AT + 2] = 0x81;
        assert_eq!(
            verify_apdu_interface(&bytes),
            Err(Error::UnexpectedEndpoint(0x81))
        );
    }

    #[test]
    fn an_endpoint_descriptor_of_another_length_is_refused() {
        let mut bytes = ledger();
        bytes[ENDPOINT_IN_AT] = 0x09;
        assert_eq!(
            verify_apdu_interface(&bytes),
            Err(Error::UnexpectedEndpoint(0x82))
        );
    }

    #[test]
    fn the_interface_string_index_is_not_part_of_the_expected_shape() {
        let mut bytes = ledger();
        bytes[APDU_INTERFACE_AT + 8] = 0x00;
        assert_eq!(
            verify_apdu_interface(&bytes).map(|apdu| apdu.endpoint_in),
            Ok(0x82)
        );
    }

    #[test]
    fn swapped_endpoints_are_refused() {
        let mut bytes = ledger();
        bytes[ENDPOINT_IN_AT + 2] = 0x02;
        bytes[ENDPOINT_OUT_AT + 2] = 0x82;
        assert_eq!(
            verify_apdu_interface(&bytes),
            Err(Error::UnexpectedEndpoint(0x02))
        );
    }

    #[test]
    fn a_non_hid_interface_class_is_refused() {
        let mut bytes = ledger();
        bytes[APDU_INTERFACE_AT + 5] = 0xFF;
        assert_eq!(verify_apdu_interface(&bytes), Err(Error::NotApduInterface));
    }

    #[test]
    fn the_boot_keyboard_interface_is_refused_in_the_apdu_position() {
        let mut bytes = ledger();
        bytes[APDU_INTERFACE_AT + 6] = 0x01;
        bytes[APDU_INTERFACE_AT + 7] = 0x01;
        assert_eq!(verify_apdu_interface(&bytes), Err(Error::NotApduInterface));
    }

    #[test]
    fn an_apdu_interface_that_is_not_first_is_refused() {
        let mut bytes = vec![0x09, 0x02, 0x00, 0x00, 0x03, 0x01, 0x00, 0xA0, 0x32];
        bytes.extend_from_slice(&[0x09, 0x04, 0x00, 0x00, 0x02, 0x03, 0x01, 0x01, 0x04]);
        bytes.extend_from_slice(&[0x09, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, 0x3F, 0x00]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x81, 0x03, 0x08, 0x00, 0x01]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x01, 0x03, 0x08, 0x00, 0x01]);
        bytes.extend_from_slice(&[0x09, 0x04, 0x01, 0x00, 0x02, 0x03, 0x00, 0x00, 0x05]);
        bytes.extend_from_slice(&[0x09, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, 0x22, 0x00]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x82, 0x03, 0x40, 0x00, 0x01]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x02, 0x03, 0x40, 0x00, 0x01]);
        bytes.extend_from_slice(&[0x09, 0x04, 0x02, 0x00, 0x02, 0xFF, 0xFF, 0xFF, 0x06]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x83, 0x02, 0x40, 0x00, 0x00]);
        bytes.extend_from_slice(&[0x07, 0x05, 0x03, 0x02, 0x40, 0x00, 0x00]);
        assert_eq!(
            verify_apdu_interface(&restated(bytes)),
            Err(Error::NotApduInterface)
        );
    }

    #[test]
    fn an_alternate_setting_in_the_apdu_position_is_refused() {
        let mut bytes = ledger();
        bytes[APDU_INTERFACE_AT + 3] = 0x01;
        assert_eq!(verify_apdu_interface(&bytes), Err(Error::NotApduInterface));
    }

    #[test]
    fn an_apdu_interface_with_one_endpoint_is_refused() {
        let mut bytes = ledger();
        bytes[APDU_INTERFACE_AT + 4] = 0x01;
        assert_eq!(verify_apdu_interface(&bytes), Err(Error::NotApduInterface));
    }

    #[test]
    fn a_bulk_endpoint_where_an_interrupt_one_belongs_is_refused() {
        let mut bytes = ledger();
        bytes[ENDPOINT_IN_AT + 3] = 0x02;
        assert_eq!(
            verify_apdu_interface(&bytes),
            Err(Error::NotInterruptEndpoint(0x82))
        );
    }

    #[test]
    fn an_endpoint_that_cannot_carry_a_whole_report_is_refused() {
        let mut bytes = ledger();
        bytes[ENDPOINT_OUT_AT + 4] = 0x20;
        assert_eq!(
            verify_apdu_interface(&bytes),
            Err(Error::UnexpectedPacketSize(32))
        );
    }

    #[test]
    fn a_missing_hid_class_descriptor_is_refused() {
        let mut bytes = ledger();
        bytes[HID_AT + 1] = 0x22;
        assert_eq!(
            verify_apdu_interface(&bytes),
            Err(Error::MissingHidDescriptor)
        );
    }

    #[test]
    fn a_zero_length_hid_class_descriptor_is_refused_instead_of_spinning() {
        let mut bytes = ledger();
        bytes[HID_AT] = 0x00;
        assert_eq!(
            verify_apdu_interface(&bytes),
            Err(Error::MissingHidDescriptor)
        );
    }

    #[test]
    fn an_overlong_hid_class_descriptor_that_swallows_the_endpoints_is_refused() {
        let mut bytes = ledger();
        bytes[HID_AT] = 0xFF;
        assert_eq!(verify_apdu_interface(&bytes), Err(Error::Truncated));
    }

    #[test]
    fn a_descriptor_cut_short_by_the_transfer_is_refused() {
        let bytes = ledger();
        assert_eq!(
            verify_apdu_interface(bytes.get(..30).unwrap_or(&[])),
            Err(Error::Truncated)
        );
    }

    #[test]
    fn a_descriptor_cut_short_but_restating_its_length_is_refused() {
        let bytes = ledger();
        let cut = restated(bytes.get(..30).unwrap_or(&[]).to_vec());
        assert_eq!(verify_apdu_interface(&cut), Err(Error::Truncated));
    }

    #[test]
    fn an_empty_or_headerless_descriptor_is_refused() {
        assert_eq!(verify_apdu_interface(&[]), Err(Error::NotAConfiguration));
        assert_eq!(
            verify_apdu_interface(&[0x09, 0x02, 0x04, 0x00]),
            Err(Error::NotAConfiguration)
        );
    }

    #[test]
    fn a_configuration_with_another_interface_count_is_refused() {
        let mut bytes = ledger();
        bytes[HEADER_AT + 4] = 0x02;
        assert_eq!(
            verify_apdu_interface(&bytes),
            Err(Error::UnexpectedInterfaceCount(2))
        );
    }

    #[test]
    fn the_configuration_value_is_read_from_the_descriptor() {
        let mut bytes = ledger();
        bytes[HEADER_AT + 5] = 0x07;
        assert_eq!(
            verify_apdu_interface(&bytes).map(|apdu| apdu.configuration_value),
            Ok(0x07)
        );
    }
}
