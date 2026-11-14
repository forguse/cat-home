"""Rebuild the hand-planned scenery districts of Willow Village revision 3.

By default verify the checked-in layout. Use --write to replace its additive
rows after running the same geometry and older-revision compatibility checks.
"""

from __future__ import annotations

import argparse
from collections import Counter
from fractions import Fraction
import json

from compile_map import BASE_PHASES, DENSITY_PHASE, Layout, PHASES, SOURCE, WALKABLE_SCENIC_KINDS, cells, distance, flood


# The oval is a placement envelope, not a ring: larger trees collect in close,
# irregular groups with small clearings among them. Each row is district,
# cluster, center, tile radii, and ordered target counts by silhouette.
GROVES = (
    ('home', 'farmhouse_northwest', (6, 9), (3, 6), (('tree', 10), ('shrub', 6), ('rock', 4))),
    ('home', 'farmhouse_north', (27, 8), (16, 4), (('tree', 18), ('shrub', 12), ('rock', 4))),
    ('home', 'pond_east_slope', (31, 33), (7, 5), (('tree', 5), ('rock', 8), ('shrub', 7))),
    ('home', 'workshop_back', (47, 17), (4, 9), (('tree', 8), ('shrub', 9), ('wood_pile', 2))),
    ('home', 'farm_lane_side_yard', (48, 32), (3, 5), (('tree', 10), ('shrub', 11), ('rock', 5))),
    ('farm', 'field_west', (6, 53), (2, 12), (('rock', 10), ('shrub', 8), ('tree', 4))),
    ('farm', 'field_east', (49, 53), (2, 12), (('tree', 10), ('rock', 10), ('shrub', 8))),
    ('farm', 'field_south', (29, 61), (14, 2), (('rock', 6), ('shrub', 9))),
    ('farm', 'field_inner_island', (27, 54), (7, 3), (('shrub', 4), ('rock', 4), ('flowers', 12), ('grass', 10))),
    ('orchard', 'orchard_west_bank', (8, 76), (4, 12), (('tree', 34), ('shrub', 17), ('log', 4))),
    ('orchard', 'orchard_northwest', (18, 71), (8, 5), (('tree', 12), ('shrub', 10), ('rock', 6))),
    ('orchard', 'orchard_west_footpath_copse', (22, 77), (3, 3), (('tree', 6), ('shrub', 4), ('rock', 4))),
    ('orchard', 'orchard_low_bend_copse', (27, 83), (4, 2), (('tree', 6), ('shrub', 5), ('log', 2))),
    ('orchard', 'orchard_east_edge', (43, 80), (4, 12), (('tree', 23), ('shrub', 10), ('rock', 8))),
    ('orchard', 'orchard_low_slope', (29, 89), (15, 2), (('tree', 13), ('shrub', 13), ('log', 4))),
    ('forest', 'forest_upper_slope', (20, 94), (16, 2), (('tree', 15), ('shrub', 9), ('log', 5))),
    ('forest', 'forest_west_bank', (7, 104), (3, 10), (('tree', 12), ('shrub', 10), ('log', 4))),
    ('forest', 'forest_lower_slope', (22, 120), (17, 3), (('tree', 30), ('shrub', 17), ('rock', 7))),
    ('forest', 'forest_east_copse', (42, 101), (3, 10), (('tree', 17), ('wood_pile', 4), ('crate', 3))),
    ('creek', 'forest_creek_northwest', (55, 19), (4, 15), (('tree', 35), ('rock', 15), ('shrub', 15))),
    ('creek', 'forest_creek_northeast', (69, 19), (3, 15), (('tree', 26), ('rock', 11), ('shrub', 12))),
    ('creek', 'forest_creek_west', (54, 70), (3, 21), (('tree', 43), ('rock', 15), ('shrub', 14))),
    ('creek', 'forest_creek_east', (69, 66), (3, 18), (('tree', 32), ('rock', 13), ('shrub', 11))),
    ('wetland', 'wetland_west_reeds', (53, 100), (5, 7), (('reeds', 26), ('shrub', 8), ('rock', 6))),
    ('wetland', 'wetland_southwest', (54, 118), (6, 6), (('reeds', 36), ('shrub', 12), ('log', 5))),
    ('wetland', 'wetland_east_shore', (74, 117), (7, 7), (('reeds', 28), ('rock', 14), ('shrub', 14))),
    ('wetland', 'wetland_east_reeds', (84, 103), (4, 9), (('reeds', 27), ('shrub', 10), ('rock', 4))),
    ('north_homes', 'forest_north_homes', (108, 6), (30, 2), (('tree', 34), ('shrub', 28), ('rock', 8))),
    ('north_homes', 'north_back_west', (88, 29), (11, 2), (('tree', 10), ('shrub', 11), ('rock', 5))),
    ('north_homes', 'north_back_east', (118, 30), (16, 1), (('tree', 12), ('shrub', 13))),
    ('north_homes', 'north_middle_courts_west', (100, 22), (3, 3), (('tree', 3), ('shrub', 7), ('flowers', 4))),
    ('north_homes', 'north_middle_courts_east', (112, 22), (3, 3), (('tree', 4), ('shrub', 9), ('bench', 1))),
    ('market', 'market_back_west', (84, 38), (7, 2), (('shrub', 9), ('tree', 4), ('crate', 3))),
    ('market', 'market_back_east', (108, 38), (10, 2), (('tree', 6), ('shrub', 12), ('crate', 3))),
    ('market', 'market_south_edge', (97, 62), (13, 1), (('shrub', 7), ('crate', 4), ('bench', 2))),
    ('market', 'market_corner_seat', (95, 54), (4, 3), (('bench', 2), ('shrub', 7), ('crate', 3))),
    ('craft', 'craft_back_yards', (146, 37), (20, 1), (('tree', 12), ('shrub', 13), ('wood_pile', 5))),
    ('craft', 'craft_maker_yards', (162, 44), (4, 6), (('tree', 8), ('rock', 8), ('wood_pile', 4), ('crate', 3))),
    ('craft', 'craft_south_yards', (146, 62), (20, 1), (('shrub', 15), ('rock', 13), ('wood_pile', 4))),
    ('craft', 'craft_workyard_midblock', (145, 52), (7, 4), (('tree', 5), ('wood_pile', 6), ('crate', 6), ('shrub', 5))),
    ('east_homes', 'east_home_backyards', (146, 69), (19, 1), (('tree', 14), ('shrub', 15), ('rock', 6))),
    ('east_homes', 'east_home_orchard', (141, 87), (6, 7), (('tree', 11), ('shrub', 13), ('rock', 6))),
    ('east_homes', 'east_home_east_park', (165, 88), (2, 8), (('tree', 9), ('shrub', 9), ('rock', 5))),
    ('commons', 'commons_hall_west', (100, 74), (2, 5), (('tree', 6), ('shrub', 8), ('bench', 2))),
    ('commons', 'commons_hall_east', (114, 75), (3, 6), (('tree', 8), ('shrub', 9), ('rock', 4))),
    ('commons', 'commons_hall_north', (103, 69), (11, 1), (('tree', 7), ('shrub', 10))),
    ('commons', 'commons_park_edge', (81, 88), (5, 3), (('tree', 8), ('shrub', 12), ('rock', 5))),
    ('commons', 'commons_south_garden', (102, 90), (8, 1), (('shrub', 7), ('bench', 2), ('tree', 5))),
    ('south_farm', 'south_farm_west_break', (95, 111), (3, 12), (('tree', 19), ('shrub', 14), ('rock', 12))),
    ('south_farm', 'south_farm_barnyard', (129, 105), (2, 7), (('tree', 7), ('wood_pile', 5), ('shrub', 9))),
    ('south_farm', 'south_farm_spinney', (107, 105), (4, 4), (('tree', 6), ('shrub', 8), ('rock', 4))),
    ('south_farm', 'south_farm_windbreak', (117, 111), (2, 5), (('tree', 4), ('fence', 6), ('shrub', 6))),
    ('south_farm', 'south_farm_belt', (120, 121), (10, 2), (('tree', 12), ('rock', 9), ('shrub', 9))),
    ('quarry', 'quarry_high_ridge', (162, 8), (17, 4), (('rock', 67), ('tree', 12), ('shrub', 12))),
    ('quarry', 'quarry_north_face', (181, 14), (6, 7), (('rock', 39), ('shrub', 14), ('tree', 6))),
    ('quarry', 'quarry_scree', (165, 28), (17, 3), (('rock', 33), ('tree', 8), ('shrub', 12))),
    ('quarry', 'quarry_workyard_scree', (160, 21), (5, 4), (('rock', 20), ('shrub', 5), ('tree', 3))),
    ('quarry', 'quarry_workyard_foot', (170, 24), (5, 3), (('rock', 15), ('wood_pile', 3), ('shrub', 4))),
    ('east_woods', 'forest_east_upper', (180, 41), (7, 5), (('tree', 37), ('shrub', 14), ('rock', 10))),
    ('east_woods', 'forest_entrance_side', (177, 53), (4, 4), (('tree', 8), ('shrub', 8), ('rock', 6))),
    ('east_woods', 'forest_entrance_south', (179, 64), (7, 6), (('tree', 23), ('rock', 12), ('shrub', 13))),
    ('east_woods', 'forest_east_lower', (179, 84), (7, 10), (('tree', 58), ('shrub', 27), ('log', 8))),
    ('future', 'forest_works_margin', (186, 112), (2, 11), (('tree', 13), ('shrub', 9), ('rock', 7))),
    ('future', 'future_reserve_divider', (154, 111), (2, 11), (('tree', 5), ('shrub', 13), ('rock', 9))),
    ('future', 'future_east_divider', (170, 111), (2, 11), (('tree', 4), ('shrub', 10), ('rock', 9))),
    (None, 'forest_north_connector', (105, 34), (25, 2), (('tree', 24), ('shrub', 28), ('rock', 8))),
    (None, 'forest_west_connector', (47, 102), (4, 11), (('tree', 16), ('shrub', 12), ('rock', 7))),
    (None, 'forest_craft_connector', (144, 66), (19, 2), (('tree', 19), ('shrub', 18), ('rock', 6))),
    (None, 'forest_south_connector', (114, 94), (28, 2), (('tree', 22), ('shrub', 18), ('rock', 8))),
    (None, 'forest_orchard_connector', (49, 86), (3, 7), (('tree', 10), ('shrub', 12), ('log', 4))),
    (None, 'forest_quarry_connector', (168, 34), (17, 2), (('tree', 16), ('shrub', 14), ('rock', 11))),
)

