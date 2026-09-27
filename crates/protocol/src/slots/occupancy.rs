use crate::slots::error::Error;
use crate::slots::result::Result;
use crate::state::DeviceState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Abstraction for whether a pending or signed transaction
/// exists in the system
pub struct Occupancy {
    pub pending: bool,
    pub signed: bool,
}

impl Occupancy {
    // Both slots free, which is also what a freshly erased vault reads back as.
    pub const EMPTY: Self = Self {
        pending: false,
        signed: false,
    };

    /// Return device state
    pub const fn state(self) -> Result<DeviceState> {
        match (self.pending, self.signed) {
            (false, false) => Ok(DeviceState::Idle),
            (true, false) => Ok(DeviceState::PendingStored),
            (false, true) => Ok(DeviceState::SignedStored),
            (true, true) => Err(Error::BothSlotsOccupied),
        }
    }

    /// Can only store pending if both slots are empty
    pub const fn may_store_pending(self) -> Result<()> {
        match (self.pending, self.signed) {
            (false, false) => Ok(()),
            (true, false) => Err(Error::PendingAlreadyStored),
            (false, true) => Err(Error::SignedNotCollected),
            (true, true) => Err(Error::BothSlotsOccupied),
        }
    }

    /// Can only store signed if the pending slot is occupied
    pub const fn may_store_signed(self) -> Result<()> {
        match (self.pending, self.signed) {
            (true, false) => Ok(()),
            (false, false) => Err(Error::NothingToSign),
            (false, true) => Err(Error::SignedNotCollected),
            (true, true) => Err(Error::BothSlotsOccupied),
        }
    }

    /// Can only clear pending if the signed is not occupied
    pub const fn may_clear_pending(self) -> Result<()> {
        match (self.pending, self.signed) {
            (true, false) => Ok(()),
            (true, true) => Err(Error::BothSlotsOccupied),
            _ => Err(Error::NothingToSign),
        }
    }

    /// Can only clear signed if only signed occupied
    pub const fn may_clear_signed(self) -> Result<()> {
        match (self.pending, self.signed) {
            (false, true) => Ok(()),
            (true, true) => Err(Error::BothSlotsOccupied),
            _ => Err(Error::NothingSigned),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::Occupancy;
    use crate::state::DeviceState;

    const EMPTY: Occupancy = Occupancy {
        pending: false,
        signed: false,
    };
    const PENDING: Occupancy = Occupancy {
        pending: true,
        signed: false,
    };
    const SIGNED: Occupancy = Occupancy {
        pending: false,
        signed: true,
    };
    const BOTH: Occupancy = Occupancy {
        pending: true,
        signed: true,
    };

    #[test]
    fn each_occupancy_maps_to_the_state_the_lamp_shows() {
        assert_eq!(EMPTY.state(), Ok(DeviceState::Idle));
        assert_eq!(PENDING.state(), Ok(DeviceState::PendingStored));
        assert_eq!(SIGNED.state(), Ok(DeviceState::SignedStored));
    }

    #[test]
    fn both_slots_occupied_is_never_a_state() {
        assert_eq!(BOTH.state(), Err(Error::BothSlotsOccupied));
        assert_eq!(BOTH.may_store_pending(), Err(Error::BothSlotsOccupied));
        assert_eq!(BOTH.may_store_signed(), Err(Error::BothSlotsOccupied));
        assert_eq!(BOTH.may_clear_signed(), Err(Error::BothSlotsOccupied));
        assert_eq!(BOTH.may_clear_pending(), Err(Error::BothSlotsOccupied));
    }

    #[test]
    fn a_pending_transaction_is_only_accepted_when_the_device_is_idle() {
        assert_eq!(EMPTY.may_store_pending(), Ok(()));
        assert_eq!(
            PENDING.may_store_pending(),
            Err(Error::PendingAlreadyStored)
        );
        assert_eq!(SIGNED.may_store_pending(), Err(Error::SignedNotCollected));
    }

    #[test]
    fn a_signature_is_only_accepted_when_something_is_pending() {
        assert_eq!(PENDING.may_store_signed(), Ok(()));
        assert_eq!(EMPTY.may_store_signed(), Err(Error::NothingToSign));
        assert_eq!(SIGNED.may_store_signed(), Err(Error::SignedNotCollected));
    }

    #[test]
    fn only_a_stored_signature_can_be_cleared() {
        assert_eq!(SIGNED.may_clear_signed(), Ok(()));
        assert_eq!(EMPTY.may_clear_signed(), Err(Error::NothingSigned));
        assert_eq!(PENDING.may_clear_signed(), Err(Error::NothingSigned));
    }

    #[test]
    fn only_a_stored_pending_transaction_can_be_abandoned() {
        assert_eq!(PENDING.may_clear_pending(), Ok(()));
        assert_eq!(EMPTY.may_clear_pending(), Err(Error::NothingToSign));
        assert_eq!(SIGNED.may_clear_pending(), Err(Error::NothingToSign));
    }

    #[test]
    fn the_two_slots_can_never_both_be_filled_by_a_permitted_action() {
        for pending in [false, true] {
            for signed in [false, true] {
                let occupancy = Occupancy { pending, signed };
                assert!(
                    !(occupancy.may_store_pending().is_ok()
                        && occupancy.may_store_signed().is_ok()),
                    "{occupancy:?} permits filling both slots"
                );
            }
        }
    }
}
