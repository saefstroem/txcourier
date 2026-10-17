use pio::{
    Assembler, InSource, IrqIndexMode, JmpCondition, MovDestination, MovOperation, MovSource,
    Program,
};

// Pico-PIO-USB's receive flag numbering. A blocking wait on it parks the detector after every
// EOP, including the host's own.
pub const IRQ_END_OF_PACKET: u8 = 2;
// Pico-PIO-USB's receive flag numbering. Raised once, when the bus first leaves idle J for
// the SYNC's K.
pub const IRQ_START_OF_PACKET: u8 = 3;
// Pico-PIO-USB's receive flag numbering: the recovered bit clock handed to the decoder. The
// decoder waits on it by absolute index, so both must share one PIO block.
pub const IRQ_DECODER_TRIGGER: u8 = 4;

// Pico-PIO-USB's oversampling: a two-cycle tick and six one-cycle polls per bit, so a rising
// edge gets six chances per bit to re-phase the clock.
pub const CYCLES_PER_BIT: u32 = 8;

// RP2350 datasheet: a PIO block's instruction memory holds 32 words.
const PROGRAM_SPACE: usize = 32;

// Only D+ is sampled, into the ISR, as a one-bit scratch for the SE0 test.
const ONE_BIT: u8 = 1;
// Makes the instruction two cycles, as the eight-cycle bit accounting requires.
const ONE_EXTRA_CYCLE: u8 = 1;

pub fn program() -> Program<PROGRAM_SPACE> {
    let mut assembler = Assembler::<PROGRAM_SPACE>::new();

    // labels labels
    let mut end_of_packet = assembler.label();
    let mut idle = assembler.label();
    let mut wrap_target = assembler.label();
    let mut wrap_source = assembler.label();
    let mut fell = assembler.label();
    let mut stayed_high = assembler.label();
    let mut rose = assembler.label();


    assembler.bind(&mut end_of_packet);
    // wait for signal from cpu 
    assembler.irq(false, true, IRQ_END_OF_PACKET, IrqIndexMode::DIRECT);
    // bind to label
    assembler.bind(&mut idle);
    // if wire is J jump to idle
    assembler.jmp(JmpCondition::PinHigh, &mut idle);

    // a k arrived, it is start of packet
    assembler.irq_with_delay(
        false,
        false,
        IRQ_START_OF_PACKET,
        IrqIndexMode::DIRECT,
        ONE_EXTRA_CYCLE,
    );

    // bind to wrap target
    assembler.bind(&mut wrap_target);

    // delay for 2 cycles
    assembler.irq_with_delay(
        false,
        false,
        IRQ_DECODER_TRIGGER,
        IrqIndexMode::DIRECT,
        ONE_EXTRA_CYCLE,
    );

    // poll two cycles and check if it rose
    assembler.jmp(JmpCondition::PinHigh, &mut rose);
    assembler.jmp(JmpCondition::PinHigh, &mut rose);

    // label binding
    assembler.bind(&mut fell);
    // poll 4 cycles and check if it rose
    assembler.jmp(JmpCondition::PinHigh, &mut rose);
    assembler.jmp(JmpCondition::PinHigh, &mut rose);
    assembler.jmp(JmpCondition::PinHigh, &mut rose);
    assembler.jmp(JmpCondition::PinHigh, &mut rose);

    //label bindings after this go back to wrap target
    assembler.bind(&mut wrap_source);

    // if it stayed high
    assembler.bind(&mut stayed_high);

    // set X to be D+ sample taken later
    assembler.mov_with_delay(
        MovDestination::X,
        MovOperation::None,
        MovSource::ISR,
        ONE_EXTRA_CYCLE,
    );

    // if d+ was low too it means SE0 so jump to end of packet
    assembler.jmp(JmpCondition::XDecNonZero, &mut end_of_packet);

    // high loop
    assembler.bind(&mut rose);

    // clear bit at ISR
    assembler.mov_with_delay(
        MovDestination::ISR,
        MovOperation::None,
        MovSource::NULL,
        ONE_EXTRA_CYCLE,
    );

    // raise flag
    assembler.irq(false, false, IRQ_DECODER_TRIGGER, IrqIndexMode::DIRECT);
    
    // Sample d+ 1 if its low
    assembler.r#in(InSource::PINS, ONE_BIT);
    // if D- still low then its either j
    assembler.jmp(JmpCondition::PinHigh, &mut stayed_high);
    // No it fell to K, jump to fell
    assembler.jmp(JmpCondition::Always, &mut fell);

    // add the wrap targets
    assembler.assemble_with_wrap(wrap_source, wrap_target)
}

#[cfg(test)]
mod tests {
    use pio::InSource;
    use pio::Instruction;
    use pio::InstructionOperands;
    use pio::JmpCondition;
    use pio::MovDestination;
    use pio::MovSource;
    use pio::SideSet;

