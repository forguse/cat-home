"""Bake original wildlife masters into a standalone two-texel BGRA atlas."""

import argparse
from dataclasses import dataclass
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from bake_premium_assets import ATLAS, Import, NEAREST, load_frame, pack, premultiply_bgra


@dataclass(frozen=True)
class Pose:
    name: str
    x: int
    y: int
    on_ground: bool = False


@dataclass(frozen=True)
class Species:
    name: str
    master: str
    row: int
    scale: float
    poses: tuple[Pose | None, ...]
    edges: tuple[int, ...] = (0, 256, 512, 768, 1024, 1280, 1536)


A = "野生动物-A-哺乳.png"
B = "野生动物-B-地表.png"
C = "野生动物-C-水岸鸟.png"
D = "野生动物-D-园鸟与虫.png"
E = "野生动物-E-飞虫.png"

# Pose anchors use the body root in flight and the center of foot contact on
# land. The few moved crop edges protect long wings, tails, and leap poses.
SPECIES = (
    Species("grey_cat", A, 0, .097, (
        Pose("idle", 120, 223, True), Pose("look", 116, 225, True),
        Pose("walk", 127, 219, True), Pose("walk", 131, 223, True),
        Pose("sleep", 131, 217, True), Pose("stretch", 118, 230, True))),
    Species("black_cat", A, 1, .097, (
        Pose("idle", 121, 223, True), Pose("look", 120, 222, True),
        Pose("walk", 129, 220, True), Pose("walk", 132, 224, True),
        Pose("sleep", 130, 217, True), Pose("stretch", 115, 231, True))),
    Species("squirrel", A, 2, .078, (
        Pose("idle", 139, 221, True), Pose("forage", 135, 222, True),
        Pose("leap", 130, 223, True), Pose("land", 145, 221, True),
        Pose("leap", 130, 223, True), None), (0, 256, 495, 768, 1024, 1280, 1536)),
    Species("hedgehog", A, 3, .072, (
        Pose("idle", 124, 196, True), Pose("sniff", 125, 199, True),
        Pose("walk", 125, 197, True), Pose("walk", 129, 197, True),
        None, Pose("curl", 137, 213, True))),
    Species("rabbit", B, 0, .073, (
        Pose("idle", 132, 245, True), Pose("crouch", 140, 238, True),
        Pose("hop", 144, 245, True), Pose("hop", 149, 244, True),
        Pose("land", 146, 244, True), Pose("groom", 142, 245, True)),
        (0, 256, 512, 810, 1024, 1280, 1536)),
    Species("frog", B, 1, .056, (
        Pose("idle", 125, 198, True), Pose("call", 137, 198, True),
        Pose("hop", 130, 199, True), Pose("hop", 144, 200, True),
        Pose("land", 135, 204, True), None)),
    Species("lizard", B, 2, .064, (
        Pose("idle", 132, 173, True), Pose("scurry", 143, 173, True),
        Pose("scurry", 136, 172, True), Pose("flatten", 136, 171, True),
        Pose("scurry", 143, 173, True), None)),
    Species("field_mouse", B, 3, .055, (
        Pose("idle", 140, 208, True), Pose("forage", 132, 199, True),
        Pose("run", 143, 198, True), Pose("run", 145, 199, True),
        Pose("run", 138, 203, True), Pose("groom", 127, 206, True))),
    Species("sparrow", C, 0, .059, (
        Pose("perch", 157, 223, True), Pose("peck", 149, 222, True),
        Pose("fly", 150, 159), Pose("fly", 139, 149),
        Pose("fly", 156, 155), Pose("land", 152, 200, True)),
        (0, 256, 512, 768, 1044, 1280, 1536)),
    Species("swallow", C, 1, .064, (
        Pose("perch", 161, 211, True), Pose("fly", 151, 169),
        Pose("fly", 158, 164), None, Pose("fly", 140, 162), Pose("land", 146, 215, True)),
        (0, 256, 512, 768, 1041, 1280, 1536)),
    Species("duckling", C, 2, .079, (
        Pose("idle", 130, 212, True), Pose("walk", 130, 209, True),
        Pose("walk", 139, 204, True), Pose("swim", 129, 181),
        Pose("swim", 128, 179), Pose("dabble", 132, 173))),
    Species("crow", C, 3, .076, (
        Pose("idle", 145, 222, True), Pose("walk", 129, 227, True),
        Pose("fly", 142, 155), Pose("fly", 143, 152),
        Pose("fly", 141, 156), Pose("land", 147, 212, True)),
        (0, 256, 512, 748, 1039, 1280, 1536)),
    Species("great_tit", D, 0, .058, (
        Pose("perch", 160, 219, True), Pose("peck", 148, 217, True),
        Pose("fly", 145, 159), Pose("fly", 139, 154),
        Pose("fly", 151, 156), Pose("land", 149, 201, True)),
        (0, 256, 512, 768, 1044, 1280, 1536)),
    Species("white_pigeon", D, 1, .075, (
        Pose("idle", 145, 227, True), Pose("walk", 143, 228, True),
        Pose("fly", 148, 153), Pose("fly", 140, 154),
        Pose("fly", 149, 165), Pose("land", 148, 213, True)),
        (0, 256, 512, 746, 1046, 1280, 1536)),
    Species("snail", D, 2, .044, (
        Pose("idle", 124, 201, True), Pose("crawl", 130, 201, True),
        None, Pose("feel", 129, 202, True), None, Pose("retract", 128, 203, True))),
    Species("ladybird", D, 3, .047, (
        Pose("idle", 124, 178, True), Pose("crawl", 128, 181, True),
        Pose("unfold", 128, 178, True), Pose("fly", 140, 147),
        Pose("fly", 140, 146), None)),
    Species("firefly", E, 0, .046, (
        Pose("idle", 127, 182, True), None, Pose("unfold", 137, 145),
        Pose("fly", 140, 143), Pose("fly", 136, 139), None)),
    Species("honeybee", E, 1, .049, (
        Pose("idle", 132, 184, True), None, Pose("fly", 134, 127),
        Pose("fly", 138, 125), Pose("fly", 143, 123), None)),
    Species("butterfly", E, 2, .058, (
        Pose("perch", 142, 198, True), None, Pose("fly", 141, 152),
        Pose("fly", 140, 151), Pose("fly", 143, 146), None)),
)


