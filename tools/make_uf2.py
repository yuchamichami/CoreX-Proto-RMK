#!/usr/bin/env python3
"""Generate application-only UF2 with an explicit, range-checked target side."""
import argparse
import hashlib
import json
from pathlib import Path
import struct


def convert(side, data):
    base, limit = (0x1000, 0xA0000) if side == "left" else (0x26000, 0xB0000)
    if len(data) < 8:
        raise ValueError("The application does not contain a vector table")
    stack, reset = struct.unpack_from("<II", data)
    if not (0x20000008 <= stack <= 0x20040000 and stack % 8 == 0):
        raise ValueError("Invalid nRF52840 initial stack pointer")
    if not (reset & 1 and base <= (reset & ~1) < base + len(data)):
        raise ValueError("Reset handler does not belong to the selected side's application")
    count = (len(data) + 255) // 256
    if base + count * 256 > limit:
        raise ValueError("Application would overlap persistent settings")
    blocks = []
    for index in range(count):
        payload = data[index * 256:(index + 1) * 256].ljust(256, b"\0")
        header = struct.pack("<8I", 0x0A324655, 0x9E5D5157, 0x2000,
                             base + index * 256, 256, index, count, 0xADA52840)
        blocks.append(header + payload + b"\0" * 220 + struct.pack("<I", 0x0AB16F30))
    return b"".join(blocks), base, base + count * 256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("side", choices=["left", "right"])
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    data = args.binary.read_bytes()
    try:
        uf2, base, end = convert(args.side, data)
    except ValueError as error:
        parser.error(str(error))
    args.output.write_bytes(uf2)
    print(json.dumps({"side": args.side, "uf2": args.output.name,
                      "binary_bytes": len(data), "range": [hex(base), hex(end)],
                      "sha256": hashlib.sha256(uf2).hexdigest()}))


if __name__ == "__main__":
    main()
