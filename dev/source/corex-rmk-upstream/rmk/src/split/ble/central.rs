use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "subrating")]
use bt_hci::cmd::le::LeSubrateRequest;
use bt_hci::cmd::le::{LeReadLocalSupportedFeatures, LeSetPhy, LeSetScanParams};
use bt_hci::controller::{ControllerCmdAsync, ControllerCmdSync};
use embassy_futures::select::{Either, Either3, select, select3};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use trouble_host::prelude::*;

use super::GattSplitMessage;
use crate::ble::adv::Adv;
use crate::ble::scan::{SPLIT_CENTRAL_SCAN_WINDOW, scan_config, start_scan};
use crate::ble::sleep::report_activity;
use crate::ble::{update_ble_phy, update_conn_params, wait_for_stack_started};
use crate::event::{
    EventSubscriber, KeyboardEvent, SleepStateEvent, SubscribableEvent, publish_event_async,
};
use crate::matrix::MatrixState;
use crate::split::ble::PeerAddress;
use crate::split::driver::{
    PeripheralManager, SplitDriverError, SplitReader, SplitWriter, set_peripheral_connected,
};
use crate::split::{PeripheralMatrixConfig, SPLIT_MESSAGE_MAX_SIZE, SplitMessage};
use crate::storage::{StorageItem, StorageKey, StorageValue, read, store};

static PERIPHERAL_FOUND: Signal<crate::RawMutex, (u8, BdAddr)> = Signal::new();

/// One peripheral link's lifecycle, owned by [`scan_and_connect_peripherals`].
#[derive(Debug, PartialEq, Eq)]
enum SlotState {
    /// Unknown address: discover the peripheral by scanning.
    NoAddr,
    /// Known address without a live session: connect the peripheral.
    Disconnected([u8; 6]),
    /// The link is up and handed to the slot's session task.
    Connected([u8; 6]),
    /// Discovered only during an explicit pairing window, not yet persisted.
    Candidate([u8; 6]),
    /// Local address is cleared; the old session is releasing and disconnecting.
    Clearing,
}

impl SlotState {
    fn connect_timed_out(&mut self) {
        // A radio timeout is never permission to replace an established pair.
        if matches!(self, Self::Candidate(_)) {
            *self = Self::NoAddr;
        }
    }

    fn clear_peer(&mut self) -> bool {
        let live = matches!(self, Self::Connected(_) | Self::Clearing);
        *self = if live { Self::Clearing } else { Self::NoAddr };
        live
    }

    fn session_ended(&mut self) {
        *self = match *self {
            Self::Connected(addr) => Self::Disconnected(addr),
            Self::Clearing => Self::NoAddr,
            _ => return,
        };
    }

    fn restore_saved_peer(&mut self, peer: &PeerAddress) -> bool {
        if peer.is_valid && matches!(self, Self::NoAddr) {
            *self = Self::Disconnected(peer.address);
            true
        } else {
            false
        }
    }

    fn adopt_discovered_peer(&mut self, address: [u8; 6], sleeping: bool, pairing: bool) -> bool {
        if pairing && !sleeping && matches!(self, Self::NoAddr) {
            *self = Self::Candidate(address);
            true
        } else {
            false
        }
    }
}

// A sleeping central must still hear its known half wake and advertise. Two
// seconds of the usual 30ms/100ms scan, then four seconds without scanning, bounds
// the retry duty cycle without opening discovery to another keyboard set.
const SLEEP_RECONNECT_WINDOW: Duration = Duration::from_secs(2);
const SLEEP_RECONNECT_PAUSE: Duration = Duration::from_secs(4);

// The split service and its two characteristics, declared by `#[gatt_service]`
// in `split::ble::peripheral` and discovered by UUID here.
const SPLIT_SERVICE_UUID: u128 = 0x4dd5fbaa_18e5_4b07_bf0a_353698659946;
const MESSAGE_TO_CENTRAL_UUID: u128 = 0x0e6313e3_bd0b_45c2_8d2e_37a2e8128bc3;
const MESSAGE_TO_PERIPHERAL_UUID: u128 = 0x4b3514fb_cae4_4d38_a097_3a2a3d1c3b9c;
#[cfg(feature = "custom_message")]
const CUSTOM_TO_CENTRAL_UUID: u128 = 0x5f2a7c14_9b3e_4a51_8d76_2c1e4b8a6f03;
#[cfg(feature = "custom_message")]
const CUSTOM_TO_PERIPHERAL_UUID: u128 = 0x5f2a7c15_9b3e_4a51_8d76_2c1e4b8a6f03;

/// Scan for peripheral addresses, connect them, and hand each connection to
/// that slot's session; sessions report back on `ended`.
pub(crate) async fn scan_and_connect_peripherals<
    'a,
    C: Controller + ControllerCmdSync<LeSetScanParams>,