def entries(species):
    phases = {}
    for col, pose in enumerate(species.poses):
        if pose is None:
            continue
        phase = phases.get(pose.name, 0)
        phases[pose.name] = phase + 1
        top = 256 * species.row
        bottom = top + 256
        if species.name == "swallow" and col == 4:
            bottom += 8
        bounds = (species.edges[col], top, species.edges[col + 1], bottom)
        id_ = f"wild_{species.name}_{pose.name}_{phase}"
        yield Import(id_, species.master, bounds, (256 * col + pose.x, top + pose.y),
                     (31, 27), scale=species.scale, keep_anchor_margin=pose.on_ground)


def proof(groups, path):
    """Compare the baked sprites at normal output scale and an enlarged view."""
    panel_w, panel_h = 438, 104
    sheet = Image.new("RGB", (panel_w * 2, panel_h * ((len(groups) + 1) // 2)), "#e8e1cf")
    font = ImageFont.truetype("msyh.ttc", 10)
    for i, (species, frames) in enumerate(groups):
        panel = Image.new("RGBA", (panel_w - 2, panel_h - 2), "#93a77c")
        draw = ImageDraw.Draw(panel)
        draw.rectangle((0, 43, panel.width, panel.height), fill="#31434d")
        draw.text((6, 3), species.name, font=font, fill="#272c27")
        for j, frame in enumerate(frames):
            x = 77 + j * 59
            is_grounded = frame.anchor[1] >= frame.image.height * .75
            for scale, baseline in ((1.5, 30 if is_grounded else 20),
                                    (2.5, 98 if is_grounded else 76)):
                width, height = (max(1, round(side * scale)) for side in frame.image.size)
                ax, ay = (round(pos * scale) for pos in frame.anchor)
                panel.alpha_composite(frame.image.resize((width, height), NEAREST), (x - ax, baseline - ay))
            label = frame.id.removeprefix(f"wild_{species.name}_")
            draw.text((x - 20, 31), label, font=font, fill="#272c27")
        sheet.paste(panel, ((i % 2) * panel_w, (i // 2) * panel_h))
    path.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=ATLAS / "source")
    parser.add_argument("--out", type=Path, default=ATLAS / "wildlife")
    parser.add_argument("--preview", type=Path)
    args = parser.parse_args()
    masters = {}
    groups = [(species, [load_frame(args.source, entry, masters) for entry in entries(species)])
              for species in SPECIES]
    frames = [frame for _, group in groups for frame in group]
    for species, group in groups:
        pixels = {(frame.image.size, frame.image.tobytes()) for frame in group}
        if len(pixels) != len(group):
            raise ValueError(f"{species.name}: identical frames after shrinking")
    index, pages = pack(frames)
    args.out.mkdir(parents=True, exist_ok=True)
    for i, page in enumerate(pages):
        (args.out / f"page_{i:02}.bgra").write_bytes(premultiply_bgra(page))
    (args.out / "index.json").write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")
    if args.preview:
        proof(groups, args.preview)
    print(f"{len(SPECIES)} wildlife species, {len(frames)} frames, {len(pages)} pages")


if __name__ == "__main__":
    main()
