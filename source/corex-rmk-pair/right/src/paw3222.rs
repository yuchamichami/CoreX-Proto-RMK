use crate::paw_wire::{PawWire, Wire};
use crate::paw3222_schedule::{ACTIVE_INTERVAL_MS, MotionSchedule, Wait};
use embassy_nrf::gpio::{Flex, Input, Output, OutputDrive, Pull};
use embassy_time::{Duration, Instant, Timer};
use rmk::embassy_futures::select::{Either, select};
use rmk::event::{
    Axis, AxisEvent, AxisValType, EventSubscriber, PointingEvent, SleepStateEvent, publish_event,
};
use rmk::macros::processor;
use rmk::processor::{PollingProcessor, Processor};

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

#[processor(subscribe = [SleepStateEvent])]
pub struct Paw3222<'a> {
    keymap: &'a rmk::keymap::KeyMap<'a>,
    bus: PawWire<GpioWire>,
    motion: Input<'static>,
    _aux: Output<'static>,
    _battery_enable: Output<'static>,
    _j3_reset: Input<'static>,
    ready: bool,
    retry_at: Instant,
    attempts: u8,
    polls: u32,
    sleeping: bool,
    schedule: MotionSchedule,
}
impl<'a> Paw3222<'a> {
    pub fn new(
        keymap: &'a rmk::keymap::KeyMap<'a>,
        bus: GpioWire,
        motion: Input<'static>,
        aux: Output<'static>,
        battery_enable: Output<'static>,
        j3_reset: Input<'static>,
    ) -> Self {
        Self {
            keymap,
            bus: PawWire(bus),
            motion,
            _aux: aux,
            _battery_enable: battery_enable,
            _j3_reset: j3_reset,
            ready: false,
            retry_at: Instant::now() + embassy_time::Duration::from_millis(500),
            attempts: 0,
            polls: 0,
            sleeping: false,
            schedule: MotionSchedule::new(embassy_time::TICK_HZ),
        }
    }
    async fn on_sleep_state_event(&mut self, event: SleepStateEvent) {
        self.sleeping = event.0;
        // Keep rest modes, power and motion detection alive. With NCS tied LOW,
        // forcing power-down can strand the sensor until its supply is cut.
        // A normal wake must NOT reset the sensor or drain its first movement.
        log::info!(
            "PAW3222 {}: motion IRQ armed, sensor power retained",
            if event.0 { "idle" } else { "awake" }
        );
    }
    async fn poll(&mut self) {
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
            self.schedule.on_ready(Instant::now().as_ticks());
            log::info!("CoreX RMK PAW3222 J4 ready: ID=30, motion IRQ, {ACTIVE_INTERVAL_MS}ms active interval");
            return;
        }
        self.schedule.on_sample(Instant::now().as_ticks());
        let status = self.bus.read(0x02);
        if status == 0xff && self.bus.read(0x00) != 0x30 {
            self.ready = false;
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
        // Publish raw counts. The output controller selects the mode and
        // applies gain once, so a queued layer change cannot reinterpret
        // already-scaled cursor data as scrolling (or the reverse).
        // Apply Vial's persisted AML setting before any motion subscriber sees
        // this sample. No 50ms settings timer is needed while the ball is idle.
        rmk::set_auto_mouse_layer_enabled(crate::tuning::aml_enabled(self.keymap));
        publish_event(PointingEvent {
            device_id: 0,
            axes: [
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::X,
                    value: dx,
                },
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::Y,
                    value: dy,
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

impl PollingProcessor for Paw3222<'_> {
    fn interval(&self) -> Duration {
        Duration::from_millis(ACTIVE_INTERVAL_MS)
    }

    async fn update(&mut self) {
        self.poll().await;
    }

    // register_processor(poll) calls this override. There is no fixed ticker:
    // idle is a GPIO level wait with a sparse communication-health deadline.
    async fn polling_loop(&mut self) -> ! {
        let mut events = Self::subscriber();
        loop {
            if !self.ready {
                match select(events.next_event(), Timer::at(self.retry_at)).await {
                    Either::First(event) => self.process(event).await,
                    Either::Second(_) => self.poll().await,
                }
                continue;
            }
            let pending = self.schedule.wait(
                Instant::now().as_ticks(),
                self.sleeping,
                self.motion.is_low(),
            );
            let wait = async {
                match pending {
                    Wait::Until(deadline) => {
                        Timer::at(Instant::from_ticks(deadline)).await;
                        false // Recheck IRQ after pacing; no need to read if HIGH.
                    }
                    Wait::MotionOrHealth { deadline } => {
                        let _ = select(
                            self.motion.wait_for_low(),
                            Timer::at(Instant::from_ticks(deadline)),
                        )
                        .await;
                        true
                    }
                    Wait::Read => true,
                }
            };
            match select(events.next_event(), wait).await {
                Either::First(event) => self.process(event).await,
                Either::Second(true) => self.poll().await,
                Either::Second(false) => {}
            }
        }
    }
}
