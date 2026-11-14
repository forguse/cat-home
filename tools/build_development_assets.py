"""Bake three visible construction phases for each of the eight development sites."""

import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from build_basketball_pack import INK, NEAREST, ROOT, Frame, pack_pages
from build_life_assets import METAL, WOOD, WOOD_LIGHT


OUTPUT = ROOT / "assets" / "developments"
STONE = "#a29889"
STONE_LIGHT = "#c7b7a1"
DIRT = "#a98667"
SHADOW = (59, 50, 45, 74)


def stake(draw, x, y):
    draw.line((x, y - 4, x, y), fill=WOOD, width=2)
    draw.point((x, y - 3), fill="#e6bd83")


def brick(draw, x, y, width):
    draw.rectangle((x, y, x + width - 1, y + 2), fill=STONE, outline="#796d66")
    draw.line((x + 1, y + 1, x + width - 2, y + 1), fill=STONE_LIGHT)


def road(stage):
    image = Image.new("RGBA", (14, 40))
    draw = ImageDraw.Draw(image)
    if stage == 1:
        for y in (4, 16, 27, 37):
            draw.line((3, y, 10, y), fill="#e8c790")
            for x in (2, 11):
                stake(draw, x, y + 1)
        for y in range(7, 36, 4):
            draw.point((7, y), fill="#d5b188")
    else:
        draw.polygon([(3, 0), (10, 0), (13, 11), (11, 38), (8, 39), (3, 39),
                      (1, 31), (1, 10)], fill=DIRT, outline="#8c765c")
        for y in range(1, 40, 5):
            if stage == 2:
                draw.line((3, y, 10, y), fill=WOOD_LIGHT)
                stake(draw, 1, y + 1)
            else:
                for x, w in ((3, 4), (7, 4)):
                    brick(draw, x + (y % 2), y, w)
                draw.point((2, y + 2), fill="#b8a184")
    # Site column 7 is the east road tile. The frame spans both road tiles.
    return Frame(f"road_{stage}", image, (11, 4))


def bridge(stage):
    image = Image.new("RGBA", (16, 48))
    draw = ImageDraw.Draw(image)
    for y in ((4,) if stage == 1 else (4, 44)):
        for x in (1, 7, 12):
            brick(draw, x, y, min(5, 16 - x))
    if stage >= 2:
        draw.rectangle((3, 6, 12, 43), fill=WOOD_LIGHT, outline=WOOD)
        for y in range(8, 43, 3):
            draw.line((4, y, 11, y), fill=WOOD)
            draw.point((5 if y % 2 else 9, y + 1), fill="#e3b87c")
    if stage == 3:
        for x in (2, 13):
            draw.line((x, 7, x, 43), fill=INK, width=2)
            draw.line((x, 8, x, 42), fill=WOOD_LIGHT)
            for y in (10, 24, 38):
                draw.rectangle((x - 1, y, x + 1, y + 3), fill=WOOD, outline=INK)
                draw.point((x, y), fill="#ead2a5")
    # The work point is in column 12, the west tile of the two-column bridge.
    return Frame(f"bridge_{stage}", image, (4, 4))


def stone_base(draw, x, y, right):
    draw.polygon([(x + 2, y), (right - 3, y), (right, y + 3), (right - 2, y + 5),
                  (x + 1, y + 5), (x, y + 3)], fill=STONE, outline=INK)
    draw.line((x + 2, y + 2, right - 3, y + 2), fill=STONE_LIGHT)


def timber_frame(draw, x, y, right, bottom):
    for post in (x, right):
        draw.line((post, y, post, bottom), fill=INK, width=3)
        draw.line((post, y, post, bottom), fill=WOOD_LIGHT)
    draw.line((x - 1, y, right + 1, y), fill=INK, width=3)
    draw.line((x - 1, y, right + 1, y), fill=WOOD_LIGHT)
    draw.line((x + 1, bottom - 2, right - 2, y + 2), fill=WOOD)


