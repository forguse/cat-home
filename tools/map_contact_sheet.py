"""Arrange native map audit captures; the input pixels come from the game renderer."""
import argparse
from pathlib import Path
from PIL import Image, ImageDraw


def read_pam(path):
    header, pixels = path.read_bytes().split(b'ENDHDR\n', 1)
    fields = dict(line.split(maxsplit=1) for line in header.decode().splitlines()[1:] if ' ' in line)
    if fields.get('DEPTH') != '4':
        raise ValueError('Expected native RGBA capture')
    return Image.frombytes('RGBA', (int(fields['WIDTH']), int(fields['HEIGHT'])), pixels)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    files = sorted(args.directory.glob('*.pam'))
    if not files:
        parser.error('No native captures in this directory')
    sheet = Image.new('RGB', (1448, ((len(files) + 3) // 4) * 292), '#20282b')
    draw = ImageDraw.Draw(sheet)
    for i, path in enumerate(files):
        picture = read_pam(path)
        picture.save(path.with_suffix('.png'))
        x, y = (i % 4) * 362, (i // 4) * 292
        draw.text((x + 8, y + 5), path.stem, fill='white')
        sheet.paste(picture.resize((362, 270), Image.Resampling.NEAREST).convert('RGB'), (x, y + 22))
    sheet.save(args.directory / 'contact.png')
    print(f'{len(files)} native frames assembled')


if __name__ == '__main__':
    main()
