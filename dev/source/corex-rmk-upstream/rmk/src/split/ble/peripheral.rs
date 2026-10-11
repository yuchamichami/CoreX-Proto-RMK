#[cfg(feature = "subrating")]
use bt_hci::{cmd::le::LeSetHostFeature, controller::ControllerCmdSync};
use embassy_futures::join::join;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
#[cfg(feature = "custom_message")]
use postcard::experimental::max_size::MaxSize;
use rmk_types::connection::ConnectionStatus;
use trouble_host::prelude::*;

#[cfg(feature = "storage")]
use super::PeerAddress;
use super::{GattSplitMessage, SplitMessage};
use crate::ble::adv::{Adv, advertise};
#[cfg(feature = "custom_message")]
use crate::custom_message::{CustomMessage, CustomMessageTarget, forward};
use crate::event::{CentralConnectedEvent, SleepStateEvent, publish_event};
use crate::split::driver::{SplitDriverError, SplitReader, SplitWriter};
use crate::split::peripheral::SplitPeripheral;
use crate::state::update_status;

/// Gatt service used in split peripheral to send split message to central
#[gatt_service(uuid = "4dd5fbaa-18e5-4b07-bf0a-353698659946")]
pub(crate) struct SplitBleService {
    #[characteristic(uuid = "0e6313e3-bd0b-45c2-8d2e-37a2e8128bc3", read, notify, indicate)]
    pub(crate) message_to_central: GattSplitMessage,

    #[characteristic(
        uuid = "4b3514fb-cae4-4d38-a097-3a2a3d1c3b9c",
        write_without_response,
        read,
        notify
    )]
    pub(crate) message_to_peripheral: GattSplitMessage,

    #[cfg(feature = "custom_message")]
    #[characteristic(uuid = "5f2a7c14-9b3e-4a51-8d76-2c1e4b8a6f03", read, notify)]
    pub(crate) custom_to_central:
        heapless::Vec<u8, { crate::custom_message::CustomMessage::POSTCARD_MAX_SIZE }>,
    #[cfg(feature = "custom_message")]
    #[characteristic(
        uuid = "5f2a7c15-9b3e-4a51-8d76-2c1e4b8a6f03",
        write_without_response,
        read
    )]
    pub(crate) custom_to_peripheral:
        heapless::Vec<u8, { crate::custom_message::CustomMessage::POSTCARD_MAX_SIZE }>,
}

/// Gatt server in split peripheral
#[gatt_server]
pub(crate) struct BleSplitPeripheralServer {
    pub(crate) service: SplitBleService,
}

/// BLE driver for split peripheral
pub(crate) struct BleSplitPeripheralDriver<'stack, 'server, 'c, P: PacketPool> {
    message_to_peripheral: Characteristic<GattSplitMessage>,
    message_to_central: Characteristic<GattSplitMessage>,
    #[cfg(feature = "custom_message")]
    custom_to_peripheral: Characteristic<
        heapless::Vec<u8, { crate::custom_message::CustomMessage::POSTCARD_MAX_SIZE }>,
    >,
    conn: &'c GattConnection<'stack, 'server, P>,
}

impl<'stack, 'server, 'c, P: PacketPool> BleSplitPeripheralDriver<'stack, 'server, 'c, P> {
    pub(crate) fn new(
        server: &'server BleSplitPeripheralServer,
        conn: &'c GattConnection<'stack, 'server, P>,
    ) -> Self {
        Self {
            message_to_central: server.service.message_to_central.clone(),
            message_to_peripheral: server.service.message_to_peripheral.clone(),
            #[cfg(feature = "custom_message")]
            custom_to_peripheral: server.service.custom_to_peripheral.clone(),
            conn,
        }
    }
}

