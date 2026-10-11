//! Exercise the real writer loop with a fake HID endpoint and USB callbacks.
//! Kept in one test: the USB channels and embassy clock are process globals.

use core::cell::{Cell, RefCell};
use core::future::{Future, poll_fn};
use core::pin::{Pin, pin};
use core::task::{Context, Poll, Waker};
use std::rc::Rc;
use std::vec::Vec;

use embassy_time::{Duration, MockDriver};
use embassy_usb::Handler;

use super::*;
use crate::hid::MouseReport;

#[derive(Clone)]
struct FakeWriter {
    sent: Rc<RefCell<Vec<Report>>>,
    ready: Rc<Cell<bool>>,
    disabled_once: Rc<Cell<bool>>,
}

impl FakeWriter {
    fn new() -> Self {
        Self {
            sent: Rc::new(RefCell::new(Vec::new())),
            ready: Rc::new(Cell::new(true)),
            disabled_once: Rc::new(Cell::new(false)),
        }
    }
}

impl HidWriterTrait for FakeWriter {
    type ReportType = Report;

    async fn write_report(&mut self, report: &Report) -> Result<usize, HidError> {
        if self.disabled_once.replace(false) {
            return Err(HidError::UsbEndpointError(EndpointError::Disabled));
        }
        poll_fn(|_| {
            if self.ready.get() {
                self.sent.borrow_mut().push(report.clone());
                Poll::Ready(Ok(1))
            } else {
                Poll::Pending
            }
        })
        .await
    }
}

fn poll_pending(future: Pin<&mut impl Future>) {
    assert!(future.poll(&mut Context::from_waker(Waker::noop())).is_pending());
}

fn key(code: u8) -> Report {
    Report::KeyboardReport(KeyboardReport {
        keycodes: [code, 0, 0, 0, 0, 0],
        ..Default::default()
    })
}

fn assert_key(report: &Report, code: u8) {
    let Report::KeyboardReport(report) = report else {
        panic!("expected keyboard report");
    };
    assert_eq!(report.keycodes, [code, 0, 0, 0, 0, 0]);
    assert_eq!(report.modifier, 0);
}

fn setup() -> UsbDeviceHandler {
    MockDriver::get().reset();
    USB_WRITER_EVENT.reset();
    USB_REMOTE_WAKEUP.reset();
    crate::test_support::reset_connection_status();
    let mut handler = UsbDeviceHandler::new();
    handler.enabled(true);
    handler.configured(true);
    USB_REPORT_CHANNEL.clear();
    handler
}

fn suspended_press_and_release_keep_order() {
    let mut handler = setup();
    handler.suspended(true);
    let mut writer = FakeWriter::new();
    let recorded = writer.sent.clone();
    let mut task = pin!(run_usb_writer(&mut writer));
    USB_REPORT_CHANNEL.try_send(key(4)).unwrap();
    USB_REPORT_CHANNEL.try_send(key(0)).unwrap();
    poll_pending(task.as_mut());
    assert!(recorded.borrow().is_empty());
    assert!(USB_REMOTE_WAKEUP.signaled());

    handler.suspended(false);
    poll_pending(task.as_mut());
    let reports = recorded.borrow();
    assert_eq!(reports.len(), 2);
    assert_key(&reports[0], 4);
    assert_key(&reports[1], 0);
}

fn first_mouse_delta_is_not_lost_or_duplicated() {
    let mut handler = setup();
    handler.suspended(true);
    let mut writer = FakeWriter::new();
    let recorded = writer.sent.clone();
    let mut task = pin!(run_usb_writer(&mut writer));
    USB_REPORT_CHANNEL
        .try_send(Report::MouseReport(MouseReport {
            x: 17,
            y: -4,
            ..Default::default()
        }))
        .unwrap();
    poll_pending(task.as_mut());
    handler.suspended(false);
    poll_pending(task.as_mut());
    let reports = recorded.borrow();
    assert_eq!(reports.len(), 1);
    let Report::MouseReport(report) = &reports[0] else {
        panic!("expected mouse report");
    };
    assert_eq!((report.x, report.y), (17, -4));
}

