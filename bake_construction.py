
"""Bake the two observable halves of each building's six construction stages."""

from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from bake_world import OUTPUT, bgra


SIZE = (48, 40)
X, GROUND = 24, 35
INK = "#51434b"
PEG = "#c9996d"
CORD = "#e7b982"
WORK = "#b07f5e"
PALE = "#e8c28d"


@dataclass(frozen=True)
class House:
    label: str
    width: int
    height: int
    raised: int
    profile: str
    material: str
    wall: str
    light: str
    shade: str
    roof: str
    roof_light: str


HOUSES = [
    House("小木屋", 12, 15, 2, "gable", "logs", "#ba8458", "#d99e68", "#986b56", "#81535d", "#a46a68"),
    House("石屋", 13, 16, 2, "stone", "stone", "#9a9ba6", "#b9bec3", "#77788d", "#676984", "#8888a3"),
    House("红砖屋", 12, 16, 2, "gable", "brick", "#ac6662", "#c77f72", "#8e5457", "#744956", "#9c5360"),
    House("树屋", 10, 22, 8, "gable", "logs", "#b98a5e", "#d4a571", "#8f654c", "#657a5c", "#8da575"),
    House("玻璃花房", 14, 15, 2, "round", "glass", "#80acb1", "#b0d2c4", "#6a929e", "#66869b", "#a3c7c5"),
    House("水边小屋", 12, 17, 4, "gable", "logs", "#75a1b0", "#a6bdc0", "#5e7e96", "#526d8a", "#7e9bb0"),
    House("雨亭", 14, 16, 2, "awning", "open", "#a57459", "#cb9e78", "#805e58", "#786684", "#a48ba1"),
    House("谷仓", 14, 18, 2, "barn", "logs", "#ab6760", "#cb8574", "#8c4f55", "#785059", "#a26469"),
    House("瞭望塔", 8, 25, 2, "spire", "tower", "#a48370", "#c5a28b", "#836c69", "#576681", "#7c87a1"),
    House("面包房", 13, 15, 2, "awning", "bakery", "#d0a77d", "#edc89a", "#ab826d", "#a95769", "#e29b95"),
    House("温室", 14, 14, 2, "round", "glass", "#729b7d", "#b4c796", "#577b71", "#587866", "#8eaa85"),
    House("工坊", 13, 16, 2, "slope", "workshop", "#8c918b", "#adb3a1", "#686f73", "#527979", "#79a29c"),
]


def background(art, h):
    x1, x2 = X - h.width, X + h.width
    art.ellipse((x1 - 7, GROUND - 2, x2 + 7, GROUND + 3), fill=(73, 62, 77, 51))
    if h.label == "水边小屋":
        art.polygon([(x1 - 5, 37), (x1 + 1, 33), (x2 + 4, 32), (x2 + 7, 37)],
                    fill=(76, 125, 142, 170))
        art.line((x1 - 1, 36, x1 + 7, 36), fill="#98b9be")
        art.line((x2 - 4, 34, x2 + 3, 34), fill="#98b9be")


def stack(art, x, y, fill, highlight, rows=2):
    for index in range(rows):
        left = x + index
        top = y - 2 * index
        art.rectangle((left, top, left + 6 - index, top + 1), fill=fill, outline=INK)
        art.line((left + 1, top, left + 4 - index, top), fill=highlight)


def materials(art, h, later, scattered=True):
    x1, x2 = X - h.width, X + h.width
    stack(art, x1 - 5, GROUND, h.shade, h.light, 2 + later)
    if scattered:
        stack(art, x2 + 1, GROUND - 1, WORK, PALE, 1 + later)
    if h.material in {"stone", "brick", "tower"}:
        for x, y in [(x1 - 1, 33), (x1 + 2, 35), (x2 - 1, 36)]:
            art.polygon([(x, y), (x + 2, y - 1), (x + 3, y + 1), (x + 1, y + 1)],
                        fill=h.wall, outline=h.shade)


