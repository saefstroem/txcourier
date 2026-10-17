use txcourier_pio_usb::{DESCRIPTOR_CONFIGURATION, DESCRIPTOR_DEVICE, Request};
use txcourier_protocol::usb::{ApduInterface, Device, Header, verify_apdu_interface};

use crate::host::bus::Bus;
use crate::host::control;
use crate::host::error::Error;
use crate::host::result::Result;
use crate::host::transport::Endpoint;

// USB 2.0 section 8.3.2.1: address zero is reserved as the default address. A device
// answers there from bus reset until SetAddress completes (section 9.1.1.3).
pub const DEFAULT_ADDRESS: u8 = 0;
// USB 2.0 section 9.4.6: SetAddress accepts 1 to 127. This port holds one device, so the
// lowest legal address is used.
pub const ASSIGNED_ADDRESS: u8 = 1;
// USB 2.0 section 5.5.3: a device returns at least the first eight bytes of its device
// descriptor in a single packet, whatever its endpoint zero packet size.
pub const SHORT_DESCRIPTOR_LEN: u16 = 8;
// USB 2.0 Table 9-8: the device descriptor is 18 bytes, fields at offsets 0 through 17.
pub const FULL_DESCRIPTOR_LEN: u16 = 18;
// USB 2.0 Table 9-10: the configuration descriptor header is 9 bytes, before the interface
// and endpoint descriptors that wTotalLength covers.
pub const CONFIGURATION_HEADER_LEN: u16 = 9;
// USB 2.0 section 5.5.3: a full-speed endpoint zero uses 8, 16, 32 or 64 bytes. Eight is
// the smallest, and the only safe assumption before bMaxPacketSize0 has been read.
pub const SMALLEST_MAX_PACKET: usize = 8;
// USB 2.0 section 7.1.7.5 and Table 7-14: a device has 10 ms of reset recovery before it
// must answer. Twelve frames span 11.1 ms, since the first SOF is sent immediately.
pub const SETTLE_FRAMES: u16 = 12;
// USB 2.0 section 9.2.6.3: a device is allowed a 2 ms SetAddress recovery interval, and must
// accept Setup packets at its new address only at the end of it. The first SOF is sent
// immediately, so four frames span 3 ms.
const SETADDRESS_RECOVERY_FRAMES: u16 = 4;

// USB 2.0 section 9.4.3: the low byte of wValue is the descriptor index. It must be zero
// for a device descriptor, and selects the first of n configuration descriptors.
const FIRST_INDEX: u8 = 0;
// Ledger's USB vendor id, read from the Nano S Plus with ioreg, which resolves 0x2C97 to
// "Ledger" from the host's own vendor database. idProduct is deliberately not checked: it
// varies across Ledger models and between application and bootloader mode.
const LEDGER_VENDOR_ID: u16 = 0x2C97;
// USB 2.0 Table 9-8: bMaxPacketSize0 is at byte offset 7 of the device descriptor.
const MAX_PACKET_SIZE_INDEX: usize = 7;
// USB 2.0 Table 9-13: bEndpointAddress bits 3..0 are the endpoint number. Bit 7 is the
// direction and bits 6..4 are reserved.
const ENDPOINT_NUMBER_MASK: u8 = 0x0F;

pub struct Ledger {
    pub interrupt_in: Endpoint,
    pub interrupt_out: Endpoint,
}


/// Retrieves the device descriptor from peripheral
fn device_descriptor(
    bus: &mut Bus,
    address: u8,
    max_packet: usize,
    wanted: u16,
    out: &mut [u8],
    reply: &mut [u8],
) -> Result<usize> {
    let request = Request::GetDescriptor {
        kind: DESCRIPTOR_DEVICE,
        index: FIRST_INDEX,
        length: wanted,
    };

    // request the device descriptor from ledger
    control::control(bus, address, request, max_packet, out, reply)
}

/// Assigns address to the connected peripheral
fn assign_address(bus: &mut Bus, from: u8, to: u8, reply: &mut [u8]) -> Result<()> {
    // assign address to the ledger
    control::control(bus, from, Request::SetAddress(to), 0, &mut [], reply).map(|_| ())
}

