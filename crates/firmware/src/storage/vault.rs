use txcourier_protocol::slots::Occupancy;
use txcourier_protocol::store::{
    TOKEN_TABLE_RECORD_LEN, Kind, Record, SLOT_RECORD_LEN, Slot, decode, encode, scan,
};
use txcourier_protocol::token::Tokens;
use txcourier_protocol::token::decode as decode_tokens;

use crate::storage::error::Error;
use crate::storage::flash::{Flash, Region};
use crate::storage::result::Result;

// One stack buffer serves every region, so it is sized for the larger record, the token
// table record.
const MAX_RECORD_LEN: usize = TOKEN_TABLE_RECORD_LEN;

const _: () = assert!(SLOT_RECORD_LEN <= MAX_RECORD_LEN);

/// The host device memory 
pub struct Vault {
    flash: Flash,
    pending: Region,
    signed: Region,
    token_table: Region,
    occupancy: Occupancy,
    tokens: Option<Tokens>,
}

impl Vault {
    pub fn new() -> Self {
        // create vault instance
        let mut vault = Self {
            flash: Flash::new(),
            pending: Region::pending(SLOT_RECORD_LEN),
            signed: Region::signed(SLOT_RECORD_LEN),
            token_table: Region::token_table(TOKEN_TABLE_RECORD_LEN),
            occupancy: Occupancy::EMPTY,
            tokens: None,
        };

        // refresh memory by reading from flash
        vault.refresh();
        vault
    }

    /// state of the host device
    pub fn occupancy(&self) -> Occupancy {
        self.occupancy
    }

    /// whether there are any tokens programmed in device
    pub fn provisioned(&self) -> bool {
        self.tokens.is_some()
    }

    /// read the pending region
    pub fn read_pending(&self, out: &mut [u8]) -> usize {
        read(&self.pending, out)
    }

    /// read the signed region
    pub fn read_signed(&self, out: &mut [u8]) -> usize {
        read(&self.signed, out)
    }

    /// access the programmed allowed tokens
    pub fn tokens(&self) -> Option<&Tokens> {
        self.tokens.as_ref()
    }

    /// store a pending unsigned transaction
    pub fn store_pending(&mut self, payload: &[u8]) -> Result<()> {
        self.occupancy.may_store_pending()?;
        self.append(self.pending, payload)
    }

    /// replace a pending unsigned transaction with a signed transaction
    pub fn replace_pending_with_signed(&mut self, payload: &[u8]) -> Result<()> {
        self.occupancy.may_store_signed()?;
        self.tombstone(self.pending)?;
        self.append(self.signed, payload)
    }

    /// clear a pending transaction
    pub fn clear_pending(&mut self) -> Result<()> {
        self.occupancy.may_clear_pending()?;
        self.tombstone(self.pending)
    }

    /// clear a signed transaction
    pub fn clear_signed(&mut self) -> Result<()> {
        self.occupancy.may_clear_signed()?;
        self.tombstone(self.signed)
    }

    /// stores allowed tokens in the system
    pub fn store_tokens(&mut self, payload: &[u8]) -> Result<()> {
        self.append(self.token_table, payload)
    }

    /// append some payload to a region, this will overwrite if it is full
    fn append(&mut self, region: Region, payload: &[u8]) -> Result<()> {
        // find the next area where we can add data
        let found = scan(region.contents(), region.record_len);

        // the index is the position of the slot
        // and sequence is the number of slots, so essnetially
        // corrupt slots are skipped
        let (index, sequence) = if found.full {
            self.flash.erase_sector(&region);
            (0, 0)
        } else {
            (found.next_index, found.next_sequence)
        };

        // write to that region
        self.write(
            region,
            index,
            &Record {
                sequence,
                kind: Kind::Live,
                payload,
            },
        )
    }

    /// Quick way to mark region as deleted
    fn tombstone(&mut self, region: Region) -> Result<()> {
        
        // scan to find region
        let found = scan(region.contents(), region.record_len);

        if found.full {
            self.flash.erase_sector(&region);
            self.refresh();
            return Ok(());
        }

        // then we simply make this region on the next sequence
        // which means whenever someone tries to read for this region 
        // they will see tombstone value
        self.write(
            region,
            found.next_index,
            &Record {
                sequence: found.next_sequence,
                kind: Kind::Tombstone,
                payload: &[],
            },
        )
    }

    fn write(&mut self, region: Region, index: usize, record: &Record<'_>) -> Result<()> {
        let mut bytes = [0u8; MAX_RECORD_LEN];
        let Some(slot) = bytes.get_mut(..region.record_len) else {
            return Err(Error::RegionUnsupported(region.record_len));
        };

        // encode the requested data
        encode(record, slot)?;

        // write record then refresh our local state
        self.flash.write_record(&region, index, slot);
        self.refresh();

        Ok(())
    }

    /// reads from flash to ensure we have state on stack
    fn refresh(&mut self) {
        self.occupancy = Occupancy {
            pending: occupied(&self.pending),
            signed: occupied(&self.signed),
        };
        self.tokens = stored_tokens(&self.token_table);
    }
}

/// returns the stored tokens from the region
fn stored_tokens(region: &Region) -> Option<Tokens> {
    let mut blob = [0u8; MAX_RECORD_LEN];
    let len = read(region, &mut blob);
    decode_tokens(blob.get(..len).unwrap_or(&[])).ok()
}

/// checks if a region has data inside
fn occupied(region: &Region) -> bool {
    scan(region.contents(), region.record_len)
        .live_index
        .is_some()
}

/// read the contents of a region
fn read(region: &Region, out: &mut [u8]) -> usize {
    let contents = region.contents();
    let found = scan(contents, region.record_len);

    // extract the index
    let Some(index) = found.live_index else {
        return 0;
    };

    // compute start position
    let start = index * region.record_len;
    // extract thge bytes from the section
    let Some(bytes) = contents.get(start..start + region.record_len) else {
        return 0;
    };

    // read it as a record
    let Slot::Written(record) = decode(bytes) else {
        return 0;
    };

    // get mutable value for the slot
    let Some(slot) = out.get_mut(..record.payload.len()) else {
        return 0;
    };

    // copy the payload into it
    slot.copy_from_slice(record.payload);

    record.payload.len()
}
