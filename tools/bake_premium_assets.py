"""Import approved raster masters into the optional two-texel BGRA atlas."""

import argparse
from dataclasses import dataclass
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from build_basketball_pack import ROOT, premultiply_bgra


ATLAS = ROOT / "assets" / "premium"
PAGE_SIDE = 256
PAGE_LIMIT = 16
NEAREST = Image.Resampling.NEAREST
COVERAGE = [0 if a < 32 else 255 if a >= 192 else a for a in range(256)]
ACTOR_SCALE = 38 / 216


@dataclass(frozen=True)
class Import:
    id: str
    master: str
    bounds: tuple[int, int, int, int]
    anchor: tuple[int, int]
    max_size: tuple[int, int]
    scale: float | None = None
    opaque: bool = False
    aliases: tuple[str, ...] = ()
    hand: tuple[int, int] | None = None
    keep_anchor_margin: bool = False


# Bounds isolate each drawn object, including ones that straddle a nominal
# sheet grid. Anchors identify the central outer edge of the lowest door step.
BUILDINGS = (
    Import("west_house_0", "西屋-v2.png", (82, 102, 1146, 1153), (606, 1141), (96, 104)),
    Import("east_house_0", "建筑同族-v2.png", (47, 53, 480, 511), (248, 506), (96, 104)),
    Import("kitchen", "建筑同族-v2.png", (497, 53, 1014, 505), (695, 497), (96, 104)),
    Import("storehouse", "建筑同族-v2.png", (1040, 70, 1495, 501), (1259, 492), (96, 104)),
    Import("workshop", "建筑同族-v2.png", (27, 565, 463, 971), (240, 963), (64, 70)),
    Import("west_house_3", "建筑同族-v2.png", (475, 536, 998, 972), (686, 964), (106, 108)),
    Import("east_house_3", "建筑同族-v2.png", (1033, 532, 1518, 972), (1214, 962), (106, 108)),
)


def cell_bounds(columns, rows, col, row, inset=0):
    return (col * columns + inset, row * rows + inset,
            (col + 1) * columns - inset, (row + 1) * rows - inset)


def landscape():
    keys = ("tree_oak", "tree_birch", "tree_apple", "tree_cherry", "tree_maple", "bush_berry",
            "flowers_wild", "bush_herb", "rocks_moss", "tree_stump", "logs", "well_stone")
    # The wood and branches identify trunk and root pivots more precisely than
    # a canopy midpoint. Ground props are anchored to their base contact.
    local = ((176, 328), (187, 336), (175, 332), (151, 329), (171, 315), (177, 299),
             (157, 299), (136, 299), (186, 296), (166, 297), (183, 294), (181, 315))
    for i, (key, anchor) in enumerate(zip(keys, local)):
        if key == "tree_apple":
            yield Import(key, "苹果树-自然树形.png", (0, 0, 1254, 1254), (617, 1144), (56, 72))
            continue
        row, col = divmod(i, 4)
        bounds = cell_bounds(362, 362, col, row)
        max_size = (56, 72) if i < 5 else (40, 28) if i < 8 else (32, 32)
        yield Import(key, "景观-v3-透明.png", bounds,
                     (bounds[0] + anchor[0], bounds[1] + anchor[1]), max_size)


