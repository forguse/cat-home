"""Bake small, dependency-free render atlases for the Windows cat garden.

Tiny Town by Kenney and Pet Cats Pack by LuizMelo are CC0. The original
geometry, recolors, props, gardener and sparrow are composed in this file
and bake_scene.py. Nothing in here runs with the desktop app.
"""

from pathlib import Path

from PIL import Image, ImageDraw

from bake_scene import CatSprites, NEAREST, SIZE, lawn_texture, make_scene, tiles


OUTPUT = Path(__file__).resolve().parent / "assets" / "world"
SPRITE_SIZE = (48, 32)
LIGHT = {
    "dawn": ((0.91, 0.88, 0.91), (19, 9, 18), (255, 190, 119), 0.22),
    "day": ((1.00, 1.00, 1.00), (0, 0, 0), (0, 0, 0), 0.00),
    "dusk": ((0.88, 0.70, 0.82), (28, 13, 24), (255, 171, 108), 0.36),
    "night": ((0.55, 0.63, 0.80), (10, 12, 26), (255, 184, 100), 0.55),
}


def base_yard():
    tile_set = tiles()
    # Reserve a proper patch beside the visitor path for the future flowers.
    tile_set[83] = Image.new("RGBA", (16, 16))
    scene = make_scene(tile_set, water_bowl=False)
    art = ImageDraw.Draw(scene)
    art.line((134, 80, 138, 80), fill="#a6bd88")
    art.line((137, 84, 141, 84), fill="#849f71")
    art.point((135, 83), fill="#bbcc99")
    return scene


def lit_color(color, scale, lift, lamp, glow):
    return tuple(
        min(255, round((value * scale[channel] + lift[channel]) * (1 - glow) + lamp[channel] * glow))
        for channel, value in enumerate(color)
    )


def lighting(scene, name):
    scale, lift, lamp, strength = LIGHT[name]
    frame = scene.copy()
    pixels = []
    for index, (red, green, blue, alpha) in enumerate(scene.getdata()):
        if not alpha:
            pixels.append((0, 0, 0, 0))
            continue
        x, y = index % SIZE[0], index // SIZE[0]
        # A narrow pool of warm light falls across the doorstep and grass.
        distance = ((x - 36) / 16) ** 2 + ((y - 64) / 11) ** 2
        glow = strength * max(0.0, 1.0 - distance) if alpha == 255 else 0
        if y < 56:
            glow *= 0.5
        lit = lit_color((red, green, blue), scale, lift, lamp, glow)
        pixels.append((*lit, alpha))
    frame.putdata(pixels)
    if strength > 0:
        art = ImageDraw.Draw(frame)
        art.rectangle((33, 58, 37, 63), fill="#ffdf97" if name != "night" else "#ffce75")
        art.line((35, 58, 35, 63), fill="#b77857")
        art.line((33, 60, 37, 60), fill="#b77857")
        art.point((34, 59), fill="#fff1bf")
        art.point((36, 62), fill="#fff1bf")
        art.line((33, 64, 37, 64), fill="#dc955b")
    if name == "night":
        ImageDraw.Draw(frame).point((143, 46), fill=(255, 223, 143, 192))
        ImageDraw.Draw(frame).point((100, 57), fill=(248, 215, 137, 185))
        ImageDraw.Draw(frame).point((17, 67), fill=(235, 228, 152, 176))
    return frame


def terrain_frames():
    """Clock phase first, then two lawn tiles, stepping stones and flowers."""
    tile_set = tiles()
    plain = lawn_texture(tile_set[0])
    varied = lawn_texture(tile_set[1])
    stones = plain.copy()
    stones.alpha_composite(lawn_texture(tile_set[43]))
    flowers = varied.copy()
    art = ImageDraw.Draw(flowers)
    art.point((5, 10), fill="#637d54")
    art.line((4, 9, 6, 9), fill="#fff0aa")
    art.point((5, 8), fill="#ffcc92")
    art.point((12, 5), fill="#637d54")
    art.point((12, 4), fill="#e6b2a3")
    for scale, lift, lamp, _ in LIGHT.values():
        for tile in (plain, varied, stones, flowers):
            frame = tile.copy()
            frame.putdata([
                (*lit_color((red, green, blue), scale, lift, lamp, 0), alpha)
                for red, green, blue, alpha in tile.getdata()
            ])
            yield frame


