// Not fixed by any specification: every sample must read idle J, so more samples let fewer
// glitches through. The u8 counters cap it at 255.
pub const BUS_SAMPLES: u8 = 64;

#[cfg(test)]
mod tests {
    use super::BUS_SAMPLES;

    #[test]
    fn the_sample_count_is_a_power_of_two() {
        assert!(BUS_SAMPLES.is_power_of_two());
    }

    #[test]
    fn a_counter_that_sees_every_sample_still_fits_the_byte_reporting_it() {
        let mut high = 0u8;
        for _ in 0..BUS_SAMPLES {
            high = high.checked_add(1).expect("counts every sample");
        }
        assert_eq!(high, BUS_SAMPLES);
    }
}
