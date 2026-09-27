use rp2350wallet_protocol::wire::{HEADER_LEN, MAX_FRAME_LEN, frame_len};
use usb_device::UsbError;
use usb_device::bus::{UsbBus, UsbBusAllocator};
use usb_device::device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid};

use crate::usb::error::Error;
use crate::usb::result::Result;
use crate::usb::serial::{BULK_PACKET_SIZE, CdcAcmClass};

// pid.codes: VID 0x1209 is allocated for open-source hardware under its licence terms.
const VENDOR_ID: u16 = 0x1209;
// pid.codes: PID 0x0001 is the test PID, for in-house use only. A distributed device needs an
// allocated PID.
const PRODUCT_ID: u16 = 0x0001;
// USB 2.0 section 5.5.3: a full-speed endpoint zero uses 8, 16, 32 or 64 bytes.
const CONTROL_PACKET_SIZE: u8 = 64;

// A free-form string descriptor. Nothing matches on it.
const MANUFACTURER: &str = "4NT1DR41N";
// A free-form string descriptor. Nothing matches on it.
const PRODUCT: &str = "L3DG3RM1TM";
// macOS names the callout node /dev/cu.usbmodem followed by this serial, which the CLI
// matches by prefix. It is fixed, so two couriers on one host are indistinguishable.
const SERIAL_NUMBER: &str = "0001";

// One full frame plus one bulk packet. The HAL reports overflow rather than a short read, so
// a whole packet must fit behind a partial frame.
const INBOX_LEN: usize = MAX_FRAME_LEN + BULK_PACKET_SIZE as usize;
// An iteration count, not a time. Exceeding it is an error, and any error out of serve halts
// the device until power cycle.
const WRITE_POLL_LIMIT: u32 = 1_000_000;
// Not fixed by any specification: 200 ms after a frame's first packet its remains are
// dropped, so the next request is not spliced onto them.
const PARTIAL_FRAME_TIMEOUT_US: u64 = 200_000;

/// The connection between the rp2350 and the host machine
pub struct Link<'a, B: UsbBus> {
    device: UsbDevice<'a, B>,
    serial: CdcAcmClass<'a, B>,
    inbox: [u8; INBOX_LEN],
    filled: usize,
    partial_since: u64,
}

impl<'a, B: UsbBus> Link<'a, B> {
    pub fn new(allocator: &'a UsbBusAllocator<B>) -> Result<Self> {
        // claims the serial endpoints
        let serial = CdcAcmClass::new(allocator)?;

        // Describes the device to whoever connects to it
        let device = UsbDeviceBuilder::new(allocator, UsbVidPid(VENDOR_ID, PRODUCT_ID))
            .composite_with_iads()
            .max_packet_size_0(CONTROL_PACKET_SIZE)?
            .strings(&[StringDescriptors::default()
                .manufacturer(MANUFACTURER)
                .product(PRODUCT)
                .serial_number(SERIAL_NUMBER)])?
            .build();

        Ok(Self {
            device,
            serial,
            inbox: [0; INBOX_LEN],
            filled: 0,
            partial_since: 0,
        })
    }

    /// Answers any control requests
    pub fn poll(&mut self) -> bool {
        self.device.poll(&mut [&mut self.serial])
    }

    /// Receive data from the host (not us)
    pub fn receive(&mut self, now_us: u64) -> Result<Option<&[u8]>> {
        // if frame is older than 200ms discard it
        self.discard_stale_frame(now_us);

        // Get free mutable bytes that we cna use
        let Some(free) = self.inbox.get_mut(self.filled..) else {
            self.filled = 0;
            return Ok(None);
        };

        // Read a packet from the serial bus
        let read = match self.serial.read_packet(free) {
            Ok(read) => read,
            Err(UsbError::WouldBlock) => return Ok(None),
            Err(error) => return Err(error.into()),
        };

        // If this is a new transmission mark it as a partial reception from time now
        if self.filled == 0 {
            self.partial_since = now_us;
        }

        // Increment how much we have read
        self.filled += read;

        // If its less than a header this is probably malformed or not enough yet.
        if self.filled < HEADER_LEN {
            return Ok(None);
        }

        // Read the header
        let Some(header) = self.inbox.get(..HEADER_LEN) else {
            return Ok(None);
        };

        // Compute size of the frame
        let Ok(total) = frame_len(header) else {
            self.filled = 0;
            return Ok(None);
        };

        // If we didnt get everythign yet there is more
        if self.filled < total {
            return Ok(None);
        }

        // Otherwise reset filled state and return everything
        self.filled = 0;
        Ok(self.inbox.get(..total))
    }

    /// Remove frame that is older than 200ms
    fn discard_stale_frame(&mut self, now_us: u64) {
        // If we havent filled anything it means we havent read any frames
        if self.filled == 0 {
            return;
        }

        if now_us.wrapping_sub(self.partial_since) > PARTIAL_FRAME_TIMEOUT_US {
            self.filled = 0;
        }
    }

    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        // For each chunk
        for chunk in data.chunks(usize::from(BULK_PACKET_SIZE)) {
            let mut sent = false;

            // Write packet to the host from rp2350
            for _ in 0..WRITE_POLL_LIMIT {
                match self.serial.write_packet(chunk) {
                    Ok(_) => {
                        sent = true;
                        break;
                    }
                    Err(UsbError::WouldBlock) => {
                        self.poll();
                    }
                    Err(error) => return Err(error.into()),
                }
            }

            if !sent {
                return Err(Error::Endpoint(UsbError::WouldBlock));
            }
        }

        Ok(())
    }
}
