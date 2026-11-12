"""Bake original supporting characters in the roster's fixed atlas order."""

from PIL import Image, ImageDraw

from bake_world import OUTPUT, SPRITE_SIZE, bgra


INK = "#4c404f"
PAPER = "#f1ddb4"
GOLD = "#e6b373"

def stamp(art, rows, x, y, colors):
    for dy, row in enumerate(rows):
        for dx, key in enumerate(row):
            if key != ".":
                art.point((x + dx, y + dy), fill=colors[key])


def face(art, center, top, skin, shade, hair, pose):
    art.rounded_rectangle((center - 6, top, center + 6, top + 10), radius=3, fill=INK)
    art.rounded_rectangle((center - 5, top + 1, center + 5, top + 9), radius=2, fill=hair)
    art.polygon([(center - 3, top + 2), (center + 4, top + 2), (center + 5, top + 6),
                 (center + 3, top + 9), (center - 2, top + 9), (center - 4, top + 7)],
                fill=skin)
    if pose == 2:
        art.line((center, top + 6, center + 3, top + 6), fill=shade)
    else:
        art.point((center - 1, top + 5), fill=INK)
        art.point((center + 3, top + 5), fill=INK)
    art.point((center + 3, top + 7), fill=shade)


def feet(art, left, right, top, color, pose):
    for x, raised in ((left, pose == 1), (right, pose == 2)):
        art.rectangle((x, top, x + 2, 28 - raised), fill=color)
        art.line((x - 1, 29 - raised, x + 2, 29 - raised), fill=INK)


def gardener(art, pose):
    # Straw brim, overalls, and an upright seedling distinguish the horticulturist.
    feet(art, 20, 25, 25, "#496751", pose)
    art.polygon([(20, 18), (27, 18), (30, 24), (28, 28), (19, 28), (17, 24)],
                fill="#417754", outline=INK)
    art.rectangle((20, 19, 27, 23), fill="#e6c178")
    art.rectangle((22, 19, 25, 27), fill="#65a273")
    art.line((22, 24, 26, 24), fill="#acc275")
    face(art, 23, 10, "#f5c49a", "#c67f70", "#996054", pose)
    art.polygon([(13, 11), (17, 9), (18, 7), (22, 6), (27, 7), (28, 10),
                 (34, 11), (32, 13), (15, 13)], fill="#e7bc70", outline=INK)
    art.line((18, 10, 28, 10), fill="#9a644f", width=2)
    art.line((16, 11, 31, 11), fill="#f6db98")
    pot = 20 - (pose == 1)
    art.polygon([(11, pot + 1), (17, pot + 1), (16, pot + 5), (12, pot + 5)],
                fill="#ad624e", outline=INK)
    art.line((14, pot + 1, 14, pot - 4), fill="#397754")
    art.polygon([(14, pot - 2), (16, pot - 5), (18, pot - 4), (17, pot - 2)],
                fill="#7eaf52")
    art.line((12, pot - 3, 14, pot - 1), fill="#7eaf52")


def carpenter(art, pose):
    # Tall shoulders, squared boots, a broad hard hat and a diagonal saw.
    feet(art, 19, 27, 24, "#6d514b", pose)
    art.polygon([(19, 15), (28, 15), (33, 20), (32, 26), (16, 26), (15, 20)],
                fill="#af6a4b", outline=INK)
    art.polygon([(20, 16), (27, 16), (29, 25), (19, 25)], fill="#efac56")
    art.line((20, 19, 20, 23), fill="#f7d492", width=2)
    art.line((28, 19, 28, 23), fill="#f7d492", width=2)
    art.line((17, 25, 30, 25), fill="#775047", width=2)
    art.rectangle((23, 24, 25, 26), fill=GOLD)
    face(art, 24, 7, "#dfac82", "#ab705e", "#6b534f", pose)
    art.line((20, 14, 27, 14), fill="#795c54")
    art.polygon([(15, 8), (18, 7), (19, 4), (22, 3), (27, 3), (30, 6),
                 (31, 8), (34, 8), (32, 10), (16, 10)], fill="#f6bc54", outline=INK)
    art.line((22, 5, 27, 5), fill="#ffe1a0")
    y = 16 - (pose == 1)
    art.polygon([(30, y + 3), (40, y - 2), (40, y + 1), (33, y + 6)],
                fill="#c1cbd0", outline=INK)
    art.line((34, y + 3, 39, y), fill="#eff2eb")
    art.rectangle((28, y + 3, 31, y + 7), fill="#7f514d", outline=INK)


