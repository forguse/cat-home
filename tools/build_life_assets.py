"""Bake original household action frames and reusable props into native BGRA pages."""

import argparse
import json
from math import hypot
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from build_basketball_pack import (
    CARPENTER, FOOT, INK, KEEPER, NEAREST, ROOT, Frame, Pose, pack_pages, place,
    player, scale_point,
)


DESTINATION = ROOT / "assets" / "life"
AUTHOR_SIZE = (48, 48)
PLAYER_SIZE = (34, 34)
WOOD = "#865f4d"
WOOD_LIGHT = "#d7a46f"
METAL = "#99aca7"
METAL_LIGHT = "#d9d6b9"
WATER = "#9acac6"
GREEN = "#7f9d63"


def p(lead, rear=(16, 32), lean=0, crouch=0, lift=0, stride=0, expression="neutral"):
    return Pose(lead, rear, lean, crouch, lift, stride, expression)


# Phases are preparation, contact, and recovery. Held items for walk/carry/rest
# are separate frames, so the renderer can follow the current inventory.
MOTIONS = {
    "idle": (p((30, 32)), p((30, 31), crouch=1), p((31, 32))),
    "walk": (p((29, 34), (17, 28), stride=-3), p((32, 31), (15, 34), lift=1), p((34, 28), stride=3)),
    "carry": (p((34, 28), (28, 27), 1, stride=-3), p((34, 27), (28, 26), 1, lift=1),
              p((34, 28), (28, 27), 1, stride=3)),
    "hoe_clear": (p((31, 24), (24, 27), -1, 1, stride=-2), p((37, 36), (30, 32), 5, 6, stride=3),
                  p((35, 31), (26, 29), 2, 3, stride=2)),
    "hoe_till": (p((34, 22), (28, 28), 1, 1, stride=-2), p((37, 37), (32, 33), 5, 6, stride=2),
                 p((35, 35), (30, 32), 4, 4, stride=2)),
    "sow": (p((31, 31), (23, 33), 1, 2), p((38, 40), (26, 37), 5, 9, stride=3),
            p((33, 34), (25, 36), 3, 5, stride=2)),
    "water": (p((33, 28), (28, 33), 1, 1), p((35, 32), (31, 36), 4, 5, stride=2),
              p((33, 30), (28, 35), 2, 3)),
    "harvest": (p((32, 33), (23, 35), 2, 3), p((38, 40), (26, 35), 4, 8, stride=3),
                p((35, 30), (26, 33), 2, 3)),
    "store": (p((34, 29), (27, 29), 1, 2), p((37, 34), (30, 34), 4, 5), p((35, 34), (28, 33), 3, 4)),
    "cook": (p((36, 23), (30, 33), 2, 1), p((37, 29), (30, 35), 3, 2), p((36, 26), (30, 33), 2, 1)),
    "eat": (p((33, 32), (29, 34), 0, 3), p((32, 24), (27, 36), 1, 4),
            p((32, 31), (27, 35), 0, 3, expression="smile")),
    "fill_bowl": (p((34, 28), (28, 34), 1, 2), p((37, 35), (31, 39), 5, 9, stride=2),
                  p((35, 32), (29, 36), 2, 4)),
    "drink": (p((32, 32), (18, 34)), p((32, 23), (18, 33), 1, 1),
              p((32, 30), (19, 34), 1, expression="smile")),
    "axe": (p((25, 13), (22, 20), -2, 1, stride=-2), p((39, 32), (34, 29), 4, 4, stride=3),
            p((34, 29), (29, 29), 2, 2, stride=2)),
    "hammer": (p((32, 12), (29, 32), 1, 1), p((38, 29), (32, 34), 4, 4), p((34, 21), (31, 32), 2, 1)),
    "rest": (p((30, 34), (18, 34), 1, 5), p((30, 37), (18, 39), 0, 9),
             p((29, 37), (17, 38), -1, 10, expression="tired")),
    "sleep": (p((30, 37), (21, 39), 0, 8, expression="tired"), p((31, 38), (20, 40), 0, 10),
              p((30, 37), (21, 40), 0, 9)),
    "fish": (p((32, 26), (28, 31), 1), p((34, 23), (29, 29), 1, 1), p((33, 25), (29, 31), 1)),
    "fish_success": (p((33, 24), (26, 29), 1), p((34, 14), (26, 25), 0, lift=1, expression="smile"),
                     p((32, 19), (28, 29), 1, expression="smile")),
    "fish_fail": (p((34, 24), (28, 31), 1), p((33, 17), (21, 31), -1, 1, expression="tired"),
                  p((33, 29), (16, 34), 0, 2, expression="tired")),
}


