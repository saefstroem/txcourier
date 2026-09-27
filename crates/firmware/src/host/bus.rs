use core::mem::replace;
use core::ptr::{read_volatile, write_volatile};

use rp235x_hal as hal;

use hal::Timer;
use hal::gpio::bank0::{Gpio12, Gpio13};
use hal::gpio::{
    DefaultTypeState, FunctionPio0, InputOverride, OutputDriveStrength, OutputSlewRate, Pin, PinId,
    PullType,
};
use hal::pac::{PIO0, PIO1, RESETS, SIO, TIMER0};
use hal::pio::{
    Buffers, PIO, PIOBuilder, PIOExt, PinDir, Running, SM0, ShiftDirection, StateMachine, Stopped,
    Tx,
};
use hal::timer::CopyableTimer0;
use rp2350wallet_pio_usb::{
    ACKNOWLEDGE_ENCODED, CRC16_FINAL_XOR, CRC16_INITIAL, CRC16_NIBBLE_TABLE,
    DECODER_CLEAR_INPUT_SHIFT, DECODER_PRELOAD_ONES, DECODER_RESET_LINE_STATE,
    DETECTOR_CYCLES_PER_BIT, DETECTOR_IRQ_DECODER_TRIGGER, DETECTOR_IRQ_END_OF_PACKET,
    DETECTOR_IRQ_START_OF_PACKET, Instruction, PID_DATA0, PID_DATA1, SYNC, TRANSMIT_CYCLES_PER_BIT,
    TRANSMIT_DRIVE_SINGLE_ENDED_ZERO, TRANSMIT_IRQ_END_OF_PACKET, TRANSMIT_RELEASE, TRANSMIT_START,
    decoder_program, decoder_restart_instruction, detector_program, encode_sequence,
    encoded_sequence_len, start_of_frame, transmit_program,
};
use rp2350wallet_protocol::BUS_SAMPLES;
use rp2350wallet_protocol::wire::Packet;

use crate::clocks::{FULL_SPEED_BIT_HZ, SYS_HZ};
use crate::host::error::Error;
use crate::host::result::Result;

// Sources cited below:
//   USB 2.0 Specification  https://www.usb.org/document-library/usb-20-specification
//   RP2350 Datasheet       https://rptl.io/rp2350-datasheet
//   Waveshare schematic    https://files.waveshare.com/wiki/RP2350-USB-A/RP2350-USB-A.pdf
//   Pico-PIO-USB (MIT)     https://github.com/sekigon-gonnoc/Pico-PIO-USB

// Waveshare schematic: USB-A D+ is GP12. The RP2350's own USB controller is wired to the
// Type-C connector, so this line is reachable only as a bank 0 GPIO.
const DATA_PLUS_PIN: u8 = 12;
// Waveshare schematic: USB-A D- is GP13. The PIO side-set, set and out pin groups all start
// at D+ and span two pins, so D- must be the next GPIO.
const DATA_MINUS_PIN: u8 = 13;
const _: () = assert!(DATA_MINUS_PIN == DATA_PLUS_PIN + 1);
// D+ and D-: the width of every two-pin PIO group, and of PINDIRS_DRIVING.
const DATA_LINE_COUNT: u8 = 2;

// RP2350 datasheet, SIO GPIO_IN: bit n is GPIO n.
const DATA_PLUS_MASK: u32 = 1 << DATA_PLUS_PIN;
const DATA_MINUS_MASK: u32 = 1 << DATA_MINUS_PIN;

// Pico-PIO-USB pio_usb_bus_get_line_state: D+ is bit 0 and D- is bit 1.
const DATA_PLUS_CODE: u8 = 0b01;
const DATA_MINUS_CODE: u8 = 0b10;

// RP2350 datasheet, SMx_CLKDIV: the fractional divider is an 8-bit field, in steps of 1/256.
const DIVISOR_FRACTION_STEPS: u64 = 256;

// SYS_HZ / (FULL_SPEED_BIT_HZ * cycles per bit): 2.5 for the transmitter, 1.25 for the
// detector.
const TRANSMIT_DIVISOR: (u16, u8) = divisor(TRANSMIT_CYCLES_PER_BIT);
const DETECTOR_DIVISOR: (u16, u8) = divisor(DETECTOR_CYCLES_PER_BIT);
// Undivided. At 120 MHz one 12 Mbit/s bit is ten system cycles, the rate the decoder program
// is written for. This is what fixes SYS_HZ.
const DECODER_DIVISOR: (u16, u8) = (1, 0);

