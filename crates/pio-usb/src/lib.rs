#![cfg_attr(not(test), no_std)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        dead_code,
        unused_must_use
    )
)]

mod crc;
mod data;
mod decoder;
mod detector;
mod encode;
mod error;
mod pid;
mod result;
mod setup;
mod token;
mod transmit;

pub use pio::Instruction;

pub use crc::CRC16_FINAL_XOR;
pub use crc::CRC16_INITIAL;
pub use crc::CRC16_NIBBLE_TABLE;
pub use data::data_packet;
pub use data::data_payload;
pub use decoder::CLEAR_INPUT_SHIFT as DECODER_CLEAR_INPUT_SHIFT;
pub use decoder::PRELOAD_ONES as DECODER_PRELOAD_ONES;
pub use decoder::RESET_LINE_STATE as DECODER_RESET_LINE_STATE;
pub use decoder::program as decoder_program;
pub use decoder::restart_instruction as decoder_restart_instruction;
pub use detector::CYCLES_PER_BIT as DETECTOR_CYCLES_PER_BIT;
pub use detector::IRQ_DECODER_TRIGGER as DETECTOR_IRQ_DECODER_TRIGGER;
pub use detector::IRQ_END_OF_PACKET as DETECTOR_IRQ_END_OF_PACKET;
pub use detector::IRQ_START_OF_PACKET as DETECTOR_IRQ_START_OF_PACKET;
pub use detector::program as detector_program;
pub use encode::ACKNOWLEDGE_ENCODED;
pub use encode::encode;
pub use encode::encode_sequence;
pub use encode::encoded_len;
pub use encode::encoded_sequence_len;
pub use error::Error;
pub use pid::ACK as PID_ACK;
pub use pid::DATA0 as PID_DATA0;
pub use pid::DATA1 as PID_DATA1;
pub use pid::IN as PID_IN;
pub use pid::NAK as PID_NAK;
pub use pid::OUT as PID_OUT;
pub use pid::SETUP as PID_SETUP;
pub use pid::STALL as PID_STALL;
pub use pid::is_well_formed as pid_is_well_formed;
pub use result::Result;
pub use setup::DESCRIPTOR_CONFIGURATION;
pub use setup::DESCRIPTOR_DEVICE;
pub use setup::Request;
pub use setup::SETUP_LEN;
pub use token::SYNC;
pub use token::start_of_frame;
pub use token::token;
pub use transmit::CYCLES_PER_BIT as TRANSMIT_CYCLES_PER_BIT;
pub use transmit::DRIVE_J as TRANSMIT_DRIVE_J;
pub use transmit::DRIVE_K as TRANSMIT_DRIVE_K;
pub use transmit::DRIVE_SINGLE_ENDED_ZERO as TRANSMIT_DRIVE_SINGLE_ENDED_ZERO;
pub use transmit::IRQ_END_OF_PACKET as TRANSMIT_IRQ_END_OF_PACKET;
pub use transmit::RELEASE as TRANSMIT_RELEASE;
pub use transmit::START as TRANSMIT_START;
pub use transmit::program as transmit_program;

#[cfg(test)]
mod tests {
    use super::decoder_program;
    use super::detector_program;
    use super::transmit_program;

    const INSTRUCTIONS_PER_BLOCK: usize = 32;

    #[test]
    fn all_three_programs_fill_exactly_one_block() {
        let total = transmit_program().code.len()
            + detector_program().code.len()
            + decoder_program().code.len();
        assert_eq!(total, INSTRUCTIONS_PER_BLOCK);
    }
}