fn reconfiguration_discards_old_input(disconnect: bool, suspended: bool) {
    let mut handler = setup();
    if suspended {
        handler.suspended(true);
    }
    let mut writer = FakeWriter::new();
    writer.ready.set(false);
    let endpoint = writer.ready.clone();
    let recorded = writer.sent.clone();
    let mut task = pin!(run_usb_writer(&mut writer));
    USB_REPORT_CHANNEL.try_send(key(4)).unwrap();
    USB_REPORT_CHANNEL.try_send(key(5)).unwrap();
    poll_pending(task.as_mut());

    // All callbacks happen before the writer is polled: a latest-state-only
    // check would see Configured again and incorrectly send the retained key.
    if disconnect {
        handler.enabled(false);
        handler.enabled(true);
    } else {
        handler.reset();
    }
    handler.configured(true);
    endpoint.set(true);
    USB_REPORT_CHANNEL.try_send(key(6)).unwrap();
    poll_pending(task.as_mut());
    let reports = recorded.borrow();
    assert_eq!(reports.len(), 1);
    assert_key(&reports[0], 6);
}

fn endpoint_disabled_before_suspend_callback_retries_same_report() {
    let mut handler = setup();
    let mut writer = FakeWriter::new();
    writer.disabled_once.set(true);
    let recorded = writer.sent.clone();
    let mut task = pin!(run_usb_writer(&mut writer));
    USB_REPORT_CHANNEL.try_send(key(4)).unwrap();
    poll_pending(task.as_mut());
    assert!(recorded.borrow().is_empty());
    handler.suspended(true);
    handler.suspended(false);
    poll_pending(task.as_mut());
    let reports = recorded.borrow();
    assert_eq!(reports.len(), 1);
    assert_key(&reports[0], 4);
}

fn failed_wakeup_drains_input_and_releases_held_controls_on_resume() {
    let mut handler = setup();
    let mut writer = FakeWriter::new();
    let recorded = writer.sent.clone();
    let mut task = pin!(run_usb_writer(&mut writer));
    USB_REPORT_CHANNEL.try_send(key(4)).unwrap();
    poll_pending(task.as_mut());
    assert_eq!(recorded.borrow().len(), 1);
    handler.suspended(true);
    USB_REPORT_CHANNEL.try_send(key(0)).unwrap();
    poll_pending(task.as_mut());

    MockDriver::get().advance(USB_REPORT_WAIT + Duration::from_millis(1));
    poll_pending(task.as_mut());
    // Much more than a full queue: an unresponsive host must not block input
    // processing. Nothing from this period is replayed later.
    for _ in 0..crate::REPORT_CHANNEL_SIZE * 3 {
        USB_REPORT_CHANNEL.try_send(key(5)).unwrap();
        poll_pending(task.as_mut());
    }
    assert_eq!(recorded.borrow().len(), 1);
    handler.suspended(false);
    USB_REPORT_CHANNEL.try_send(key(6)).unwrap();
    poll_pending(task.as_mut());
    let reports = recorded.borrow();
    assert_eq!(reports.len(), 6);
    assert_key(&reports[1], 0);
    let Report::MouseReport(mouse) = &reports[2] else {
        panic!("expected mouse release");
    };
    assert_eq!(
        (mouse.buttons, mouse.x, mouse.y, mouse.wheel, mouse.pan),
        (0, 0, 0, 0, 0)
    );
    let Report::MediaKeyboardReport(media) = &reports[3] else {
        panic!("expected media release");
    };
    assert_eq!({ media.usage_id }, 0);
    let Report::SystemControlReport(system) = &reports[4] else {
        panic!("expected system release");
    };
    assert_eq!({ system.usage_id }, 0);
    assert_key(&reports[5], 6);
}