def survey(art, h, later):
    x1, x2 = X - h.width, X + h.width
    if later:
        art.polygon([(x1 - 3, 33), (x1 + 3, 30), (x2 + 2, 30), (x2 + 4, 35),
                     (x1 - 4, 36)], fill="#9a765f", outline="#795e56")
        art.line((x1 + 2, 33, x2, 33), fill="#ac866b")
    art.line((x1, 33, x1 + 5, 29, x2 + 3, 30, x2, 34, x1, 33), fill=CORD)
    for x, y in [(x1, 33), (x1 + 5, 29), (x2 + 3, 30), (x2, 34)]:
        art.line((x, y + 1, x, y - 3), fill=INK)
        art.point((x, y - 3), fill=PEG)
    if later:
        materials(art, h, True)
    else:
        art.polygon([(X - 4, 32), (X - 1, 30), (X + 5, 31), (X + 3, 34)],
                    fill="#e7d5af", outline="#9f8772")
        art.line((X - 1, 31, X + 2, 32, X + 1, 33), fill=h.roof)


def floor(art, h, ratio=1):
    x1, x2 = X - h.width, X + h.width
    end = x1 + int(2 * h.width * ratio)
    fill = h.shade if h.material in {"stone", "brick", "glass", "tower"} else "#96705d"
    art.polygon([(x1 - 2, 33), (x1 + 3, 29), (x2 + 4, 29), (x2 + 2, 35),
                 (x1 - 2, 36)], fill="#96725e", outline="#795e54")
    art.polygon([(x1, 32), (x1 + 4, 29), (end + 3, 29), (end + 2, 34), (x1, 35)],
                fill=fill, outline=INK)
    art.line((x1 + 1, 32, end + 1, 32), fill=h.light)
    for joint in range(x1 + 3, end, 4):
        art.line((joint, 33, joint, 34), fill=INK)


def beam(art, points, highlight=PEG, width=2):
    art.line(points, fill=INK, width=width + 1)
    art.line(points, fill=highlight, width=width)


def trunk(art, h, grown):
    if h.label != "树屋":
        return
    art.polygon([(X + 4, 35), (X + 1, 18), (X + 6, 12), (X + 8, 22),
                 (X + 7, 34), (X + 10, 36)], fill="#805a46", outline=INK)
    art.line((X + 5, 34, X + 4, 23), fill="#aa794f")
    if grown:
        for x, y, r in [(X + 10, 12, 7), (X + 12, 8, 6), (X + 16, 12, 5), (X + 7, 8, 6)]:
            art.ellipse((x-r, y-r, x+r, y+r), fill="#536f56", outline="#465c4f")
        art.arc((X + 4, 4, X + 19, 17), 170, 280, fill="#9eb57b")


def raised_support(art, h):
    if h.raised <= 2:
        return
    x1, x2 = X - h.width, X + h.width
    sill = GROUND - h.raised
    for x in (x1 + 2, x2 - 2):
        beam(art, (x, GROUND + 1, x, sill), "#92735a")
    beam(art, (x1 + 2, GROUND - 1, x2 - 2, sill), "#ab7d57", 1)
    art.rectangle((x1 - 1, sill, x2 + 3, sill + 1), fill="#c29468", outline=INK)


def scaffold(art, h, side, upper):
    x = X + side * (h.width + 2)
    y = GROUND - h.height + 4
    low = GROUND - (h.height - h.raised) // 2
    beam(art, (x, GROUND + 1, x, y), "#b68b65", 1)
    beam(art, (x + side * 3, GROUND + 1, x + side * 3, low - 2), "#9c7057", 1)
    left, right = sorted((x - side * 4, x + side * 4))
    art.line((left, low, right, low), fill=PALE, width=2)
    art.line((x, low, x + side * 3, GROUND - 2), fill="#956c57")
    if upper:
        art.line((left, y + 2, x, y + 2), fill=PALE, width=2)
        art.line((x, y, x + side * 3, low), fill="#956c57")


def bare_frame(art, h, later):
    x1, x2 = X - h.width, X + h.width
    sill, lintel = GROUND - h.raised, GROUND - h.height
    posts = (x1 + 1, X, x2 - 1) if later else (x1 + 1, X)
    for i, x in enumerate(posts):
        y = lintel if i != 1 or later else lintel + 4
        beam(art, (x, sill, x, y), h.light)
        art.point((x, y), fill=PALE)
    end = x2 + 2 if later else X
    beam(art, (x1, lintel, end, lintel), PEG)
    if later:
        beam(art, (x1 + 1, sill - 1, X, lintel + 1, x2 - 1, sill - 1), h.shade, 1)
        art.line((x1 - 1, lintel, X + 2, lintel - 6, x2 + 2, lintel), fill=PALE)
        art.line((X + 1, lintel - 5, X + 1, lintel), fill=WORK)
        art.line((X - 5, lintel - 3, X - 5, lintel), fill=WORK)
    else:
        beam(art, (x1 + 2, sill - 1, X, lintel + 5), h.shade, 1)
        beam(art, (x2 - 1, sill, x2 - 1, sill - 4), PEG, 1)