const _: () = assert!(divides_exactly(TRANSMIT_CYCLES_PER_BIT));
const _: () = assert!(divides_exactly(DETECTOR_CYCLES_PER_BIT));

// RP2350 datasheet, SMx_SHIFTCTRL: a PULL_THRESH of 0 selects 32 bits, one full OSR per pull.
// pack() fills four-byte words to match.
const SYMBOL_WORD_BITS: u8 = 32;
const BYTE_BITS: u8 = 8;
const BYTES_PER_WORD: usize = 4;
// Four copies of symbol 1, which drives J. Padding a word with them holds the bus idle.
const IDLE_SYMBOLS: u8 = 0x55;

// USB 2.0 sections 8.2 and 8.4.1: a token is SYNC, then PID, ADDR, ENDP and CRC5 in three
// bytes. token.rs builds all four.
const TOKEN_LEN: usize = 4;
// A transaction sends at most a token followed by one data packet.
const MAX_PACKETS: usize = 2;
const LONGEST_PACKET: usize = Packet::CAPACITY + TOKEN_LEN;
const ENCODED_CAPACITY: usize = encoded_sequence_len(LONGEST_PACKET, MAX_PACKETS);
const WORD_CAPACITY: usize = ENCODED_CAPACITY.div_ceil(BYTES_PER_WORD);

// State machine indices. Each must match the typed handle taken from split() and named by
// ParkedTransmitter and ParkedDecoder.
const TRANSMIT_MACHINE: usize = 0;
const DECODER_MACHINE: usize = 0;
// RP2350 datasheet, PIO FSTAT: RXEMPTY is bits 11:8, one bit per state machine.
const RECEIVE_EMPTY_SHIFT: u32 = 8;
const DECODER_EMPTY_MASK: u32 = 1 << (RECEIVE_EMPTY_SHIFT + DECODER_MACHINE as u32);

// PIO IRQ flag bits, at the indices the three programs in crates/pio-usb declare.
const TRANSMIT_END_OF_PACKET_FLAG: u8 = 1 << TRANSMIT_IRQ_END_OF_PACKET;
const RECEIVE_START_FLAG: u8 = 1 << DETECTOR_IRQ_START_OF_PACKET;
const RECEIVE_END_OF_PACKET_FLAG: u8 = 1 << DETECTOR_IRQ_END_OF_PACKET;
const DECODER_TRIGGER_FLAG: u8 = 1 << DETECTOR_IRQ_DECODER_TRIGGER;
// 0x1C. Pico-PIO-USB's IRQ_RX_ALL_MASK is 0x1E; its extra flag 1 reports a bit-stuff error
// that no program in this crate raises.
const RECEIVE_FLAGS: u8 = RECEIVE_END_OF_PACKET_FLAG | RECEIVE_START_FLAG | DECODER_TRIGGER_FLAG;

// USB 2.0 section 7.1.7.5, Table 7-13: TDRST, reset driven by a hub port, is 10 ms to 20 ms.
// TDRSTR, reset driven by a root port, is 50 ms minimum. This host is a root port.
const RESET_HOLD_US: u64 = 15_000;
// Pico-PIO-USB pauses 100 us after releasing reset. This is not the recovery time of USB 2.0
// Table 7-14, which SETTLE_FRAMES provides.
const RESET_RECOVERY_US: u64 = 100;
// USB 2.0 sections 8.4.3 and 11.2.1: a frame is 1 ms, and a full-speed host sends one SOF
// per frame.
const FRAME_PERIOD_US: u64 = 1_000;
// USB 2.0 section 8.4.3: the SOF frame number is an 11-bit field.
const FRAME_NUMBER_MASK: u16 = 0x7FF;
// Not fixed by any specification. USB 2.0 section 7.1.18.1 bounds a device's turnaround at
// 7.5 bit times, 625 ns; Pico-PIO-USB waits 3 us.
const REPLY_START_TIMEOUT_US: u32 = 10;
// Not fixed by any specification: the silence after which a reply is treated as ended.
const REPLY_GAP_TIMEOUT_US: u32 = 7;
// Not fixed by any specification; Pico-PIO-USB waits without a bound. One frame, so a stuck
// transmit costs at most one SOF.
const TRANSMIT_TIMEOUT_US: u32 = 1_000;
// The tail still on the wire when await_ends returns: SE0, J, release and up to fifteen
// idle-J symbols, about 1.6 us. Pico-PIO-USB polls the program counter instead.
const PARK_SETTLE_US: u32 = 3;

