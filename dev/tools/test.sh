#!/bin/sh
# Host tests and distribution checks; no connected hardware is accessed.
set -eu
COREX_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
COREX_RUSTC=$(rustup which --toolchain 1.95.0 rustc)
mkdir -p "$COREX_ROOT/build/tests"
for COREX_REL_MODULE in right/src/paw_wire right/src/tuning_values right/src/paw3222_schedule shared/status_led_logic; do
    COREX_MODULE=${COREX_REL_MODULE##*/}
    "$COREX_RUSTC" --edition 2024 --test \
        "$COREX_ROOT/source/corex-rmk-pair/$COREX_REL_MODULE.rs" \
        -o "$COREX_ROOT/build/tests/$COREX_MODULE"
    "$COREX_ROOT/build/tests/$COREX_MODULE"
done
python3 "$COREX_ROOT/tools/default_keymap.py" --check
python3 "$COREX_ROOT/tools/test_default_keymap.py"
python3 "$COREX_ROOT/tools/verify_release.py"
"$COREX_ROOT/tools/test_ble.sh"