# Short runs mark individual property lines; incomplete runs leave yard gates.
FENCES = (
    ('home', 'home_north_fence', 5, 12, 31, 12),
    ('home', 'workshop_fence', 39, 7, 49, 7),
    ('farm', 'field_west_fence', 6, 44, 6, 60),
    ('farm', 'field_east_fence', 47, 45, 47, 61),
    ('orchard', 'orchard_crate_yard', 39, 79, 39, 85),
    ('forest', 'forest_logging_front', 39, 97, 39, 102),
    ('north_homes', 'north_house_row_split', 79, 15, 134, 15),
    ('north_homes', 'south_house_row_split', 79, 28, 134, 28),
    ('market', 'market_store_fence', 114, 53, 118, 53),
    ('craft', 'craft_workshop_yard', 155, 61, 165, 61),
    ('east_homes', 'east_home_garden_front', 141, 88, 156, 88),
    ('commons', 'commons_court_west', 78, 71, 78, 79),
    ('commons', 'commons_court_south', 81, 81, 92, 81),
    ('south_farm', 'south_farm_dooryard', 119, 111, 119, 117),
    ('south_farm', 'south_farm_north', 96, 99, 110, 99),
    ('future', 'future_west_yard', 138, 101, 138, 108),
    ('future', 'future_east_yard', 186, 102, 186, 109),
)

