"""Bake the original pixel art and assemble the local basketball content pack.

Run from any directory with Python and Pillow. The game loads the resulting
ZIP_STORED archive as data and does not run this tool.
"""

import argparse
from dataclasses import dataclass
from hashlib import sha3_256
import json
from pathlib import Path
import random
from zipfile import ZIP_STORED, ZipFile, ZipInfo

from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
PACK = ROOT / "content-packs" / "basketball"
PAGE_SIZE = 128
NEAREST = Image.Resampling.NEAREST
FOOT = (23, 43)
INK = "#493b43"


@dataclass(frozen=True)
class Outfit:
    cap: str
    cap_light: str
    cap_band: str
    skin: str
    shadow: str
    hair: str
    shirt: str
    sleeve: str
    apron: str
    seam: str
    boots: str


KEEPER = Outfit("#c49352", "#edc777", "#64845a", "#f4c394", "#c98663", "#875743",
                "#67835d", "#9aab69", "#426d62", "#81a98a", "#725345")
CARPENTER = Outfit("#d89842", "#f7c86e", "#9a6652", "#deaa7d", "#ae725e", "#655251",
                   "#aa694e", "#d28c5c", "#d9a050", "#f6cd79", "#69504b")


@dataclass(frozen=True)
class Pose:
    lead: tuple[int, int]
    rear: tuple[int, int]
    lean: int = 0
    crouch: int = 0
    lift: int = 0
    stride: int = 0
    expression: str = "neutral"


# The lead and rear hand coordinates are authored per pose. Held props use the
# lead coordinate, while airborne props are placed by the simulation.
POSES = {
    "invite": (Pose((31, 30), (15, 33)), Pose((36, 17), (16, 32), 1),
               Pose((37, 20), (17, 32), 1, expression="smile")),
    "accept": (Pose((30, 31), (16, 31)), Pose((31, 22), (17, 32), crouch=1),
               Pose((32, 20), (17, 31), expression="smile")),
    "decline": (Pose((30, 28), (16, 31)), Pose((36, 23), (16, 30), -2),
                Pose((34, 25), (15, 31), -1, expression="tired")),
    "take": (Pose((34, 32), (25, 32), 2, 2), Pose((35, 37), (29, 38), 4, 7),
             Pose((34, 29), (28, 30), 1, 2)),
    "carry": (Pose((33, 27), (27, 28), 1, stride=-2),
              Pose((34, 26), (28, 27), 1, lift=1, stride=1),
              Pose((33, 27), (27, 28), 1, stride=3)),
    "bounce": (Pose((36, 27), (15, 34), 1, 1, stride=-2),
               Pose((37, 31), (16, 33), 3, 4, stride=2),
               Pose((37, 26), (17, 34), 1, 1, stride=2)),
    "wait": (Pose((31, 29), (15, 30), 1, 3, stride=-2),
             Pose((34, 27), (17, 29), 2, 3, stride=1),
             Pose((31, 29), (15, 31), 1, 2, stride=2)),
    "pass": (Pose((32, 26), (27, 25), -1, 3, stride=-2),
             Pose((39, 25), (36, 26), 4, 2, stride=2),
             Pose((41, 26), (37, 28), 4, 1, stride=3)),
    "catch": (Pose((39, 26), (34, 25), 3, 2, stride=3),
              Pose((35, 27), (30, 27), 2, 4, stride=2),
              Pose((32, 26), (26, 27), 0, 2, stride=-1)),
    "shoot": (Pose((31, 22), (27, 24), 1, 4, stride=-1),
              Pose((31, 9), (27, 14), 0, 0, 2),
              Pose((35, 5), (30, 12), 1, 0, 4)),
    "pickup": (Pose((32, 33), (19, 34), 2, 3, stride=3),
               Pose((37, 39), (23, 35), 5, 9, stride=4),
               Pose((35, 29), (29, 30), 2, 2, stride=1)),
    "cheer": (Pose((33, 17), (13, 17), expression="smile"),
              Pose((34, 5), (13, 6), 0, 0, 4, 1, "smile"),
              Pose((36, 11), (12, 13), 0, 0, 2, 1, "smile")),
    "miss": (Pose((31, 25), (18, 26), -1, 1),
             Pose((36, 24), (12, 25), -1, 3, expression="tired"),
             Pose((30, 32), (16, 32), 0, 2, expression="smile")),
    "return": (Pose((33, 27), (27, 29), 1, 1), Pose((36, 34), (30, 35), 3, 5),
               Pose((36, 36), (25, 35), 3, 6)),
    "rest": (Pose((30, 32), (18, 35), 1, 8), Pose((32, 27), (17, 35), 1, 8),
             Pose((31, 33), (17, 35), 0, 8, expression="tired")),
}