def characters():
    # Keep one scaling factor across both characters and all poses. In the
    # source cells both straight bodies are approximately 216 pixels tall.
    groups = ("walk", "hoe_till", "water", "harvest")
    feet = (
        ((129, 229), (135, 230), (132, 230), (119, 229), (128, 230), (123, 230)),
        ((119, 231), (123, 231), (117, 231), (103, 232), (113, 232), (117, 232)),
        ((131, 231), (135, 229), (133, 231), (122, 230), (127, 232), (124, 232)),
        ((122, 232), (122, 231), (122, 232), (108, 232), (107, 233), (112, 233)),
    )
    walk_hands = (((171, 151), (166, 156), (167, 147)),
                  ((170, 149), (164, 161), (169, 149)))
    for row, anchors in enumerate(feet):
        sex = "female" if row < 2 else "male"
        for col, anchor in enumerate(anchors):
            group = groups[(row % 2) * 2 + col // 3]
            phase = col % 3
            id_ = f"{sex}_{group}_{phase}"
            bounds = cell_bounds(256, 256, col, row)
            hand = None
            if group == "walk":
                hx, hy = walk_hands[row // 2][phase]
                hand = (bounds[0] + hx, bounds[1] + hy)
            yield Import(id_, "主角工作-A.png", bounds,
                         (bounds[0] + anchor[0], bounds[1] + anchor[1]), (50, 48), scale=ACTOR_SCALE,
                         hand=hand)


def idle_and_drink():
    groups = (
        ("主角工作-B-v1.png", "idle", "female", 0, ((135, 231), (135, 231), (135, 231)),
         ((171, 165), (172, 165), (172, 164))),
        ("主角工作-B-v1.png", "idle", "male", 2, ((133, 230), (133, 230), (133, 230)),
         ((165, 161), (165, 161), (165, 161))),
        ("主角生活-C.png", "drink", "female", 1, ((130, 240), (130, 240), (130, 240)), None),
        ("主角生活-C.png", "drink", "male", 3, ((128, 236), (128, 236), (128, 236)), None),
    )
    for master, action, sex, row, feet, hands in groups:
        for col, (x, y) in enumerate(feet):
            bounds = cell_bounds(256, 256, col, row)
            hand = None if hands is None else (bounds[0] + hands[col][0], bounds[1] + hands[col][1])
            yield Import(f"{sex}_{action}_{col}", master, bounds, (bounds[0] + x, bounds[1] + y),
                         (50, 48), scale=ACTOR_SCALE, hand=hand)


def orange_cat():
    actions = ("idle", "walk", "eat", "drink", "rest", "sleep", "sit", "hop")
    feet = (
        ((120, 225), (120, 225), (121, 225), (120, 226), (120, 226), (119, 226)),
        ((120, 226), (120, 226), (121, 226), (115, 224), (115, 224), (115, 224)),
        ((119, 216), (119, 216), (119, 216), (116, 215), (116, 215), (116, 215)),
        ((118, 221), (118, 221), (118, 221), (120, 225), (121, 225), (120, 225)),
    )
    for row, anchors in enumerate(feet):
        for col, (x, y) in enumerate(anchors):
            action = actions[row * 2 + col // 3]
            bounds = cell_bounds(256, 256, col, row)
            yield Import(f"cat_{action}_{col % 3}", "橘子-v1.png", bounds,
                         (bounds[0] + x, bounds[1] + y), (25, 23), scale=18 / 191,
                         keep_anchor_margin=action == "hop")


def layered_work():
    actions = ("carry", "axe", "hammer", "sow")
    feet = (
        ((132, 231), (134, 231), (133, 231), (114, 232), (120, 231), (126, 233)),
        ((121, 234), (120, 233), (121, 233), (109, 234), (113, 234), (114, 234)),
        ((132, 231), (134, 231), (132, 231), (118, 232), (120, 234), (124, 232)),
        ((123, 232), (124, 232), (122, 233), (104, 231), (106, 231), (108, 232)),
    )
    carry_hands = (((171, 158), (171, 154), (171, 147)),
                   ((174, 151), (169, 156), (173, 145)))
    for row, anchors in enumerate(feet):
        sex = "female" if row < 2 else "male"
        for col, (x, y) in enumerate(anchors):
            action = actions[(row % 2) * 2 + col // 3]
            phase = col % 3
            bounds = cell_bounds(256, 256, col, row)
            hand = None
            if action == "carry":
                hx, hy = carry_hands[row // 2][phase]
                hand = (bounds[0] + hx, bounds[1] + hy)
            yield Import(f"{sex}_{action}_{phase}", "主角动作-B-分层.png", bounds,
                         (bounds[0] + x, bounds[1] + y), (50, 48), scale=ACTOR_SCALE, hand=hand)


def layered_life():
    sheets = (
        ("主角生活-C-透明.png", ("cook", "eat", "rest", "store"), (
            (131, 231), (132, 234), (134, 231), (125, 231),
            (134, 234), (133, 231), (125, 232), (127, 231),
        )),
        ("主角钓鱼-D-分层.png", ("fish", "fish_success", "fish_fail", "sleep"), (
            (123, 231), (119, 231), (123, 231), (127, 195),
            (126, 231), (123, 231), (126, 231), (128, 195),
        )),
        ("主角照料-E.png", (None, None, "hoe_clear", "greet"), (
            (120, 231), (125, 233), (116, 230), (125, 230),
            (120, 231), (125, 233), (114, 230), (124, 233),
        )),
    )
    for master, actions, anchors in sheets:
        for row in range(4):
            sex = "female" if row < 2 else "male"
            for col in range(6):
                group = (row % 2) * 2 + col // 3
                action = actions[group]
                if action is None:
                    continue
                x, y = anchors[group + (4 if row >= 2 else 0)]
                bounds = cell_bounds(256, 256, col, row)
                # For fish, the second hotspot is the fixed rod tip. The host
                # draws the line from this point to the actual water position.
                hand = (bounds[0] + 220, bounds[1] + (62 if row < 2 else 64)) if action == "fish" else None
                yield Import(f"{sex}_{action}_{col % 3}", master, bounds,
                             (bounds[0] + x, bounds[1] + y), (50, 48), scale=ACTOR_SCALE, hand=hand)


def south_bank():
    for i, (key, anchor, size) in enumerate((
        ("guesthouse_3", (253, 499), (88, 92)), ("depot_3", (342, 495), (83, 86)),
        ("market_3", (252, 488), (96, 96)), ("stall_3", (223, 468), (69, 72)),
        ("well_3", (251, 468), (57, 64)), ("garden_3", (254, 453), (87, 72)),
    )):
        row, col = divmod(i, 3)
        bounds = cell_bounds(512, 512, col, row)
        yield Import(key, "南岸设施-v2.png", bounds, (bounds[0] + anchor[0], bounds[1] + anchor[1]), size)


def building_upgrades():
    buildings = ("west_house", "kitchen", "storehouse", "workshop")
    sizes = ((98, 106), (94, 99), (96, 98), (64, 70))
    # The Level 4 warehouse is serviced at the open bay. Its small side
    # door would shift the facade into the neighboring structure's space.
    definitions = (
        ("建筑升级-三级.png", 3, (
            ((0, 0, 512, 512), (256, 486)),
            ((1010, 0, 1536, 512), (1230, 480)), ((0, 512, 512, 1024), (270, 968)),
            ((512, 512, 1024, 1024), (717, 964)),
        ), ("structure_upgrade_scaffold", (1282, 960), (42, 45))),
        ("建筑升级-四级.png", 4, (
            ((0, 0, 512, 512), (231, 495)),
            ((1008, 0, 1536, 512), (1247, 480)), ((0, 512, 512, 1024), (260, 977)),
            ((512, 512, 1024, 1024), (758, 978)),
        ), ("scaffold_frame", (1264, 979), (66, 68))),
    )
    for master, level, geometry, (scaffold, anchor, size) in definitions:
        for name, (bounds, door), limit in zip(buildings, geometry, sizes):
            yield Import(f"{name}_level_{level}", master, bounds, door, limit)
        yield Import(scaffold, master, (1024, 512, 1536, 1024), anchor, size)
    yield Import("east_house_level_3", "东屋-紧凑升级.png", (0, 0, 1254, 627),
                 (626, 600), (92, 102))
    yield Import("east_house_level_4", "东屋-紧凑升级.png", (0, 627, 1254, 1254),
                 (627, 1214), (94, 104))


def housing():
    phases = ("clear", "foundation", "frame", "roof", "fit_out", "move_in")
    plans = (
        ("single", "单人屋-六阶段.png", 64 / 411, (
            (247, 490), (747, 495), (1244, 495),
            (246, 977), (742, 977), (1241, 980),
        )),
        ("shared", "双人屋-六阶段.png", 96 / 465, (
            (256, 478), (758, 482), (1271, 482),
            (260, 976), (762, 976), (1275, 976),
        )),
    )
    # The physical foundation determines the scale for the whole sequence.
    # Keeping that scale preserves relative depth and the growth of the roof.
    for plan, master, scale, doors in plans:
        for i, (phase, door) in enumerate(zip(phases, doors)):
            row, col = divmod(i, 3)
            bounds = cell_bounds(512, 512, col, row)
            yield Import(f"housing_{plan}_{phase}", master, bounds, door, (108, 100),
                         scale=scale, keep_anchor_margin=True)


def yard_objects():
    # Scene props use feet; movable props use the grip or carried center.
    objects = (
        ("shared_table", (193, 228), (34, 22)),
        ("shared_stool", (195, 233), (13, 14)),
        ("prop_bowl", (189, 223), (13, 9)),
        ("prop_bowl_full", (191, 223), (13, 9)),
        ("prop_meal", (193, 202), (11, 9)),
        ("prop_stove", (189, 240), (21, 23)),
        ("prop_wood", (202, 148), (19, 14)),
        ("housing_moving_box", (194, 160), (20, 15)),
        ("prop_hoe", (244, 93), (23, 21)),
        ("prop_axe", (224, 141), (20, 13)),
        ("prop_hammer", (229, 147), (18, 15)),
        ("prop_water_can", (224, 92), (18, 16)),
        ("prop_seeds", (181, 97), (12, 12)),
        ("prop_crop", (191, 52), (18, 18)),
        ("prop_fish", (199, 126), (16, 7)),
        ("prop_pail", (189, 31), (14, 16)),
    )
    for i, (key, point, size) in enumerate(objects):
        row, col = divmod(i, 4)
        bounds = cell_bounds(384, 256, col, row)
        yield Import(key, "庭院物件-统一材质.png", bounds,
                     (bounds[0] + point[0], bounds[1] + point[1]), size)


def garden_beds():
    stages = ("cleared", "tilled", "seed", "sprout", "growing", "mature")
    for i, stage in enumerate(stages):
        row, col = divmod(i, 3)
        bounds = cell_bounds(512, 512, col, row)
        yield Import(f"plot_{stage}", "种植-六阶段.png", bounds,
                     (bounds[0] + 257, bounds[1] + 258), (18, 14), scale=18 / 436)


def fill_bowl():
    for row, sex in enumerate(("female", "male")):
        for col in range(3):
            bounds = cell_bounds(512, 512, col, row)
            # The lowest water cluster in the pouring pose falls about eight
            # world pixels east of the contact foot, matching the real bowl.
            yield Import(f"{sex}_fill_bowl_{col}", "添水碗-分层.png", bounds,
                         (bounds[0] + 251, bounds[1] + 445), (37, 40), scale=38 / 360)


def terrain():
    keys = ("ground_grass", "ground_path", "ground_soil", "ground_water_shallow", "ground_water", "ground_rock")
    for i, key in enumerate(keys):
        row, col = divmod(i, 3)
        bounds = cell_bounds(512, 512, col, row, inset=2)
        yield Import(key, "地形-v1.png", bounds, bounds[:2], (64, 64), opaque=True)


@dataclass(frozen=True)
class Frame:
    id: str
    image: Image.Image
    anchor: tuple[int, int]
    aliases: tuple[str, ...]
    hand: tuple[int, int] | None


def load_frame(source, entry, masters):
    if entry.master not in masters:
        with Image.open(source / entry.master) as master:
            if master.mode != "RGBA" and not entry.opaque:
                raise ValueError(f"{entry.master}: expected an RGBA master")
            masters[entry.master] = master.convert("RGBA")
    master = masters[entry.master]
    left, top, right, bottom = entry.bounds
    if not (0 <= left < right <= master.width and 0 <= top < bottom <= master.height):
        raise ValueError(f"{entry.id}: source rectangle lies outside its master")
    image = master.crop(entry.bounds)
    mask = image.getchannel("A")
    if not entry.opaque and min(mask.getpixel(point) for point in ((0, 0), (image.width - 1, 0),
                                              (0, image.height - 1), (image.width - 1, image.height - 1))) >= 32:
        raise ValueError(f"{entry.id}: source has no clear transparent corner")
    rect = mask.point(lambda a: 255 if a >= 8 else 0).getbbox()
    if rect is None:
        raise ValueError(f"{entry.id}: no visible pixels")
    if entry.keep_anchor_margin:
        ax, ay = entry.anchor[0] - left, entry.anchor[1] - top
        rect = (min(rect[0], ax), min(rect[1], ay), max(rect[2], ax), max(rect[3], ay))
    image = image.crop(rect)
    image.putalpha(image.getchannel("A").point(COVERAGE))
    scale = entry.scale if entry.scale is not None else min(entry.max_size[0] / image.width, entry.max_size[1] / image.height)
    size = tuple(max(1, round(side * scale)) for side in image.size)
    if any(side > limit for side, limit in zip(size, entry.max_size)):
        raise ValueError(f"{entry.id}: {size} exceeds maximum {entry.max_size}")

    def scale_anchor(point):
        x, y = point[0] - left - rect[0], point[1] - top - rect[1]
        if not (0 <= x <= image.width and 0 <= y <= image.height):
            raise ValueError(f"{entry.id}: anchor lies outside its cropped image")
        return round(x * size[0] / image.width), round(y * size[1] / image.height)

    hand = scale_anchor(entry.hand) if entry.hand is not None else None
    return Frame(entry.id, image.resize(size, NEAREST), scale_anchor(entry.anchor), entry.aliases, hand)


def pack(frames):
    pages = [Image.new("RGBA", (PAGE_SIDE, PAGE_SIDE))]
    manifest = {"pages": [], "frames": []}
    x, y, row = 0, 0, 0
    for frame in frames:
        w, h = frame.image.size
        if w > PAGE_SIDE or h > PAGE_SIDE:
            raise ValueError(f"{frame.id}: frame exceeds the page dimensions")
        if x + w > PAGE_SIDE:
            x, y, row = 0, y + row + 1, 0
        if y + h > PAGE_SIDE:
            pages.append(Image.new("RGBA", (PAGE_SIDE, PAGE_SIDE)))
            x, y, row = 0, 0, 0
        pages[-1].paste(frame.image, (x, y))
        for id_ in (frame.id, *frame.aliases):
            entry = {"id": id_, "page": len(pages) - 1, "rect": [x, y, w, h], "anchor": list(frame.anchor)}
            if frame.hand is not None:
                entry["hand_anchor"] = list(frame.hand)
            manifest["frames"].append(entry)
        x += w + 1
        row = max(row, h)
    if len(pages) > PAGE_LIMIT:
        raise ValueError(f"premium atlas uses {len(pages)} pages; limit is {PAGE_LIMIT}")
    manifest["pages"] = [{"width": PAGE_SIDE, "height": PAGE_SIDE} for _ in pages]
    return manifest, pages


def proof(frames, path):
    columns = 4
    cell_w, cell_h = 186, 167
    rows = (len(frames) + columns - 1) // columns
    sheet = Image.new("RGB", (columns * cell_w, 3 * rows * cell_h), "#e7e1cc")
    font = ImageFont.truetype("msyh.ttc", 11)
    for i, frame in enumerate(frames):
        for j, background in enumerate(("#8ba77a", "#bc966a", "#344b54")):
            left, top = i % columns * cell_w, (i // columns * 3 + j) * cell_h
            panel = Image.new("RGBA", (cell_w - 4, cell_h - 4), background)
            draw = ImageDraw.Draw(panel)
            draw.text((5, 2), f"{frame.id}  {frame.image.width}×{frame.image.height}", font=font,
                      fill="#f6ead8" if j == 2 else "#302a26")
            width, height = (round(side * 1.5) for side in frame.image.size)
            obj = frame.image.resize((width, height), NEAREST)
            at = ((panel.width - width) // 2, panel.height - height - 2)
            panel.alpha_composite(obj, at)
            sheet.paste(panel, (left, top))
    path.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=ATLAS / "source")
    parser.add_argument("--out", type=Path, default=ATLAS)
    parser.add_argument("--preview", type=Path)
    args = parser.parse_args()
    masters = {}
    frames = [load_frame(args.source, entry, masters)
              for entry in (*BUILDINGS, *landscape(), *characters(), *terrain(), *idle_and_drink(),
                            *orange_cat(), *layered_work(), *layered_life(), *south_bank(),
                            *building_upgrades(), *housing(), *yard_objects(), *garden_beds(), *fill_bowl())]
    index, pages = pack(frames)
    args.out.mkdir(parents=True, exist_ok=True)
    for i, page in enumerate(pages):
        (args.out / f"page_{i:02}.bgra").write_bytes(premultiply_bgra(page))
    (args.out / "index.json").write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")
    if args.preview:
        proof(frames, args.preview)
    print(f"{len(index['frames'])} frame IDs, {len(frames)} distinct frames, {len(pages)} pages, "
          f"{len(pages) * PAGE_SIDE ** 2 * 4} bytes")


if __name__ == "__main__":
    main()