impl<'stack, 'server, 'c, P: PacketPool> SplitReader
    for BleSplitPeripheralDriver<'stack, 'server, 'c, P>
{
    async fn read(&mut self) -> Result<SplitMessage, SplitDriverError> {
        let message = loop {
            match self.conn.next().await {
                GattConnectionEvent::Disconnected { reason } => {
                    error!("Disconnected from central: {:?}", reason);
                    update_status(|c| *c = ConnectionStatus::new());
                    return Err(SplitDriverError::Disconnected);
                }
                GattConnectionEvent::Gatt { event: gatt_event } => {
                    match &gatt_event {
                        GattEvent::Read(event) => {
                            info!("Gatt read event: {:?}", event.handle());
                        }
                        GattEvent::Write(event) => {
                            // Write to peripheral
                            if event.handle() == self.message_to_peripheral.handle {
                                let parsed = event.with_data(|_, data| {
                                    trace!("Got message from central: {:?}", data);
                                    postcard::from_bytes::<SplitMessage>(data)
                                });
                                match parsed {
                                    Ok(message) => {
                                        trace!("Message from central: {:?}", message);
                                        break message;
                                    }
                                    Err(e) => {
                                        error!("Postcard deserialize split message error: {}", e)
                                    }
                                }
                            } else if cfg!(feature = "custom_message") && {
                                #[cfg(feature = "custom_message")]
                                {
                                    event.handle() == self.custom_to_peripheral.handle
                                }
                                #[cfg(not(feature = "custom_message"))]
                                {
                                    false
                                }
                            } {
                                // Not a `SplitMessage`, so the read goes on.
                                // A peripheral has nowhere to forward to.
                                #[cfg(feature = "custom_message")]
                                event.with_data(|_, data| {
                                    match postcard::from_bytes::<CustomMessage>(data) {
                                        // An end of the chain: it delivers what names it and
                                        // has nowhere to relay the rest to.
                                        Ok(message) => match message.target {
                                            CustomMessageTarget::Peripherals => {
                                                publish_event(message)
                                            }
                                            _ => (),
                                        },
                                        Err(_) => {
                                            warn!("[split] undecodable custom message dropped")
                                        }
                                    }
                                });
                            } else {
                                info!("Gatt write other event: {:?}", event.handle());
                            }
                        }
                        _ => debug!("Other gatt event"),
                    };
                    match gatt_event.accept() {
                        Ok(r) => r.send().await,
                        Err(e) => warn!("[gatt] error sending response: {:?}", e),
                    }
                }
                GattConnectionEvent::ConnectionParamsUpdated {
                    conn_interval,
                    peripheral_latency,
                    supervision_timeout,
                } => info!(
                    "[split] params updated: interval {:?}us, latency {:?}, timeout {:?}ms",
                    conn_interval.as_micros(),
                    peripheral_latency,
                    supervision_timeout.as_millis()
                ),
                GattConnectionEvent::SubratingParamsUpdated {
                    subrate_factor,
                    peripheral_latency,
                    continuation_number,
                    supervision_timeout,
                } => info!(
                    "[split] subrating updated: subrate {:?}, latency {:?}, continuation {:?}, timeout {:?}ms",
                    subrate_factor,
                    peripheral_latency,
                    continuation_number,
                    supervision_timeout.as_millis()
                ),
                GattConnectionEvent::PhyUpdated { tx_phy, rx_phy } => {
                    info!("[split] PHY updated: {:?}, {:?}", tx_phy, rx_phy)
                }
                _ => (),
            }
        };
        Ok(message)
    }
}

impl<'stack, 'server, 'c, P: PacketPool> SplitWriter
    for BleSplitPeripheralDriver<'stack, 'server, 'c, P>
{
    async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError> {
        let gatt_msg = GattSplitMessage::try_from(message)?;
        debug!("Writing split message to central: {:?}", message);
        self.message_to_central
            .notify(self.conn, &gatt_msg, true)
            .await
            .map_err(|e| {
                error!("BLE notify error: {:?}", e);
                SplitDriverError::BleError(1)
            })?;
        Ok(gatt_msg.len)
    }
}

/// Let the controller accept the central's subrate requests on the split link.
///
/// Must run concurrently with `ble_task()` (whose runner serves the HCI command)
/// and before any advertising, since the flag only applies to links opened after
/// it is set.
#[cfg(feature = "subrating")]
async fn init_subrating_host_feature<C: Controller + ControllerCmdSync<LeSetHostFeature>>(
    stack: &Stack<'_, C, impl PacketPool>,
) {
    const CONN_SUBRATING_HOST_BIT: u8 = 38;
    let cmd = LeSetHostFeature::new(CONN_SUBRATING_HOST_BIT, 1);
    if let Err(e) = stack.command(cmd).await {
        error!(
            "[split_peri] error setting subrating host feature flag: {:?}",
            e
        );
    }
}

/// Initialize and run the nRF peripheral keyboard service via BLE.
///
/// # Arguments
///
/// * `id` - The id of the peripheral
/// * `central_addr` - The address of the central
/// * `stack` - The stack to use
pub async fn initialize_nrf_ble_split_peripheral_and_run<
    'b,
    's: 'b,
    #[cfg(feature = "subrating")] C: Controller + ControllerCmdSync<LeSetHostFeature>,
    #[cfg(not(feature = "subrating"))] C: Controller,
