"""Bake original supporting characters in the roster's fixed atlas order."""

from PIL import Image, ImageDraw

from bake_world import OUTPUT, SPRITE_SIZE, bgra


INK = "#4c404f"
SKIN = "#f5c79b"
SHADE = "#c88d7a"
BOOT = "#61525a"
PAPER = "#f1ddb4"
METAL = "#9db2b5"
GOLD = "#e6b373"

# Job coat, shadow, headwear color, hair, and short pixel masks for hat and tool.
HUMANS = (
    ("H02", "#88b78b", "#547a66", "#a0c599", "#8d6657",
     ("......HH.......", ".....HLLH......", "....HLLLHH.....",
      "...HHHHHHH.....", "..HHHHHHHHHH..."),
     ("...H...", "..HHH..", "....H..", "PPPPPPP", "PHHHHHP", ".IIIII.")),
    ("H03", "#bb885f", "#8c6254", "#e8b064", "#6b514c",
     (".....HHHHH.....", "....HHHHHHH....", "...HHHHHHHHH...",
      "...HLLLHHHHH...", "..HHHHHHHHHHH.."),
     (".MMM...", "MMMMM..", "..I....", "..G....", "..G....", "..G....")),
    ("H04", "#686e91", "#4c506e", "#767894", "#584b5b",
     (".....HHHHH.....", "....HHHHHHH....", "...HHHHHHHHH...",
      "...HHH...HHH...", "..HHH.....HHH.."),
     ("..II...", ".IPPI..", ".IGGI..", ".IGLI..", ".IPPI..", "..II...")),
    ("H05", "#8caeb9", "#637e94", "#b6ccce", "#6b565b",
     ("......HHH......", "....HHHHHHH....", "...HLLLLLHHH...",
      "...HHHHHHHHH...", "....HHHHHHH...."),
     ("..MIM..", "..MLM..", "..MLM..", "..MPM..", "..MIM..", ".IIIII.")),
    ("H06", "#d79d96", "#a76f75", "#fff1d5", "#8b5e59",
     ("....HH..HH.....", "...HHHHHHHH....", "...HLLHHLLH...",
      "....HHHHHHH....", "....HHHHHHH...."),
     ("...GG..", "..GPPG.", "..GPPG.", "...GG..", "...I...", "...I...")),
    ("H07", "#668ea9", "#4f6685", "#83a9c0", "#68555b",
     ("......HHH......", "....HHHHHHH....", "....HLLLLHHH...",
      "....HHHHHHHHH..", "........HHHHH.."),
     ("..IIII.", ".IPPPPI", ".IPLPII", ".IPLPPI", ".IPPPPI", "..IIII.")),
    ("H08", "#6f9e9b", "#4f727e", "#e3bd78", "#785b55",
     ("......HHH......", ".....HHHHHH....", "....HLLHHHHH...",
      "...HHHHHHHHH...", "...HHHHHHHHHH.."),
     (".M..M..", ".MMMM..", "..MM...", "...MI..", "...IM..", "....M..")),
    ("H09", "#c5a1c0", "#8c6d98", "#b57f9a", "#946d59",
     ("......HHH......", ".....HHHHH.....", "...HHHHHHHHH...",
      "..HLLHHHHHHHH..", "...HHHHHHHH...."),
     ("..II...", ".IGGIP.", "IGHGPI.", "IGMGLI.", ".IGGI..", "..II...")),
    ("H10", "#ac8bb0", "#786180", "#ce9d86", "#6d534f",
     ("......HHH......", ".....HHHHH.....", "....HLLLHHH....",
      "...HHHHHHHHH...", ".HHHHHHHHHHHHH."),
     (".IIIIII", ".IPGPPI", ".IGPPGI", ".IGGGGI", ".IIIIII")),
)


def stamp(art, rows, x, y, colors):
    for dy, row in enumerate(rows):
        for dx, key in enumerate(row):
            if key != ".":
                art.point((x + dx, y + dy), fill=colors[key])


