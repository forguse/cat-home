"""Draw the original cottage, meadow and repeatable expansion tiles at native size."""

from random import Random

from PIL import Image, ImageDraw


GRASS = "#8ba664"
TERRACE = [
    (7, 66), (11, 66), (11, 60), (19, 60), (19, 55), (26, 55), (26, 51),
    (133, 51), (133, 54), (142, 54), (142, 59), (149, 59), (149, 63),
    (153, 63), (153, 82), (149, 82), (149, 87), (145, 87), (145, 91),
    (140, 91), (140, 94), (20, 94), (20, 92), (15, 92), (15, 88),
    (10, 88), (10, 81), (7, 81),
]


def turf(art, x, y, shade="#749454", light="#a9b871"):
    art.line((x - 1, y - 1, x, y), fill=shade)
    art.line((x, y, x + 1, y - 2), fill=shade)
    art.point((x - 1, y - 1), fill=light)


def bloom(art, x, y, petals, center):
    art.line((x, y + 1, x, y + 3), fill="#4d754c")
    art.point((x + 1, y + 2), fill="#61864c")
    art.line((x - 1, y, x + 1, y), fill=petals)
    art.point((x, y - 1), fill=petals)
    art.point((x, y), fill=center)


def pebble(art, x, y):
    art.polygon([(x - 2, y - 1), (x + 1, y - 2), (x + 3, y),
                 (x + 2, y + 2), (x - 2, y + 2), (x - 3, y)], fill="#597755")
    art.polygon([(x - 2, y - 1), (x + 1, y - 2), (x + 2, y),
                 (x + 1, y + 1), (x - 2, y + 1)], fill="#8f9580")
    art.line((x - 1, y - 1, x + 1, y - 1), fill="#b5b694")


def meadow(scene):
    mask = Image.new("L", scene.size)
    ImageDraw.Draw(mask).polygon(TERRACE, fill=255)
    art = ImageDraw.Draw(scene)
    art.polygon([(x, y + 4) for x, y in TERRACE], fill="#473f3c")
    art.polygon([(x, y + 3) for x, y in TERRACE], fill="#6c5846")
    art.polygon([(x, y + 1) for x, y in TERRACE], fill="#937953")

    lawn = Image.new("RGBA", scene.size, GRASS)
    green = ImageDraw.Draw(lawn)
    green.ellipse((8, 46, 77, 79), fill="#7d9d5c")
    green.ellipse((9, 50, 70, 72), fill="#749459")
    green.ellipse((108, 49, 171, 81), fill="#82a05f")
    green.ellipse((120, 46, 165, 71), fill="#759658")
    green.ellipse((37, 68, 74, 89), fill="#9aae68")
    green.ellipse((43, 72, 70, 83), fill="#a2b471")
    green.ellipse((99, 72, 145, 92), fill="#91a967")
    green.line(TERRACE + [TERRACE[0]], fill="#a9b778")
    green.line([(20, 93), (140, 93)], fill="#7c9857")
    green.line([(21, 94), (138, 94)], fill="#a3ad6f")
    rng = Random(4685)
    for x, y in [(13, 69), (21, 76), (31, 84), (44, 58), (68, 78), (77, 66),
                 (82, 86), (96, 63), (103, 82), (118, 57), (120, 80), (137, 85),
                 (145, 71), (141, 62), (16, 82), (25, 91), (60, 88), (132, 90)]:
        for _ in range(rng.randrange(2, 5)):
            sx, sy = x + rng.randrange(-3, 4), y + rng.randrange(-2, 3)
            green.line((sx, sy, sx + rng.randrange(0, 3), sy),
                       fill=rng.choice(("#789657", "#95ab68", "#a1af70")))
        if x not in (44, 77, 96, 118):
            turf(green, x, y)
    scene.paste(lawn, (0, 0), mask)
    for x, y in [(17, 92), (27, 96), (38, 97), (49, 96), (66, 97), (92, 95),
                 (103, 97), (120, 96), (133, 96), (144, 91), (151, 84), (9, 83)]:
        art.line((x, y, x + 2, y), fill="#ab8b60")