// USB 2.0 section 8.3.5.2: a data packet ends with a 16-bit CRC, after SYNC and PID.
const CHECKSUM_LEN: usize = 2;
const CHECKSUM_STARTS_AT: usize = 2;
const DATA_PACKET_MIN_LEN: usize = CHECKSUM_STARTS_AT + CHECKSUM_LEN;
const ACKNOWLEDGE_WORDS: usize = ACKNOWLEDGE_ENCODED.len().div_ceil(BYTES_PER_WORD);

// The CRC16 is folded four bits at a time through a 16-entry table.
const NIBBLE_BITS: u32 = 4;
const NIBBLE_MASK: u16 = 0x0F;

#[unsafe(link_section = ".data.host")]
static NIBBLE_TABLE: [u16; 16] = CRC16_NIBBLE_TABLE;

pub type DataPlusPin =
    Pin<Gpio12, <Gpio12 as DefaultTypeState>::Function, <Gpio12 as DefaultTypeState>::PullType>;

pub type DataMinusPin =
    Pin<Gpio13, <Gpio13 as DefaultTypeState>::Function, <Gpio13 as DefaultTypeState>::PullType>;

type ParkedTransmitter = StateMachine<(PIO0, SM0), Stopped>;
type LiveTransmitter = StateMachine<(PIO0, SM0), Running>;
type ParkedDecoder = StateMachine<(PIO1, SM0), Stopped>;
type ListeningDecoder = StateMachine<(PIO1, SM0), Running>;

enum Transmitter {
    Parked(ParkedTransmitter),
    Live(LiveTransmitter),
    Missing,
}

enum Decoder {
    Parked(ParkedDecoder),
    Listening(ListeningDecoder),
    Missing,
}

pub struct Reception {
    pub len: usize,
    pub start_seen: bool,
    pub end_seen: bool,
    pub checksum_ok: bool,
    pub acknowledged: bool,
}

struct Registers {
    transmit_flags: *const u32,
    transmit_flags_clear: *mut u32,
    transmit_instruction: *mut u32,
    transmit_fifo: *mut u32,
    receive_flags: *const u32,
    receive_flags_clear: *mut u32,
    receive_status: *const u32,
    receive_fifo: *const u32,
    microseconds: *const u32,
}

impl Registers {
    /// Captures the pointers to registers so that we can access them easily
    fn capture() -> Self {
        let transmit = unsafe { &*PIO0::ptr() };
        let receive = unsafe { &*PIO1::ptr() };
        let timer = unsafe { &*TIMER0::ptr() };

        Self {
            transmit_flags: transmit.irq().as_ptr().cast_const(),
            transmit_flags_clear: transmit.irq().as_ptr(),
            transmit_instruction: transmit.sm(TRANSMIT_MACHINE).sm_instr().as_ptr(),
            transmit_fifo: transmit.txf(TRANSMIT_MACHINE).as_ptr(),
            receive_flags: receive.irq().as_ptr().cast_const(),
            receive_flags_clear: receive.irq().as_ptr(),
            receive_status: receive.fstat().as_ptr().cast_const(),
            receive_fifo: receive.rxf(DECODER_MACHINE).as_ptr().cast_const(),
            microseconds: timer.timerawl().as_ptr().cast_const(),
        }
    }
}

pub struct Bus {
    transmit_block: PIO<PIO0>,
    receive_block: PIO<PIO1>,
    transmitter: Transmitter,
    symbols: Tx<(PIO0, SM0)>,
    decoder: Decoder,
    decoder_offset: u8,
    gpio_in_ptr: *const u32,
    timer: Timer<CopyableTimer0>,
    registers: Registers,
    last_end_of_packet_at: u32,
    acknowledge: [u32; ACKNOWLEDGE_WORDS],
    start_word: u32,
    frame: u16,
    next_frame_at: u64,
    heartbeat: fn(),
}

