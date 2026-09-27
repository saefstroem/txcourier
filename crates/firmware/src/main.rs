#![no_std]
#![no_main]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code,
    unused_must_use
)]

mod clocks;
mod error;
mod fault;
mod host;
mod led;
mod result;
mod signing;
mod storage;
mod usb;

use core::convert::Infallible;
use core::panic::PanicInfo;

use rp235x_hal as hal;

use hal::block::ImageDef;
use hal::gpio::Pins;
use hal::pac::Peripherals;
use hal::usb::UsbBus;
use hal::{Sio, Timer, Watchdog};
use rp2350wallet_protocol::FAULT_BYTE;
use rp2350wallet_protocol::policy::approve;
use rp2350wallet_protocol::token::decode as decode_tokens;
use rp2350wallet_protocol::wire::{
    MAX_FRAME_LEN, Refusal, Request, Response, Slot, TRANSACTION_CAPACITY, Transaction,
};
use usb_device::bus::UsbBusAllocator;

use crate::error::Error;
use crate::host::Bus;
use crate::result::Result;
use crate::signing::SigningAttempts;
use crate::storage::Vault;
use crate::usb::Link;

#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: ImageDef = ImageDef::secure_exe();

#[hal::entry]
fn main() -> ! {
    let _ = start();
    fault::halt()
}

fn start() -> Result<Infallible> {
    // initialize rp2350 peripherals
    let mut pac = Peripherals::take().ok_or(Error::PeripheralsUnavailable)?;

    // create the watchdog in order to guarantee watchdog ownership
    let mut watchdog = Watchdog::new(pac.WATCHDOG);

    // init clocks and ticks
    let clocks = clocks::init(
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )?;


    // create abstraction of timer peripheral 
    let timer = Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);

    // create the SIO, these are wired directly to the core which
    // means we can access them without contention or negotiation
    let sio = Sio::new(pac.SIO);

    // Initialize the pins
    let pins = Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // vault is our abstraction for persistent storage
    let mut vault = Vault::new();
    announce(&vault);

    // move the gpio pins to ownerhsip of the LED
    led::adopt(pins.gpio0, pins.gpio4);

    // Create the USB C bus allocator to acquire endpoint address
    // this is computed by the rp2350
    let allocator = UsbBusAllocator::new(UsbBus::new(
        pac.USB,
        pac.USB_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    // setup the usb bus for host mode, which
    // is going to be communicating with the ledger
    let mut bus = Bus::new(
        pac.PIO0,
        pac.PIO1,
        &mut pac.RESETS,
        pins.gpio12,
        pins.gpio13,
        timer,
        led::tick,
    )?;
    let mut link = Link::new(&allocator)?;
    let mut signing = SigningAttempts::new();
    let mut frame = [0u8; MAX_FRAME_LEN];

    // Enter main system loop
    loop {
        // Get current time
        let now_us = timer.get_counter().ticks();

        // Poll the link for connected host
        if link.poll() {
            // Serve the request
            serve(&mut link, &mut vault, &signing, &mut frame, now_us)?;
        }

        // Check if we need to do anything
        signing.attend(&mut bus, &mut vault, now_us);

        // Announce our status
        announce(&vault);
        // Reflect any kind of state changes on the LED
        led::tick();
    }
}

/// Announce the state of the system via the LED
fn announce(vault: &Vault) {
    match vault.occupancy().state() {
        Ok(state) => led::publish(state),
        Err(_) => led::declare_fault(),
    }
}

/// Attempt to add a transaction to the vault
fn admit(vault: &mut Vault, transaction: &[u8]) -> Response {
    if let Err(error) = approve(transaction) {
        return refused(Refusal::from(error));
    }

    match vault.store_pending(transaction) {
        Ok(()) => Response::Acknowledged,
        Err(error) => refused(Refusal::from(error)),
    }
}

/// Attempt to store tokens in the system
fn provision(vault: &mut Vault, blob: &[u8]) -> Response {
    if let Err(error) = vault.occupancy().may_store_pending() {
        return refused(Refusal::from(error));
    }

    if let Err(error) = decode_tokens(blob) {
        return refused(Refusal::from(error));
    }

    match vault.store_tokens(blob) {
        Ok(()) => Response::Acknowledged,
        Err(error) => refused(Refusal::from(error)),
    }
}

const fn refused(refusal: Refusal) -> Response {
    Response::Refused { refusal, detail: 0 }
}

/// Read the stored transaction 
fn stored(len: usize, bytes: &[u8]) -> Result<Response> {
    Ok(Response::Slot {
        transaction: Transaction::from_slice(bytes.get(..len).unwrap_or(&[]))?,
    })
}

fn serve(
    link: &mut Link<'_, UsbBus>,
    vault: &mut Vault,
    signing: &SigningAttempts,
    frame: &mut [u8],
    now_us: u64,
) -> Result<()> {
    // Read the request
    let request = {
        let Some(received) = link.receive(now_us)? else {
            return Ok(());
        };
        match Request::decode(received) {
            Ok(request) => request,
            Err(_) => return Ok(()),
        }
    };

    // Compute a response
    let response = match request {
        Request::Status => Response::Status { // return status
            state: match vault.occupancy().state() {
                Ok(state) => state.to_byte(),
                Err(_) => FAULT_BYTE,
            },
            provisioned: vault.provisioned(),
            refusal: signing.refusal(),
            detail: signing.detail(),
        },
        // Attempt to set a transaction
        Request::Put(transaction) => admit(vault, transaction.as_slice()),

        // Attempt to read a well defined flash slot
        Request::Get(slot) => {
            let mut bytes = [0u8; TRANSACTION_CAPACITY];
            let len = match slot {
                Slot::Pending => vault.read_pending(&mut bytes),
                Slot::Signed => vault.read_signed(&mut bytes),
            };
            stored(len, &bytes)?
        }
        // Attempt to clear a slot
        Request::Clear(slot) => {
            let cleared = match slot {
                Slot::Pending => vault.clear_pending(),
                Slot::Signed => vault.clear_signed(),
            };
            match cleared {
                Ok(()) => Response::Acknowledged,
                Err(error) => refused(Refusal::from(error)),
            }
        }
        // Attempt to provision tokens for the system
        Request::Provision(tokens) => provision(vault, tokens.as_slice()),
    };

    let written = response.encode(frame)?;

    // check if we have anything to send back
    let Some(reply) = frame.get(..written) else {
        return Ok(());
    };

    // Send the reply 
    link.write(reply)?;
    Ok(())
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    fault::halt()
}
