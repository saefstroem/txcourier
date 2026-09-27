use pio::{
    Assembler, Instruction, InstructionOperands, IrqIndexMode, JmpCondition, OutDestination,
    Program, SetDestination, SideSet,
};

// Pico-PIO-USB's flag number for the transmitter. The CPU polls it; nothing routes it to the
// NVIC.
pub const IRQ_END_OF_PACKET: u8 = 0;

// out pc, 2 loads only the low two bits of PC, so the symbol instructions must sit at
// absolute addresses 0 to 3.
pub const ORIGIN: u8 = 0;

// The symbol that drives SE0 and falls into a driven J: the USB end-of-packet.
pub const ADDRESS_END_OF_PACKET: u8 = 0;
// The out pc that side-sets J. The EOP at address 0 falls into it for its trailing J bit.
pub const ADDRESS_DRIVE_J: u8 = 1;
// The set pindirs, 0 that tri-states both lines. It falls through into the K symbol.
pub const ADDRESS_RELEASE_BUS: u8 = 2;
// The out pc that side-sets K, and the highest address a two-bit symbol can reach.
pub const ADDRESS_DRIVE_K: u8 = 3;
// Above the four symbol addresses, so no symbol can jump here. Only the CPU sets PC to it.
pub const ADDRESS_ENABLE_DRIVERS: u8 = 4;

// Two bit times of SE0 must fit in one instruction. With a 2-bit side-set the delay field is
// 3 bits, so an instruction spans at most 8 cycles.
pub const CYCLES_PER_BIT: u32 = 4;

// RP2350 datasheet: a PIO block's instruction memory holds 32 words.
const PROGRAM_SPACE: usize = 32;

// One side-set bit per data line, so every cycle drives both D+ and D-.
const SIDE_SET_BITS: u8 = 2;
// An enable bit would come out of the delay field, leaving no instruction long enough for
// the EOP's SE0. Every instruction, including those pushed through SMx_INSTR, carries one.
const SIDE_SET_OPTIONAL: bool = false;
// Side-set drives levels, which change every bit. SET owns the directions, which change only
// at packet edges.
const SIDE_SET_TO_PINDIRS: bool = false;

// USB 2.0: full-speed J is D+ high and D- low. Side-set bit 0 is D+.
const SIDE_DRIVE_J: u8 = 0b01;
// USB 2.0: full-speed K is D- high and D+ low.
const SIDE_DRIVE_K: u8 = 0b10;
// USB 2.0: end-of-packet is SE0, both lines driven low.
const SIDE_SINGLE_ENDED_ZERO: u8 = 0b00;

// Pin direction 0 is input. Both lines go high impedance, as USB requires between packets.
const PINDIRS_RELEASED: u8 = 0b00;
// Pin direction 1 is output, one bit per data line.
const PINDIRS_DRIVING: u8 = 0b11;

// Four wire actions fit in two bits. out pc makes each symbol a jump target.
const SYMBOL_BITS: u8 = 2;
// The instruction itself takes one cycle; three delay cycles complete a four-cycle bit.
const REST_OF_BIT_TIME: u8 = 3;
// 2 * CYCLES_PER_BIT - 1. USB 2.0 signals end-of-packet with SE0 for two bit times.
const TWO_BIT_TIMES: u8 = 7;

// These instructions run one at a time through SMx_INSTR, outside the bit loop.
const NO_DELAY: u8 = 0;

// out pc, 2 loads only the low two bits of PC, so a symbol reaches addresses 0 to 3.
const ADDRESSES_A_SYMBOL_CAN_REACH: u8 = 1 << SYMBOL_BITS;

// Re-arms the transmitter. The jump's own side-set latches J before address 4 enables the
// drivers, so a packet never opens with an SE0 glitch.
pub const START: Instruction = Instruction {
    operands: InstructionOperands::JMP {
        condition: JmpCondition::Always,
        address: ADDRESS_ENABLE_DRIVERS,
    },
    delay: NO_DELAY,
    side_set: Some(SIDE_DRIVE_J),
};

// Holds SE0 from SMx_INSTR for the bus reset. Pico-PIO-USB uses the pad output overrides
// instead.
pub const DRIVE_SINGLE_ENDED_ZERO: Instruction = drive(SIDE_SINGLE_ENDED_ZERO);
// Holds the bus driven at idle J.
pub const DRIVE_J: Instruction = drive(SIDE_DRIVE_J);
// Holds the bus driven at K.
pub const DRIVE_K: Instruction = drive(SIDE_DRIVE_K);

// Tri-states both lines while latching J, so the bus falls back to its pull resistors.
pub const RELEASE: Instruction = Instruction {
    operands: InstructionOperands::SET {
        destination: SetDestination::PINDIRS,
        data: PINDIRS_RELEASED,
    },
    delay: NO_DELAY,
    side_set: Some(SIDE_DRIVE_J),
};

const fn drive(line_state: u8) -> Instruction {
    Instruction {
        operands: InstructionOperands::SET {
            destination: SetDestination::PINDIRS,
            data: PINDIRS_DRIVING,
        },
        delay: NO_DELAY,
        side_set: Some(line_state),
    }
}

