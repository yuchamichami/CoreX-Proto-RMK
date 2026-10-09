use crate::paw_wire::{PawWire, Scale, Wire};
use embassy_nrf::gpio::{Flex, Input, Output, OutputDrive, Pull};
use embassy_time::{Instant, Timer};
use rmk::event::{
    Axis, AxisEvent, AxisValType, LayerChangeEvent, PointingEvent, SleepStateEvent, publish_event,
};
use rmk::macros::processor;

pub struct GpioWire {
    pub sck: Output<'static>,
    pub sdio: Flex<'static>,
}
impl Wire for GpioWire {
    fn clock(&mut self, h: bool) {
        if h {
            self.sck.set_high()
        } else {
            self.sck.set_low()
        }
    }
    fn output(&mut self, yes: bool) {
        if yes {
            self.sdio.set_as_output(OutputDrive::Standard)
        } else {
            self.sdio.set_as_input(Pull::None)
        }
    }
    fn data(&mut self, h: bool) {
        if h {
            self.sdio.set_high()
        } else {
            self.sdio.set_low()
        }
    }
    fn sample(&mut self) -> bool {
        self.sdio.is_high()
    }
    fn delay_us(&mut self, us: u32) {
        cortex_m::asm::delay(us * 64);
    }
}

#[processor(subscribe = [SleepStateEvent, LayerChangeEvent], poll_interval = 15)]
pub struct Paw3222<'a> {
    keymap: &'a rmk::keymap::KeyMap<'a>,
    tuning: crate::tuning::Tuning,
    bus: PawWire<GpioWire>,
    motion: Input<'static>,
    _aux: Output<'static>,
    _led1: Output<'static>,
    _led2: Output<'static>,
    _battery_enable: Output<'static>,
    _j3_reset: Input<'static>,
    ready: bool,
    retry_at: Instant,
    attempts: u8,
    scale: Scale,
    scrolling: bool,
    polls: u32,
    last_check: Instant,
}
impl<'a> Paw3222<'a> {
    pub fn new(
        keymap: &'a rmk::keymap::KeyMap<'a>,
        bus: GpioWire,
        motion: Input<'static>,
        aux: Output<'static>,
        led1: Output<'static>,
        led2: Output<'static>,
        battery_enable: Output<'static>,
        j3_reset: Input<'static>,
    ) -> Self {
        Self {
            keymap,
            tuning: crate::tuning::Tuning::default(),
            bus: PawWire(bus),
            motion,
            _aux: aux,
            _led1: led1,
            _led2: led2,
            _battery_enable: battery_enable,
            _j3_reset: j3_reset,
            ready: false,
            retry_at: Instant::now() + embassy_time::Duration::from_millis(500),
            attempts: 0,
            scale: Scale::default(),
            scrolling: false,
            polls: 0,
            last_check: Instant::MIN,
        }
    }
    async fn on_layer_change_event(&mut self, e: LayerChangeEvent) {
        let scroll = e.0 == 3;
        if scroll != self.scrolling {
            self.scale.reset();
            self.scrolling = scroll;
        }
    }
    async fn on_sleep_state_event(&mut self, _: SleepStateEvent) {
        // Sensor's own sleep1/2 settings are retained, as in the verified ZMK build.
        // No shared 3V3 power switching is performed.
    }
    async fn poll(&mut self) {
        let tuning = crate::tuning::read(self.keymap);
        if tuning != self.tuning {
            self.scale.reset();
            self.tuning = tuning;
            log::info!("PAW tuning cursor={}/5 scroll=1/{} (both axes)",tuning.cursor,tuning.scroll);
        }
        if !self.ready {
            if Instant::now() < self.retry_at {
                return;
            }
            let id = self.bus.read(0x00);
            if id != 0x30 {
                self.attempts = self.attempts.saturating_add(1);
                if self.attempts == 1 || self.attempts == 10 {
                    log::warn!("PAW3222 J4 ID={id:02x}, expected 30; no motion output");
                }
                self.retry_at = Instant::now()
                    + embassy_time::Duration::from_millis(if self.attempts < 10 {
                        100
                    } else {
                        2000
                    });
                return;
            }
            let cfg = self.bus.read(0x06);
            self.bus.write(0x06, cfg | 0x80);
            Timer::after_millis(2).await;
            self.bus.write(0x09, 0x5a);
            let op = self.bus.read(0x05);
            self.bus.write(0x05, op | 0x18);
            self.bus.write(0x09, 0x00);
            for reg in [0x02, 0x03, 0x04, 0x12] {
                let _ = self.bus.read(reg);
            }
            let check = self.bus.read(0x00);
            if check != 0x30 {
                self.retry_at = Instant::now() + embassy_time::Duration::from_millis(100);
                return;
            }
            self.ready = true;
            log::info!("CoreX RMK PAW3222 J4 ready: ID=30, 15ms poll, SDIO released for reads");
            return;
        }
        // Level check avoids a lost falling edge; sparse fallback also checks a stuck HIGH IRQ.
        if self.motion.is_high() && self.last_check.elapsed().as_millis() < 500 {
            return;
        }
        self.last_check = Instant::now();
        let status = self.bus.read(0x02);
        if status == 0xff && self.bus.read(0x00) != 0x30 {
            self.ready = false;
            self.scale.reset();
            self.attempts = 0;
            self.retry_at = Instant::now() + embassy_time::Duration::from_millis(100);
            log::warn!("PAW3222 lost valid ID; motion suppressed until reinitialization");
            return;
        }
        if status & 0x80 == 0 {
            return;
        }
        let dx = (self.bus.read(0x03) as i8) as i16;
        let dy = (self.bus.read(0x04) as i8) as i16;
        if dx == 0 && dy == 0 {
            return;
        }
        let (x, y) = if self.scrolling {
            (dx, dy)
        } else {
            self.scale.cursor_with_gain(dx, dy, self.tuning.cursor, self.tuning.cursor)
        };
        if x == 0 && y == 0 {
            return;
        }
        publish_event(PointingEvent {
            device_id: 0,
            axes: [
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::X,
                    value: x,
                },
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::Y,
                    value: y,
                },
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::Z,
                    value: 0,
                },
            ],
        });
        self.polls += 1;
        // Sparse summary: no per-frame logging on the input hot path.
        if self.polls % 128 == 1 {
            log::info!("PAW3222 motion reports={}, last raw={dx},{dy}", self.polls);
        }
    }
}