def wall(art, h, later):
    x1, x2 = X - h.width + 1, X + h.width - 1
    top, bottom = GROUND - h.height, GROUND - h.raised
    if h.material == "open":
        for x in (x1, x2):
            beam(art, (x, top, x, bottom), h.light, 2)
        art.line((x1, bottom - 2, x2, bottom - 2), fill=h.shade)
        return
    right = x2 if later else X + 3
    art.rectangle((x1, top, right, bottom), fill=h.wall, outline=INK)
    if later:
        art.polygon([(x2, top), (x2 + 4, top - 3), (x2 + 4, bottom - 3), (x2, bottom)],
                    fill=h.shade, outline=INK)
        art.line((x2 + 1, top + 1, x2 + 1, bottom - 1), fill=h.wall)
    else:
        beam(art, (x2, top, x2, bottom), PEG, 1)
        art.line((right, bottom - 1, x2, top + 2), fill=WORK)
    if h.material == "glass":
        for x in range(x1 + 2, right, 5):
            art.rectangle((x, top + 3, min(right - 1, x + 2), bottom - 2), fill=h.light)
            art.line((x - 1, top + 1, x - 1, bottom - 1), fill=h.shade)
        art.line((x1 + 1, top + 6, right - 1, top + 6), fill=h.shade)
        art.line((x1 + 2, top + 2, x1 + 4, top + 1), fill="#d2ded2")
    else:
        spacing = 3 if h.material in {"stone", "brick"} else 4
        for y in range(top + 3, bottom, spacing):
            art.line((x1 + 1, y, right - 1, y), fill=h.shade)
            if h.material in {"stone", "brick"}:
                for x in range(x1 + 3 + (y % 2) * 2, right - 1, 5):
                    art.line((x, y - 2, x, y - 1), fill=h.shade)
            art.point((x1 + 2, y - 1), fill=h.light)
    art.line((x1 + 1, bottom - 1, right - 1, bottom - 1), fill=h.shade)


def roof_outline(h):
    left, right = X - h.width - 2, X + h.width + 3
    base = GROUND - h.height + 1
    profiles = {
        "gable": [(left, base), (X + 1, base - 8), (right, base)],
        "stone": [(left, base), (left, base - 4), (left + 4, base - 4), (left + 4, base - 6),
                  (X - 3, base - 6), (X - 3, base - 4), (X + 2, base - 4), (X + 2, base - 6),
                  (right - 3, base - 6), (right - 3, base - 4), (right, base - 4), (right, base)],
        "round": [(left, base), (left + 3, base - 5), (left + 9, base - 8), (X + 7, base - 8),
                  (right - 4, base - 5), (right, base)],
        "awning": [(left, base), (left + 4, base - 6), (right - 3, base - 6), (right, base)],
        "barn": [(left, base), (left + 3, base - 5), (X - 5, base - 9),
                 (X + 6, base - 9), (right - 3, base - 5), (right, base)],
        "spire": [(left, base), (left + 2, base - 5), (X + 1, base - 10),
                  (right - 2, base - 5), (right, base)],
        "slope": [(left, base), (left + 1, base - 7), (right - 2, base - 3), (right, base)],
    }
    return profiles[h.profile]


