use byteorder::{BigEndian, ByteOrder};
use embassy_time::Instant;
use embedded_io_async::{Read, Write};
use rmk_types::action::{Action, KeyAction, KeyboardAction};
use rmk_types::protocol::vial::VialCommand;
use rmk_types::protocol::vial::{VIA_PROTOCOL_VERSION, ViaCommand, ViaKeyboardInfo};
use vial::process_vial;

use crate::config::{RmkConfig, VialConfig};
use crate::hid::ViaReport;
use crate::host::context::KeyboardContext;
use crate::host::via::keycode_convert::{from_via_keycode, to_via_keycode};
use crate::keymap::KeyMap;
use crate::{MACRO_MAX_NUM, MACRO_SPACE_SIZE, boot};

pub(crate) mod keycode_convert;
#[cfg(feature = "host_lock")]
mod lock;
#[cfg(all(test, feature = "host_lock"))]
mod security_tests;
mod vial;

// Match Vial's bootloader-key firewall, also covering RMK's reboot and storage
// erase keys. Apply to every host keycode write path, including dynamic entries.
fn keycode_allowed(keycode: u16, unlocked: bool) -> bool {
    unlocked
        || !matches!(
            from_via_keycode(keycode),
            KeyAction::Single(Action::KeyboardControl(
                KeyboardAction::Bootloader | KeyboardAction::Reboot | KeyboardAction::ClearEeprom
            ))
        )
}

fn reject(report: &mut ViaReport) {
    report.input_data.fill(0);
    report.input_data[0] = ViaCommand::Unhandled as u8;
}

pub struct VialService<'a> {
    ctx: KeyboardContext<'a>,
    vial_config: VialConfig<'static>,
    #[cfg(feature = "host_lock")]
    locker: lock::VialLock<'a>,
}

impl<'a> VialService<'a> {
    pub fn new(keymap: &'a KeyMap<'a>, config: &RmkConfig<'static>) -> Self {
        Self {
            ctx: KeyboardContext::new(keymap),
            vial_config: config.vial_config,
            #[cfg(feature = "host_lock")]
            locker: lock::VialLock::new(config.vial_config.unlock_keys, keymap, config.vial_config.insecure),
        }
    }

    fn is_unlocked(&self) -> bool {
        #[cfg(feature = "host_lock")]
        {
            self.locker.is_unlocked()
        }
        #[cfg(not(feature = "host_lock"))]
        {
            true
        }
    }

    fn request_allowed(&self, request: &[u8; 32]) -> bool {
        #[cfg(feature = "host_lock")]
        if self.locker.is_unlocking() {
            // Freeze configuration while the physical challenge is held. Lock
            // remains available to cancel an attempt explicitly.
            return request[0] == ViaCommand::Vial as u8
                && matches!(
                    request[1].into(),
                    VialCommand::GetKeyboardId
                        | VialCommand::GetSize
                        | VialCommand::GetKeyboardDef
                        | VialCommand::GetUnlockStatus
                        | VialCommand::UnlockStart
                        | VialCommand::UnlockPoll
                        | VialCommand::Lock
                );
        }
        match ViaCommand::from(request[0]) {
            ViaCommand::DynamicKeymapMacroSetBuffer
            | ViaCommand::DynamicKeymapMacroReset
            | ViaCommand::EepromReset
            | ViaCommand::BootloaderJump
            | ViaCommand::DynamicKeymapReset => self.is_unlocked(),
            ViaCommand::GetKeyboardValue if request[1] == ViaKeyboardInfo::SwitchMatrixState as u8 => {
                cfg!(feature = "host_lock") && self.is_unlocked()
            }
            ViaCommand::Vial if request[1] == VialCommand::QmkSettingsReset as u8 => self.is_unlocked(),
            _ => true,
        }
    }