impl Bus {
    /// This turns the PIO programs into a USB host
    pub fn new(
        transmit_block: PIO0,
        receive_block: PIO1,
        resets: &mut RESETS,
        data_plus: DataPlusPin,
        data_minus: DataMinusPin,
        timer: Timer<CopyableTimer0>,
        heartbeat: fn(),
    ) -> Result<Self> {
        // pio0 is used for transmitting
        let mut data_plus = data_plus.into_function::<FunctionPio0>();
        let mut data_minus = data_minus.into_function::<FunctionPio0>();
        
        // configure d+ and d- to be output pins
        configure_pad(&mut data_plus);
        configure_pad(&mut data_minus);

        // get the transmit and receive allocators, and the machines which are abstractions
        // over the PIO "processors". The resets is the register that controls many things on the chip
        let (mut transmit_block, transmit_machine, _, _, _) = transmit_block.split(resets);
        let (mut receive_block, decoder_machine, detector_machine, _, _) =
            receive_block.split(resets);

        // Get the transmit program which describes how to send USB data
        let transmit_program = transmit_program();

        // Compute the init instruction
        let start_word = u32::from(TRANSMIT_START.encode(transmit_program.side_set));

        // install the transmit program into the PIO
        let transmit = transmit_block.install(&transmit_program)?;
        
        // Configure the transmitter 
        let (mut transmitter, _, symbols) = PIOBuilder::from_installed_program(transmit)
            .side_set_pin_base(DATA_PLUS_PIN)
            .set_pins(DATA_PLUS_PIN, DATA_LINE_COUNT)
            .out_pins(DATA_PLUS_PIN, DATA_LINE_COUNT)
            .clock_divisor_fixed_point(TRANSMIT_DIVISOR.0, TRANSMIT_DIVISOR.1)
            .out_shift_direction(ShiftDirection::Left)
            .autopull(true)
            .pull_threshold(SYMBOL_WORD_BITS)
            .buffers(Buffers::OnlyTx)
            .build(transmit_machine);
        
        // Set the initial state to resting input state.
        transmitter.set_pindirs([
            (DATA_PLUS_PIN, PinDir::Input),
            (DATA_MINUS_PIN, PinDir::Input),
        ]);

        // Install the detector program
        let detect = receive_block.install(&detector_program())?;
        let (detector, _, _) = PIOBuilder::from_installed_program(detect)
            .in_pin_base(DATA_PLUS_PIN)
            .jmp_pin(DATA_MINUS_PIN)
            .clock_divisor_fixed_point(DETECTOR_DIVISOR.0, DETECTOR_DIVISOR.1)
            .in_shift_direction(ShiftDirection::Left)
            .autopush(false)
            .push_threshold(BYTE_BITS)
            .build(detector_machine);

        // start the detector and drop the handle
        // to prevent anyone from stopping it.
        let _ = detector.start();

        let decode = receive_block.install(&decoder_program())?;
        let decoder_offset = decode.offset();
        let (decoder, _, _) = PIOBuilder::from_installed_program(decode)
            .in_pin_base(DATA_PLUS_PIN)
            .jmp_pin(DATA_PLUS_PIN)
            .clock_divisor_fixed_point(DECODER_DIVISOR.0, DECODER_DIVISOR.1)
            .in_shift_direction(ShiftDirection::Right)
            .autopush(true)
            .push_threshold(BYTE_BITS)
            .buffers(Buffers::OnlyRx)
            .build(decoder_machine);

        // Capture the registers so that we have easy to access pointers.
        let registers = Registers::capture();
        let gpio_in_ptr = unsafe { &*SIO::ptr() }.gpio_in().as_ptr().cast_const();
        let acknowledge = packed_acknowledge();

        Ok(Self {
            transmit_block,
            receive_block,
            transmitter: Transmitter::Parked(transmitter),
            symbols,
            decoder: Decoder::Parked(decoder),
            decoder_offset,
            gpio_in_ptr,
            timer,
            registers,
            last_end_of_packet_at: 0,
            acknowledge,
            start_word,
            frame: 0,
            next_frame_at: 0,
            heartbeat,
        })
    }

    pub fn now_us(&self) -> u64 {
        self.timer.get_counter().ticks()
    }

    pub fn wait_until_us(&self, deadline: u64) {
        while self.now_us() < deadline {}
    }

