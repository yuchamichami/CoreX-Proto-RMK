#!/usr/bin/env python3
"""Check the two distribution UF2s without opening or writing any device."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify(image, path):
    raw = path.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == image['uf2_sha256'], 'UF2 SHA-256 mismatch')
    require(len(raw) > 0 and len(raw) % 512 == 0, 'Invalid UF2 length')
    base, limit = (0x26000, 0xB0000) if image['side'] == 'right' else (0x1000, 0xA0000)
    count = len(raw) // 512
    require(base + count * 256 <= limit, 'Application overlaps protected flash')
    parts = []
    for index in range(count):
        block = raw[index * 512:(index + 1) * 512]
        expected = (0x0A324655, 0x9E5D5157, 0x2000,
                    base + index * 256, 256, index, count, 0xADA52840)
        require(struct.unpack_from('<8I', block) == expected, 'UF2 block header mismatch')
        require(struct.unpack_from('<I', block, 508)[0] == 0x0AB16F30, 'UF2 end marker mismatch')
        parts.append(block[32:288])
    payload = b''.join(parts)
    binary = payload[:image['binary_bytes']]
    require(not any(payload[image['binary_bytes']:]), 'Unexpected nonzero UF2 padding')
    require(hashlib.sha256(binary).hexdigest() == image['binary_sha256'], 'Binary SHA-256 mismatch')
    stack, reset = struct.unpack_from('<II', binary)
    require(0x20000008 <= stack <= 0x20040000 and stack % 8 == 0, 'Invalid RAM vector')
    require(reset & 1 and base <= (reset & ~1) < base + len(binary), 'Invalid reset vector')
    for pattern in [b'/Users/', b'/home/', b'\\Users\\']:
        require(pattern not in payload, 'Private build path found in firmware payload')
    return binary


def verify_rebuilt_schema(root, target, manifest, required=False):
    build_dir = target / 'thumbv7em-none-eabihf/release/build'
    metadata_files = [p for p in build_dir.glob('rmk-*/output')
                      if re.fullmatch(r'rmk-[0-9a-f]+', p.parent.name)]
    constants_files = list(build_dir.glob('rmk-types-*/out/constants.rs'))
    if not metadata_files or not constants_files:
        require(not required, 'Missing rebuilt storage metadata; use --target-dir for the build cache')
        print('SKIP rebuilt schema: Cargo metadata unavailable (use --target-dir for an external cache)')
        return
    metadata = max(metadata_files, key=lambda p: p.stat().st_mtime).read_text()
    constants = max(constants_files, key=lambda p: p.stat().st_mtime).read_text()
    commit = re.search(r'^cargo:rustc-env=RMK_COMMIT=(.*)$', metadata, re.M)
    features = re.search(r'^cargo:rustc-env=RMK_FEATURES=(.*)$', metadata, re.M)
    require(commit is not None and features is not None, 'Missing RMK build metadata')
    require(commit.group(1) == manifest['rmk_base_commit'], 'Rebuilt RMK commit changed the schema')
    require(features.group(1) == manifest['rmk_features'], 'Rebuilt RMK features changed the schema')
    cargo = (root / 'source/corex-rmk-upstream/rmk/Cargo.toml').read_text()
    version = re.search(r'^version = "([^"]+)"$', cargo, re.M)
    require(version is not None, 'Missing RMK version')
    framing = b''
    for name in ['MACRO_SPACE_SIZE', 'COMBO_SIZE', 'MORSE_SIZE']:
        value = re.search(r'pub const ' + name + r': usize = (\d+);', constants)
        require(value is not None, 'Missing storage capacity: ' + name)
        framing += struct.pack('<I', int(value.group(1)))
    value = 0x811C9DC5
    for byte in version.group(1).encode() + commit.group(1).encode() + features.group(1).encode() + framing:
        value = ((value ^ byte) * 0x01000193) & 0xFFFFFFFF
    require(value == int(manifest['storage_schema_hash'], 16), 'Rebuilt storage schema mismatch')
    print('PASS rebuilt storage schema: 0x%08X' % value)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--side', choices=['right', 'left', 'both'], default='both',
                        help='side to verify (default: both)')
    parser.add_argument('--rebuilt', action='store_true', help='also validate build/firmware structure, bounds, paths and available schema metadata')
    parser.add_argument('--require-identical', action='store_true', help='also require rebuilt images to be byte-identical to the distribution')
    parser.add_argument('--target-dir', type=Path, help='Cargo target cache for checking rebuilt schema')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    manifest = json.loads((root / 'firmware/manifest.json').read_text())
    sums = dict(line.split(None, 1)[::-1] for line in (root / 'firmware/SHA256SUMS').read_text().splitlines())
    try:
        require({image['side'] for image in manifest['images']} == {'left', 'right'}, 'Missing side')
        for image in manifest['images']:
            if args.side != 'both' and image['side'] != args.side:
                continue
            require(sums.get(image['filename']) == image['uf2_sha256'], 'Checksum manifest mismatch')
            verify(image, root / 'firmware' / image['filename'])
            print('PASS distribution ' + image['side'] + ': ' + image['filename'])
            if args.rebuilt or args.require_identical:
                rebuilt_path = root / 'build/firmware' / image['filename']
                rebuilt_bin = root / 'build/firmware' / Path(image['filename']).with_suffix('.bin')
                rebuilt = dict(image, binary_bytes=rebuilt_bin.stat().st_size,
                               binary_sha256=hashlib.sha256(rebuilt_bin.read_bytes()).hexdigest(),
                               uf2_sha256=hashlib.sha256(rebuilt_path.read_bytes()).hexdigest())
                verify(rebuilt, rebuilt_path)
                same = rebuilt['uf2_sha256'] == image['uf2_sha256']
                print('PASS rebuilt ' + image['side'] + ' structure/range/privacy; distribution hash ' +
                      ('matches' if same else 'differs (checkout/toolchain metadata may change layout)'))
                if args.require_identical:
                    require(same, 'Rebuilt image is not byte-identical to the distribution')
        if args.rebuilt or args.require_identical:
            target = args.target_dir or Path(os.environ.get('CARGO_TARGET_DIR', str(root / 'build/target')))
            verify_rebuilt_schema(root, target, manifest, required=args.require_identical)
    except (ValueError, OSError) as error:
        parser.exit(1, 'FAIL: ' + str(error) + '\n')


if __name__ == '__main__':
    main()