@dataclass(frozen=True)
class Frame:
    id: str
    image: Image.Image
    anchor: tuple[int, int]
    body: str = "prop"
    hand: tuple[int, int] | None = None


def segment(draw, joints, color, highlight):
    draw.line(joints, fill=INK, width=6, joint="curve")
    draw.line(joints, fill=color, width=4, joint="curve")
    x, y = joints[0]
    draw.point((x, y - 1), fill=highlight)


def player(pose, outfit, taller=False):
    image = Image.new("RGBA", (48, 48))
    draw = ImageDraw.Draw(image)
    center = 23 + pose.lean
    hips = 34 + pose.crouch // 2 - pose.lift
    chest = 24 + pose.crouch - pose.lift - int(taller)
    base = FOOT[1] - pose.lift
    left_foot, right_foot = 18 - pose.stride, 27 + pose.stride

    draw.ellipse((12, 41, 33, 45), fill=(52, 41, 47, 56 if pose.lift else 95))
    for x, end, offset in ((center - 3, left_foot, -1), (center + 3, right_foot, 0)):
        sole = base + offset
        knee = ((x + end) // 2 + pose.crouch // 3, hips + (sole - hips) // 2)
        segment(draw, [(x, hips), knee, (end, sole - 1)], outfit.boots, outfit.seam)
        draw.rounded_rectangle((end - 2, sole - 3, end + 4, sole), radius=1,
                               fill=outfit.boots, outline=INK)
        draw.line((end, sole - 2, end + 3, sole - 2), fill="#ae8463")

    def arm(shoulder, palm, rear=False):
        x, y = palm
        elbow = ((shoulder[0] + x) // 2 + (-1 if rear else 1),
                 (shoulder[1] + y) // 2 + (1 if y < shoulder[1] else 2))
        segment(draw, [shoulder, elbow, palm], outfit.shirt if rear else outfit.sleeve,
                outfit.sleeve)
        draw.ellipse((x - 2, y - 2, x + 2, y + 2), fill=outfit.shadow, outline=INK)
        draw.line((x - 1, y, x + 1, y - 1), fill=outfit.skin)

    arm((center - 5, chest + 1), pose.rear, rear=True)
    draw.polygon([(center - 5, chest), (center + 4, chest - 1), (center + 7, chest + 2),
                  (center + 7, hips - 1), (center + 4, hips + 2), (center - 6, hips + 1),
                  (center - 7, chest + 5)], fill=outfit.shirt, outline=INK)
    draw.polygon([(center - 3, chest + 1), (center - 1, chest + 2),
                  (center - 1, chest + 5), (center + 3, chest + 5),
                  (center + 3, chest + 1), (center + 5, chest + 1),
                  (center + 5, hips - 1), (center - 3, hips + 1)], fill=outfit.apron)
    draw.line((center - 2, chest + 6, center + 3, chest + 6), fill=outfit.seam)
    draw.point((center + 3, chest + 3), fill="#f4d493")

    top = chest - 13
    draw.polygon([(center - 5, top + 4), (center - 3, top + 1), (center + 5, top + 1),
                  (center + 7, top + 5), (center + 8, top + 8), (center + 6, top + 12),
                  (center + 1, top + 14), (center - 4, top + 11)], fill=outfit.shadow, outline=INK)
    draw.polygon([(center - 2, top + 4), (center + 5, top + 4), (center + 7, top + 8),
                  (center + 5, top + 11), (center + 1, top + 12), (center - 2, top + 10)],
                 fill=outfit.skin)
    draw.line([(center - 4, top + 5), (center - 4, top + 9), (center - 2, top + 11)],
              fill=outfit.hair, width=2)
    draw.point((center + 2, top + 7), fill=INK)
    draw.point((center + 6, top + 7), fill=INK)
    if pose.expression == "smile":
        draw.arc((center + 2, top + 8, center + 6, top + 11), 0, 180, fill=outfit.shadow)
    elif pose.expression == "tired":
        draw.line((center + 3, top + 10, center + 5, top + 9), fill=outfit.shadow)
    else:
        draw.point((center + 5, top + 10), fill=outfit.shadow)

    if not taller:
        draw.line([(center - 5, top + 6), (center - 7, top + 9),
                   (center - 7, top + 13), (center - 5, top + 15)],
                  fill=outfit.hair, width=3)
        draw.point((center - 7, top + 11), fill="#e0b365")
    else:
        draw.line((center - 3, top + 10, center - 1, top + 12), fill=outfit.hair)

    brim = center + (9 if taller else 10)
    draw.polygon([(center - 8, top + 4), (center - 5, top + 2), (center - 5, top),
                  (center - 2, top - 3), (center + 3, top - 3), (center + 6, top - 1),
                  (center + 7, top + 2), (brim, top + 3), (brim - 1, top + 5),
                  (center - 6, top + 6)], fill=outfit.cap, outline=INK)
    draw.line([(center - 3, top), (center, top - 2), (center + 3, top - 1)],
              fill=outfit.cap_light)
    draw.line((center - 4, top + 3, center + 6, top + 2), fill=outfit.cap_band)
    draw.line((center - 6, top + 4, brim - 1, top + 3), fill=outfit.cap_light)
    arm((center + 5, chest + 2), pose.lead)
    return image


def basketball(rotation):
    image = Image.new("RGBA", (11, 11))
    draw = ImageDraw.Draw(image)
    draw.ellipse((0, 0, 10, 10), fill="#5e3937")
    draw.ellipse((1, 1, 9, 9), fill="#c4653e")
    draw.ellipse((1, 1, 8, 8), fill="#e1834b")
    draw.arc((1, 1, 9, 9), 192, 285, fill="#f6bc72")
    draw.arc((1, 1, 9, 9), 7, 98, fill="#ad5036")
    seams = (
        (((5, 1), (4, 3), (4, 6), (6, 9)), ((1, 5), (3, 4), (6, 4), (9, 6))),
        (((3, 1), (5, 3), (7, 4), (9, 4)), ((1, 7), (4, 7), (6, 5), (7, 2))),
        (((1, 3), (4, 4), (6, 6), (7, 9)), ((2, 7), (5, 6), (6, 3), (6, 1))),
    )
    for seam in seams[rotation]:
        draw.line(seam, fill="#743f35")
    draw.point((3, 2), fill="#ffcc84")
    return image


def court():
    image = Image.new("RGBA", (96, 64))
    draw = ImageDraw.Draw(image)
    shape = [(4, 3), (91, 3), (95, 9), (95, 57), (91, 61), (4, 61), (0, 56), (0, 9)]
    draw.polygon(shape, fill="#bb8568", outline="#976a59")
    draw.line((4, 60, 91, 60), fill="#8a6157")
    draw.rectangle((5, 7, 90, 55), fill="#c59474")
    draw.rectangle((7, 10, 88, 54), fill="#c99b7a")
    specks = random.Random(711)
    for _ in range(360):
        x, y = specks.randrange(3, 93), specks.randrange(7, 58)
        draw.point((x, y), fill=specks.choice(("#c19275", "#ca9a79", "#cf9f7f", "#bf9172")))
    draw.rectangle((5, 7, 90, 55), outline="#e5cba5")
    draw.rectangle((29, 7, 51, 27), outline="#ecd1a8")
    draw.arc((29, 18, 51, 36), 0, 180, fill="#ecd1a8")
    draw.arc((12, -12, 68, 46), 15, 165, fill="#ead0a8")
    draw.arc((30, 49, 51, 61), 180, 360, fill="#e6cba2")
    for x, y in ((6, 31), (27, 7), (50, 55), (29, 21), (86, 55), (90, 45)):
        draw.point((x, y), fill="#bb8568")
    return image


def hoop(stage):
    image = Image.new("RGBA", (46, 58))
    draw = ImageDraw.Draw(image)
    draw.ellipse((16, 50, 43, 56), fill=(63, 45, 50, 67))
    draw.polygon([(24, 48), (36, 48), (40, 52), (39, 55), (21, 55), (20, 52)],
                 fill="#9a8373", outline="#695750")
    draw.line((23, 51, 36, 51), fill="#cfb49b")
    if stage == 0:
        return image
    draw.polygon([(29, 51), (29, 19), (25, 15), (24, 11), (29, 11), (34, 17),
                  (34, 51), (32, 53)], fill="#57747a", outline="#4c535a")
    draw.line([(30, 51), (30, 19), (26, 14)], fill="#8ca0a0")
    for y in (46, 49):
        draw.point((32, y), fill="#b8b5a0")
    if stage == 1:
        return image
    draw.polygon([(12, 5), (37, 3), (40, 5), (40, 19), (13, 23), (11, 21)],
                 fill="#e5cb9d", outline="#6d5954")
    draw.polygon([(12, 7), (36, 5), (37, 18), (13, 21)], fill="#f1dfb3")
    draw.line((15, 20, 36, 17), fill="#d4ae83")
    draw.line([(19, 18), (19, 12), (29, 11), (29, 17)], fill="#a26857")
    draw.line([(17, 24), (20, 35), (27, 35), (30, 24)], fill="#ab9889")
    for x in (18, 22, 26):
        draw.line((x, 26, min(29, x + 6), 34), fill="#e3caaa")
        draw.line((x + 4, 25, max(19, x), 34), fill="#efdbb3")
    draw.arc((15, 21, 32, 28), 180, 360, fill="#6f443d", width=2)
    draw.arc((16, 22, 31, 27), 180, 360, fill="#e6945b")
    return image


def rim_front():
    image = Image.new("RGBA", (18, 8))
    draw = ImageDraw.Draw(image)
    draw.arc((0, 0, 17, 7), 0, 180, fill="#713f3a", width=2)
    draw.arc((1, 1, 16, 6), 0, 180, fill="#ce7049")
    draw.line((7, 6, 11, 6), fill="#eea36d")
    return image


def rack():
    image = Image.new("RGBA", (22, 17))
    draw = ImageDraw.Draw(image)
    draw.ellipse((0, 13, 21, 16), fill=(59, 44, 45, 75))
    draw.polygon([(1, 6), (7, 3), (19, 3), (21, 6), (20, 13), (3, 15), (1, 12)],
                 fill="#a87451", outline="#654c48")
    draw.polygon([(4, 6), (8, 4), (18, 4), (19, 6), (16, 8), (5, 9)], fill="#6c4e44")
    draw.line((3, 9, 19, 7), fill="#e0aa71", width=2)
    draw.line((4, 12, 19, 10), fill="#d09860")
    for x in (5, 17):
        draw.line((x, 8, x, 13), fill="#7c594b")
    return image


def cup():
    image = Image.new("RGBA", (9, 9))
    draw = ImageDraw.Draw(image)
    draw.rectangle((5, 3, 8, 6), fill="#9cb8b1", outline="#526b70")
    draw.polygon([(0, 1), (6, 1), (6, 6), (5, 8), (2, 8), (1, 7)],
                 fill="#84a7a7", outline="#526b70")
    draw.line((1, 1, 5, 1), fill="#e8d9ae")
    draw.line((2, 3, 2, 6), fill="#c8d7c8")
    return image


def all_frames():
    frames = [Frame("court", court(), (0, 0))]
    for name, stage in (("hoop_footing", 0), ("hoop_post", 1), ("hoop", 2)):
        frames.append(Frame(name, hoop(stage), (31, 53)))
    frames.extend((Frame("rim_front", rim_front(), (8, 3)), Frame("ball_rack", rack(), (11, 13)),
                   Frame("cup", cup(), (4, 4))))
    frames.extend(Frame(f"ball_{i}", basketball(i).resize((9, 9), NEAREST), (4, 4))
                  for i in range(3))
    player_size = (34, 34)
    foot = scale_point(FOOT, (48, 48), player_size)
    for animation, poses in POSES.items():
        for index, pose in enumerate(poses):
            for suffix, outfit in (("", KEEPER), ("_alt", CARPENTER)):
                id_ = f"{animation}_{index}{suffix}"
                sprite = player(pose, outfit, bool(suffix))
                hand = scale_point(pose.lead, sprite.size, player_size)
                body = "human_male" if suffix else "human_female"
                frames.append(Frame(id_, sprite.resize(player_size, NEAREST), foot, body, hand))
    return frames


def scale_point(point, source, target):
    return tuple(p * d // s for p, s, d in zip(point, source, target))


def premultiply_bgra(image):
    data = bytearray()
    for r, g, b, a in image.getdata():
        data.extend((b * a // 255, g * a // 255, r * a // 255, a))
    return bytes(data)


def pack_pages(frames):
    pages = [Image.new("RGBA", (PAGE_SIZE, PAGE_SIZE))]
    index = []
    x, y, row_height = 0, 0, 0
    for frame in frames:
        # Keep the anchor in the crop, even if an aerial pose lifts the shoes.
        left, top, right, bottom = frame.image.getbbox()
        points = [frame.anchor]
        if frame.hand is not None:
            points.append(frame.hand)
        for ax, ay in points:
            left, top = min(left, ax), min(top, ay)
            right, bottom = max(right, ax + 1), max(bottom, ay + 1)
        sprite = frame.image.crop((left, top, right, bottom))
        width, height = sprite.size
        if x + width > PAGE_SIZE:
            x, y, row_height = 0, y + row_height + 1, 0
        if y + height > PAGE_SIZE:
            pages.append(Image.new("RGBA", (PAGE_SIZE, PAGE_SIZE)))
            x, y, row_height = 0, 0, 0
        pages[-1].paste(sprite, (x, y))
        entry = {
            "id": frame.id, "page": f"page_{len(pages) - 1:02}",
            "rect": [x, y, width, height],
            "anchor": [frame.anchor[0] - left, frame.anchor[1] - top], "body": frame.body,
        }
        if frame.hand is not None:
            entry["hand_anchor"] = [frame.hand[0] - left, frame.hand[1] - top]
        index.append(entry)
        x += width + 1
        row_height = max(row_height, height)

    data = {}
    page_index = []
    for number, page in enumerate(pages):
        id_ = f"page_{number:02}"
        path = f"assets/{id_}.bgra"
        pixels = premultiply_bgra(page)
        data[path] = pixels
        page_index.append({"id": id_, "file": path, "sha3_256": sha3_256(pixels).hexdigest(),
                           "width": PAGE_SIZE, "height": PAGE_SIZE})
    data["assets/index.json"] = (json.dumps({"pages": page_index, "frames": index},
                                             ensure_ascii=False, indent=2) + "\n").encode()
    return data


def place(image, frame, xy, flip=False):
    art = frame.image
    x, y = frame.anchor
    if flip:
        art = art.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
        x = art.width - 1 - x
    image.alpha_composite(art, (xy[0] - x, xy[1] - y))


PREVIEW_LABELS = {
    "invite": "邀请", "accept": "接受", "decline": "婉拒", "take": "取球", "carry": "带球赴场",
    "bounce": "运球", "wait": "等候接球", "pass": "传球", "catch": "接球", "shoot": "投篮",
    "pickup": "捡球", "cheer": "庆祝", "miss": "没进", "return": "归还篮球", "rest": "坐下喝水",
}


def preview(frames, destination):
    by_id = {frame.id: frame for frame in frames}
    result = Image.new("RGB", (1060, 1067), "#202825")
    draw = ImageDraw.Draw(result)
    header = ImageFont.truetype("msyhbd.ttc", 29)
    label = ImageFont.truetype("msyh.ttc", 16)
    title = ImageFont.truetype("msyhbd.ttc", 18)
    draw.text((24, 18), "庭院篮球 · 像素素材预览", font=header, fill="#f2dfb9")
    draw.text((24, 57), "动作试摆 · 实际球路与命中由运行时决定", font=label, fill="#a7b4a6")
    scene = Image.new("RGBA", (320, 106), "#75866d")
    garden = ImageDraw.Draw(scene)
    grain = random.Random(241)
    for _ in range(800):
        x, y = grain.randrange(scene.width), grain.randrange(scene.height)
        garden.point((x, y), fill=grain.choice(("#6e8067", "#829275", "#879479")))
    place(scene, by_id["court"], (112, 42))
    place(scene, by_id["hoop"], (160, 54))
    place(scene, by_id["shoot_1"], (125, 81))
    place(scene, by_id["wait_1_alt"], (186, 72), flip=True)
    place(scene, by_id["ball_1"], (139, 34))
    place(scene, by_id["rim_front"], (152, 25))
    place(scene, by_id["ball_rack"], (98, 88))
    result.paste(scene.resize((1012, 318), NEAREST), (24, 88))

    for number, (animation, name) in enumerate(PREVIEW_LABELS.items()):
        column, row = number % 3, number // 3
        x, y = 24 + column * 340, 426 + row * 97
        draw.rounded_rectangle((x, y, x + 329, y + 89), radius=7, fill="#344038")
        draw.text((x + 12, y + 10), name, font=title, fill="#f0dfb9")
        phases = Image.new("RGBA", (115, 47))
        for phase in range(3):
            sprite = by_id[f"{animation}_{phase}" + ("_alt" if animation in ("accept", "catch", "rest") else "")]
            foot = (18 + phase * 38, 44)
            place(phases, sprite, foot)
            hand = (foot[0] + sprite.hand[0] - sprite.anchor[0],
                    foot[1] + sprite.hand[1] - sprite.anchor[1])
            if animation in ("take", "carry", "return", "catch", "pickup", "pass", "shoot", "bounce"):
                ball = hand
                if animation == "bounce" and phase == 1:
                    ball = (hand[0], foot[1] - 4)
                if animation == "pickup" and phase == 0:
                    ball = (hand[0] + 3, foot[1] - 4)
                if animation == "catch" and phase == 0:
                    ball = (hand[0] + 6, hand[1] - 2)
                if animation == "shoot" and phase == 2:
                    ball = (hand[0] + 4, hand[1] - 4)
                if animation == "pass" and phase == 2:
                    ball = (hand[0] + 4, hand[1] - 3)
                place(phases, by_id[f"ball_{phase}"], ball)
            elif animation == "rest":
                place(phases, by_id["cup"], hand)
        phases = phases.resize((184, 75), NEAREST)
        result.paste(phases, (x + 119, y + 6), phases)
    draw.rounded_rectangle((24, 919, 1034, 1043), radius=7, fill="#344038")
    for column, (ids, text) in enumerate((
        (("ball_0", "ball_1", "ball_2"), "篮球转动"),
        (("hoop_footing", "hoop_post", "hoop"), "篮架施工阶段"),
        (("ball_rack",), "球筐"),
        (("cup",), "水杯"),
    )):
        x = (46, 292, 639, 854)[column]
        draw.text((x, 929), text, font=title, fill="#f0dfb9")
        for index, id_ in enumerate(ids):
            icon = by_id[id_].image
            height = 58 if id_.startswith("hoop") else icon.height
            sprite = icon.resize((icon.width * 2, height * 2), NEAREST) if height < 20 else icon
            position = (x + index * (54 if height == 58 else 36), 1034 - sprite.height)
            result.paste(sprite, position, sprite)
    destination.parent.mkdir(parents=True, exist_ok=True)
    result.save(destination)


def assemble(destination, asset_files):
    names = ("manifest.json", "LICENSE.txt", "rules/items.json", "rules/facilities.json",
             "scenes/prepare.json", "scenes/friendly_game.json")
    entries = {name: (PACK / name).read_bytes() for name in names}
    entries.update(asset_files)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with ZipFile(destination, "w", compression=ZIP_STORED) as archive:
        for path, data in sorted(entries.items()):
            archive.writestr(ZipInfo(path, (2026, 1, 1, 0, 0, 0)), data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=PACK / "庭院篮球.catpack")
    parser.add_argument("--preview", type=Path)
    parser.add_argument("--art-only", action="store_true")
    args = parser.parse_args()
    frames = all_frames()
    asset_files = pack_pages(frames)
    if args.preview:
        preview(frames, args.preview)
    if not args.art_only:
        assemble(args.output, asset_files)
    print(f"{len(frames)} frames, {len(asset_files) - 1} pages, "
          f"{sum(len(data) for data in asset_files.values())} asset bytes")


if __name__ == "__main__":
    main()