    async fn process_via_packet(&self, report: &mut ViaReport) {
        let command_id = report.output_data[0];
        if !self.request_allowed(&report.output_data) {
            reject(report);
            return;
        }

        // Caller pre-fills `input_data` from `output_data`, so individual arms
        // only need to overwrite the bytes they actually change.
        match command_id.into() {
            ViaCommand::GetProtocolVersion => {
                BigEndian::write_u16(&mut report.input_data[1..3], VIA_PROTOCOL_VERSION);
            }
            ViaCommand::GetKeyboardValue => {
                // Check the second u8
                match report.output_data[1].try_into() {
                    Ok(v) => match v {
                        ViaKeyboardInfo::Uptime => {
                            let value = Instant::now().as_millis() as u32;
                            BigEndian::write_u32(&mut report.input_data[2..6], value);
                        }
                        ViaKeyboardInfo::LayoutOptions => {
                            let layout_option = self.ctx.layout_options();
                            BigEndian::write_u32(&mut report.input_data[2..6], layout_option);
                        }
                        #[cfg(not(feature = "host_lock"))]
                        ViaKeyboardInfo::SwitchMatrixState => {
                            error!("It is not secure to use matrix tester without vial lock");
                        }
                        #[cfg(feature = "host_lock")]
                        ViaKeyboardInfo::SwitchMatrixState if self.locker.is_unlocked() => {
                            let bitmap = &mut report.input_data[2..];
                            bitmap.fill(0);
                            self.ctx.read_matrix_state(bitmap);
                            // Vial wants each row's bytes big-endian (QMK matrix_row_t order).
                            let (rows, cols, _) = self.ctx.keymap_dimensions();
                            let row_len = cols.div_ceil(8);
                            if row_len > 1 {
                                let len = (rows * row_len).min(bitmap.len());
                                for row in bitmap[..len].chunks_mut(row_len) {
                                    row.reverse();
                                }
                            }
                        }
                        ViaKeyboardInfo::FirmwareVersion => {
                            BigEndian::write_u32(
                                &mut report.input_data[2..6],
                                env!("COREX_VIA_VERSION").parse::<u32>().unwrap_or(0),
                            );
                        }
                        _ => (),
                    },
                    Err(e) => error!("Invalid subcommand: {} of GetKeyboardValue", e),
                }
            }
            ViaCommand::SetKeyboardValue => {
                // Check the second u8
                match report.output_data[1].try_into() {
                    Ok(v) => match v {
                        ViaKeyboardInfo::LayoutOptions => {
                            let layout_option = BigEndian::read_u32(&report.output_data[2..6]);
                            let _ = self.ctx.set_layout_options(layout_option).await;
                        }
                        ViaKeyboardInfo::DeviceIndication => {
                            let _device_indication = report.output_data[2];
                            warn!("SetKeyboardValue - DeviceIndication")
                        }
                        _ => (),
                    },
                    Err(e) => error!("Invalid subcommand: {} of GetKeyboardValue", e),
                }
            }
            ViaCommand::DynamicKeymapGetKeyCode => {
                let layer = report.output_data[1];
                let row = report.output_data[2];
                let col = report.output_data[3];
                let action = self.ctx.get_action(layer, row, col);
                let keycode = to_via_keycode(action);
                info!("Getting keycode: {:02X} at ({},{}), layer {}", keycode, row, col, layer);
                BigEndian::write_u16(&mut report.input_data[4..6], keycode);
            }
            ViaCommand::DynamicKeymapSetKeyCode => {
                let layer = report.output_data[1];
                let row = report.output_data[2];
                let col = report.output_data[3];
                let keycode = BigEndian::read_u16(&report.output_data[4..6]);
                if !keycode_allowed(keycode, self.is_unlocked()) {
                    reject(report);
                    return;
                }
                let action = from_via_keycode(keycode);
                info!(
                    "Setting keycode: 0x{:02X} at ({},{}), layer {} as {:?}",
                    keycode, row, col, layer, action
                );
                let _ = self.ctx.set_action(layer, row, col, action).await;
            }
            ViaCommand::DynamicKeymapReset => {
                warn!("Dynamic keymap reset -- not supported")
            }
            ViaCommand::CustomSetValue => {
                // backlight/rgblight/rgb matrix/led matrix/audio settings here
                warn!("Custom set value -- not supported")
            }
            ViaCommand::CustomGetValue => {
                // backlight/rgblight/rgb matrix/led matrix/audio settings here
                warn!("Custom get value -- not supported")
            }
            ViaCommand::CustomSave => {
                // backlight/rgblight/rgb matrix/led matrix/audio settings here
                warn!("Custom get value -- not supported")
            }
            ViaCommand::EepromReset => {
                warn!("Resetting storage..");
                self.ctx.reset_storage().await;
                // TODO: Reboot after a eeprom reset?
            }
            ViaCommand::BootloaderJump => {
                warn!("Bootloader jumping");
                boot::jump_to_bootloader();
            }
            ViaCommand::DynamicKeymapMacroGetCount => {
                report.input_data[1] = MACRO_MAX_NUM as u8;
            }
            ViaCommand::DynamicKeymapMacroGetBufferSize => {
                BigEndian::write_u16(&mut report.input_data[1..3], MACRO_SPACE_SIZE as u16);
            }
            ViaCommand::DynamicKeymapMacroGetBuffer => {
                let offset = BigEndian::read_u16(&report.output_data[1..3]) as usize;
                let size = report.output_data[3] as usize;
                // The payload is `[4..4 + size]` of a 32-byte report.
                if size <= 28 {
                    let out = &mut report.input_data[4..4 + size];
                    self.ctx.keymap.macros(|m| {
                        let bytes = m.bytes().get(offset..).unwrap_or(&[]);
                        let n = bytes.len().min(size);
                        out[..n].copy_from_slice(&bytes[..n]);
                        out[n..].fill(0);
                    });
                } else {
                    report.input_data[0] = 0xFF;
                }
            }
            ViaCommand::DynamicKeymapMacroSetBuffer => {
                let offset = BigEndian::read_u16(&report.output_data[1..3]) as usize;
                let size = report.output_data[3] as usize;
                if size <= 28 {
                    let changed = self
                        .ctx
                        .keymap
                        .macros(|m| m.write(offset, &report.output_data[4..4 + size]));
                    #[cfg(feature = "storage")]
                    if crate::keyboard::macros::persist(self.ctx.keymap, changed)
                        .await
                        .is_err()
                    {
                        error!("Failed to save the macros");
                    }
                    #[cfg(not(feature = "storage"))]
                    if !changed.is_empty() {
                        warn!("Macros changed but not saved: there is no storage");
                    }
                } else {
                    report.input_data[0] = 0xFF;
                }
            }
            ViaCommand::DynamicKeymapMacroReset => {
                let changed = self.ctx.keymap.macros(|m| m.clear());
                #[cfg(feature = "storage")]
                if crate::keyboard::macros::persist(self.ctx.keymap, changed)
                    .await
                    .is_err()
                {
                    error!("Failed to save the macros");
                }
                #[cfg(not(feature = "storage"))]
                if !changed.is_empty() {
                    warn!("Macros changed but not saved: there is no storage");
                }
            }
            ViaCommand::DynamicKeymapGetLayerCount => {
                report.input_data[1] = self.ctx.keymap_dimensions().2 as u8;
            }
            ViaCommand::DynamicKeymapGetBuffer => {
                let offset = BigEndian::read_u16(&report.output_data[1..3]);
                // size <= 28
                let size = report.output_data[3];
                let (rows, cols, layers) = self.ctx.keymap_dimensions();
                if size > 28
                    || size % 2 != 0
                    || offset % 2 != 0
                    || offset as usize + size as usize > rows * cols * layers * 2
                {
                    reject(report);
                    return;
                }
                debug!("Getting keymap buffer, offset: {}, size: {}", offset, size);
                let mut idx = 4;
                let start = (offset / 2) as usize;
                let count = (size / 2) as usize;
                for i in 0..count {
                    let a = self.ctx.get_action_flat(start + i);
                    let kc = to_via_keycode(a);
                    BigEndian::write_u16(&mut report.input_data[idx..idx + 2], kc);
                    idx += 2;
                }
            }
            ViaCommand::DynamicKeymapSetBuffer => {
                debug!("Dynamic keymap set buffer");
                let offset = BigEndian::read_u16(&report.output_data[1..3]);
                // size <= 28
                let size = report.output_data[3];
                let (rows, cols, layers) = self.ctx.keymap_dimensions();
                if size > 28
                    || size % 2 != 0
                    || offset % 2 != 0
                    || offset as usize + size as usize > rows * cols * layers * 2
                {
                    reject(report);
                    return;
                }
                // Validate the whole request before writing any cell. Offsets
                // and sizes are bytes, and VIA keycodes are big endian.
                if report.output_data[4..4 + size as usize]
                    .chunks_exact(2)
                    .any(|bytes| !keycode_allowed(BigEndian::read_u16(bytes), self.is_unlocked()))
                {
                    reject(report);
                    return;
                }
                let mut idx = 4;
                for i in 0..(size as usize / 2) {
                    let via_keycode = BigEndian::read_u16(&report.output_data[idx..idx + 2]);
                    let action = from_via_keycode(via_keycode);
                    let flat_index = offset as usize / 2 + i;
                    let (layer, in_layer) = (flat_index / (rows * cols), flat_index % (rows * cols));
                    let _ = self
                        .ctx
                        .set_action(layer as u8, (in_layer / cols) as u8, (in_layer % cols) as u8, action)
                        .await;
                    idx += 2;
                }
            }
            ViaCommand::DynamicKeymapGetEncoder => {
                warn!("Keymap get encoder -- not supported");
            }
            ViaCommand::DynamicKeymapSetEncoder => {
                warn!("Keymap set encoder -- not supported");
            }
            ViaCommand::Vial => {
                process_vial(
                    report,
                    &self.vial_config,
                    #[cfg(feature = "host_lock")]
                    &self.locker,
                    &self.ctx,
                )
                .await
            }
            ViaCommand::Unhandled => {
                info!("Unknown cmd: {:?}", report.output_data);
                report.input_data[0] = ViaCommand::Unhandled as u8
            }
        }
    }
}