def watchkeeper(art, pose):
    # A peaked hood and swinging cape read as one continuous night-blue shape.
    feet(art, 20, 26, 26, "#393d64", pose)
    art.polygon([(21, 5), (28, 5), (31, 9), (31, 15), (34, 24),
                 (35, 27), (31, 28), (15, 28), (17, 25), (18, 14), (18, 9)],
                fill="#444777", outline="#37334f")
    art.polygon([(23, 6), (27, 6), (30, 10), (27, 17), (30, 27),
                 (17, 27), (20, 17), (20, 9)], fill="#676996")
    art.line((28, 18, 30, 26), fill="#a6a2c2")
    art.polygon([(21, 10), (27, 9), (28, 13), (26, 17), (22, 16), (20, 13)],
                fill="#dab18f", outline="#39354f")
    art.point((24, 12), fill=INK)
    if pose != 2:
        art.point((26, 12), fill=INK)
    art.rectangle((23, 18, 25, 19), fill=GOLD)
    lantern = 20 - (pose == 1)
    art.line((16, 18, 12, lantern), fill="#a59a8b")
    art.polygon([(10, lantern + 1), (14, lantern + 1), (16, lantern + 3),
                 (15, lantern + 7), (9, lantern + 7), (8, lantern + 3)],
                fill="#f4bc60", outline=INK)
    art.rectangle((11, lantern + 3, 13, lantern + 5), fill="#fff1a0")


def weather_observer(art, pose):
    # Sleek white field coat, blue goggles and a raised striped wind vane.
    feet(art, 20, 26, 25, "#4a697c", pose)
    art.polygon([(20, 15), (27, 15), (30, 19), (31, 27), (17, 27), (18, 19)],
                fill="#d5e9ea", outline=INK)
    art.line((22, 18, 22, 25), fill="#62899e", width=2)
    art.line((19, 23, 28, 23), fill="#58abc1", width=2)
    art.rectangle((25, 18, 27, 20), fill="#658999")
    face(art, 24, 8, "#b98771", "#805a62", "#424e62", pose)
    art.polygon([(17, 11), (18, 7), (21, 5), (26, 5), (29, 7), (30, 10),
                 (27, 9), (21, 8)], fill="#69a4ba", outline=INK)
    art.line((20, 13, 29, 13), fill="#3c536d", width=2)
    art.line((21, 12, 24, 12), fill="#9ee3ec")
    art.point((27, 12), fill="#9ee3ec")
    art.line((33, 24, 35, 5), fill="#587580", width=2)
    end_y = 6 - (pose == 1)
    art.polygon([(35, 6), (43, end_y), (41, end_y + 4), (35, 10)],
                fill="#ef8b70", outline=INK)
    art.line((38, 6, 38, 8), fill=PAPER, width=2)
    art.rectangle((31, 20, 34, 22), fill="#d5e9ea")


def cook(art, pose):
    # Puffy white toque, rounded sleeves and an unmistakable cream apron.
    feet(art, 20, 27, 25, "#8e4e57", pose)
    art.rounded_rectangle((14, 16, 33, 27), radius=4, fill="#bc626b", outline=INK)
    art.polygon([(20, 18), (27, 18), (31, 26), (17, 26)], fill="#f7e5ca", outline=INK)
    art.line((21, 22, 27, 22), fill="#d59580")
    art.line((24, 22, 24, 25), fill="#d59580")
    face(art, 23, 9, "#f4c89f", "#c7867b", "#8f505a", pose)
    art.ellipse((15, 4, 24, 12), fill="#fef1db", outline=INK)
    art.ellipse((22, 3, 31, 12), fill="#fef1db", outline=INK)
    art.rounded_rectangle((17, 8, 30, 13), radius=2, fill="#fef1db", outline=INK)
    art.line((19, 11, 28, 11), fill="#d6bbaf")
    y = 21 - (pose == 1)
    art.line((31, y + 3, 37, y), fill="#804c50", width=2)
    art.ellipse((34, y - 2, 41, y + 2), fill="#64717d", outline=INK)
    art.line((36, y, 39, y), fill="#f1c884")