fn configuration(
    bus: &mut Bus,
    address: u8,
    max_packet: usize,
    out: &mut [u8],
    reply: &mut [u8],
) -> Result<usize> {
    // request the header for the device configuration
    let header = Request::GetDescriptor {
        kind: DESCRIPTOR_CONFIGURATION,
        index: FIRST_INDEX,
        length: CONFIGURATION_HEADER_LEN,
    };
    let header_len = control::control(bus, address, header, max_packet, out, reply)?;

    // read it from the header
    let declared =
        Header::read(out.get(..header_len).unwrap_or(&[])).map_err(|_| Error::CorruptReply)?;
    let wanted = usize::from(declared.total_length).min(out.len());
    let wanted = u16::try_from(wanted).unwrap_or(u16::MAX);

    let whole = Request::GetDescriptor {
        kind: DESCRIPTOR_CONFIGURATION,
        index: FIRST_INDEX,
        length: wanted,
    };

    // now read the full variable length configuration
    control::control(bus, address, whole, max_packet, out, reply)
}

/// Selects configuration for connected device
fn select_configuration(bus: &mut Bus, address: u8, value: u8, reply: &mut [u8]) -> Result<()> {
    control::control(
        bus,
        address,
        Request::SetConfiguration(value),
        0,
        &mut [],
        reply,
    )
    .map(|_| ())
}

/// Endpoints to communicate with the ledger
fn endpoints(address: u8, apdu: &ApduInterface) -> Ledger {
    Ledger {
        interrupt_in: Endpoint::new(address, apdu.endpoint_in & ENDPOINT_NUMBER_MASK),
        interrupt_out: Endpoint::new(address, apdu.endpoint_out & ENDPOINT_NUMBER_MASK),
    }
}

/// reads the maximum packet size from offset 7 of the descriptor
fn max_packet_of(descriptor: &[u8], len: usize) -> usize {
    if len <= MAX_PACKET_SIZE_INDEX {
        return SMALLEST_MAX_PACKET;
    }

    descriptor
        .get(MAX_PACKET_SIZE_INDEX)
        .map(|size| usize::from(*size))
        .unwrap_or(SMALLEST_MAX_PACKET)
}

/// Attaches a device to the bus
pub fn attach(bus: &mut Bus, reply: &mut [u8], descriptor: &mut [u8]) -> Result<Ledger> {
    // resets the bus to a known state
    bus.reset()?;
    for _ in 0..SETTLE_FRAMES {
        bus.keep_alive()?;
    }

    // compute device descriptor
    let short = device_descriptor(
        bus,
        DEFAULT_ADDRESS,
        SMALLEST_MAX_PACKET,
        SHORT_DESCRIPTOR_LEN,
        descriptor,
        reply,
    )?;

    // compute max packet size
    let max_packet = max_packet_of(descriptor, short);

    // receive full identity
    let full = device_descriptor(
        bus,
        DEFAULT_ADDRESS,
        max_packet,
        FULL_DESCRIPTOR_LEN,
        descriptor,
        reply,
    )?;

    let device =
        Device::read(descriptor.get(..full).unwrap_or(&[])).map_err(|_| Error::CorruptReply)?;
    
    // ensure the device is a ledger
    if device.vendor != LEDGER_VENDOR_ID {
        return Err(Error::UnexpectedVendor(device.vendor));
    }

    // assign the address to the ledger
    assign_address(bus, DEFAULT_ADDRESS, ASSIGNED_ADDRESS, reply)?;
    for _ in 0..SETADDRESS_RECOVERY_FRAMES {
        bus.keep_alive()?;
    }

    let len = configuration(bus, ASSIGNED_ADDRESS, max_packet, descriptor, reply)?;
    let found = descriptor.get(..len).unwrap_or(&[]);
    
    // Verify that the apdu interface is the one we are expecting
    // which is the apdu transport, we ensure we have the correct interface
    // so that we can send smartcard commands
    let apdu = verify_apdu_interface(found).map_err(|_| Error::NoVendorInterface)?;
    let ledger = endpoints(ASSIGNED_ADDRESS, &apdu);

    // select the configuration we want to use for the ledger
    select_configuration(bus, ASSIGNED_ADDRESS, apdu.configuration_value, reply)?;

    Ok(ledger)
}
