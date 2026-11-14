"""Bake native pixel sprites for visiting tradespeople."""

import argparse
from dataclasses import dataclass
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from build_basketball_pack import (
    FOOT, INK, NEAREST, ROOT, Frame, Outfit, Pose, pack_pages, place, player, scale_point,
)
from build_life_assets import crop, long_tool, vessel
from build_visitor_nameplates import label_frames, runtime_labels


OUTPUT = ROOT / "assets" / "visitors"
SIZE = (34, 34)


@dataclass(frozen=True)
class Profession:
    id: str
    title: str
    outfit: Outfit
    silhouette: str
    prop: str
    tall: bool = False


PROFESSIONS = (
    Profession("mail_carrier", "送信员",
               Outfit("#5c6681", "#91a4a7", "#46506a", "#e7b790", "#b87d6c", "#6b5552",
                      "#607593", "#849db0", "#465b7b", "#bdc4a5", "#59535e"),
               "visor", "parcel", True),
    Profession("seed_merchant", "种子商",
               Outfit("#7e7655", "#b7a275", "#b48567", "#e4ae84", "#b16e5e", "#65534b",
                      "#77926f", "#a4aa76", "#766052", "#cda876", "#655744"),
               "felt", "seeds"),
    Profession("carpenter", "木匠",
               Outfit("#d9974d", "#f3ce83", "#996655", "#e9b489", "#b87665", "#5c4b4b",
                      "#b1765b", "#ca8d65", "#854d48", "#c79a73", "#785f51"),
               "hardhat", "saw", True),
    Profession("cook", "厨师",
               Outfit("#d9c8ac", "#f2dec0", "#bcab91", "#f5c69f", "#bd7b68", "#855858",
                      "#ab7268", "#d0a18b", "#debea0", "#f0d5b1", "#815b55"),
               "toque", "meal"),
    Profession("fisher", "钓客",
               Outfit("#598486", "#92a9a0", "#4e6a75", "#c99879", "#9a655d", "#4f4e5a",
                      "#648586", "#94a99c", "#4f626d", "#b2c0aa", "#545f61"),
               "bucket", "rod", True),
    Profession("weaver", "织补匠",
               Outfit("#9c6983", "#c890a2", "#7f536e", "#e9b697", "#b77a6b", "#664653",
                      "#967892", "#b99eab", "#82617f", "#d3a7aa", "#644e66"),
               "bonnet", "cloth"),
    Profession("fruit_grower", "果农",
               Outfit("#ba8f54", "#e3bc72", "#a27858", "#d9a47d", "#a46e62", "#5d5049",
                      "#8e9061", "#a9a977", "#6e704f", "#b4b481", "#615345"),
               "straw", "fruit", True),
    Profession("florist", "花匠",
               Outfit("#b88480", "#e2aa92", "#966c73", "#f0ba95", "#be816d", "#8c6057",
                      "#8da184", "#b0b393", "#b08083", "#d3b2a0", "#775b69"),
               "flower", "bouquet"),
    Profession("traveling_painter", "行脚画师",
               Outfit("#607584", "#92a8a2", "#53606f", "#e8b28a", "#af7464", "#62505a",
                      "#7b8397", "#a0acb5", "#606780", "#c3bdb1", "#5c5568"),
               "beret", "palette"),
    Profession("road_worker", "修路工",
               Outfit("#c19c57", "#edca83", "#9a774d", "#d7a581", "#9c6559", "#6b514d",
                      "#867166", "#b6916d", "#cf9b59", "#f4d293", "#5f5750"),
               "helmet", "mallet", True),
)


POSES = {
    "idle": (Pose((32, 32), (18, 33)), Pose((33, 31), (18, 33), crouch=1),
             Pose((32, 31), (18, 32), expression="smile")),
    "walk": (Pose((31, 34), (18, 31), stride=-3), Pose((34, 30), (16, 32), lift=1),
             Pose((32, 32), (17, 34), stride=3)),
    "greet": (Pose((34, 28), (27, 29), 1), Pose((38, 20), (29, 29), 3),
              Pose((35, 23), (27, 30), 1, 1, expression="smile")),
    "work": (Pose((34, 25), (23, 31), 1, 1), Pose((37, 34), (26, 31), 4, 6),
             Pose((35, 30), (24, 30), 2, 3)),
    "carry": (Pose((33, 28), (27, 28), 1, stride=-3), Pose((35, 26), (28, 27), 1, lift=1),
              Pose((34, 28), (28, 28), 1, stride=3)),
    "rest": (Pose((29, 34), (18, 35), 0, 4), Pose((30, 38), (18, 39), 0, 9),
             Pose((30, 38), (19, 38), -1, 9, expression="tired")),
}


