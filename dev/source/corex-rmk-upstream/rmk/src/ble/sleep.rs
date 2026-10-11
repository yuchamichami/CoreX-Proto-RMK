//! Keyboard-wide sleep management.
//!
//! One manager owns the keyboard's sleep state: it watches [`SLEEP_INPUT`],
//! latches each decision in [`SLEEPING_STATE`] for pollers like the battery
//! service, and publishes it as [`SleepStateEvent`] for everything else — the
//! display, and on split centrals the per-link connection-parameter followers
//! in `split::ble::central`.

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_futures::select::{Either, select};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};

use crate::SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS;
use crate::event::{SleepStateEvent, publish_event};

/// The latched sleep state.
/// - `true`: the keyboard is idle and sleeping
/// - `false`: the keyboard is awake
pub(crate) static SLEEPING_STATE: AtomicBool = AtomicBool::new(false);

// Updated by the keyboard's physical-switch bitmap. This includes layer keys
// and keys mapped to mouse buttons, which need not appear in the HID key list.
static KEYS_HELD: AtomicBool = AtomicBool::new(false);

/// Input to [`run_sleep_manager`], same encoding as [`SLEEPING_STATE`]:
/// - `true`: sleep now, without waiting out the idle timeout
/// - `false`: activity — wake up, or restart the idle timeout
static SLEEP_INPUT: Signal<crate::RawMutex, bool> = Signal::new();

/// Report keyboard activity: wake the keyboard up, or restart the idle timeout
/// when it's already awake.
pub(crate) fn report_activity() {
    SLEEP_INPUT.signal(false);
}

/// Report a physical key event together with the complete held-switch state.
pub(crate) fn report_keyboard_activity(any_pressed: bool) {
    KEYS_HELD.store(any_pressed, Ordering::Release);
    report_activity();
}

/// Ask the keyboard to sleep now instead of waiting out the idle timeout. Sent
/// when the host suspends us or when advertising times out.
pub(crate) fn request_sleep() {
    SLEEP_INPUT.signal(true);
}

/// The keyboard's one sleep manager.
///
/// Run by [`crate::ble::BleTransport`], the single always-present BLE task, so
/// the state can never get stuck: split or not, connected or not, inputs always
/// reach it. Disabled when the configured timeout
/// (`split_central_sleep_timeout_seconds`) is 0, the default.
pub(crate) async fn run_sleep_manager() {
    if SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS == 0 {
        info!("Sleep management disabled (timeout = 0)");
        core::future::pending::<()>().await;
        return;
    }

    info!(
        "Sleep manager started with {}s timeout",
        SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS
    );
    manage_sleep_state(Duration::from_secs(SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS.into())).await
}

/// The sleep state machine, separate from [`run_sleep_manager`] only so tests
/// can drive it with a short timeout (the configured one is 0 in test builds).
async fn manage_sleep_state(idle_timeout: Duration) -> ! {
    loop {
        // Awake: sleep once the keyboard has been idle for `idle_timeout`, or as
        // soon as something asks us to. The input is polled first so activity
        // racing the timeout wins the tie instead of causing a spurious sleep.
        loop {
            match select(SLEEP_INPUT.wait(), Timer::after(idle_timeout)).await {
                Either::First(true) | Either::Second(_) => {
                    if !KEYS_HELD.load(Ordering::Acquire) {
                        break;
                    }
                    debug!("Key still held, postponing sleep");
                }
                Either::First(false) => debug!("Activity detected, resetting sleep timeout"),
            }
        }
        info!("Entering sleep mode");
        SLEEPING_STATE.store(true, Ordering::Release);
        publish_event(SleepStateEvent::new(true));

        // Asleep: only activity wakes us; further sleep requests change nothing.
        while SLEEP_INPUT.wait().await {}

        info!("Waking up from sleep mode due to activity");
        SLEEPING_STATE.store(false, Ordering::Release);
        publish_event(SleepStateEvent::new(false));
    }
}

