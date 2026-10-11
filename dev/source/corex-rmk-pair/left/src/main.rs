#![no_std]
#![no_main]
#[path = "../../shared/status_led.rs"]
mod status_led;
use rmk::macros::rmk_peripheral;
#[rmk_peripheral(id = 0)]
mod keyboard {
    #[register_processor(poll)]
    fn status_leds() -> crate::status_led::StatusLeds {
        use embassy_nrf::gpio::{Level, Output, OutputDrive};
        use embassy_nrf::pwm::{Config, Prescaler, SequenceLoad, SequencePwm};
        // Verified stock Cornix left RGB: P0.13 power, P0.24 data, two GRB pixels.
        // PWM0 generates the waveform with EasyDMA, independently of BLE IRQs.
        let mut config = Config::default();
        config.prescaler = Prescaler::Div1;
        config.max_duty = 20;
        config.sequence_load = SequenceLoad::Common;
        config.ch0_idle_level = Level::Low;
        crate::status_led::StatusLeds::new(
            crate::status_led::Lights::Left {
                power: Output::new(p.P0_13, Level::Low, OutputDrive::Standard),
                pwm: SequencePwm::new_1ch(p.PWM0, p.P0_24, config).unwrap(),
            },
            false,
        )
    }
}