def long_tool(draw, kind, grip, end):
    """Paint a small tool with a head normal to its shaft."""
    draw.line((grip, end), fill=INK, width=3)
    draw.line((grip, end), fill=WOOD_LIGHT)
    vx, vy = end[0] - grip[0], end[1] - grip[1]
    length = max(1, hypot(vx, vy))
    nx, ny = round(-4 * vy / length), round(4 * vx / length)
    x, y = end
    if kind == "hoe":
        draw.line(((x - nx, y - ny), (x + nx, y + ny)), fill=INK, width=4)
        draw.line(((x - nx, y - ny), (x + nx, y + ny)), fill=METAL, width=2)
        draw.point((x - nx, y - ny), fill=METAL_LIGHT)
    elif kind == "axe":
        blade = [(x - nx, y - ny), (x - nx + 3, y - ny + 2), (x + nx + 1, y + ny + 2),
                 (x + nx, y + ny), (x + 1, y)]
        draw.polygon(blade, fill=METAL, outline=INK)
        draw.line((blade[1], blade[2]), fill=METAL_LIGHT)
    elif kind == "hammer":
        draw.line(((x - nx, y - ny), (x + nx, y + ny)), fill=INK, width=6)
        draw.line(((x - nx, y - ny), (x + nx, y + ny)), fill=METAL, width=3)
        draw.point((x + nx, y + ny), fill=METAL_LIGHT)
    else:
        draw.line((grip, end), fill=WOOD_LIGHT)
        draw.point(end, fill=METAL_LIGHT)


def vessel(draw, kind, grip, tilt=False):
    x, y = grip
    if kind in ("water_can", "pail"):
        draw.arc((x - 4, y - 2, x + 5, y + 6), 178, 355, fill=INK, width=2)
        draw.polygon([(x - 5, y + 3), (x + 4, y + 2), (x + 6, y + 5),
                      (x + 4, y + 9), (x - 4, y + 10)], fill=METAL, outline=INK)
        draw.line((x - 3, y + 5, x + 3, y + 4), fill=METAL_LIGHT)
        if kind == "water_can":
            spout = (x + 11, y + (7 if tilt else 1))
            draw.line(((x + 4, y + 7), (x + 8, y + 5), spout), fill=INK, width=3)
            draw.line(((x + 4, y + 7), (x + 8, y + 5), spout), fill=METAL_LIGHT)
            return spout
    elif kind == "basket":
        draw.arc((x - 7, y - 2, x + 7, y + 7), 178, 355, fill=WOOD, width=2)
        draw.polygon([(x - 9, y + 4), (x + 9, y + 4), (x + 7, y + 10),
                      (x - 6, y + 10)], fill=WOOD_LIGHT, outline=INK)
        draw.line((x - 7, y + 6, x + 7, y + 6), fill=WOOD)
        for stripe in (-4, 0, 4):
            draw.line((x + stripe, y + 7, x + stripe, y + 9), fill="#a97a54")
    elif kind == "cup":
        draw.rectangle((x + 3, y - 2, x + 6, y + 2), fill=METAL, outline=INK)
        draw.polygon([(x - 4, y - 4), (x + 3, y - 4), (x + 3, y + 2), (x + 1, y + 4),
                      (x - 3, y + 4)], fill="#82a9a8", outline=INK)
        draw.line((x - 2, y - 3, x + 2, y - 3), fill=METAL_LIGHT)
    else:
        draw.ellipse((x - 6, y - 2, x + 7, y + 4), fill="#e6cd9f", outline=INK)
        draw.arc((x - 6, y, x + 7, y + 5), 0, 180, fill="#8d725e")
    return x + 5, y + 5


def crop(draw, point):
    x, y = point
    draw.ellipse((x - 3, y - 2, x + 4, y + 3), fill="#d29151", outline=INK)
    draw.line([(x, y - 1), (x + 1, y - 5), (x + 4, y - 4)], fill=GREEN, width=2)
    draw.line((x - 2, y, x + 1, y - 1), fill="#f1be76")