>(
    stack: &'a Stack<'_, C, DefaultPacketPool>,
    conns: &[Channel<NoopRawMutex, Connection<'a, DefaultPacketPool>, 1>;
         crate::SPLIT_PERIPHERALS_NUM],
    ended: &Channel<NoopRawMutex, usize, { crate::SPLIT_PERIPHERALS_NUM }>,
) {
    let mut peripheral_slots: [SlotState; crate::SPLIT_PERIPHERALS_NUM] =
        core::array::from_fn(|_| SlotState::NoAddr);
    restore_saved_peers(&mut peripheral_slots).await;
    let mut pairing = super::PairingWindow::new();
    if peripheral_slots
        .iter()
        .any(|slot| matches!(slot, SlotState::NoAddr))
    {
        pairing.open();
    }
    let mut reset_requested = false;
    let mut central = stack.central();
    wait_for_stack_started().await;
    loop {
        while let Ok(id) = ended.try_receive() {
            peripheral_slots[id].session_ended();
            clear_link_reset(id);
        }
        reset_requested |= super::PAIRING_REQUEST.try_take().is_some();
        if reset_requested {
            reset_requested = false;
            PERIPHERAL_FOUND.reset();
            if clear_saved_peers(&mut peripheral_slots).await {
                report_activity();
                pairing.open();
            }
        }
        let pairing_remaining = pairing.remaining();
        if pairing_remaining.is_none() {
            for slot in &mut peripheral_slots {
                if matches!(slot, SlotState::Candidate(_)) {
                    *slot = SlotState::NoAddr;
                }
            }
        }
        let mut pending: heapless::Vec<(usize, [u8; 6]), { crate::SPLIT_PERIPHERALS_NUM }> =
            heapless::Vec::new();
        for (id, slot) in peripheral_slots.iter().enumerate() {
            if let SlotState::Disconnected(addr) | SlotState::Candidate(addr) = slot {
                let _ = pending.push((id, *addr));
            }
        }
        if !pending.is_empty() {
            let targets: heapless::Vec<Address, { crate::SPLIT_PERIPHERALS_NUM }> = pending
                .iter()
                .map(|(_, addr)| Address::random(*addr))
                .collect();
            let config = ConnectConfig {
                connect_params: default_split_conn_params(),
                scan_config: ScanConfig {
                    filter_accept_list: &targets,
                    ..scan_config(SPLIT_CENTRAL_SCAN_WINDOW)
                },
            };
            let timeout = if crate::state::current_sleep_state() {
                SLEEP_RECONNECT_WINDOW
            } else {
                Duration::from_secs(15)
            };
            let timeout = pairing_remaining
                .map(|remaining| remaining.min(timeout))
                .unwrap_or(timeout);
            let result = select(
                super::PAIRING_REQUEST.wait(),
                with_timeout(timeout, central.connect(&config)),
            )
            .await;
            let connected = match result {
                Either::First(()) => {
                    reset_requested = true;
                    continue;
                }
                Either::Second(Ok(Ok(conn))) => {
                    let peer = conn.peer_address();
                    if let Some(&(id, addr)) = pending
                        .iter()
                        .find(|(_, addr)| Address::random(*addr) == peer)
                    {
                        if matches!(peripheral_slots[id], SlotState::Candidate(_)) {
                            if pairing.remaining().is_none()
                                || store(StorageItem::PeerAddress(PeerAddress::new(
                                    id as u8, true, addr,
                                )))
                                .await
                                .is_err()
                            {
                                peripheral_slots[id] = SlotState::NoAddr;
                                drop(conn);
                                continue;
                            }
                        }
                        peripheral_slots[id] = SlotState::Connected(addr);
                        conns[id].send(conn).await;
                    }
                    true
                }
                Either::Second(Ok(Err(e))) => {
                    #[cfg(feature = "defmt")]
                    let e = defmt::Debug2Format(&e);
                    error!("Connect error: {:?}", e);
                    Timer::after_millis(500).await;
                    false
                }
                Either::Second(Err(_)) => {
                    for &(id, _) in &pending {
                        peripheral_slots[id].connect_timed_out();
                    }
                    false
                }
            };
            if !connected
                && let Either::First(()) = select(
                    super::PAIRING_REQUEST.wait(),
                    wait_before_sleep_retry(ended),
                )
                .await
            {
                reset_requested = true;
            }
        } else if peripheral_slots
            .iter()
            .all(|s| matches!(s, SlotState::Connected(_)))
        {
            pairing.close();
            if let Either::First(()) =
                select(super::PAIRING_REQUEST.wait(), ended.ready_to_receive()).await
            {
                reset_requested = true;
            }
        } else if let Some(remaining) = pairing_remaining
            && !crate::state::current_sleep_state()
            && peripheral_slots
                .iter()
                .any(|slot| matches!(slot, SlotState::NoAddr))
        {
            PERIPHERAL_FOUND.reset();
            let session = start_scan(stack, SPLIT_CENTRAL_SCAN_WINDOW, &[]).await;
            let event = select(
                super::PAIRING_REQUEST.wait(),
                with_timeout(
                    remaining.min(Duration::from_secs(30)),
                    select3(
                        PERIPHERAL_FOUND.wait(),
                        ended.ready_to_receive(),
                        wait_until_sleep(),
                    ),
                ),
            )
            .await;
            session.stop().await;
            match event {
                Either::First(()) => reset_requested = true,
                Either::Second(Ok(Either3::First((id, addr)))) => {
                    if let Some(slot) = peripheral_slots.get_mut(id as usize) {
                        slot.adopt_discovered_peer(
                            addr.into_inner(),
                            crate::state::current_sleep_state(),
                            pairing.remaining().is_some(),
                        );
                    }
                }
                _ => (),
            }
        } else {
            // An unpaired half with an expired window stays closed. Explicit
            // reset still works here, and while a previous link is shutting down.
            if let Either3::First(()) = select3(
                super::PAIRING_REQUEST.wait(),
                ended.ready_to_receive(),
                async {
                    if let Some(remaining) = pairing_remaining {
                        Timer::after(remaining.min(Duration::from_secs(1))).await;
                    } else {
                        core::future::pending::<()>().await;
                    }
                },
            )
            .await
            {
                reset_requested = true;
            }
        }
    }
}