# A horizontal fence asset cannot mark a north/south garden edge. Low stem
# runs keep the vertical lines readable and leave breaks at actual approaches.
HEDGES = (
    ('east_home_side_west', 125, 71, 79),
    ('east_home_side_central', 140, 73, 80),
    ('east_home_side_east', 158, 74, 80),
)

# Purposeful bare earth, rock shading and seasonal pasture. These are terrain
# tints only. Completed construction and an active court paint over the grass.
COVER = (
    ('quarry', 'ridge_stone', (159, 8), (15, 5), (135, 127, 105)),
    ('quarry', 'ridge_east_stone', (180, 13), (7, 7), (142, 133, 109)),
    ('quarry', 'scree_apron', (166, 26), (16, 5), (129, 132, 94)),
    ('north_homes', 'narrow_cottage_gardens', (105, 14), (16, 4), (115, 139, 77)),
    ('north_homes', 'eastern_cottage_gardens', (127, 24), (11, 5), (110, 137, 78)),
    ('commons', 'hall_yard', (107, 75), (11, 6), (126, 136, 91)),
    ('commons', 'court_grassy_frame', (86, 76), (11, 8), (120, 143, 78)),
    ('east_homes', 'east_upper_yards', (145, 75), (16, 5), (118, 139, 80)),
    ('east_homes', 'east_lawn', (151, 87), (12, 6), (124, 145, 86)),
    ('east_woods', 'deep_woods', (181, 85), (7, 14), (91, 119, 67)),
    ('creek', 'reedy_east_bank', (68, 64), (3, 14), (102, 125, 79)),
    ('creek', 'reedy_west_bank', (55, 64), (3, 14), (102, 125, 79)),
    ('south_farm', 'south_farm_meadow', (104, 111), (9, 12), (126, 147, 84)),
    ('south_farm', 'barn_edge', (125, 114), (7, 9), (121, 135, 85)),
    ('craft', 'artisan_yards', (153, 51), (15, 10), (131, 137, 97)),
    ('market', 'market_verges', (102, 49), (15, 10), (128, 141, 92)),
    ('farm', 'fallow_pasture', (26, 50), (14, 8), (119, 144, 79)),
    ('farm', 'field_turning_strip', (22, 56), (10, 4), (133, 146, 92)),
    ('home', 'pond_gravel_bank', (27, 35), (8, 4), (116, 132, 82)),
    ('orchard', 'orchard_hollow', (25, 76), (12, 5), (104, 131, 73)),
    ('wetland', 'visible_east_silt', (69, 110), (8, 10), (117, 129, 86)),
    ('future', 'pasture_first_row_west', (145, 103), (6, 4), (122, 144, 87)),
    ('future', 'pasture_first_row_center', (162, 104), (6, 4), (107, 133, 73)),
    ('future', 'pasture_first_row_east', (177, 104), (6, 4), (130, 147, 88)),
    ('future', 'pasture_last_row_west', (146, 118), (6, 4), (112, 134, 74)),
    ('future', 'pasture_last_row_center', (162, 117), (6, 4), (127, 147, 83)),
    ('future', 'pasture_last_row_east', (178, 118), (5, 4), (107, 132, 79)),
)