def fish(draw, point):
    x, y = point
    draw.polygon([(x - 7, y + 2), (x - 4, y), (x - 6, y - 4), (x - 2, y - 2),
                  (x + 2, y - 4), (x + 6, y - 2), (x + 7, y), (x + 4, y + 3),
                  (x + 1, y + 3), (x - 2, y + 1)], fill="#7fa1a0", outline=INK)
    draw.line((x, y - 1, x + 3, y - 2), fill="#c8d4b6")
    draw.point((x + 4, y), fill=INK)


def add_tools(image, animation, phase, pose):
    under = Image.new("RGBA", image.size)
    back = ImageDraw.Draw(under)
    hand = pose.lead
    if animation in ("hoe_clear", "hoe_till", "axe", "hammer"):
        kind, ends = {
            "hoe_clear": ("hoe", ((40, 16), (45, 41), (44, 38))),
            "hoe_till": ("hoe", ((39, 9), (43, 43), (45, 41))),
            "axe": ("axe", ((17, 6), (43, 26), (41, 30))),
            "hammer": ("hammer", ((33, 5), (43, 35), (41, 14))),
        }[animation]
        end = ends[phase]
        long_tool(back, kind, hand, end)
        if phase == 1:
            x, y = end
            for dx, dy in ((-3, -2), (1, 2), (4, -3)):
                back.point((x + dx, y + dy), fill=WOOD_LIGHT if kind == "hoe" else "#e7c794")
    elif animation in ("water", "fill_bowl"):
        outlet = vessel(back, "water_can" if animation == "water" else "pail", hand, phase == 1)
        if phase == 1:
            x, y = outlet
            back.line((x, y, min(47, x + 1), min(46, y + 6)), fill=WATER)
            back.point((max(0, x - 2), min(46, y + 4)), fill="#cfdfc7")
    elif animation in ("fish", "fish_success", "fish_fail"):
        ends = ((43, 18), (46, 14), (43, 18)) if animation == "fish" else ((44, 16), (45, 6), (44, 14))
        x, y = ends[phase]
        long_tool(back, "rod", hand, (x, y))
        lure_y = (37, 40, 38)[phase] if animation == "fish" else (34, 26, 30)[phase]
        back.line((x, y, x, lure_y), fill="#a9b8ad")
        if animation == "fish_success" and phase == 2:
            fish(back, (41, lure_y + 1))
        else:
            back.point((x, lure_y), fill="#df9369")
    elif animation in ("cook", "eat"):
        x, y = hand
        back.line((x, y, x + 4, y + (5 if animation == "cook" else -2)), fill=WOOD_LIGHT, width=2)
        if animation == "cook" and phase == 1:
            back.line([(x + 5, y + 8), (x + 6, y + 5), (x + 5, y + 3)], fill=(227, 219, 187, 160))
    elif animation == "drink":
        vessel(back, "cup", (hand[0] + 3, hand[1] - 1))
    elif animation == "sow" and phase == 1:
        x, y = hand
        back.point((x + 3, y + 3), fill="#e8c487")
        back.point((x + 4, y + 4), fill="#f7e1b0")
    elif animation == "harvest" and phase == 2:
        crop(back, (hand[0] + 4, hand[1] - 1))
    elif animation == "store" and phase < 2:
        vessel(back, "basket", (hand[0] - 1, hand[1] - 4))
    under.alpha_composite(image)
    return under


def sleeping(outfit, phase):
    image = Image.new("RGBA", AUTHOR_SIZE)
    draw = ImageDraw.Draw(image)
    y = 36 - (phase == 1)
    draw.ellipse((7, 40, 40, 46), fill=(52, 41, 47, 80))
    draw.rounded_rectangle((8, y - 1, 37, 43), radius=3, fill="#b18b76", outline="#805d59")
    draw.polygon([(10, 40), (12, y - 4), (17, y - 7), (26, y - 6), (30, y - 2),
                  (26, 41), (13, 42)], fill=outfit.shirt, outline=INK)
    draw.line([(15, y - 4), (19, y - 5), (24, y - 4)], fill=outfit.sleeve, width=2)
    draw.ellipse((25, y - 7, 37, y + 1), fill=outfit.skin, outline=INK)
    draw.arc((26, y - 7, 36, y + 1), 175, 340, fill=outfit.hair, width=3)
    draw.line((31, y - 2, 34, y - 2), fill=INK)
    draw.point((34, y), fill=outfit.shadow)
    return image


