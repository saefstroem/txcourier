use rp235x_hal as hal;

use hal::arch::{interrupt_disable, nop};

use crate::led;

pub fn halt() -> ! {
    led::declare_fault();
    led::tick();
    interrupt_disable();
    loop {
        nop();
    }
}