def flower_bed(art):
    art.ellipse((126, 82, 149, 89), fill=(56, 60, 58, 78))
    art.polygon([(125, 80), (129, 77), (147, 77), (151, 80), (149, 87),
                 (127, 87)], fill="#51404b")
    art.polygon([(127, 80), (130, 78), (146, 78), (149, 80), (148, 83),
                 (128, 83)], fill="#856453")
    art.line((128, 84, 148, 84), fill="#ce9474")
    art.line((129, 85, 147, 85), fill="#9d695a")
    for x, height, color in (
        (130, 5, "#f2bf8d"), (134, 8, "#e28ba4"), (138, 6, "#fcda9f"),
        (142, 9, "#c5a3cf"), (146, 5, "#f3a798"),
    ):
        top = 81 - height
        art.line((x, top, x, 82), fill="#496b53")
        art.point((x + 1, 79), fill="#8da675")
        art.line((x - 1, top, x + 1, top), fill=color)
        art.line((x, top - 1, x, top + 1), fill=color)
        art.point((x, top), fill="#f7e6ad")


def workbench(art):
    art.ellipse((87, 60, 114, 68), fill=(49, 61, 55, 74))
    art.rectangle((90, 55, 92, 65), fill="#5b4450")
    art.rectangle((107, 55, 110, 65), fill="#5b4450")
    art.line((91, 60, 109, 60), fill="#956d56")
    art.polygon([(87, 52), (91, 50), (109, 50), (113, 52), (113, 57),
                 (87, 57)], fill="#634b54")
    art.polygon([(88, 53), (92, 51), (108, 51), (112, 53), (112, 54),
                 (88, 54)], fill="#dba981")
    art.line((88, 55, 112, 55), fill="#a5745e")
    art.rectangle((106, 47, 109, 51), fill="#688b84")
    art.line((107, 46, 110, 46), fill="#8eb4a0")
    art.point((110, 49), fill="#8eb4a0")
    art.line((94, 52, 99, 52), fill="#687489")
    art.point((94, 51), fill="#cfd4d9")
    art.point((100, 52), fill="#9f6c55")


def pavilion(art):
    art.ellipse((111, 54, 149, 66), fill=(43, 58, 58, 84))
    art.polygon([(111, 59), (116, 55), (146, 55), (151, 60),
                 (146, 63), (116, 63)], fill="#5f4e5a")
    art.line((117, 59, 146, 59), fill="#c58f70")
    art.line((119, 61, 144, 61), fill="#a17364")
    for x in (118, 142):
        art.rectangle((x, 39, x + 2, 58), fill="#604959")
        art.line((x + 1, 40, x + 1, 57), fill="#c08b70")
    art.line((121, 50, 141, 50), fill="#765b5e")
    art.line((121, 52, 141, 52), fill="#ac846e")
    for x in (124, 131, 138):
        art.line((x, 50, x, 55), fill="#77595f")
    art.polygon([(111, 40), (114, 36), (118, 34), (118, 32), (126, 28),
                 (126, 26), (133, 26), (133, 28), (141, 32), (141, 34),
                 (147, 37), (150, 40), (149, 42), (112, 42)], fill="#463e52")
    art.polygon([(114, 39), (120, 35), (120, 33), (129, 29), (131, 28),
                 (139, 32), (139, 34), (146, 38), (147, 39)], fill="#70869a")
    art.line((115, 39, 146, 39), fill="#8ea7ae")
    art.line((119, 36, 141, 36), fill="#5c7089")
    art.line((124, 33, 137, 33), fill="#a4b4b5")
    art.line((130, 28, 130, 31), fill="#c9ccba")
    art.line((121, 43, 141, 43), fill="#a7786e")
    art.line((130, 43, 130, 47), fill="#584554")
    art.rectangle((128, 47, 132, 50), fill="#885b57")
    art.rectangle((129, 47, 131, 49), fill="#f0c982")
    art.point((130, 48), fill="#ffedb3")


