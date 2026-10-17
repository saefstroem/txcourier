use crate::state::DeviceState;

// Not fixed by any specification: a 1 s half-period, a slow idle heartbeat.
const HALF_PERIOD_IDLE_US: u32 = 1_000_000;
// Not fixed by any specification: five times the idle rate, so a stored signature is visibly
// not idle. This file's tests pin the 5:1 ratio.
const HALF_PERIOD_SIGNED_US: u32 = 200_000;

// A match arm needs a constant pattern, so the enum's byte is bound here.
const STATE_IDLE: u8 = DeviceState::Idle.to_byte();
// A match arm needs a constant pattern, so the enum's byte is bound here.
const STATE_PENDING: u8 = DeviceState::PendingStored.to_byte();
// A match arm needs a constant pattern, so the enum's byte is bound here.
const STATE_SIGNED: u8 = DeviceState::SignedStored.to_byte();

pub fn lamp_is_lit(now_us: u32, state: u8) -> bool {
    let half_period = match state {
        STATE_PENDING => return true,
        STATE_IDLE => HALF_PERIOD_IDLE_US,
        STATE_SIGNED => HALF_PERIOD_SIGNED_US,
        _ => return false,
    };

    (now_us / half_period).is_multiple_of(2)
}

#[cfg(test)]
mod tests {
    use super::HALF_PERIOD_IDLE_US;
    use super::HALF_PERIOD_SIGNED_US;
    use super::STATE_IDLE;
    use super::STATE_PENDING;
    use super::STATE_SIGNED;
    use super::lamp_is_lit;
    use crate::state::FAULT_BYTE;

    fn transitions(state: u8, half_period: u32, span: u32) -> usize {
        let mut changes = 0;
        let mut previous = lamp_is_lit(0, state);

        for step in 1..span {
            let now = step * (half_period / 8);
            let lit = lamp_is_lit(now, state);
            if lit != previous {
                changes += 1;
                previous = lit;
            }
        }

        changes
    }

    #[test]
    fn a_stored_pending_transaction_holds_the_lamp_solid() {
        for step in 0..64 {
            assert!(lamp_is_lit(step * 137_941, STATE_PENDING));
        }
    }

    #[test]
    fn a_fault_keeps_the_lamp_dark_at_every_instant() {
        for step in 0..64 {
            assert!(!lamp_is_lit(step * 137_941, FAULT_BYTE));
        }
    }

    #[test]
    fn an_unrecognised_state_byte_keeps_the_lamp_dark() {
        for state in [0x00u8, 0x7F, 0xA5, 0xFE] {
            if state == STATE_IDLE || state == STATE_PENDING || state == STATE_SIGNED {
                continue;
            }
            assert!(!lamp_is_lit(0, state));
            assert!(!lamp_is_lit(500_000, state));
        }
    }

    #[test]
    fn idle_blinks_once_per_two_seconds() {
        assert!(lamp_is_lit(0, STATE_IDLE));
        assert!(lamp_is_lit(HALF_PERIOD_IDLE_US - 1, STATE_IDLE));
        assert!(!lamp_is_lit(HALF_PERIOD_IDLE_US, STATE_IDLE));
        assert!(!lamp_is_lit(2 * HALF_PERIOD_IDLE_US - 1, STATE_IDLE));
        assert!(lamp_is_lit(2 * HALF_PERIOD_IDLE_US, STATE_IDLE));
    }

    #[test]
    fn a_stored_signature_blinks_five_times_faster_than_idle() {
        assert!(lamp_is_lit(0, STATE_SIGNED));
        assert!(!lamp_is_lit(HALF_PERIOD_SIGNED_US, STATE_SIGNED));
        assert!(lamp_is_lit(2 * HALF_PERIOD_SIGNED_US, STATE_SIGNED));

        let idle = transitions(STATE_IDLE, HALF_PERIOD_IDLE_US, 64);
        let signed = transitions(STATE_SIGNED, HALF_PERIOD_SIGNED_US, 64);
        assert_eq!(idle, signed);
        assert_eq!(
            HALF_PERIOD_IDLE_US / HALF_PERIOD_SIGNED_US,
            5,
            "the signed blink is meant to be visibly faster than idle"
        );
    }

    #[test]
    fn the_phase_depends_only_on_the_clock_so_it_needs_no_stored_state() {
        for state in [STATE_IDLE, STATE_SIGNED] {
            for step in 0..128 {
                let now = step * 73_331;
                assert_eq!(lamp_is_lit(now, state), lamp_is_lit(now, state));
            }
        }
    }

    #[test]
    fn the_microsecond_counter_wrapping_costs_at_most_one_irregular_blink() {
        let full_period = 2 * HALF_PERIOD_IDLE_US;
        let whole_periods_before_wrap = u32::MAX / full_period;
        let remainder = u32::MAX - whole_periods_before_wrap * full_period;

        assert_ne!(
            remainder, 0,
            "a clean divisor would mean no glitch at all; this test exists because there is one"
        );
        assert!(remainder < full_period);

        assert!(lamp_is_lit(0, STATE_IDLE));
        assert_eq!(
            lamp_is_lit(whole_periods_before_wrap * full_period, STATE_IDLE),
            lamp_is_lit(0, STATE_IDLE),
            "every period before the wrap starts in phase"
        );
    }
}