static LINK_RESET: [AtomicBool; crate::SPLIT_PERIPHERALS_NUM] =
    [const { AtomicBool::new(false) }; crate::SPLIT_PERIPHERALS_NUM];
static LINK_RESET_WAKE: [Signal<crate::RawMutex, ()>; crate::SPLIT_PERIPHERALS_NUM] =
    [const { Signal::new() }; crate::SPLIT_PERIPHERALS_NUM];
static LINK_RESET_DEADLINE: [Signal<crate::RawMutex, ()>; crate::SPLIT_PERIPHERALS_NUM] =
    [const { Signal::new() }; crate::SPLIT_PERIPHERALS_NUM];

fn request_link_reset(id: usize) {
    LINK_RESET[id].store(true, Ordering::Release);
    LINK_RESET_WAKE[id].signal(());
    LINK_RESET_DEADLINE[id].signal(());
}

fn clear_link_reset(id: usize) {
    LINK_RESET[id].store(false, Ordering::Release);
    LINK_RESET_WAKE[id].reset();
    LINK_RESET_DEADLINE[id].reset();
}

pub(crate) async fn wait_for_link_reset(id: usize) {
    if !link_reset_requested(id) {
        LINK_RESET_WAKE[id].wait().await;
    }
}

pub(crate) fn link_reset_requested(id: usize) -> bool {
    LINK_RESET[id].load(Ordering::Acquire)
}

// If discovery or a GATT write is stuck, reset must still terminate the old
// session. The manager gets two seconds to durably clear the remote peer first.
async fn reset_disconnect_deadline(id: usize) {
    if !link_reset_requested(id) {
        LINK_RESET_DEADLINE[id].wait().await;
    }
    Timer::after_secs(2).await;
}

async fn clear_saved_peers(slots: &mut [SlotState]) -> bool {
    let mut cleared = false;
    for (id, slot) in slots.iter_mut().enumerate() {
        if store(StorageItem::PeerAddress(PeerAddress::new(
            id as u8, false, [0; 6],
        )))
        .await
        .is_ok()
        {
            if slot.clear_peer() {
                request_link_reset(id);
            }
            cleared = true;
        } else {
            error!("Cannot clear split peer {}", id);
        }
    }
    cleared
}

async fn restore_saved_peers(slots: &mut [SlotState]) -> bool {
    let mut restored = false;
    for (id, slot) in slots.iter_mut().enumerate() {
        if matches!(slot, SlotState::NoAddr)
            && let Ok(Some(StorageValue::PeerAddress(peer))) =
                read(StorageKey::PeerAddress(id as u8)).await
        {
            restored |= slot.restore_saved_peer(&peer);
        }
    }
    restored
}

// Avoid another permanent subscriber: only discovery needs to watch sleep,
// and its one-second check stops an already-running scan promptly.
async fn wait_until_sleep() {
    while !crate::state::current_sleep_state() {
        Timer::after_secs(1).await;
    }
}

async fn wait_before_sleep_retry(
    ended: &Channel<NoopRawMutex, usize, { crate::SPLIT_PERIPHERALS_NUM }>,
) {
    let deadline = Instant::now() + SLEEP_RECONNECT_PAUSE;
    while crate::state::current_sleep_state() && Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if with_timeout(
            remaining.min(Duration::from_secs(1)),
            ended.ready_to_receive(),
        )
        .await
        .is_ok()
        {
            break;
        }
    }
}

// When no peripheral address is saved, the central should first scan for peripheral.
// This handler is used to handle the scan result.
pub(crate) struct ScanHandler;

impl EventHandler for ScanHandler {
    fn on_adv_reports(&self, mut it: LeAdvReportsIter<'_>) {
        while let Some(Ok(report)) = it.next() {
            let Some(Adv::SplitPeripheral { id }) = Adv::decode(report.data) else {
                continue;
            };
            info!(
                "Found split peripheral: id={:?}, addr={:?}",
                id, report.addr
            );
            PERIPHERAL_FOUND.signal((id, report.addr));
            break;
        }
    }
}