def builds():
    overlay = Image.new("RGBA", SIZE)
    yield overlay.copy()
    art = ImageDraw.Draw(overlay)
    flower_bed(art)
    yield overlay.copy()
    workbench(art)
    yield overlay.copy()
    pavilion(art)
    yield overlay


def flower_bed_snapshots():
    overlay = Image.new("RGBA", SIZE)
    art = ImageDraw.Draw(overlay)
    art.ellipse((126, 82, 149, 89), fill=(56, 60, 58, 78))
    art.polygon([(126, 81), (130, 78), (146, 78), (150, 81), (148, 85),
                 (128, 85)], fill="#8e7262")
    art.line((131, 81, 146, 81), fill="#a5896b")
    art.line((130, 83, 147, 83), fill="#725d55")
    for x in (128, 149):
        art.line((x, 76, x, 81), fill="#705251")
        art.point((x, 76), fill="#d8a780")
    art.line((129, 77, 148, 77), fill="#d4b589")
    art.line((143, 79, 147, 85), fill="#614e54")
    art.line((144, 80, 146, 83), fill="#cb9c72")
    art.line((145, 85, 147, 85), fill="#82939c")
    yield overlay.copy()

    art.polygon([(125, 80), (129, 77), (147, 77), (151, 80), (149, 87),
                 (127, 87)], fill="#51404b")
    art.polygon([(127, 80), (130, 78), (146, 78), (149, 80), (148, 83),
                 (128, 83)], fill="#856453")
    art.line((128, 84, 139, 84), fill="#ce9474")
    art.line((130, 81, 146, 81), fill="#a58066")
    art.line((131, 83, 145, 83), fill="#664c4d")
    art.line((142, 87, 150, 85), fill="#9c6e59")
    art.line((142, 86, 150, 84), fill="#e0a881")
    yield overlay.copy()

    art.line((128, 84, 148, 84), fill="#ce9474")
    art.line((129, 85, 147, 85), fill="#9d695a")
    art.line((142, 86, 148, 86), fill="#51404b")
    art.point((150, 84), fill="#51404b")
    for x, y in ((132, 79), (138, 80), (144, 78)):
        art.line((x, y - 2, x, y + 1), fill="#496b53")
        art.point((x - 1, y - 2), fill="#8da675")
        art.point((x + 1, y - 3), fill="#a1b587")
    yield overlay


def workbench_snapshots():
    overlay = Image.new("RGBA", SIZE)
    art = ImageDraw.Draw(overlay)
    art.ellipse((87, 60, 114, 68), fill=(49, 61, 55, 74))
    art.polygon([(88, 61), (93, 58), (109, 58), (113, 61), (111, 65),
                 (89, 65)], fill="#654b53")
    art.line((92, 60, 110, 60), fill="#e0a983")
    art.line((90, 62, 112, 62), fill="#a77460")
    art.line((90, 64, 110, 64), fill="#c99173")
    art.line((95, 58, 96, 64), fill="#4f424b")
    art.line((106, 58, 107, 64), fill="#4f424b")
    yield overlay.copy()

    for left, right in ((89, 93), (106, 110)):
        art.line((left + 2, 53, left, 63), fill="#604a52", width=2)
        art.line((left + 2, 54, right, 63), fill="#604a52", width=2)
        art.point((left + 2, 54), fill="#c89573")
        art.line((left, 60, right, 60), fill="#b78868")
    art.line((91, 55, 108, 55), fill="#634b54", width=2)
    art.line((92, 54, 107, 54), fill="#d29d78")
    yield overlay.copy()

    art.rectangle((90, 55, 92, 65), fill="#5b4450")
    art.rectangle((107, 55, 110, 65), fill="#5b4450")
    art.line((91, 60, 109, 60), fill="#956d56")
    art.polygon([(87, 52), (91, 50), (109, 50), (113, 52), (113, 56),
                 (87, 56)], fill="#634b54")
    art.line((89, 52, 111, 52), fill="#dba981")
    art.line((88, 54, 105, 54), fill="#c58b6c")
    art.line((93, 51, 97, 51), fill="#73798a")
    art.line((95, 51, 95, 54), fill="#a77561")
    art.point((93, 50), fill="#b8c2cb")
    yield overlay


