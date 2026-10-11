//! Vial's physical-presence challenge: 50 observed 100 ms hold steps.

use core::cell::Cell;

use embassy_time::{Duration, Instant};

use crate::host::lock::HostLock;
use crate::keymap::KeyMap;

const HOLD_STEPS: u8 = 50;
const HOLD_STEP: Duration = Duration::from_millis(100);

pub(super) struct VialLock<'a> {
    gate: HostLock<'a>,
    remaining: Cell<u8>,
    last_step: Cell<Instant>,
}

impl<'a> VialLock<'a> {
    pub fn new(keys: &'a [(u8, u8)], keymap: &'a KeyMap<'a>, insecure: bool) -> Self {
        Self {
            gate: HostLock::new(keys, keymap, insecure, Duration::from_millis(500)),
            remaining: Cell::new(HOLD_STEPS),
            last_step: Cell::new(Instant::MIN),
        }
    }

    pub fn is_unlocked(&self) -> bool {
        self.gate.is_unlocked()
    }
    pub fn is_unlocking(&self) -> bool {
        self.gate.is_unlocking()
    }

    pub fn start(&self) {
        if !self.is_unlocked() {
            self.gate.unlocking();
            self.remaining.set(HOLD_STEPS);
            self.last_step.set(Instant::now());
        }
    }

    pub fn poll(&self) -> u8 {
        // A poll cannot start an attempt, nor revive an expired one. A host
        // flooding polls also cannot shorten the physical hold.
        if self.gate.is_unlocking() {
            self.gate.unlocking();
            if !self.gate.all_unlock_keys_held() {
                self.remaining.set(HOLD_STEPS);
                self.last_step.set(Instant::now());
            } else if self.last_step.get().elapsed() >= HOLD_STEP {
                self.last_step.set(Instant::now());
                let remaining = self.remaining.get().saturating_sub(1);
                self.remaining.set(remaining);
                if remaining == 0 {
                    self.gate.unlock();
                }
            }
        }
        self.remaining.get()
    }

    pub fn lock(&self) {
        self.gate.lock();
        self.remaining.set(HOLD_STEPS);
    }
}
