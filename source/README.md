# Firmware source

- `corex-rmk-pair/right/`: CoreX right central, PAW3222 only, application v0.9.4.
- `corex-rmk-pair/left/`: stock Cornix left peripheral, application v0.9.0.
- `corex-rmk-upstream/`: four RMK crates pinned to commit `8a6889854fb996be592c55075b385234133e1772`, with the CoreX patches described in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).

Use the repository-root [`build.sh`](../build.sh) to preserve the settings schema and remove local paths from distributable firmware. See [BUILDING.md](../BUILDING.md).

Keyboard Cargo manifests retain their existing `MIT OR Apache-2.0` declarations. File-specific notices, including the Apache-2.0 PAW3222 wire implementation, take precedence.