fn disconnect_after_wake_timeout_does_not_replay_or_stall() {
    let mut handler = setup();
    handler.suspended(true);
    let mut writer = FakeWriter::new();
    let recorded = writer.sent.clone();
    let mut task = pin!(run_usb_writer(&mut writer));
    USB_REPORT_CHANNEL.try_send(key(4)).unwrap();
    poll_pending(task.as_mut());
    MockDriver::get().advance(USB_REPORT_WAIT + Duration::from_millis(1));
    poll_pending(task.as_mut());
    handler.enabled(false);
    handler.enabled(true);
    handler.configured(true);
    USB_REPORT_CHANNEL.try_send(key(6)).unwrap();
    poll_pending(task.as_mut());
    let reports = recorded.borrow();
    assert_eq!(reports.len(), 1);
    assert_key(&reports[0], 6);
}

fn blocked_producer_drops_old_report_after_reconfiguration(disconnect: bool) {
    let mut handler = setup();
    for _ in 0..crate::REPORT_CHANNEL_SIZE {
        USB_REPORT_CHANNEL.try_send(key(4)).unwrap();
    }
    let mut producer = pin!(crate::channel::send_hid_report(key(5)));
    poll_pending(producer.as_mut());

    if disconnect {
        handler.enabled(false);
        handler.enabled(true);
    } else {
        handler.reset();
    }
    handler.configured(true);
    USB_REPORT_CHANNEL.try_send(key(6)).unwrap();
    assert!(
        producer
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_ready()
    );

    let mut writer = FakeWriter::new();
    let recorded = writer.sent.clone();
    let mut task = pin!(run_usb_writer(&mut writer));
    poll_pending(task.as_mut());
    let reports = recorded.borrow();
    assert_eq!(reports.len(), 1);
    assert_key(&reports[0], 6);
}

fn blocked_producer_survives_suspend_in_the_same_session() {
    let mut handler = setup();
    for _ in 0..crate::REPORT_CHANNEL_SIZE {
        USB_REPORT_CHANNEL.try_send(key(4)).unwrap();
    }
    let mut producer = pin!(crate::channel::send_hid_report(key(5)));
    poll_pending(producer.as_mut());
    handler.suspended(true);
    handler.suspended(false);
    let _ = USB_REPORT_CHANNEL.try_receive().unwrap();
    assert!(
        producer
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_ready()
    );
    for _ in 1..crate::REPORT_CHANNEL_SIZE {
        assert_key(&USB_REPORT_CHANNEL.try_receive().unwrap(), 4);
    }
    assert_key(&USB_REPORT_CHANNEL.try_receive().unwrap(), 5);
}

fn wake_timeout_also_invalidates_a_blocked_producer() {
    let mut handler = setup();
    handler.suspended(true);
    let mut writer = FakeWriter::new();
    let recorded = writer.sent.clone();
    let mut task = pin!(run_usb_writer(&mut writer));
    USB_REPORT_CHANNEL.try_send(key(4)).unwrap();
    poll_pending(task.as_mut());
    for _ in 0..crate::REPORT_CHANNEL_SIZE {
        USB_REPORT_CHANNEL.try_send(key(4)).unwrap();
    }
    let mut producer = pin!(crate::channel::send_hid_report(key(5)));
    poll_pending(producer.as_mut());
    MockDriver::get().advance(USB_REPORT_WAIT + Duration::from_millis(1));
    poll_pending(task.as_mut());
    // Resume before the previously blocked producer runs. The discarded input
    // must not return merely because the bus is still the same USB connection.
    handler.suspended(false);
    assert!(
        producer
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_ready()
    );
    poll_pending(task.as_mut());
    let reports = recorded.borrow();
    assert_eq!(reports.len(), 4);
    assert_key(&reports[0], 0);
}