def postman(art, pose):
    # A compact post jacket with gold cap badge and a large cross-body satchel.
    feet(art, 20, 25, 23, "#425477", pose)
    art.polygon([(19, 16), (27, 16), (30, 19), (29, 24), (18, 24), (17, 19)],
                fill="#547faf", outline=INK)
    art.line((20, 18, 28, 23), fill="#eccf91", width=2)
    art.rectangle((19, 20, 21, 21), fill="#d1dbee")
    art.rounded_rectangle((25, 21, 32, 27), radius=1, fill="#ac7556", outline=INK)
    art.line((27, 23, 31, 23), fill="#edc489")
    face(art, 23, 9, "#e7b187", "#b4756b", "#63515b", pose)
    art.polygon([(16, 9), (20, 6), (27, 6), (30, 8), (30, 10), (34, 10),
                 (32, 12), (17, 11)], fill="#5177a1", outline=INK)
    art.rectangle((22, 8, 24, 9), fill="#efc678")
    y = 17 - (pose == 1)
    art.rectangle((11, y, 17, y + 5), fill="#faf0cf", outline=INK)
    art.line((12, y + 1, 14, y + 3), fill="#b99273")
    art.line((14, y + 3, 16, y + 1), fill="#b99273")
    art.point((16, y + 1), fill="#d2726d")


def plumber(art, pose):
    # Low teal dungarees, yellow rubber boots, and the wide jaws of a pipe wrench.
    feet(art, 19, 27, 23, "#efb861", pose)
    art.rounded_rectangle((15, 17, 33, 26), radius=3, fill="#2c8b88", outline=INK)
    art.rectangle((19, 19, 28, 24), fill="#62bab0")
    art.line((20, 17, 20, 23), fill="#bee2ba", width=2)
    art.line((28, 17, 28, 23), fill="#bee2ba", width=2)
    art.line((22, 22, 26, 22), fill="#337978")
    face(art, 24, 10, "#c79573", "#996760", "#5c514d", pose)
    art.polygon([(16, 11), (20, 8), (27, 8), (31, 10), (32, 12),
                 (29, 13), (16, 13)], fill="#dda767", outline=INK)
    art.line((19, 10, 27, 10), fill="#eed18b")
    y = 16 - (pose == 1)
    art.line((32, y + 8, 36, y + 2), fill="#d6d9d2", width=3)
    art.polygon([(36, y + 2), (34, y), (35, y - 3), (37, y - 2),
                 (37, y), (39, y + 1), (41, y - 1), (42, y + 1), (39, y + 4)],
                fill="#bac9c8", outline=INK)
    art.point((36, y + 3), fill="#f2f0d7")


def painter(art, pose):
    # Oversize plum beret, trailing scarf, paint palette and upright brush.
    feet(art, 21, 26, 24, "#765170", pose)
    art.polygon([(20, 16), (27, 16), (30, 20), (30, 26), (18, 26), (16, 21)],
                fill="#b47aa3", outline=INK)
    art.polygon([(19, 17), (29, 17), (32, 21), (37, 21), (34, 24),
                 (30, 24), (26, 20), (19, 19)], fill="#73c2b2", outline=INK)
    art.rectangle((21, 21, 26, 24), fill="#e8b4bb")
    art.point((25, 23), fill="#db8e69")
    face(art, 23, 8, "#f4c6a1", "#ba7f7b", "#a76364", pose)
    art.ellipse((13, 5, 31, 11), fill="#965b92", outline=INK)
    art.point((22, 4), fill="#6e4f70")
    art.line((16, 8, 26, 8), fill="#c088ae")
    y = 18 - (pose == 1)
    art.ellipse((10, y, 18, y + 6), fill="#e2bb88", outline=INK)
    art.point((12, y + 3), fill="#da7569")
    art.point((14, y + 2), fill="#7089c1")
    art.point((16, y + 3), fill="#6dbf9e")
    art.line((14, y + 3, 16, y - 5), fill="#7d534f")
    art.point((16, y - 5), fill="#f2c875")