def rank(tile):
    x, y = tile
    key = (x * 73856093) ^ (y * 19349663) ^ 20261115
    return ((key ^ (key >> 16)) * 1103515245) & 0xffffffff


def rebuild(data):
    data['map_revision'] = 2
    data['phase_ids'] = list(PHASES[:-1])
    data['scenic_objects'] = [row for row in data['scenic_objects'] if not row['id'].startswith('density.')]
    data['ground_cover'] = [row for row in data['ground_cover'] if not row['id'].startswith('cover.density.')]
    baseline = Layout(json.loads(json.dumps(data), parse_float=Fraction))
    reserved = baseline.scenery_exclusions() | baseline.density_exclusions() | baseline.permanent_water
    assigned = {tuple(row['tile']) for row in data['scenic_objects']}
    added = []

    def place(region, name, kind, tile):
        if tile in reserved or tile in assigned or tile not in baseline.interior:
            return False
        if baseline.density_foreground_overlaps(tile, kind):
            return False
        if region is None:
            if any(x <= tile[0] < x + w and y <= tile[1] < y + h
                   for x, y, w, h in baseline.regions.values()):
                return False
        else:
            x, y, w, h = baseline.regions[region]
            if not (x <= tile[0] < x + w and y <= tile[1] < y + h):
                return False
        hard = kind not in WALKABLE_SCENIC_KINDS
        added.append(dict(id=f'density.{name}.{kind}.{tile[0]}_{tile[1]}', cluster=name,
                          kind=kind, tile=list(tile), phase=DENSITY_PHASE, solid=hard))
        assigned.add(tile)
        return True

    for region, name, left, top, right, bottom in FENCES:
        for tile in sorted(cells((left, top, right - left + 1, bottom - top + 1))):
            # Leave assigned breaks at recurring entrances, plus actual map reservations.
            if (tile[0] + tile[1]) % 6 != 1:
                place(region, name, 'fence', tile)

    for name, x, top, bottom in HEDGES:
        for y in range(top, bottom + 1):
            if (y - top) % 4 != 3:
                place('east_homes', name, 'shrub', (x, y))

    for region, name, (cx, cy), (rx, ry), bands in GROVES:
        candidates = []
        for x, y in cells((cx - rx, cy - ry, 2 * rx + 1, 2 * ry + 1)):
            radial = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
            if radial < 1.17:
                candidates.append((radial * 0.55 + rank((x, y)) / 2**32, x, y))
        ordered = [(x, y) for _, x, y in sorted(candidates)]
        stems = set()
        for kind, quantity in bands:
            for tile in ordered:
                if quantity == 0:
                    break
                # Avoid long adjacent trunk rows. Rocks and hedge stems can
                # adjoin, preserving a readable continuous built edge.
                if kind == 'tree' and any(distance(tile, trunk) < 2 for trunk in stems):
                    continue
                if place(region, name, kind, tile):
                    quantity -= 1
                    if kind == 'tree':
                        stems.add(tile)

    # Rough pasture occupies actual unbuilt footprints. Ground and construction
    # state hide it as the same parcel starts its designed use.
    temporary = baseline.temporary_ground_footprints()
    unavailable = baseline.scenery_exclusions(False)
    future_lots = {row['id'] for row in data['lots'] if row['capability'] == 'bounded_building'}
    for name, tiles in temporary.items():
        footprint = tiles - unavailable
        if not footprint:
            continue
        future = name in future_lots
        x0 = min(x for x, _ in tiles)
        y0 = min(y for _, y in tiles)
        for x, y in sorted(footprint):
            # In the larger lots, bands of light stems pass into denser clumps
            # and small bare gaps. Leave irregular gaps along the new lane.
            key = rank((x, y))
            band = ((x - x0) + (y - y0) * 2) % 11
            if future or name == 'future_lane':
                coverage = 69 if 2 <= band <= 7 else 25
            else:
                coverage = 68
            if key % 100 >= coverage:
                continue
            kind = 'reeds' if (future or name == 'commons_hall') and 2 <= band <= 4 and key % 4 != 0 else 'grass'
            tile = x, y
            if tile not in assigned:
                assigned.add(tile)
                added.append(dict(id=f'density.meadow.{name}.{x}_{y}', cluster='temporary_meadow',
                                  kind=kind, tile=[x, y], phase=DENSITY_PHASE, solid=False,
                                  temporary_ground=True))

    # Solid terrain must remain attached to the main pedestrian system. A tiny
    # accidental grass pocket gets its nearest new obstacle removed, then the
    # full compiler rechecks every phase, road, standing path and filled lot.
    _, _, previous = baseline.geometry(set(BASE_PHASES))
    home = baseline.pois['home_xiaohe.door']
    added_by_tile = {tuple(row['tile']): row for row in added if row['solid']}
    baseline_reached = flood(home, previous)
    while True:
        walk = previous - added_by_tile.keys()
        reached = flood(home, walk)
        stranded = (baseline_reached & walk) - reached
        if not stranded:
            break
        boundary = {(x + dx, y + dy) for x, y in stranded
                    for dx, dy in ((-1, 0), (1, 0), (0, -1), (0, 1))}
        options = boundary & added_by_tile.keys()
        if not options:
            raise ValueError('canopy group separates an old walkable district from its approaches')
        tile = min(options, key=lambda p: (rank(p), p))
        added.remove(added_by_tile.pop(tile))

    data['scenic_objects'].extend(sorted(added, key=lambda row: row['id']))
    data['ground_cover'].extend(dict(id=f'cover.density.{name}', region=region,
                                     center=list(center), radius=list(radius), color=list(color))
                                for region, name, center, radius, color in COVER)
    data['map_revision'] = 3
    data['phase_ids'] = list(PHASES)
    return added


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='write the validated revision 3 blueprint')
    args = parser.parse_args()
    previous = SOURCE.read_bytes()
    data = json.loads(previous)
    added = rebuild(data)
    generated = (json.dumps(data, ensure_ascii=False, indent=2) + '\n').encode('utf-8')
    layout = Layout(json.loads(generated, parse_float=Fraction))
    if args.write:
        if previous != generated:
            SOURCE.write_bytes(generated)
    elif previous != generated:
        parser.error('the blueprint does not match this design; run with --write')
    print(f'{len(added)} new scenic anchors: {dict(sorted(Counter(row["kind"] for row in added).items()))}')
    print(f'{len(layout.scenic)} total anchors; {len(data["ground_cover"])} ground patches; {layout.phase_count} geometry profiles validated')


if __name__ == '__main__':
    main()