def human_frames(role, coat, dark, hat, hair, headwear, tool):
    colors = {"I": INK, "P": PAPER, "M": METAL, "G": GOLD, "H": hat, "L": "#fbe6ca"}
    for pose in range(3):
        frame = Image.new("RGBA", SPRITE_SIZE)
        art = ImageDraw.Draw(frame)
        art.ellipse((15, 27, 33, 30), fill=(64, 46, 52, 57))
        for x, step in ((21, pose == 1), (26, pose == 2)):
            art.rectangle((x, 25, x + 2, 28 - step), fill=dark)
            art.line((x - 1, 29 - step, x + 2, 29 - step), fill=BOOT)

        hem = 29 if role == "H04" else 27
        art.polygon([(21, 17), (27, 17), (30, 20), (31, hem),
                     (17, hem), (18, 20)], fill=INK)
        art.polygon([(21, 19), (27, 19), (29, 21), (30, hem - 1),
                     (18, hem - 1), (19, 21)], fill=coat)
        art.polygon([(25, 20), (28, 20), (29, 25), (29, hem - 1), (25, hem - 1)], fill=dark)
        art.line((22, 21, 22, 26), fill=colors["L"] if role in ("H02", "H05") else hat)
        art.point((26, 24), fill=GOLD)
        if role == "H06":
            art.polygon([(21, 22), (26, 22), (28, 27), (20, 27)], fill=PAPER)
        if role == "H07":
            art.line((20, 20, 27, 26), fill=PAPER)
        if role in ("H04", "H10"):
            art.line((19, 21, 28, 21), fill=hat)

        art.rounded_rectangle((18, 9, 29, 19), radius=3, fill=INK)
        art.rounded_rectangle((19, 10, 28, 18), radius=2, fill=hair)
        art.polygon([(21, 12), (27, 12), (28, 15), (26, 18), (22, 18),
                     (20, 16)], fill=SKIN)
        art.point((26, 17), fill=SHADE)
        if pose == 2:
            art.line((23, 15, 26, 15), fill=SHADE)
        else:
            art.point((23, 14), fill=INK)
            art.point((26, 14), fill=INK)
        stamp(art, headwear, 16, 7, colors)
        art.point((29, 22 - (pose == 1)), fill=SKIN)
        stamp(art, tool, 29 + (pose == 2), 17 + (pose == 1), colors)
        yield frame