def pavilion_snapshots():
    overlay = Image.new("RGBA", SIZE)
    art = ImageDraw.Draw(overlay)
    art.ellipse((115, 55, 149, 66), fill=(53, 53, 56, 61))
    for x, y in ((116, 59), (144, 61)):
        art.line((x, y - 7, x, y), fill="#634b51")
        art.point((x, y - 7), fill="#dbab78")
    art.line((117, 55, 143, 57), fill="#d2aa74")
    art.polygon([(131, 61), (134, 58), (145, 58), (147, 61),
                 (146, 64), (132, 64)], fill="#705258")
    art.line((134, 59, 144, 59), fill="#dfab78")
    art.line((133, 61, 145, 61), fill="#a8795d")
    art.line((134, 63, 145, 63), fill="#c5916d")
    yield overlay.copy()

    for x, y in ((117, 58), (144, 59)):
        art.line((x, y - 17, x, y), fill="#6e5055", width=3)
        art.line((x, y - 17, x, y), fill="#be946f")
    art.line((117, 42, 144, 42), fill="#644953", width=3)
    art.line((118, 42, 143, 42), fill="#d6a87e")
    art.line((119, 55, 143, 44), fill="#a5755f")
    art.line((119, 44, 143, 56), fill="#a5755f")
    yield overlay.copy()

    for x in (117, 144):
        art.line((x, 33, x, 42), fill="#6e5055", width=3)
        art.line((x, 33, x, 42), fill="#be946f")
    art.line((117, 34, 144, 34), fill="#644953", width=3)
    art.line((118, 34, 143, 34), fill="#d6a87e")
    art.polygon([(122, 34), (122, 38), (128, 39), (133, 37), (140, 38), (140, 34)],
                fill="#8797a3")
    art.line((122, 35, 140, 35), fill="#b1b4b4")
    art.line((140, 42, 134, 57), fill="#704f53", width=3)
    art.line((140, 42, 134, 57), fill="#c1936c")
    art.line((145, 42, 139, 57), fill="#704f53", width=3)
    art.line((145, 42, 139, 57), fill="#c1936c")
    for x, y in ((139, 45), (138, 48), (137, 51), (136, 54)):
        art.line((x, y, x + 5, y), fill="#dbad7e")
    yield overlay


def cat_frames(cat, sequence):
    for action, number in sequence:
        sprite = Image.new("RGBA", SPRITE_SIZE)
        cat.draw(sprite, action, number, 24, 29)
        yield sprite


def grey_visitor(art, pose):
    edge = "#485363"
    fur = "#91a2b0"
    stripe = "#62778b"
    light = "#c8d6d7"
    tail = 15 + (pose == 1)
    art.line([(20, 27), (16, 27), (13, 24), (13, 18), (tail, 15), (tail + 1, 16)],
             fill=edge, width=5)
    art.line([(20, 27), (16, 26), (14, 23), (14, 18), (tail, 16)],
             fill=fur, width=2)
    art.point((14, 21), fill=stripe)
    art.ellipse((17, 16, 32, 29), fill=fur, outline=edge)
    art.ellipse((22, 19, 31, 28), fill=light)
    art.line((18, 21, 21, 22), fill=stripe)
    art.line((18, 24, 21, 25), fill=stripe)
    art.rounded_rectangle((20, 25, 25, 29), radius=1, fill=light, outline=edge)
    art.rounded_rectangle((26, 25, 31, 29), radius=1, fill=light, outline=edge)
    head_y = 12 + (pose == 2)
    art.polygon([(18, head_y + 3), (18, head_y - 3), (23, head_y - 1),
                 (26, head_y - 1), (31, head_y - 3), (32, head_y + 4),
                 (32, head_y + 8), (28, head_y + 11), (22, head_y + 11),
                 (17, head_y + 8)], fill=fur, outline=edge)
    art.point((20, head_y), fill="#be8e9b")
    art.point((30, head_y), fill="#be8e9b")
    art.line((24, head_y, 26, head_y), fill=stripe)
    art.point((25, head_y + 1), fill=stripe)
    art.ellipse((20, head_y + 5, 29, head_y + 9), fill=light)
    if pose == 1:
        art.line((21, head_y + 4, 23, head_y + 4), fill=edge)
        art.line((27, head_y + 4, 29, head_y + 4), fill=edge)
    else:
        art.point((22, head_y + 4), fill="#45685c")
        art.point((28, head_y + 4), fill="#45685c")
    art.point((25, head_y + 6), fill="#ad7888")
    art.point((25, head_y + 7), fill=edge)


