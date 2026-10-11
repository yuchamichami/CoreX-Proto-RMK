//! Keep physical key state while the radio is disconnected. A fresh link gets
//! the currently held switches and, at most, one recent wake tap.

use core::{future::Future, pin::pin};

use embassy_futures::select::{Either3, select3};
use embassy_time::{Duration, Instant, Timer};

use crate::event::{KeyboardEvent, KeyboardEventPos, SubscribableEvent};
use crate::matrix::MatrixState;
use crate::split::SplitMessage;
use crate::split::driver::{SplitDriverError, SplitWriter};

const WAKE_TAP_AGE: Duration = Duration::from_secs(10);
const PAIR_HOLD: Duration = Duration::from_secs(5);

pub(crate) struct PeripheralInput {
    pub(crate) keys: <KeyboardEvent as SubscribableEvent>::Subscriber,
    held: MatrixState,
    rows: u8,
    cols: u8,
    wake: Option<(KeyboardEvent, Instant, bool)>,
    pair_since: Option<Instant>,
    suppress_pair_keys: bool,
}

impl PeripheralInput {
    pub(crate) fn new(rows: u8, cols: u8) -> Self {
        Self {
            keys: KeyboardEvent::subscriber(),
            held: MatrixState::new(rows.into(), cols.into()),
            rows,
            cols,
            wake: None,
            pair_since: None,
            suppress_pair_keys: false,
        }
    }

    // The CoreX left's physical Tab and T switches. Unlike the unused seventh
    // column, both positions are populated on the production matrix.
    fn pair_key(event: KeyboardEvent) -> bool {
        matches!(event.pos, KeyboardEventPos::Key(pos) if pos.row == 0 && matches!(pos.col, 0 | 5))
    }

    pub(crate) fn record(&mut self, event: KeyboardEvent, disconnected: bool) -> bool {
        self.held.update(&event);
        if self.suppress_pair_keys && Self::pair_key(event) {
            if !self.held.read(0, 0) && !self.held.read(0, 5) {
                self.suppress_pair_keys = false;
            }
            return false;
        }
        if disconnected && matches!(event.pos, KeyboardEventPos::Key(_)) {
            if self
                .wake
                .is_some_and(|(_, at, _)| at.elapsed() > WAKE_TAP_AGE)
            {
                self.wake = None;
            }
            if let Some((press, _, released)) = self.wake.as_mut() {
                if event.pos == press.pos && !event.pressed {
                    *released = true;
                }
            } else if event.pressed {
                self.wake = Some((event, Instant::now(), false));
            }
        }
        true
    }

    fn update_pair_hold(&mut self) {
        let held = self.rows > 0 && self.cols > 5 && self.held.read(0, 0) && self.held.read(0, 5);
        if !held || self.suppress_pair_keys {
            self.pair_since = None;
        } else {
            self.pair_since.get_or_insert(Instant::now());
        }
    }

    fn fire_pair_hold(&mut self) {
        if self.pair_since.is_some_and(|at| at.elapsed() >= PAIR_HOLD) {
            self.pair_since = None;
            self.wake = None;
            self.suppress_pair_keys = true;
            super::request_pairing();
        }
    }

    /// Advertise/wait while still consuming matrix events. No disconnected
    /// subscriber backlog can block the matrix or lose the held-key snapshot.
    pub(crate) async fn while_disconnected<F: Future>(&mut self, operation: F) -> F::Output {
        let mut operation = pin!(operation);
        loop {
            self.update_pair_hold();
            let until_hold = self
                .pair_since
                .map(|at| (at + PAIR_HOLD).saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::from_secs(86_400));
            match select3(
                operation.as_mut(),
                self.keys.next_message_pure(),
                Timer::after(until_hold),
            )
            .await
            {
                Either3::First(result) => return result,
                Either3::Second(event) => {
                    self.record(event, true);
                }
                Either3::Third(_) => self.fire_pair_hold(),
            }
        }
    }

