use std::fmt::{Display, Formatter, Result as FmtResult};

use rp2350wallet_protocol::wire::Slot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Status,
    Push,
    Show(Slot),
    Clear(Slot),
    Provision,
    Quit,
}

impl Action {
    pub const ALL: [Self; 8] = [
        Self::Status,
        Self::Push,
        Self::Show(Slot::Pending),
        Self::Show(Slot::Signed),
        Self::Clear(Slot::Pending),
        Self::Clear(Slot::Signed),
        Self::Provision,
        Self::Quit,
    ];
}

impl Display for Action {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Status => f.write_str("Show status"),
            Self::Push => f.write_str("Push an unsigned transaction"),
            Self::Show(slot) => write!(f, "Show the {} transaction", slot.name()),
            Self::Clear(slot) => write!(f, "Clear the {} transaction", slot.name()),
            Self::Provision => f.write_str("Provision the token table"),
            Self::Quit => f.write_str("Quit"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use rp2350wallet_protocol::wire::Slot;

    use super::Action;

    #[test]
    fn every_menu_entry_reads_differently() {
        let labels: HashSet<String> = Action::ALL.iter().map(ToString::to_string).collect();
        assert_eq!(labels.len(), Action::ALL.len());
    }

    #[test]
    fn every_menu_entry_appears_once() {
        for (index, entry) in Action::ALL.iter().enumerate() {
            assert!(
                !Action::ALL[index + 1..].contains(entry),
                "{entry} is listed twice"
            );
        }
    }

    #[test]
    fn quit_is_the_last_entry() {
        assert_eq!(Action::ALL.last(), Some(&Action::Quit));
    }

    #[test]
    fn both_slots_can_be_shown_and_cleared() {
        for slot in Slot::ALL {
            assert!(Action::ALL.contains(&Action::Show(slot)));
            assert!(Action::ALL.contains(&Action::Clear(slot)));
        }
    }

    #[test]
    fn slot_entries_name_their_slot() {
        assert_eq!(
            Action::Show(Slot::Pending).to_string(),
            "Show the pending transaction"
        );
        assert_eq!(
            Action::Clear(Slot::Signed).to_string(),
            "Clear the signed transaction"
        );
    }
}
