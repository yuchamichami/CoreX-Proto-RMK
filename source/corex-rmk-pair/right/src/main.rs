#![no_std]
#![no_main]
mod paw3222;
mod paw_wire;
mod pointing_mode;
mod tuning;
mod tuning_values;
use rmk::macros::rmk_central;
// PAW3222-only pointing build; J3 TrackPoint is not initialized.
#[rmk_central]
mod keyboard {
    #[register_processor(poll)]
    fn paw() -> crate::paw3222::Paw3222<'static> {
        use embassy_nrf::gpio::{Flex, Input, Level, Output, OutputDrive, Pull};
        crate::paw3222::Paw3222::new(
            &keymap,
            crate::paw3222::GpioWire {
                sck: Output::new(p.P1_02, Level::High, OutputDrive::Standard),
                sdio: Flex::new(p.P1_06),
            },
            Input::new(p.P1_04, Pull::Up),
            Output::new(p.P1_01, Level::Low, OutputDrive::Standard),
            Output::new(p.P0_07, Level::Low, OutputDrive::Standard),
            Output::new(p.P1_09, Level::Low, OutputDrive::Standard),
            Output::new(p.P0_31, Level::Low, OutputDrive::Standard),
            Input::new(p.P0_21, Pull::Up),
        )
    }
    #[register_processor(event)]
    fn motion_output() -> rmk::input_device::pointing::PointingProcessor<'static> {
        rmk::input_device::pointing::PointingProcessor::new(
            &keymap,
            rmk::input_device::pointing::PointingProcessorConfig {
                device_id: 0,
                ..Default::default()
            },
        )
    }
    #[register_processor(poll)]
    fn pointing_mode() -> crate::pointing_mode::PointingModeController<'static> {
        crate::pointing_mode::PointingModeController::new(&keymap)
    }
}
