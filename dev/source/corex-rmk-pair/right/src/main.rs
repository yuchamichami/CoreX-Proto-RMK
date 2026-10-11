#![no_std]
#![no_main]
mod paw3222;
mod paw3222_schedule;
mod paw_wire;
mod pointing_mode;
#[path = "../../shared/status_led.rs"]
mod status_led;
mod tuning;
mod tuning_values;
use rmk::macros::rmk_central;
// PAW3222-only pointing build; J3 TrackPoint is not initialized.
#[rmk_central]
mod keyboard {
    #[register_processor(poll)]
    fn status_leds() -> crate::status_led::StatusLeds {
        use embassy_nrf::gpio::{Level, Output, OutputDrive};
        crate::status_led::StatusLeds::new(
            crate::status_led::Lights::Right {
                first: Output::new(p.P0_07, Level::Low, OutputDrive::Standard),
                second: Output::new(p.P1_09, Level::Low, OutputDrive::Standard),
            },
            true,
        )
    }
    #[register_processor(poll)]
    async fn paw() -> crate::paw3222::Paw3222<'static> {
        use embassy_nrf::gpio::{Flex, Input, Level, Output, OutputDrive, Pull};
        // Q3 drives Q2 to enable VSENSE. R7/R8 and C10 need 350 ms to settle
        // before the generated battery ADC task takes its first sample.
        let battery_enable = Output::new(p.P0_31, Level::High, OutputDrive::Standard);
        embassy_time::Timer::after_millis(350).await;
        crate::paw3222::Paw3222::new(
            &keymap,
            crate::paw3222::GpioWire {
                sck: Output::new(p.P1_02, Level::High, OutputDrive::Standard),
                sdio: Flex::new(p.P1_06),
            },
            Input::new(p.P1_04, Pull::Up),
            Output::new(p.P1_01, Level::Low, OutputDrive::Standard),
            battery_enable,
            Input::new(p.P0_21, Pull::Up),
        )
    }
    #[register_processor(poll)]
    fn pointing_mode() -> crate::pointing_mode::PointingModeController<'static> {
        crate::pointing_mode::PointingModeController::new(&keymap)
    }
}
