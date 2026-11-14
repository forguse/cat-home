"""Bake fixed-width UTF-8 visitor names into consecutive premultiplied BGRA frames."""

import argparse
import json
from pathlib import Path
import re

from PIL import Image, ImageDraw, ImageFont

from build_basketball_pack import NEAREST, ROOT, Frame, premultiply_bgra


OUTPUT = ROOT / "assets" / "visitors"
SIZE = (64, 12)
ANCHOR = (32, 11)


def runtime_labels():
    """Follow the runtime's declared order and exact display labels."""
    source = (ROOT / "src" / "visitors.rs").read_text(encoding="utf-8")
    body = source.split("const PROFESSIONS: [Profession; 10] = [", 1)[1].split("];", 1)[0]
    order = re.findall(r"Profession::(\w+)", body)
    labels_body = source.split("pub const fn label(self) -> &'static str {", 1)[1].split("\n    }", 1)[0]
    labels = dict(re.findall(r'Self::(\w+) => "([^"]+)"', labels_body))
    names_body = source.split("pub const fn asset_name(self) -> &'static str {", 1)[1].split("\n    }", 1)[0]
    names = dict(re.findall(r'Self::(\w+) => "([^"]+)"', names_body))
    if len(order) != 10 or set(order) != set(labels) or set(order) != set(names):
        raise ValueError("Visitor professions and nameplate label definitions differ")
    return [(names[variant], labels[variant]) for variant in order]


def nameplate(text, font):
    # Oversampling preserves dense Chinese strokes without shipping a font.
    scale = 4
    mask = Image.new("L", (SIZE[0] * scale, SIZE[1] * scale))
    draw = ImageDraw.Draw(mask)
    left, top, right, bottom = draw.textbbox((0, 0), text, font=font)
    width, height = right - left, bottom - top
    if width > (SIZE[0] - 6) * scale or height > 9 * scale:
        raise ValueError(f"Nameplate text does not fit {SIZE}: {text}")
    draw.text(((mask.width - width) // 2 - left, (mask.height - height) // 2 - top),
              text, font=font, fill=255)
    mask = mask.resize(SIZE, Image.Resampling.LANCZOS)
    bounds = mask.point(lambda alpha: 255 if alpha >= 40 else 0).getbbox()
    if bounds is None:
        raise ValueError(f"Missing glyphs: {text}")
    left, _, right, _ = bounds
    image = Image.new("RGBA", SIZE)
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((left - 3, 0, right + 2, SIZE[1] - 1), radius=2, fill=(38, 37, 41, 226))
    draw.line((left - 1, SIZE[1] - 1, right, SIZE[1] - 1), fill=(26, 24, 30, 216))
    ink = Image.new("RGBA", SIZE, (250, 235, 201, 255))
    ink.putalpha(mask)
    image.alpha_composite(ink)
    return image


def preview(plates, output):
    board = Image.new("RGBA", (488, 196), "#4e6253")
    draw = ImageDraw.Draw(board)
    for row, plate in enumerate(plates):
        col, line = divmod(row, 5)
        x, y = 29 + col * 230, 13 + line * 36
        draw.rectangle((x - 10, y - 3, x + 200, y + 29), fill="#6d826c")
        board.alpha_composite(plate.resize((192, 36), NEAREST), (x, y - 4))
    output.parent.mkdir(parents=True, exist_ok=True)
    board.convert("RGB").save(output)


def label_frames(labels):
    font = ImageFont.truetype("msyhbd.ttc", 36)
    return [Frame(f"{job}_label", nameplate(text, font), ANCHOR) for job, text in labels]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview", type=Path)
    args = parser.parse_args()
    labels = runtime_labels()
    plates = [frame.image for frame in label_frames(labels)]
    OUTPUT.mkdir(parents=True, exist_ok=True)
    (OUTPUT / "nameplates.bgra").write_bytes(b"".join(premultiply_bgra(plate) for plate in plates))
    index = {"width": SIZE[0], "height": SIZE[1], "anchor": ANCHOR,
             "frames": [{"profession": job, "text": label} for job, label in labels]}
    (OUTPUT / "nameplates.json").write_text(json.dumps(index, ensure_ascii=False, indent=2) + "\n",
                                            encoding="utf-8")
    if args.preview:
        preview(plates, args.preview)


if __name__ == "__main__":
    main()