def seed_merchant(art, pose):
    # Tall traveler with a burgundy poncho, broad felt hat and seed-packet crate.
    feet(art, 21, 26, 24, "#5f584e", pose)
    art.polygon([(20, 14), (28, 14), (34, 22), (27, 27), (23, 24),
                 (16, 25), (13, 22)], fill="#a85870", outline=INK)
    art.polygon([(20, 16), (28, 16), (30, 21), (25, 24), (22, 21), (17, 22)],
                fill="#cf887f")
    art.line((16, 23, 22, 22), fill="#e4ab70")
    art.line((27, 25, 32, 22), fill="#e4ab70")
    face(art, 24, 6, "#e4ad87", "#af7063", "#674d55", pose)
    art.line((21, 13, 26, 13), fill="#795758")
    art.polygon([(12, 8), (18, 7), (20, 3), (27, 3), (29, 7), (37, 8),
                 (34, 10), (15, 10)], fill="#955c6c", outline=INK)
    art.line((19, 7, 29, 7), fill="#e4ab70")
    y = 22 - (pose == 1)
    art.polygon([(33, y), (41, y), (40, y + 6), (34, y + 6)],
                fill="#9b7458", outline=INK)
    art.line((34, y + 2, 40, y + 2), fill="#e0b67a")
    for x, color in ((34, "#e7da9f"), (36, "#9dc186"), (39, "#e0a790")):
        art.rectangle((x, y - 3, x + 1, y), fill=color)


# Keep this order in sync with role indexes H02–H10 in the renderer.
HUMANS = (gardener, carpenter, watchkeeper, weather_observer, cook,
          postman, plumber, painter, seed_merchant)


def human_frames(draw_character):
    for pose in range(3):
        frame = Image.new("RGBA", SPRITE_SIZE)
        art = ImageDraw.Draw(frame)
        art.ellipse((15, 27, 33, 30), fill=(64, 46, 52, 57))
        draw_character(art, pose)
        yield frame