/// Serve one peripheral slot: take each link established by
/// [`scan_and_connect_peripherals`], run the split session over it until it ends, and
/// report back so the radio reconnects or rediscovers the peripheral.
pub(crate) async fn run_peripheral_session<
    'a,
    #[cfg(not(feature = "subrating"))] C: Controller
        + ControllerCmdSync<LeSetScanParams>
        + ControllerCmdAsync<LeSetPhy>
        + ControllerCmdSync<LeReadLocalSupportedFeatures>,
    #[cfg(feature = "subrating")] C: Controller
        + ControllerCmdSync<LeSetScanParams>
        + ControllerCmdAsync<LeSetPhy>
        + ControllerCmdSync<LeReadLocalSupportedFeatures>
        + ControllerCmdAsync<LeSubrateRequest>,
>(
    id: usize,
    conns: &Channel<NoopRawMutex, Connection<'a, DefaultPacketPool>, 1>,
    ended: &Channel<NoopRawMutex, usize, { crate::SPLIT_PERIPHERALS_NUM }>,
    stack: &'a Stack<'_, C, DefaultPacketPool>,
    matrix_config: PeripheralMatrixConfig,
) {
    trace!("SPLIT_MESSAGE_MAX_SIZE: {}", SPLIT_MESSAGE_MAX_SIZE);
    loop {
        let conn = conns.receive().await;
        set_peripheral_connected(id, true);
        let pressed_keys = RefCell::new(MatrixState::new(
            matrix_config.rows.into(),
            matrix_config.cols.into(),
        ));
        if let Either::First(Err(e)) = select(
            run_central_manager_task(id, stack, &conn, matrix_config, &pressed_keys),
            reset_disconnect_deadline(id),
        )
        .await
        {
            #[cfg(feature = "defmt")]
            let e = defmt::Debug2Format(&e);
            error!("BLE central error: {:?}", e);
        }
        set_peripheral_connected(id, false);
        release_disconnected_keys(&pressed_keys, matrix_config).await;
        // Dropping the last handle files a disconnect request that the stack
        // runner serves on its own; the pause lets that finish and lets the
        // peripheral advertise again, so the reconnect finds a clean state.
        drop(conn);
        Timer::after_millis(500).await;
        ended.send(id).await;
    }
}

// A missing key-up must not leave a modifier, mouse button, or sleep hold
// latched after a radio drop. Release only this half's recorded down switches.
async fn release_disconnected_keys(
    pressed_keys: &RefCell<MatrixState>,
    matrix: PeripheralMatrixConfig,
) {
    for row in 0..matrix.rows {
        for col in 0..matrix.cols {
            let pressed = pressed_keys.borrow().read(row, col);
            if pressed {
                publish_event_async(KeyboardEvent::key(
                    row + matrix.row_offset,
                    col + matrix.col_offset,
                    false,
                ))
                .await;
                pressed_keys
                    .borrow_mut()
                    .update(&KeyboardEvent::key(row, col, false));
            }
        }
    }
}

/// Default connection parameters for the central <-> peripheral connection.
fn default_split_conn_params() -> RequestedConnParams {
    RequestedConnParams {
        min_connection_interval: Duration::from_micros(7500),
        max_connection_interval: Duration::from_micros(7500),
        max_latency: 30, // 232.5ms, same as the awake subrate params
        supervision_timeout: Duration::from_secs(6),
        ..Default::default()
    }
}

/// Connection parameters for the central <-> peripheral connection while the central sleeps.
fn sleep_split_conn_params() -> RequestedConnParams {
    if crate::state::active_transport().is_some() {
        RequestedConnParams {
            min_connection_interval: Duration::from_millis(20),
            max_connection_interval: Duration::from_millis(20),
            max_latency: 200, // 4s
            supervision_timeout: Duration::from_secs(15),
            ..Default::default()
        }
    } else {
        RequestedConnParams {
            min_connection_interval: Duration::from_millis(200),
            max_connection_interval: Duration::from_millis(200),
            max_latency: 20, // ~4s
            supervision_timeout: Duration::from_secs(15),
            ..Default::default()
        }
    }
}

#[cfg(feature = "subrating")]
pub(crate) mod subrating {
    // Measurements on nrf52840 for subrate request parameters:
    //    |-------+------+-----+------+---------+------------|---------|
    //    |   HCL | [ms] |  SF | [ms] | IC [µA] |  KPL [ms]  | IP [µA] |
    //    |-------+------+-----+------+---------+------------|---------|
    //    |    60 |  450 |  10 |   75 |      80 |  41 /   82 |      21 |
    //    |    30 |  225 |  30 |  225 |      75 | 116 /  232 |      21 |
    // ==>|    60 |  450 |  30 |  225 |      59 | 116 /  232 |      21 |<== Connected Sleep
    //    |   180 | 1350 |  30 |  225 |      48 | 116 /  232 |      21 |
    //    |   300 | 2250 |  30 |  225 |      48 | 116 /  232 |      21 |
    //    |    30 |  225 |  60 |  450 |      63 | 228 /  457 |      21 |
    //    |    60 |  450 |  60 |  450 |      48 | 228 /  457 |      21 |
    //    |   180 | 1350 |  60 |  450 |      39 | 228 /  457 |      21 |
    //    |   300 | 2250 |  60 |  450 |      34 | 228 /  457 |      21 |
    //    |   300 | 2250 | 120 |  900 |      32 | 453 /  907 |      21 |
    // ==>| no HC |      | 100 |  750 |      24 | 378 /  757 |      21 |<== Disconnected Sleep
    //    | no HC |      | 125 |  937 |      22 | 472 /  945 |      21 |
    //    | no HC |      | 250 | 1875 |      21 | 941 / 1882 |      24 |
    //    |-------+------+-----+------+---------+------------|---------|
    //    HCL .. Host Connection max latency (host <-> central, assumes 7.5ms interval)
    //    SF ... Subrate Factor split connection
    //    IC ... Central average current
    //    KPL .. Key Press latency (mean/worst)
    //    IP ... Peripheral average current
    //
    //
    // In active mode without pressing any key, the peripheral current depends on the max
    // latency of the split connection:
    //    | max_latency |  [ms] | min_timeout [ms] | IP [µA] |
    //    |-------------+-------+------------------+---------|
    //    |          10 |    75 |              165 |      72 |
    // ==>|          30 |   225 |              465 |      38 |<== Default Params
    //    |          60 |   450 |              915 |      30 |
    //    |         300 |  2250 |             4515 |      21 |
    //    |-------------+-------+------------------+---------|

