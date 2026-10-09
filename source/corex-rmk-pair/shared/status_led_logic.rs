//! Finite status indications. No animation or deadline survives its last pulse.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Off,
    Red,
    Green,
    Blue,
    Amber,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame(pub Color, pub Color);

pub const OFF: Frame = Frame(Color::Off, Color::Off);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    SplitDisconnected,
    SplitConnected,
    HostConnected,
    HostDisconnected,
    Profile(u8),
    LocalLow,
    PeerLow,
    Pairing,
}

impl Notice {
    fn pattern(self) -> (Frame, u8, u64, u8) {
        use Color::*;
        match self {
            Self::SplitDisconnected => (Frame(Off, Blue), 3, 160, 1),
            Self::SplitConnected => (Frame(Off, Green), 2, 160, 1),
            Self::HostConnected => (Frame(Off, Green), 1, 500, 1),
            Self::HostDisconnected => (Frame(Off, Amber), 2, 160, 1),
            // Display profile 0 as one pulse, never as no indication.
            Self::Profile(id) => (Frame(Off, Blue), id.min(4) + 1, 180, 2),
            Self::LocalLow => (Frame(Red, Off), 1, 350, 3),
            Self::PeerLow => (Frame(Red, Off), 2, 350, 3),
            Self::Pairing => (Frame(Blue, Blue), 3, 220, 4),
        }
    }
}

#[derive(Debug)]
pub struct Player {
    notice: Option<Notice>,
    battery_pending: [Option<Notice>; 2],
    start_ms: u64,
    sleeping: bool,
}

impl Player {
    pub const fn new() -> Self {
        Self {
            notice: None,
            battery_pending: [None; 2],
            start_ms: 0,
            sleeping: false,
        }
    }

    pub fn request(&mut self, notice: Notice, now_ms: u64) {
        self.expire(now_ms);
        if self.sleeping {
            return;
        }
        if let Some(current) = self.notice {
            if matches!(notice, Notice::LocalLow | Notice::PeerLow) && current.pattern().3 >= 3 {
                if current != notice && !self.battery_pending.contains(&Some(notice)) {
                    if let Some(slot) = self.battery_pending.iter_mut().find(|slot| slot.is_none()) {
                        *slot = Some(notice);
                    }
                }
                return;
            }
            if current.pattern().3 > notice.pattern().3 {
                return;
            }
        }
        self.notice = Some(notice);
        self.start_ms = now_ms;
    }

    pub fn set_sleeping(&mut self, sleeping: bool) {
        self.sleeping = sleeping;
        // Neither sleep entry nor wake replays a stale connection animation.
        self.notice = None;
        self.battery_pending = [None; 2];
    }

    pub fn cancel_pairing(&mut self) {
        if self.notice == Some(Notice::Pairing) {
            self.notice = None;
        }
    }

    fn expire(&mut self, now_ms: u64) {
        if let Some(notice) = self.notice {
            let (_, pulses, width, _) = notice.pattern();
            // Leave a visible dark gap between queued battery indications.
            let phases = u64::from(pulses) * 2 - u64::from(self.battery_pending[0].is_none());
            if now_ms.saturating_sub(self.start_ms) >= phases * width {
                self.notice = None;
            }
        }
        if self.notice.is_none() {
            self.notice = self.battery_pending[0];
            self.battery_pending[0] = self.battery_pending[1];
            self.battery_pending[1] = None;
            self.start_ms = now_ms;
        }
    }

    pub fn sample(&mut self, now_ms: u64) -> (Frame, Option<u64>) {
        self.expire(now_ms);
        let Some(notice) = self.notice else { return (OFF, None) };
        let (frame, _, width, _) = notice.pattern();
        let phase = now_ms.saturating_sub(self.start_ms) / width;
        let deadline = self.start_ms + (phase + 1) * width;
        (if phase % 2 == 0 { frame } else { OFF }, Some(deadline))
    }
}

/// Valid measurements only, with hysteresis and one-minute warning suppression.
#[derive(Debug)]
pub struct LowBattery {
    low: bool,
    last_warning_ms: Option<u64>,
}

impl LowBattery {
    pub const fn new() -> Self {
        Self {
            low: false,
            last_warning_ms: None,
        }
    }

    pub fn update(&mut self, level: Option<u8>, charging: bool, now_ms: u64) -> bool {
        if charging || level.is_none_or(|v| v > 100) {
            self.low = false;
            return false;
        }
        let level = level.unwrap();
        if level <= 20 {
            self.low = true;
        } else if level >= 25 {
            self.low = false;
        }
        if self.low
            && self
                .last_warning_ms
                .is_none_or(|last| now_ms.saturating_sub(last) >= 60_000)
        {
            self.last_warning_ms = Some(now_ms);
            true
        } else {
            false
        }
    }
}

