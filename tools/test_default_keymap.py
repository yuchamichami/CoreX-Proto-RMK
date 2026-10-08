#!/usr/bin/env python3
"""Check the recipient keymap against known physical Cornix key positions.

These tests deliberately do not import the generator or its position remapping.
They read the two delivered representations and use physical typing expectations.
No keyboard, serial port, USB device, or BLE connection is opened.
"""
import json
from pathlib import Path
import re
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]

# Logical coreX coordinates from the existing Vial definition and matrix map.
# The Y key and encoder push are exceptions to the regular right-hand rows.
BASE = [
    ['KC_BSPACE', 'KC_P', 'KC_O', 'LT3(KC_I)', 'KC_U', 'KC_BTN3', 'USER11'],
    ['KC_ENTER', 'KC_BSLASH', 'KC_L', 'KC_K', 'KC_J', 'KC_H', 'USER26'],
    ['KC_SLASH', 'KC_UP', 'KC_DOT', 'KC_COMMA', 'KC_M', 'KC_N', 'USER17'],
    ['KC_RIGHT', 'KC_DOWN', 'KC_LEFT', 'MO(6)', 'KC_Y', 'KC_SPACE', 'KC_NO'],
    ['KC_TAB', 'KC_Q', 'KC_W', 'KC_E', 'KC_R', 'KC_T', 'KC_NO'],
    ['KC_CAPSLOCK', 'KC_A', 'KC_S', 'KC_D', 'KC_F', 'KC_G', 'KC_NO'],
    ['KC_LSHIFT', 'KC_Z', 'KC_X', 'KC_C', 'KC_V', 'KC_B', 'KC_MUTE'],
    ['KC_LCTRL', 'KC_LGUI', 'KC_LALT', 'MO(1)', 'MO(4)', 'KC_SPACE', 'KC_NO'],
]

# Normalize the public Vial spelling into the separate RMK vocabulary. This is
# only for artifact agreement; physical expectations below do not use this map.
ALIASES = {
    'KC_NO': 'No', 'KC_TRNS': '_', 'KC_BSPACE': 'Backspace',
    'KC_ENTER': 'Enter', 'KC_BSLASH': 'Backslash', 'KC_SLASH': 'Slash',
    'KC_UP': 'Up', 'KC_DOWN': 'Down', 'KC_LEFT': 'Left', 'KC_RIGHT': 'Right',
    'KC_DOT': 'Dot', 'KC_COMMA': 'Comma', 'KC_TAB': 'Tab',
    'KC_CAPSLOCK': 'CapsLock', 'KC_LSHIFT': 'LShift', 'KC_LCTRL': 'LCtrl',
    'KC_LGUI': 'LGui', 'KC_LALT': 'LAlt', 'KC_SPACE': 'Space',
    'KC_MUTE': 'AudioMute', 'KC_BTN1': 'MouseBtn1', 'KC_BTN2': 'MouseBtn2',
    'KC_BTN3': 'MouseBtn3', 'KC_ESCAPE': 'Escape', 'KC_DELETE': 'Delete',
    'KC_SCOLON': 'Semicolon', 'KC_MINUS': 'Minus', 'KC_EQUAL': 'Equal',
    'KC_QUOTE': 'Quote', 'KC_WH_U': 'MouseWheelUp', 'KC_WH_D': 'MouseWheelDown',
    'KC_VOLU': 'AudioVolUp', 'KC_VOLD': 'AudioVolDown', 'LT3(KC_I)': 'LT(3,I)',
}


def normalize(code):
    if code in ALIASES:
        return ALIASES[code]
    if re.fullmatch(r'KC_[A-Z]', code):
        return code[-1]
    if re.fullmatch(r'KC_[0-9]', code):
        return 'Kc' + code[-1]
    if re.fullmatch(r'USER\d+', code):
        return 'User' + str(int(code[4:]))
    if re.fullmatch(r'MO\(\d+\)', code):
        return code
    raise AssertionError('Unreviewed preset action: ' + repr(code))


class DefaultKeymapTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.config = tomllib.loads(
            (ROOT / 'source/corex-rmk-pair/right/keyboard.toml').read_text())
        cls.preset = json.loads((ROOT / 'keymaps/coreX-Cornix-default.vil').read_text())
        cls.layers = cls.preset['layout']

    def test_base_typing_matches_known_physical_positions(self):
        self.assertEqual(self.layers[0], BASE)

    def test_number_layer_uses_stock_numbers_and_symbols(self):
        layer = self.layers[1]
        # Left physical top row: Escape 1 2 3 4 5.
        self.assertEqual(layer[4][:6], ['KC_ESCAPE', 'KC_1', 'KC_2', 'KC_3', 'KC_4', 'KC_5'])
        # Right physical top row: 6 7 8 9 0 Delete. Y lives at (3,4).
        self.assertEqual([layer[r][c] for r, c in [(3, 4), (0, 4), (0, 3), (0, 2), (0, 1), (0, 0)]],
                         ['KC_6', 'KC_7', 'KC_8', 'KC_9', 'KC_0', 'KC_DELETE'])
        self.assertEqual(layer[5][4:6], ['KC_SCOLON', 'KC_MINUS'])
        self.assertEqual(layer[1][4:6], ['KC_QUOTE', 'KC_EQUAL'])

    def test_stock_empty_cells_are_disabled_not_transparent(self):
        # Number layer: letters, thumb Fn positions, and arrows are deliberately
        # disabled by the manufacturer. KC_TRNS here would type base-layer keys.
        for row, col in [(1, 0), (1, 1), (2, 5), (3, 0), (3, 3), (3, 5),
                         (5, 0), (5, 1), (6, 1), (7, 3), (7, 4), (7, 5)]:
            with self.subTest(row=row, col=col):
                self.assertEqual(self.layers[1][row][col], 'KC_NO')
        # Spare Fn layer has only the two encoder buttons enabled.
        enabled = {(r, c): value for r, row in enumerate(self.layers[6])
                   for c, value in enumerate(row) if value != 'KC_NO'}
        self.assertEqual(enabled, {(0, 5): 'KC_BTN3', (6, 6): 'KC_MUTE'})

    def test_trackball_layers_and_settings_keep_their_existing_numbers(self):
        self.assertEqual(self.config['behavior']['auto_mouse_layer'][0]['target_layer'], 2)
        self.assertEqual(self.layers[0][0][3], 'LT3(KC_I)')
        for layer in (2, 3):
            self.assertEqual([self.layers[layer][1][col] for col in (4, 3, 2)],
                             ['KC_BTN1', 'KC_BTN3', 'KC_BTN2'])
            self.assertEqual(self.layers[layer][0][3], 'KC_TRNS')
        self.assertEqual([self.layers[0][row][6] for row in range(3)],
                         ['USER11', 'USER26', 'USER17'])
        for layer in self.layers:
            self.assertEqual([layer[row][col] for row, col in [(3, 6), (4, 6), (5, 6), (7, 6)]],
                             ['KC_NO'] * 4)

    def test_bluetooth_uses_left_column_and_no_accidental_peer_reset(self):
        for index in (4, 5):
            actions = {(r, c): code for r, row in enumerate(self.layers[index])
                       for c, code in enumerate(row) if code.startswith('USER')}
            self.assertEqual(actions, {(5, 0): 'USER00', (6, 0): 'USER01', (7, 0): 'USER02'})
        # User7 has a five-second peer-reset action; it must not be introduced
        # by renumbering the stock BLE layer or by confusing it with MO(7).
        reserved = {int(code[4:]) for layer in self.layers for row in layer
                    for code in row if code.startswith('USER')}
        self.assertEqual(reserved, {0, 1, 2, 11, 17, 26})

    def test_encoder_direction_and_buttons_follow_stock(self):
        for layer in self.preset['encoder_layout']:
            self.assertEqual(layer, [['KC_WH_U', 'KC_WH_D'], ['KC_VOLD', 'KC_VOLU']])
        for layer in self.config['keymap']['layer']:
            # RMK orders CW then CCW, while Vial stores CCW then CW.
            self.assertEqual(layer['encoders'],
                             [['MouseWheelDown', 'MouseWheelUp'], ['AudioVolUp', 'AudioVolDown']])
        for index in (0, 1, 4, 5, 6):
            self.assertEqual(self.layers[index][0][5], 'KC_BTN3')
            self.assertEqual(self.layers[index][6][6], 'KC_MUTE')

    def test_firmware_and_importable_preset_agree_on_every_cell(self):
        positions = [(int(row), int(col)) for row, col in
                     re.findall(r'\((\d+),(\d+)\)', self.config['layout']['map'])]
        self.assertEqual(len(positions), 56)
        self.assertEqual(set(positions), {(row, col) for row in range(8) for col in range(7)})
        self.assertEqual(self.config['keymap']['layers'], 7)
        self.assertEqual(len(self.layers), 7)
        self.assertEqual(len(self.config['keymap']['layer']), 7)
        self.assertEqual(len(self.preset['encoder_layout']), 7)
        for index, (preset, config) in enumerate(zip(self.layers, self.config['keymap']['layer'])):
            self.assertEqual([len(row) for row in preset], [7] * 8)
            tokens = config['keys'].split()
            self.assertEqual(len(tokens), 56)
            for (row, col), token in zip(positions, tokens):
                with self.subTest(layer=index, row=row, col=col):
                    self.assertEqual(token, normalize(preset[row][col]))

    def test_fn_targets_exist_without_changing_old_pointing_indices(self):
        self.assertEqual(self.layers[0][7][3], 'MO(1)')
        self.assertEqual(self.layers[0][7][4], 'MO(4)')
        self.assertEqual(self.layers[0][3][3], 'MO(6)')
        for layer in self.layers:
            for row in layer:
                for code in row:
                    match = re.fullmatch(r'MO\((\d+)\)', code)
                    if match:
                        self.assertLess(int(match[1]), len(self.layers))

    def test_update_does_not_request_storage_or_unrelated_preset_reset(self):
        storage = self.config['storage']
        self.assertFalse(storage['clear_storage'])
        self.assertFalse(storage.get('clear_layout', False))
        self.assertEqual((storage['start_addr'], storage['num_sectors']), (0xB0000, 32))
        self.assertEqual(self.preset['uid'], 3553991396538085187)
        self.assertEqual((self.config['layout']['rows'], self.config['layout']['cols']), (8, 7))
        # Loading the preset should update key/encoder assignments, not silently
        # replace a recipient's macros, tap dances, combos, or QMK settings.
        for name in ('macro', 'tap_dance', 'combo', 'key_override', 'alt_repeat_key', 'settings'):
            self.assertNotIn(name, self.preset)


if __name__ == '__main__':
    unittest.main(verbosity=2)
