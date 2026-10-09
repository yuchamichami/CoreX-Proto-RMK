//! Exposed channels which can be used to share data across devices & processors

use core::future::poll_fn;

use embassy_sync::channel::{Channel, TrySendError};
#[cfg(feature = "_ble")]
use embassy_sync::signal::Signal;
pub use embassy_sync::{blocking_mutex, channel, pubsub, zerocopy_channel};
use rmk_types::connection::ConnectionType;
#[cfg(feature = "_ble")]
use {crate::ble::profile::BleProfileAction, rmk_types::led_indicator::LedIndicator};

#[cfg(all(feature = "vial", feature = "_ble"))]
use crate::VIAL_CHANNEL_SIZE;
use crate::hid::Report;
use crate::{REPORT_CHANNEL_SIZE, RawMutex};

type ReportChannel = Channel<RawMutex, Report, REPORT_CHANNEL_SIZE>;

/// Signal for LED indicator, used in BLE keyboards only since BLE receiving is not async
#[cfg(feature = "_ble")]
pub(crate) static LED_SIGNAL: Signal<RawMutex, LedIndicator> = Signal::new();

/// Drained by the USB HID writer task. Routed through `send_hid_report`
/// from the keyboard task and ad-hoc producers (e.g. steno chord output).
#[cfg(not(feature = "_no_usb"))]
pub static USB_REPORT_CHANNEL: ReportChannel = Channel::new();

/// Drained by the BLE HID writer task. Routed through `send_hid_report`.
#[cfg(feature = "_ble")]
pub static BLE_REPORT_CHANNEL: ReportChannel = Channel::new();

fn report_channel(transport: ConnectionType) -> Option<&'static ReportChannel> {
    match transport {
        #[cfg(not(feature = "_no_usb"))]
        ConnectionType::Usb => Some(&USB_REPORT_CHANNEL),
        #[cfg(feature = "_ble")]
        ConnectionType::Ble => Some(&BLE_REPORT_CHANNEL),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn active_report_channel() -> Option<(ConnectionType, &'static ReportChannel)> {
    let transport = crate::state::active_transport()?;
    report_channel(transport).map(|ch| (transport, ch))
}

/// Reports generated while no transport is selected are dropped on the floor.
pub(crate) async fn send_hid_report(mut report: Report) {
    let Some((transport, ch)) = active_report_channel() else {
        return;
    };
    #[cfg(not(feature = "_no_usb"))]
    let usb_session = (transport == ConnectionType::Usb).then(crate::usb::usb_session);

    #[cfg(feature = "_ble")]
    let ble_generation = (transport == ConnectionType::Ble).then(crate::ble::report_writer::generation);

    loop {
        match ch.try_send(report) {
            Ok(()) => return,
            Err(TrySendError::Full(r)) => report = r,
        }

        poll_fn(|cx| ch.poll_ready_to_send(cx)).await;
        if crate::state::active_transport() != Some(transport) {
            return;
        }
        #[cfg(feature = "_ble")]
        if ble_generation.is_some_and(|epoch| epoch != crate::ble::report_writer::generation()) {
            return;
        }
        // A reset/re-enumeration can return to USB before this blocked sender
        // is polled. A wake timeout also discards its retained old input.
        #[cfg(not(feature = "_no_usb"))]
        if usb_session.is_some_and(|session| session != crate::usb::usb_session()) {
            return;
        }
    }
}

/// Drops the report when the active transport's queue is full or no
/// transport is selected. Use for producers where back-pressure would block
/// the matrix scan (e.g. steno chord output).
pub(crate) fn try_send_hid_report(report: Report) {
    if let Some((_, ch)) = active_report_channel() {
        let _ = ch.try_send(report);
    }
}

/// Drains queued reports for the previous output and schedules releases.
/// Both writers cancel in-flight input and release all HID controls.
pub(crate) fn clear_and_release_report_channel(transport: ConnectionType) {
    #[cfg(not(feature = "_no_usb"))]
    if transport == ConnectionType::Usb {
        crate::usb::clear_and_release_usb_reports();
        return;
    }
    #[cfg(feature = "_ble")]
    if transport == ConnectionType::Ble {
        crate::ble::report_writer::clear_and_release();
    }
}

#[cfg(feature = "_ble")]
pub(crate) static BLE_PROFILE_CHANNEL: Channel<RawMutex, BleProfileAction, 1> = Channel::new();

/// Vial RX from BLE GATT `output_data` writes — one 32-byte chunk per write.
/// Pushed by `gatt_events_task`, drained by [`crate::ble::host::HostGattHandler::run`].
#[cfg(all(feature = "vial", feature = "_ble"))]
pub(crate) static VIAL_BLE_RX_CHANNEL: Channel<RawMutex, [u8; 32], VIAL_CHANNEL_SIZE> = Channel::new();

/// Rynk RX from the BLE `output_data` writes. The 512 B ring is ~2× one MTU's maximal payload.
#[cfg(all(feature = "rynk", feature = "_ble"))]
pub(crate) static RYNK_BLE_RX_PIPE: embassy_sync::pipe::Pipe<RawMutex, 512> = embassy_sync::pipe::Pipe::new();
