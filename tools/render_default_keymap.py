#!/usr/bin/env python3
"""Draw the initial physical keymap from the distributed Vial preset.

This is a keymap diagram, not a screenshot of Vial. Encoder editor controls and
trackball setting cells are omitted; their physical controls are shown instead.
"""
import argparse
from html import escape
import json
import os
from pathlib import Path

os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
from PyQt5.QtGui import QColor, QFontDatabase, QImage, QPainter
from PyQt5.QtSvg import QSvgRenderer
from PyQt5.QtWidgets import QApplication

ROOT = Path(__file__).resolve().parents[1]
UNIT = 64
LEFT = 35
RIGHT = 610
TOP = 90
WIDTH, HEIGHT = 1100, 480
LABELS = {
    'KC_BSPACE': ['Backspace'], 'KC_TAB': ['Tab'], 'KC_CAPSLOCK': ['Caps'],
    'KC_LSHIFT': ['Shift'], 'KC_LCTRL': ['Ctrl'], 'KC_LGUI': ['Cmd'],
    'KC_LALT': ['Alt'], 'KC_ENTER': ['Enter'], 'KC_SPACE': ['Space'],
    'KC_BSLASH': ['\\'], 'KC_SLASH': ['/'], 'KC_COMMA': [','], 'KC_DOT': ['.'],
    'KC_LEFT': ['←'], 'KC_RIGHT': ['→'], 'KC_UP': ['↑'], 'KC_DOWN': ['↓'],
    'KC_MUTE': ['消音'], 'KC_BTN3': ['中', 'クリック'],
    'MO(1)': ['数字', 'Fn'], 'MO(4)': ['BT'], 'MO(6)': ['接続'],
    'LT3(KC_I)': ['I', '長押しScroll'],
}


def label(code):
    if code in LABELS:
        return LABELS[code]
    if code.startswith('KC_') and len(code) == 4:
        return [code[3:]]
    raise ValueError('Add a readable label for ' + code)


def physical_keys(definition):
    """Read this definition's explicitly positioned, single-key KLE rows."""
    found = set()
    for row in definition['layouts']['keymap']:
        if len(row) != 2 or not isinstance(row[0], dict) or not isinstance(row[1], str):
            raise ValueError('Expected explicitly positioned single-key KLE rows')
        props, address = row
        if '\n' in address:  # Vial's four encoder rotation editor controls.
            continue
        matrix_row, col = map(int, address.split(','))
        if col == 6 and matrix_row != 6:  # Trackball settings and circular decoration.
            continue
        if (matrix_row, col) in found:
            raise ValueError('Unexpected duplicate physical key ' + address)
        found.add((matrix_row, col))
        for name in ['rx', 'ry', 'x', 'y']:
            if name not in props:
                raise ValueError('Missing explicit geometry: ' + name)
        yield matrix_row, col, props


def svg_text(x, y, value, size=24, weight=500, color='#20242a', anchor='middle'):
    return (f'<text x="{x:.2f}" y="{y:.2f}" text-anchor="{anchor}" '
            f'font-size="{size}" font-weight="{weight}" fill="{color}">{escape(value)}</text>')