>(
    id: usize,
    stack: &'b Stack<'s, C, DefaultPacketPool>,
    rows: u8,
    cols: u8,
) {
    let mut input = super::input::PeripheralInput::new(rows, cols);
    publish_event(CentralConnectedEvent { connected: false });

    let mut peripheral = stack.peripheral();
    let runner = stack.runner();

    // First, read central address from storage
    let mut central_addr =
        match crate::storage::read(crate::storage::StorageKey::PeerAddress(0)).await {
            Ok(Some(crate::storage::StorageValue::PeerAddress(a))) if a.is_valid => Some(a.address),
            _ => None,
        };

    let peri_task = async {
        // Set subrating host support before any advertising/connecting
        #[cfg(feature = "subrating")]
        init_subrating_host_feature(stack).await;

        let server = BleSplitPeripheralServer::new_default("rmk").unwrap();
        let mut pairing = super::PairingWindow::new();
        if central_addr.is_none() {
            // One bounded commissioning window after booting an unpaired half.
            pairing.open();
        }
        let mut reset_requested = false;
        loop {
            reset_requested |= super::PAIRING_REQUEST.try_take().is_some();
            if reset_requested {
                reset_requested = false;
                if crate::storage::store(crate::storage::StorageItem::PeerAddress(
                    PeerAddress::new(0, false, [0; 6]),
                ))
                .await
                .is_err()
                {
                    error!("Cannot clear saved split peer; pairing remains closed");
                    pairing.close();
                    continue;
                }
                central_addr = None;
                pairing.open();
            }
            update_status(|c| *c = ConnectionStatus::new());
            publish_event(CentralConnectedEvent { connected: false });
            publish_event(SleepStateEvent::new(false));
            let timeout = if central_addr.is_some() {
                Duration::from_secs(300)
            } else if let Some(remaining) = pairing.remaining() {
                remaining
            } else {
                publish_event(SleepStateEvent::new(true));
                input
                    .while_disconnected(super::PAIRING_REQUEST.wait())
                    .await;
                reset_requested = true;
                continue;
            };
            let result = select(
                super::PAIRING_REQUEST.wait(),
                input.while_disconnected(split_peripheral_advertise(
                    id,
                    central_addr,
                    &mut peripheral,
                    &server,
                    timeout,
                )),
            )
            .await;
            match result {
                Either::First(()) => reset_requested = true,
                Either::Second(Ok(conn)) => {
                    let new_addr = conn.raw().peer_address().addr.into_inner();
                    if central_addr.is_some_and(|saved| saved != new_addr)
                        || (central_addr.is_none() && pairing.remaining().is_none())
                    {
                        warn!("Rejected split central outside the pairing window");
                        drop(conn);
                        continue;
                    }
                    if central_addr.is_none() {
                        if crate::storage::store(crate::storage::StorageItem::PeerAddress(
                            PeerAddress::new(0, true, new_addr),
                        ))
                        .await
                        .is_err()
                        {
                            error!("Cannot save split central");
                            drop(conn);
                            continue;
                        }
                        central_addr = Some(new_addr);
                    }
                    pairing.close();
                    publish_event(CentralConnectedEvent { connected: true });
                    let mut link =
                        SplitPeripheral::new(BleSplitPeripheralDriver::new(&server, &conn));
                    #[cfg(not(feature = "custom_message"))]
                    let session = link.run(&mut input);
                    #[cfg(feature = "custom_message")]
                    let session = select(link.run(&mut input), {
                        let custom_to_central = &server.service.custom_to_central;
                        forward(None, async |encoded| {
                            custom_to_central.notify_raw(&conn, encoded, false).await
                        })
                    });
                    if let Either::First(()) = select(super::PAIRING_REQUEST.wait(), session).await
                    {
                        reset_requested = true;
                    }
                    drop(conn);
                    info!("Disconnected from the central");
                }
                Either::Second(Err(BleHostError::BleHost(Error::Timeout))) => {
                    pairing.remaining();
                    if central_addr.is_some() {
                        publish_event(SleepStateEvent::new(true));
                        if let Either::First(()) =
                            select(super::PAIRING_REQUEST.wait(), input.wait_for_activity()).await
                        {
                            reset_requested = true;
                        }
                    }
                }
                Either::Second(Err(e)) => {
                    #[cfg(feature = "defmt")]
                    let e = defmt::Debug2Format(&e);
                    error!("Advertise error: {:?}", e);
                    input.while_disconnected(Timer::after_millis(500)).await;
                }
            }
        }
    };

    join(
        crate::ble::ble_task(runner, &crate::ble::NoopHandler),
        peri_task,
    )
    .await;
}

/// Known peers advertise only to their saved central. Open advertising is
/// available solely to the lifecycle's bounded pairing window.
async fn split_peripheral_advertise<'a, 'b, C: Controller>(
    id: usize,
    central_addr: Option<[u8; 6]>,
    peripheral: &mut Peripheral<'a, C, DefaultPacketPool>,
    server: &'b BleSplitPeripheralServer<'_>,
    timeout: Duration,
) -> Result<GattConnection<'a, 'b, DefaultPacketPool>, BleHostError<C::Error>> {
    let adv = match central_addr {
        Some(addr) => Adv::Directed(Address::random(addr)),
        None => Adv::SplitPeripheral { id: id as u8 },
    };
    advertise(peripheral, &server.server, adv, timeout).await
}