const _: () = assert!(ADDRESS_END_OF_PACKET < ADDRESSES_A_SYMBOL_CAN_REACH);
const _: () = assert!(ADDRESS_DRIVE_J < ADDRESSES_A_SYMBOL_CAN_REACH);
const _: () = assert!(ADDRESS_RELEASE_BUS < ADDRESSES_A_SYMBOL_CAN_REACH);
const _: () = assert!(ADDRESS_DRIVE_K < ADDRESSES_A_SYMBOL_CAN_REACH);
const _: () = assert!(ADDRESS_ENABLE_DRIVERS >= ADDRESSES_A_SYMBOL_CAN_REACH);
const _: () = assert!(REST_OF_BIT_TIME as u32 + 1 == CYCLES_PER_BIT);
const _: () = assert!(TWO_BIT_TIMES as u32 + 1 == 2 * CYCLES_PER_BIT);

/// Essentially this program goes to either #1 or #3 drives the needed pins then
/// moves on to the next instruction, it can also be 0 which means end of packet
pub fn program() -> Program<PROGRAM_SPACE> {
    // every instruction also sets d+ and d- on the side
    let side_set = SideSet::new(SIDE_SET_OPTIONAL, SIDE_SET_BITS, SIDE_SET_TO_PINDIRS);
    let mut assembler = Assembler::<PROGRAM_SPACE>::new_with_side_set(side_set);

    let mut wrap_target = assembler.label();
    let mut wrap_source = assembler.label();

    // symbol 0, end of packet: raise flag 0 and drive se0 for two bit times,
    // then fall into the j below
    assembler.irq_with_delay_and_side_set(
        false,
        false,
        IRQ_END_OF_PACKET,
        IrqIndexMode::DIRECT,
        TWO_BIT_TIMES,
        SIDE_SINGLE_ENDED_ZERO,
    );

    // wrap target: after the drivers are switched on we continue here
    assembler.bind(&mut wrap_target);

    // symbol 1: drive j for one bit, then take the next 2 bits from the osr and jump there
    assembler.out_with_delay_and_side_set(
        OutDestination::PC,
        SYMBOL_BITS,
        REST_OF_BIT_TIME,
        SIDE_DRIVE_J,
    );

    // symbol 2: let go of the bus, the pins stay inputs until the cpu turns them back on
    assembler.set_with_delay_and_side_set(
        SetDestination::PINDIRS,
        PINDIRS_RELEASED,
        REST_OF_BIT_TIME,
        SIDE_DRIVE_J,
    );

    // symbol 3: drive k for one bit, then take the next 2 bits from the osr and jump there
    assembler.out_with_delay_and_side_set(
        OutDestination::PC,
        SYMBOL_BITS,
        REST_OF_BIT_TIME,
        SIDE_DRIVE_K,
    );

    // switch both drivers on at j, only the cpu can jump here
    assembler.set_with_side_set(SetDestination::PINDIRS, PINDIRS_DRIVING, SIDE_DRIVE_J);

    assembler.bind(&mut wrap_source);

    assembler
        .assemble_with_wrap(wrap_source, wrap_target)
        .set_origin(Some(ORIGIN))
}

#[cfg(test)]
mod tests {
    use super::ADDRESS_DRIVE_J;
    use super::ADDRESS_ENABLE_DRIVERS;
    use super::DRIVE_J;
    use super::DRIVE_K;
    use super::DRIVE_SINGLE_ENDED_ZERO;
    use super::ORIGIN;
    use super::RELEASE;
    use super::START;
    use super::program;

    const REFERENCE_WORDS: [u16; 5] = [0xC700, 0x6BA2, 0xEB80, 0x73A2, 0xE883];

    const REFERENCE_START_WORD: u16 = 0x0804;

    #[test]
    fn the_assembled_words_match_the_reference_implementation() {
        assert_eq!(program().code.as_slice(), REFERENCE_WORDS);
    }

    #[test]
    fn the_program_must_load_at_offset_zero() {
        assert_eq!(program().origin, Some(ORIGIN));
    }

    #[test]
    fn the_loop_wraps_back_to_the_first_symbol_address() {
        let program = program();
        assert_eq!(program.wrap.target, ADDRESS_DRIVE_J);
        assert_eq!(program.wrap.source, ADDRESS_ENABLE_DRIVERS);
    }

    #[test]
    fn the_start_instruction_matches_the_reference_implementation() {
        assert_eq!(START.encode(program().side_set), REFERENCE_START_WORD);
    }

    #[test]
    fn driving_j_is_the_same_word_the_program_uses_to_enable_its_drivers() {
        let enable_drivers = program().code[usize::from(ADDRESS_ENABLE_DRIVERS)];
        assert_eq!(DRIVE_J.encode(program().side_set), enable_drivers);
    }

    #[test]
    fn each_static_drive_differs_only_in_its_side_set() {
        let side_set = program().side_set;
        assert_eq!(DRIVE_SINGLE_ENDED_ZERO.encode(side_set), 0xE083);
        assert_eq!(DRIVE_J.encode(side_set), 0xE883);
        assert_eq!(DRIVE_K.encode(side_set), 0xF083);
    }

    #[test]
    fn releasing_leaves_the_output_latch_at_idle() {
        assert_eq!(RELEASE.encode(program().side_set), 0xE880);
    }
}