#[cfg(test)]
pub(crate) async fn run_sleep_manager_for_test(idle_timeout: Duration) -> ! {
    manage_sleep_state(idle_timeout).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_block_on as block_on;

    /// Run the state machine with a 1s idle timeout until `script` finishes.
    fn with_sleep_manager(script: impl core::future::Future<Output = ()>) {
        block_on(async {
            select(manage_sleep_state(Duration::from_secs(1)), script).await;
        });
    }

    fn sleeping() -> bool {
        SLEEPING_STATE.load(Ordering::Acquire)
    }

    #[test]
    fn sleeps_when_idle_and_wakes_on_activity() {
        with_sleep_manager(async {
            Timer::after_millis(900).await;
            assert!(!sleeping(), "still inside the idle timeout");

            Timer::after_millis(200).await;
            assert!(sleeping(), "idle timeout elapsed");

            report_activity();
            Timer::after_millis(10).await;
            assert!(!sleeping(), "activity wakes the keyboard");
        });
    }

    #[test]
    fn activity_restarts_the_idle_timeout() {
        with_sleep_manager(async {
            // Each report lands inside the timeout, so 1.8s of steady typing
            // must never reach it.
            for _ in 0..3 {
                Timer::after_millis(600).await;
                assert!(!sleeping(), "activity must restart the idle timeout");
                report_activity();
            }
        });
    }

    /// Ball motion is activity too. A trackball can go minutes without a key
    /// event, and treating only `KeyboardEvent` as activity put a board that is
    /// being actively moused to sleep mid-use.
    #[test]
    fn pointing_activity_restarts_the_idle_timeout() {
        use embassy_futures::select::select3;
        use rmk_types::action::KeyAction;

        use crate::config::{BehaviorConfig, PositionalConfig};
        use crate::core_traits::Runnable;
        use crate::event::{Axis, AxisEvent, AxisValType, PointingEvent, publish_event};
        use crate::input_device::pointing::{PointingProcessor, PointingProcessorConfig};
        use crate::keymap::{KeyMap, KeymapData};

        let roll = || PointingEvent {
            device_id: 0,
            axes: [
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::X,
                    value: 4,
                },
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::Y,
                    value: -3,
                },
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::Z,
                    value: 0,
                },
            ],
        };

        block_on(async {
            let mut behavior = BehaviorConfig::default();
            let positional: PositionalConfig<1, 1> = PositionalConfig::default();
            let mut data: KeymapData<1, 1, 1, 0> = KeymapData::new([[[KeyAction::No]]]);
            let keymap = KeyMap::new(&mut data, &mut behavior, &positional).await;
            let mut processor = PointingProcessor::new(&keymap, PointingProcessorConfig::default());

            let script = async {
                // Same shape as `activity_restarts_the_idle_timeout`, with the
                // ball as the only input: 1.8s of steady rolling, no key ever.
                for _ in 0..3 {
                    Timer::after_millis(600).await;
                    assert!(!sleeping(), "ball motion must restart the idle timeout");
                    publish_event(roll());
                    // Let the processor pick the event up before the next check.
                    Timer::after_millis(1).await;
                }
            };

            select3(manage_sleep_state(Duration::from_secs(1)), processor.run(), script).await;
        });
    }

    #[test]
    fn held_switches_prevent_idle_sleep_until_after_release() {
        use crate::event::KeyboardEvent;
        use crate::matrix::MatrixState;

        with_sleep_manager(async {
            let mut keys = MatrixState::new(2, 3);
            // Duplicate presses must not leave a phantom hold after key-up.
            for _ in 0..2 {
                keys.update(&KeyboardEvent::key(1, 2, true));
                report_keyboard_activity(keys.any_pressed());
            }
            Timer::after_millis(2200).await;
            assert!(!sleeping(), "a held Fn, modifier or mouse key is not idle");

            keys.update(&KeyboardEvent::key(1, 2, false));
            report_keyboard_activity(keys.any_pressed());
            Timer::after_millis(900).await;
            assert!(!sleeping(), "the full idle interval starts at key-up");
            Timer::after_millis(200).await;
            assert!(sleeping(), "one release clears the duplicate press too");
        });
    }

    #[test]
    fn sleep_request_does_not_suspend_a_held_switch() {
        with_sleep_manager(async {
            report_keyboard_activity(true);
            request_sleep();
            Timer::after_millis(10).await;
            assert!(!sleeping());
            report_keyboard_activity(false);
            Timer::after_millis(1100).await;
            assert!(sleeping());
        });
    }

    #[test]
    fn connected_sleep_preserves_the_first_key_report() {
        use rmk_types::ble::BleState;
        use rmk_types::keycode::HidKeyCode;

        use crate::config::{BehaviorConfig, PositionalConfig};
        use crate::event::KeyboardEvent;
        use crate::hid::Report;
        use crate::keyboard::Keyboard;
        use crate::keymap::{KeyMap, KeymapData};

        block_on(async {
            crate::test_support::reset_connection_status();
            crate::state::set_ble_state(BleState::Connected);
            crate::channel::BLE_REPORT_CHANNEL.clear();
            let mut behavior = BehaviorConfig::default();
            let positional: PositionalConfig<1, 1> = PositionalConfig::default();
            let mut data: KeymapData<1, 1, 1, 0> = KeymapData::new([[[crate::k!(A)]]]);
            let keymap = KeyMap::new(&mut data, &mut behavior, &positional).await;
            let mut keyboard = Keyboard::new(&keymap);
            let script = async {
                Timer::after_millis(1100).await;
                assert!(sleeping());
                assert_eq!(
                    crate::state::current_ble_status().state,
                    BleState::Connected
                );
                keyboard.process_inner(KeyboardEvent::key(0, 0, true)).await;
                keyboard
                    .process_inner(KeyboardEvent::key(0, 0, false))
                    .await;
                Timer::after_millis(1).await;
                assert!(!sleeping());
                let Report::KeyboardReport(down) =
                    crate::channel::BLE_REPORT_CHANNEL.try_receive().unwrap()
                else {
                    panic!("the first key-down must reach the existing BLE connection");
                };
                assert!(down.keycodes.contains(&(HidKeyCode::A as u8)));
                let Report::KeyboardReport(up) =
                    crate::channel::BLE_REPORT_CHANNEL.try_receive().unwrap()
                else {
                    panic!("the key-up must also reach the existing BLE connection");
                };
                assert_eq!(up.keycodes, [0; 6]);
                assert_eq!(
                    crate::state::current_ble_status().state,
                    BleState::Connected
                );
            };
            select(manage_sleep_state(Duration::from_secs(1)), script).await;
        });
    }

    #[test]
    fn sleep_request_skips_the_idle_timeout() {
        with_sleep_manager(async {
            request_sleep();
            Timer::after_millis(10).await;
            assert!(sleeping(), "a sleep request doesn't wait for the timeout");

            request_sleep();
            Timer::after_millis(10).await;
            assert!(sleeping(), "a second request while asleep changes nothing");

            report_activity();
            Timer::after_millis(10).await;
            assert!(!sleeping());
        });
    }
}