def uniform(draw, job, pose):
    x = 23 + pose.lean
    y = 11 + pose.crouch - pose.lift - int(job.tall)
    cap, light, band = job.outfit.cap, job.outfit.cap_light, job.outfit.cap_band
    if job.silhouette == "toque":
        draw.rounded_rectangle((x - 7, y - 8, x + 7, y + 2), radius=4, fill=light, outline=INK)
        for dx in (-3, 0, 3):
            draw.line((x + dx, y - 6, x + dx, y - 1), fill=cap)
        draw.line((x - 5, y + 1, x + 5, y + 1), fill=band)
    elif job.silhouette in ("helmet", "hardhat"):
        draw.arc((x - 7, y - 6, x + 7, y + 5), 180, 360, fill=light, width=2)
        draw.line((x, y - 5, x, y + 1), fill=cap)
        if job.silhouette == "helmet":
            draw.rectangle((x + 3, y + 2, x + 6, y + 4), fill="#f1d391")
    elif job.silhouette in ("beret", "visor", "felt"):
        draw.ellipse((x - 8, y - 6, x + 7, y + 2), fill=cap, outline=INK)
        draw.line((x - 5, y - 4, x + 3, y - 4), fill=light)
        if job.silhouette == "beret":
            draw.point((x - 1, y - 7), fill=INK)
        elif job.silhouette == "visor":
            draw.rectangle((x + 3, y + 2, x + 11, y + 4), fill=band, outline=INK)
            draw.point((x + 2, y - 1), fill="#ddba7d")
    elif job.silhouette == "bucket":
        draw.polygon([(x - 5, y - 4), (x + 4, y - 4), (x + 7, y + 2), (x + 11, y + 4),
                      (x - 9, y + 5), (x - 7, y + 2)], fill=cap, outline=INK)
        draw.line((x - 5, y, x + 5, y - 1), fill=light)
    elif job.silhouette == "bonnet":
        draw.line([(x - 6, y + 2), (x - 8, y + 7), (x - 7, y + 14)], fill=cap, width=3)
        draw.line((x - 7, y + 13, x - 3, y + 15), fill=light, width=2)
        draw.polygon([(x - 5, y + 13), (x, y + 15), (x - 2, y + 20)], fill=cap, outline=INK)
    elif job.silhouette == "straw":
        draw.line((x - 11, y + 5, x + 11, y + 3), fill=light, width=2)
    else:
        draw.ellipse((x + 1, y - 2, x + 6, y + 3), fill="#f3cc89", outline="#a97270")
        draw.point((x + 4, y), fill="#b26371")