    /// reads the D+ and D- lines as byte register
    fn read_dp_dm(&self) -> u8 {
        // read inverted register from the gpio in ptr
        let inverted = unsafe { read_volatile(self.gpio_in_ptr) };
        
        // convert back to readable
        let levels = !inverted;

        let mut code = 0u8;

        if levels & DATA_PLUS_MASK != 0 {
            code |= DATA_PLUS_CODE;
        }

        if levels & DATA_MINUS_MASK != 0 {
            code |= DATA_MINUS_CODE;
        }

        code
    }

    /// reads the D+ and D- lines as a sample of BUS_SAMPLES
    pub fn sample(&self) -> (u8, u8) {
        let mut data_plus_high = 0u8;
        let mut data_minus_high = 0u8;

        for _ in 0..BUS_SAMPLES {
            let code = self.read_dp_dm();

            if code & DATA_PLUS_CODE != 0 {
                data_plus_high = data_plus_high.saturating_add(1);
            }

            if code & DATA_MINUS_CODE != 0 {
                data_minus_high = data_minus_high.saturating_add(1);
            }
        }

        (data_plus_high, data_minus_high)
    }

    /// resets the bus state for clean comms
    pub fn reset(&mut self) -> Result<()> {
        (self.heartbeat)();

        // need to park the transmitter in order to executed parked instructions
        self.park_transmitter()?;

        // send reset signal
        self.exec_parked(TRANSMIT_DRIVE_SINGLE_ENDED_ZERO)?;
        
        // wait for 15ms
        self.wait_until_us(self.now_us() + RESET_HOLD_US);
        
        // release the bus
        self.exec_parked(TRANSMIT_RELEASE)?;
        self.wait_until_us(self.now_us() + RESET_RECOVERY_US);

        self.frame = 0;
        self.next_frame_at = self.now_us();

        Ok(())
    }

    /// we need to keep sending bus activity to prevent
    /// the device transceiver from going to sleep
    pub fn keep_alive(&mut self) -> Result<()> {
        (self.heartbeat)();

        // send a start of frame packet to the device
        if self.next_frame_at == 0 {
            self.next_frame_at = self.now_us();
        }

        // wait until the next frame time
        self.wait_until_us(self.next_frame_at);

        // transmit SOF
        self.transmit(&start_of_frame(self.frame))?;


        self.frame = (self.frame + 1) & FRAME_NUMBER_MASK;
        self.next_frame_at += FRAME_PERIOD_US;

        Ok(())
    }

    /// transmits a packet to the device
    pub fn transmit(&mut self, packet: &[u8]) -> Result<()> {
        self.send(&[packet], false)
    }

    /// e2e usb exchange
    pub fn transact(
        &mut self,
        packets: &[&[u8]],
        reply: &mut [u8],
        acknowledge_data: bool,
    ) -> Result<Reception> {
        // send a heartbeat
        (self.heartbeat)();

        // arm the receiver and then emit the packets
        // in the case of a reply is sent very fast
        self.arm_receiver()?;
        self.send(packets, true)?;

        // Receive the reply, and acknowledge it if requested
        let reception = self.receive(reply, acknowledge_data);

        // park decoder 
        self.park_decoder()?;
        Ok(reception)
    }

    fn send(&mut self, packets: &[&[u8]], listen_at_end_of_packet: bool) -> Result<()> {
        // if there is nothing to send we can return early
        if packets.is_empty() {
            return Ok(());
        }

        let mut words = [0u32; WORD_CAPACITY];
        // encode the packets into words, which is the format that the PIO can send
        let word_count = self.encode(packets, &mut words)?;

        // park the transmitter so that we can execute parked instructions
        self.park_transmitter()?;
        let Transmitter::Parked(mut parked) = replace(&mut self.transmitter, Transmitter::Missing)
        else {
            return Err(Error::TransmitterUnavailable);
        };

        let mut pending = words.get(..word_count).unwrap_or(&[]).iter();
        let mut next = pending.next();

        // while we have words to send, write them to the PIO
        while let Some(word) = next
            && self.symbols.write(*word)
        {
            next = pending.next();
        }

        self.symbols.clear_stalled_flag();
        self.transmit_block.clear_irq(TRANSMIT_END_OF_PACKET_FLAG);
        parked.exec_instruction(TRANSMIT_START);
        let live = parked.start();

        let started_at = self.micros();

        // keep writing packets as now the PIO is running and transmitting
        while let Some(word) = next {
            if self.symbols.write(*word) {
                next = pending.next();
            }
            if self.micros().wrapping_sub(started_at) > TRANSMIT_TIMEOUT_US {
                break;
            }
        }

        // Everything is queued but the FIFO still holds the tail, so the transmitter has not
        // reached its terminal starve yet. A stall flagged here starved mid-packet.
        let underran = self.symbols.has_stalled();

        // wait for the PIO to finish transmitting, and then park it again
        let complete = self.await_ends(packets.len(), listen_at_end_of_packet, started_at);
        self.transmitter = Transmitter::Live(live);

        if underran {
            Err(Error::TransmitUnderrun)
        } else if complete {
            Ok(())
        } else {
            Err(Error::TransmitStuck)
        }
    }

