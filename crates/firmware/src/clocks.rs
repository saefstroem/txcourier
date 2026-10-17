use rp235x_hal as hal;

use hal::Watchdog;
use hal::clocks::ClocksManager;
use hal::fugit::{HertzU32, RateExtU32};
use hal::pac::{CLOCKS, PLL_SYS, PLL_USB, RESETS, XOSC};
use hal::pll::common_configs::PLL_USB_48MHZ;
use hal::pll::{PLLConfig, setup_pll_blocking};
use hal::xosc::setup_xosc_blocking;

use crate::result::Result;

// Waveshare RP2350-USB-A: the board crystal is 12 MHz.
pub const XTAL_HZ: u32 = 12_000_000;

// USB 2.0 section 7.1.11: full speed signals at 12 Mbit/s. Equal to XTAL_HZ only by
// coincidence.
pub const FULL_SPEED_BIT_HZ: u32 = 12_000_000;

// Ten system cycles per full-speed bit, the rate the PIO decoder program is written for.
pub const SYS_HZ: u32 = 120_000_000;

// The system PLL's VCO frequency: XTAL_HZ * FBDIV, divided by the post dividers to SYS_HZ.
const VCO_HZ: u32 = 1_200_000_000;

// RP2350 datasheet, PLL: two 3-bit post dividers, each 1 to 7. 1.2 GHz / 5 / 2 = 120 MHz.
// The larger runs first, at the VCO rate.
const POST_DIV1: u8 = 5;
const POST_DIV2: u8 = 2;

// The PLL feedback divider: VCO_HZ / XTAL_HZ = 100.
const FBDIV: u32 = VCO_HZ / XTAL_HZ;

// FBDIV is an integer, so VCO_HZ must be an exact multiple of the reference.
const _: () = assert!(
    VCO_HZ.is_multiple_of(XTAL_HZ),
    "VCO is not an exact multiple of the reference: FBDIV would silently truncate"
);

// SYS_HZ must equal XTAL_HZ * FBDIV / (POST_DIV1 * POST_DIV2).
const _: () = assert!(
    SYS_HZ == XTAL_HZ * FBDIV / (POST_DIV1 as u32 * POST_DIV2 as u32),
    "SYS_HZ does not match what this PLL configuration produces"
);

// The PIO decoder needs a system clock that is an exact multiple of the bit rate.
const _: () = assert!(
    SYS_HZ.is_multiple_of(FULL_SPEED_BIT_HZ),
    "PIO USB requires a system clock that is an exact multiple of 12 MHz"
);

// initializes the clocks, and starts them, after this
// our system clock is running
pub fn init(
    xosc_dev: XOSC,
    clocks_dev: CLOCKS,
    pll_sys_dev: PLL_SYS,
    pll_usb_dev: PLL_USB,
    resets: &mut RESETS,
    watchdog: &mut Watchdog,
) -> Result<ClocksManager> {
    // setup crystal
    let xosc = setup_xosc_blocking(xosc_dev, XTAL_HZ.Hz())?;

    // start tick generation at 12 cycles per microsecond, which is 1 tick per microsecond
    watchdog.enable_tick_generation((XTAL_HZ / 1_000_000) as u16);

    // create the blocks manager which takes ownership of the clocks PAC
    let mut clocks = ClocksManager::new(clocks_dev);


    // initialize the pll's for system and usb 
    let pll_sys = setup_pll_blocking(
        pll_sys_dev,
        xosc.operating_frequency(),
        PLLConfig {
            vco_freq: HertzU32::Hz(VCO_HZ),
            refdiv: 1,
            post_div1: POST_DIV1,
            post_div2: POST_DIV2,
        },
        &mut clocks,
        resets,
    )?;

    let pll_usb = setup_pll_blocking(
        pll_usb_dev,
        xosc.operating_frequency(),
        PLL_USB_48MHZ, // 1.2ghz / 5 = 240mhz / 5 = 48mhz
        &mut clocks,
        resets,
    )?;

    // initialize the system and usb clocks
    clocks.init_default(&xosc, &pll_sys, &pll_usb)?;

    Ok(clocks)
}