#[cfg(feature = "_ble")]
fn output_switch_discards_retained_input_and_releases_old_host(already_pressed: bool, switch_back: bool) {
    use crate::channel::{BLE_REPORT_CHANNEL, send_hid_report};
    use crate::state::{set_ble_state, set_preferred_connection};
    use rmk_types::ble::BleState;

    let mut handler = setup();
    BLE_REPORT_CHANNEL.clear();
    set_ble_state(BleState::Connected);
    let mut writer = FakeWriter::new();
    let recorded = writer.sent.clone();
    let mut task = pin!(run_usb_writer(&mut writer));
    if already_pressed {
        for report in [
            key(4),
            Report::MouseReport(MouseReport {
                buttons: 1,
                ..Default::default()
            }),
            Report::MediaKeyboardReport(usbd_hid::descriptor::MediaKeyboardReport { usage_id: 0xb5 }),
            Report::SystemControlReport(usbd_hid::descriptor::SystemControlReport { usage_id: 0x81 }),
        ] {
            USB_REPORT_CHANNEL.try_send(report).unwrap();
            poll_pending(task.as_mut());
        }
    }
    let prior_count = recorded.borrow().len();
    handler.suspended(true);
    // Test both a not-yet-sent press and a release for controls already held.
    USB_REPORT_CHANNEL
        .try_send(Report::MouseReport(MouseReport {
            buttons: if already_pressed { 0 } else { 1 },
            x: 19,
            ..Default::default()
        }))
        .unwrap();
    poll_pending(task.as_mut());
    for _ in 0..crate::REPORT_CHANNEL_SIZE {
        USB_REPORT_CHANNEL.try_send(key(5)).unwrap();
    }
    let mut producer = pin!(send_hid_report(key(5)));
    poll_pending(producer.as_mut());
    set_preferred_connection(ConnectionType::Ble);
    poll_pending(task.as_mut());
    assert_eq!(recorded.borrow().len(), prior_count);

    let mut new_ble_input = pin!(send_hid_report(key(6)));
    assert!(
        new_ble_input
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_ready()
    );
    assert_key(&BLE_REPORT_CHANNEL.try_receive().unwrap(), 6);
    assert!(
        BLE_REPORT_CHANNEL.try_receive().is_err(),
        "old USB input must not move to BLE"
    );
    if switch_back {
        // Even an output round-trip before this blocked sender is polled must
        // not let its old USB input reappear in the current queue.
        set_preferred_connection(ConnectionType::Usb);
    }
    assert!(
        producer
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_ready()
    );
    handler.suspended(false);
    poll_pending(task.as_mut());
    let reports = recorded.borrow();
    assert_eq!(reports.len(), prior_count + 4);
    assert_key(&reports[prior_count], 0);
    let Report::MouseReport(mouse) = &reports[prior_count + 1] else {
        panic!("expected old host mouse release");
    };
    assert_eq!(
        (mouse.buttons, mouse.x, mouse.y, mouse.wheel, mouse.pan),
        (0, 0, 0, 0, 0)
    );
    let Report::MediaKeyboardReport(media) = &reports[prior_count + 2] else {
        panic!("expected old host media release");
    };
    assert_eq!({ media.usage_id }, 0);
    let Report::SystemControlReport(system) = &reports[prior_count + 3] else {
        panic!("expected old host system release");
    };
    assert_eq!({ system.usage_id }, 0);
}

#[test]
fn usb_wake_regressions() {
    suspended_press_and_release_keep_order();
    first_mouse_delta_is_not_lost_or_duplicated();
    reconfiguration_discards_old_input(false, true);
    reconfiguration_discards_old_input(true, true);
    reconfiguration_discards_old_input(false, false);
    endpoint_disabled_before_suspend_callback_retries_same_report();
    failed_wakeup_drains_input_and_releases_held_controls_on_resume();
    disconnect_after_wake_timeout_does_not_replay_or_stall();
    blocked_producer_drops_old_report_after_reconfiguration(false);
    blocked_producer_drops_old_report_after_reconfiguration(true);
    blocked_producer_survives_suspend_in_the_same_session();
    wake_timeout_also_invalidates_a_blocked_producer();
    #[cfg(feature = "_ble")]
    {
        output_switch_discards_retained_input_and_releases_old_host(false, false);
        output_switch_discards_retained_input_and_releases_old_host(true, true);
    }
}