    #[unsafe(link_section = ".data.host")]
    #[inline(never)]
    fn await_ends(&mut self, mut remaining: usize, listen: bool, started_at: u32) -> bool {
        while self.micros().wrapping_sub(started_at) <= TRANSMIT_TIMEOUT_US {
            
            // wait until we have receive an end of packet flag
            if self.transmit_flags() & TRANSMIT_END_OF_PACKET_FLAG == 0 {
                continue;
            }

            // the end of packet flag is the last flag that is set
            remaining = remaining.saturating_sub(1);
            if remaining == 0 { // remaining should be 0 now.
                if listen { // clears receive flags and allows receiption
                    self.clear_receive_flags(RECEIVE_FLAGS);
                }
                // mark when we received the last packet at
                self.last_end_of_packet_at = self.micros();
                return true;
            }

            // clear the end of packet flag set by the pio
            self.clear_transmit_flags(TRANSMIT_END_OF_PACKET_FLAG);
        }

        // update the last end of packet at to now
        self.last_end_of_packet_at = self.micros();
        false
    }

    #[unsafe(link_section = ".data.host")]
    #[inline(never)]
    fn receive(&mut self, received: &mut [u8], armed: bool) -> Reception {
        let mut reception = Reception {
            len: 0,
            start_seen: false,
            end_seen: false,
            checksum_ok: false,
            acknowledged: false,
        };

        self.clear_transmit_flags(TRANSMIT_END_OF_PACKET_FLAG);

        let waiting_since = self.micros();
        while self.micros().wrapping_sub(waiting_since) <= REPLY_START_TIMEOUT_US {
            // wait for receiption to start
            if self.receive_flags() & RECEIVE_START_FLAG != 0 {
                reception.start_seen = true;
                break;
            }
        }

        // if we hit the timeout and did not see a start of packet, return early
        if !reception.start_seen {
            return reception;
        }

        let mut checksum = Checksum::new();
        let mut last_activity = self.micros();

        loop {
            // read bytes from the PIO until we see an end of packet or timeout
            if let Some(byte) = self.read_byte() {
                checksum.fold(received, reception.len);
                // store the received bytes
                store(received, &mut reception.len, byte);
                last_activity = self.micros();
            }

            // if we see an end of packet, mark it and break
            if self.receive_flags() & RECEIVE_END_OF_PACKET_FLAG != 0 {
                reception.end_seen = true;
                break;
            }

            // if we have not seen any activity for a while, break
            if self.micros().wrapping_sub(last_activity) > REPLY_GAP_TIMEOUT_US {
                break;
            }
        }

        // consume the rest of the bytes in the fifo if any
        while let Some(byte) = self.read_byte() {
            checksum.fold(received, reception.len);
            store(received, &mut reception.len, byte);
        }

        // set whether checksum match
        reception.checksum_ok = checksum.matches(received, reception.len);

        if armed // armed is whether if we should ack this packet or not
            && reception.end_seen
            && reception.checksum_ok
            && looks_like_data(received, reception.len) // looks_like_data checks if the packet is a data packet
        {
            // send the ack
            self.fire_acknowledge();
            reception.acknowledged = true;


            let fired_at = self.micros();
            // start the timeout
            while self.micros().wrapping_sub(fired_at) <= TRANSMIT_TIMEOUT_US {
                // if we see an end of packet, break
                if self.transmit_flags() & TRANSMIT_END_OF_PACKET_FLAG != 0 {
                    break;
                }
            }
            self.last_end_of_packet_at = self.micros();
        }

        reception
    }

    #[inline(always)]
    fn micros(&self) -> u32 {
        unsafe { read_volatile(self.registers.microseconds) }
    }

    #[inline(always)]
    fn transmit_flags(&self) -> u8 {
        (unsafe { read_volatile(self.registers.transmit_flags) } & 0xFF) as u8
    }

