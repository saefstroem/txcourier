//! Blink the onboard WS2812 RGB LED on the Waveshare RP2350-USB-A.
//!
//! Build & flash:
//!     cargo run --release
//!
//! The onboard LED is a WS2812, not a plain LED, so it needs a timed
//! serial waveform. That is driven here by a PIO state machine.
//!
//! LED_PIN is a guess (GP16, as used on the RP2350-Zero). Confirm it on
//! the pinout diagram at waveshare.com/wiki/RP2350-USB-A. If nothing
//! lights up, change BOTH `LED_PIN` and the `pins.gpioNN` line below,
//! then reflash.

#![no_std]
#![no_main]

use panic_halt as _;
use rp235x_hal as hal;

use embedded_hal::delay::DelayNs;
use hal::gpio::{FunctionPio0, Pin, PullNone};
use hal::pio::{PIOBuilder, PIOExt, PinDir, ShiftDirection};
use hal::Sio;

/// Tell the boot ROM about our application. Without this the RP2350
/// will flash the binary but refuse to run it.
#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

/// GPIO the WS2812 data line is wired to.
const LED_PIN: u8 = 16;

/// Crystal on the board, in Hz.
const XTAL_HZ: u32 = 12_000_000;

/// WS2812 bit timing, in PIO cycles. 10 cycles per bit total.
const T1: u8 = 2;
const T2: u8 = 5;
const T3: u8 = 3;

#[hal::entry]
fn main() -> ! {
    let mut pac = hal::pac::Peripherals::take().unwrap();
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);

    let clocks = hal::clocks::init_clocks_and_plls(
        XTAL_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .unwrap();

    let sio = Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let mut timer = hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);

    // Hand the LED pin over to PIO0. Must match LED_PIN.
    let _led: Pin<_, FunctionPio0, PullNone> = pins.gpio16.reconfigure();

    // --- WS2812 PIO program -------------------------------------------------
    //
    //   bitloop:
    //     out x, 1         side 0 [T3-1]   ; shift out next bit, line low
    //     jmp !x do_zero   side 1 [T1-1]   ; line high; branch on bit value
    //     jmp bitloop      side 1 [T2-1]   ; a '1': stay high longer
    //   do_zero:
    //     nop              side 0 [T2-1]   ; a '0': drop low early
    //
    let side_set = pio::SideSet::new(false, 1, false);
    let mut a = pio::Assembler::<32>::new_with_side_set(side_set);

    let mut wrap_target = a.label();
    let mut wrap_source = a.label();
    let mut do_zero = a.label();

    a.bind(&mut wrap_target);
    a.out_with_delay_and_side_set(pio::OutDestination::X, 1, T3 - 1, 0);
    a.jmp_with_delay_and_side_set(pio::JmpCondition::XIsZero, &mut do_zero, T1 - 1, 1);
    a.jmp_with_delay_and_side_set(pio::JmpCondition::Always, &mut wrap_target, T2 - 1, 1);
    a.bind(&mut do_zero);
    a.nop_with_delay_and_side_set(T2 - 1, 0);
    a.bind(&mut wrap_source);

    let program = a.assemble_with_wrap(wrap_source, wrap_target);

    let (mut pio, sm0, _, _, _) = pac.PIO0.split(&mut pac.RESETS);
    let installed = pio.install(&program).unwrap();

    // WS2812 runs at 800 kHz; 10 PIO cycles per bit means an 8 MHz state
    // machine. 150 MHz / 8 MHz = 18.75 -> integer 18, fraction 0.75*256 = 192.
    let (div_int, div_frac) = (18u16, 192u8);

    let (mut sm, _rx, mut tx) = PIOBuilder::from_installed_program(installed)
        .side_set_pin_base(LED_PIN)
        .out_shift_direction(ShiftDirection::Left)
        .autopull(true)
        .pull_threshold(24)
        .clock_divisor_fixed_point(div_int, div_frac)
        .build(sm0);

    sm.set_pindirs([(LED_PIN, PinDir::Output)]);
    let _sm = sm.start();

    // --- Colour cycle -------------------------------------------------------
    // Kept dim on purpose: these LEDs are uncomfortably bright at full scale.
    let colours = [
        grb(24, 0, 0),  // red
        grb(0, 24, 0),  // green
        grb(0, 0, 24),  // blue
        grb(0, 0, 0),   // off
    ];

    let mut i = 0usize;
    loop {
        // 24 bits of colour, left-aligned in the 32-bit FIFO word.
        let word = colours[i % colours.len()] << 8;
        while !tx.write(word) {}
        timer.delay_ms(400);
        i = i.wrapping_add(1);
    }
}

/// Pack r/g/b into the WS2812 wire order, which is green-red-blue.
const fn grb(r: u8, g: u8, b: u8) -> u32 {
    ((g as u32) << 16) | ((r as u32) << 8) | (b as u32)
}