"""Bake the role motion loops used by the native cast renderer.

Run bake_world.py and bake_cast.py first if the source sprites have changed.
The output is role-major, then Bob, Hop, Nap, Patrol, Still, Tilt. Every loop
contains six 72 by 48 premultiplied BGRA frames around pivot (36, 44).
"""

import argparse
from dataclasses import dataclass
from pathlib import Path
from tempfile import NamedTemporaryFile

from PIL import Image, ImageDraw

from bake_world import OUTPUT, SPRITE_SIZE, bgra


SOURCE_WIDTH, SOURCE_HEIGHT = SPRITE_SIZE
SOURCE_PIVOT_X, SOURCE_PIVOT_Y = 24, 29
SOURCE_FRAME_BYTES = SOURCE_WIDTH * SOURCE_HEIGHT * 4
WIDTH, HEIGHT = 72, 48
PIVOT_X, PIVOT_Y = 36, 44
SIZE = (WIDTH, HEIGHT)
PIXEL_RATIO = WIDTH / SOURCE_WIDTH
FRAME_COUNT = 6
NEAREST = Image.Resampling.NEAREST
MOTIONS = ("Bob", "Hop", "Nap", "Patrol", "Still", "Tilt")
LEGACY = ("cat_idle", "grey_cat", "black_cat", "sparrow", "squirrel",
          "hedgehog", "rabbit", "frog")
FLIERS = {13, 18, 22, 23, 24, 25, 26, 29}
CRAWLERS = {15, 17, 19, 20, 21, 27, 28}
# Wing inks follow the original plumage and translucent insect wings.
WING_PALETTES = {
    22: ((101, 128, 111, 170), (169, 188, 144, 170), (211, 226, 162, 140)),
    23: ((139, 126, 122, 190), (220, 209, 187, 195), (252, 238, 212, 220)),
}
# Small ink marks in source coordinates. Each follows the transformed body:
# coat seams, lantern glass, tool edges, tail rings, wing edges and pelt marks.
INK_DETAILS = (
    (24, 24, 1, 0), (14, 22, 1, 0), (36, 17, 1, -1), (12, 24, 0, -1),
    (38, 7, 1, 0), (38, 21, 1, 0), (14, 19, 1, 0), (36, 18, 1, -1),
    (13, 21, 0, -1), (36, 24, 1, 0), (22, 21, 0, 1), (24, 19, 1, 0),
    (29, 19, 1, 0), (24, 25, 1, -1), (19, 19, 0, -1), (22, 22, 1, -1),
    (23, 17, 0, 1), (23, 25, 1, 0), (24, 23, 1, -1), (29, 22, 0, 1),
    (22, 24, 1, -1), (22, 25, 0, 1), (24, 22, 1, 0), (23, 23, 0, 1),
    (25, 22, 1, -1), (25, 22, 0, 1), (19, 23, 1, 1), (25, 24, 1, 0),
    (26, 23, 1, 1), (27, 23, 1, -1),
)


@dataclass(frozen=True)
class Pose:
    source: int
    width: int = 100
    height: int = 100
    lift: int = 0
    sway: int = 0
    lean: int = 0
    feet: int = 0
    wing: int = 0


# Authored anticipation, stretch and recovery. These are local pose curves;
# the path a visitor takes across the garden remains part of the simulation.
POSES = (
    (Pose(0), Pose(1, 103, 95, lean=1), Pose(2, 109, 90, lean=2),
     Pose(0, 106, 92, sway=1, lean=2), Pose(1, 101, 98, sway=-1, lean=1), Pose(2, 99, 101)),
    (Pose(0, 108, 91), Pose(1, 96, 103, 2, -1, 1, wing=4),
     Pose(2, 91, 110, 4, 0, 1, wing=8), Pose(0, 94, 105, 4, 1, -1, wing=3),
     Pose(1, 102, 97, 2, 1, -1, wing=-3), Pose(2, 111, 86, 0, 1, 0, wing=-1)),
    (Pose(0, 103, 79, lean=1), Pose(1, 107, 71, lean=2),
     Pose(2, 113, 64, sway=1, lean=3), Pose(0, 116, 61, sway=1, lean=3),
     Pose(1, 113, 66, sway=1, lean=2), Pose(2, 108, 72, lean=1)),
    (Pose(0, 98, 101, sway=-1, lean=1, feet=2, wing=-3),
     Pose(1, 96, 101, 1, 0, 1, 1, 5), Pose(2, 101, 97, feet=-1, wing=7),
     Pose(0, 103, 97, sway=1, lean=-1, feet=-2, wing=2),
     Pose(1, 98, 100, 1, 0, -1, -1, -3), Pose(2, 102, 98, feet=1, wing=-1)),
    (Pose(0), Pose(1, 101, 101, lean=1), Pose(0, 101, 103, sway=1),
     Pose(2, 99, 102, sway=1, lean=-1), Pose(1, 103, 98, lean=-1), Pose(2, 100, 99)),
    (Pose(0), Pose(1, 100, 98, lean=2), Pose(2, 100, 96, lean=4),
     Pose(0, 103, 92, sway=1, lean=5), Pose(1, 102, 95, sway=1, lean=3),
     Pose(2, 100, 99, lean=1)),
)