# Each animal has its own profile and color ramp. Pose offsets are sub-sprite
# movements relative to the fixed foot or flight anchor.
ANIMALS = (
    ("A09", "#4a5781", "#828cba", "#f4e2ca", "#d77d71", "#727da0",
     ("...#.............", "...###....##.....", "....#a##.#bba#...",
      ".....#aa#aab#e...", ".##...#aacc##....", "..####aacc##.....",
      "....#aacc##......", "...##.#a##.......", "..##...##........"),
     ((0, 0), (0, -3), (1, -1))),
    ("A10", "#df9b44", "#ffd96e", "#fff0ba", "#e88b5e", "#e68b46",
     (".........####....", "........#bbcb#...", "........#bbb#eee.",
      "...##...#abbbee#.", "..#bb###abbb##...", ".#cbbbbbabb#.....",
      ".#abbbbcbb#......", "..#aaaaab#.......", "...######........",
      ".....e..e........"),
     ((0, 0), (1, -1), (0, -1))),
    ("A11", "#a56d52", "#ca8b64", "#f2cb9d", "#9bab73", "#7f9766",
     ("....######.......", "...#bccbab#..e.e.", "..#bcaaabb#.###..",
      "..#bcaccab##ded..", ".#dbcabbbb#deed#.", "#dd#bbccc#ddd#...",
      "#dddb#aaa#ddd#...", ".############...."),
     ((0, 0), (1, 0), (-1, 0))),
    ("A12", "#a74456", "#ed7164", "#ffd09a", "#636477", "#9a7183",
     (".....##.##....", "....#a#.#a#...", "...#bbbbbba#..", "..#bb#a#bba#..",
      "..#abbb#bba#..", ".d#ab#b#aba#d.", "..#aaba##a#...", "...#######....",
      "..d.d...d.d..."),
     ((0, 0), (0, -1), (1, 0))),
    ("A13", "#546b62", "#93ad96", "#e3eb9b", "#f8eb91", "#a6c59c",
     (".....b.b.....", "....#.#.#....", "..b#aaaaa#b..", ".bb#ababa#bb.",
      ".b.#accca#.b.", "...#cdddc#...", "....cdcdc....", ".....ccc....."),
     ((0, -1), (0, -3), (1, -2))),
    ("A14", "#af713e", "#f7c854", "#f5e9d7", "#f5ad65", "#766275",
     ("....ccc.cc....", "...ccccc.cc...", "...ccccc.cc...", "..##bb#bbb##..",
      ".#abb#bb##bd#.", ".#bb#bb#bbbd#.", "..##bbb##bb#..", "....#######...",
      "......e.e....."),
     ((0, 0), (0, -3), (1, -1))),
    ("A15", "#485374", "#68799c", "#b1b4c4", "#887c8b", "#5b637f",
     ("...........####.....", "..........#abbb#....", ".........#aab#beee..",
      "........#aaaaa#eee#.", ".......#aabbb##.....", "......#aabaa#.......",
      ".....#abaaabb#......", "###.#abaaabbb#......", ".##aaaaaabbb#.......",
      "...###aabb##........", ".......###..........", ".......d.d.........."),
     ((0, 0), (0, -2), (1, 0))),
    ("A16", "#67948a", "#e1ca62", "#f6edd6", "#56647c", "#b38269",
     (".......####....", "......#dddd#...", ".....#dc#cde#..",
      ".....#dccdde#..", "....#dbbadd#...", "...#abbbaab#...",
      ".##aabbbaa#....", "..##bbaab#.....", "....####.......",
      ".....e.e......."),
     ((0, 0), (1, -1), (0, -1))),
    ("A17", "#8f65ad", "#d095c9", "#ffdb9d", "#776581", "#aeb2cd",
     ("..#####...#####..", ".#bcaab#.#baacb#.", "#bbaaabb#bbaaabb#",
      ".#baebaa#abeab#.", "..#baabd#dbaab#..",
      "..#bcaa#d#aacb#..", "...#abb#d#bba#...", "....###.d.###....",
      "......d...d......"),
     ((0, 0), (0, -3), (1, -1))),
    ("A18", "#709569", "#acd276", "#e5dca0", "#507e6a", "#9aa58b",
     (".............###...", "...........##aab#..", "..........#aa#abe..",
      "....##...#abbbc#...", "...#a####abdbc#....", "..#abdabdbbcc#.....",
      ".#b#..##ddd##......", "#d#.....d..d.......", "##.....dd...dd....."),
     ((0, 0), (1, 0), (-1, 0))),
    ("A19", "#a77b77", "#c89b87", "#eed9b9", "#e9a6ae", "#a06d78",
     ("...........##.....", "..........#bd#....", "......##..#bd#....",
      ".....#ab###bb#....", "....#abbbbbb#be...",
      "...#abbbbccccbe#..", ".##abbbbcccc##....",
      "#d#aaaabbcc##.....", "d..#######........", ".....e...e........"),
     ((0, 0), (1, -1), (0, -1))),
    ("A20", "#c3a6bd", "#eee5dd", "#fff8e3", "#a99dbb", "#d58d80",
     ("...........####....", "..........#cccb#...", ".........#c#cbe#..",
      ".........#cbbbe#..", "........#caddb##..", ".##....#ccdaab#...",
      "#bb####cbdaabb#...", "#cbcbcbcbabcbc#...", ".#cbcbcbcbcccc#...",
      "..##bbbbbccc##....", "....########......", "......e...e......."),
     ((0, 0), (0, -2), (1, -1))),
)


def animal_frames(role, first, second, third, fourth, fifth, rows, offsets):
    width = max(map(len, rows))
    left = 24 - width // 2
    colors = dict(zip("#abcde", (INK, first, second, third, fourth, fifth)))
    airborne = role in ("A09", "A13", "A14", "A17")
    for pose, (dx, dy) in enumerate(offsets):
        frame = Image.new("RGBA", SPRITE_SIZE)
        art = ImageDraw.Draw(frame)
        top = (25 if airborne else 29) - len(rows) + dy
        art.ellipse((left + 2, 28, left + width - 2, 30),
                    fill=(61, 48, 53, 28 if airborne else 52))
        if role == "A13" and pose != 2:
            glow_x, glow_y = left + dx + 6, top + 6
            art.ellipse((glow_x - 4, glow_y - 4, glow_x + 4, glow_y + 4),
                        fill=(238, 216, 127, 80 if pose == 1 else 46))
        stamp(art, rows, left + dx, top, colors)
        yield frame


def frames():
    for human in HUMANS:
        yield from human_frames(human)
    for animal in ANIMALS:
        yield from animal_frames(*animal)


def main():
    OUTPUT.mkdir(exist_ok=True)
    with (OUTPUT / "extra-cast.bgra").open("wb") as stream:
        for frame in frames():
            stream.write(bgra(frame))


if __name__ == "__main__":
    main()