def market(stage):
    image = Image.new("RGBA", (25, 24))
    draw = ImageDraw.Draw(image)
    draw.polygon([(3, 13), (20, 13), (24, 21), (0, 21)], fill="#b7997c", outline="#8f755f")
    for y in (16, 19):
        for x in (4, 10, 16):
            draw.line((x, y, x + 3, y), fill=STONE_LIGHT)
    if stage == 1:
        for x in (3, 20):
            stake(draw, x, 15)
    else:
        timber_frame(draw, 4, 8, 19, 16)
        draw.rectangle((2, 12, 21, 15), fill=WOOD, outline=INK)
        draw.line((4, 12, 19, 12), fill=WOOD_LIGHT)
    if stage == 3:
        draw.rectangle((7, 2, 16, 7), fill="#d8b587", outline=INK)
        draw.line((9, 4, 14, 4), fill="#8c5a57")
        draw.line((10, 6, 13, 6), fill="#8c5a57")
    return Frame(f"market_{stage}", image, (12, 22))


def stall(stage):
    image = Image.new("RGBA", (25, 26))
    draw = ImageDraw.Draw(image)
    draw.ellipse((1, 19, 23, 25), fill=SHADOW)
    stone_base(draw, 2, 18, 22)
    if stage >= 2:
        timber_frame(draw, 4, 6, 20, 19)
        draw.rectangle((4, 12, 20, 16), fill=WOOD_LIGHT, outline=INK)
    if stage == 3:
        draw.polygon([(3, 6), (7, 1), (19, 1), (24, 7), (22, 9), (2, 9)],
                     fill="#b9766b", outline=INK)
        for x in (7, 12, 17):
            draw.polygon([(x, 2), (x + 2, 2), (x + 3, 8), (x, 8)], fill="#eccba5")
    return Frame(f"stall_{stage}", image, (12, 24))


def shelter(kind, stage):
    image = Image.new("RGBA", (32, 32))
    draw = ImageDraw.Draw(image)
    guest = kind == "guesthouse"
    draw.ellipse((1, 24, 30, 31), fill=SHADOW)
    stone_base(draw, 3, 23, 29)
    if stage >= 2:
        timber_frame(draw, 5, 11, 27, 25)
        draw.line([(5, 11), (15, 3), (27, 11)], fill=INK, width=2)
        draw.line([(6, 10), (15, 4), (26, 10)], fill=WOOD_LIGHT)
    if stage == 3:
        draw.rectangle((5, 10, 27, 25), fill="#d4ba92" if guest else "#bca47f", outline=INK)
        draw.polygon([(1, 12), (13, 2), (19, 2), (31, 11), (28, 14), (4, 14)],
                     fill="#9d7769" if guest else "#788b78", outline=INK)
        draw.line([(3, 11), (13, 3), (19, 3), (26, 9)], fill="#c8a18a")
        if guest:
            draw.rectangle((17, 16, 23, 24), fill=WOOD, outline=INK)
            draw.point((21, 21), fill="#edcc83")
            draw.rectangle((8, 16, 13, 20), fill="#91a6a0", outline=INK)
        else:
            for x in (8, 19):
                draw.rectangle((x, 16, x + 5, 22), fill="#8a755b", outline=INK)
                draw.line((x + 1, 18, x + 4, 18), fill=WOOD_LIGHT)
            draw.line((8, 24, 24, 24), fill="#e2c29a")
    return Frame(f"{kind}_{stage}", image, (16, 29))