def roof(image, art, h, partial):
    outline = roof_outline(h)
    edge = outline[0][1]
    art.line(outline + [outline[0]], fill=INK)
    for x in range(outline[0][0] + 3, outline[-1][0], 4):
        art.line((x, edge, X + (x - X) // 2, edge - 4), fill=WORK)
    layer = Image.new("RGBA", SIZE)
    skin = ImageDraw.Draw(layer)
    skin.polygon(outline, fill=h.roof, outline=INK)
    skin.line(outline[1:-1], fill=h.roof_light)
    mask = layer.getchannel("A")
    pattern = Image.new("RGBA", SIZE)
    marks = ImageDraw.Draw(pattern)
    for y in range(edge - 2, edge - 9, -2):
        for x in range(outline[0][0] + y % 3, outline[-1][0], 5):
            marks.line((x, y, x + 2, y), fill=h.roof_light)
    if h.label == "面包房":
        for x in range(outline[0][0] + 2, outline[-1][0], 6):
            marks.line((x, edge - 1, x + 2, edge - 5), fill="#f2cbab", width=3)
    elif h.material == "glass":
        for x in range(outline[0][0] + 4, outline[-1][0], 5):
            marks.line((x, edge, X + (x - X) // 2, edge - 7), fill=h.light)
    layer.paste(Image.alpha_composite(layer, pattern), (0, 0), mask)
    if partial:
        right = X + 4
        image.alpha_composite(layer.crop((0, 0, right, SIZE[1])))
        art.line((right, edge - 4, right, edge), fill=PALE)
    else:
        image.alpha_composite(layer)
    art.line((outline[0][0], edge, outline[-1][0], edge), fill=INK, width=2)


def window(art, x, y, warm=False):
    art.rectangle((x-2, y-2, x+2, y+2), fill="#667e8a", outline=INK)
    art.line((x-1, y-1, x+1, y-1), fill="#f5cf87" if warm else "#b9d7cb")
    art.line((x, y-1, x, y+1), fill=PALE)
    art.line((x-2, y+2, x+2, y+2), fill="#b99272")


def fittings(art, h, full, warm=False):
    x1, x2 = X - h.width, X + h.width
    top, bottom = GROUND - h.height, GROUND - h.raised
    if h.material != "open":
        center = X - 1
        door = 4 if h.label in {"谷仓", "工坊"} else 3
        art.rectangle((center-door, bottom-7, center+door, bottom-1), fill=INK, outline=h.shade)
        art.rectangle((center-door+1, bottom-6, center+door-1, bottom-1), fill=h.shade)
        art.point((center+1, bottom-3), fill=PALE)
        if h.label == "谷仓":
            art.line((center-door+1, bottom-6, center+door-1, bottom-1), fill=PALE)
            art.line((center+door-1, bottom-6, center-door+1, bottom-1), fill=PALE)
        if h.material != "glass":
            window(art, x1 + 5, top + 6, warm)
            if full and h.width > 8:
                window(art, x2 - 5, top + 6, warm)
        if h.label == "瞭望塔":
            window(art, X, top + 5, warm)
            art.line((x1 - 2, top + 8, x2 + 3, top + 8), fill=PALE)
            for x in range(x1 - 1, x2 + 3, 3):
                art.line((x, top + 7, x, top + 10), fill=h.shade)
    if h.label in {"树屋", "瞭望塔"}:
        left = x2 + 2
        beam(art, (left, GROUND + 1, left, top + 9), WORK, 1)
        beam(art, (left + 3, GROUND + 1, left + 3, top + 9), WORK, 1)
        for y in range(GROUND - 1, top + 9, -3):
            art.line((left, y, left + 3, y), fill=PALE)
    if h.label == "雨亭":
        art.line((X, top + 1, X, top + 5), fill=INK)
        art.rectangle((X - 1, top + 4, X + 1, top + 7),
                      fill="#f9c173" if warm else "#c4a06e", outline="#79575d")
    if h.label == "水边小屋":
        art.polygon([(X - 3, GROUND - 1), (X + 6, GROUND - 1), (X + 10, GROUND + 2),
                     (X - 1, GROUND + 2)], fill="#b98462", outline=INK)
        art.line((X, GROUND, X + 7, GROUND), fill=PALE)
    if h.label == "面包房" and full:
        art.rectangle((X - 4, top + 1, X + 4, top + 3), fill="#e3b889", outline="#915e60")
        art.line((X - 2, top + 2, X + 2, top + 2), fill="#a76657")
    if h.label in {"小木屋", "红砖屋", "工坊"} and full:
        art.rectangle((x2 - 4, top - 9, x2 - 1, top - 3), fill=h.wall, outline=INK)
        art.line((x2 - 5, top - 9, x2, top - 9), fill=h.light)


def final_detail(art, h, later):
    x1, x2 = X - h.width, X + h.width
    if not later:
        art.rectangle((x2 + 2, GROUND - 4, x2 + 6, GROUND), fill=WORK, outline=INK)
        art.line((x2 + 3, GROUND - 2, x2 + 5, GROUND - 2), fill=PALE)
    else:
        for x, y in [(x1 - 2, 34), (x1 + 1, 36), (x2 + 3, 34)]:
            art.line((x, y, x, y - 2), fill="#67836a")
            art.point((x, y - 2), fill="#d7a58e" if h.material == "glass" else "#bdc181")
        if h.label == "工坊":
            beam(art, (x2 + 1, 33, x2 + 6, 33), PEG, 1)
            art.line((x2 + 3, 31, x2 + 5, 29), fill="#bfc3ba")
        if h.label == "谷仓":
            stack(art, x1 - 4, 35, "#b38a5f", "#e3b779", 2)


def render(h, stage, later):
    image = Image.new("RGBA", SIZE)
    art = ImageDraw.Draw(image)
    background(art, h)
    if stage == 0:
        survey(art, h, later)
        return image
    trunk(art, h, stage >= 2 or later)
    floor(art, h, 1 if stage >= 2 or later else 0.45)
    if stage == 1:
        materials(art, h, later)
        if later:
            raised_support(art, h)
            for x in (X - h.width + 1, X + h.width - 1):
                beam(art, (x, GROUND - h.raised, x, GROUND - h.raised - 4), PEG)
        return image
    raised_support(art, h)
    if stage in (2, 3):
        bare_frame(art, h, later or stage == 3)
        if stage == 3:
            roof(image, art, h, partial=not later)
        scaffold(art, h, -1 if stage == 2 else 1, stage == 3 or later)
        materials(art, h, not later, scattered=stage != 3)
        return image
    wall(art, h, later or stage == 5)
    roof(image, art, h, partial=False)
    fittings(art, h, later or stage == 5, warm=stage == 5 and later)
    if stage == 4:
        scaffold(art, h, 1, not later)
        materials(art, h, not later, scattered=False)
    else:
        final_detail(art, h, later)
    return image


def proof(sprites, destination):
    width, row_height = 1090, 92
    board = Image.new("RGBA", (width, 98 + row_height * len(HOUSES)), "#242733")
    art = ImageDraw.Draw(board)
    fonts = Path("C:/Windows/Fonts")
    try:
        title = ImageFont.truetype(str(fonts / "msyh.ttc"), 23)
        label = ImageFont.truetype(str(fonts / "msyh.ttc"), 14)
    except OSError:
        title, label = ImageFont.load_default(23), ImageFont.load_default(14)
    art.text((22, 15), "猫咪小屋  ·  建筑从备料到入住", font=title, fill="#e8d8bf")
    art.text((24, 49), "每种建筑的六个阶段各有两种画面。", font=label, fill="#bdb7b0")
    headings = ("备料", "地基", "框架", "屋顶", "收尾", "竣工")
    for column, heading in enumerate(headings):
        art.text((160 + column * 152, 73), heading, font=label, fill="#d7bc95")
    for row, house in enumerate(HOUSES):
        y = 98 + row * row_height
        art.rounded_rectangle((17, y+2, width-15, y+row_height-3), radius=8,
                              fill="#303444" if row % 2 else "#2d3140")
        art.text((25, y+34), house.label, font=label, fill="#efe0c3")
        for column in range(12):
            image = sprites[row * 12 + column].resize((72, 60), Image.Resampling.NEAREST)
            board.alpha_composite(image, (153 + column*76, y+11))
    board.save(destination)


def main():
    sprites = [render(house, stage, bool(later))
               for house in HOUSES for stage in range(6) for later in range(2)]
    frames = [bytes(bgra(sprite)) for sprite in sprites]
    assert len(set(frames)) == 144
    assert all(len(frame) == SIZE[0] * SIZE[1] * 4 for frame in frames)
    assert all(max(frame[i:i+3]) <= frame[i+3] for frame in frames for i in range(0, len(frame), 4))
    output = OUTPUT / "construction-progress.bgra"
    output.write_bytes(b"".join(frames))
    proof(sprites, Path(__file__).with_name("construction-progress-preview.png"))
    print(f"{output.name}: {len(frames)} distinct frames, {output.stat().st_size} bytes")


if __name__ == "__main__":
    main()