def grove(art):
    art.ellipse((12, 52, 48, 66), fill="#637e4f")
    art.ellipse((14, 53, 40, 62), fill="#506c46")
    art.polygon([(24, 45), (28, 45), (28, 56), (32, 59), (27, 59), (24, 58),
                 (21, 60), (19, 59), (23, 55)], fill="#594737")
    art.line((25, 50, 25, 56), fill="#946e4a")
    art.line((24, 51, 22, 48), fill="#ab7e53")
    art.polygon([(9, 48), (8, 44), (10, 40), (9, 36), (12, 33), (12, 29),
                 (16, 27), (19, 22), (25, 20), (32, 22), (36, 27), (38, 29),
                 (39, 34), (42, 37), (41, 44), (38, 50), (34, 50), (31, 53),
                 (26, 51), (22, 54), (17, 52), (14, 52), (13, 49)], fill="#344f3f")
    art.polygon([(10, 44), (13, 36), (16, 34), (15, 29), (19, 26), (23, 22),
                 (30, 22), (35, 26), (35, 31), (39, 36), (39, 42), (36, 46),
                 (33, 45), (29, 50), (23, 49), (20, 51), (16, 48), (12, 48)], fill="#537a4c")
    for shape, color in [
        ([(11, 39), (14, 35), (20, 34), (23, 36), (23, 41), (20, 45), (16, 45), (12, 43)], "#719656"),
        ([(16, 32), (18, 27), (22, 26), (24, 23), (28, 23), (31, 26), (31, 30),
          (28, 33), (24, 33), (21, 36)], "#87a45b"),
        ([(24, 36), (28, 33), (32, 32), (35, 33), (36, 37), (32, 41), (28, 42), (24, 41)], "#6e9353"),
        ([(17, 46), (21, 44), (25, 45), (28, 43), (31, 44), (29, 48), (25, 49), (21, 48)], "#618450"),
    ]:
        art.polygon(shape, fill=color)
    art.line([(18, 29), (21, 29), (21, 27), (25, 27), (26, 25), (28, 25)], fill="#a5b76a")
    art.line([(13, 39), (15, 37), (18, 37)], fill="#a0b56a")
    art.line([(25, 37), (28, 35), (31, 35)], fill="#90ab64")
    for x, y in [(17, 43), (24, 39), (31, 28), (29, 45), (20, 31)]:
        art.point((x, y), fill="#b0bb6c")


def hedge(art, x, y):
    art.ellipse((x - 12, y - 5, x + 12, y + 3), fill="#638452")
    art.polygon([(x - 12, y - 2), (x - 12, y - 6), (x - 9, y - 9), (x - 6, y - 9),
                 (x - 3, y - 13), (x + 3, y - 13), (x + 6, y - 10),
                 (x + 10, y - 9), (x + 12, y - 6), (x + 12, y - 2), (x + 9, y),
                 (x + 3, y - 1), (x, y + 1), (x - 5, y), (x - 9, y + 1)], fill="#42634b")
    art.ellipse((x - 10, y - 8, x - 1, y - 2), fill="#688b50")
    art.ellipse((x - 6, y - 12, x + 5, y - 3), fill="#789b55")
    art.ellipse((x + 2, y - 9, x + 10, y - 3), fill="#648952")
    art.line((x - 4, y - 10, x + 1, y - 10), fill="#9ab163")
    art.line((x - 8, y - 6, x - 6, y - 6), fill="#8ea95d")
    art.line((x + 4, y - 7, x + 7, y - 7), fill="#8aa65d")
    art.point((x - 1, y - 8), fill="#cbb285")
    art.point((x + 6, y - 5), fill="#c29f9b")


def roof(art):
    art.polygon([(22, 49), (45, 24), (51, 23), (69, 25), (82, 46), (77, 52),
                 (28, 54), (23, 52)], fill="#344641")
    art.polygon([(45, 26), (53, 26), (74, 49), (27, 49)], fill="#4e7669")
    art.polygon([(53, 26), (68, 27), (79, 46), (74, 49)], fill="#3c605b")
    art.line([(25, 48), (45, 25), (51, 24), (69, 26), (81, 46)], fill="#6e9180")
    for y, left, right in [(30, 44, 56), (34, 41, 60), (38, 37, 64),
                           (42, 33, 67), (46, 29, 71)]:
        art.line((left, y, right, y), fill="#3b6058")
        for x in range(left + 2 + (y % 8), right, 7):
            art.line((x, y - 2, x, y), fill="#3b6058")
            art.line((x + 1, y - 2, min(x + 4, right), y - 2), fill="#628a77")
    for y, left, right in [(32, 61, 71), (37, 65, 74), (42, 69, 77)]:
        art.line((left, y, right, y), fill="#304f4d")
        art.point((left + 2, y - 2), fill="#608174")
        art.point((right - 2, y + 1), fill="#52776a")
    art.line([(24, 50), (29, 52), (74, 51), (81, 47)], fill="#2d4945", width=2)
    art.line((28, 50, 73, 50), fill="#7c9a80")
    art.polygon([(45, 44), (45, 38), (50, 34), (55, 38), (55, 44)], fill="#344d4a")
    art.line([(44, 38), (50, 33), (56, 38)], fill="#9ca58b")
    art.ellipse((47, 37, 53, 43), fill="#af8c63")
    art.ellipse((48, 38, 52, 42), fill="#3d5b59")
    art.line((48, 40, 51, 40), fill="#749186")
    art.polygon([(64, 22), (69, 22), (69, 33), (65, 35), (63, 32), (63, 24)], fill="#845d54")
    art.rectangle((63, 22, 66, 31), fill="#b88068")
    art.line((63, 27, 66, 27), fill="#875d52")
    art.line((65, 30, 68, 30), fill="#6c4b46")
    art.polygon([(62, 20), (69, 20), (70, 22), (69, 24), (62, 24)], fill="#684e48")
    art.line((63, 20, 69, 20), fill="#c59373")