    pub(crate) async fn wait_for_activity(&mut self) {
        let event = self.keys.next_message_pure().await;
        self.record(event, true);
    }

    /// Called only after the central's subscription-ready status arrives.
    pub(crate) async fn synchronize<W: SplitWriter>(
        &mut self,
        writer: &mut W,
    ) -> Result<(), SplitDriverError> {
        while let Some(event) = self.keys.try_next_message_pure() {
            self.record(event, true);
        }
        self.pair_since = None;
        for row in 0..self.rows {
            for col in 0..self.cols {
                let event = KeyboardEvent::key(row, col, true);
                if self.held.read(row, col) && !(self.suppress_pair_keys && Self::pair_key(event)) {
                    writer.write(&SplitMessage::Key(event)).await?;
                }
            }
        }
        if let Some((press, at, true)) = self.wake.take()
            && at.elapsed() <= WAKE_TAP_AGE
            && let KeyboardEventPos::Key(pos) = press.pos
            && !self.held.read(pos.row, pos.col)
        {
            writer.write(&SplitMessage::Key(press)).await?;
            writer
                .write(&SplitMessage::Key(KeyboardEvent {
                    pressed: false,
                    ..press
                }))
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_block_on;

    #[derive(Default)]
    struct Writer(std::vec::Vec<KeyboardEvent>);
    impl SplitWriter for Writer {
        async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError> {
            if let SplitMessage::Key(event) = message {
                self.0.push(*event);
            }
            Ok(1)
        }
    }

    #[test]
    fn reconnect_restores_held_modifier_and_one_recent_wake_tap() {
        test_block_on(async {
            let mut input = PeripheralInput::new(4, 7);
            input.record(KeyboardEvent::key(2, 0, true), false);
            input.record(KeyboardEvent::key(1, 1, true), true);
            input.record(KeyboardEvent::key(1, 1, false), true);
            input.record(KeyboardEvent::key(1, 2, true), true);
            input.record(KeyboardEvent::key(1, 2, false), true);
            let mut writer = Writer::default();
            input.synchronize(&mut writer).await.unwrap();
            assert_eq!(
                writer.0,
                [
                    KeyboardEvent::key(2, 0, true),
                    KeyboardEvent::key(1, 1, true),
                    KeyboardEvent::key(1, 1, false)
                ]
            );
            writer.0.clear();
            input.synchronize(&mut writer).await.unwrap();
            assert_eq!(
                writer.0,
                [KeyboardEvent::key(2, 0, true)],
                "wake tap is replayed only once"
            );
        });
    }

    #[test]
    fn stale_wake_tap_is_not_typed_after_a_long_disconnect() {
        test_block_on(async {
            let mut input = PeripheralInput::new(4, 7);
            input.record(KeyboardEvent::key(1, 1, true), true);
            input.record(KeyboardEvent::key(1, 1, false), true);
            Timer::after(WAKE_TAP_AGE + Duration::from_millis(1)).await;
            let mut writer = Writer::default();
            input.synchronize(&mut writer).await.unwrap();
            assert!(writer.0.is_empty());
        });
    }

    #[test]
    fn recovery_gesture_is_bounded_and_not_typed_on_the_new_link() {
        test_block_on(async {
            super::super::PAIRING_REQUEST.reset();
            let mut input = PeripheralInput::new(4, 7);
            input.record(KeyboardEvent::key(0, 0, true), true);
            input.record(KeyboardEvent::key(0, 5, true), true);
            input
                .while_disconnected(super::super::PAIRING_REQUEST.wait())
                .await;
            assert!(Instant::now().as_millis() >= 5000);
            let mut writer = Writer::default();
            input.synchronize(&mut writer).await.unwrap();
            assert!(writer.0.is_empty());
            assert!(!input.record(KeyboardEvent::key(0, 0, false), false));
            assert!(!input.record(KeyboardEvent::key(0, 5, false), false));
            assert!(input.record(KeyboardEvent::key(0, 0, true), false));
        });
    }
}
