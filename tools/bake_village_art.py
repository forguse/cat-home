"""Bake Willow Village's generated structure sources and resident labels."""
from pathlib import Path
import json

from PIL import Image, ImageDraw, ImageFont

from bake_premium_assets import ATLAS, Frame, NEAREST, pack, premultiply_bgra
from build_visitor_nameplates import nameplate


def largest_piece(im, *, coverage=82):
    """Keep the main opaque silhouette and its antialiasing, removing stray islands."""
    w, h = im.size
    alpha = list(im.getchannel("A").getdata())
    seen = bytearray(w * h)
    best = []
    for i in range(w * h):
        if seen[i] or alpha[i] < coverage:
            continue
        seen[i] = 1
        partly = [i]
        for at in partly:
            x = at % w
            adjacent = (at - 1 if x else -1, at + 1 if x + 1 < w else -1, at - w, at + w)
            for n in adjacent:
                if 0 <= n < w * h and not seen[n] and alpha[n] >= coverage:
                    seen[n] = 1
                    partly.append(n)
        if len(partly) > len(best):
            best = partly
    if not best:
        raise ValueError("No opaque silhouette in generated source")
    clean = Image.new("L", (w, h))
    pixels = clean.load()
    for at in best:
        pixels[at % w, at // w] = alpha[at]
    im.putalpha(clean)
    return im


def structure(master, key, cell, width, door_x):
    col, row = cell
    rect = (col * master.width // 3, row * master.height // 4,
            (col + 1) * master.width // 3, (row + 1) * master.height // 4)
    source = largest_piece(master.crop(rect))
    mask = source.getchannel("A").getbbox()
    source = source.crop(mask)
    height = round(source.height * width / source.width)
    door = round((door_x - mask[0]) * width / source.width)
    if not 0 <= door <= width:
        raise ValueError(f"{key}: door outside selected sprite")
    image = source.resize((width, height), Image.Resampling.LANCZOS)
    image.putalpha(image.getchannel("A").point(lambda a: 0 if a < 64 else (255 if a > 208 else a)))
    return Frame(key, image, (door, height), (), None)


def generated_stalls(master):
    for col, kind in enumerate(("produce", "cloth", "fish")):
        tile = master.crop((col * master.width // 3, 0, (col + 1) * master.width // 3, master.height))
        tile = largest_piece(tile)
        tile = tile.crop(tile.getchannel("A").getbbox())
        width, height = 96, 76
        tile = tile.resize((width, height), Image.Resampling.LANCZOS)
        tile.putalpha(tile.getchannel("A").point(lambda a: 0 if a < 64 else (255 if a > 208 else a)))
        yield Frame(f"village_{kind}_empty", tile, (width // 2, height), (), None)


def north_serving_stalls(master):
    for col, kind in enumerate(("meal", "cloth")):
        tile = largest_piece(master.crop((col * master.width // 2, 0, (col + 1) * master.width // 2, master.height)))
        tile = tile.crop(tile.getchannel("A").getbbox())
        # Keep the north-facing stall roof behind the staffed service tile.
        tile = tile.resize((91, 54), Image.Resampling.LANCZOS)
        tile.putalpha(tile.getchannel("A").point(lambda a: 0 if a < 64 else (255 if a > 208 else a)))
        yield Frame(f"village_{kind}_rear", tile, (46, 54), (), None)


def north_merchants(master):
    for col, (role, height) in enumerate((("cook", 38), ("weaver", 39))):
        tile = largest_piece(master.crop((col * master.width // 2, 0, (col + 1) * master.width // 2, master.height)))
        tile = tile.crop(tile.getchannel("A").getbbox())
        width = round(tile.width * height / tile.height)
        tile = tile.resize((width, height), Image.Resampling.LANCZOS)
        tile.putalpha(tile.getchannel("A").point(lambda a: 0 if a < 72 else (255 if a > 198 else a)))
        yield Frame(f"{role}_village_counter_north", tile, (width // 2, height), (), None)


def forest_crowns(master):
    cuts = (0, 630, 1505, master.width)
    species = (
        ("village_forest_laurel", 47, 60),
        ("village_forest_camphor", 64, 53),
        ("village_forest_hornbeam", 50, 56),
    )
    for i, (name, width, height) in enumerate(species):
        tile = largest_piece(master.crop((cuts[i], 0, cuts[i + 1], master.height)), coverage=145)
        tile = tile.crop(tile.getchannel("A").getbbox())
        tile = tile.resize((width, height), Image.Resampling.LANCZOS)
        tile.putalpha(tile.getchannel("A").point(lambda a: 0 if a < 70 else (255 if a > 211 else a)))
        yield Frame(name, tile, (width // 2, height), (), None)


def public_equipment(master):
    for col, (name, width) in enumerate((("village_mill", 25), ("village_drain", 16))):
        tile = largest_piece(master.crop((col * master.width // 2, 0, (col + 1) * master.width // 2, master.height)))
        tile = tile.crop(tile.getchannel("A").getbbox())
        height = round(tile.height * width / tile.width)
        tile = tile.resize((width, height), Image.Resampling.LANCZOS)
        tile.putalpha(tile.getchannel("A").point(lambda a: 0 if a < 70 else (255 if a > 211 else a)))
        yield Frame(name, tile, (width // 2, height), (), None)


def tools_and_materials(master):
    for index, (name, width) in enumerate((
        ("washing", 30), ("sorting", 32), ("drying", 40),
        ("sack", 18), ("materials", 40), ("toolrack", 24),
    )):
        row, col = divmod(index, 3)
        tile = largest_piece(master.crop((col * master.width // 3, row * master.height // 2,
                                          (col + 1) * master.width // 3, (row + 1) * master.height // 2)),
                             coverage=96)
        tile = tile.crop(tile.getchannel("A").getbbox())
        height = round(tile.height * width / tile.width)
        tile = tile.resize((width, height), Image.Resampling.LANCZOS)
        tile.putalpha(tile.getchannel("A").point(lambda a: 0 if a < 64 else (255 if a > 207 else a)))
        yield Frame("village_" + name, tile, (width // 2, height), (), None)


def overlay_kit(master, names, widths, coverage, *, rows=2):
    cell_width = master.width // 3
    cell_height = master.height // rows
    for i, (name, width) in enumerate(zip(names, widths)):
        row, col = divmod(i, 3)
        tile = largest_piece(master.crop((col * cell_width, row * cell_height,
                                          (col + 1) * cell_width, (row + 1) * cell_height)),
                             coverage=coverage)
        tile = tile.crop(tile.getchannel("A").getbbox())
        height = round(tile.height * width / tile.width)
        tile = tile.resize((width, height), Image.Resampling.LANCZOS)
        tile.putalpha(tile.getchannel("A").point(lambda a: 0 if a < 128 else (255 if a > 206 else a)))
        yield Frame("village_" + name, tile, (width // 2, height), (), None)


def resident_frames(master, rows):
    for index, resident in enumerate(rows):
        row, col = divmod(index, 5)
        tile = largest_piece(master.crop((col * master.width // 5, row * master.height // 4,
                                          (col + 1) * master.width // 5, (row + 1) * master.height // 4)),
                             coverage=100)
        tile = tile.crop(tile.getchannel("A").getbbox())
        height = [38, 36, 39, 38, 39, 38, 40, 38, 39, 38,
                  35, 38, 39, 38, 37, 37, 40, 36, 38, 37][index]
        width = round(tile.width * height / tile.height)
        if not 16 <= width <= 34:
            raise ValueError(f"Resident {resident['id']} has incorrect proportions ({width}×{height})")
        tile = tile.resize((width, height), Image.Resampling.LANCZOS)
        tile.putalpha(tile.getchannel("A").point(lambda a: 0 if a < 74 else (255 if a > 198 else a)))
        # Keep the authored head/body and move only the two foot marks for the
        # two walking phases. These frames retain the same exact ground anchor.
        key = resident["id"] + "_village_body"
        yield Frame(key, tile, (width // 2, height), (), None)
        for phase in range(2):
            walk = Image.new("RGBA", tile.size)
            walk.alpha_composite(tile.crop((0, 0, width, height - 7)), (0, 0))
            mid = width // 2
            left = tile.crop((0, height - 7, mid, height))
            right = tile.crop((mid, height - 7, width, height))
            walk.alpha_composite(left, (0 if phase else -1, height - 7 - phase))
            walk.alpha_composite(right, (mid + (0 if phase else 1), height - 8 + phase))
            yield Frame(f"{key}_walk_{phase}", walk, (width // 2, height), (), None)


def apple_phases(master):
    names = ("sapling", "dormant", "bud", "flower", "green_fruit", "after")
    sizes = ((24, 33), (56, 54), (57, 55), (58, 55), (56, 56), (56, 56))
    for index, (name, size) in enumerate(zip(names, sizes)):
        row, col = divmod(index, 3)
        tile = largest_piece(master.crop((col * master.width // 3, row * master.height // 2,
                                          (col + 1) * master.width // 3, (row + 1) * master.height // 2)),
                             coverage=60)
        tile = tile.crop(tile.getchannel("A").getbbox())
        tile = tile.resize(size, Image.Resampling.LANCZOS)
        tile.putalpha(tile.getchannel("A").point(lambda a: 0 if a < 49 else (255 if a > 205 else a)))
        yield Frame("village_apple_" + name, tile, (size[0] // 2, size[1]), (), None)


def main():
    source = ATLAS / "source" / "柳溪村建筑.png"
    master = Image.open(source).convert("RGBA")
    frames = [structure(master, key, cell, size, door) for key, cell, size, door in (
        ("village_post", (0, 0), 95, 200),
        ("village_seed", (1, 0), 92, 177),
        ("village_wood", (2, 0), 96, 149),
        ("village_pottery", (0, 1), 104, 206),
        ("village_textile", (1, 1), 100, 167),
        ("village_bakery", (2, 1), 102, 119),
        ("village_hall", (2, 2), 127, 195),
        ("village_depot", (2, 3), 125, 173),
    )]
    frames.extend(generated_stalls(Image.open(ATLAS / "source" / "柳溪村空摊.png").convert("RGBA")))
    frames.extend(north_serving_stalls(Image.open(ATLAS / "source" / "柳溪村南排背面.png").convert("RGBA")))
    frames.extend(north_merchants(Image.open(ATLAS / "source" / "柳溪村南排住户背影.png").convert("RGBA")))
    frames.extend(forest_crowns(Image.open(ATLAS / "source" / "柳溪村经营林树冠.png").convert("RGBA")))
    frames.extend(public_equipment(Image.open(ATLAS / "source" / "柳溪村石磨和排水格.png").convert("RGBA")))
    frames.extend(tools_and_materials(Image.open(ATLAS / "source" / "柳溪村作业道具.png").convert("RGBA")))
    apple = largest_piece(Image.open(ATLAS / "source" / "苹果树-成熟果与暖叶.png").convert("RGBA"))
    apple = apple.crop(apple.getchannel("A").getbbox())
    apple = apple.resize((56, 61), Image.Resampling.LANCZOS)
    apple.putalpha(apple.getchannel("A").point(lambda a: 0 if a < 65 else (255 if a > 208 else a)))
    frames.append(Frame("village_apple", apple, (28, 61), (), None))
    frames.extend(apple_phases(Image.open(ATLAS / "source" / "苹果树-其余六态.png").convert("RGBA")))
    frames.extend(overlay_kit(
        Image.open(ATLAS / "source" / "柳溪村旧屋改造施工.png").convert("RGBA"),
        ("relocation", "remodel", "tool_room", "grain_room", "public_foundation", "public_frame"),
        (23, 24, 23, 23, 43, 38), 150,
    ))
    frames.extend(overlay_kit(
        Image.open(ATLAS / "source" / "柳溪村院墙长椅芦苇.png").convert("RGBA"),
        ("garden_wall", "bench", "crate", "lantern", "hedge", "reeds"),
        (30, 27, 25, 18, 28, 35), 112,
    ))
    frames.extend(overlay_kit(
        Image.open(ATLAS / "source" / "柳溪村纪念物.png").convert("RGBA"),
        ("reward_paw", "reward_lantern", "reward_stones", "reward_card", "reward_cushion",
         "reward_tree", "reward_portrait", "reward_plaque", "reward_flags"),
        (11, 14, 15, 12, 16, 12, 15, 18, 22), 140, rows=3,
    ))
    # Small wooden notice frame uses the same ground contact as the source buildings.
    notice = Image.new("RGBA", (25, 27))
    draw = ImageDraw.Draw(notice)
    draw.rectangle((3, 5, 5, 25), fill="#6e4d35")
    draw.rectangle((19, 5, 21, 25), fill="#6e4d35")
    draw.rectangle((2, 24, 6, 25), fill="#503c30")
    draw.rectangle((18, 24, 22, 25), fill="#503c30")
    draw.polygon([(2, 4), (6, 1), (19, 1), (23, 4), (21, 7), (3, 7)], fill="#6d523a")
    draw.line((6, 1, 19, 1), fill="#d4a975")
    draw.rectangle((4, 6, 20, 19), fill="#9c7349", outline="#5c4332")
    draw.rectangle((7, 8, 17, 17), fill="#e3cc9b")
    draw.line((9, 11, 15, 11), fill="#87795d")
    draw.line((9, 13, 14, 13), fill="#87795d")
    draw.line((9, 15, 12, 15), fill="#87795d")
    frames.append(Frame("village_notice", notice, (12, 26), (), None))
    font = ImageFont.truetype("msyhbd.ttc", 36)
    root = ATLAS.parent.parent
    data = json.loads((root / "assets/maps/willow-village-192x128-r1.json").read_text(encoding="utf-8"))
    frames.extend(resident_frames(Image.open(ATLAS / "source" / "柳溪村二十住户.png").convert("RGBA"), data["residents"]))
    for row in data["residents"]:
        label = nameplate(row["name"].split("·", 1)[0], font)
        label = label.crop(label.getbbox())
        frames.append(Frame(row["id"] + "_village_label", label, (label.width // 2, label.height), (), None))
    # Height-first shelf packing leaves enough room for scenery while staying
    # within the formal 11 core + 1 wildlife + 4 village-page budget.
    frames.sort(key=lambda frame: (-frame.image.height, -frame.image.width, frame.id))
    index, pages = pack(frames)
    if len(pages) > 4:
        raise ValueError("Village bank exceeds the four-page budget")
    out = ATLAS / "village"
    out.mkdir(exist_ok=True)
    (out / "index.json").write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")
    for i, page in enumerate(pages):
        (out / f"page_{i:02}.bgra").write_bytes(premultiply_bgra(page))
    print(f"{len(index['frames'])} frames, {len(pages)} pages")


if __name__ == "__main__":
    main()
