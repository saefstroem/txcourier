use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU8, Ordering};

use rp235x_hal as hal;

use hal::gpio::bank0::{Gpio0, Gpio4};
use hal::gpio::{DefaultTypeState, Pin, PinState};
use hal::pac::{SIO, TIMER0};
use txcourier_protocol::{DeviceState, FAULT_BYTE, lamp_is_lit};

// RP2350 datasheet, SIO GPIO_OUT_SET and GPIO_OUT_CLR: bit n is GPIO n. The LED anode is on
// GP0.
const ANODE_MASK: u32 = 1 << 0;
// RP2350 datasheet, SIO GPIO_OUT_SET and GPIO_OUT_CLR: bit n is GPIO n. The LED cathode is
// on GP4 and is only ever driven low.
const CATHODE_MASK: u32 = 1 << 4;

pub type AnodePin =
    Pin<Gpio0, <Gpio0 as DefaultTypeState>::Function, <Gpio0 as DefaultTypeState>::PullType>;

pub type CathodePin =
    Pin<Gpio4, <Gpio4 as DefaultTypeState>::Function, <Gpio4 as DefaultTypeState>::PullType>;

static STATE: AtomicU8 = AtomicU8::new(FAULT_BYTE);

/// initializes the LED pins
pub fn adopt(anode: AnodePin, cathode: CathodePin) {
    let _ = anode.into_push_pull_output_in_state(PinState::Low);
    let _ = cathode.into_push_pull_output_in_state(PinState::Low);
}

pub fn publish(state: DeviceState) {
    STATE.store(state.to_byte(), Ordering::Relaxed);
}

pub fn declare_fault() {
    STATE.store(FAULT_BYTE, Ordering::Relaxed);
}

/// Flashes the LED
pub fn tick() {
    let sio = unsafe { &*SIO::ptr() };
    let timer = unsafe { &*TIMER0::ptr() };
    let now_us = unsafe { read_volatile(timer.timerawl().as_ptr()) };

    unsafe {
        // compute whether the lamp should be turned on
        if lamp_is_lit(now_us, STATE.load(Ordering::Relaxed)) {
            // turn off cathode
            // and turn on anode
            write_volatile(sio.gpio_out_clr().as_ptr(), CATHODE_MASK);
            write_volatile(sio.gpio_out_set().as_ptr(), ANODE_MASK);
        } else {
            // turn off both
            write_volatile(sio.gpio_out_clr().as_ptr(), ANODE_MASK | CATHODE_MASK);
        }
    }
}
