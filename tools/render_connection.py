#!/usr/bin/env python3
"""Render the editable connection diagram using Zen Maru Gothic."""
import argparse
import os
from pathlib import Path

os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
from PyQt5.QtGui import QColor, QFontDatabase, QImage, QPainter
from PyQt5.QtSvg import QSvgRenderer
from PyQt5.QtWidgets import QApplication


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--font-dir', type=Path, required=True)
    args = parser.parse_args()
    app = QApplication([])
    for weight in ['Regular', 'Medium', 'Bold']:
        path = args.font_dir / ('ZenMaruGothic-' + weight + '.ttf')
        if QFontDatabase.addApplicationFont(str(path)) < 0:
            parser.error('Could not load ' + str(path))
    images = Path(__file__).resolve().parents[1] / 'docs/images'
    renderer = QSvgRenderer(str(images / 'connection.svg'))
    if not renderer.isValid():
        parser.error('Invalid connection.svg')
    image = QImage(renderer.defaultSize() * 2, QImage.Format_ARGB32)
    image.fill(QColor('#ffffff'))
    painter = QPainter(image)
    renderer.render(painter)
    painter.end()
    if not image.save(str(images / 'connection.png')):
        parser.error('Could not save connection.png')


if __name__ == '__main__':
    main()
