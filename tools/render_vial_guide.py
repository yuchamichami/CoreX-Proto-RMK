#!/usr/bin/env python3
"""Render guide images from Vial widgets, without importing any device transport."""
import argparse
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace

os.environ['QT_QPA_PLATFORM'] = 'offscreen'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--vial-source', type=Path, required=True, help='vial-gui checkout root')
parser.add_argument('--font-dir', type=Path, required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(args.vial_source / 'src/main/python'))

from PyQt5.QtCore import Qt
from PyQt5.QtGui import QColor, QFont, QFontDatabase, QImage, QPainter, QPalette
from PyQt5.QtWidgets import QApplication, QLabel, QVBoxLayout, QWidget
from kle_serial import Serial
from keycodes.keycodes import recreate_keyboard_keycodes
from tabbed_keycodes import FilteredTabbedKeycodes
from themes import Theme
from util import KeycodeDisplay
from widgets.keyboard_widget import KeyboardWidget

app = QApplication([])
for style in ('Regular', 'Medium', 'Bold'):
    if QFontDatabase.addApplicationFont(str(args.font_dir / f'ZenMaruGothic-{style}.ttf')) < 0:
        raise RuntimeError(f'Missing Zen Maru Gothic {style} font')
app.setFont(QFont('Zen Maru Gothic', 14))
app.setStyle('Fusion')
Theme.set_theme('Light')
palette = app.palette()
palette.setColor(QPalette.Window, QColor('white'))
palette.setColor(QPalette.Button, QColor('#e3e6e7'))
palette.setColor(QPalette.Highlight, QColor('#137c79'))
app.setPalette(palette)

definition = json.loads((root / 'source/corex-rmk-pair/right/vial.json').read_text())
preset = json.loads((root / 'keymaps/CoreX-Cornix-default.vil').read_text())
# Rendering-only metadata: layer count and protocol come from the preset;
# macro/morse counts are the unchanged RMK defaults for this firmware.
recreate_keyboard_keycodes(SimpleNamespace(
    vial_protocol=preset['vial_protocol'], layers=len(preset['layout']),
    macro_count=32, tap_dance_count=8, custom_keycodes=definition['customKeycodes'],
    midi=None, supported_features=[],
))


class Layout:
    def get_choice(self, _):
        return 0


keys, encoders = [], []
for key in Serial().deserialize(definition['layouts']['keymap']).keys:
    key.layout_index = key.layout_option = -1
    key.row = key.col = key.encoder_idx = key.encoder_dir = None
    if key.labels[4] == 'e':
        key.encoder_idx, key.encoder_dir = map(int, key.labels[0].split(','))
        encoders.append(key)
    else:
        key.row, key.col = map(int, key.labels[0].split(','))
        keys.append(key)

keyboard = KeyboardWidget(Layout())
keyboard.set_keys(keys, encoders)
for widget in keyboard.widgets:
    key = widget.desc
    if key.row is not None:
        code = preset['layout'][0][key.row][key.col]
    else:
        code = preset['encoder_layout'][0][key.encoder_idx][key.encoder_dir]
    KeycodeDisplay.display_keycode(widget, code)
keyboard.resize(keyboard.minimumSizeHint())
keyboard.setFont(QFont('Zen Maru Gothic', 12))


def label(text, size=16, bold=False):
    widget = QLabel(text)
    widget.setFont(QFont('Zen Maru Gothic', size, QFont.Bold if bold else QFont.Normal))
    return widget


def save_panel(filename, title, note, content, width, height=None):
    panel = QWidget()
    layout = QVBoxLayout(panel)
    layout.setContentsMargins(36, 28, 36, 26)
    layout.setSpacing(18)
    layout.addWidget(label(title, 23, True))
    layout.addWidget(label(note))
    layout.addWidget(content, 0, Qt.AlignHCenter)
    footer = label('説明図 · 現行の配列・設定データから生成しています。実機の画面ではありません。', 12)
    footer.setStyleSheet('color: #646464')
    layout.addWidget(footer)
    panel.resize(width, height or panel.sizeHint().height())
    panel.show()
    app.processEvents()
    image = QImage(panel.size() * 2, QImage.Format_ARGB32)
    image.fill(QColor('white'))
    painter = QPainter(image)
    painter.scale(2, 2)
    panel.render(painter)
    painter.end()
    destination = root / 'docs/images' / filename
    if not image.save(str(destination)):
        raise RuntimeError(f'Could not save {destination}')
    panel.hide()
    return panel


overview = save_panel(
    'vial-overview.png', 'CoreX PAW3222 — Layer 0',
    '配列内のキーを選んで割り当てを変更。円の中の「感度」「Scroll」と、その下の「AML」は設定用の欄です。',
    keyboard, keyboard.minimumSizeHint().width() + 72,
)

tabs = FilteredTabbedKeycodes()
for index in range(tabs.count()):
    if tabs.tabText(index) == 'User':
        tabs.setCurrentIndex(index)
        break
else:
    raise RuntimeError('Vial User tab is missing')
tabs.setFixedSize(1150, 330)
user_tab = tabs.currentWidget()
for button in user_tab.alternatives[0].buttons:
    if button.keycode.qmk_id == 'USER10':
        button.setStyleSheet('QPushButton { border: 2px solid #137c79; background: #e8f5f3; border-radius: 4px; }')
settings = save_panel(
    'vial-user-settings.png', '感度を 2倍 → 1.5倍にする例',
    '① Layer 0 の「感度」を選ぶ　　② User タブの「感度 1.5倍」を選ぶ',
    tabs, 1222,
)
print('Updated docs/images/vial-overview.png and vial-user-settings.png')