def black_visitor(art, pose):
    edge = "#322d40"
    fur = "#4d465d"
    shine = "#77708b"
    tail_tip = 16 if pose == 2 else 13
    art.line([(19, 25), (13, 26), (10, 24), (8, 21), (8, tail_tip)],
             fill=edge, width=4)
    art.line([(18, 25), (13, 25), (10, 23), (9, 20), (9, tail_tip)],
             fill=shine, width=1)
    head = 29 + (pose == 2)
    art.polygon([(14, 24), (16, 20), (21, 18), (27, 19), (30, 21),
                 (33, 24), (32, 27), (17, 27)], fill=fur, outline=edge)
    art.line([(17, 21), (21, 19), (27, 20)], fill=shine, width=2)
    art.polygon([(17, 25), (21, 25), (20, 28), (24, 28), (24, 29),
                 (17, 29), (16, 28)], fill=fur, outline=edge)
    art.polygon([(29, 25), (32, 25), (34, 28), (38, 28), (38, 29),
                 (30, 29)], fill=fur, outline=edge)
    art.polygon([(head, 20), (head - 1, 15), (head + 3, 17), (head + 5, 17),
                 (head + 6, 14), (head + 8, 18), (head + 10, 21),
                 (head + 12, 22), (head + 11, 24), (head + 6, 25),
                 (head + 2, 24)], fill=fur, outline=edge)
    art.point((head + 6, 17), fill="#ab7c95")
    art.line((head + 2, 19, head + 7, 19), fill=shine)
    art.point((head + 10, 22), fill="#b8929c")
    if pose == 1:
        art.line((head + 6, 21, head + 8, 21), fill="#b08760")
    else:
        art.point((head + 7, 21), fill="#f6d083")


def visitor_cat(coat):
    draw_cat = {"grey": grey_visitor, "black": black_visitor}[coat]
    for pose in range(3):
        frame = Image.new("RGBA", SPRITE_SIZE)
        art = ImageDraw.Draw(frame)
        art.ellipse((12, 27, 36, 30), fill=(58, 47, 56, 55))
        draw_cat(art, pose)
        yield frame