def actors():
    frames = []
    foot = scale_point(FOOT, AUTHOR_SIZE, PLAYER_SIZE)
    for animation, poses in MOTIONS.items():
        for phase, pose in enumerate(poses):
            for sex, outfit in (("female", KEEPER), ("male", CARPENTER)):
                sprite = sleeping(outfit, phase) if animation == "sleep" else add_tools(
                    player(pose, outfit, sex == "male"), animation, phase, pose)
                hand = None if animation == "sleep" else scale_point(pose.lead, AUTHOR_SIZE, PLAYER_SIZE)
                id_ = f"{sex}_{animation}_{phase}"
                frames.append(Frame(id_, sprite.resize(PLAYER_SIZE, NEAREST), foot, f"human_{sex}", hand))
    return frames


def props():
    frames = []
    for kind in ("hoe", "axe", "hammer", "rod", "water_can", "pail", "basket", "cup", "bowl",
                 "fish", "crop", "seeds", "meal", "wood", "stone", "rope"):
        image = Image.new("RGBA", (30, 28))
        draw = ImageDraw.Draw(image)
        anchor = (13, 13)
        if kind in ("hoe", "axe", "hammer", "rod"):
            anchor = (7, 17)
            long_tool(draw, kind, anchor, (23, 5 if kind != "hammer" else 8))
        elif kind == "fish":
            fish(draw, anchor)
        elif kind == "crop":
            crop(draw, anchor)
        elif kind == "seeds":
            draw.polygon([(9, 9), (16, 9), (17, 13), (15, 18), (9, 17), (8, 13)],
                         fill="#c2a27d", outline=INK)
            draw.line((10, 10, 15, 10), fill=METAL_LIGHT)
            draw.point((13, 14), fill=WOOD)
        elif kind in ("wood", "stone", "rope"):
            if kind == "wood":
                for y in (16, 12):
                    draw.rounded_rectangle((5, y, 21, y + 4), radius=2, fill=WOOD, outline=INK)
                    draw.line((7, y + 1, 18, y + 1), fill=WOOD_LIGHT)
                    draw.point((19, y + 2), fill="#f2ca91")
            elif kind == "stone":
                draw.polygon([(6, 17), (7, 12), (13, 9), (18, 11), (20, 16), (16, 19), (9, 19)],
                             fill="#aaa394", outline=INK)
                draw.line((9, 12, 15, 11), fill="#d0c2a6")
            else:
                draw.ellipse((5, 9, 21, 19), fill=WOOD_LIGHT, outline=INK)
                draw.arc((8, 11, 18, 17), 12, 340, fill=WOOD, width=2)
                draw.line((14, 16, 24, 21), fill=WOOD_LIGHT, width=2)
        else:
            vessel(draw, "bowl" if kind == "meal" else kind, anchor)
            if kind == "meal":
                draw.line((10, 12, 15, 12), fill="#96a064", width=2)
            if kind == "bowl":
                draw.line((10, 12, 17, 12), fill=WATER)
        frames.append(Frame(f"prop_{kind}", image, anchor))
    return frames


def window(draw, x, y, warm=False):
    draw.rectangle((x, y, x + 10, y + 11), fill="#5d545a", outline=INK)
    draw.rectangle((x + 2, y + 2, x + 8, y + 8), fill="#d7ab76" if warm else "#7e9c97")
    draw.line((x + 5, y + 2, x + 5, y + 9), fill="#786859")
    draw.line((x - 1, y + 11, x + 11, y + 11), fill="#c9a47b", width=2)