    use super::CYCLES_PER_BIT;
    use super::IRQ_END_OF_PACKET;
    use super::program;

    const ADDRESS_END_OF_PACKET: u8 = 0;
    const ADDRESS_TICK: u8 = 3;
    const ADDRESS_FIRST_POLL: u8 = 4;
    const ADDRESS_FELL: u8 = 6;
    const ADDRESS_LAST_POLL: u8 = 9;
    const ADDRESS_STAYED_HIGH: u8 = 10;
    const ADDRESS_ROSE: u8 = 12;

    const REFERENCE_WORDS: [u16; 17] = [
        0xC022, 0x00C1, 0xC103, 0xC104, 0x00CC, 0x00CC, 0x00CC, 0x00CC, 0x00CC, 0x00CC, 0xA126,
        0x0040, 0xA1C3, 0xC004, 0x4001, 0x00CA, 0x0006,
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
    fn the_program_boots_parked_on_the_end_of_packet_flag() {
        assert!(matches!(
            operands(ADDRESS_END_OF_PACKET),
            InstructionOperands::IRQ {
                clear: false,
                wait: true,
                index: IRQ_END_OF_PACKET,
                ..
            }
        ));
    }

    #[test]
    fn the_bit_clock_loop_wraps_from_the_last_poll_back_to_the_tick() {
        let program = program();
        assert_eq!(program.wrap.source, ADDRESS_LAST_POLL);
        assert_eq!(program.wrap.target, ADDRESS_TICK);
    }

    #[test]
    fn a_line_held_at_k_ticks_once_per_bit() {
        assert_eq!(cycles_along(&[3, 4, 5, 6, 7, 8, 9]), CYCLES_PER_BIT);
    }

    #[test]
    fn a_line_held_at_j_ticks_once_per_bit() {
        assert_eq!(cycles_along(&[12, 13, 14, 15, 10, 11]), CYCLES_PER_BIT);
    }

    #[test]
    fn a_falling_edge_preserves_the_bit_clock_cadence() {
        assert_eq!(cycles_along(&[13, 14, 15, 16, 6, 7, 8, 9]), CYCLES_PER_BIT);
    }

    #[test]
    fn a_falling_edge_rejoins_the_low_loop_past_its_first_two_polls() {
        assert!(matches!(
            operands(16),
            InstructionOperands::JMP {
                condition: JmpCondition::Always,
                address: ADDRESS_FELL,
            }
        ));
        assert_ne!(ADDRESS_FELL, ADDRESS_FIRST_POLL);
    }

    #[test]
    fn a_rising_edge_re_phases_the_next_tick_to_the_edge() {
        let seen_at_first_poll = cycles_along(&[3, 4, 12]);
        let seen_at_last_poll = cycles_along(&[3, 4, 5, 6, 7, 8, 9, 12]);
        assert!(seen_at_first_poll < CYCLES_PER_BIT);
        assert!(seen_at_last_poll > CYCLES_PER_BIT);
    }

    #[test]
    fn every_poll_in_the_low_loop_watches_for_the_rising_edge() {
        for address in ADDRESS_FIRST_POLL..=ADDRESS_LAST_POLL {
            assert!(matches!(
                operands(address),
                InstructionOperands::JMP {
                    condition: JmpCondition::PinHigh,
                    address: ADDRESS_ROSE,
                }
            ));
        }
    }

    #[test]
    fn end_of_packet_needs_both_lines_low_at_once() {
        assert!(matches!(
            operands(12),
            InstructionOperands::MOV {
                destination: MovDestination::ISR,
                source: MovSource::NULL,
                ..
            }
        ));
        assert!(matches!(
            operands(14),
            InstructionOperands::IN {
                source: InSource::PINS,
                bit_count: 1,
            }
        ));
        assert!(matches!(
            operands(15),
            InstructionOperands::JMP {
                condition: JmpCondition::PinHigh,
                address: ADDRESS_STAYED_HIGH,
            }
        ));
        assert!(matches!(
            operands(ADDRESS_STAYED_HIGH),
            InstructionOperands::MOV {
                destination: MovDestination::X,
                source: MovSource::ISR,
                ..
            }
        ));
        assert!(matches!(
            operands(11),
            InstructionOperands::JMP {
                condition: JmpCondition::XDecNonZero,
                address: ADDRESS_END_OF_PACKET,
            }
        ));
    }

    fn operands(address: u8) -> InstructionOperands {
        let word = program().code[usize::from(address)];
        Instruction::decode(word, SideSet::new(false, 0, false))
            .expect("a valid instruction")
            .operands
    }

    fn cycles_along(path: &[usize]) -> u32 {
        let code = program().code;
        path.iter()
            .map(|address| 1 + u32::from(code[*address] >> 8 & 0x1F))
            .sum()
    }
}