def gardener_frames():
    base_colors = {
        "#": "#43394a", "h": "#854e4d", "H": "#bc795e", "s": "#ffdaaa",
        "S": "#efae85", "p": "#e89287", "e": "#4d3e50", "r": "#df8b79",
    }
    braids = (".###.......###.", "#Hhh#.....#hHh#", "#Hh#.......#hh#",
              "#hr#.......#rh#", ".#h#.......#h#.")
    face = (
        "...#########...", "..#HHHHHhhhh#..", ".#HHHHHhhhhh#..",
        ".#HHHssshhhhh#.", ".#Hssssssshhh#.", ".#Hssesesshhh#.",
        "..#hpssspshh#..", "...#ssssssh#...", "....##SS##.....",
    )

    # Atlas order is part of the renderer contract, three poses per outfit.
    outfits = (
        (  # Straw hat and green overalls for gardening.
            {"C": "#bdd5a2", "c": "#6f9a7c", "d": "#466955", "a": "#eec887",
             "A": "#ffe2a0", "b": "#80584e"},
            (".....######......", "....#AAAAaa#.....", "...#AAaaAaaa#....",
             "..#aaaaaaaaaa#...", ".##AAAAAAAAaa##..", "..############..."),
            ("....#######....", "...#CCaCaCC#...", "..#CScacacCC#..", "..#scccccCs#...",
             "...#cdCcdC#....", "...#dcccdd#....", "....######....."),
        ),
        (  # Cream apron over a coral dress for feeding.
            {"C": "#f4a48b", "c": "#c77679", "d": "#a95e70", "a": "#efd3aa",
             "A": "#fff2ce", "b": "#814f62"},
            (".................", ".........##......", ".......##Cr#.....",
             ".....##AA#C#.....", "....#AaaA##......"),
            ("....#######....", "...#CAcAcCC#...", "..#CCAAACCc#...", "..#sAaaAAsc#...",
             "...#AAaaAAc#...", "..#caaaaaacc#..", "..###########.."),
        ),
        (  # Gold helmet and work jacket for building.
            {"C": "#75a2ad", "c": "#487d8c", "d": "#39566b", "a": "#e6a651",
             "A": "#ffdb78", "b": "#715047"},
            (".....#######.....", "....#AAaAAAa#....", "...#AAAaAAAAa#...",
             "...#AAAaAAAaa#...", "..##AAAAAAAA###..", "...###########..."),
            ("....#######....", "...#CCaCcCC#...", "..#CCAcAcCcC#..", "..#sCAaAaCs#...",
             "...#ccdccC#....", "...#ddcccC#....", "....######....."),
        ),
        (  # Blue rain hood, rain jacket and golden rain boots.
            {"C": "#91cece", "c": "#5699ab", "d": "#476779", "a": "#f0ca75",
             "A": "#ffe7a0", "b": "#d29655"},
            (".....#######.....", "....#CCCCccc#....", "...#CCccCCccc#...",
             "..#CCc####cCc#...", "..#Cc#....#cCc#..", "..#c#......#cc#..",
             "..#c#......#dc#..", "..#cd#....#dc#...", "...#dc#..#dc#...."),
            ("...#######.....", "..#CCCAdcCC#...", "..#CCcAccCcC#..", "..#cCCCacCcc#..",
             "...#CcAdccc#...", "...#Cccdcc##...", "...########...."),
        ),
        (  # Knit sweater and a mint scarf off duty.
            {"C": "#c9aec7", "c": "#ac83a8", "d": "#776481", "a": "#83aa94",
             "A": "#b6cf9e", "b": "#725b77"},
            (),
            ("....#######....", "...#AAAAaAA#...", "..#CCcAacCC#...", "..#sCcAAcCs#...",
             "...#CCCacC#....", "...#ccccCC#....", "....######....."),
        ),
    )

    def stamp(art, rows, origin, colors):
        for y, row in enumerate(rows, origin[1]):
            for x, char in enumerate(row, origin[0]):
                if char != ".":
                    art.point((x, y), fill=colors[char])

    for outfit, (palette, hat, clothes) in enumerate(outfits):
        colors = {**base_colors, **palette}
        for step in range(3):
            frame = Image.new("RGBA", SPRITE_SIZE)
            art = ImageDraw.Draw(frame)
            art.ellipse((16, 27, 32, 30), fill=(63, 48, 52, 61))
            stamp(art, braids, (17, 16), colors)
            stamp(art, face, (17, 12), colors)
            if step == 2:
                art.point((22, 17), fill=colors["S"])
                art.point((24, 17), fill=colors["S"])
            stamp(art, hat, (16, 9), colors)
            stamp(art, clothes, (17, 21), colors)
            left, right = 21 + (step == 1), 26 - (step == 2)
            for x, lift in ((left, step == 1), (right, step == 2)):
                art.line((x, 27, x + 2, 27), fill=colors["d"])
                art.line((x - 1, 29 - lift, x + 2, 29 - lift), fill=colors["b"])
                art.point((x, 28 - lift), fill=colors["a"] if outfit == 3 else colors["b"])
            yield frame


