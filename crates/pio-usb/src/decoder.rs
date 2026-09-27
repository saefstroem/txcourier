use crate::detector::IRQ_DECODER_TRIGGER;
use crate::encode::ONES_BEFORE_STUFFING;
use pio::{
    Assembler, InSource, Instruction, InstructionOperands, JmpCondition, MovDestination,
    MovOperation, MovSource, Program, SetDestination, WaitSource,
};

// RP2350 datasheet: a PIO block's instruction memory holds 32 words.
const PROGRAM_SPACE: usize = 32;

// One bit-clock tick yields one decoded data bit.
const ONE_BIT: u8 = 1;
// RP2350 datasheet, WAIT: waiting for a 1 on an IRQ flag also clears it, which consumes the
// tick.
const WAIT_FOR_SET: u8 = 1;
// The flag is indexed directly within this block. Relative mode would add the state machine
// id.
const ABSOLUTE_FLAG: bool = false;
// The decoder is event-driven, and its longest path already nearly fills an 83 ns bit.
const NO_DELAY: u8 = 0;

// X holds the previous line state. Arming it at J, the idle state, makes the KJKJKJKK sync
// field decode to 0x80. Which of 0 and ~0 stands for J is defined by this file.
const PREVIOUS_LINE_WAS_J: u8 = 0;

// mov to ISR also zeroes the input shift counter, so a packet starts on a byte boundary.
pub const CLEAR_INPUT_SHIFT: Instruction =
    mov(MovDestination::ISR, MovOperation::None, MovSource::NULL);

// X survives the rest of the arming sequence, so the remembered line state is reset
// explicitly.
pub const RESET_LINE_STATE: Instruction = Instruction {
    operands: InstructionOperands::SET {
        destination: SetDestination::X,
        data: PREVIOUS_LINE_WAS_J,
    },
    delay: NO_DELAY,
    side_set: None,
};

// PIO can shift in a zero from NULL but has no source of ones, so the OSR is filled with
// them.
pub const PRELOAD_ONES: Instruction =
    mov(MovDestination::OSR, MovOperation::Invert, MovSource::NULL);

pub const fn restart_instruction(offset: u8) -> Instruction {
    Instruction {
        operands: InstructionOperands::JMP {
            condition: JmpCondition::Always,
            address: offset,
        },
        delay: NO_DELAY,
        side_set: None,
    }
}

const fn mov(destination: MovDestination, op: MovOperation, source: MovSource) -> Instruction {
    Instruction {
        operands: InstructionOperands::MOV {
            destination,
            op,
            source,
        },
        delay: NO_DELAY,
        side_set: None,
    }
}

pub fn program() -> Program<PROGRAM_SPACE> {
    let mut assembler = Assembler::<PROGRAM_SPACE>::new();

    let mut reset_run = assembler.label();
    let mut await_tick = assembler.label();
    let mut shift_zero = assembler.label();
    let mut flip = assembler.label();
    let mut wrap_source = assembler.label();
    let mut line_is_k = assembler.label();
    let mut shift_one = assembler.label();

    // bind reset run to this position
    assembler.bind(&mut reset_run);
    // store 6 in the Y register, this is how many 1's before fake flip
    assembler.set(SetDestination::Y, ONES_BEFORE_STUFFING);
    // bind await tick here
    assembler.bind(&mut await_tick);
    // Wait for the trigger
    assembler.wait(
        WAIT_FOR_SET,
        WaitSource::IRQ,
        IRQ_DECODER_TRIGGER,
        ABSOLUTE_FLAG,
    );

    // If there are 0 1s left we should jump to flip
    assembler.jmp(JmpCondition::YIsZero, &mut flip);
    // if the wire is K go to K
    assembler.jmp(JmpCondition::PinHigh, &mut line_is_k);
    // Wire is not K, J, and if it was J before
    assembler.jmp(JmpCondition::XIsZero, &mut shift_one);
    // bind to shift zero
    assembler.bind(&mut shift_zero);
    // Output 0
    assembler.r#in(InSource::NULL, ONE_BIT);
    // bind to flip label
    assembler.bind(&mut flip);
    // Invert X and store it there
    assembler.mov(MovDestination::X, MovOperation::Invert, MovSource::X);
    // bind to wrap label and go back to 0
    assembler.bind(&mut wrap_source);
    // bind to label 
    assembler.bind(&mut line_is_k);
    // if wire is K 
    assembler.jmp(JmpCondition::XIsZero, &mut shift_zero);
    // bind label
    assembler.bind(&mut shift_one);
    // output one
    assembler.r#in(InSource::OSR, ONE_BIT);
    // decrement if y is not zero then jump to await tick
    assembler.jmp(JmpCondition::YDecNonZero, &mut await_tick);

    // assemble the program with wrap jump condition
    assembler.assemble_with_wrap(wrap_source, reset_run)
}

#[cfg(test)]
mod tests {
    use pio::InSource;
    use pio::Instruction;
    use pio::InstructionOperands;
    use pio::JmpCondition;
    use pio::MovDestination;
    use pio::MovOperation;
    use pio::MovSource;
    use pio::SetDestination;
    use pio::SideSet;
    use pio::WaitSource;

