use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use pio::{Assembler, JmpCondition, OutDestination, Program, SideSet};
use rp235x_hal as hal;

use hal::gpio::bank0::Gpio16;
use hal::gpio::{DefaultTypeState, FunctionPio2, Pin};
use hal::pac::{PIO2, RESETS, TIMER0};
use hal::pio::{Buffers, InstallError, PIOBuilder, PIOExt, PinDir, ShiftDirection};
use txcourier_protocol::{DeviceState, FAULT_BYTE, lamp_is_lit};

use crate::clocks::SYS_HZ;

const LED_PIN: u8 = 16;
const BIT_HZ: u32 = 800_000;
const CYCLES_PER_BIT: u32 = 10;
const DIVISOR: u32 = SYS_HZ / (BIT_HZ * CYCLES_PER_BIT);
const _: () = assert!(SYS_HZ.is_multiple_of(BIT_HZ * CYCLES_PER_BIT) && DIVISOR <= u16::MAX as u32);
const PROGRAM_SPACE: usize = 32;
const COLOR_BITS: u8 = 24;
const MACHINE: usize = 0;
const LIT: u32 = 0x10_00_00 << (u32::BITS - COLOR_BITS as u32);
const DARK: u32 = 0;

pub type LedPin =
    Pin<Gpio16, <Gpio16 as DefaultTypeState>::Function, <Gpio16 as DefaultTypeState>::PullType>;

static STATE: AtomicU8 = AtomicU8::new(FAULT_BYTE);
static READY: AtomicBool = AtomicBool::new(false);
static SHOWN: AtomicBool = AtomicBool::new(false);

/// initializes the LED pins
pub fn adopt(block: PIO2, pin: LedPin, resets: &mut RESETS) -> Result<(), InstallError> {
    let _ = pin.into_function::<FunctionPio2>();
    let (mut block, machine, _, _, _) = block.split(resets);
    let installed = block.install(&program())?;
    let (mut machine, _, mut colors) = PIOBuilder::from_installed_program(installed)
        .side_set_pin_base(LED_PIN) // set pin level as a side effect of each inst
        .clock_divisor_fixed_point(DIVISOR as u16, 0) // set the clock divisor for the PIO machine
        .out_shift_direction(ShiftDirection::Left) // G R B is the order of the color bits
        .autopull(true)
        .pull_threshold(COLOR_BITS)
        .buffers(Buffers::OnlyTx) // use only the TX buffer for the PIO machine
        .build(machine); // finalize the build of the state machine
    machine.set_pindirs([(LED_PIN, PinDir::Output)]);
    let _ = machine.start();
    let _ = colors.write(DARK);
    READY.store(true, Ordering::Relaxed);
    Ok(())
}

pub fn publish(state: DeviceState) {
    STATE.store(state.to_byte(), Ordering::Relaxed);
}

pub fn declare_fault() {
    STATE.store(FAULT_BYTE, Ordering::Relaxed);
}

/// Flashes the LED
pub fn tick() {
    if !READY.load(Ordering::Relaxed) {
        return;
    }
    let timer = unsafe { &*TIMER0::ptr() };
    let now_us = unsafe { read_volatile(timer.timerawl().as_ptr()) };

    // compute whether the lamp should be turned on
    let lit = lamp_is_lit(now_us, STATE.load(Ordering::Relaxed));
    if SHOWN.swap(lit, Ordering::Relaxed) == lit {
        return;
    }

    let pio = unsafe { &*PIO2::ptr() };
    unsafe { write_volatile(pio.txf(MACHINE).as_ptr(), if lit { LIT } else { DARK }) };
}

fn program() -> Program<PROGRAM_SPACE> {
    let mut assembler =
        Assembler::<PROGRAM_SPACE>::new_with_side_set(SideSet::new(false, 1, false));
    let mut wrap_target = assembler.label();
    let mut wrap_source = assembler.label();
    let mut zero = assembler.label();

    assembler.bind(&mut wrap_target);// make this the wrap target
    // output one bit to the X register and set the side pin accordingly to low
    assembler.out_with_delay_and_side_set(OutDestination::X, 1, 3, 0); 
    // jump to zero label if X is zero, otherwise continue, set pin to high
    assembler.jmp_with_delay_and_side_set(JmpCondition::XIsZero, &mut zero, 2, 1);
    // jump to wrap target otherwise, set pin to high
    assembler.jmp_with_delay_and_side_set(JmpCondition::Always, &mut wrap_target, 2, 1);
    // bind the zero label
    assembler.bind(&mut zero);
    // set the side pin to low for the zero condition
    assembler.nop_with_delay_and_side_set(2, 0);
    // bind the wrap source label
    assembler.bind(&mut wrap_source); // after this, the program will loop back to the wrap target

    // finalize with wrap
    assembler.assemble_with_wrap(wrap_source, wrap_target)
}
