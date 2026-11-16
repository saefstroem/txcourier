use crate::store::record::payload_capacity;

// W25Q16JV datasheet: a sector is 4 KiB, the unit and alignment the boot ROM erases in. One
// sector per region, so erasing one slot cannot reach another.
pub const SECTOR_LEN: usize = 4096;
// W25Q16JV datasheet: a page is 256 bytes, the unit and alignment the boot ROM programs in.
pub const PAGE_LEN: usize = 256;

// One page, the smallest record the boot ROM programs. Sixteen fit a sector, and a tombstone
// spends one.
pub const SLOT_RECORD_LEN: usize = 256;
// The only page multiple that divides the sector, holds a 1945-byte token table with 12
// bytes of overhead, and fits one 2048-byte frame. Two per sector, so every third write
// erases.
pub const TOKEN_TABLE_RECORD_LEN: usize = 2048;

const _: () = assert!(SLOT_RECORD_LEN.is_multiple_of(PAGE_LEN));
const _: () = assert!(TOKEN_TABLE_RECORD_LEN.is_multiple_of(PAGE_LEN));
const _: () = assert!(SECTOR_LEN.is_multiple_of(SLOT_RECORD_LEN));
const _: () = assert!(SECTOR_LEN.is_multiple_of(TOKEN_TABLE_RECORD_LEN));
const _: () = assert!(payload_capacity(SLOT_RECORD_LEN) > 0);
const _: () = assert!(payload_capacity(TOKEN_TABLE_RECORD_LEN) > 0);

#[cfg(test)]
mod tests {
    use super::SECTOR_LEN;
    use super::SLOT_RECORD_LEN;
    use super::TOKEN_TABLE_RECORD_LEN;
    use crate::store::record::payload_capacity;

    #[test]
    fn a_slot_sector_holds_sixteen_transactions_before_an_erase() {
        assert_eq!(SECTOR_LEN / SLOT_RECORD_LEN, 16);
        assert_eq!(SECTOR_LEN % SLOT_RECORD_LEN, 0);
    }

    #[test]
    fn a_config_sector_holds_two_token_tables_before_an_erase() {
        assert_eq!(SECTOR_LEN / TOKEN_TABLE_RECORD_LEN, 2);
        assert_eq!(SECTOR_LEN % TOKEN_TABLE_RECORD_LEN, 0);
    }

    #[test]
    fn the_capacities_are_what_the_wire_types_promise() {
        assert_eq!(payload_capacity(SLOT_RECORD_LEN), 244);
        assert_eq!(payload_capacity(TOKEN_TABLE_RECORD_LEN), 2036);
    }
}