def cottage(art):
    art.ellipse((22, 62, 82, 73), fill="#789352")
    art.ellipse((25, 63, 77, 70), fill="#5f7d49")
    art.polygon([(29, 47), (69, 47), (77, 44), (77, 63), (71, 67), (67, 69), (29, 69)], fill="#5c483e")
    art.rectangle((30, 49, 68, 66), fill="#ddb788")
    art.rectangle((31, 53, 66, 65), fill="#edcfa0")
    art.polygon([(69, 49), (76, 46), (76, 62), (69, 66)], fill="#b38c6b")
    art.line([(70, 60), (73, 59), (76, 57)], fill="#997358")
    art.line((31, 54, 66, 54), fill="#815f46", width=2)
    art.line((32, 55, 32, 66), fill="#8b6248", width=2)
    art.line((66, 54, 66, 66), fill="#8b6248", width=2)
    art.line([(59, 65), (64, 60), (64, 56)], fill="#ad8057")
    art.line((40, 62, 43, 62), fill="#cfaa7c")
    art.line((41, 65, 42, 65), fill="#cfaa7c")
    art.line((58, 58, 61, 58), fill="#d2ab79")
    art.line((55, 63, 58, 63), fill="#d2ab79")
    art.line((30, 67, 68, 67), fill="#a68360")
    art.line((33, 68, 66, 68), fill="#78624a")
    art.rectangle((32, 57, 38, 64), fill="#7b5847")
    art.rectangle((33, 58, 37, 63), fill="#afc0aa")
    art.line((33, 60, 37, 60), fill="#6d785e")
    art.line((35, 58, 35, 63), fill="#6d785e")
    art.point((34, 59), fill="#d9d8b1")
    art.line((32, 64, 38, 64), fill="#c49e6e")
    art.polygon([(45, 67), (45, 58), (47, 55), (52, 55), (55, 58), (55, 67)], fill="#695042")
    art.polygon([(46, 67), (46, 58), (48, 56), (52, 56), (54, 58), (54, 67)], fill="#a76649")
    art.line((48, 57, 48, 66), fill="#c68d61")
    art.line((51, 57, 51, 66), fill="#8d573f")
    art.line((47, 60, 53, 60), fill="#89563e")
    art.point((52, 62), fill="#f0c480")
    art.rectangle((45, 68, 55, 68), fill="#bf9a71")
    art.line((44, 69, 56, 69), fill="#e2bd8b")
    art.line((43, 70, 57, 70), fill="#a88361")
    art.line((45, 71, 55, 71), fill="#70684b")
    roof(art)
    art.ellipse((59, 67, 66, 69), fill="#71864c")
    art.polygon([(60, 64), (65, 64), (64, 68), (61, 68)], fill="#a8674b")
    art.line((60, 64, 65, 64), fill="#e3a471")
    art.line([(62, 64), (62, 61), (64, 59)], fill="#54734c")
    art.line([(62, 63), (60, 62), (59, 60)], fill="#779b55")
    art.point((64, 59), fill="#f1bf75")


def base_yard():
    scene = Image.new("RGBA", (160, 100))
    meadow(scene)
    art = ImageDraw.Draw(scene)
    grove(art)
    hedge(art, 143, 60)
    cottage(art)
    for x, y in [(14, 74), (22, 86), (137, 84), (146, 77), (76, 59)]:
        pebble(art, x, y)
    for x, y, petals, center in [
        (14, 68, "#dfb88a", "#f4de9e"), (17, 66, "#e9c597", "#f4e1a6"),
        (20, 69, "#d19d8e", "#f2c584"), (145, 80, "#d6a49b", "#f6cf8a"),
        (142, 81, "#e8c494", "#f6e5ac"), (145, 86, "#deb88f", "#f3d99c"),
        (31, 68, "#dbc28d", "#f4e1a4"),
    ]:
        bloom(art, x, y, petals, center)
    return scene


def terrain_sources():
    lawn = Image.new("RGBA", (16, 16), GRASS)
    art = ImageDraw.Draw(lawn)
    art.line((3, 4, 5, 4), fill="#94aa68")
    art.line((8, 13, 10, 13), fill="#819f5f")
    art.point((13, 8), fill="#97ac6c")
    varied = lawn.copy()
    art = ImageDraw.Draw(varied)
    art.line((10, 4, 13, 4), fill="#95aa69")
    turf(art, 6, 10, "#789758", "#a1b16e")
    stones = lawn.copy()
    art = ImageDraw.Draw(stones)
    pebble(art, 5, 7)
    art.line((9, 9, 11, 9), fill="#8e9980")
    art.point((10, 8), fill="#afb590")
    flowers = varied.copy()
    art = ImageDraw.Draw(flowers)
    bloom(art, 6, 9, "#e0c890", "#f4dfa1")
    bloom(art, 11, 6, "#d9b19d", "#efce91")
    return lawn, varied, stones, flowers