def source_frames(name):
    data = (OUTPUT / f"{name}.bgra").read_bytes()
    if not data or len(data) % SOURCE_FRAME_BYTES:
        raise ValueError(f"Invalid source atlas: {name}")
    return tuple(decode(data[offset:offset + SOURCE_FRAME_BYTES])
                 for offset in range(0, len(data), SOURCE_FRAME_BYTES))


def decode(frame):
    premultiplied = Image.frombytes("RGBA", SPRITE_SIZE, frame, "raw", "BGRA")
    pixels = []
    for index, (r, g, b, a) in enumerate(premultiplied.getdata()):
        if not a or (a < 112 and index // SOURCE_WIDTH >= SOURCE_PIVOT_Y - 2):
            # Floor shadows are repainted beneath the transformed sprite.
            pixels.append((0, 0, 0, 0))
        else:
            rgb = tuple(min(255, (value * 255 + a // 2) // a) for value in (r, g, b))
            pixels.append((*rgb, a))
    premultiplied.putdata(pixels)
    return premultiplied


def roster_frames():
    visitors = source_frames("extra-cast")
    groups = [visitors[start:start + 3] for start in range(0, len(visitors), 3)]
    if len(groups) != 21 or len(groups[-1]) != 3:
        raise ValueError("The supporting cast must contain 21 sets of three frames")
    return (source_frames("keeper")[:3], *groups[:9],
            *(source_frames(name) for name in LEGACY), *groups[9:])


def draw_cat(art, phase, motion, coat):
    edge, dark, fur, light, cream, stripes = coat
    if motion == "Nap":
        art.ellipse((23, 32, 48, 43), fill=edge)
        art.ellipse((24, 32, 46, 42), fill=fur)
        art.line([(24, 38), (27, 41), (35, 42), (42, 41)], fill=dark, width=2)
        art.line([(26, 37), (30, 34), (35, 34)], fill=light)
        art.polygon([(33, 37), (34, 32), (37, 30), (40, 32), (43, 31),
                     (45, 34), (48, 37), (47, 41), (43, 43), (37, 42)], fill=fur, outline=edge)
        art.polygon([(35, 33), (37, 31), (39, 34)], fill=dark)
        art.polygon([(43, 38), (46, 37), (47, 39), (45, 41), (41, 41)], fill=cream)
        art.line((41, 37, 43, 37), fill=edge)
        art.point((46, 38), fill=dark)
        art.line([(25, 40), (24, 41), (27, 42), (33 + phase, 43), (37, 42)], fill=stripes, width=2)
        art.line((28, 42, 32 + phase, 42), fill=light)
        return

    # The free end of the tail moves separately from the head and planted paws.
    tail = [(32, 39), (27, 37), (24, 34), (24 - phase, 31), (25 - phase, 29)]
    art.line(tail, fill=edge, width=4, joint="curve")
    art.line(tail, fill=fur, width=2, joint="curve")
    art.point((25 - phase, 29), fill=light)
    art.line((24 - phase, 31, 24 - phase, 32), fill=stripes)
    art.polygon([(27, 36), (30, 33), (36, 32), (42, 33), (45, 36),
                 (45, 40), (42, 43), (37, 42), (34, 43), (28, 42), (26, 39)],
                fill=fur, outline=edge)
    art.polygon([(28, 36), (31, 34), (36, 33), (38, 35), (34, 38), (29, 38)], fill=light)
    art.line([(28, 40), (31, 41), (36, 40), (38, 41), (43, 40)], fill=dark, width=2)
    art.line([(31, 35), (32, 37), (34, 38)], fill=stripes)
    art.line([(34, 34), (35, 36), (37, 37)], fill=stripes)
    step = (-1, 0, 1)[phase] if motion == "Patrol" else 0
    for foot in (30 + step, 41 - step):
        art.rectangle((foot, 40, foot + 3, 43), fill=edge)
        art.line((foot + 1, 41, foot + 2, 41), fill=cream)
        art.line((foot + 1, 42, foot + 2, 42), fill=light)
    art.polygon([(37, 33), (37, 28), (38, 26), (42, 29), (45, 29), (48, 27),
                 (49, 29), (49, 33), (51, 35), (50, 38), (46, 40),
                 (41, 39), (38, 37)], fill=fur, outline=edge)
    art.polygon([(38, 28), (41, 31), (38, 32)], fill=light)
    art.polygon([(45, 31), (48, 29), (48, 32)], fill=dark)
    art.line([(39, 33), (42, 31), (45, 31)], fill=light)
    art.line([(43, 30), (44, 32), (44, 33)], fill=stripes)
    art.polygon([(42, 37), (45, 35), (48, 35), (50, 36), (49, 38),
                 (46, 39), (43, 38)], fill=cream)
    art.line((42, 34, 42, 35 if phase != 2 else 34), fill=edge)
    art.point((42, 34), fill=edge)
    art.point((48, 35), fill=edge)
    art.point((47, 37), fill=dark)
    art.point((44, 38), fill=light)


CATS = {
    10: ("#6a403d", "#b96746", "#dc884d", "#f5b66a", "#ffe1a6", "#a85b42"),
    11: ("#454355", "#68637b", "#898294", "#aca1ad", "#f2dfc7", "#645c75"),
    12: ("#333045", "#4a4056", "#5c526b", "#81738c", "#ccbac4", "#474052"),
}


KEEPER_OUTFITS = (
    # Boots, jacket shadow, lit sleeve, front layer, seam and fastening.
    ("#695447", "#557957", "#8d9d57", "#38665e", "#759a7d", "#f0cf87"),
    ("#795354", "#b37068", "#d78d7c", "#e1bc92", "#f5d9aa", "#d58f70"),
    ("#695447", "#477782", "#6ea0a4", "#3e616b", "#80aaa7", "#f6c468"),
    ("#bd864d", "#4e7d88", "#78a7ab", "#6795a1", "#a8c8bc", "#f1c581"),
    ("#735966", "#896a80", "#b88e9f", "#9b8292", "#be9eab", "#97ad8b"),
)


def draw_keeper_nap(art, phase):
    # A blanket under the tree, shoes tucked to the side and head turned into a sleeve.
    edge = "#5c4b51"
    art.polygon([(20, 40), (44, 38), (54, 41), (52, 44), (20, 44), (18, 43)],
                fill="#b58871", outline="#816558")
    art.line((21, 43, 48, 43), fill="#d6ac85")
    art.polygon([(24, 39), (27, 34), (32, 31 - phase), (37, 31 - phase),
                 (42, 35), (44, 39), (43, 41), (25, 42)], fill="#977a91", outline=edge)
    art.line([(26, 38), (31, 34), (36, 33 - phase), (40, 35)], fill="#bb98a6", width=2)
    art.line([(29, 40), (35, 39), (37, 36)], fill="#775f77", width=2)
    art.polygon([(37, 36), (41, 35), (45, 37), (44, 40), (39, 39)],
                fill="#87a382")
    art.polygon([(40, 34), (41, 30), (45, 28), (49, 28), (53, 31),
                 (55, 36), (52, 40), (47, 40), (43, 38)], fill="#f4c394", outline=edge)
    art.polygon([(41, 32), (44, 29), (49, 29), (53, 31), (54, 34),
                 (51, 32), (48, 32), (45, 35), (42, 36)], fill="#825443")
    art.line([(43, 32), (46, 30), (49, 30)], fill="#ba7c53")
    art.line((49, 35, 51, 35), fill=edge)
    art.point((53, 37), fill="#bd7863")
    art.line([(47, 39), (50, 39), (52, 38)], fill="#e6aa80")
    art.line((21, 41, 24, 41), fill="#6f5552", width=2)


def draw_keeper(art, phase, motion, outfit=0):
    if motion == "Nap":
        draw_keeper_nap(art, phase)
        return
    edge = "#514237"
    boot, jacket, sleeve, front, seam, fastening = KEEPER_OUTFITS[outfit]
    skin, peach, blush = "#f4c394", "#eaa376", "#bc745d"
    step = (-1, 0, 1)[phase] if motion == "Patrol" else 0
    art.polygon([(29, 31), (40, 31), (41, 37), (40, 39), (39 - step, 42), (41 - step, 42),
                 (40 - step, 44), (35 - step, 44), (35, 40), (33, 40), (32 + step, 44),
                 (27 + step, 44), (26 + step, 42), (29 + step, 41), (29, 37)],
                fill=boot, outline=edge)
    art.line((28 + step, 42, 31 + step, 42), fill="#b08a64")
    art.line((36 - step, 42, 39 - step, 42), fill="#b08a64")
    art.polygon([(30, 24), (38, 23), (42, 26), (43, 31), (42, 34),
                 (40, 34), (40, 38), (29, 38), (28, 34), (26, 34), (25, 31), (26, 27)],
                fill=jacket, outline=edge)
    art.polygon([(27, 28), (30, 26), (34, 26), (32, 31), (28, 32)], fill=sleeve)
    if outfit in (0, 1, 2):
        art.polygon([(31, 27), (32, 27), (33, 32), (38, 31), (38, 26), (39, 26),
                     (40, 36), (38, 38), (32, 37)], fill=front)
        art.line((33, 33, 37, 32), fill=seam)
        art.point((32, 32), fill=fastening)
        art.point((38, 31), fill=fastening)
    elif outfit == 3:
        art.line([(37, 27), (36, 32), (37, 37)], fill=seam)
        art.point((36, 29), fill=fastening)
        art.point((36, 34), fill=fastening)
    else:
        art.line([(31, 27), (35, 29), (40, 27)], fill=fastening, width=3)
        art.line((35, 28, 36, 34), fill=fastening, width=2)
        art.point((36, 33), fill=seam)
    art.ellipse((26, 30, 29, 34), fill=peach, outline=edge)
    art.point((27, 31), fill=skin)
    art.ellipse((40, 30, 43, 34), fill=peach, outline=edge)
    art.polygon([(26, 18), (29, 15), (39, 15), (42, 17), (44, 21),
                 (43, 25), (40, 28), (33, 28), (29, 26), (27, 23)], fill=peach, outline=edge)
    art.polygon([(30, 19), (39, 17), (42, 19), (42, 24), (39, 26),
                 (33, 26), (30, 23)], fill=skin)
    art.polygon([(27, 18), (30, 15), (40, 16), (43, 19), (40, 19),
                 (38, 20), (35, 19), (32, 21), (31, 24), (29, 23)], fill="#754b3d")
    art.line([(30, 19), (33, 18), (36, 18)], fill="#ae6c47")
    art.line((35, 22, 35, 22 if phase == 2 else 23), fill=edge)
    art.line((41, 21, 41, 21 if phase == 2 else 22), fill=edge)
    art.point((32, 24), fill=blush)
    art.point((42, 24), fill=blush)
    art.line((38, 25, 39, 25), fill=blush)

    if outfit == 2:
        art.polygon([(24, 17), (26, 13), (28, 12), (29, 9), (33, 7), (39, 7),
                     (43, 9), (46, 12), (46, 15), (49, 16), (49, 18),
                     (43, 19), (31, 20), (26, 19)], fill="#e1a359", outline=edge)
        art.polygon([(29, 12), (33, 9), (38, 8), (42, 10), (44, 13),
                     (42, 15), (34, 16), (29, 15)], fill="#f4cb76")
        art.line([(35, 9), (36, 11), (35, 15)], fill="#ca8849")
        art.line([(26, 17), (31, 18), (42, 17), (46, 16)], fill="#ffe297")
        return
    if outfit == 3:
        art.polygon([(25, 26), (24, 21), (25, 15), (28, 11), (32, 9), (40, 9),
                     (44, 12), (47, 17), (46, 25), (43, 28), (41, 26), (43, 22),
                     (43, 18), (39, 16), (33, 17), (31, 20), (30, 26), (32, 27),
                     (29, 29)], fill="#5d8d9c", outline=edge)
        art.line([(26, 23), (26, 18), (28, 14), (32, 12), (39, 11), (42, 13)],
                 fill="#9ebcbb", width=2)
        art.line([(30, 27), (33, 29), (38, 29), (43, 27)], fill="#6798a2", width=2)
        art.point((38, 29), fill=fastening)
        return
    if outfit in (1, 4):
        art.polygon([(26, 20), (25, 17), (26, 13), (29, 10), (35, 9), (40, 10),
                     (43, 12), (44, 16), (43, 19), (39, 17), (36, 18), (32, 17),
                     (30, 21), (29, 25), (26, 24)], fill="#825443", outline=edge)
        art.line([(27, 16), (29, 13), (33, 11), (37, 11)], fill="#bd7e54", width=2)
        art.line([(31, 14), (34, 13), (38, 13)], fill="#a16749")
        if outfit == 1:
            art.polygon([(39, 11), (41, 8), (43, 9), (43, 12), (46, 11), (47, 14),
                         (46, 15), (42, 14), (40, 15)], fill="#d48b82", outline="#85545a")
            art.point((41, 12), fill="#efb1a1")
        return

    # A woven crown with a green ribbon makes the gardener readable in profile.
    art.polygon([(21, 17), (24, 14), (28, 13), (28, 10), (31, 7), (36, 6), (40, 7),
                 (43, 10), (44, 13), (47, 14), (48, 16), (46, 18), (38, 19), (26, 20),
                 (22, 19)], fill="#c59252", outline=edge)
    art.polygon([(29, 12), (30, 9), (35, 7), (39, 8), (42, 10), (41, 12),
                 (35, 14), (30, 14)], fill="#eac278")
    art.line([(28, 14), (33, 15), (38, 14), (43, 12)], fill="#658657", width=2)
    art.line([(22, 17), (26, 18), (36, 17), (45, 15)], fill="#f9d888")
    art.line([(26, 16), (27, 15), (30, 16)], fill="#ac7648")
    art.line((32, 10, 36, 9), fill="#f8d78e")
    art.point((43, 14), fill="#fff0a7")
    art.point((45, 13), fill="#8ca163")


def draw_frog(art, phase, motion):
    edge, dark, leaf, light = "#45523d", "#5c7443", "#7b984e", "#a8b968"
    art.polygon([(27, 38), (28, 33), (31, 30), (37, 29), (43, 31), (46, 35),
                 (46, 40), (42, 43), (29, 43), (25, 42), (25, 40)], fill=leaf, outline=edge)
    art.polygon([(27, 37), (29, 34), (35, 33), (37, 36), (35, 40),
                 (32, 41), (28, 40)], fill=light)
    art.line([(27, 41), (30, 41), (32, 39), (34, 39)], fill=dark, width=2)
    art.polygon([(35, 35), (35, 31), (37, 28), (41, 28), (43, 31), (46, 30),
                 (49, 31), (50, 34), (49, 38), (45, 40), (38, 40)], fill=leaf, outline=edge)
    art.polygon([(38, 29), (41, 29), (42, 32), (39, 34), (36, 33)], fill="#c8c983")
    art.polygon([(38, 37), (41, 38), (46, 37), (49, 36), (48, 38), (45, 39),
                 (40, 39)], fill="#d5c586")
    art.line([(39, 36), (44, 36), (47, 35)], fill=dark)
    art.line((40, 31, 40 + (phase == 1), 32), fill=edge)
    art.point((48, 33), fill=edge)
    art.point((29, 36), fill=dark)
    art.point((33, 35), fill=dark)
    art.point((34, 36), fill=dark)
    if motion != "Nap":
        art.line([(39, 41), (42, 41), (43 + phase, 42), (45 + phase, 42)], fill=dark)


def draw_duck(art, phase, motion):
    edge, shadow, feather, cream, gold = "#685145", "#bf9f76", "#e0c89a", "#fff0c8", "#da914d"
    step = (-1, 0, 1)[phase] if motion == "Patrol" else 0
    art.line([(31, 39), (31 + step, 42), (28 + step, 43), (33 + step, 43)], fill=gold)
    art.line([(38, 39), (38 - step, 42), (37 - step, 43), (41 - step, 43)], fill="#b6794a")
    art.polygon([(25, 33), (29, 35), (32, 34), (38, 32), (43, 33), (45, 36), (43, 41),
                 (38, 43), (31, 42), (27, 40), (24, 36)], fill=feather, outline=edge)
    art.polygon([(27, 36), (32, 36), (37, 35), (40, 37), (38, 39),
                 (33, 40), (29, 39)], fill=cream)
    art.line([(31, 41), (37, 42), (41, 40)], fill=shadow)
    art.polygon([(38, 33), (38, 30), (40, 27), (44, 26), (48, 28), (49, 30),
                 (49, 34), (46, 36), (41, 36)], fill=feather, outline=edge)
    art.polygon([(40, 30), (42, 28), (46, 28), (48, 30), (46, 33), (42, 34)], fill=cream)
    art.polygon([(47, 31), (50, 31), (53, 33), (51, 34), (48, 34)], fill=gold, outline="#a56d46")
    art.line((49, 32, 50, 32), fill="#fbc472")
    art.line((45, 30, 45, 30 if phase == 2 else 31), fill=edge)


def draw_squirrel(art, phase, motion):
    edge, dark, chestnut, bright, cream = "#694944", "#9f5d4a", "#bf7851", "#e5a16b", "#f1cf97"
    swish = (0, 1, -1)[phase]
    art.polygon([(37, 40), (32, 39), (26, 38), (22, 34), (20 - swish, 29),
                 (21 - swish, 24), (24 - swish, 21), (29 - swish, 21), (32 - swish, 24),
                 (33, 28), (30, 33), (32, 35), (37, 36)], fill=chestnut, outline=edge)
    art.polygon([(22 - swish, 29), (22 - swish, 25), (25 - swish, 22), (28 - swish, 22),
                 (30 - swish, 24), (27, 26), (26, 30), (28, 34), (24, 33)], fill=bright)
    art.line([(29, 27), (27, 30), (27, 33), (30, 36), (33, 37)], fill=dark, width=2)
    art.polygon([(31, 36), (32, 32), (37, 31), (42, 33), (43, 39),
                 (41, 43), (33, 43), (30, 41)], fill=chestnut, outline=edge)
    art.line([(31, 39), (34, 42), (39, 42)], fill=dark, width=2)
    art.polygon([(39, 34), (42, 35), (42, 40), (39, 42), (36, 39)], fill=cream)
    art.polygon([(36, 31), (36, 26), (38, 24), (41, 27), (44, 26), (47, 27),
                 (48, 30), (50, 32), (48, 35), (43, 37), (38, 34)], fill=chestnut, outline=edge)
    art.polygon([(37, 28), (38, 25), (40, 28), (43, 27), (45, 28), (43, 30), (38, 31)], fill=bright)
    art.line([(42, 33), (45, 32), (48, 33), (45, 35), (41, 35)], fill=cream, width=2)
    art.line((45, 30, 45, 30 if phase == 2 else 31), fill=edge)
    if motion != "Nap":
        art.line([(41, 37), (43, 39), (46 + phase, 39)], fill=dark, width=2)
        art.line((34 - swish, 42, 37 - swish, 42), fill=bright)


def draw_hedgehog(art, phase, motion):
    edge, bark, brown, tips, cream = "#624b4b", "#7b5c55", "#997560", "#c29370", "#e6bf96"
    art.polygon([(24, 40), (23, 37), (25, 34), (24, 33), (27, 33), (27, 31), (30, 32),
                 (31, 30), (34, 31), (36, 30), (38, 32), (41, 31), (42, 34), (44, 38),
                 (41, 42), (29, 43)], fill=brown, outline=edge)
    for x, y in ((27, 34), (30, 32), (34, 33), (37, 32), (26, 38), (30, 37), (34, 36), (36, 39)):
        art.line((x, y, x + 2, y - 1), fill=tips)
        art.point((x + 2, y), fill=bark)
    art.line([(26, 41), (31, 42), (38, 41)], fill=bark, width=2)
    art.polygon([(38, 37), (39, 34), (42, 33), (45, 35), (46, 37), (49, 39),
                 (46, 41), (41, 42), (38, 40)], fill=cream, outline=edge)
    art.line((39, 36, 41, 34), fill=tips)
    art.point((44, 37), fill=edge)
    art.point((47, 39), fill=bark)
    if motion != "Nap":
        art.line((40 + phase, 42, 43 + phase, 42), fill=tips)
    else:
        art.point((41 + phase, 36), fill=tips)


def draw_rabbit(art, phase, motion):
    edge, brown, coat, light, white = "#675454", "#997968", "#c1a586", "#e0c29d", "#f3e2bc"
    art.ellipse((23, 34, 29, 40), fill=white, outline=edge)
    art.polygon([(26, 38), (28, 33), (34, 31), (39, 32), (44, 37),
                 (44, 42), (39, 43), (28, 42)], fill=coat, outline=edge)
    art.polygon([(27, 38), (30, 34), (34, 33), (37, 35), (35, 39),
                 (32, 41), (28, 40)], fill=light)
    art.line([(30, 42), (35, 40), (37, 41), (40, 40)], fill=brown)
    ear = 0 if motion == "Nap" else phase
    art.polygon([(37, 32), (33 - ear, 24), (33 - ear, 21), (35 - ear, 19),
                 (37 - ear, 20), (40, 27), (41, 32)], fill=coat, outline=edge)
    art.polygon([(40, 32), (40, 25), (42, 19), (44, 18), (46, 20),
                 (45, 27), (43, 33)], fill=light, outline=edge)
    art.line([(42, 28), (43, 22), (44, 20)], fill="#c48882", width=2)
    art.line([(38, 27), (36 - ear, 22)], fill="#b7807b")
    art.polygon([(36, 33), (39, 29), (44, 29), (48, 31), (50, 35), (48, 38),
                 (43, 39), (39, 38)], fill=coat, outline=edge)
    art.line([(37, 33), (40, 31), (44, 31), (46, 32)], fill=light, width=2)
    art.polygon([(42, 36), (45, 34), (49, 35), (47, 37), (43, 38)], fill=white)
    art.line((44, 33, 44, 33 if phase == 2 else 34), fill=edge)
    art.point((48, 35), fill="#a16f71")
    if motion == "Patrol":
        art.line((39 + phase, 42, 44 + phase, 42), fill=brown)


BIRDS = {
    13: ("#5d4845", "#8b6655", "#ae8263", "#d1a076", "#e8c498", "#c0835b"),
    18: ("#3f4158", "#4d5975", "#64769a", "#99a9b4", "#efd1b3", "#b87360"),
    24: ("#3a374a", "#44455d", "#565b77", "#777f99", "#79849a", "#777087"),
    25: ("#484c4d", "#586d6e", "#748b85", "#a0a89b", "#edd095", "#98775a"),
    29: ("#554c62", "#79728d", "#9c8da2", "#c4adb8", "#ede0cc", "#b48281"),
}


def draw_bird(art, phase, motion, role):
    edge, dark, wing, light, chest, bill = BIRDS[role]
    long_tail = role in (18, 24)
    tip = 22 if long_tail else 25
    foot = 41 if role == 18 else 43
    if motion in ("Bob", "Nap", "Still", "Tilt") or phase == 0:
        art.line([(34, 39), (34, foot - 1), (32, foot), (36, foot)], fill=bill)
        art.line([(38, 39), (38, foot - 1), (37, foot), (40, foot)], fill=bill)
    art.polygon([(34, 37), (tip, 34), (tip + 1, 37), (tip + 3, 39), (31, 39)],
                fill=dark, outline=edge)
    if role == 18:
        art.line((29, 38, 23, 40), fill=edge)
    art.polygon([(29, 36), (33, 33), (38, 32), (42, 34), (43, 38), (40, 41),
                 (35, 42), (30, 40)], fill=wing, outline=edge)
    art.polygon([(36, 36), (41, 34), (43, 36), (42, 39), (39, 41),
                 (35, 41)], fill=chest)
    art.polygon([(30, 36), (34, 34), (39, 35), (37, 38), (32, 40), (29, 38)],
                fill=dark)
    art.line([(31, 37), (35, 35), (37, 35)], fill=light)
    art.line((31, 39, 35, 37), fill=wing)
    if role in (13, 25, 29):
        art.line((32, 38, 35, 38), fill=light)
    crown = 27 if role in (18, 24) else 29
    art.polygon([(36, 33), (37, crown + 2), (40, crown), (44, crown),
                 (47, crown + 2), (48, 34), (45, 37), (40, 38), (37, 36)],
                fill=wing, outline=edge)
    art.polygon([(38, crown + 2), (40, crown + 1), (44, crown + 1),
                 (46, crown + 3), (41, crown + 3)], fill=light)
    if role in (13, 18, 25):
        art.polygon([(42, 33), (46, 32), (46, 35), (43, 36), (40, 36)], fill=chest)
    if role == 18:
        art.line((43, 36, 46, 35), fill=bill)
    beak_tip = 51 if role == 24 else 49
    art.polygon([(46, 32), (beak_tip, 33), (47, 35)], fill=bill, outline=edge)
    art.point((47, 33), fill=bill)
    art.line((44, 31, 44, 31 if phase == 2 else 32), fill=edge)
    if role in (24, 29) and phase != 2:
        art.point((43, 30), fill=chest)


NATIVE_WILDLIFE = {
    14: draw_squirrel, 15: draw_hedgehog, 16: draw_rabbit, 17: draw_frog, 19: draw_duck,
}


def native_frame(role, phase, motion):
    frame = Image.new("RGBA", SIZE)
    art = ImageDraw.Draw(frame)
    if role == 0:
        draw_keeper(art, phase, motion)
    elif role in CATS:
        draw_cat(art, phase, motion, CATS[role])
    elif role in NATIVE_WILDLIFE:
        NATIVE_WILDLIFE[role](art, phase, motion)
    else:
        draw_bird(art, phase, motion, role)
    return frame


def keeper_costumes():
    """Bake the five costumes in the original 48×32 atlas order."""
    for outfit in range(len(KEEPER_OUTFITS)):
        for phase in range(3):
            frame = Image.new("RGBA", SIZE)
            art = ImageDraw.Draw(frame)
            art.ellipse((25, 42, 47, 45), fill=(60, 46, 55, 57))
            draw_keeper(art, phase, "Patrol", outfit)
            yield frame.resize(SPRITE_SIZE, NEAREST)


def warm_source(source):
    """Give existing cast costumes the same warm top-left light as the new sprites."""
    frame = source.copy()
    pixels = frame.load()
    left, top, right, bottom = frame.getbbox()
    for y in range(top, bottom):
        for x in range(left, right):
            r, g, b, a = pixels[x, y]
            if a == 0:
                continue
            value = max(r, g, b)
            light = 1.0 - .52 * (y - top) / (bottom - top) - .48 * (x - left) / (right - left)
            if value < 92:
                # Soft plum/brown edges keep costume details legible against shaded grass.
                offsets = (9, 3, 6)
            else:
                offsets = (round(15 * light - 4), round(10 * light - 6), round(3 * light - 6))
            graded = tuple(max(0, min(255, value + offset))
                           for value, offset in zip((r, g, b), offsets))
            pixels[x, y] = (*graded, a)
    return frame


def role_pose(role, motion, index, pose):
    if motion == "Nap" and (role == 0 or role in CATS):
        # The lying poses are drawn in profile. Only the small motion of
        # breathing remains, so the sleeper keeps its height and shape.
        return Pose(pose.source, (pose.width + 100) // 2,
                    (pose.height + 200) // 3, sway=pose.sway, lean=pose.lean)
    if role in CRAWLERS:
        # Belly-led animals stay low. They reach forward and contract rather
        # than copying the walking gait of the upright characters.
        slide = (0, 1, 2, 1, 0, -1)[index]
        return Pose(pose.source, pose.width if motion == "Nap" else pose.width + abs(slide) * 3,
                    pose.height if motion == "Nap" else (pose.height + 100) // 2,
                    pose.lift // (1 if role == 17 else 2), pose.sway + slide,
                    (pose.lean + 1) // 2, pose.feet // 2, pose.wing)
    if role in FLIERS:
        # A butterfly has two broad wings in the base art. Contract them
        # around the body instead of stamping the bird's separate wing fan.
        width = pose.width + ((2, -18, -32, -12, 9, -4)[index]
                              if role == 26 and motion in ("Hop", "Patrol") else 0)
        lift = pose.lift + (index in (1, 2, 3)) if motion == "Hop" else pose.lift
        return Pose(pose.source, width, pose.height, lift, pose.sway,
                    pose.lean, 0, pose.wing)
    if role in (14, 16):
        # Keep the squirrel's high tail and rabbit's upright ears readable
        # during the gather and stretch of a jump.
        lift = pose.lift + (index in (2, 3)) if motion == "Hop" else pose.lift
        return Pose(pose.source, pose.width, pose.height, lift, pose.sway,
                    pose.lean, pose.feet, pose.wing)
    return pose


def draw_wing(art, role, box, pose):
    if not pose.wing:
        return
    if role in BIRDS:
        ink, _, feather, glint, _, _ = BIRDS[role]
    elif role in WING_PALETTES:
        ink, feather, glint = WING_PALETTES[role]
    else:
        return
    left, top, right, bottom = box
    x = (left + right) // 2
    y = top + (bottom - top) * 2 // 3
    span = max(3, (right - left) // 3)
    tip = max(1, min(HEIGHT - 4, y - round(pose.wing * PIXEL_RATIO)))
    art.polygon([(x + 1, y), (x - span + 1, tip + 1), (x - span - 3, tip),
                 (x - span - 1, tip + 3), (x - span + 2, tip + 5), (x - 1, y + 3)],
                fill=feather, outline=ink)
    art.line((x - span - 1, tip + 1, x - 2, y), fill=glint)
    art.line((x - span + 2, tip + 3, x - 1, y + 2), fill=ink)


def sharpen_corners(sprite):
    """Scale2x edge rules refine diagonals without averaging away ink colors."""
    width, height = sprite.size
    pixels = sprite.load()
    doubled = Image.new("RGBA", (width * 2, height * 2))
    output = doubled.load()
    for y in range(height):
        for x in range(width):
            center = pixels[x, y]
            north = pixels[x, max(y - 1, 0)]
            west = pixels[max(x - 1, 0), y]
            east = pixels[min(x + 1, width - 1), y]
            south = pixels[x, min(y + 1, height - 1)]
            edges = north != south and west != east
            output[x * 2, y * 2] = west if edges and west == north else center
            output[x * 2 + 1, y * 2] = east if edges and north == east else center
            output[x * 2, y * 2 + 1] = west if edges and west == south else center
            output[x * 2 + 1, y * 2 + 1] = east if edges and south == east else center
    return doubled


def fine_ink(frame, role, box, origin, scaled_size, lean):
    sx, sy, dx, dy = INK_DETAILS[role]
    width, height = scaled_size
    if not (box[0] <= sx < box[2] and box[1] <= sy < box[3]):
        return
    row = (sy - box[1]) * height // (box[3] - box[1])
    x = origin[0] + (sx - box[0]) * width // (box[2] - box[0])
    x += round(lean * (1 - row / max(1, height - 1)) ** 2)
    y = origin[1] + row
    pixels = frame.load()
    for along, change in ((-1, -21), (0, 35), (1, 19)):
        point = (x + dx * along, y + dy * along)
        if not (0 <= point[0] < WIDTH and 0 <= point[1] < HEIGHT):
            continue
        r, g, b, a = pixels[point]
        if a >= 200 and max(r, g, b) >= 95:
            ink = tuple(max(0, min(255, value + change)) for value in (r, g, b))
            pixels[point] = (*ink, a)


def animated_frame(role, motion, source, pose, native):
    bounds = source.getbbox()
    if not bounds:
        raise ValueError(f"Blank source frame for role {role}")
    body = source.crop(bounds)
    scale = 1 if native else PIXEL_RATIO
    anchor_x = PIVOT_X if native else SOURCE_PIVOT_X
    anchor_y = PIVOT_Y if native else SOURCE_PIVOT_Y
    width = round(body.width * scale * pose.width / 100)
    height = round(body.height * scale * pose.height / 100)
    if not native:
        body = sharpen_corners(body)
    body = body.resize((width, height), NEAREST)
    left = PIVOT_X + round((bounds[0] - anchor_x) * scale * pose.width / 100 + pose.sway * PIXEL_RATIO)
    # Squash and stretch around the existing foot or flight anchor.
    bottom = PIVOT_Y + round((bounds[3] - anchor_y) * scale - pose.lift * PIXEL_RATIO)
    top = min(max(bottom - height, 0), HEIGHT - height)
    left = min(max(left, 0), WIDTH - width)
    frame = Image.new("RGBA", SIZE)
    art = ImageDraw.Draw(frame)
    radius = max(3, width * 3 // 8 - pose.lift // 2)
    opacity = max(19, (36 if role in FLIERS else 57) - pose.lift * 5)
    art.ellipse((PIVOT_X - radius, PIVOT_Y - 2, PIVOT_X + radius, PIVOT_Y + 1),
                fill=(60, 46, 55, opacity))
    if motion in ("Hop", "Patrol"):
        draw_wing(art, role, (left, top, left + width, top + height), pose)

    # Row bends keep the face and held tools joined to the body. At the
    # ground the two sides take turns reaching and planting the feet.
    lean = round(pose.lean * PIXEL_RATIO)
    for row in range(height):
        bend = round(lean * (1 - row / max(1, height - 1)) ** 2)
        x = min(max(left + bend, 0), WIDTH - width)
        y = top + row
        if pose.feet and row >= height - max(2, height // 6):
            split = max(1, min(width - 1, PIVOT_X - x))
            for start, stop, direction in ((0, split, -1), (split, width, 1)):
                shift = direction * round(pose.feet * PIXEL_RATIO)
                part = body.crop((start, row, stop, row + 1))
                frame.alpha_composite(part, (max(0, min(WIDTH - part.width, x + start + shift)), y))
        else:
            frame.alpha_composite(body.crop((0, row, width, row + 1)), (x, y))
    if not native:
        fine_ink(frame, role, bounds, (left, top), (width, height), lean)
    return frame


def bake(preview):
    roles = roster_frames()
    if len(roles) != 30:
        raise ValueError(f"Expected 30 roles, got {len(roles)}")
    samples = []
    destination = OUTPUT / "action-animation.bgra"
    with NamedTemporaryFile(dir=OUTPUT, prefix="action-animation-", delete=False) as stream:
        temporary = Path(stream.name)
        try:
            for role, sources in enumerate(roles):
                native = role == 0 or role in CATS or role in BIRDS or role in NATIVE_WILDLIFE
                if not native:
                    sources = tuple(warm_source(source) for source in sources)
                for motion, curve in zip(MOTIONS, POSES):
                    sprites = (tuple(native_frame(role, phase, motion) for phase in range(3))
                               if native else sources)
                    frames = []
                    for index, key in enumerate(curve):
                        pose = role_pose(role, motion, index, key)
                        source = sprites[pose.source % len(sprites)]
                        frames.append(animated_frame(role, motion, source, pose, native))
                    pixels = [bytes(bgra(frame)) for frame in frames]
                    if len(set(pixels)) != FRAME_COUNT:
                        raise ValueError(f"Repeated frames for role {role} in {motion}")
                    stream.writelines(pixels)
                    if preview:
                        samples.append(frames)
            stream.close()
            temporary.replace(destination)
        finally:
            stream.close()
            temporary.unlink(missing_ok=True)
    if preview:
        contact_sheet(samples, preview)


def contact_sheet(samples, destination):
    cell_w, cell_h = 148, 101
    sheet = Image.new("RGB", (34 + len(MOTIONS) * cell_w, 32 + 30 * cell_h), "#242832")
    art = ImageDraw.Draw(sheet)
    for column, name in enumerate(MOTIONS):
        art.text((40 + column * cell_w, 12), name, fill="#e9dbbc")
    for role in range(30):
        y = 32 + role * cell_h
        art.text((8, y + 24), f"{role:02}", fill="#b5c2c5")
        for column in range(len(MOTIONS)):
            x = 34 + column * cell_w
            art.rectangle((x, y, x + cell_w - 3, y + cell_h - 3), fill="#373b46")
            frame = samples[role * len(MOTIONS) + column][2].resize((WIDTH * 2, HEIGHT * 2), NEAREST)
            sheet.paste(frame, (x, y), frame)
    sheet.save(destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview", type=Path, help="optional contact-sheet PNG")
    options = parser.parse_args()
    bake(options.preview)


if __name__ == "__main__":
    main()