def facade(draw, x, y, w, h, wall, roof, style, door_center):
    right, floor = x + w - 1, y + h
    draw.polygon([(x, y + 12), (right, y + 12), (right, floor - 3),
                  (right - 4, floor), (x, floor)], fill=wall, outline=INK)
    draw.line((x + 1, floor - 3, right - 2, floor - 3), fill="#a3836c", width=2)
    if style == "shop":
        crest = [(x - 3, y + 16), (x + 5, y + 4), (right - 1, y + 2),
                 (right + 4, y + 14), (right - 1, y + 17)]
    else:
        crest = [(x - 4, y + 16), (x + w // 2 - 1, y), (right - 4, y + 4),
                 (right + 4, y + 16), (right - 2, y + 19), (x, y + 18)]
    draw.polygon(crest, fill=roof, outline=INK)
    draw.line((crest[0], crest[1], crest[2]), fill="#d2a38a")
    draw.line((x + 1, y + 16, right, y + 16), fill="#6d514f", width=2)
    for t in (7, 13):
        draw.line((x + t, y + 13, min(right - 6, x + t + 10), y + 12), fill="#a46f63")
    door_x = door_center - 4
    draw.rectangle((door_x, floor - 17, door_x + 8, floor - 3), fill="#886750", outline=INK)
    draw.line((door_x + 2, floor - 15, door_x + 2, floor - 5), fill="#b78d66")
    draw.point((door_x + 6, floor - 9), fill="#edc47b")
    if w >= 34:
        window(draw, x + 5, floor - 17, style == "kitchen")
    draw.line((door_x - 1, floor - 2, door_x + 9, floor - 2), fill="#d5aa7d", width=2)


def service_extension(draw, name, stage, floor):
    # A shallow addition fits on the left of the door within the footprint.
    left, right = 20, 40
    draw.polygon([(left, floor - 7), (right - 4, floor - 7), (right, floor - 3),
                  (right - 2, floor), (left - 1, floor)], fill="#a08d7d", outline=INK)
    draw.line((left + 1, floor - 4, right - 2, floor - 4), fill="#c5ae8e")
    if stage == 1:
        for x in (left + 2, right - 3):
            draw.line((x, floor - 5, x, floor - 12), fill=WOOD, width=2)
            draw.point((x, floor - 12), fill="#e0ba91")
        return

    awning_top, awning_bottom = floor - 24, floor - 19
    draw.line((right - 2, floor - 3, right - 2, awning_top), fill=WOOD, width=3)
    draw.line((left + 1, floor - 3, left + 1, awning_top), fill=WOOD, width=2)
    draw.line((left, awning_top, right - 1, awning_bottom), fill=WOOD, width=3)
    if stage == 2:
        draw.line((left + 2, floor - 4, right - 4, awning_top + 6), fill=WOOD_LIGHT, width=2)
        return

    shade = {"kitchen": "#a26d60", "storehouse": "#778570", "workshop": "#98735b"}[name]
    draw.polygon([(left - 1, awning_top), (left + 4, awning_top - 6),
                  (right - 4, awning_top - 3), (right + 2, awning_bottom),
                  (right, awning_bottom + 2)], fill=shade, outline=INK)
    draw.line((left + 4, awning_top - 5, right - 5, awning_top - 2), fill="#d6ae88")
    if name == "kitchen":
        draw.rectangle((left + 3, floor - 14, right - 4, floor - 7), fill="#8d6f59", outline=INK)
        draw.ellipse((left + 5, floor - 17, left + 11, floor - 13), fill=METAL_LIGHT, outline=INK)
    elif name == "storehouse":
        for x, y in ((left + 3, floor - 12), (left + 9, floor - 8)):
            draw.rectangle((x, y, x + 6, y + 5), fill=WOOD_LIGHT, outline=INK)
            draw.line((x + 1, y + 2, x + 5, y + 2), fill=WOOD)
    else:
        draw.line((left + 2, floor - 12, right - 3, floor - 12), fill=WOOD_LIGHT, width=3)
        draw.line((left + 5, floor - 13, left + 12, floor - 16), fill=METAL_LIGHT, width=2)


def house_upgrade(draw, stage, floor, roof):
    left, right = 19, 39
    draw.polygon([(left, floor - 6), (right - 3, floor - 6), (right, floor - 3),
                  (right - 2, floor), (left, floor)], fill="#a08d7d", outline=INK)
    draw.line((left + 2, floor - 3, right - 2, floor - 3), fill="#c5ae8e")
    if stage == 1:
        for x in (left + 2, right - 2):
            draw.line((x, floor - 3, x, floor - 10), fill=WOOD, width=2)
        return
    for x in (left + 2, right - 2):
        draw.line((x, floor - 2, x, floor - 21), fill=WOOD, width=2)
    if stage == 2:
        draw.line((left, floor - 21, right, floor - 21), fill=WOOD, width=2)
        draw.line((left + 2, floor - 5, right - 2, floor - 19), fill=WOOD_LIGHT, width=2)
        return
    draw.polygon([(left, floor - 21), (left + 3, floor - 25), (right - 3, floor - 25),
                  (right, floor - 21), (right - 1, floor - 19), (left + 1, floor - 19)],
                 fill=roof, outline=INK)
    draw.line((left + 4, floor - 24, right - 4, floor - 24), fill="#d2a38a")
    draw.rectangle((left + 3, floor - 7, right - 3, floor - 4), fill="#b58a69", outline=INK)
    draw.line((left + 5, floor - 6, right - 5, floor - 6), fill="#d0ab80")


def building(name, stage):
    image = Image.new("RGBA", (96, 63))
    draw = ImageDraw.Draw(image)
    house = name in ("west_house", "east_house")
    floor = 54
    door_center = {
        "west_house": 45, "east_house": 45, "kitchen": 50,
        "storehouse": 49, "workshop": 55,
    }[name]
    draw.ellipse((12, floor - 4, 84 if house else 75, floor + 5), fill=(58, 47, 44, 80))
    wall, roof = {
        "west_house": ("#d9c5a4", "#a66a63"),
        "east_house": ("#c5c1a2", "#778489"),
        "kitchen": ("#dbc29d", "#b67b61"),
        "storehouse": ("#b5a484", "#818471"),
        "workshop": ("#bdab8a", "#9b725b"),
    }[name]
    if name in ("west_house", "kitchen"):
        draw.polygon([(47, 18), (47, 5), (55, 6), (55, 21)], fill="#a4796e", outline=INK)
        draw.line((46, 6, 55, 7), fill="#c7a095", width=2)
    style = "shop" if name in ("workshop", "storehouse") else name
    facade(draw, 19, 15, 42 if house else 49, 39, wall, roof, style, door_center)
    if name == "workshop":
        draw.rectangle((18, 35, 32, 39), fill="#aa815b", outline=INK)
        draw.line((22, 37, 28, 37), fill="#f3d497")
    if name == "storehouse":
        for x in (21, 30):
            draw.rectangle((x, 46, x + 7, 53), fill=WOOD_LIGHT, outline=INK)
            draw.line((x + 1, 48, x + 6, 51), fill=WOOD)
    if stage:
        if house:
            house_upgrade(draw, stage, floor, roof)
        else:
            service_extension(draw, name, stage, floor)
    return Frame(f"{name}_{stage}" if house or stage else name, image, (door_center, floor))


def buildings():
    houses = [building(name, stage) for name in ("west_house", "east_house") for stage in range(4)]
    names = ("kitchen", "storehouse", "workshop")
    existing = [building(name, 0) for name in names]
    additions = [building(name, stage) for name in names for stage in range(1, 4)]
    return houses + existing + additions


def shared_table():
    image = Image.new("RGBA", (16, 11))
    draw = ImageDraw.Draw(image)
    draw.ellipse((1, 7, 15, 10), fill=(48, 37, 40, 89))
    draw.line((3, 5, 3, 8), fill=INK, width=2)
    draw.line((12, 5, 12, 8), fill=INK, width=2)
    draw.point((3, 7), fill=WOOD_LIGHT)
    draw.point((12, 7), fill=WOOD_LIGHT)
    draw.polygon([(1, 3), (4, 1), (12, 1), (14, 2), (15, 4),
                  (13, 6), (2, 6), (0, 5)], fill=WOOD_LIGHT, outline=INK)
    draw.line((3, 5, 12, 5), fill=WOOD)
    draw.line((4, 2, 11, 2), fill="#e8bb80")
    draw.point((6, 4), fill="#bb8c60")
    draw.point((9, 3), fill="#bb8c60")
    return Frame("shared_table", image, (8, 9))


def shared_stool():
    image = Image.new("RGBA", (7, 7))
    draw = ImageDraw.Draw(image)
    draw.ellipse((0, 4, 6, 6), fill=(48, 37, 40, 85))
    draw.line((1, 2, 1, 5), fill=INK)
    draw.line((5, 2, 5, 5), fill=INK)
    draw.rounded_rectangle((0, 0, 6, 3), radius=1, fill=WOOD_LIGHT, outline=INK)
    draw.line((2, 1, 4, 1), fill="#ebbe86")
    return Frame("shared_stool", image, (3, 5))


LABELS = ("站立", "步行", "搬运", "清草", "翻地", "播种", "浇水", "采收", "入库", "做饭", "吃饭",
          "添猫碗", "饮水", "伐木", "修建", "休息", "睡眠", "垂钓", "钓到鱼", "空钩")


def preview(frames, output):
    by_id = {frame.id: frame for frame in frames}
    sheet = Image.new("RGB", (1160, 1524), "#222c28")
    draw = ImageDraw.Draw(sheet)
    title = ImageFont.truetype("msyhbd.ttc", 28)
    label = ImageFont.truetype("msyh.ttc", 16)
    draw.text((24, 20), "小禾与阿榆 · 核心生活动作", font=title, fill="#efe0ba")
    draw.text((24, 61), "每格按动作起势、接触、收势排列。上排小禾，下排阿榆。", font=label, fill="#a9b6a8")
    for number, (motion, name) in enumerate(zip(MOTIONS, LABELS)):
        x, y = 24 + number % 4 * 280, 98 + number // 4 * 185
        draw.rounded_rectangle((x, y, x + 269, y + 173), radius=7, fill="#39483f")
        draw.text((x + 11, y + 8), name, font=label, fill="#efe0ba")
        for row, sex in enumerate(("female", "male")):
            cell = Image.new("RGBA", (125, 34))
            for phase in range(3):
                frame = by_id[f"{sex}_{motion}_{phase}"]
                place(cell, frame, (18 + 42 * phase, 31))
            cell = cell.resize((250, 68), NEAREST)
            sheet.paste(cell, (x + 10, y + 31 + 69 * row), cell)
    draw.rounded_rectangle((24, 1029, 1134, 1115), radius=7, fill="#39483f")
    draw.text((35, 1040), "可独立叠放的工具、食物与物料", font=label, fill="#efe0ba")
    for i, frame in enumerate(frame for frame in frames if frame.id.startswith("prop_")):
        cell = frame.image.resize((51, 48), NEAREST)
        sheet.paste(cell, (34 + i * 67, 1063), cell)
    draw.rounded_rectangle((24, 1128, 1134, 1500), radius=7, fill="#39483f")
    draw.text((35, 1140), "庭院建筑与家具", font=label, fill="#efe0ba")
    for row, (kind, text) in enumerate((("west_house", "西屋"), ("east_house", "东屋"),
                                       ("kitchen", "厨房"), ("storehouse", "库房"), ("workshop", "作坊"))):
        y = 1162 + row * 65
        draw.text((39, y + 27), text, font=label, fill="#a9b6a8")
        for stage in range(4):
            key = kind if stage == 0 and kind in by_id else f"{kind}_{stage}"
            frame = by_id[key]
            sheet.paste(frame.image, (106 + stage * 131, y), frame.image)
    draw.text((795, 1189), "共享餐桌", font=label, fill="#a9b6a8")
    table = by_id["shared_table"].image.resize((64, 44), NEAREST)
    sheet.paste(table, (895, 1180), table)
    stool = by_id["shared_stool"].image.resize((28, 28), NEAREST)
    sheet.paste(stool, (868, 1200), stool)
    sheet.paste(stool, (962, 1200), stool)
    output.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview", type=Path)
    args = parser.parse_args()
    frames = actors() + props() + buildings() + [shared_table(), shared_stool()]
    data = pack_pages(frames)
    DESTINATION.mkdir(parents=True, exist_ok=True)
    index = json.loads(data.pop("assets/index.json"))
    for page in index["pages"]:
        source = page["file"]
        page["file"] = Path(source).name
        (DESTINATION / page["file"]).write_bytes(data[source])
    (DESTINATION / "index.json").write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")
    if args.preview:
        preview(frames, args.preview)
    print(f"{len(frames)} frames, {len(data)} pages")


if __name__ == "__main__":
    main()