/// Two GRB pixels plus a final always-low PWM sample. nRF PWM at 16 MHz,
/// COUNTERTOP=20: 0=375 ns high; 1=875 ns high. The final sample's end-delay
/// provides a reset low longer than 280 us, including newer WS2812 revisions.
pub fn ws2812_words(frame: Frame) -> [u16; 49] {
    let mut words = [0x8000; 49];
    for (pixel, color) in [frame.0, frame.1].into_iter().enumerate() {
        let grb = match color {
            Color::Off => [0, 0, 0],
            Color::Red => [0, 8, 0],
            Color::Green => [8, 0, 0],
            Color::Blue => [0, 0, 8],
            Color::Amber => [4, 8, 0],
        };
        for (byte, value) in grb.into_iter().enumerate() {
            for bit in 0..8 {
                words[pixel * 24 + byte * 8 + bit] = 0x8000 | if value & (0x80 >> bit) != 0 { 14 } else { 6 };
            }
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_pulses_are_bounded_and_leave_no_idle_deadline() {
        for profile in 0..5 {
            let mut player = Player::new();
            player.request(Notice::Profile(profile), 0);
            for pulse in 0..=u64::from(profile) {
                assert_ne!(player.sample(pulse * 360).0, OFF);
            }
            assert_eq!(player.sample((u64::from(profile) * 2 + 1) * 180), (OFF, None));
            assert_eq!(player.sample(60_000), (OFF, None));
        }
    }

    #[test]
    fn sleep_cancels_running_and_new_indications() {
        let mut player = Player::new();
        player.request(Notice::LocalLow, 0);
        player.set_sleeping(true);
        player.request(Notice::Pairing, 1);
        assert_eq!(player.sample(2), (OFF, None));
        player.set_sleeping(false);
        assert_eq!(player.sample(3), (OFF, None));
    }

    #[test]
    fn battery_priority_is_not_replaced_by_connection_churn() {
        let mut player = Player::new();
        player.request(Notice::PeerLow, 0);
        player.request(Notice::HostConnected, 10);
        assert_eq!(player.sample(700).0, Frame(Color::Red, Color::Off));
        assert_eq!(player.sample(1050), (OFF, None));
    }

    #[test]
    fn both_low_batteries_are_shown_after_pairing_without_an_idle_timer() {
        let mut player = Player::new();
        player.request(Notice::Pairing, 0);
        player.request(Notice::LocalLow, 10);
        player.request(Notice::PeerLow, 20);
        assert_eq!(player.sample(1100).0, OFF);
        assert_eq!(player.sample(1320).0, Frame(Color::Red, Color::Off));
        assert_eq!(player.sample(1670).0, OFF);
        assert_eq!(player.sample(2020).0, Frame(Color::Red, Color::Off));
        assert_eq!(player.sample(2720).0, Frame(Color::Red, Color::Off));
        assert_eq!(player.sample(3070), (OFF, None));
    }

    #[test]
    fn pairing_close_cancels_only_pairing_notice() {
        let mut player = Player::new();
        player.request(Notice::Pairing, 0);
        player.cancel_pairing();
        assert_eq!(player.sample(1), (OFF, None));
        player.request(Notice::LocalLow, 2);
        player.cancel_pairing();
        assert_ne!(player.sample(3).0, OFF);
    }

    #[test]
    fn unavailable_and_charging_are_not_low_battery() {
        let mut battery = LowBattery::new();
        assert!(!battery.update(None, false, 0));
        assert!(!battery.update(Some(255), false, 0));
        assert!(!battery.update(Some(5), true, 0));
        assert!(battery.update(Some(5), false, 0));
    }

    #[test]
    fn low_battery_is_throttled_and_has_hysteresis() {
        let mut battery = LowBattery::new();
        assert!(battery.update(Some(20), false, 0));
        assert!(!battery.update(Some(19), false, 59_999));
        assert!(battery.update(Some(22), false, 60_000));
        assert!(!battery.update(Some(25), false, 120_000));
        assert!(!battery.update(Some(22), false, 180_000));
        assert!(battery.update(Some(20), false, 180_000));
    }

    #[test]
    fn late_timer_does_not_extend_led_or_start_an_idle_ticker() {
        let mut player = Player::new();
        player.request(Notice::SplitDisconnected, 0);
        assert_eq!(player.sample(30_000), (OFF, None));
    }

    #[test]
    fn every_notice_finishes_with_led_off() {
        for notice in [
            Notice::SplitDisconnected,
            Notice::SplitConnected,
            Notice::HostConnected,
            Notice::HostDisconnected,
            Notice::Profile(255),
            Notice::LocalLow,
            Notice::PeerLow,
            Notice::Pairing,
        ] {
            let mut player = Player::new();
            player.request(notice, 10);
            assert_ne!(player.sample(10).0, OFF);
            assert_eq!(player.sample(3_000), (OFF, None));
        }
    }

    #[test]
    fn ws2812_uses_grb_msb_first_and_low_latch_sample() {
        let words = ws2812_words(Frame(Color::Red, Color::Green));
        assert_eq!(words.len(), 49);
        assert_eq!(words[8 + 4], 0x8000 | 14); // Red 8's bit3, after first G byte.
        assert_eq!(words[24 + 4], 0x8000 | 14); // Second pixel G=8.
        assert_eq!(words.iter().filter(|&&word| word == 0x8000 | 14).count(), 2);
        assert_eq!(words[48], 0x8000); // Low throughout the latch interval.
    }
}