    use super::CLEAR_INPUT_SHIFT;
    use super::IRQ_DECODER_TRIGGER;
    use super::ONES_BEFORE_STUFFING;
    use super::PRELOAD_ONES;
    use super::RESET_LINE_STATE;
    use super::program;
    use super::restart_instruction;

    const ADDRESS_RESET_RUN: u8 = 0;
    const ADDRESS_AWAIT_TICK: u8 = 1;
    const ADDRESS_SHIFT_ZERO: u8 = 5;
    const ADDRESS_FLIP: u8 = 6;
    const ADDRESS_LINE_IS_K: u8 = 7;
    const ADDRESS_SHIFT_ONE: u8 = 8;

    const REFERENCE_WORDS: [u16; 10] = [
        0xE046, 0x20C4, 0x0066, 0x00C7, 0x0028, 0x4061, 0xA029, 0x0025, 0x40E1, 0x0081,
    ];

    #[test]
    fn the_assembled_words_match_the_reference_implementation() {
        assert_eq!(program().code.as_slice(), REFERENCE_WORDS);
    }

    #[test]
    fn the_program_is_relocatable() {
        assert_eq!(program().origin, None);
    }

    #[test]
    fn the_program_blocks_on_the_detector_trigger() {
        assert!(matches!(
            operands(ADDRESS_AWAIT_TICK),
            InstructionOperands::WAIT {
                polarity: 1,
                source: WaitSource::IRQ,
                index: IRQ_DECODER_TRIGGER,
                relative: false,
            }
        ));
    }

    #[test]
    fn the_allowed_run_of_ones_is_the_one_the_encoder_stuffs_after() {
        assert!(matches!(
            operands(ADDRESS_RESET_RUN),
            InstructionOperands::SET {
                destination: SetDestination::Y,
                data: ONES_BEFORE_STUFFING,
            }
        ));
    }

    #[test]
    fn a_stuff_bit_flips_the_line_state_without_entering_the_data_stream() {
        assert!(matches!(
            operands(2),
            InstructionOperands::JMP {
                condition: JmpCondition::YIsZero,
                address: ADDRESS_FLIP,
            }
        ));
        assert!(matches!(
            operands(ADDRESS_FLIP),
            InstructionOperands::MOV {
                destination: MovDestination::X,
                op: MovOperation::Invert,
                source: MovSource::X,
            }
        ));
        assert_eq!(program().wrap.source, ADDRESS_FLIP);
    }

    #[test]
    fn a_data_zero_resets_the_run_counter() {
        assert!(matches!(
            operands(ADDRESS_SHIFT_ZERO),
            InstructionOperands::IN {
                source: InSource::NULL,
                bit_count: 1,
            }
        ));
        assert_eq!(program().wrap.target, ADDRESS_RESET_RUN);
    }

    #[test]
    fn a_data_one_counts_down_without_resetting_the_run_counter() {
        assert!(matches!(
            operands(ADDRESS_SHIFT_ONE),
            InstructionOperands::IN {
                source: InSource::OSR,
                bit_count: 1,
            }
        ));
        assert!(matches!(
            operands(9),
            InstructionOperands::JMP {
                condition: JmpCondition::YDecNonZero,
                address: ADDRESS_AWAIT_TICK,
            }
        ));
    }

    #[test]
    fn an_unchanged_line_is_a_one_and_a_changed_line_is_a_zero() {
        assert!(matches!(
            operands(3),
            InstructionOperands::JMP {
                condition: JmpCondition::PinHigh,
                address: ADDRESS_LINE_IS_K,
            }
        ));
        assert!(matches!(
            operands(4),
            InstructionOperands::JMP {
                condition: JmpCondition::XIsZero,
                address: ADDRESS_SHIFT_ONE,
            }
        ));
        assert!(matches!(
            operands(ADDRESS_LINE_IS_K),
            InstructionOperands::JMP {
                condition: JmpCondition::XIsZero,
                address: ADDRESS_SHIFT_ZERO,
            }
        ));
    }

    #[test]
    fn arming_starts_from_a_clean_shift_register_and_an_idle_line() {
        let side_set = program().side_set;
        assert_eq!(CLEAR_INPUT_SHIFT.encode(side_set), 0xA0C3);
        assert_eq!(RESET_LINE_STATE.encode(side_set), 0xE020);
        assert_eq!(PRELOAD_ONES.encode(side_set), 0xA0EB);
    }

    #[test]
    fn the_restart_jumps_to_wherever_the_program_was_installed() {
        for offset in [0u8, 5, 22] {
            assert_eq!(
                restart_instruction(offset).encode(program().side_set),
                u16::from(offset)
            );
        }
    }

    #[test]
    fn no_instruction_carries_a_delay() {
        for word in program().code {
            assert_eq!(word >> 8 & 0x1F, 0, "{word:#06x} has a delay field");
        }
    }

    fn operands(address: u8) -> InstructionOperands {
        let word = program().code[usize::from(address)];
        Instruction::decode(word, SideSet::new(false, 0, false))
            .expect("a valid instruction")
            .operands
    }
}