# Each animal has its own profile and color ramp. Pose offsets are sub-sprite
# movements relative to the fixed foot or flight anchor.
ANIMALS = (
    ("A09", "#655d86", "#a49db6", "#f2dbc9", "#cb806e", "#727da0",
     ("...#.............", "...###....##.....", "....#a##.#bba#...",
      ".....#aa#aab#e...", ".##...#aacc##....", "..####aacc##.....",
      "....#aacc##......", "...##.#a##.......", "..##...##........"),
     ((0, 0), (0, -3), (1, -1))),
    ("A10", "#d89857", "#ffe098", "#ffecc1", "#e88b5e", "#d7a766",
     (".........####....", "........#bbcb#...", "........#bbb#eee.",
      "...##...#abbbee#.", "..#bb###abbb##...", ".#cbbbbbabb#.....",
      ".#abbbbcbb#......", "..#aaaaab#.......", "...######........",
      ".....e..e........"),
     ((0, 0), (1, -1), (0, -1))),
    ("A11", "#a77464", "#ce9679", "#efbd92", "#94af87", "#7e9b82",
     ("....######.......", "...#bbccab#..e.e.", "..#bcaabbb#.###..",
      "..#bca#aab##ded..", ".#dbcaabbb#deed#.", "#dd#bbabb#ddd#...",
      "#dddb#aaa#ddd#...", ".############...."),
     ((0, 0), (1, 0), (-1, 0))),
    ("A12", "#b65866", "#e68b81", "#ffc099", "#636477", "#9a7183",
     (".....##.##....", "....#a#.#a#...", "...#bbbbbba#..", "..#bb#a#bba#..",
      "..#abbb#bba#..", ".d#ab#b#aba#d.", "..#aaba##a#...", "...#######....",
      "..d.d...d.d..."),
     ((0, 0), (0, -1), (1, 0))),
    ("A13", "#637f77", "#95a98d", "#ecf0af", "#f4d987", "#a6c59c",
     (".....b.b.....", "....#.#.#....", "..b#aaaaa#b..", ".bb#ababa#bb.",
      ".b.#accca#.b.", "...#cdddc#...", "....cdcdc....", ".....ccc....."),
     ((0, -1), (0, -3), (1, -2))),
    ("A14", "#b08056", "#f1c978", "#f7edd1", "#f5ad65", "#766275",
     ("....ccc.cc....", "...ccccc.cc...", "...ccccc.cc...", "..##bb#bbb##..",
      ".#abb#bb##bd#.", ".#bb#bb#bbbd#.", "..##bbb##bb#..", "....#######...",
      "......e.e....."),
     ((0, 0), (0, -3), (1, -1))),
    ("A15", "#566278", "#788a9f", "#a8a5ae", "#95928d", "#717184",
     ("..........###.....", ".........#abb#....", "........#aab#ed...",
      ".......#aaabbdd#..", "......#aaaab###...", ".###.#aaabba#.....",
      "..##aaaaaabb#.....", "...#aaaaabbb#.....", "....###aaab#......",
      "......#####.......", ".......d..d......."),
     ((0, 0), (0, -2), (1, 0))),
    ("A16", "#719486", "#cfca7e", "#f4e7c6", "#a0aec1", "#b77f6e",
     ("..........####....", ".........#ddad#...", ".........#c#cae...",
      "....#...#ccccae#..", "...#a###abbca#....", "..#aaddbbbbb#.....",
      ".#aadaabbbbb#.....", "..##aabbba##......", "....######........",
      "......e..e........"),
     ((0, 0), (1, -1), (0, -1))),
    ("A17", "#967aae", "#d29eb6", "#f2d39a", "#776581", "#aeb2cd",
     ("..#####...#####..", ".#bcaab#.#baacb#.", "#bbaaabb#bbaaabb#",
      ".#baebaa#abeab#.", "..#baabd#dbaab#..",
      "..#bcaa#d#aacb#..", "...#abb#d#bba#...", "....###.d.###....",
      "......d...d......"),
     ((0, 0), (0, -3), (1, -1))),
    ("A18", "#78a180", "#acbe85", "#ced09c", "#668878", "#9aa58b",
     (".............###...", "...........##aab#..", "..........#aa#abe..",
      "....##...#abbbc#...", "...#a####aabbc#....", "..#abbaabbbcc#.....",
      ".#b#..##ddd##......", "#d#.....d..d.......", "##.....dd...dd....."),
     ((0, 0), (1, 0), (-1, 0))),
    ("A19", "#a78075", "#c59a86", "#ead3b2", "#dbab9f", "#946d6b",
     ("...........##.....", "..........#bd#....", "......##..#bd#....",
      ".....#ab###bb#....", "....#abbbbbb#be...",
      "...#abbbbccccbe#..", ".##abbbbcccc##....",
      "#d#aaaabbcc##.....", "d..#######........", ".....e...e........"),
     ((0, 0), (1, -1), (0, -1))),
    ("A20", "#c8bfca", "#eee3d8", "#fff2db", "#acabc2", "#c3847b",
     ("..........#####...", ".........#bbbcb#..", "........#bbb#bbe..",
      ".......#bbddbbbe#.", "......#bbbdddd##..", "..#####bbdbbcc#...",
      ".#bbbabbdbbcc#....", "..#babbdbccbc#....", "...##abbbccc#.....",
      ".....#######......", ".......e..e......."),
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
        yield from human_frames(*human)
    for animal in ANIMALS:
        yield from animal_frames(*animal)


def main():
    OUTPUT.mkdir(exist_ok=True)
    with (OUTPUT / "extra-cast.bgra").open("wb") as stream:
        for frame in frames():
            stream.write(bgra(frame))


if __name__ == "__main__":
    main()