    #[inline(always)]
    fn clear_transmit_flags(&self, flags: u8) {
        unsafe { write_volatile(self.registers.transmit_flags_clear, u32::from(flags)) }
    }

    #[inline(always)]
    fn receive_flags(&self) -> u8 {
        (unsafe { read_volatile(self.registers.receive_flags) } & 0xFF) as u8
    }

    #[inline(always)]
    fn clear_receive_flags(&self, flags: u8) {
        unsafe { write_volatile(self.registers.receive_flags_clear, u32::from(flags)) }
    }

    #[inline(always)]
    fn read_byte(&self) -> Option<u8> {
        let status = unsafe { read_volatile(self.registers.receive_status) };
        
        // if the decoder fifo is empty, return None
        if status & DECODER_EMPTY_MASK != 0 {
            return None;
        }

        // read the word and convert to bytes
        let word = unsafe { read_volatile(self.registers.receive_fifo) };
        let [byte, _, _, _] = word.to_be_bytes();
        Some(byte)
    }

    #[inline(always)]
    fn fire_acknowledge(&self) {
        unsafe {
            // dispatch transmit instruction in order to be able to send anything
            write_volatile(self.registers.transmit_instruction, self.start_word);
            for word in self.acknowledge {
                // transmit stored ack packet
                write_volatile(self.registers.transmit_fifo, word);
            }
        }
    }

    /// resets the decoder
    fn arm_receiver(&mut self) -> Result<()> {
        self.park_decoder()?;
        let Decoder::Parked(mut parked) = replace(&mut self.decoder, Decoder::Missing) else {
            return Err(Error::ReceiverUnavailable);
        };

        // clear stored bytes from prev txs
        parked.clear_fifos();

        // restart the decoder program to reset its state
        parked.exec_instruction(decoder_restart_instruction(self.decoder_offset));
        parked.exec_instruction(DECODER_CLEAR_INPUT_SHIFT);
        parked.exec_instruction(DECODER_RESET_LINE_STATE);
        parked.exec_instruction(DECODER_PRELOAD_ONES);
        self.receive_block.clear_irq(DECODER_TRIGGER_FLAG);

        // re arm the decoder to listen for packets
        self.decoder = Decoder::Listening(parked.start());
        Ok(())
    }

    fn encode(&self, packets: &[&[u8]], words: &mut [u32]) -> Result<usize> {
        let raw_len = packets.iter().map(|packet| packet.len()).sum::<usize>();
        if raw_len > LONGEST_PACKET || packets.len() > MAX_PACKETS {
            return Err(Error::PacketTooLong(raw_len));
        }

        let mut encoded = [0u8; ENCODED_CAPACITY];
        let encoded_len = encode_sequence(packets, &mut encoded)?;

        Ok(pack(encoded.get(..encoded_len).unwrap_or(&[]), words))
    }

    /// stops the transmitter from executing further instructions
    fn park_transmitter(&mut self) -> Result<()> {
        match replace(&mut self.transmitter, Transmitter::Missing) {
            Transmitter::Parked(parked) => {
                self.transmitter = Transmitter::Parked(parked);
                Ok(())
            }
            Transmitter::Live(live) => {
                // need to wait for any kind of transmission to finish. 
                // e.g. await_ends returns when eop fires so we need at least 1.6micros
                while self.micros().wrapping_sub(self.last_end_of_packet_at) < PARK_SETTLE_US {}
                self.transmitter = Transmitter::Parked(live.stop());
                Ok(())
            }
            Transmitter::Missing => Err(Error::TransmitterUnavailable),
        }
    }

    /// stops the decoder from executing further instructions
    fn park_decoder(&mut self) -> Result<()> {
        match replace(&mut self.decoder, Decoder::Missing) {
            Decoder::Parked(parked) => {
                self.decoder = Decoder::Parked(parked);
                Ok(())
            }
            Decoder::Listening(listening) => {
                self.decoder = Decoder::Parked(listening.stop());
                Ok(())
            }
            Decoder::Missing => Err(Error::ReceiverUnavailable),
        }
    }

    fn exec_parked(&mut self, instruction: Instruction) -> Result<()> {
        let Transmitter::Parked(parked) = &mut self.transmitter else {
            return Err(Error::TransmitterUnavailable);
        };
        parked.exec_instruction(instruction);
        Ok(())
    }
}