    use bt_hci::cmd::le::{LeSubrateRequest, LeSubrateRequestParams};
    use bt_hci::controller::ControllerCmdAsync;
    use bt_hci::param::{ConnHandle, Duration, Error as HciError};
    use trouble_host::prelude::*;

    const SLEEP_HOST_CONN_SUBRATE: u16 = 30;
    const SLEEP_NO_HOST_SUBRATE: u16 = 100;

    // In some cases, the subrate request procedure does not complete with only one continuation.
    const SLEEP_CONTINUATION_NUMBER: u16 = 2;

    const fn calc_max_latency(subrate_max: u16) -> u16 {
        // BLE spec requires: Subrate_Max * (Max_Latency + 1) <= 500.
        // We use 250 here to tolerant clock drift(max 500 ppm).
        (250 / subrate_max) - 1
    }

    /// Default subrating params when the central is awake.
    pub(super) fn default_split_subrating_params(handle: ConnHandle) -> LeSubrateRequestParams {
        LeSubrateRequestParams {
            handle,
            subrate_min: 1,
            subrate_max: 1,
            max_latency: 30,
            continuation_number: 0,
            supervision_timeout: Duration::from_secs(6),
        }
    }

    pub(super) fn sleep_split_subrating_params(handle: ConnHandle) -> LeSubrateRequestParams {
        let subrate = if crate::state::active_transport().is_some() {
            SLEEP_HOST_CONN_SUBRATE
        } else {
            SLEEP_NO_HOST_SUBRATE
        };

        LeSubrateRequestParams {
            handle,
            subrate_min: subrate,
            subrate_max: subrate,
            max_latency: calc_max_latency(subrate), // answers every 3600ms with a host, 3750ms without
            continuation_number: SLEEP_CONTINUATION_NUMBER,
            supervision_timeout: Duration::from_millis(15_000),
        }
    }

    pub(crate) async fn update_subrate_factor<
        C: Controller + ControllerCmdAsync<LeSubrateRequest>,
        P: PacketPool,
    >(
        stack: &Stack<'_, C, P>,
        params: LeSubrateRequestParams,
    ) -> bool {
        for _ in 0..10 {
            let subrate_request = LeSubrateRequest::from(params);
            match stack.async_command(subrate_request).await {
                Ok(_) => {
                    debug!("[update_subrate_factor] requested {:?}", params);
                    return true;
                }
                Err(BleHostError::BleHost(Error::Hci(error))) => {
                    // A connection runs one link-layer control procedure at a time, and
                    // a fresh one is still running its own.
                    if error == HciError::CONTROLLER_BUSY
                        || error == HciError::DIFFERENT_TRANSACTION_COLLISION
                    {
                        info!(
                            "[update_subrate_factor] controller busy, retrying: {:?}",
                            error
                        );
                        embassy_time::Timer::after_millis(100).await;
                        continue;
                    }
                    if error == HciError::UNKNOWN_CONN_IDENTIFIER {
                        error!("[update_subrate_factor] stale split link, rebooting");
                        embassy_time::Timer::after_millis(100).await;
                        crate::boot::reboot_keyboard();
                    }
                    error!("[update_subrate_factor] HCI error: {:?}", error);
                    return false;
                }
                Err(e) => {
                    #[cfg(feature = "defmt")]
                    let e = defmt::Debug2Format(&e);
                    error!("[update_subrate_factor] BLE host error: {:?}", e);
                    return false;
                }
            }
        }
        warn!("[update_subrate_factor] controller stayed busy, giving up");
        false
    }
}

async fn run_central_manager_task<
    'b,
    's: 'b,
    #[cfg(not(feature = "subrating"))] C: Controller + ControllerCmdAsync<LeSetPhy> + ControllerCmdSync<LeReadLocalSupportedFeatures>,
    #[cfg(feature = "subrating")] C: Controller
        + ControllerCmdAsync<LeSetPhy>
        + ControllerCmdSync<LeReadLocalSupportedFeatures>
        + ControllerCmdAsync<LeSubrateRequest>,
    P: PacketPool,
