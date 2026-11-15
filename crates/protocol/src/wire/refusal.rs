use crate::{bitcoin, evm, policy, slots, store};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Refusal {
    BothSlotsOccupied = 1,
    PendingAlreadyStored = 2,
    SignedNotCollected = 3,
    NothingToSign = 4,
    NothingSigned = 5,
    MalformedTransaction = 6,
    NotATransfer = 7,
    TooLarge = 11,
    StorageFull = 12,
    NoTokenDescriptor = 17,
    NoLedger = 13,
    LedgerDeclined = 14,
    LedgerFailed = 15,
    SigningTimedOut = 16,
    UnknownAsset = 18,
    TooPrecise = 19,
    AmountTooLarge = 20,
    RecipientIsContract = 21,
    MalformedBitcoin = 22,
    CoinShort = 23,
    UnknownChain = 24,
    DustPayment = 25,
}

impl Refusal {
    pub const fn to_byte(self) -> u8 {
        self as u8
    }
}

impl From<slots::Error> for Refusal {
    fn from(error: slots::Error) -> Self {
        match error {
            slots::Error::BothSlotsOccupied => Self::BothSlotsOccupied,
            slots::Error::PendingAlreadyStored => Self::PendingAlreadyStored,
            slots::Error::SignedNotCollected => Self::SignedNotCollected,
            slots::Error::NothingToSign => Self::NothingToSign,
            slots::Error::NothingSigned => Self::NothingSigned,
        }
    }
}

impl From<policy::Error> for Refusal {
    fn from(error: policy::Error) -> Self {
        match error {
            policy::Error::Transaction(_) => Self::MalformedTransaction,
            policy::Error::Transfer(_) => Self::NotATransfer,
            policy::Error::TooLongToSign { .. } => Self::TooLarge,
        }
    }
}

impl From<store::Error> for Refusal {
    fn from(error: store::Error) -> Self {
        match error {
            store::Error::RecordTooSmall(_) => Self::StorageFull,
            store::Error::PayloadTooLarge { .. } => Self::TooLarge,
        }
    }
}

impl From<evm::Error> for Refusal {
    fn from(error: evm::Error) -> Self {
        match error {
            evm::Error::WrongLength(_) | evm::Error::Transaction(_) => Self::MalformedTransaction,
            evm::Error::UnknownAsset => Self::UnknownAsset,
            evm::Error::TooPrecise => Self::TooPrecise,
            evm::Error::AmountTooLarge => Self::AmountTooLarge,
            evm::Error::RecipientIsContract => Self::RecipientIsContract,
        }
    }
}

