use super::*;
use core::{cell::{Cell, RefCell}, future::{Future, poll_fn}, pin::{Pin, pin}, task::{Context, Poll, Waker}};
use std::{rc::Rc, vec::Vec};
use crate::hid::HidError;

struct FakeWriter { ready: Rc<Cell<bool>>, sent: Rc<RefCell<Vec<Report>>> }
impl HidWriterTrait for FakeWriter {
    type ReportType = Report;
    async fn write_report(&mut self, r: &Report) -> Result<usize, HidError> {
        poll_fn(|_| if self.ready.get() {
            self.sent.borrow_mut().push(r.clone()); Poll::Ready(Ok(1))
        } else { Poll::Pending }).await
    }
}
fn pending(f: Pin<&mut impl Future>) { assert!(f.poll(&mut Context::from_waker(Waker::noop())).is_pending()); }
fn key(code: u8) -> Report { Report::KeyboardReport(KeyboardReport { keycodes: [code,0,0,0,0,0], ..Default::default() }) }

#[test]
fn switching_output_cancels_blocked_input_and_releases_every_control() {
    embassy_time::MockDriver::get().reset();
    let _session = Session::new();
    let ready = Rc::new(Cell::new(false));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let mut writer = FakeWriter { ready: ready.clone(), sent: sent.clone() };
    let mut task = pin!(run(&mut writer));
    BLE_REPORT_CHANNEL.try_send(key(4)).unwrap();
    pending(task.as_mut());
    clear_and_release();
    ready.set(true);
    pending(task.as_mut());
    let reports = sent.borrow();
    assert_eq!(reports.len(), 4);
    assert!(matches!(&reports[0], Report::KeyboardReport(k) if k.keycodes == [0;6] && k.modifier == 0));
    assert!(matches!(&reports[1], Report::MouseReport(m) if m.buttons == 0 && m.x == 0 && m.y == 0));
    assert!(matches!(&reports[2], Report::MediaKeyboardReport(m) if m.usage_id == 0));
    assert!(matches!(&reports[3], Report::SystemControlReport(s) if s.usage_id == 0));
}

#[test]
fn a_new_host_never_gets_old_input_or_release_markers() {
    embassy_time::MockDriver::get().reset();
    let old = Session::new();
    BLE_REPORT_CHANNEL.try_send(key(4)).unwrap();
    clear_and_release();
    drop(old);
    let _new = Session::new();
    assert!(BLE_REPORT_CHANNEL.is_empty());
    let sent = Rc::new(RefCell::new(Vec::new()));
    let mut writer = FakeWriter { ready: Rc::new(Cell::new(true)), sent: sent.clone() };
    let mut task = pin!(run(&mut writer));
    BLE_REPORT_CHANNEL.try_send(key(5)).unwrap();
    pending(task.as_mut());
    assert_eq!(sent.borrow().len(), 1);
    assert!(matches!(&sent.borrow()[0], Report::KeyboardReport(k) if k.keycodes[0] == 5));
}

#[test]
fn blocked_producer_cannot_cross_a_ble_session() {
    embassy_time::MockDriver::get().reset();
    crate::test_support::reset_connection_status();
    crate::state::set_ble_state(rmk_types::ble::BleState::Connected);
    let old = Session::new();
    for _ in 0..crate::REPORT_CHANNEL_SIZE { BLE_REPORT_CHANNEL.try_send(key(4)).unwrap(); }
    let mut producer = pin!(crate::channel::send_hid_report(key(5)));
    pending(producer.as_mut());
    drop(old);
    let _new = Session::new();
    assert!(producer.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_ready());
    assert!(BLE_REPORT_CHANNEL.is_empty());
}


#[test]
fn an_unsubscribed_keyboard_report_does_not_skip_mouse_release() {
    struct SelectiveWriter { sent: Vec<Report> }
    impl HidWriterTrait for SelectiveWriter {
        type ReportType = Report;
        async fn write_report(&mut self, r: &Report) -> Result<usize, HidError> {
            if matches!(r, Report::KeyboardReport(_)) { return Err(HidError::BleError); }
            self.sent.push(r.clone()); Ok(1)
        }
    }
    embassy_time::MockDriver::get().reset();
    let _session = Session::new();
    let mut writer = SelectiveWriter { sent: Vec::new() };
    clear_and_release();
    {
        let mut task = pin!(run(&mut writer));
        pending(task.as_mut());
    }
    assert_eq!(writer.sent.len(), 3);
    assert!(matches!(&writer.sent[0], Report::MouseReport(m) if m.buttons == 0));
}

#[test]
fn a_blocked_notify_has_a_deadline_and_does_not_stall_new_input() {
    embassy_time::MockDriver::get().reset();
    let _session = Session::new();
    let ready = Rc::new(Cell::new(false));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let mut writer = FakeWriter { ready: ready.clone(), sent: sent.clone() };
    let mut task = pin!(run(&mut writer));
    BLE_REPORT_CHANNEL.try_send(key(4)).unwrap();
    pending(task.as_mut());
    embassy_time::MockDriver::get().advance(Duration::from_secs(6));
    pending(task.as_mut());
    ready.set(true);
    BLE_REPORT_CHANNEL.try_send(key(5)).unwrap();
    pending(task.as_mut());
    assert_eq!(sent.borrow().len(), 1);
    assert!(matches!(&sent.borrow()[0], Report::KeyboardReport(k) if k.keycodes[0] == 5));
}