>(
    id: usize,
    stack: &'b Stack<'s, C, P>,
    conn: &Connection<'b, P>,
    matrix_config: PeripheralMatrixConfig,
    pressed_keys: &RefCell<MatrixState>,
) -> Result<(), BleHostError<C::Error>> {
    let client = GattClient::<C, P, 10>::new(stack, conn).await?;

    // Split link uses 2M PHY always.
    update_ble_phy(stack, conn, PhyKind::Le2M).await;

    info!("Updating connection parameters for peripheral");
    update_conn_params(stack, conn, &default_split_conn_params()).await;

    let (Either3::First(e) | Either3::Second(e) | Either3::Third(e)) = select3(
        ble_central_task(&client, conn),
        discover_and_run_manager(id, &client, matrix_config, pressed_keys),
        update_conn_params_on_sleep_change(stack, conn),
    )
    .await;
    e
}

async fn ble_central_task<'a, C: Controller + ControllerCmdAsync<LeSetPhy>, P: PacketPool>(
    client: &GattClient<'a, C, P, 10>,
    conn: &Connection<'a, P>,
) -> Result<(), BleHostError<C::Error>> {
    // Watch for the disconnect; draining the other events keeps the small
    // per-connection queue from sitting full and dropping it.
    let conn_events = async {
        loop {
            if let ConnectionEvent::Disconnected { reason } = conn.next().await {
                info!("Connection lost: {:?}", reason);
                break;
            }
        }
    };

    match select(client.task(), conn_events).await {
        Either::First(e) => e,
        Either::Second(()) => Ok(()),
    }
}

/// Discover the split service on the connected peripheral, then run its
/// [`PeripheralManager`] over the GATT link.
async fn discover_and_run_manager<C: Controller + ControllerCmdAsync<LeSetPhy>, P: PacketPool>(
    id: usize,
    client: &GattClient<'_, C, P, 10>,
    matrix_config: PeripheralMatrixConfig,
    pressed_keys: &RefCell<MatrixState>,
) -> Result<(), BleHostError<C::Error>> {
    let services = client
        .services_by_uuid(&Uuid::new_long(SPLIT_SERVICE_UUID.to_le_bytes()))
        .await?;
    info!("Services found");
    let Some(service) = services.first() else {
        return Ok(());
    };
    let message_to_central = client
        .characteristic_by_uuid::<GattSplitMessage>(
            service,
            &Uuid::new_long(MESSAGE_TO_CENTRAL_UUID.to_le_bytes()),
        )
        .await?;
    info!("Message to central found");
    let message_to_peripheral = client
        .characteristic_by_uuid::<GattSplitMessage>(
            service,
            &Uuid::new_long(MESSAGE_TO_PERIPHERAL_UUID.to_le_bytes()),
        )
        .await?;
    info!("Subscribing notifications");
    let listener = client.subscribe(&message_to_central, false).await?;
    let split_ble_driver = BleSplitCentralDriver {
        listener,
        message_to_peripheral,
        client,
        pressed_keys,
    };
    #[cfg(not(feature = "custom_message"))]
    PeripheralManager::new(split_ble_driver, id, matrix_config)
        .run()
        .await;
    // A peripheral built without `custom_message` has no such characteristics;
    #[cfg(feature = "custom_message")]
    {
        use postcard::experimental::max_size::MaxSize;

        use crate::custom_message::{CustomMessage, CustomMessageTarget, forward, send};
        use crate::event::publish_event;

        let to_central = client
            .characteristic_by_uuid::<heapless::Vec<u8, { CustomMessage::POSTCARD_MAX_SIZE }>>(
                service,
                &Uuid::new_long(CUSTOM_TO_CENTRAL_UUID.to_le_bytes()),
            )
            .await
            .ok();
        let to_peripheral = client
            .characteristic_by_uuid::<heapless::Vec<u8, { CustomMessage::POSTCARD_MAX_SIZE }>>(
                service,
                &Uuid::new_long(CUSTOM_TO_PERIPHERAL_UUID.to_le_bytes()),
            )
            .await
            .ok();
        let mut incoming = match &to_central {
            Some(characteristic) => Some(client.subscribe(characteristic, false).await?),
            None => None,
        };

        let from_peripheral = async {
            match incoming.as_mut() {
                None => core::future::pending().await,
                Some(listener) => loop {
                    let notification = listener.next().await;
                    match postcard::from_bytes::<CustomMessage>(notification.as_ref()) {
                        // A central is the one board with links on both sides: it relays
                        // what is headed past it and delivers what names it.
                        Ok(message) => match message.target {
                            CustomMessageTarget::Dongle => send(message),
                            CustomMessageTarget::Central => publish_event(message),
                            CustomMessageTarget::Peripherals => (),
                        },
                        Err(_) => warn!("[split] undecodable custom message dropped"),
                    }
                },
            }
        };

        let to_this_peripheral = async {
            let Some(characteristic) = to_peripheral.as_ref() else {
                return core::future::pending().await;
            };
            forward(Some(CustomMessageTarget::Peripherals), async |encoded| {
                client
                    .write_characteristic_without_response(characteristic, encoded)
                    .await
            })
            .await
        };

        select3(
            PeripheralManager::new(split_ble_driver, id, matrix_config).run(),
            from_peripheral,
            to_this_peripheral,
        )
        .await;
    }
    info!("Peripheral manager stopped");
    Ok(())
}

