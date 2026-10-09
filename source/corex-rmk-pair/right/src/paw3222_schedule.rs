//! PAW3222 read deadlines, independent of the GPIO and async runtime.
//!
//! Times are monotonic clock ticks. The driver supplies the clock rate, checks
//! the current IRQ *level*, and waits for LOW or the returned health deadline.
//! Cancelling a wait for a keyboard event must not restart either deadline.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Wait {
    /// Pacing applies even when IRQ stays LOW or the previous read was empty.
    Until(u64),
    /// Arm a LOW-level GPIO wait, raced against this absolute deadline.
    MotionOrHealth { deadline: u64 },
    Read,
}

pub struct MotionSchedule {
    active_interval: u64,
    awake_health_interval: u64,
    asleep_health_interval: u64,
    last_sample: u64,
    read_after: u64,
}

impl MotionSchedule {
    pub const fn new(ticks_per_second: u64) -> Self {
        assert!(ticks_per_second > 0);
        Self {
            // Round up so a clock such as 32768 Hz never polls faster than 15 ms.
            active_interval: ticks_per_second.saturating_mul(15).saturating_add(999) / 1000,
            awake_health_interval: ticks_per_second.saturating_mul(5),
            asleep_health_interval: ticks_per_second.saturating_mul(30),
            last_sample: 0,
            read_after: 0,
        }
    }

    /// Initialization has finished. The first pending movement can be read now.
    pub fn on_ready(&mut self, now: u64) {
        self.last_sample = now;
        self.read_after = now;
    }

    /// Call when a status/motion read starts, including empty or failed reads.
    pub fn on_sample(&mut self, now: u64) {
        self.last_sample = now;
        self.read_after = now.saturating_add(self.active_interval);
    }

    pub fn wait(&self, now: u64, sleeping: bool, motion_low: bool) -> Wait {
        if now < self.read_after {
            return Wait::Until(self.read_after);
        }

        let health_interval = if sleeping {
            self.asleep_health_interval
        } else {
            self.awake_health_interval
        };
        let health = self.last_sample.saturating_add(health_interval);
        if motion_low || now >= health {
            Wait::Read
        } else {
            Wait::MotionOrHealth { deadline: health }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A 1 kHz clock makes the scripted times below milliseconds.
    fn ready_at(now: u64) -> MotionSchedule {
        let mut schedule = MotionSchedule::new(1000);
        schedule.on_ready(now);
        schedule
    }

    #[test]
    fn first_low_level_is_read_without_waiting_for_a_new_edge() {
        let schedule = ready_at(800);
        assert_eq!(schedule.wait(800, false, true), Wait::Read);
        assert_eq!(schedule.wait(800, true, true), Wait::Read);
    }

    #[test]
    fn motion_after_idle_does_not_wait_for_the_health_timer() {
        let mut schedule = ready_at(800);
        schedule.on_sample(800);
        assert_eq!(
            schedule.wait(2000, true, false),
            Wait::MotionOrHealth { deadline: 30800 }
        );
        // LOW arrived after the driver had armed its level wait.
        assert_eq!(schedule.wait(2001, true, true), Wait::Read);
    }

    #[test]
    fn held_low_is_limited_to_one_read_each_15ms_even_for_empty_reads() {
        let mut schedule = ready_at(0);
        let mut reads = Vec::new();
        for now in 0..=150 {
            match schedule.wait(now, false, true) {
                Wait::Read => {
                    reads.push(now);
                    // A status read that reports no movement must be paced too.
                    schedule.on_sample(now);
                }
                Wait::Until(deadline) => assert!(deadline > now),
                Wait::MotionOrHealth { .. } => panic!("IRQ is already LOW"),
            }
        }
        assert_eq!(reads, (0..=150).step_by(15).collect::<Vec<_>>());
    }

    #[test]
    fn low_during_pacing_is_still_read_at_the_next_slot() {
        let mut schedule = ready_at(0);
        schedule.on_sample(100);
        assert_eq!(schedule.wait(101, false, false), Wait::Until(115));
        assert_eq!(schedule.wait(102, false, true), Wait::Until(115));
        assert_eq!(schedule.wait(115, false, true), Wait::Read);
    }

    #[test]
    fn high_irq_has_five_second_awake_and_thirty_second_asleep_fallbacks() {
        for (sleeping, interval) in [(false, 5000), (true, 30000)] {
            let mut schedule = ready_at(100);
            for read_at in [100 + interval, 100 + interval * 2] {
                assert_eq!(
                    schedule.wait(read_at - 1, sleeping, false),
                    Wait::MotionOrHealth { deadline: read_at }
                );
                assert_eq!(schedule.wait(read_at, sleeping, false), Wait::Read);
                schedule.on_sample(read_at);
            }
        }
    }

    #[test]
    fn unrelated_events_do_not_postpone_a_health_read() {
        let mut schedule = ready_at(0);
        schedule.on_sample(100);
        // Cancelling and re-arming the async GPIO wait must not create a fresh
        // five-second deadline each time a layer or sleep event is handled.
        for now in (115..5100).step_by(7) {
            assert_eq!(
                schedule.wait(now, false, false),
                Wait::MotionOrHealth { deadline: 5100 }
            );
        }
        assert_eq!(schedule.wait(5100, false, false), Wait::Read);
    }

    #[test]
    fn wake_recomputes_health_deadline_without_delaying_first_motion() {
        let mut schedule = ready_at(100);
        schedule.on_sample(100);
        assert_eq!(
            schedule.wait(10000, true, false),
            Wait::MotionOrHealth { deadline: 30100 }
        );
        assert_eq!(schedule.wait(10000, false, false), Wait::Read);
        assert_eq!(schedule.wait(10000, true, true), Wait::Read);
        assert_eq!(schedule.wait(10000, false, true), Wait::Read);
    }

    #[test]
    fn sleep_changes_preserve_active_pacing() {
        let mut schedule = ready_at(0);
        schedule.on_sample(100);
        for sleeping in [false, true, false] {
            assert_eq!(schedule.wait(101, sleeping, true), Wait::Until(115));
        }
        assert_eq!(schedule.wait(115, false, true), Wait::Read);
    }

    #[test]
    fn pace_uses_clock_ticks_without_rounding_below_15ms() {
        let mut schedule = MotionSchedule::new(32768);
        schedule.on_ready(1234);
        assert_eq!(schedule.wait(1234, false, true), Wait::Read);
        schedule.on_sample(1234);
        assert_eq!(schedule.wait(1234 + 491, false, true), Wait::Until(1234 + 492));
        assert_eq!(schedule.wait(1234 + 492, false, true), Wait::Read);
        assert_eq!(
            schedule.wait(1234 + 492, false, false),
            Wait::MotionOrHealth { deadline: 1234 + 32768 * 5 }
        );
    }
}
