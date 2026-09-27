use usb_device::bus::{InterfaceNumber, UsbBus, UsbBusAllocator};
use usb_device::class::{ControlIn, ControlOut, UsbClass};
use usb_device::control::{Recipient, Request, RequestType};
use usb_device::descriptor::DescriptorWriter;
use usb_device::endpoint::{EndpointIn, EndpointOut, EndpointType, In, Out};
use usb_device::{Result, UsbError};

// USB 2.0 section 5.8.3: a full-speed bulk endpoint uses 8, 16, 32 or 64 bytes. The RP2350
// DPRAM buffers cap it at 64.
pub const BULK_PACKET_SIZE: u16 = 64;

// Nothing is written to this endpoint. Any legal size, at most 64, serves.
const NOTIFICATION_PACKET_SIZE: u16 = 16;
// USB 2.0 Table 9-13: a full-speed interrupt endpoint polls every 1 to 255 frames. The
// endpoint is never written, so every poll is answered with a NAK.
const NOTIFICATION_INTERVAL_FRAMES: u8 = 16;
// USB 2.0 section 5.8.4: a full-speed bulk endpoint has no polling interval. The HAL
// discards the byte.
const BULK_INTERVAL_FRAMES: u8 = 0;

// CDC 1.2 Table 3: the Communications interface class. The IAD repeats it as
// bFunctionClass, which a host matches its driver on.
const COMMUNICATIONS_INTERFACE_CLASS: u8 = 0x02;
// CDC 1.1 Table 16: Abstract Control Model. Hosts bind a serial driver to it.
const ABSTRACT_CONTROL_MODEL_SUBCLASS: u8 = 0x02;
// CDC 1.1 Table 17: no class-specific protocol. This device answers no AT commands.
const NO_CLASS_SPECIFIC_PROTOCOL: u8 = 0x00;
// CDC 1.1 Table 18: the Data interface class, which carries the bulk endpoints.
const DATA_INTERFACE_CLASS: u8 = 0x0a;
// CDC 1.2 section 4.6: the data interface subclass is unused and is 00h.
const DATA_INTERFACE_SUBCLASS: u8 = 0x00;
// CDC 1.1 Table 19: no protocol over the data interface.
const DATA_INTERFACE_PROTOCOL: u8 = 0x00;
// The IAD spans the control and data interfaces allocated in new().
const INTERFACES_PER_FUNCTION: u8 = 2;

// CDC 1.1 Table 24: CS_INTERFACE, the descriptor type of every functional descriptor below.
const CLASS_SPECIFIC_INTERFACE_DESCRIPTOR: u8 = 0x24;
// CDC 1.1 Table 25: the Header functional descriptor, which must open the functional set.
const HEADER_SUBTYPE: u8 = 0x00;
// CDC 1.1 Table 25: the Call Management functional descriptor, which names the data
// interface.
const CALL_MANAGEMENT_SUBTYPE: u8 = 0x01;
// CDC 1.1 Table 25: the Abstract Control Management functional descriptor, which lists the
// requests answered.
const ABSTRACT_CONTROL_MANAGEMENT_SUBTYPE: u8 = 0x02;
// CDC 1.1 Table 25: the Union functional descriptor, which binds the control and data
// interfaces into one function.
const UNION_SUBTYPE: u8 = 0x06;

// bcdCDC: the CDC revision these descriptors follow, 1.10 in BCD.
const CDC_SPECIFICATION_RELEASE: u16 = 0x0110;
// CDC PSTN 1.2 Table 3: D0 clear, no call management. D1 must then be zero.
const CALL_MANAGEMENT_CAPABILITIES: u8 = 0x00;
// CDC 1.1 Table 28: D1 and D2 set. D1 covers the line coding and control line state requests
// and the SERIAL_STATE notification; D2 covers Send_Break.
const ABSTRACT_CONTROL_MANAGEMENT_CAPABILITIES: u8 = 0x06;

// CDC 1.1 Table 4: SET_LINE_CODING, which configures rate, stop bits, parity and data bits.
const REQUEST_SET_LINE_CODING: u8 = 0x20;
// CDC 1.1 Table 4: GET_LINE_CODING, which reads the current line coding back.
const REQUEST_GET_LINE_CODING: u8 = 0x21;
// CDC 1.1 Table 4: SET_CONTROL_LINE_STATE, the RS-232 DTR and RTS signals. Accepted and
// discarded.
const REQUEST_SET_CONTROL_LINE_STATE: u8 = 0x22;
// CDC 1.1 Table 4: SEND_BREAK. Accepted so the host sees no stall, then ignored.
const REQUEST_SEND_BREAK: u8 = 0x23;