/// [`SplitReader`]/[`SplitWriter`] over the peripheral's GATT link: reads are
/// notifications on `message_to_central`, writes go to `message_to_peripheral`.
struct BleSplitCentralDriver<
    'a,
    'b,
    'c,
    C: Controller + ControllerCmdAsync<LeSetPhy>,
    P: PacketPool,
> {
    listener: NotificationListener<'b, { trouble_host::config::GATT_CLIENT_NOTIFICATION_MTU }>,
    message_to_peripheral: Characteristic<GattSplitMessage>,
    client: &'c GattClient<'a, C, P, 10>,
    pressed_keys: &'c RefCell<MatrixState>,
}

impl<'a, 'b, 'c, C: Controller + ControllerCmdAsync<LeSetPhy>, P: PacketPool> SplitReader
    for BleSplitCentralDriver<'a, 'b, 'c, C, P>
{
    async fn read(&mut self) -> Result<SplitMessage, SplitDriverError> {
        let data = self.listener.next().await;
        let message =
            postcard::from_bytes(data.as_ref()).map_err(|_| SplitDriverError::DeserializeError)?;
        debug!("Received split message: {:?}", message);

        if let SplitMessage::Key(event) = message {
            self.pressed_keys.borrow_mut().update(&event);
        }
        // Key events from the peripheral count as activity for sleep management
        if matches!(message, SplitMessage::Key(_) | SplitMessage::Pointing(_)) {
            report_activity();
        }

        Ok(message)
    }
}

impl<'a, 'b, 'c, C: Controller + ControllerCmdAsync<LeSetPhy>, P: PacketPool> SplitWriter
    for BleSplitCentralDriver<'a, 'b, 'c, C, P>
{
    async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError> {
        let gatt_msg = GattSplitMessage::try_from(message)?;
        if let Err(e) = self
            .client
            .write_characteristic_without_response(&self.message_to_peripheral, gatt_msg.as_gatt())
            .await
        {
            if let BleHostError::BleHost(Error::NotFound) = e {
                error!("Peripheral disconnected");
                return Err(SplitDriverError::Disconnected);
            }
            #[cfg(feature = "defmt")]
            let e = defmt::Debug2Format(&e);
            error!("BLE message_to_peripheral_write error: {:?}", e);
        }

        Ok(gatt_msg.len)
    }
}

/// Keep one peripheral link's connection parameters in sync with the keyboard's
/// sleep state. Runs for as long as the link is up.
async fn update_conn_params_on_sleep_change<
    'b,
    's: 'b,
    #[cfg(not(feature = "subrating"))] C: Controller + ControllerCmdAsync<LeSetPhy> + ControllerCmdSync<LeReadLocalSupportedFeatures>,
    #[cfg(feature = "subrating")] C: Controller + ControllerCmdAsync<LeSetPhy> + ControllerCmdAsync<LeSubrateRequest>,
    P: PacketPool,
