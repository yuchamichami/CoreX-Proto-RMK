pub mod central;
pub(crate) mod input;
pub mod peripheral;

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant};

use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};
use trouble_host::types::gatt_traits::AsGatt;

use super::SplitMessage;
use super::driver::SplitDriverError;

static PAIRING_REQUEST: Signal<crate::RawMutex, ()> = Signal::new();
static PAIRING_OPEN: AtomicBool = AtomicBool::new(false);
const PAIRING_DURATION: Duration = Duration::from_secs(60);

/// Forget this half's saved split peer and open a 60-second pairing window.
/// The always-running link lifecycle handles this even when the other half is absent.
pub fn request_pairing() {
    PAIRING_REQUEST.signal(());
}

/// Current split pairing state, also published as `SplitPairingEvent`.
pub fn pairing_window_open() -> bool {
    PAIRING_OPEN.load(Ordering::Acquire)
}

struct PairingWindow {
    deadline: Option<Instant>,
}

impl PairingWindow {
    fn new() -> Self {
        Self { deadline: None }
    }

    fn open(&mut self) {
        self.deadline = Some(Instant::now() + PAIRING_DURATION);
        self.publish(true);
    }

    fn close(&mut self) {
        self.deadline = None;
        self.publish(false);
    }

    fn remaining(&mut self) -> Option<Duration> {
        let deadline = self.deadline?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining == Duration::MIN {
            self.close();
            None
        } else {
            Some(remaining)
        }
    }

    fn publish(&self, open: bool) {
        if PAIRING_OPEN.swap(open, Ordering::AcqRel) != open {
            crate::event::publish_event(crate::event::SplitPairingEvent { open });
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, MaxSize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PeerAddress {
    pub peer_id: u8,
    pub is_valid: bool,
    pub address: [u8; 6],
}

impl PeerAddress {
    pub(crate) fn new(peer_id: u8, is_valid: bool, address: [u8; 6]) -> Self {
        Self {
            peer_id,
            is_valid,
            address,
        }
    }
}

#[derive(Default, Clone)]
pub(crate) struct GattSplitMessage {
    buf: [u8; SplitMessage::POSTCARD_MAX_SIZE],
    len: usize,
}

impl TryFrom<&SplitMessage> for GattSplitMessage {
    type Error = SplitDriverError;

    fn try_from(value: &SplitMessage) -> Result<Self, Self::Error> {
        let mut buf = [0; SplitMessage::POSTCARD_MAX_SIZE];
        let encoded = postcard::to_slice(value, &mut buf).map_err(|e| {
            error!("Postcard serialize split message error: {}", e);
            SplitDriverError::SerializeError
        })?;

        let len = encoded.len();

        // Check if slice starts at the beginning of buffer
        if encoded.as_ptr() != buf.as_ptr() {
            error!("Postcard serialize split message did not use the buffer correctly!");
            return Err(SplitDriverError::SerializeError);
        }

        Ok(Self { buf, len })
    }
}

impl AsGatt for GattSplitMessage {
    const MIN_SIZE: usize = 0;

    const MAX_SIZE: usize = SplitMessage::POSTCARD_MAX_SIZE;

    fn as_gatt(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventSubscriber, SplitPairingEvent, SubscribableEvent};
    use crate::test_support::test_block_on;

    #[test]
    fn pairing_window_closes_and_publishes_its_expiry() {
        test_block_on(async {
            PAIRING_OPEN.store(false, Ordering::Release);
            let mut events = SplitPairingEvent::subscriber();
            let mut window = PairingWindow::new();
            window.open();
            assert!(pairing_window_open());
            assert!(events.next_event().await.open);
            embassy_time::MockDriver::get().advance(PAIRING_DURATION);
            assert!(window.remaining().is_none());
            assert!(!pairing_window_open());
            assert!(!events.next_event().await.open);
            assert!(window.remaining().is_none());
            assert!(events.try_next_message_pure().is_none());
        });
    }
}