// CDC PSTN 1.2 Table 17: the line coding structure is 4 + 1 + 1 + 1 bytes.
const LINE_CODING_BYTES: usize = 7;
// CDC 1.1 Table 50: dwDTERate is four little-endian bytes at offset 0.
const LINE_CODING_RATE_BYTES: usize = 4;
// CDC 1.1 Table 50: bCharFormat is at offset 4.
const LINE_CODING_STOP_BITS_OFFSET: usize = 4;
// CDC 1.1 Table 50: bParityType is at offset 5.
const LINE_CODING_PARITY_OFFSET: usize = 5;
// CDC PSTN 1.2 Table 17: bDataBits is at offset 6, the last byte.
const LINE_CODING_DATA_BITS_OFFSET: usize = 6;

// No UART sits behind this class. The rate is stored only to answer GET_LINE_CODING before
// a host sets its own.
const DEFAULT_DATA_RATE: u32 = 115_200;
// CDC 1.1 Table 50: bCharFormat 0 is one stop bit.
const STOP_BITS_ONE: u8 = 0;
// CDC 1.1 Table 50: bParityType 0 is no parity.
const PARITY_NONE: u8 = 0;
// CDC PSTN 1.2 Table 17: bDataBits is 5, 6, 7, 8 or 16. Stored and echoed, never acted on.
const DATA_BITS_EIGHT: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCoding {
    pub data_rate: u32,
    pub stop_bits: u8,
    pub parity: u8,
    pub data_bits: u8,
}

impl Default for LineCoding {
    fn default() -> Self {
        Self {
            data_rate: DEFAULT_DATA_RATE,
            stop_bits: STOP_BITS_ONE,
            parity: PARITY_NONE,
            data_bits: DATA_BITS_EIGHT,
        }
    }
}

impl LineCoding {
    fn encode(self) -> [u8; LINE_CODING_BYTES] {
        let [rate_byte_0, rate_byte_1, rate_byte_2, rate_byte_3] = self.data_rate.to_le_bytes();

        [
            rate_byte_0,
            rate_byte_1,
            rate_byte_2,
            rate_byte_3,
            self.stop_bits,
            self.parity,
            self.data_bits,
        ]
    }

    fn decode(payload: &[u8]) -> Option<Self> {
        let rate_bytes = payload.get(..LINE_CODING_RATE_BYTES)?;
        let rate: [u8; LINE_CODING_RATE_BYTES] = rate_bytes.try_into().ok()?;

        Some(Self {
            data_rate: u32::from_le_bytes(rate),
            stop_bits: *payload.get(LINE_CODING_STOP_BITS_OFFSET)?,
            parity: *payload.get(LINE_CODING_PARITY_OFFSET)?,
            data_bits: *payload.get(LINE_CODING_DATA_BITS_OFFSET)?,
        })
    }
}

fn header_functional_descriptor() -> [u8; 3] {
    let [release_low, release_high] = CDC_SPECIFICATION_RELEASE.to_le_bytes();

    [HEADER_SUBTYPE, release_low, release_high]
}

fn call_management_functional_descriptor(data_interface: u8) -> [u8; 3] {
    [
        CALL_MANAGEMENT_SUBTYPE,
        CALL_MANAGEMENT_CAPABILITIES,
        data_interface,
    ]
}

fn abstract_control_management_functional_descriptor() -> [u8; 2] {
    [
        ABSTRACT_CONTROL_MANAGEMENT_SUBTYPE,
        ABSTRACT_CONTROL_MANAGEMENT_CAPABILITIES,
    ]
}

fn union_functional_descriptor(control_interface: u8, subordinate_interface: u8) -> [u8; 3] {
    [UNION_SUBTYPE, control_interface, subordinate_interface]
}

fn addresses_control_interface(request: &Request, control_interface: u8) -> bool {
    request.request_type == RequestType::Class
        && request.recipient == Recipient::Interface
        && request.index == u16::from(control_interface)
}

pub struct CdcAcmClass<'a, B: UsbBus> {
    control_interface: InterfaceNumber,
    data_interface: InterfaceNumber,
    notification_endpoint: EndpointIn<'a, B>,
    bulk_in_endpoint: EndpointIn<'a, B>,
    bulk_out_endpoint: EndpointOut<'a, B>,
    line_coding: LineCoding,
}

impl<'a, B: UsbBus> CdcAcmClass<'a, B> {
    pub fn new(allocator: &'a UsbBusAllocator<B>) -> Result<Self> {
        let control_interface = allocator.interface();
        let data_interface = allocator.interface();

        // Declare endpoint to go from rp2350 to the host so that we can inform 
        // about status for this communication line
        let notification_endpoint = allocator.alloc::<In>(
            None,
            EndpointType::Interrupt,
            NOTIFICATION_PACKET_SIZE,
            NOTIFICATION_INTERVAL_FRAMES,
        )?;

        // Declare endpoint to go from rp2350 to the host
        // this is where data will travel from rp2350
        let bulk_in_endpoint = allocator.alloc::<In>(
            None,
            EndpointType::Bulk,
            BULK_PACKET_SIZE,
            BULK_INTERVAL_FRAMES,
        )?;

        // Declare endpoint to go from host to rp2350
        let bulk_out_endpoint = allocator.alloc::<Out>(
            None,
            EndpointType::Bulk,
            BULK_PACKET_SIZE,
            BULK_INTERVAL_FRAMES,
        )?;


        Ok(Self {
            control_interface,
            data_interface,
            notification_endpoint,
            bulk_in_endpoint,
            bulk_out_endpoint,
            line_coding: LineCoding::default(),
        })
    }