def sparrow_frames():
    colors = {
        "#": "#4e3d4c", "t": "#a56e58", "T": "#d5a485", "c": "#f0d3af",
        "w": "#bb8771", "e": "#fff0c4", "b": "#e6b578",
    }
    bird = (
        "......###...", ".....#Ttt#..", "..#..#T#cb#.", ".##.#Twccc#.",
        "#t##twwccc#.", ".##ttwTcc#..", "...######...",
        ".....b.b....", ".....b.bb...",
    )
    for step in range(3):
        frame = Image.new("RGBA", SPRITE_SIZE)
        art = ImageDraw.Draw(frame)
        art.ellipse((18, 28, 31, 30), fill=(64, 48, 52, 57))
        lift = 2 if step == 1 else 0
        for y, line in enumerate(bird):
            for x, char in enumerate(line):
                if char in colors:
                    art.point((18 + x, 21 + y - lift), fill=colors[char])
        if step == 1:
            art.line((19, 25, 21, 24), fill=colors["T"])
        elif step == 2:
            art.point((24, 23), fill=colors["e"])
        yield frame


# Each animal keeps the same foot anchor as the atlas-derived cats.
WILDLIFE = {
    "squirrel": (
        {"#": "#523c44", "T": "#e2a46e", "t": "#b57557", "E": "#f4ca95",
         "e": "#ffe4b9", "b": "#765249"},
        (
            "...#####...........", "..#TTTTt#..........", ".#TTtttt#....##....",
            ".#TTt###....#tt#...", "#TTtt#.....#ttTt##.", "#Ttt#.....#tEt#et#.",
            "#ttt#....#ttEEeeb#.", ".#tt#...#tttEEE##..", ".#ttt###tttEEt#....",
            "..#tttttttEEt#.....", "...#tttTtEEE##.....", "....#tttttttt#.....",
            ".....########......", "......bb...bb......",
        ),
        (0, 0, 1),
        (((14, 5, "t"), (15, 5, "#")), ((1, 3, "T"), (12, 11, "E"))),
    ),
    "hedgehog": (
        {"#": "#4e4050", "S": "#927063", "s": "#694f52", "h": "#c28e71",
         "E": "#eac3a0", "e": "#f6d8b1", "b": "#92695c"},
        (
            ".....#.#...........", "...##s#s###........", "..#SsssSss##.......",
            ".#SsSssSsSSs#..#...", ".#sSssSsSsss#.#h##.", "#SsSssSssSs#hhEh#..",
            "#sSssSssSS#hE#eE#..", "#ssSsSSss#hEEEEeh#.", ".#ssssss#hEEEEE##..",
            "..#############....", ".....bb.....bb.....",
        ),
        (0, 0, 0),
        (((13, 6, "h"), (14, 6, "#")), ((6, 10, "."), (7, 10, "b"), (11, 10, "b"))),
    ),
    "rabbit": (
        {"#": "#534958", "w": "#e0c9b5", "W": "#f3dfc8", "p": "#d5a1a0",
         "e": "#fff1d9", "b": "#ac8b85"},
        (
            ".........###......", ".....##.#Wp#......", "....#Wp##Wp#......",
            "....#Wp#Wp#.......", "....#Wp#W#........", ".....#wWW##.......",
            "......#WWWW#......", ".....#WWWW#W#.....", ".....#WWWWeWe#....",
            "....#WWWWWeeeb#...", ".###wwWWWWWe##....", "#eW#wwWWWWw#......",
            "#WWwwwwWWww#......", ".##wwwwwwwww##....", "...############...",
            ".....bb...bbb.....",
        ),
        (0, 2, 0),
        (((10, 7, "w"), (11, 7, "#")), ((6, 3, "p"), (12, 15, "w"))),
    ),
    "frog": (
        {"#": "#40544e", "G": "#9cb883", "g": "#64866e", "v": "#7ba388",
         "E": "#c8d595", "e": "#e5dd9b", "b": "#819878"},
        (
            "......###...###....", ".....#GeG#.#GeG#...", "....#Ge#G###G#eG#..",
            "....#GGGGvGGGGG#...", "...#vvvGGGGGGvvv#..", "..#gggvGEEEEEGgg#..",
            ".#ggvg#EEEEEE#gvg#.", ".#gvvg#EEEEEE#gvv#.", "..####bbbbbbb####..",
            "..vvv.........vvv..",
        ),
        (0, 1, 0),
        (((7, 2, "e"), (13, 2, "e")), ((8, 7, "e"), (9, 7, "e"), (10, 7, "e"))),
    ),
}


