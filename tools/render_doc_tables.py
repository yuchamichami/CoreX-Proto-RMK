#!/usr/bin/env python3
"""Render Markdown tables in Zen Maru Gothic; preserve editable text in details."""
import argparse
import html
import os
from pathlib import Path
import re

os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
from PyQt5.QtCore import Qt, QRectF
from PyQt5.QtGui import QColor, QFont, QFontDatabase, QFontMetricsF, QImage, QPainter, QPen
from PyQt5.QtWidgets import QApplication
from PyQt5.QtSvg import QSvgRenderer

ROOT = Path(__file__).resolve().parents[1]
TABLE = re.compile(r'(?m)^\|[^\n]+\|\n\|\s*:?-+[^\n]*\|\n(?:\|[^\n]*\|(?:\n|$))+')
BLOCK = re.compile(r'<!-- zen-table:start -->.*?<!-- zen-table:end -->', re.S)
LINK = re.compile(r'\[([^\]]+)\]\(([^)]+)\)')


def plain(text):
    text = LINK.sub(r'\1', text)
    return html.unescape(re.sub(r'[*`]', '', text)).strip()


def wrap(text, metrics, width):
    lines, current = [], ''
    for char in text:
        if char == '\n' or (current and metrics.horizontalAdvance(current + char) > width):
            lines.append(current)
            current = ''
        if char != '\n':
            current += char
    return lines + [current]


def render(table, destination):
    rawrows = [line.strip().strip('|').split('|') for line in table.strip().splitlines()]
    rows = [[plain(cell) for cell in row] for index, row in enumerate(rawrows) if index != 1]
    cols = len(rows[0])
    ratios = {2: [.28, .72], 3: [.22, .36, .42], 4: [.20, .26, .26, .28]}.get(cols, [1 / cols] * cols)
    width, padding = 1120, 20
    widths = [int((width - 2) * ratio) for ratio in ratios]
    widths[-1] = width - 2 - sum(widths[:-1])
    fonts = []
    for weight in [QFont.Medium, QFont.Bold]:
        font = QFont('Zen Maru Gothic'); font.setPixelSize(23); font.setWeight(weight); fonts.append(font)
    prepared, heights = [], []
    for idx, row in enumerate(rows):
        font = fonts[1 if idx == 0 else 0]
        metrics = QFontMetricsF(font)
        lines = [wrap(cell, metrics, colwidth - padding * 2) for cell, colwidth in zip(row, widths)]
        height = max(len(cell) for cell in lines) * 33 + padding * 2
        prepared.append(lines); heights.append(height)
    height = sum(heights) + 2
    image = QImage(width * 2, height * 2, QImage.Format_ARGB32)
    image.fill(QColor('#ffffff'))
    painter = QPainter(image); painter.scale(2, 2)
    painter.setRenderHint(QPainter.Antialiasing); painter.setRenderHint(QPainter.TextAntialiasing)
    y = 1
    for index, (row, rowheight) in enumerate(zip(prepared, heights)):
        if index == 0:
            painter.fillRect(QRectF(1, y, width - 2, rowheight), QColor('#f5f5f5'))
        painter.setFont(fonts[1 if index == 0 else 0]); painter.setPen(QColor('#242424'))
        x = 1
        for cell, colwidth in zip(row, widths):
            baseline = y + padding + QFontMetricsF(painter.font()).ascent()
            for line in cell:
                painter.drawText(QRectF(x + padding, baseline - QFontMetricsF(painter.font()).ascent(), colwidth - padding * 2, 33), Qt.AlignLeft | Qt.AlignTop, line)
                baseline += 33
            x += colwidth
        painter.setPen(QPen(QColor('#dedede'), 1))
        painter.drawLine(1, y + rowheight, width - 1, y + rowheight)
        y += rowheight
    painter.setPen(QPen(QColor('#d7d7d7'), 1)); painter.drawRect(1, 1, width - 2, height - 2)
    painter.end(); image.save(str(destination))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--font-dir', type=Path, required=True, help='Directory containing ZenMaruGothic-Medium.ttf and -Bold.ttf')
    args = parser.parse_args()
    app = QApplication([])
    for weight in ['Medium', 'Bold']:
        path = args.font_dir / ('ZenMaruGothic-' + weight + '.ttf')
        if QFontDatabase.addApplicationFont(str(path)) < 0:
            parser.error('Could not load ' + str(path))
    diagram = QSvgRenderer(str(ROOT / 'docs/images/connection.svg'))
    diagram_image = QImage(2160, 540, QImage.Format_ARGB32); diagram_image.fill(QColor('#ffffff'))
    diagram_painter = QPainter(diagram_image); diagram.render(diagram_painter); diagram_painter.end()
    diagram_image.save(str(ROOT / 'docs/images/connection.png'))
    dest = ROOT / 'docs/images/tables'; dest.mkdir(parents=True, exist_ok=True)
    for relative in ['README.md', 'BUILDING.md', 'docs/usage.md', 'docs/flashing.md']:
        path = ROOT / relative
        source = path.read_text()
        source = BLOCK.sub(lambda match: '\n' + TABLE.search(match.group()).group() + '\n', source)
        counter = 0
        def replace(match):
            nonlocal counter
            counter += 1
            name = path.stem.lower() + '-table-' + str(counter).zfill(2) + '.png'
            table = match.group()
            render(table, dest / name)
            relative_image = os.path.relpath(dest / name, path.parent)
            headings = re.findall(r'(?m)^#{1,6} (.+)$', source[:match.start()])
            title = headings[-1] if headings else '設定'
            links = list(dict.fromkeys(LINK.findall(table)))
            refs = '\n\n関連リンク：' + ' ／ '.join('[' + label + '](' + url + ')' for label, url in links) if links else ''
            return ('<!-- zen-table:start -->\n![' + title + 'の表](' + relative_image + ')' + refs +
                    '\n\n<details>\n<summary>表をテキストで読む</summary>\n\n' + table.rstrip() +
                    '\n\n</details>\n<!-- zen-table:end -->\n')
        path.write_text(TABLE.sub(replace, source))
        print(relative + ': ' + str(counter) + ' tables')


if __name__ == '__main__':
    main()