    /// Reads from the output endpoint from the host
    pub fn read_packet(&self, buffer: &mut [u8]) -> Result<usize> {
        self.bulk_out_endpoint.read(buffer)
    }

    /// Writes a packet via the rp2350's data endpoint directed toward host
    pub fn write_packet(&self, data: &[u8]) -> Result<usize> {
        if data.len() > usize::from(BULK_PACKET_SIZE) {
            return Err(UsbError::BufferOverflow);
        }

        self.bulk_in_endpoint.write(data)
    }
}

impl<B: UsbBus> UsbClass<B> for CdcAcmClass<'_, B> {
    /// explains to the host what kind of device this is in terms of config
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        let control_interface = u8::from(self.control_interface);
        let data_interface = u8::from(self.data_interface);

        // start at interface 0
        // span 2 interface
        // it is a comms device
        // we use the ACM port
        // no specific protocol
        // no human readable name
        writer.iad(
            self.control_interface,
            INTERFACES_PER_FUNCTION,
            COMMUNICATIONS_INTERFACE_CLASS,
            ABSTRACT_CONTROL_MODEL_SUBCLASS,
            NO_CLASS_SPECIFIC_PROTOCOL,
            None,
        )?;

        // the first control inferface
        // this interface controls the line
        // virtual serial port
        // no specific proto used
        writer.interface(
            self.control_interface,
            COMMUNICATIONS_INTERFACE_CLASS,
            ABSTRACT_CONTROL_MODEL_SUBCLASS,
            NO_CLASS_SPECIFIC_PROTOCOL,
        )?;

        writer.write(
            CLASS_SPECIFIC_INTERFACE_DESCRIPTOR,
            &header_functional_descriptor(),
        )?;

        writer.write(
            CLASS_SPECIFIC_INTERFACE_DESCRIPTOR,
            &call_management_functional_descriptor(data_interface),
        )?;

        writer.write(
            CLASS_SPECIFIC_INTERFACE_DESCRIPTOR,
            &abstract_control_management_functional_descriptor(),
        )?;

        writer.write(
            CLASS_SPECIFIC_INTERFACE_DESCRIPTOR,
            &union_functional_descriptor(control_interface, data_interface),
        )?;

        // register endpoint
        writer.endpoint(&self.notification_endpoint)?;

        // register the data interface
        writer.interface(
            self.data_interface,
            DATA_INTERFACE_CLASS,
            DATA_INTERFACE_SUBCLASS,
            DATA_INTERFACE_PROTOCOL,
        )?;

        // add the endpoints for data interface
        writer.endpoint(&self.bulk_in_endpoint)?;
        writer.endpoint(&self.bulk_out_endpoint)?;

        Ok(())
    }


    fn reset(&mut self) {
        self.line_coding = LineCoding::default();
    }

    /// Answer control in request
    fn control_in(&mut self, transfer: ControlIn<B>) {
        let request = *transfer.request();

        // Only accept whitelisted requests
        if !addresses_control_interface(&request, u8::from(self.control_interface)) {
            return;
        }

        // If the request is a get line coding, we return that
        if request.request == REQUEST_GET_LINE_CODING {
            let encoded = self.line_coding.encode();
            let _ = transfer.accept_with(&encoded);
            return;
        }
        // otherwise reject
        let _ = transfer.reject();
    }


    /// Handles control out request from host
    fn control_out(&mut self, transfer: ControlOut<B>) {
        let request = *transfer.request();

        // same as above
        if !addresses_control_interface(&request, u8::from(self.control_interface)) {
            return;
        }

        // Match the request to whitelist
        match request.request {
            REQUEST_SET_LINE_CODING => match LineCoding::decode(transfer.data()) {
                Some(line_coding) => {
                    self.line_coding = line_coding;
                    // we accept only if we are able to parse it
                    let _ = transfer.accept();
                }
                None => {
                    let _ = transfer.reject();
                }
            },
            // There's no DTR/RTS pin and no serial line to send a break on, so there's nothing to do
            REQUEST_SET_CONTROL_LINE_STATE | REQUEST_SEND_BREAK => {
                let _ = transfer.accept();
            }
            _ => {
                let _ = transfer.reject();
            }
        }
    }
}