impl VialService<'_> {
    /// Drive one Vial session against `rx`/`tx` (32-byte request → 32-byte
    /// response, processed in place). Returns on any read/write error;
    /// transport-specific reconnect lives in the caller.
    pub async fn run_session<R: Read, T: Write>(&self, rx: &mut R, tx: &mut T) {
        let mut buf = [0u8; 32];
        loop {
            if rx.read_exact(&mut buf).await.is_err() {
                return;
            }
            let mut report = ViaReport {
                input_data: buf,
                output_data: buf,
            };
            self.process_via_packet(&mut report).await;
            if tx.write_all(&report.input_data).await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use embassy_futures::block_on;
    use rmk_types::action::KeyAction;

    use super::*;
    use crate::config::{BehaviorConfig, PositionalConfig};
    use crate::keymap::{KeyMap, KeymapData};

    /// Build a minimal 1x1x1 keymap + `VialService` and run `f` against it.
    fn with_service<R>(f: impl FnOnce(&mut VialService) -> R) -> R {
        let mut data = KeymapData::new([[[KeyAction::No]]]);
        let mut behavior = BehaviorConfig::default();
        let positional = PositionalConfig::<1, 1>::default();
        let keymap = block_on(KeyMap::new(&mut data, &mut behavior, &positional));
        let mut config = RmkConfig::default();
        config.vial_config.insecure = true;
        let mut service = VialService::new(&keymap, &config);
        f(&mut service)
    }

    /// These tests run no storage task, so writes that wait for one would never be
    /// answered; the stand-in answers them.
    fn process(service: &mut VialService, report: &mut ViaReport) {
        block_on(embassy_futures::select::select(
            service.process_via_packet(report),
            crate::test_support::drain_flash_channel(),
        ));
    }

    /// A `DynamicKeymapMacroSetBuffer` (0x0F) report with `offset = 0` and the
    /// given payload `size` byte. The caller mirrors `Runnable::run` by seeding
    /// `input_data` with a copy of `output_data`.
    fn macro_set_buffer_report(size: u8) -> ViaReport {
        let mut output_data = [0u8; 32];
        output_data[0] = 0x0F; // DynamicKeymapMacroSetBuffer
        output_data[3] = size;
        ViaReport {
            input_data: output_data,
            output_data,
        }
    }

    // `output_data` is [u8; 32], so the handler slices `output_data[4..4 + size]`.
    // size == 28 is the largest payload that fits (writes output_data[4..32]).
    #[test]
    fn macro_set_buffer_max_size_ok() {
        with_service(|service| {
            let mut report = macro_set_buffer_report(28);
            process(service, &mut report);
        });
    }

    // size == 29 slices output_data[4..33], which is out of bounds. The sibling
    // DynamicKeymapMacroGetBuffer handler already rejects size > 28 with 0xFF;
    // SetBuffer must do the same instead of panicking.
    #[test]
    fn macro_set_buffer_oversize_rejected() {
        with_service(|service| {
            let mut report = macro_set_buffer_report(29);
            process(service, &mut report);
            assert_eq!(report.input_data[0], 0xFF);
        });
    }

    /// A `GetKeyboardValue(LayoutOptions)` (0x02 0x02) or
    /// `SetKeyboardValue(LayoutOptions)` (0x03 0x02) report carrying `value`
    /// as the big-endian payload.
    fn layout_options_report(command_id: u8, value: u32) -> ViaReport {
        let mut output_data = [0u8; 32];
        output_data[0] = command_id;
        output_data[1] = 0x02; // ViaKeyboardInfo::LayoutOptions
        BigEndian::write_u32(&mut output_data[2..6], value);
        ViaReport {
            input_data: output_data,
            output_data,
        }
    }

    // GetKeyboardValue(LayoutOptions) must return the value written by
    // SetKeyboardValue(LayoutOptions) instead of a hardcoded 0, so the Vial
    // GUI shows the user's selection again after a reconnect.
    #[test]
    fn layout_options_set_then_get_roundtrip() {
        with_service(|service| {
            let mut set = layout_options_report(0x03, 0x00C0_FFEE);
            process(service, &mut set);

            let mut get = layout_options_report(0x02, 0);
            process(service, &mut get);
            assert_eq!(BigEndian::read_u32(&get.input_data[2..6]), 0x00C0_FFEE);
        });
    }
}
