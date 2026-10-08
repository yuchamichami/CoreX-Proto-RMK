#!/bin/sh
# Host tests for the PAW wire protocol, fractional motion, and Vial settings.
set -eu
COREX_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
COREX_RUSTC=$(rustup which --toolchain 1.95.0 rustc)
mkdir -p "$COREX_ROOT/build/tests"
for COREX_MODULE in paw_wire tuning_values; do
    "$COREX_RUSTC" --edition 2024 --test \
        "$COREX_ROOT/source/corex-rmk-pair/right/src/$COREX_MODULE.rs" \
        -o "$COREX_ROOT/build/tests/$COREX_MODULE"
    "$COREX_ROOT/build/tests/$COREX_MODULE"
done
python3 "$COREX_ROOT/tools/verify_release.py"
