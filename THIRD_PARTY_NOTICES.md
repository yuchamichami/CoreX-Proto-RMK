# Third-party notices

This repository preserves the license declarations and copyright notices already present in its firmware sources. It does not relicense third-party material. The keyboard Cargo packages declare `MIT OR Apache-2.0`; file-specific declarations take precedence. The UF2 images also contain Nordic components with separate license terms.

## RMK

The four crates in `source/corex-rmk-upstream/` are a source snapshot of [rmk-rs/rmk](https://github.com/rmk-rs/rmk/tree/8a6889854fb996be592c55075b385234133e1772), commit `8a6889854fb996be592c55075b385234133e1772`:

- `rmk` 0.9.0
- `rmk-macro` 0.8.0
- `rmk-config` 0.8.0
- `rmk-types` 0.4.0

The original [MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE) license texts are included, as well as copies inside the vendor snapshot. The MIT text retains `Copyright (c) 2024 HaoboGu`.

The snapshot includes these CoreX changes to upstream runtime code:

- `rmk-macro/src/codegen/orchestrator.rs` and `rmk/src/lib.rs`: initialize the stored keymap before creating custom pointing processors.
- `rmk/src/keyboard/auto_mouse_layer.rs`: runtime AML enable/disable, releasing only an automatically owned layer when disabled, and associated tests.
- `rmk/src/ble/battery_service.rs` and `rmk/src/ble/mod.rs`: refresh battery characteristics from cached measurements on connection and read; send the current level when a host subscribes or restores an encrypted connection; allow unencrypted reads and notification subscriptions for battery attributes while preserving the existing HID and Vial access requirements.

The repository's `tools/git-metadata/git` preserves the installed firmware's RMK commit identifier for storage compatibility. That identifier is a build-metadata compatibility value; this distribution also includes the modifications listed above. Vendored README/test configuration files are retained from the working source snapshot.

The unused STM32 maintenance utility `rmk-config/src/gen_usb_map.py` also accepts its data directory as a command-line argument instead of a developer's hardcoded local path. This utility is not part of the nRF52840 build.

## PAW3222 wire protocol

[`paw_wire.rs`](source/corex-rmk-pair/right/src/paw_wire.rs) retains its Apache-2.0 notice:

> Copyright 2024 Google LLC; modifications 2025 sekigon-gonnoc. Rust adaptation 2026 CoreX prototype.

The working protocol and register sequence derive from [sekigon-gonnoc/zmk-driver-paw3222](https://github.com/sekigon-gonnoc/zmk-driver-paw3222/tree/fc946760e7f870e512be8fce7a26cc5c004a8663), commit `fc946760e7f870e512be8fce7a26cc5c004a8663`. That driver cites Zephyr's [input_paw32xx.c](https://github.com/zephyrproject-rtos/zephyr/blob/19c6240b6865bcb28e1d786d4dcadfb3a02067a0/drivers/input/input_paw32xx.c), also Apache-2.0. The CoreX Rust adaptation adds fractional cursor scaling and tests. The full Apache-2.0 text is included in [LICENSE-APACHE](LICENSE-APACHE).

## Nordic radio libraries and ARM attribution

The firmware links `nrf-sdc` 0.4.0 / `nrf-mpsl` 0.4.0 and their `*-sys` 0.3.0 crates. Their Rust wrappers and the linked binary libraries have different licenses:

- Rust wrapper license texts: [nrf-sdc MIT](licenses/NRF-SDC-MIT.txt), [nrf-sdc Apache-2.0](licenses/NRF-SDC-APACHE-2.0.txt), [nrf-mpsl MIT](licenses/NRF-MPSL-MIT.txt), [nrf-mpsl Apache-2.0](licenses/NRF-MPSL-APACHE-2.0.txt).
- Linked Nordic SoftDevice Controller / MPSL binary libraries: [Nordic 5-Clause](licenses/NORDIC-5-CLAUSE.txt), including the requirement to use this software only with a Nordic Semiconductor integrated circuit.
- MPSL's included ARM functions: [ARM BSD-3-Clause attribution](licenses/ARM-MPSL-ATTRIBUTION.txt).
- Additional Nordic/ARM source notices: [nrfx](licenses/NORDIC-NRFX.txt) and [CMSIS Apache-2.0](licenses/CMSIS-APACHE-2.0.txt).

These are copied from the exact Cargo packages resolved by the lockfile, not paraphrased license grants. Keep these notices with binary redistribution.

## Cargo dependencies

[`licenses/dependencies.json`](licenses/dependencies.json) lists the resolved dependency names, versions, declared licenses, upstream repositories, and included license files. [`licenses/dependencies/`](licenses/dependencies/) retains the notices distributed with those packages. The inventory includes build dependencies as well as runtime dependencies; inclusion in the inventory does not imply every package is linked into the UF2.

Some Apache-2.0/MIT dual-licensed crate archives omit separate license files. For these, the inventory points to the included standard Apache-2.0 license text and records the upstream repository. Where upstream source provides additional attribution, that attribution remains applicable.

## Cornix and Vial

Cornix is the original keyboard on which the left hardware and encoder presentation are based. This repository is a CoreX-specific firmware configuration, not a replacement source of official Cornix releases. Vial is a separate configuration application; it is not bundled here. Screenshots in the documentation show Vial operating with the CoreX definition. The project names and UI references do not imply endorsement.

## 初期キーマップの参照元

`keymaps/reference/cornix-default-keymap.vil` は、JezailFunderが配布しているCornixの初期設定ファイルです。取得元・ハッシュ・CoreXで変更した箇所は[keymaps/README.md](keymaps/README.md)に記載しています。
