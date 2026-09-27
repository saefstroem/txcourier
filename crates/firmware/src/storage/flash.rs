use core::slice::from_raw_parts;

use rp235x_hal as hal;

use hal::rom_data;
use rp2350wallet_protocol::store::SECTOR_LEN;

// RP2350 datasheet: XIP maps flash at 0x1000_0000. Subtracting it converts a linked address
// to the flash offset the boot ROM's erase and program calls take.
const FLASH_BASE: usize = 0x1000_0000;
// W25Q16JV datasheet: command D8h erases a 64 KiB block. flash_range_erase takes this size
// and uses the block command only for a count of at least one block.
const BLOCK_LEN: u32 = 65_536;
// W25Q16JV datasheet: D8h is the 64 KiB block erase. Every erase here is one 4 KiB sector,
// so the boot ROM only ever sends 20h.
const BLOCK_ERASE_COMMAND: u8 = 0xD8;

type Plain = unsafe extern "C" fn();
type Erase = unsafe extern "C" fn(u32, usize, u32, u8);
type Program = unsafe extern "C" fn(u32, *const u8, usize);

unsafe extern "C" {
    static __pending_sector: u8;
    static __signed_sector: u8;
    static __token_table_sector: u8;
}

#[derive(Clone, Copy)]
pub struct Region {
    base: usize,
    pub record_len: usize,
}

#[derive(Clone, Copy)]
pub struct Flash {
    connect: Plain,
    exit_execute_in_place: Plain,
    erase: Erase,
    program: Program,
    flush_cache: Plain,
    enter_execute_in_place: Plain,
}

impl Region {
    /// return the pending tx region
    pub fn pending(record_len: usize) -> Self {
        Self {
            base: (&raw const __pending_sector) as usize,
            record_len,
        }
    }

    /// return the signed region
    pub fn signed(record_len: usize) -> Self {
        Self {
            base: (&raw const __signed_sector) as usize,
            record_len,
        }
    }

    /// returns the token table region
    pub fn token_table(record_len: usize) -> Self {
        Self {
            base: (&raw const __token_table_sector) as usize,
            record_len,
        }
    }

    /// returns contents of the region
    pub fn contents(&self) -> &'static [u8] {
        unsafe { from_raw_parts(self.base as *const u8, SECTOR_LEN) }
    }

    /// compute the offset from the start of the flash section
    fn offset(&self) -> u32 {
        (self.base - FLASH_BASE) as u32
    }
}

impl Flash {
    pub fn new() -> Self {
        Self {
            connect: rom_data::connect_internal_flash::ptr(),
            exit_execute_in_place: rom_data::flash_exit_xip::ptr(),
            erase: rom_data::flash_range_erase::ptr(),
            program: rom_data::flash_range_program::ptr(),
            flush_cache: rom_data::flash_flush_cache::ptr(),
            enter_execute_in_place: rom_data::flash_enter_cmd_xip::ptr(),
        }
    }

    /// Erase a region completely
    pub fn erase_sector(&self, region: &Region) {
        unsafe { self.run_erase(region.offset(), SECTOR_LEN) }
    }

    /// Writes record to a region in the flash memory
    pub fn write_record(&self, region: &Region, index: usize, record: &[u8]) {
        let offset = region.offset() + (index * region.record_len) as u32;
        unsafe { self.run_program(offset, record.as_ptr(), record.len()) }
    }

    /// Erases `count` bytes of flash at `offset` 
    #[unsafe(link_section = ".data.storage")]
    #[inline(never)]
    unsafe fn run_erase(&self, offset: u32, count: usize) {
        unsafe {
            // route the QSPI pins to the flash chip
            (self.connect)();
            // take the chip out of continuous-read mode so it accepts commands
            (self.exit_execute_in_place)();
            // write-enable + sector erase, one per 4 KiB sector; spin until done
            (self.erase)(offset, count, BLOCK_LEN, BLOCK_ERASE_COMMAND);
            // drop cached copies of the bytes that were just erased
            (self.flush_cache)();
            // back to memory-mapped reads, in the plain 03h mode; flash is readable again
            (self.enter_execute_in_place)();
        }
    }

    /// Writes `count` bytes from `data` to flash at `offset` through the boot ROM.
    #[unsafe(link_section = ".data.storage")]
    #[inline(never)]
    unsafe fn run_program(&self, offset: u32, data: *const u8, count: usize) {
        unsafe {
            // route the QSPI pins to the flash chip
            (self.connect)();
            // take the chip out of continuous-read mode so it accepts commands
            (self.exit_execute_in_place)();
            // write-enable + page-program, one per 256-byte page; spin until done
            (self.program)(offset, data, count);
            // drop cached copies of the old 0xFF bytes
            (self.flush_cache)();
            // back to memory-mapped reads, in the plain 03h mode; flash is readable again
            (self.enter_execute_in_place)();
        }
    }
}
