use crate::state::DeviceState;

// A match arm needs a constant pattern, so the enum's byte is bound here.
const STATE_IDLE: u8 = DeviceState::Idle.to_byte();
// A match arm needs a constant pattern, so the enum's byte is bound here.
const STATE_PENDING: u8 = DeviceState::PendingStored.to_byte();
// A match arm needs a constant pattern, so the enum's byte is bound here.
const STATE_SIGNED: u8 = DeviceState::SignedStored.to_byte();

const WHITE: u32 = 0xFF_FF_FF;
const YELLOW: u32 = 0xFF_FF_00;
const GREEN: u32 = 0x00_FF_00;
const RED: u32 = 0xFF_00_00;

pub fn lamp_color(state: u8) -> u32 {
    match state {
        STATE_IDLE => WHITE,
        STATE_PENDING => YELLOW,
        STATE_SIGNED => GREEN,
        _ => RED,
    }
}

#[cfg(test)]
mod tests {
    use super::GREEN;
    use super::RED;
    use super::STATE_IDLE;
    use super::STATE_PENDING;
    use super::STATE_SIGNED;
    use super::WHITE;
    use super::YELLOW;
    use super::lamp_color;
    use crate::state::FAULT_BYTE;

    #[test]
    fn each_state_has_its_own_solid_color_and_a_fault_is_red() {
        assert_eq!(lamp_color(STATE_IDLE), WHITE);
        assert_eq!(lamp_color(STATE_PENDING), YELLOW);
        assert_eq!(lamp_color(STATE_SIGNED), GREEN);
        assert_eq!(lamp_color(FAULT_BYTE), RED);
    }

    #[test]
    fn an_unrecognised_state_byte_shows_the_fault_color() {
        for state in [0x7Fu8, 0xA5, 0xFE, 0xFF] {
            assert_eq!(lamp_color(state), RED);
        }
    }
}