def make_svg():
    preset = json.loads((ROOT / 'keymaps/CoreX-Cornix-default.vil').read_text())
    definition = json.loads((ROOT / 'source/corex-rmk-pair/right/vial.json').read_text())
    layer = preset['layout'][0]
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">',
           '<title>CoreX 初期配列図</title>',
           '<desc>Cornix 左と CoreX 右の基本配列。数字/Fn は数字レイヤー、BT は Bluetooth 設定、接続はPC切替などのレイヤー6。I を長押ししながらボールを回すとスクロール。</desc>',
           '<rect width="100%" height="100%" fill="#ffffff"/>',
           '<g font-family="Zen Maru Gothic">',
           svg_text(LEFT, 35, '初期配列', 25, 700, anchor='start'),
           svg_text(LEFT, 69, 'Cornix 左', 19, 500, '#606771', 'start'),
           svg_text(RIGHT, 69, 'CoreX 右', 19, 500, '#606771', 'start')]
    for matrix_row, col, p in physical_keys(definition):
        is_left = matrix_row >= 4
        origin_x = (LEFT + (p['rx'] + 8.5) * UNIT) if is_left else (RIGHT + p['rx'] * UNIT)
        origin_y = TOP + p['ry'] * UNIT
        x, y = p['x'] * UNIT, p['y'] * UNIT
        width, height = p.get('w', 1) * UNIT, p.get('h', 1) * UNIT
        angle = p.get('r', 0)
        cx, cy = x + width / 2, y + height / 2
        lines = label(layer[matrix_row][col])
        encoder = (matrix_row, col) in [(0, 5), (6, 6)]
        out.append(f'<g transform="translate({origin_x:.3f} {origin_y:.3f}) rotate({angle})">')
        if encoder:
            out.append(f'<circle cx="{cx:.3f}" cy="{cy:.3f}" r="{width / 2 - 4}" fill="#fff" stroke="#89919b" stroke-width="1.5"/>')
        else:
            out.append(f'<rect x="{x + 3:.3f}" y="{y + 3:.3f}" width="{width - 6:.3f}" height="{height - 6:.3f}" rx="7" fill="#fafbfc" stroke="#a5acb5" stroke-width="1.3"/>')
        if len(lines) == 2:
            out.append(svg_text(cx, cy + 1, lines[0], 23))
            out.append(svg_text(cx, cy + 20, lines[1], 10 if len(lines[1]) > 4 else 13, color='#535b65'))
        else:
            size = 11 if len(lines[0]) >= 7 else (19 if len(lines[0]) >= 5 else 22)
            out.append(svg_text(cx, cy + size * .35, lines[0], size))
        out.append('</g>')
    # Use the center of the circular trackball outline in the Vial definition.
    outline = next(p for p, a in definition['layouts']['keymap']
                   if a == '0,6' and p.get('w', 1) < .1 and p.get('r', 0) == 0)
    bx, by = RIGHT + outline['rx'] * UNIT, TOP + outline['ry'] * UNIT
    out.append(f'<circle cx="{bx:.2f}" cy="{by:.2f}" r="52" fill="#fafbfc" stroke="#89919b" stroke-width="1.5"/>')
    out.append(svg_text(bx, by + 6, 'トラックボール', 13))
    out.append(svg_text(LEFT, 438, '数字 / Fn：数字・記号', 17, 400, '#535b65', 'start'))
    out.append(svg_text(285, 438, 'BT：Bluetooth 設定', 17, 400, '#535b65', 'start'))
    out.append(svg_text(535, 438, '接続：PC切替・登録', 17, 400, '#535b65', 'start'))
    out.append(svg_text(LEFT, 466, 'I を長押ししながらボールを回すとスクロール', 17, 400, '#535b65', 'start'))
    out.extend(['</g>', '</svg>'])
    return '\n'.join(out) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--font-dir', type=Path, required=True)
    args = parser.parse_args()
    app = QApplication([])
    for weight in ['Regular', 'Medium', 'Bold']:
        path = args.font_dir / ('ZenMaruGothic-' + weight + '.ttf')
        if QFontDatabase.addApplicationFont(str(path)) < 0:
            parser.error('Could not load ' + str(path))
    images = ROOT / 'docs/images'
    svg = images / 'default-keymap.svg'
    svg.write_text(make_svg())
    renderer = QSvgRenderer(str(svg))
    if not renderer.isValid():
        parser.error('Invalid generated SVG')
    image = QImage(renderer.defaultSize() * 2, QImage.Format_ARGB32)
    image.fill(QColor('#ffffff'))
    painter = QPainter(image)
    renderer.render(painter)
    painter.end()
    if not image.save(str(images / 'default-keymap.png')):
        parser.error('Could not save default-keymap.png')


if __name__ == '__main__':
    main()