fn configure_pad<I: PinId, P: PullType>(pad: &mut Pin<I, FunctionPio0, P>) {
    // we set the input override to invert so that end of packet, when both lines
    // are low, reads as both high, since the detector can only check if a pin is high
    pad.set_input_override(InputOverride::Invert);
    pad.set_drive_strength(OutputDriveStrength::TwelveMilliAmps);
    pad.set_slew_rate(OutputSlewRate::Fast);
}

/// packs bytes into u32 words, padding with IDLE_SYMBOLS. Returns the number of words written.
fn pack(encoded: &[u8], words: &mut [u32]) -> usize {
    let mut count = 0;

    for (word, chunk) in words.iter_mut().zip(encoded.chunks(BYTES_PER_WORD)) {
        let mut bytes = [IDLE_SYMBOLS; BYTES_PER_WORD];
        for (slot, byte) in bytes.iter_mut().zip(chunk) {
            *slot = *byte;
        }
        *word = u32::from_be_bytes(bytes);
        count += 1;
    }

    count
}

/// Prebuild acknowledge packet so we can send it out quickly
fn packed_acknowledge() -> [u32; ACKNOWLEDGE_WORDS] {
    let mut words = [0u32; ACKNOWLEDGE_WORDS];
    pack(&ACKNOWLEDGE_ENCODED, &mut words);
    words
}

#[inline(always)]
fn looks_like_data(received: &[u8], len: usize) -> bool {
    if len < DATA_PACKET_MIN_LEN {
        return false;
    }
    matches!(
        received.get(..CHECKSUM_STARTS_AT),
        Some([SYNC, PID_DATA0]) | Some([SYNC, PID_DATA1])
    )
}

#[inline(always)]
fn store(received: &mut [u8], len: &mut usize, byte: u8) {
    if let Some(slot) = received.get_mut(*len) {
        *slot = byte;
        *len = len.wrapping_add(1);
    }
}

struct Checksum {
    running: u16,
}

impl Checksum {
    #[inline(always)]
    const fn new() -> Self {
        Self {
            running: CRC16_INITIAL,
        }
    }

    #[inline(always)]
    fn fold(&mut self, received: &[u8], len: usize) {
        if len < CHECKSUM_STARTS_AT + CHECKSUM_LEN {
            return;
        }

        // get the last two bytes of the received data and fold them into the running checksum
        if let Some(leaving) = received.get(len.wrapping_sub(CHECKSUM_LEN)) {
            self.running = fold_byte(self.running, *leaving);
        }
    }

    #[inline(always)]
    fn matches(&self, received: &[u8], len: usize) -> bool {
        if len < CHECKSUM_STARTS_AT + CHECKSUM_LEN {
            return false;
        }
        let declared = match received.get(len.wrapping_sub(CHECKSUM_LEN)..len) {
            Some([low, high]) => u16::from_le_bytes([*low, *high]),
            _ => return false,
        };
        self.running ^ CRC16_FINAL_XOR == declared
    }
}

#[inline(always)]
fn fold_byte(crc: u16, byte: u8) -> u16 {
    let low = fold_nibble(crc, u16::from(byte) & NIBBLE_MASK);
    fold_nibble(low, u16::from(byte) >> NIBBLE_BITS & NIBBLE_MASK)
}

#[inline(always)]
fn fold_nibble(crc: u16, nibble: u16) -> u16 {
    let index = usize::from((crc ^ nibble) & NIBBLE_MASK);
    let entry = unsafe { read_volatile(NIBBLE_TABLE.as_ptr().wrapping_add(index)) };
    crc >> NIBBLE_BITS ^ entry
}

const fn divisor(cycles_per_bit: u32) -> (u16, u8) {
    let steps =
        SYS_HZ as u64 * DIVISOR_FRACTION_STEPS / (FULL_SPEED_BIT_HZ as u64 * cycles_per_bit as u64);
    (
        (steps / DIVISOR_FRACTION_STEPS) as u16,
        (steps % DIVISOR_FRACTION_STEPS) as u8,
    )
}

const fn divides_exactly(cycles_per_bit: u32) -> bool {
    (SYS_HZ as u64 * DIVISOR_FRACTION_STEPS)
        .is_multiple_of(FULL_SPEED_BIT_HZ as u64 * cycles_per_bit as u64)
}