def well(stage):
    image = Image.new("RGBA", (19, 25))
    draw = ImageDraw.Draw(image)
    draw.ellipse((0, 19, 18, 24), fill=SHADOW)
    draw.ellipse((2, 15, 16, 23), fill=STONE, outline=INK)
    draw.arc((2, 14, 16, 22), 180, 360, fill=STONE_LIGHT)
    if stage >= 2:
        draw.ellipse((3, 13, 15, 19), fill="#596e70", outline=INK)
        draw.arc((2, 14, 16, 21), 0, 180, fill=STONE_LIGHT, width=2)
        for x in (4, 13):
            draw.point((x, 20), fill="#6e6560")
    if stage == 3:
        timber_frame(draw, 4, 5, 14, 17)
        draw.polygon([(2, 5), (6, 1), (13, 1), (17, 5), (15, 7), (3, 7)],
                     fill="#9b7464", outline=INK)
        draw.line((9, 7, 9, 13), fill="#d9b383")
        draw.point((10, 13), fill=METAL)
    return Frame(f"well_{stage}", image, (9, 23))


def garden(stage):
    image = Image.new("RGBA", (28, 22))
    draw = ImageDraw.Draw(image)
    draw.polygon([(4, 4), (23, 4), (27, 17), (23, 20), (1, 19)], fill="#88a072", outline="#657c61")
    draw.line((12, 19, 15, 6), fill="#c6ab83", width=3)
    for x, y in ((6, 7), (7, 13), (20, 9), (22, 15)):
        draw.line((x - 2, y + 2, x + 2, y + 1), fill=DIRT, width=2)
        if stage == 3:
            draw.line((x, y - 2, x, y + 2), fill="#5f7d58")
            draw.point((x - 1, y - 2), fill="#e4af83")
            draw.point((x + 1, y - 3), fill="#d695a0")
    if stage >= 2:
        draw.line((3, 4, 24, 4), fill=WOOD_LIGHT)
        for x in (4, 10, 17, 23):
            stake(draw, x, 7)
    if stage == 3:
        draw.rectangle((18, 13, 24, 15), fill=WOOD_LIGHT, outline=WOOD)
    return Frame(f"garden_{stage}", image, (14, 20))


SITE_ART = (
    ("road", "村口路", road), ("bridge", "南桥", bridge), ("market", "换货广场", market),
    ("stall", "摊棚", stall), ("guesthouse", "客舍", lambda phase: shelter("guesthouse", phase)),
    ("depot", "仓储站", lambda phase: shelter("depot", phase)),
    ("well", "公共井", well), ("garden", "花园", garden),
)


def preview(frames, output):
    entries = {frame.id: frame for frame in frames}
    image = Image.new("RGB", (940, 590), "#222c28")
    draw = ImageDraw.Draw(image)
    title = ImageFont.truetype("msyhbd.ttc", 27)
    label = ImageFont.truetype("msyh.ttc", 16)
    draw.text((24, 15), "南岸建设 · 三阶段素材", font=title, fill="#efe0ba")
    for index, (name, caption, _) in enumerate(SITE_ART):
        col, row = divmod(index, 4)
        x, y = 24 + col * 455, 68 + row * 126
        draw.rounded_rectangle((x, y, x + 437, y + 115), radius=7, fill="#39483f")
        draw.text((x + 10, y + 9), caption, font=label, fill="#efe0ba")
        for phase in (1, 2, 3):
            frame = entries[f"{name}_{phase}"]
            px = x + 99 + (phase - 1) * 106
            scaled = frame.image.resize((frame.image.width * 2, frame.image.height * 2), NEAREST)
            image.paste(scaled, (px, y + 4), scaled)
    output.parent.mkdir(parents=True, exist_ok=True)
    image.save(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview", type=Path)
    args = parser.parse_args()
    frames = [build(phase) for _, _, build in SITE_ART for phase in (1, 2, 3)]
    files = pack_pages(frames)
    index = json.loads(files.pop("assets/index.json"))
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for page in index["pages"]:
        path = page["file"]
        page["file"] = Path(path).name
        (OUTPUT / page["file"]).write_bytes(files[path])
    (OUTPUT / "index.json").write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")
    if args.preview:
        preview(frames, args.preview)
    print(f"{len(frames)} frames, {len(files)} pages")


if __name__ == "__main__":
    main()
