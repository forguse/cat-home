"""Compose a small pixel-art yard from CC0 Kenney and LuizMelo assets."""
from io import BytesIO
from pathlib import Path
from zipfile import ZipFile
import json

from PIL import Image, ImageDraw, ImageOps


ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "sources"
ASSETS = ROOT / "assets"
SIZE = (160, 100)
NEAREST = Image.Resampling.NEAREST

YARD = [
    (8, 64), (15, 64), (15, 57), (23, 57), (23, 52), (136, 52),
    (136, 55), (145, 55), (145, 62), (152, 62), (152, 81), (147, 81),
    (147, 87), (140, 87), (140, 91), (19, 91), (19, 87), (13, 87),
    (13, 81), (8, 81),
]

GRASS_PALETTE = {
    (134, 195, 107): (155, 179, 128),
    (118, 182, 96): (132, 164, 110),
    (104, 169, 81): (115, 147, 98),
}

CAT_PALETTE = {
    (155, 117, 33): (230, 149, 80),
    (136, 103, 28): (186, 106, 68),
    (136, 103, 29): (186, 106, 68),
    (214, 194, 149): (255, 231, 190),
    (107, 80, 18): (116, 64, 59),
    (181, 152, 88): (250, 184, 112),
    (149, 112, 31): (216, 127, 70),
    (120, 91, 24): (157, 84, 58),
    (80, 81, 33): (55, 44, 54),
    (172, 122, 10): (244, 161, 77),
}


def pixel_asset(data):
    return Image.open(BytesIO(data)).convert("RGBA")


def tiles():
    with ZipFile(SOURCE / "tiny-town.zip") as archive:
        return {
            number: pixel_asset(archive.read(f"Tiles/tile_{number:04d}.png"))
            for number in (
                0, 1, 2, 4, 5, 16, 17, 28, 29, 43, 44, 45, 46,
                48, 49, 50, 60, 62, 63, 75, 83, 84, 85,
            )
        }


def lawn_texture(tile):
    recolored = tile.copy()
    pixels = []
    for red, green, blue, alpha in tile.getdata():
        color = GRASS_PALETTE.get((red, green, blue))
        if color is None and green > red and green > blue:
            color = (min(255, red + 20), max(0, green - 14), min(255, blue + 16))
        pixels.append((*(color or (red, green, blue)), alpha))
    recolored.putdata(pixels)
    return recolored


def make_scene(tile_set, *, water_bowl=True):
    scene = Image.new("RGBA", SIZE)
    draw = ImageDraw.Draw(scene)
    draw.ellipse((9, 77, 153, 98), fill=(47, 38, 42, 48))
    draw.polygon([(x, y + 5) for x, y in YARD], fill="#514450")
    draw.polygon([(x, y + 3) for x, y in YARD], fill="#98776b")
    draw.polygon(YARD, fill="#49564d")

    grass = Image.new("RGBA", SIZE)
    for row in range(3, 6):
        for column in range(10):
            index = 2 if (row * 3 + column) % 13 == 0 else (row + column) % 2
            grass.alpha_composite(lawn_texture(tile_set[index]), (column * 16, row * 16))
    mask = Image.new("L", SIZE)
    ImageDraw.Draw(mask).polygon(
        [(x + (1 if x < 80 else -1), y + (1 if y < 73 else -1)) for x, y in YARD],
        fill=255,
    )
    scene.paste(grass, (0, 0), mask)
    draw = ImageDraw.Draw(scene)
    draw.line((24, 91, 133, 91), fill="#bb9880")
    for x in (24, 40, 57, 92, 113, 137):
        draw.line((x, 93, x + 3, 93), fill="#775d60")

    # Two small evergreens frame the cottage and the cat's blanket.
    scene.alpha_composite(tile_set[16], (118, 42))
    scene.alpha_composite(tile_set[4], (131, 52))
    scene.alpha_composite(tile_set[28], (17, 51))
    for index, x in enumerate((112, 126, 140)):
        scene.alpha_composite(tile_set[44 + index], (x, 55))

    # Kenney Tiny Town roof and facade arranged into one cottage.
    draw.ellipse((29, 59, 84, 77), fill="#6e8c65")
    for row, tile_row in enumerate(((48, 49, 50), (60, 63, 62), (84, 85, 75))):
        for column, tile_id in enumerate(tile_row):
            scene.alpha_composite(tile_set[tile_id], (27 + column * 16, 20 + row * 16))
    draw.rectangle((33, 58, 37, 63), fill="#f4c786")
    draw.line((35, 58, 35, 63), fill="#a86b55")
    draw.line((33, 60, 37, 60), fill="#a86b55")

    path_tile = lawn_texture(tile_set[43])
    for x, y in ((46, 66), (48, 75), (61, 73)):
        scene.alpha_composite(path_tile, (x, y))
    scene.alpha_composite(tile_set[29], (20, 68))
    scene.alpha_composite(tile_set[17], (78, 63))
    scene.alpha_composite(tile_set[83], (133, 74))

    # Small striped blanket and water bowl are the cat's own corner.
    draw = ImageDraw.Draw(scene)
    draw.rounded_rectangle((93, 69, 127, 83), radius=3, fill="#71556c")
    draw.rectangle((96, 71, 124, 81), fill="#d49ba3")
    draw.line((98, 73, 122, 73), fill="#ecc0b7")
    draw.line((98, 79, 122, 79), fill="#b97588")
    for x in (98, 103, 118, 123):
        draw.line((x, 82, x, 84), fill="#f0cfad")
    if water_bowl:
        draw.rectangle((79, 79, 91, 83), fill="#505266")
        draw.rectangle((81, 77, 89, 81), fill="#6f8eb2")
        draw.line((81, 78, 89, 78), fill="#e0edf1")
        draw.line((83, 80, 87, 80), fill="#b6d6e1")
    for x, y in ((22, 83), (71, 58), (89, 62), (130, 85), (141, 68)):
        draw.point((x, y + 1), fill="#637d54")
        draw.line((x - 1, y, x + 1, y), fill="#fff0aa")
        draw.point((x, y - 1), fill="#ffcc92")
    return scene


