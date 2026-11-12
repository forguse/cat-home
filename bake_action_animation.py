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
    13: ("#6b4e53", "#ae806c", "#d4aa87"),
    18: ("#434867", "#6374a4", "#94a2ca"),
    22: ((101, 128, 111, 170), (169, 188, 144, 170), (211, 226, 162, 140)),
    23: ((139, 126, 122, 190), (220, 209, 187, 195), (252, 238, 212, 220)),
    24: ("#3c445e", "#546387", "#8190b0"),
    25: ("#4b616c", "#6c888c", "#a4b2a1"),
    29: ("#867e98", "#c5bbca", "#f5e9e7"),
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


def role_pose(role, motion, index, pose):
    if role in CRAWLERS:
        # Belly-led animals stay low. They reach forward and contract rather
        # than copying the walking gait of the upright characters.
        slide = (0, 1, 2, 1, 0, -1)[index]
        return Pose(pose.source, pose.width + abs(slide) * 3,
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
    if role not in WING_PALETTES or not pose.wing:
        return
    ink, feather, glint = WING_PALETTES[role]
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


def animated_frame(role, motion, source, pose):
    bounds = source.getbbox()
    if not bounds:
        raise ValueError(f"Blank source frame for role {role}")
    body = source.crop(bounds)
    width = round(body.width * PIXEL_RATIO * pose.width / 100)
    height = round(body.height * PIXEL_RATIO * pose.height / 100)
    body = sharpen_corners(body).resize((width, height), NEAREST)
    left = PIVOT_X + round(((bounds[0] - SOURCE_PIVOT_X) * pose.width / 100 + pose.sway) * PIXEL_RATIO)
    # Squash and stretch around the existing foot or flight anchor.
    bottom = PIVOT_Y + round((bounds[3] - SOURCE_PIVOT_Y - pose.lift) * PIXEL_RATIO)
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
    fine_ink(frame, role, bounds, (left, top), (width, height), lean)
    return frame


def bake(preview):
    roles = roster_frames()
    if len(roles) != 30:
        raise ValueError(f"Expected 30 roles, got {len(roles)}")
    cat_sleep = source_frames("cat_sleep")
    cat_walk = source_frames("cat_walk")
    samples = []
    destination = OUTPUT / "action-animation.bgra"
    with NamedTemporaryFile(dir=OUTPUT, prefix="action-animation-", delete=False) as stream:
        temporary = Path(stream.name)
        try:
            for role, sources in enumerate(roles):
                for motion, curve in zip(MOTIONS, POSES):
                    if role == 10 and motion == "Nap":
                        sprites = cat_sleep
                    elif role == 10 and motion == "Patrol":
                        sprites = cat_walk
                    else:
                        sprites = sources
                    frames = []
                    for index, key in enumerate(curve):
                        pose = role_pose(role, motion, index, key)
                        source = sprites[(index if sprites is cat_walk else pose.source) % len(sprites)]
                        frames.append(animated_frame(role, motion, source, pose))
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