>(
    stack: &'b Stack<'s, C, P>,
    conn: &Connection<'b, P>,
) -> Result<(), BleHostError<C::Error>> {
    let mut sleep_events = SleepStateEvent::subscriber();

    let mut sleeping = crate::state::current_sleep_state();
    if !sleeping {
        // Restart the idle timeout so service discovery isn't cut short. Asleep
        // this isn't user activity, so discovery runs on the sleep parameters.
        report_activity();
    }

    let mut sleeping_conn_param_applied = false;

    loop {
        if sleeping != sleeping_conn_param_applied {
            #[cfg(not(feature = "subrating"))]
            let sent = {
                let params = if sleeping {
                    sleep_split_conn_params()
                } else {
                    default_split_conn_params()
                };
                update_conn_params(stack, conn, &params).await
            };

            #[cfg(feature = "subrating")]
            let sent = {
                let params = if sleeping {
                    subrating::sleep_split_subrating_params(conn.handle())
                } else {
                    subrating::default_split_subrating_params(conn.handle())
                };
                subrating::update_subrate_factor(stack, params).await
            };

            if sent {
                sleeping_conn_param_applied = sleeping;
            }
        }

        sleeping = sleep_events.next_event().await.0;
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::Ordering;

    use super::*;
    use crate::ble::sleep::SLEEPING_STATE;
    use crate::test_support::test_block_on as block_on;

    #[test]
    fn repeated_awake_or_sleeping_timeouts_keep_the_known_peer() {
        let address = [1, 2, 3, 4, 5, 6];
        let mut slot = SlotState::Disconnected(address);
        for _ in 0..3 {
            slot.connect_timed_out();
            assert_eq!(slot, SlotState::Disconnected(address));
        }
        let mut connected = SlotState::Connected(address);
        connected.connect_timed_out();
        assert_eq!(connected, SlotState::Connected(address));
    }

    #[test]
    fn explicit_clear_does_not_require_a_live_peer() {
        let address = [1, 2, 3, 4, 5, 6];
        let mut slot = SlotState::Disconnected(address);
        assert!(!slot.clear_peer());
        assert_eq!(slot, SlotState::NoAddr);
        assert!(!slot.restore_saved_peer(&PeerAddress::new(0, false, address)));
        assert!(!slot.adopt_discovered_peer([9; 6], false, false));
        assert!(slot.adopt_discovered_peer([9; 6], false, true));
        assert_eq!(slot, SlotState::Candidate([9; 6]));

        let mut live = SlotState::Connected(address);
        assert!(live.clear_peer());
        live.session_ended();
        assert_eq!(
            live,
            SlotState::NoAddr,
            "old session ending cannot restore a cleared address"
        );
    }

    #[test]
    fn clearing_offline_peer_reaches_storage_without_a_session_task() {
        block_on(async {
            crate::storage::clear_flash_channel();
            let mut slots = [SlotState::Disconnected([1; 6])];
            let cleared = select(
                clear_saved_peers(&mut slots),
                crate::storage::drain_flash_channel(),
            )
            .await;
            assert!(matches!(cleared, Either::First(true)));
            assert_eq!(slots[0], SlotState::NoAddr);
            assert!(!link_reset_requested(0));
        });
    }

    #[test]
    fn clearing_live_peer_waits_for_its_old_session_to_end() {
        block_on(async {
            crate::storage::clear_flash_channel();
            let mut slots = [SlotState::Connected([1; 6])];
            let cleared = select(
                clear_saved_peers(&mut slots),
                crate::storage::drain_flash_channel(),
            )
            .await;
            assert!(matches!(cleared, Either::First(true)));
            assert_eq!(slots[0], SlotState::Clearing);
            assert!(link_reset_requested(0));
            assert!(!slots[0].adopt_discovered_peer([2; 6], false, true));
            slots[0].session_ended();
            clear_link_reset(0);
            assert_eq!(slots[0], SlotState::NoAddr);
            assert!(slots[0].adopt_discovered_peer([2; 6], false, true));
        });
    }

    #[test]
    fn an_advertisement_racing_sleep_cannot_replace_the_saved_peer() {
        let address = [1, 2, 3, 4, 5, 6];
        let mut slot = SlotState::NoAddr;
        assert!(!slot.adopt_discovered_peer([9; 6], true, true));
        assert!(slot.restore_saved_peer(&PeerAddress::new(0, true, address)));
        assert_eq!(slot, SlotState::Disconnected(address));
        assert!(!slot.adopt_discovered_peer([9; 6], false, true));
    }

    #[test]
    fn sleep_stops_an_already_running_discovery_wait() {
        block_on(async {
            SLEEPING_STATE.store(false, Ordering::Release);
            let started = Instant::now();
            let sleep = async {
                Timer::after_millis(100).await;
                SLEEPING_STATE.store(true, Ordering::Release);
                core::future::pending::<()>().await;
            };
            select(wait_until_sleep(), sleep).await;
            assert!(started.elapsed() <= Duration::from_millis(1001));
        });
    }

    #[test]
    fn sleeping_reconnect_pause_expires_without_a_right_key() {
        block_on(async {
            SLEEPING_STATE.store(true, Ordering::Release);
            let ended = Channel::new();
            let started = Instant::now();
            wait_before_sleep_retry(&ended).await;
            assert!(started.elapsed() >= SLEEP_RECONNECT_PAUSE);
            assert!(started.elapsed() < SLEEP_RECONNECT_PAUSE + Duration::from_millis(1));
            assert!(
                crate::state::current_sleep_state(),
                "retrying must not wake the keyboard"
            );
        });
    }

    #[test]
    fn a_right_key_interrupts_the_reconnect_pause() {
        block_on(async {
            SLEEPING_STATE.store(true, Ordering::Release);
            let ended = Channel::new();
            let started = Instant::now();
            let wake = async {
                Timer::after_millis(100).await;
                SLEEPING_STATE.store(false, Ordering::Release);
                core::future::pending::<()>().await;
            };
            select(wait_before_sleep_retry(&ended), wake).await;
            assert!(started.elapsed() <= Duration::from_millis(1001));
        });
    }

    #[test]
    fn disconnect_releases_only_held_keys_from_that_half() {
        block_on(async {
            let matrix = PeripheralMatrixConfig {
                rows: 2,
                cols: 3,
                row_offset: 4,
                col_offset: 6,
            };
            let keys = RefCell::new(MatrixState::new(2, 3));
            keys.borrow_mut().update(&KeyboardEvent::key(0, 1, true));
            keys.borrow_mut().update(&KeyboardEvent::key(1, 2, true));
            keys.borrow_mut().update(&KeyboardEvent::key(1, 0, true));
            keys.borrow_mut().update(&KeyboardEvent::key(1, 0, false));
            let mut events = KeyboardEvent::subscriber();
            release_disconnected_keys(&keys, matrix).await;
            assert_eq!(
                events.next_message_pure().await,
                KeyboardEvent::key(4, 7, false)
            );
            assert_eq!(
                events.next_message_pure().await,
                KeyboardEvent::key(5, 8, false)
            );
            assert!(events.try_next_message_pure().is_none());
            assert!(!keys.borrow().any_pressed());
        });
    }
}