impl From<bitcoin::Error> for Refusal {
    fn from(error: bitcoin::Error) -> Self {
        match error {
            bitcoin::Error::Malformed => Self::MalformedBitcoin,
            bitcoin::Error::Short => Self::CoinShort,
            bitcoin::Error::Dust => Self::DustPayment,
            bitcoin::Error::OutputTooSmall(_) => Self::TooLarge,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Refusal;
    use crate::bitcoin;
    use crate::evm;
    use crate::policy;
    use crate::slots;
    use crate::store;
    use crate::transaction;
    use crate::transfer;

    const WIRE_BYTES: [(Refusal, u8); 22] = [
        (Refusal::BothSlotsOccupied, 1),
        (Refusal::PendingAlreadyStored, 2),
        (Refusal::SignedNotCollected, 3),
        (Refusal::NothingToSign, 4),
        (Refusal::NothingSigned, 5),
        (Refusal::MalformedTransaction, 6),
        (Refusal::NotATransfer, 7),
        (Refusal::TooLarge, 11),
        (Refusal::StorageFull, 12),
        (Refusal::NoLedger, 13),
        (Refusal::LedgerDeclined, 14),
        (Refusal::LedgerFailed, 15),
        (Refusal::SigningTimedOut, 16),
        (Refusal::NoTokenDescriptor, 17),
        (Refusal::UnknownAsset, 18),
        (Refusal::TooPrecise, 19),
        (Refusal::AmountTooLarge, 20),
        (Refusal::RecipientIsContract, 21),
        (Refusal::MalformedBitcoin, 22),
        (Refusal::CoinShort, 23),
        (Refusal::UnknownChain, 24),
        (Refusal::DustPayment, 25),
    ];

    #[test]
    fn every_refusal_keeps_the_wire_byte_the_host_already_knows() {
        for (refusal, byte) in WIRE_BYTES {
            assert_eq!(refusal.to_byte(), byte, "{refusal:?}");
        }
    }

    #[test]
    fn every_evm_error_maps_to_its_refusal() {
        let pairs = [
            (evm::Error::WrongLength(0), Refusal::MalformedTransaction),
            (
                evm::Error::Transaction(transaction::Error::Empty),
                Refusal::MalformedTransaction,
            ),
            (evm::Error::UnknownAsset, Refusal::UnknownAsset),
            (evm::Error::TooPrecise, Refusal::TooPrecise),
            (evm::Error::AmountTooLarge, Refusal::AmountTooLarge),
            (
                evm::Error::RecipientIsContract,
                Refusal::RecipientIsContract,
            ),
        ];
        for (error, refusal) in pairs {
            assert_eq!(Refusal::from(error), refusal);
        }
    }

    #[test]
    fn every_bitcoin_error_maps_to_its_refusal() {
        assert_eq!(
            Refusal::from(bitcoin::Error::Malformed),
            Refusal::MalformedBitcoin
        );
        assert_eq!(Refusal::from(bitcoin::Error::Short), Refusal::CoinShort);
        assert_eq!(Refusal::from(bitcoin::Error::Dust), Refusal::DustPayment);
        assert_eq!(
            Refusal::from(bitcoin::Error::OutputTooSmall(0)),
            Refusal::TooLarge
        );
    }

    #[test]
    fn every_policy_error_maps_to_its_own_refusal() {
        let pairs = [
            (
                policy::Error::Transaction(transaction::Error::Empty),
                Refusal::MalformedTransaction,
            ),
            (
                policy::Error::Transfer(transfer::Error::ContractCreation),
                Refusal::NotATransfer,
            ),
            (
                policy::Error::TooLongToSign {
                    len: 200,
                    capacity: 244,
                },
                Refusal::TooLarge,
            ),
        ];

        for (error, refusal) in pairs {
            assert_eq!(Refusal::from(error), refusal);
        }
    }

    #[test]
    fn every_slot_error_maps_to_its_own_refusal() {
        let pairs = [
            (slots::Error::BothSlotsOccupied, Refusal::BothSlotsOccupied),
            (
                slots::Error::PendingAlreadyStored,
                Refusal::PendingAlreadyStored,
            ),
            (
                slots::Error::SignedNotCollected,
                Refusal::SignedNotCollected,
            ),
            (slots::Error::NothingToSign, Refusal::NothingToSign),
            (slots::Error::NothingSigned, Refusal::NothingSigned),
        ];
        for (error, refusal) in pairs {
            assert_eq!(Refusal::from(error), refusal);
        }
    }

    #[test]
    fn a_shape_refusal_names_which_shape_was_wrong() {
        assert_eq!(
            Refusal::from(policy::Error::Transaction(transaction::Error::Empty)),
            Refusal::MalformedTransaction
        );
        assert_eq!(
            Refusal::from(policy::Error::Transfer(transfer::Error::ContractCreation)),
            Refusal::NotATransfer
        );
    }

    #[test]
    fn storage_errors_map_to_their_refusals() {
        assert_eq!(
            Refusal::from(store::Error::PayloadTooLarge {
                len: 300,
                capacity: 244
            }),
            Refusal::TooLarge
        );
        assert_eq!(
            Refusal::from(store::Error::RecordTooSmall(4)),
            Refusal::StorageFull
        );
    }
}