class CatSprites:
    def __init__(self):
        self.image = Image.open(SOURCE / "cat.png").convert("RGBA")
        self.image.putdata([
            (*CAT_PALETTE.get((red, green, blue), (red, green, blue)), alpha)
            for red, green, blue, alpha in self.image.getdata()
        ])
        self.frames = json.loads((SOURCE / "cat.json").read_text(encoding="utf-8"))["frames"]

    def draw(self, scene, name, index, x, y, *, face_left=False):
        frame = self.frames[f"pet_cat_{name}_{index}"]["frame"]
        left, top = frame["x"], frame["y"]
        sprite = self.image.crop((left + 16, top + 43, left + 48, top + 64)).resize((48, 32), NEAREST)
        if face_left:
            sprite = ImageOps.mirror(sprite)
        scene.alpha_composite(sprite, (x - 24, y - 29))


def put_heart(frame, x, y, color):
    draw = ImageDraw.Draw(frame)
    draw.polygon(
        [(x, y + 2), (x + 2, y), (x + 4, y + 1), (x + 6, y), (x + 8, y + 2),
         (x + 8, y + 4), (x + 4, y + 8), (x, y + 4)],
        fill=color,
    )
    draw.point((x + 2, y + 2), fill="#fff2d7")


def put_sleep_symbol(frame, x, y):
    draw = ImageDraw.Draw(frame)
    draw.line([(x, y), (x + 4, y), (x, y + 4), (x + 4, y + 4)], fill="#ebe8d1", width=1)


def idle_frames(scene, cat):
    for kind, index in (("idle", 0), ("idle", 1), ("idle", 2), ("idle", 3), ("blink", 0), ("idle", 0)):
        frame = scene.copy()
        cat.draw(frame, kind, index, 110, 78)
        yield frame


def sleep_frames(scene, cat):
    for index in range(4):
        frame = scene.copy()
        cat.draw(frame, "sleep", (index // 2) % 2, 110, 78)
        put_sleep_symbol(frame, 116 + index, 65 - index * 2)
        if index > 0:
            put_sleep_symbol(frame, 126 + index, 58 - index * 2)
        yield frame


def walk_frames(scene, cat):
    for index in range(16):
        frame = scene.copy()
        step = index if index < 8 else 15 - index
        cat.draw(frame, "walk", index % 4, 110 - step * 3, 78, face_left=index < 8)
        yield frame


def react_frames(scene, cat):
    for index in range(8):
        frame = scene.copy()
        cat.draw(frame, "sit", index % 2, 110, 78)
        put_heart(frame, 116 + (index // 3), 59 - index, "#df8496")
        yield frame


def encode_bgra(image):
    result = bytearray(SIZE[0] * SIZE[1] * 4)
    for index, (red, green, blue, alpha) in enumerate(image.getdata()):
        start = index * 4
        result[start:start + 4] = bytes((blue * alpha // 255, green * alpha // 255, red * alpha // 255, alpha))
    return result


def main():
    ASSETS.mkdir(exist_ok=True)
    scene = make_scene(tiles())
    cat = CatSprites()
    sequences = {
        "idle": list(idle_frames(scene, cat)),
        "sleep": list(sleep_frames(scene, cat)),
        "walk": list(walk_frames(scene, cat)),
        "react": list(react_frames(scene, cat)),
    }
    for name, frames in sequences.items():
        with (ASSETS / f"{name}.bgra").open("wb") as output:
            for frame in frames:
                output.write(encode_bgra(frame))
    sequences["idle"][0].resize((480, 300), NEAREST).save(ASSETS / "preview.png")


if __name__ == "__main__":
    main()