def wildlife_frames(name):
    colors, figure, lifts, details = WILDLIFE[name]
    width = max(map(len, figure))
    left = 24 - width // 2
    for step, lift in enumerate(lifts):
        frame = Image.new("RGBA", SPRITE_SIZE)
        art = ImageDraw.Draw(frame)
        top = 29 - len(figure) - lift
        art.ellipse((left + 2, 28, left + width - 2, 30), fill=(61, 46, 52, 52))
        for y, line in enumerate(figure):
            for x, char in enumerate(line):
                if char in colors:
                    art.point((left + x, top + y), fill=colors[char])
        if step > 0:
            for x, y, char in details[step - 1]:
                art.point((left + x, top + y), fill=colors.get(char, (0, 0, 0, 0)))
        yield frame


def bgra(frame):
    output = bytearray()
    for red, green, blue, alpha in frame.getdata():
        output.extend((blue * alpha // 255, green * alpha // 255, red * alpha // 255, alpha))
    return output


def preview(day_scenes, build_layers, sprites):
    samples = Image.new("RGB", (4 * 160, 3 * 100), "#272531")
    for column, (name, yard) in enumerate(day_scenes.items()):
        for row, level in enumerate((0, 2, 3)):
            scene = yard.copy()
            scene.alpha_composite(build_layers[level])
            if row > 0:
                scene.alpha_composite(sprites["keeper"][row], (90 - 24, 67 - 29))
                guest = "sparrow" if row == 1 else "grey_cat"
                scene.alpha_composite(sprites[guest][0], (131 - 24, 72 - 29))
            cat = "cat_sleep" if name == "night" else "cat_idle"
            scene.alpha_composite(sprites[cat][0], (110 - 24, 78 - 29))
            samples.paste(scene, (column * 160, row * 100), scene)
    samples.resize((1280, 600), NEAREST).save(OUTPUT / "preview.png")


def main():
    OUTPUT.mkdir(exist_ok=True)
    base = base_yard()
    day_scenes = {name: lighting(base, name) for name in LIGHT}
    build_layers = list(builds())
    cat = CatSprites()
    sprites = {
        "cat_idle": list(cat_frames(cat, (("idle", 0), ("idle", 1), ("idle", 2), ("blink", 0)))),
        "cat_sleep": list(cat_frames(cat, (("sleep", 0), ("sleep", 1)))),
        "cat_walk": list(cat_frames(cat, (("walk", step) for step in range(4)))),
        "cat_eat": list(cat_frames(cat, (("eat", step) for step in range(4)))),
        "cat_treat": list(cat_frames(cat, (("eat", 0), ("eat", 1), ("eat", 2), ("sit", 1)))),
        "keeper": list(gardener_frames()),
        "sparrow": list(sparrow_frames()),
        "grey_cat": list(visitor_cat("grey")),
        "black_cat": list(visitor_cat("black")),
    }
    sprites.update((name, list(wildlife_frames(name))) for name in WILDLIFE)
    atlases = {
        "scenes": day_scenes.values(),
        "terrain": terrain_frames(),
        "builds": build_layers,
        "construction": (
            *flower_bed_snapshots(), *workbench_snapshots(), *pavilion_snapshots()
        ),
        **sprites,
    }
    for name, frames in atlases.items():
        with (OUTPUT / (name + ".bgra")).open("wb") as stream:
            for frame in frames:
                stream.write(bgra(frame))
    preview(day_scenes, build_layers, sprites)


if __name__ == "__main__":
    main()
