//! Keep queued and in-flight HID input within one BLE host/output session.
use core::cell::Cell;

use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::Mutex, signal::Signal};
use embassy_time::{Duration, with_timeout};

use crate::RawMutex;
use crate::channel::BLE_REPORT_CHANNEL;
use crate::hid::{HidWriterTrait, KeyboardReport, MouseReport, Report};

static GENERATION: Mutex<RawMutex, Cell<u32>> = Mutex::new(Cell::new(0));
static RELEASE_PENDING: Mutex<RawMutex, Cell<bool>> = Mutex::new(Cell::new(false));
static CHANGED: Signal<RawMutex, ()> = Signal::new();

pub(crate) fn generation() -> u32 {
    GENERATION.lock(Cell::get)
}

fn invalidate() {
    GENERATION.lock(|v| v.set(v.get().wrapping_add(1)));
    BLE_REPORT_CHANNEL.clear();
    RELEASE_PENDING.lock(|v| v.set(false));
    CHANGED.signal(());
}

/// Dropping the connection future must also retire its input, including when a
/// profile change cancels it before the normal disconnection path runs.
pub(crate) struct Session;
impl Session {
    pub(crate) fn new() -> Self {
        invalidate();
        Self
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        invalidate();
    }
}

pub(crate) fn clear_and_release() {
    invalidate();
    RELEASE_PENDING.lock(|v| v.set(true));
    // A single wake marker works even with a report queue smaller than four.
    let _ = BLE_REPORT_CHANNEL.try_send(Report::KeyboardReport(KeyboardReport::default()));
}

async fn write<W: HidWriterTrait<ReportType = Report>>(writer: &mut W, report: &Report, epoch: u32) -> bool {
    let send = async {
        loop {
            if generation() != epoch {
                return false;
            }
            match select(CHANGED.wait(), writer.write_report(report)).await {
                Either::First(()) => continue,
                Either::Second(Ok(_)) => return true,
                Either::Second(Err(e)) => {
                    warn!("BLE HID report failed: {:?}", e);
                    return false;
                }
            }
        }
    };
    with_timeout(Duration::from_secs(5), send).await.unwrap_or(false)
}

pub(crate) async fn run<W: HidWriterTrait<ReportType = Report>>(writer: &mut W) -> ! {
    loop {
        let report = BLE_REPORT_CHANNEL.receive().await;
        let epoch = generation();
        if RELEASE_PENDING.lock(|v| v.replace(false)) {
            let releases = [
                Report::KeyboardReport(KeyboardReport::default()),
                Report::MouseReport(MouseReport::default()),
                Report::MediaKeyboardReport(usbd_hid::descriptor::MediaKeyboardReport { usage_id: 0 }),
                Report::SystemControlReport(usbd_hid::descriptor::SystemControlReport { usage_id: 0 }),
            ];
            for release in releases {
                if generation() != epoch {
                    break;
                }
                // An unsubscribed report type must not prevent the other types
                // from being released on this same connection.
                write(writer, &release, epoch).await;
            }
        } else {
            write(writer, &report, epoch).await;
        }
    }
}

#[cfg(test)]
mod tests;
