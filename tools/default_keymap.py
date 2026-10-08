#!/usr/bin/env python3
"""Generate coreX defaults from the pinned manufacturer Cornix layout (no device I/O)."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / 'keymaps/reference/cornix-default-keymap.vil'
REFERENCE_SHA = 'f85dd13d58398ea53e29f3fbab88d07b1f86ba09982e7eaac5d36eb314a327ab'
CONFIG = ROOT / 'source/corex-rmk-pair/right/keyboard.toml'
PRESET = ROOT / 'keymaps/coreX-Cornix-default.vil'
# Keep existing mouse/scroll/BLE layer numbers valid for previously saved layouts.
LAYERS = {0: 0, 1: 1, 2: 5, 3: 4, 4: 6}
NAMES = ['Cornix base', 'Cornix numbers', 'Mouse', 'Scroll', 'Bluetooth',
         'Cornix Bluetooth alternate', 'Cornix spare']
SETTINGS = {(0, 6): 'USER11', (1, 6): 'USER26', (2, 6): 'USER17'}


def stock_position(row, col):
    """Map by physical key position, not by coincident matrix indices."""
    if row < 4:
        return row + 4, col
    if (row, col) == (4, 5):  # Y was moved to a spare matrix row on coreX.
        return 3, 4
    if (row, col) == (5, 6):  # Right encoder push.
        return 0, 5
    if row == 7:
        if col == 3:  # No switch here: occupied by the trackball.
            return None
        if col == 4:
            return 3, 3
        if col == 5:
            return 3, 5
    return row - 4, col


def remap_action(code):
    match = re.fullmatch(r'MO\((\d+)\)', code)
    return f'MO({LAYERS[int(match[1])]})' if match else code


def generate():
    raw = REFERENCE.read_bytes()
    if hashlib.sha256(raw).hexdigest() != REFERENCE_SHA:
        raise ValueError('Manufacturer reference changed; review it before regenerating')
    stock = json.loads(raw)
    layout = [[['KC_NO'] * 7 for _ in range(8)] for _ in NAMES]
    for src, dst in LAYERS.items():
        for row, values in enumerate(stock['layout'][src]):
            for col, code in enumerate(values):
                if code == -1:
                    continue
                pos = stock_position(row, col)
                if pos is not None:
                    r, c = pos
                    layout[dst][r][c] = remap_action(code)
    # Extra layers remain transparent over the stock typing layout.
    for layer in [2, 3]:
        layout[layer] = [['KC_TRNS'] * 7 for _ in range(8)]
        layout[layer][1][4] = 'KC_BTN1'
        layout[layer][1][3] = 'KC_BTN3'
        layout[layer][1][2] = 'KC_BTN2'
    layout[0][0][3] = 'LT3(KC_I)'
    for index, layer in enumerate(layout):
        # Virtual Vial settings have no physical switch.
        for (row, col), code in SETTINGS.items():
            layer[row][col] = code if index == 0 else 'KC_NO'
        for row, col in [(3, 6), (4, 6), (5, 6), (7, 6)]:
            layer[row][col] = 'KC_NO'
    # Vial uses [CCW,CW], and coreX's encoder index is right first.
    encoder = [stock['encoder_layout'][0][1], stock['encoder_layout'][0][0]]
    return {
        'version': 1,
        'uid': int.from_bytes(b'CoreXPR1', 'little'),
        'layout': layout,
        'encoder_layout': [copy.deepcopy(encoder) for _ in NAMES],
        'layout_options': 0,
        'vial_protocol': 6,
        'via_protocol': 9,
        # Omit macros, tap dances, combos and QMK settings to leave them intact.
    }


def rmk_action(code):
    special = {
        'KC_NO': 'No', 'KC_TRNS': '_', 'KC_TAB': 'Tab', 'KC_CAPSLOCK': 'CapsLock',
        'KC_LSHIFT': 'LShift', 'KC_LCTRL': 'LCtrl', 'KC_LGUI': 'LGui', 'KC_LALT': 'LAlt',
        'KC_SPACE': 'Space', 'KC_BSPACE': 'Backspace', 'KC_ENTER': 'Enter',
        'KC_BSLASH': 'Backslash', 'KC_SLASH': 'Slash', 'KC_COMMA': 'Comma', 'KC_DOT': 'Dot',
        'KC_UP': 'Up', 'KC_DOWN': 'Down', 'KC_LEFT': 'Left', 'KC_RIGHT': 'Right',
        'KC_ESCAPE': 'Escape', 'KC_DELETE': 'Delete', 'KC_SCOLON': 'Semicolon',
        'KC_MINUS': 'Minus', 'KC_EQUAL': 'Equal', 'KC_QUOTE': 'Quote',
        'KC_MUTE': 'AudioMute', 'KC_BTN1': 'MouseBtn1', 'KC_BTN2': 'MouseBtn2',
        'KC_BTN3': 'MouseBtn3', 'KC_VOLD': 'AudioVolDown', 'KC_VOLU': 'AudioVolUp',
        'KC_WH_U': 'MouseWheelUp', 'KC_WH_D': 'MouseWheelDown', 'LT3(KC_I)': 'LT(3,I)',
    }
    if code in special:
        return special[code]
    if re.fullmatch('KC_[A-Z]', code):
        return code[3:]
    if re.fullmatch('KC_[0-9]', code):
        return 'Kc' + code[3:]
    if code.startswith('USER'):
        return 'User' + str(int(code[4:]))
    if re.fullmatch(r'MO\(\d+\)', code):
        return code
    raise ValueError('Unmapped Vial action: ' + code)


def keymap_toml(preset, layout_map):
    lines = ['[keymap]', f'layers = {len(NAMES)}']
    for index, name in enumerate(NAMES):
        lines.extend(['[[keymap.layer]]', f'name = "{name}"', 'keys = """'])
        for row in layout_map.strip().splitlines():
            cells = [(int(r), int(c)) for r, c in re.findall(r'\((\d+),(\d+)\)', row)]
            lines.append(' '.join(rmk_action(preset['layout'][index][r][c]) for r, c in cells))
        lines.append('"""')
        # TOML uses [CW,CCW], unlike the Vial .vil order.
        encoders = [[rmk_action(cw), rmk_action(ccw)]
                    for ccw, cw in preset['encoder_layout'][index]]
        lines.append('encoders = ' + json.dumps(encoders))
    return '\n'.join(lines) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='fail if checked-in defaults differ')
    args = parser.parse_args()
    preset = generate()
    old = CONFIG.read_text()
    layout_map = tomllib.loads(old)['layout']['map']
    start, end = old.index('[keymap]\n'), old.index('[ble]\n')
    config = old[:start] + keymap_toml(preset, layout_map) + old[end:]
    data = json.dumps(preset, ensure_ascii=False, indent=2) + '\n'
    if args.check:
        if config != old or not PRESET.exists() or PRESET.read_text() != data:
            parser.exit(1, 'Defaults differ; run python3 tools/default_keymap.py\n')
        print('PASS manufacturer-derived default keymap and Vial preset')
    else:
        CONFIG.write_text(config)
        PRESET.write_text(data)
        print('Generated right keyboard.toml keymap and ' + PRESET.name)


if __name__ == '__main__':
    main()
