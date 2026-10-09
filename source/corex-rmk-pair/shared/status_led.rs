//! Board status lights. All animations are brief; after the last pulse this
//! task waits only for an event. The stock left's RGB supply is then cut off.

#[path = "status_led_logic.rs"]
mod logic;

use embassy_nrf::gpio::{Level, Output};
use embassy_nrf::pwm::{SequenceConfig, SequencePwm, SingleSequenceMode, SingleSequencer};
use embassy_time::{Duration, Instant, Timer};
use rmk::embassy_futures::select::{Either, select};
use rmk::event::{
    BatteryStatusEvent, CentralConnectedEvent, ConnectionStatusChangeEvent, EventSubscriber, PeripheralBatteryEvent,
    PeripheralConnectedEvent, SleepStateEvent, SplitPairingEvent,
};
use rmk::macros::processor;
use rmk::processor::{PollingProcessor, Processor};
use rmk::types::battery::{BatteryStatus, ChargeState};
use rmk::types::connection::ConnectionType;

use logic::{Color, Frame, LowBattery, Notice, OFF, Player};

pub enum Lights {
    /// A13 D21/P0.07 and D22/P1.09 have GND cathodes: HIGH is on.
    Right {
        first: Output<'static>,
        second: Output<'static>,
    },
    /// Stock Cornix left: active-HIGH P0.13 supply; two GRB WS2812 on P0.24.
    Left {
        power: Output<'static>,
        pwm: SequencePwm<'static>,
    },
}

impl Lights {
    async fn set(&mut self, frame: Frame) {
        match self {
            Self::Right { first, second } => {
                first.set_level(if frame.0 == Color::Off { Level::Low } else { Level::High });
                second.set_level(if frame.1 == Color::Off { Level::Low } else { Level::High });
            }
            Self::Left { power, pwm } => {
                if frame == OFF {
                    // PWM is already stopped with the final zero-duty sample;
                    // keeping DIN LOW avoids feeding the unpowered RGB rail.
                    power.set_low();
                    return;
                }
                power.set_high();
                // Matches the stock Cornix ext-power startup delay.
                Timer::after_millis(50).await;
                let words = logic::ws2812_words(frame);
                let mut config = SequenceConfig::default();
                config.end_delay = 240; // 300 us latch low, after the 49th sample.
                let sequence = SingleSequencer::new(pwm, &words, config);
                if sequence.start(SingleSequenceMode::Times(1)).is_err() {
                    power.set_low();
                    log::warn!("Status RGB: PWM sequence rejected");
                    return;
                }
                // DMA clocks the 24-bit pixels; interrupts never affect bit
                // timing. Retain the RAM words until the bounded frame is over.
                Timer::after_millis(2).await;
                sequence.stop();
            }
        }
    }
}

#[processor(subscribe = [
    ConnectionStatusChangeEvent, PeripheralConnectedEvent, CentralConnectedEvent,
    BatteryStatusEvent, PeripheralBatteryEvent, SleepStateEvent, SplitPairingEvent
])]
pub struct StatusLeds {
    lights: Lights,
    player: Player,
    frame: Frame,
    central: bool,
    profile: Option<u8>,
    active_host: Option<ConnectionType>,
    split_connected: Option<bool>,
    local_battery: LowBattery,
    peer_battery: LowBattery,
    sleeping: bool,
}

fn now_ms() -> u64 {
    Instant::now().as_millis()
}

fn battery_measurement(status: BatteryStatus) -> (Option<u8>, bool) {
    match status {
        BatteryStatus::Available { level, charge_state } => (level, charge_state == ChargeState::Charging),
        BatteryStatus::Unavailable => (None, false),
    }
}

impl StatusLeds {
    pub fn new(lights: Lights, central: bool) -> Self {
        Self {
            lights,
            central,
            player: Player::new(),
            frame: OFF,
            profile: None,
            active_host: None,
            split_connected: None,
            local_battery: LowBattery::new(),
            peer_battery: LowBattery::new(),
            sleeping: false,
        }
    }

    fn notice(&mut self, notice: Notice) {
        self.player.request(notice, now_ms());
    }

    fn split_changed(&mut self, connected: bool) {
        if self.split_connected != Some(connected) {
            self.split_connected = Some(connected);
            if !connected {
                // Never replay a stale battery warning after the left disappears.
                self.peer_battery = LowBattery::new();
            }
            self.notice(if connected {
                Notice::SplitConnected
            } else {
                Notice::SplitDisconnected
            });
        }
    }

    async fn on_connection_status_change_event(&mut self, event: ConnectionStatusChangeEvent) {
        if !self.central {
            return; // The left's USB connector is not a host keyboard endpoint.
        }
        let next_profile = event.0.ble.profile;
        let next_host = event.0.decide_active();
        if self.profile.is_some_and(|old| old != next_profile) {
            self.notice(Notice::Profile(next_profile));
        } else if next_host != self.active_host {
            self.notice(if next_host.is_some() {
                Notice::HostConnected
            } else {
                Notice::HostDisconnected
            });
        }
        self.profile = Some(next_profile);
        self.active_host = next_host;
    }

    async fn on_peripheral_connected_event(&mut self, event: PeripheralConnectedEvent) {
        if self.central && event.id == 0 {
            self.split_changed(event.connected);
        }
    }

    async fn on_central_connected_event(&mut self, event: CentralConnectedEvent) {
        if !self.central {
            self.split_changed(event.connected);
        }
    }

    async fn on_battery_status_event(&mut self, event: BatteryStatusEvent) {
        let (level, charging) = battery_measurement(event.0);
        if !self.sleeping && self.local_battery.update(level, charging, now_ms()) {
            self.notice(Notice::LocalLow);
        }
    }

    async fn on_peripheral_battery_event(&mut self, event: PeripheralBatteryEvent) {
        if self.central && event.id == 0 && self.split_connected == Some(true) {
            let (level, charging) = battery_measurement(event.state.0);
            if !self.sleeping && self.peer_battery.update(level, charging, now_ms()) {
                self.notice(Notice::PeerLow);
            }
        }
    }

    async fn on_sleep_state_event(&mut self, event: SleepStateEvent) {
        self.sleeping = event.0;
        self.player.set_sleeping(event.0);
    }

    async fn on_split_pairing_event(&mut self, event: SplitPairingEvent) {
        if event.open {
            self.notice(Notice::Pairing);
        } else {
            self.player.cancel_pairing();
        }
    }
}

impl PollingProcessor for StatusLeds {
    // Only needed by the trait. polling_loop below never starts a ticker.
    fn interval(&self) -> Duration {
        Duration::from_secs(86_400)
    }
    async fn update(&mut self) {}

    async fn polling_loop(&mut self) -> ! {
        let mut events = Self::subscriber();
        log::info!(
            "CoreX {} v{} status LEDs ready",
            if self.central { "Right" } else { "Left" },
            env!("CARGO_PKG_VERSION")
        );
        // Pairing may open before this processor has subscribed. Sample once at
        // startup; all subsequent changes arrive as local events.
        self.notice(if rmk::split::ble::pairing_window_open() {
            Notice::Pairing
        } else {
            Notice::SplitDisconnected
        });
        loop {
            let (frame, deadline) = self.player.sample(now_ms());
            if frame != self.frame {
                self.lights.set(frame).await;
                self.frame = frame;
            }
            match deadline {
                Some(deadline) => match select(events.next_event(), Timer::at(Instant::from_millis(deadline))).await {
                    Either::First(event) => self.process(event).await,
                    Either::Second(_) => {}
                },
                None => self.process(events.next_event().await).await,
            }
        }
    }
}
