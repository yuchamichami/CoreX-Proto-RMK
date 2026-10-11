use embassy_time::{Duration, MockDriver};
use rmk_types::action::EncoderAction;
use rmk_types::morse::{Morse, TAP};

use super::*;
use crate::config::{BehaviorConfig, PositionalConfig};
use crate::event::KeyboardEvent;
use crate::keymap::KeymapData;

const UNLOCK_KEYS: &[(u8, u8)] = &[(3, 4), (0, 0)];

fn with_service(f: impl FnOnce(&VialService<'_>)) {
    let mut data = KeymapData::new_with_encoder([[[KeyAction::No; 7]; 8]; 1], [[EncoderAction::default(); 2]; 1]);
    data.macros[0] = b'x';
    data.macros_stored = true;
    let mut behavior = BehaviorConfig::default();
    behavior.morse.morses.push(Morse::default()).unwrap();
    let positional = PositionalConfig::<8, 7>::default();
    let keymap = embassy_futures::block_on(KeyMap::new(&mut data, &mut behavior, &positional));
    let mut config = RmkConfig::default();
    config.vial_config = VialConfig::new(b"CoreXPR1", &[], UNLOCK_KEYS);
    MockDriver::get().reset();
    f(&VialService::new(&keymap, &config));
}

fn report(bytes: &[u8]) -> ViaReport {
    let mut data = [0; 32];
    data[..bytes.len()].copy_from_slice(bytes);
    ViaReport {
        input_data: data,
        output_data: data,
    }
}

fn send(service: &VialService<'_>, bytes: &[u8]) -> ViaReport {
    let mut request = report(bytes);
    embassy_futures::block_on(embassy_futures::select::select(
        service.process_via_packet(&mut request),
        crate::test_support::drain_flash_channel(),
    ));
    request
}

fn hold(service: &VialService<'_>, held: bool) {
    for &(row, col) in UNLOCK_KEYS {
        service
            .ctx
            .keymap
            .update_matrix_state(&KeyboardEvent::key(row, col, held));
    }
}

fn tick(service: &VialService<'_>) -> ViaReport {
    MockDriver::get().advance(Duration::from_millis(100));
    send(service, &[0xfe, 7])
}

fn unlock(service: &VialService<'_>) {
    send(service, &[0xfe, 6]);
    hold(service, true);
    for _ in 0..50 {
        tick(service);
    }
    assert!(service.is_unlocked());
    hold(service, false);
}

#[test]
fn locked_commands_cannot_mutate_macros_erase_storage_boot_or_read_matrix() {
    with_service(|service| {
        for command in [0x0f, 0x10, 0x0a, 0x0b, 0x06] {
            let result = send(service, &[command, 0, 0, 1, b'z']);
            assert_eq!(result.input_data[0], 0xff, "command {command:#x}");
        }
        assert_eq!(service.ctx.keymap.macros(|m| m.bytes()[0]), b'x');
        assert_eq!(send(service, &[0xfe, 0x0c]).input_data[0], 0xff);
        hold(service, true);
        let result = send(service, &[0x02, 0x03]);
        assert_eq!(result.input_data[0], 0xff);
        assert!(result.input_data[1..].iter().all(|&n| n == 0));

        // Ordinary remapping, including the Vial accessory settings, stays usable.
        assert_ne!(send(service, &[5, 0, 1, 6, 0x7e, 0x1b]).input_data[0], 0xff);
        assert_eq!(to_via_keycode(service.ctx.get_action(0, 1, 6)), 0x7e1b);
        assert_eq!(send(service, &[4, 0, 1, 6]).input_data[4..6], [0x7e, 0x1b]);
    });
}

#[test]
fn physical_unlock_requires_start_sustained_hold_and_reports_completion_without_extra_poll() {
    with_service(|service| {
        let status = send(service, &[0xfe, 5]);
        assert_eq!(&status.input_data[..6], &[0, 0, 3, 4, 0, 0]);
        hold(service, true);
        for _ in 0..60 {
            tick(service);
        }
        assert!(!service.is_unlocked(), "poll alone cannot arm a challenge");

        send(service, &[0xfe, 6]);
        for _ in 0..500 {
            send(service, &[0xfe, 7]);
        }
        assert!(!service.is_unlocked(), "rapid polling cannot replace the hold time");
        for _ in 0..49 {
            tick(service);
        }
        assert!(!service.is_unlocked());
        let done = tick(service);
        assert_eq!(&done.input_data[..3], &[1, 0, 0]);

        // Explicit lock also cancels any in-progress attempt.
        send(service, &[0xfe, 8]);
        assert!(!service.is_unlocked());
        send(service, &[0xfe, 6]);
        send(service, &[0xfe, 8]);
        for _ in 0..60 {
            tick(service);
        }
        assert!(!service.is_unlocked());
    });
}

#[test]
fn released_challenge_key_or_expired_poll_window_resets_unlock() {
    with_service(|service| {
        hold(service, true);
        send(service, &[0xfe, 6]);
        for _ in 0..25 {
            tick(service);
        }
        service.ctx.keymap.update_matrix_state(&KeyboardEvent::key(3, 4, false));
        assert_eq!(tick(service).input_data[2], 50);
        hold(service, true);
        for _ in 0..49 {
            tick(service);
        }
        assert!(!service.is_unlocked());
        MockDriver::get().advance(Duration::from_millis(501));
        assert_eq!(send(service, &[0xfe, 7]).input_data[1], 0);
        for _ in 0..50 {
            tick(service);
        }
        assert!(!service.is_unlocked(), "an expired challenge needs a fresh start");
    });
}

#[test]
fn unlocking_freezes_writes_and_unlocked_tester_reports_both_halves() {
    with_service(|service| {
        send(service, &[0xfe, 6]);
        assert_eq!(send(service, &[5, 0, 1, 1, 0, 4]).input_data[0], 0xff);
        assert_eq!(service.ctx.get_action(0, 1, 1), KeyAction::No);
        assert_eq!(send(service, &[0xfe, 0]).input_data[4..12], *b"CoreXPR1");
        unlock(service);
        service.ctx.keymap.update_matrix_state(&KeyboardEvent::key(0, 2, true));
        service.ctx.keymap.update_matrix_state(&KeyboardEvent::key(7, 6, true));
        let matrix = send(service, &[2, 3]);
        assert_eq!(&matrix.input_data[2..10], &[4, 0, 0, 0, 0, 0, 0, 64]);
        assert!(matrix.input_data[10..].iter().all(|&n| n == 0));
        send(service, &[0x0f, 0, 0, 1, b'z']);
        assert_eq!(service.ctx.keymap.macros(|m| m.bytes()[0]), b'z');
        assert!(service.request_allowed(&report(&[0x0a]).output_data));
        assert!(service.request_allowed(&report(&[0x0b]).output_data));
        send(service, &[0xfe, 8]);
        assert_eq!(send(service, &[2, 3]).input_data[0], 0xff);
    });
}

#[test]
fn system_keycodes_cannot_bypass_lock_through_any_keycode_setter() {
    with_service(|service| {
        for keycode in [0x7c00u16, 0x7c01, 0x7c03] {
            let [hi, lo] = keycode.to_be_bytes();
            assert_eq!(send(service, &[5, 0, 0, 1, hi, lo]).input_data[0], 0xff);
            assert_eq!(send(service, &[0x13, 0, 0, 4, 0, 4, hi, lo]).input_data[0], 0xff);
            assert_eq!(
                service.ctx.get_action(0, 0, 0),
                KeyAction::No,
                "bulk writes reject atomically"
            );
            assert_eq!(send(service, &[0xfe, 4, 0, 0, 1, hi, lo]).input_data[0], 0xff);
            assert_eq!(service.ctx.get_encoder(0, 0).unwrap(), EncoderAction::default());
            for field in 0..4 {
                let mut entry = [0u8; 14];
                entry[..4].copy_from_slice(&[0xfe, 0x0d, 2, 0]);
                entry[4 + field * 2..6 + field * 2].copy_from_slice(&[lo, hi]);
                assert_eq!(send(service, &entry).input_data[0], 0xff);
            }
            assert!(service.ctx.get_morse(0).unwrap().get(TAP).is_none());
            let mut combo = [0u8; 32];
            combo[..6].copy_from_slice(&[0xfe, 0x0d, 4, 0, 4, 0]);
            let output = 4 + crate::COMBO_MAX_LENGTH * 2;
            combo[output..output + 2].copy_from_slice(&[lo, hi]);
            assert_eq!(send(service, &combo).input_data[0], 0xff);
            assert!(service.ctx.with_combos(|c| c[0].is_none()));
        }
        unlock(service);
        assert_ne!(send(service, &[5, 0, 0, 1, 0x7c, 0]).input_data[0], 0xff);
        assert_eq!(to_via_keycode(service.ctx.get_action(0, 0, 1)), 0x7c00);
    });
}

#[test]
fn bulk_keymap_uses_big_endian_byte_offsets_and_rejects_malformed_ranges() {
    with_service(|service| {
        let written = send(service, &[0x13, 0, 2, 4, 0, 4, 0x7e, 0x1b]);
        assert_ne!(written.input_data[0], 0xff);
        assert_eq!(service.ctx.get_action(0, 0, 0), KeyAction::No);
        assert_eq!(to_via_keycode(service.ctx.get_action(0, 0, 1)), 4);
        assert_eq!(to_via_keycode(service.ctx.get_action(0, 0, 2)), 0x7e1b);
        assert_eq!(&send(service, &[0x12, 0, 2, 4]).input_data[4..8], &[0, 4, 0x7e, 0x1b]);
        for command in [0x12, 0x13] {
            for (offset, size) in [(0, 29), (0, 255), (1, 2), (0, 3), (112, 2), (0xffff, 2)] {
                let [hi, lo] = (offset as u16).to_be_bytes();
                assert_eq!(send(service, &[command, hi, lo, size]).input_data[0], 0xff);
            }
        }
    });
}
