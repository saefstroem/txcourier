use crate::{policy, slots, store, token};

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
    MalformedTokens = 10,
    TooLarge = 11,
    StorageFull = 12,
    NoTokenDescriptor = 17,
    NoLedger = 13,
    LedgerDeclined = 14,
    LedgerFailed = 15,
    SigningTimedOut = 16,
}

impl Refusal {
    // Every variant, so the round-trip test fails if one is added without a byte.
    pub const ALL: [Self; 15] = [
        Self::BothSlotsOccupied,
        Self::PendingAlreadyStored,
        Self::SignedNotCollected,
        Self::NothingToSign,
        Self::NothingSigned,
        Self::MalformedTransaction,
        Self::NotATransfer,
        Self::MalformedTokens,
        Self::TooLarge,
        Self::StorageFull,
        Self::NoTokenDescriptor,
        Self::NoLedger,
        Self::LedgerDeclined,
        Self::LedgerFailed,
        Self::SigningTimedOut,
    ];

    pub const fn to_byte(self) -> u8 {
        self as u8
    }

    pub fn from_byte(byte: u8) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|refusal| refusal.to_byte() == byte)
    }

    pub const fn describe(self) -> &'static str {
        match self {
            Self::BothSlotsOccupied => "both slots are occupied, which must never happen",
            Self::PendingAlreadyStored => "a pending transaction is already stored",
            Self::SignedNotCollected => "a signed transaction is waiting to be collected",
            Self::NothingToSign => "there is no pending transaction",
            Self::NothingSigned => "there is no signed transaction",
            Self::MalformedTransaction => "the transaction is not a well-formed EIP-1559 envelope",
            Self::NotATransfer => "the transaction is not a plain transfer",
            Self::MalformedTokens => "the token table blob is not a valid record",
            Self::TooLarge => "the payload is larger than the device can store",
            Self::StorageFull => "the storage sector cannot take another record",
            Self::NoTokenDescriptor => {
                "that token has no Ledger-signed descriptor, so it could only be blind signed"
            }
            Self::NoLedger => "no Ledger answered on the USB-A port",
            Self::LedgerDeclined => "you declined the transaction on the Ledger",
            Self::LedgerFailed => "the Ledger did not complete the signature",
            Self::SigningTimedOut => "the Ledger was not approved in time",
        }
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

impl From<token::Error> for Refusal {
    fn from(_: token::Error) -> Self {
        Self::MalformedTokens
    }
}

#[cfg(test)]
mod tests {
    use super::Refusal;
    use crate::policy;
    use crate::slots;
    use crate::store;
    use crate::token;
    use crate::transaction;
    use crate::transfer;

    const WIRE_BYTES: [(Refusal, u8); 15] = [
        (Refusal::BothSlotsOccupied, 1),
        (Refusal::PendingAlreadyStored, 2),
        (Refusal::SignedNotCollected, 3),
        (Refusal::NothingToSign, 4),
        (Refusal::NothingSigned, 5),
        (Refusal::MalformedTransaction, 6),
        (Refusal::NotATransfer, 7),
        (Refusal::MalformedTokens, 10),
        (Refusal::TooLarge, 11),
        (Refusal::StorageFull, 12),
        (Refusal::NoLedger, 13),
        (Refusal::LedgerDeclined, 14),
        (Refusal::LedgerFailed, 15),
        (Refusal::SigningTimedOut, 16),
        (Refusal::NoTokenDescriptor, 17),
    ];

    #[test]
    fn every_refusal_keeps_the_wire_byte_the_host_already_knows() {
        assert_eq!(WIRE_BYTES.len(), Refusal::ALL.len());
        for (refusal, byte) in WIRE_BYTES {
            assert_eq!(refusal.to_byte(), byte, "{refusal:?}");
            assert!(Refusal::ALL.contains(&refusal), "{refusal:?}");
        }
    }

    #[test]
    fn every_refusal_round_trips_through_its_byte() {
        for refusal in Refusal::ALL {
            assert_eq!(Refusal::from_byte(refusal.to_byte()), Some(refusal));
        }
    }

    #[test]
    fn zero_is_not_a_refusal() {
        assert_eq!(Refusal::from_byte(0), None);
        assert_eq!(Refusal::from_byte(0xFF), None);
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
    fn storage_and_token_table_errors_map_to_their_refusals() {
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
        assert_eq!(
            Refusal::from(token::Error::BadMagic),
            Refusal::MalformedTokens
        );
    }
}