def job_prop(draw, prop, pose, action, phase):
    x, y = pose.lead
    if action == "greet" and phase == 1:
        return
    if action == "rest":
        x, y = 38, 36
    if prop == "parcel":
        draw.polygon([(x - 5, y - 2), (x + 3, y - 2), (x + 6, y + 1), (x + 4, y + 7),
                      (x - 5, y + 6)], fill="#c69869", outline=INK)
        draw.line((x, y - 1, x, y + 6), fill="#ead4a6")
    elif prop == "seeds":
        draw.ellipse((x - 4, y + 1, x + 6, y + 9), fill="#b6946c", outline=INK)
        draw.line((x - 2, y, x + 4, y + 1), fill="#e5c997", width=2)
        draw.point((x + 1, y + 5), fill="#829067")
    elif prop in ("saw", "mallet", "rod"):
        target = (46, y - 8 if action == "work" and phase == 0 else y + 2)
        if prop == "saw":
            draw.polygon([(x - 1, y - 2), (target[0], target[1] - 2), (target[0] - 1, target[1] + 2),
                          (x + 2, y + 3)], fill="#c3cbc0", outline=INK)
            draw.line((x + 2, y, target[0] - 2, target[1] - 1), fill="#e9e4c4")
        else:
            endpoint = (45, 7) if prop == "rod" else target
            long_tool(draw, "rod" if prop == "rod" else "hammer", (x, y), endpoint)
            if prop == "rod" and action == "work":
                draw.line((45, 7, 45, 33), fill="#b3bab3")
    elif prop == "meal":
        vessel(draw, "bowl", (x + 1, y + 2))
        draw.line((x - 3, y + 1, x + 4, y + 1), fill="#b7a66f", width=2)
    elif prop == "cloth":
        draw.polygon([(x - 6, y + 1), (x + 6, y - 1), (x + 7, y + 6), (x - 4, y + 8)],
                     fill="#b58193", outline=INK)
        draw.line((x - 4, y + 3, x + 5, y + 1), fill="#e2adae")
    elif prop == "fruit":
        vessel(draw, "basket", (x - 2, y))
        crop(draw, (x + 1, y + 3))
    elif prop == "bouquet":
        draw.line([(x - 3, y + 4), (x + 3, y - 6), (x + 7, y - 8)], fill="#779368", width=2)
        for dx, dy, color in ((1, -6, "#d997aa"), (6, -9, "#e8bd7d"), (8, -5, "#e0b1bc")):
            draw.ellipse((x + dx - 2, y + dy - 2, x + dx + 2, y + dy + 2), fill=color, outline=INK)
    else:
        draw.ellipse((x - 3, y - 2, x + 10, y + 6), fill="#c79569", outline=INK)
        for dx, dy, color in ((1, 1, "#bc756f"), (4, 3, "#738f83"), (7, 0, "#e9c976")):
            draw.point((x + dx, y + dy), fill=color)


def visitor_frames():
    frames = []
    foot = scale_point(FOOT, (48, 48), SIZE)
    for job in PROFESSIONS:
        for action, poses in POSES.items():
            for phase, pose in enumerate(poses):
                figure = player(pose, job.outfit, job.tall)
                uniform(ImageDraw.Draw(figure), job, pose)
                image = Image.new("RGBA", (48, 48))
                job_prop(ImageDraw.Draw(image), job.prop, pose, action, phase)
                image.alpha_composite(figure)
                frames.append(Frame(f"{job.id}_{action}_{phase}", image.resize(SIZE, NEAREST),
                                    foot, "human", scale_point(pose.lead, (48, 48), SIZE)))
    return frames


def preview(frames, output):
    by_id = {f.id: f for f in frames}
    image = Image.new("RGB", (1180, 645), "#222c28")
    draw = ImageDraw.Draw(image)
    title = ImageFont.truetype("msyhbd.ttc", 27)
    label = ImageFont.truetype("msyh.ttc", 16)
    draw.text((23, 15), "庭院来客 · 十种职业", font=title, fill="#efe0ba")
    for row, job in enumerate(PROFESSIONS):
        col, line = divmod(row, 5)
        x, y = 23 + col * 578, 63 + line * 112
        draw.rounded_rectangle((x, y, x + 556, y + 101), radius=7, fill="#39483f")
        draw.text((x + 9, y + 8), job.title, font=label, fill="#efe0ba")
        plate = by_id[f"{job.id}_label"].image
        image.paste(plate, (x + 9, y + 31), plate)
        for i, (action, name) in enumerate(zip(POSES, ("等候", "步行", "招呼", "工作", "搬运", "休息"))):
            panel = Image.new("RGBA", (34, 34))
            place(panel, by_id[f"{job.id}_{action}_1"], (16, 30))
            panel = panel.resize((68, 68), NEAREST)
            px = x + 80 + i * 76
            image.paste(panel, (px, y + 4), panel)
            draw.text((px + 12, y + 76), name, font=label, fill="#a9b6a8")
    output.parent.mkdir(parents=True, exist_ok=True)
    image.save(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview", type=Path)
    args = parser.parse_args()
    labels = runtime_labels()
    if [name for name, _ in labels] != [job.id for job in PROFESSIONS]:
        raise ValueError("Visitor animation and label asset IDs differ")
    frames = visitor_frames() + label_frames(labels)
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
