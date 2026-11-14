"""Compile the single Willow Village layout source into readonly Rust data.

Run from any directory: python tools/compile_map.py [--check]
The map is validated completely before generated files are written.
"""

from __future__ import annotations

import argparse
from collections import deque
from dataclasses import dataclass
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / 'assets/maps/willow-village-192x128-r1.json'
PACKED = ROOT / 'assets/maps/willow-village-192x128-r1.tiles'
RUST = ROOT / 'src/map_data_generated.rs'
WIDTH, HEIGHT, CHUNK = 192, 128, 16
TILE_ART = 16
NO_REGION = 255
PHASES = (
    'day_1', 'farm_expansion', 'weekly_market', 'permanent_market',
    'resident_arrival', 'village_development', 'community', 'cooperative_farm',
    'late_shared_home', 'later_guest_home', 'late', 'main_bridge_freight',
)
LEGACY_BRIDGE_PHASE = 'day_1_pedestrian_then_upgrade'
SCENIC_KINDS = {
    'tree': 'Tree', 'shrub': 'Shrub', 'rock': 'Rock', 'fence': 'Fence', 'bench': 'Bench',
    'crate': 'Crate', 'flowers': 'Flowers', 'grass': 'Grass', 'sign': 'Sign', 'lamp': 'Lamp',
    'reeds': 'Reeds', 'log': 'Log', 'wood_pile': 'WoodPile',
}
WALKABLE_SCENIC_KINDS = {'flowers', 'grass', 'reeds'}
ID = re.compile(r'[a-z][a-z0-9_.-]*\Z')
Point = tuple[int, int]
Rect = tuple[int, int, int, int]


class MapError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise MapError(message)


def unique_object(pairs):
    obj = {}
    for key, value in pairs:
        require(key not in obj, f'duplicate JSON key: {key}')
        obj[key] = value
    return obj


def valid_id(value, label='id'):
    require(isinstance(value, str) and ID.fullmatch(value), f'{label}: invalid identifier {value!r}')
    return value


def valid_int(value, label, low, high):
    require(type(value) is int and low <= value <= high, f'{label}: expected integer {low}..{high}, got {value!r}')
    return value


def valid_tile(value, label, bounds=(0, 0, WIDTH, HEIGHT)) -> Point:
    require(isinstance(value, list) and len(value) == 2, f'{label}: expected tile [x,y]')
    x, y, w, h = bounds
    return valid_int(value[0], label + '.x', x, x + w - 1), valid_int(value[1], label + '.y', y, y + h - 1)


def valid_rect(value, label) -> Rect:
    require(isinstance(value, list) and len(value) == 4, f'{label}: expected rect [x,y,w,h]')
    x, y = valid_tile(value[:2], label)
    w = valid_int(value[2], label + '.w', 1, WIDTH - x)
    h = valid_int(value[3], label + '.h', 1, HEIGHT - y)
    return x, y, w, h


def valid_art_rect(value, label) -> Rect:
    require(isinstance(value, list) and len(value) == 4, f'{label}: expected art envelope')
    require(all(type(n) in (int, Fraction) for n in value), f'{label}: expected exact numeric coordinates')
    texels = [Fraction(n) * TILE_ART for n in value]
    require(all(n.denominator == 1 for n in texels), f'{label}: envelope must align to art texels')
    x, y, w, h = map(int, texels)
    require(w > 0 and h > 0 and x >= 0 and y >= 0 and x + w <= WIDTH * TILE_ART and y + h <= HEIGHT * TILE_ART,
            f'{label}: envelope outside drawable world')
    return x, y, w, h


def contains(rect: Rect, point: Point):
    x, y, w, h = rect
    return x <= point[0] < x + w and y <= point[1] < y + h


def cells(rect: Rect) -> set[Point]:
    x, y, w, h = rect
    return {(xx, yy) for yy in range(y, y + h) for xx in range(x, x + w)}


def distance(a: Point, b: Point):
    return abs(a[0] - b[0]) + abs(a[1] - b[1])


def strip(points: list[Point], width: int, label: str) -> set[Point]:
    """Blueprint strokes widen towards positive x/y across each segment."""
    require(len(points) >= 2, f'{label}: need at least two road points')
    result = set()
    for (x, y), (xx, yy) in zip(points, points[1:]):
        require((x == xx) != (y == yy), f'{label}: diagonal or repeated road point {(x, y)} → {(xx, yy)}')
        if x == xx:
            result.update((x + off, cy) for cy in range(min(y, yy), max(y, yy) + 1) for off in range(width))
        else:
            result.update((cx, y + off) for cx in range(min(x, xx), max(x, xx) + 1) for off in range(width))
    require(all(0 <= x < WIDTH and 0 <= y < HEIGHT for x, y in result), f'{label}: road width crosses boundary')
    return result


def flood(start: Point, allowed: set[Point]) -> set[Point]:
    if start not in allowed:
        return set()
    reached = {start}
    queue = deque([start])
    while queue:
        x, y = queue.popleft()
        for tile in ((x + 1, y), (x, y + 1), (x - 1, y), (x, y - 1)):
            if tile in allowed and tile not in reached:
                reached.add(tile)
                queue.append(tile)
    return reached


def inside_polygon(tile: Point, polygon: list[Point]):
    x, y = (2 * a + 1 for a in tile)
    inside = False
    for (a, b), (c, d) in zip(polygon, polygon[1:] + polygon[:1]):
        a, b, c, d = 2 * a, 2 * b, 2 * c, 2 * d
        if (b > y) != (d > y):
            lhs = (x - a) * (d - b)
            rhs = (c - a) * (y - b)
            if (lhs < rhs if d > b else lhs > rhs):
                inside = not inside
    return inside


def inside_pond(tile: Point, center: Point, radius: Point):
    dx, dy = (2 * (a - b) + 1 for a, b in zip(tile, center))
    rx, ry = radius
    return (dx * ry) ** 2 + (dy * rx) ** 2 < (2 * rx * ry) ** 2


def sample(items):
    return ', '.join(str(p) for p in sorted(items)[:5])


@dataclass
class Road:
    id: str
    phase: str
    points: list[Point]
    width: int
    tiles: set[Point]


@dataclass
class Bridge:
    road: Road
    initial_width: int
    initial: set[Point]
    open_phase: str
    upgrade: str | None


class Layout:
    def __init__(self, data):
        self.data = data
        self.ids = {}
        self.phase_bits = {name: 1 << index for index, name in enumerate(PHASES)}
        self.outdoor_slots = []
        self.static_collision = []
        self.water_sites = {}
        self.permanent_water = set()
        self.parse()

    def register(self, value, kind):
        name = valid_id(value, kind)
        require(name not in self.ids, f'duplicate id {name} ({self.ids.get(name)} and {kind})')
        self.ids[name] = kind
        return name

    def rows(self, name):
        value = self.data[name]
        require(isinstance(value, list), f'{name}: expected array')
        return value

    def phase(self, value, label, allow_bridge=False):
        if allow_bridge and value == LEGACY_BRIDGE_PHASE:
            return 'day_1'
        require(isinstance(value, str) and value in self.phase_bits, f'{label}: unknown phase {value!r}')
        return value

    def add_collision(self, footprint, phase, label):
        self.static_collision.append((footprint, phase, label))

    def parse(self):
        d = self.data
        for field, value in {'width': WIDTH, 'height': HEIGHT, 'chunk_side': CHUNK,
                             'tile_world_pixels': 8, 'art_pixels_per_world_pixel': 2}.items():
            valid_int(d[field], field, value, value)
        self.id = valid_id(d['layout_id'], 'layout_id')
        revision = re.search(r'-r([1-9][0-9]*)\Z', self.id)
        require(revision is not None, 'layout_id: expected a positive -r revision suffix')
        self.revision = valid_int(d['map_revision'], 'map revision', int(revision[1]), 65_535)
        self.seed = valid_int(d['seed'], 'seed', 0, 2**64 - 1)
        require(tuple(d['phase_ids']) == PHASES, 'phase_ids must match the frozen bit assignment')
        self.regions = {}
        region_cells = set()
        for row in self.rows('regions'):
            name = self.register(row['id'], 'region')
            rect = valid_rect(row['rect'], name)
            occupied = cells(rect)
            require(not occupied & region_cells, f'{name}: primary region overlap')
            region_cells |= occupied
            self.regions[name] = rect

        self.pois = {}
        for row in self.rows('pois'):
            name = self.register(row['id'], 'poi')
            self.phase(row['phase'], name)
            require(row['region'] in self.regions, f'{name}: unknown region {row["region"]!r}')
            self.pois[name] = valid_tile(row['tile'], name)
            valid_id(row['use'], name + '.use')

        boundary = d['boundary']
        self.guard = valid_int(boundary['guard_tiles'], 'boundary.guard_tiles', 1, min(WIDTH, HEIGHT) // 2 - 1)
        self.corridor = [valid_tile(t, 'boundary corridor') for t in boundary['external_corridor']]
        self.corridor_tiles = strip(self.corridor, 1, 'boundary corridor')
        self.spawn = valid_tile(boundary['spawn_tile'], 'boundary spawn')
        self.reception = boundary['reception_poi']
        require(self.reception in self.pois and self.corridor[0] == self.pois[self.reception],
                'boundary corridor must start at its named reception POI')
        require(self.spawn == self.corridor[-1] and self.spawn[0] == WIDTH - 1,
                'boundary corridor must end at the eastern map edge')
        self.interior = cells((self.guard, self.guard, WIDTH - 2 * self.guard, HEIGHT - 2 * self.guard))
        require(self.corridor[0] in self.interior, 'boundary reception is not inside the world')
        self.allowed_land = self.interior | self.corridor_tiles

        water_shapes = []
        for row in self.rows('water'):
            name = self.register(row['id'], 'water')
            points = [valid_tile(t, name + ' polygon') for t in row['polygon']]
            require(len(points) >= 3 and len(set(points)) == len(points), f'{name}: invalid water polygon')
            self.water_sites[name] = [valid_tile(tile, name + ' fishing position') for tile in row['fishing']]
            water_shapes.append(points)
        pond = d['pond']
        pond_name = self.register(pond['id'], 'pond')
        center = valid_tile(pond['center'], 'pond center')
        require(isinstance(pond['radius'], list) and len(pond['radius']) == 2, 'pond: expected two radii')
        radius = tuple(valid_int(v, 'pond radius', 1, min(WIDTH, HEIGHT)) for v in pond['radius'])
        require(center[0] - radius[0] >= 0 and center[1] - radius[1] >= 0 and
                center[0] + radius[0] <= WIDTH and center[1] + radius[1] <= HEIGHT, 'pond crosses world edge')
        self.water_sites[pond_name] = [valid_tile(tile, 'pond fishing position') for tile in pond['fishing']]
        self.permanent_water = {
            tile for tile in cells((0, 0, WIDTH, HEIGHT))
            if any(inside_polygon(tile, polygon) for polygon in water_shapes) or inside_pond(tile, center, radius)
        }

        self.roads = {}
        legacy_roads = set()
        for row in self.rows('roads') + self.rows('spurs'):
            name = self.register(row['id'], 'road')
            phase = self.phase(row['phase'], name, allow_bridge=True)
            if row['phase'] == LEGACY_BRIDGE_PHASE:
                legacy_roads.add(name)
            points = [valid_tile(t, name) for t in row['points']]
            width = valid_int(row['width'], name + '.width', 1, 8)
            self.roads[name] = Road(name, phase, points, width, strip(points, width, name))

        self.bridges = {}
        for row in self.rows('bridge_rules'):
            name = row['road']
            require(name in self.roads and name not in self.bridges, f'invalid or repeated bridge road {name!r}')
            road = self.roads[name]
            initial = valid_int(row['initial_width'], name + '.initial_width', 0, road.width)
            valid_int(row['full_width'], name + '.full_width', road.width, road.width)
            phase = self.phase(row.get('open_phase', road.phase), name)
            require(phase == road.phase, f'{name}: road and bridge phase disagree')
            upgrade = row.get('upgrade_flag')
            if upgrade is not None:
                self.phase(upgrade, name + '.upgrade_flag')
                require(phase == 'day_1' and upgrade != phase and 0 < initial < road.width and
                        row.get('preserve_initial_lane') is True,
                        f'{name}: invalid bridge upgrade or initial lane is not preserved')
            else:
                require(initial in (0, road.width), f'{name}: missing upgrade flag for partial-width deck')
            initial_cells = strip(road.points, initial, name) if initial else set()
            require(len(road.points) == 2 and road.tiles & self.permanent_water and
                    all(p in self.allowed_land and p not in self.permanent_water for p in road.points),
                    f'{name}: bridge must cross water between two land endpoints')
            vertical = road.points[0][0] == road.points[-1][0]
            landings = {(x + off if vertical else x, y if vertical else y + off)
                        for x, y in road.points for off in range(road.width)}
            require(landings <= self.allowed_land and not landings & self.permanent_water,
                    f'{name}: at least one deck lane lacks a dry landing')
            if initial_cells:
                require(initial_cells & self.permanent_water, f'{name}: initial lane does not cross water')
            self.bridges[name] = Bridge(road, initial, initial_cells, phase, upgrade)
        require(legacy_roads == {key for key, b in self.bridges.items() if b.upgrade is not None},
                'pedestrian-then-upgrade phase must have exactly one matching bridge upgrade rule per road')

        self.buildings = {}
        self.residents = {self.register(r['id'], 'resident'): None for r in self.rows('residents')}
        self.parse_buildings()
        self.parse_fields_and_props()
        self.parse_sites()
        self.parse_scenic_objects()
        cover = d.get('ground_cover', [])
        require(isinstance(cover, list) and len(cover) <= 64, 'ground cover: expected at most 64 patches')
        for row in cover:
            name = self.register(row['id'], 'ground cover')
            center = valid_tile(row['center'], name)
            require(row['region'] in self.regions and contains(self.regions[row['region']], center),
                    f'{name}: ground cover center outside region')
            require(len(row['radius']) == 2 and len(row['color']) == 3, f'{name}: invalid cover shape/color')
            for radius in row['radius']:
                valid_int(radius, name + '.radius', 1, 16)
            for color in row['color']:
                valid_int(color, name + '.color', 0, 255)
        self.validate_geometry()

    def parse_buildings(self):
        poi_rows = {row['id']: row for row in self.rows('pois')}
        used_footprint = set()
        for row in self.rows('buildings'):
            name = self.register(row['id'], 'building')
            self.phase(row['phase'], name)
            rect = valid_rect(row['rect'], name)
            footprint = cells(rect)
            require(not footprint & used_footprint, f'{name}: overlaps another building footprint')
            used_footprint |= footprint
            require(row['region'] in self.regions, f'{name}: unknown region {row["region"]!r}')
            require(all(contains(self.regions[row['region']], t) for t in footprint), f'{name}: footprint outside its region')
            door = valid_tile(row['door'], name + '.door')
            valid_tile(row['material_slot'], name + '.material_slot')
            require(door not in footprint and any(distance(door, cell) == 1 for cell in footprint),
                    f'{name}: door must touch outside of footprint')
            door_row = poi_rows.get(name + '.door')
            require(door_row and door_row['tile'] == row['door'] and door_row['phase'] == row['phase'] and
                    door_row['region'] == row['region'] and door_row['use'] == 'entrance',
                    f'{name}: missing or mismatched door POI')
            envelope = valid_art_rect(row['render_envelope'], name)
            x, y, w, h = rect
            ex, ey, ew, eh = envelope
            require(ex <= x * TILE_ART and ey <= y * TILE_ART and
                    ex + ew >= (x + w) * TILE_ART and ey + eh >= (y + h) * TILE_ART,
                    f'{name}: envelope does not contain footprint')
            solid = row['solid_rects'] if row['solid_rects'] is not None else [row['rect']]
            require(isinstance(solid, list) and solid, f'{name}: no solid rectangles')
            occupied = set()
            for raw in solid:
                solid_cells = cells(valid_rect(raw, name + ' solid'))
                require(solid_cells <= footprint and not solid_cells & occupied, f'{name}: invalid or overlapping solid footprint')
                occupied |= solid_cells
            beds = valid_int(row['beds'], name + '.beds', 0, 8)
            require(len(row['occupants']) <= beds and len(set(row['occupants'])) == len(row['occupants']),
                    f'{name}: occupants do not fit assigned beds')
            for resident in row['occupants']:
                if resident in ('xiaohe', 'ayu'):
                    require(name == f'home_{resident}', f'{name}: invalid primary home for {resident}')
                else:
                    require(resident in self.residents and self.residents[resident] is None,
                            f'{name}: unknown resident or second address for {resident}')
                    self.residents[resident] = name
            valid_id(row['kind'], name + '.kind')
            self.buildings[name] = row
            self.add_collision(occupied, row['phase'], name)
        require(all(self.residents.values()), 'one or more residents have no assigned home')

    def parse_fields_and_props(self):
        self.fields = []
        for key, position in [('crops', 'work'), ('lots', 'access')]:
            for row in self.rows(key):
                name = self.register(row['id'], key)
                self.phase(row['phase'], name)
                rect = valid_rect(row['rect'], name)
                tile = valid_tile(row[position], name + '.' + position)
                require(not contains(rect, tile), f'{name}: approach is inside the footprint')
                if key == 'crops':
                    self.add_collision(cells(rect), row['phase'], name)
                else:
                    valid_id(row['capability'], name + '.capability')
                    self.fields.append((name, cells(rect)))

        notice = self.data['notice_prop']
        require(notice['solid'] is True, 'notice_prop must declare its solid footprint')
        reader = next((row for row in self.rows('pois') if row['id'] == notice['reader_poi']), None)
        require(reader is not None, 'notice_prop: reader POI does not exist')
        self.props = [{**notice, 'phase': reader['phase']}, *self.rows('solid_props')]
        for row in self.props:
            name = self.register(row['id'], 'prop')
            self.phase(row['phase'], name)
            tile = valid_tile(row['tile'], name)
            reader = row.get('reader_poi')
            if reader is not None:
                require(reader in self.pois and self.pois[reader] != tile, f'{name}: invalid reader POI')
            self.add_collision({tile}, row['phase'], name)

    def parse_sites(self):
        self.sites = {}
        self.outside_occupancy = {}
        for row in self.rows('interaction_sites') + self.rows('core_sites'):
            name = row['site']
            require(name in self.pois and name not in self.sites, f'{name}: missing POI or duplicate site definition')
            phase = self.phase(row['phase'], name)
            expected_phase = next(p['phase'] for p in self.rows('pois') if p['id'] == name)
            require(phase == expected_phase, f'{name}: interaction phase differs from POI')
            space = row.get('coordinate_space', 'outdoor')
            require(space in ('outdoor', 'interior'), f'{name}: unknown coordinate space {space!r}')
            interior = row.get('interior')
            require((space == 'interior') == bool(interior), f'{name}: missing or unexpected interior definition')
            bounds = (0, 0, WIDTH, HEIGHT)
            threshold = None
            if interior:
                self.register(interior['id'], 'interior')
                bounds = valid_rect(interior['bounds'], name + ' interior bounds')
                building = self.buildings.get(interior['building'])
                require(building and interior['portal'] == name == interior['building'] + '.door',
                        f'{name}: interior needs its own external building entrance')
                require(bounds[0:2] == (0, 0) and bounds[2:] == tuple(building['rect'][2:]),
                        f'{name}: interior bounds must use that building footprint')
                threshold = valid_tile(interior['threshold'], name + ' interior threshold', bounds)
                owner = interior.get('owner')
                require(owner is None or owner in building['occupants'], f'{name}: interior owner has no assigned bed')

            member_ids = set()
            occupied = {}
            actor_paths = []
            for slot in row['slots']:
                slot_id = self.register(slot['id'], 'slot')
                require(slot_id == name + '.' + valid_id(slot['slot']), f'{slot_id}: invalid local slot key')
                member_ids.add(slot_id)
                tile = valid_tile(slot['tile'], slot_id, bounds)
                require(tile not in occupied, f'{slot_id}: same tile as {occupied.get(tile)}')
                occupied[tile] = slot_id
                require(slot['role'] in ('stand', 'wait', 'load', 'prop'), f'{slot_id}: invalid slot role')
                require(slot['facing'] in ('N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'), f'{slot_id}: invalid facing')
                valid_int(slot['capacity'], slot_id + '.capacity', 1, 1)
                path = slot.get('access_path')
                if slot['role'] == 'prop':
                    require(path is None and slot['reservation_kind'] == 'equipment', f'{slot_id}: invalid prop slot')
                else:
                    require(isinstance(path, list) and path and slot['reservation_kind'] == 'standing_tile',
                            f'{slot_id}: invalid or missing standing path')
                    path = [valid_tile(t, slot_id + ' access', bounds) for t in path]
                    require(path[-1] == tile and all(distance(a, b) == 1 for a, b in zip(path, path[1:])),
                            f'{slot_id}: path must end at the slot and follow adjacent tiles')
                    if interior:
                        require(path[0] == threshold, f'{slot_id}: local path does not start at the threshold')
                        actor_paths.append((slot_id, path))
                    else:
                        self.outdoor_slots.append((slot_id, phase, path))
                if not interior:
                    require(tile not in self.outside_occupancy,
                            f'{slot_id}: same outdoor tile as {self.outside_occupancy.get(tile)}')
                    self.outside_occupancy[tile] = slot_id
            if interior:
                props = {tuple(s['tile']) for s in row['slots'] if s['role'] == 'prop'}
                require(threshold not in props, f'{name}: indoor threshold is blocked')
                for slot_id, path in actor_paths:
                    require(not set(path) & props, f'{slot_id}: indoor path crosses a prop')

            for group in row['reservation_groups']:
                group_id = self.register(group['id'], 'reservation group')
                require(group_id == name + '.' + valid_id(group['slot']), f'{group_id}: invalid local group key')
                require(group['role'] in ('exchange_group', 'operation_group') and group['reservation_kind'] == 'atomic_group',
                        f'{group_id}: invalid reservation type')
                valid_int(group['capacity'], group_id + '.capacity', 1, 1)
                members = group['members']
                require(isinstance(members, list) and len(members) >= 2 and len(set(members)) == len(members)
                        and set(members) <= member_ids, f'{group_id}: invalid member references')
            self.sites[name] = row

    def parse_scenic_objects(self):
        reserved = set(self.corridor_tiles) | set(self.pois.values()) | set(self.outside_occupancy)
        reserved.update(tile for _, _, path in self.outdoor_slots for tile in path)
        reserved.update(tile for feet in self.water_sites.values() for tile in feet)
        reserved.update(tuple(b['material_slot']) for b in self.buildings.values())
        for footprint in (r.tiles for r in self.roads.values()):
            reserved.update(footprint)
        for row in (*self.buildings.values(), *self.rows('crops'), *self.rows('lots')):
            reserved.update(cells(tuple(row['rect'])))
        for footprint, _, _ in self.static_collision:
            reserved.update(footprint)

        self.scenic = []
        assigned = set()
        for row in self.rows('scenic_objects'):
            name = self.register(row['id'], 'scenic object')
            tile = valid_tile(row['tile'], name)
            phase = self.phase(row['phase'], name)
            valid_id(row['cluster'], name + '.cluster')
            kind = row['kind']
            require(isinstance(kind, str) and kind in SCENIC_KINDS, f'{name}: unknown scenic kind {kind!r}')
            solid = kind not in WALKABLE_SCENIC_KINDS
            require(row['solid'] is solid, f'{name}: wrong collision flag for {kind}')
            require(tile in self.interior and tile not in self.permanent_water,
                    f'{name}: scenery must be on dry land inside the boundary at {tile}')
            require(tile not in assigned, f'{name}: repeated scenery tile at {tile}')
            require(tile not in reserved, f'{name}: scenery overlaps reserved map geometry at {tile}')
            assigned.add(tile)
            self.scenic.append(row)
            if solid:
                self.add_collision({tile}, phase, name)

    def active_deck(self, flags):
        deck = set()
        for bridge in self.bridges.values():
            if bridge.open_phase in flags:
                deck |= bridge.initial if bridge.upgrade and bridge.upgrade not in flags else bridge.road.tiles
        return deck

    def validate_geometry(self):
        all_occupied = set()
        for footprint, _, label in self.static_collision:
            require(footprint <= self.allowed_land, f'{label}: solid footprint crosses protected boundary')
            require(not footprint & self.permanent_water, f'{label}: solid footprint crosses water')
            require(not footprint & all_occupied, f'{label}: overlaps another solid footprint')
            all_occupied |= footprint
        for name, footprint in self.fields:
            require(footprint <= self.allowed_land and not footprint & (self.permanent_water | all_occupied),
                    f'{name}: unavailable lot tiles')
            all_occupied |= footprint

        all_road = set().union(*(r.tiles for r in self.roads.values()))
        outdoor_approaches = {tile for _, _, path in self.outdoor_slots for tile in path}
        poi_tiles = set(self.pois.values())
        for row in self.buildings.values():
            tile = tuple(row['material_slot'])
            require(tile in self.allowed_land and tile not in self.permanent_water and tile not in all_occupied and
                    tile not in poi_tiles and tile not in self.outside_occupancy and tile not in outdoor_approaches and
                    tile not in all_road and contains(self.regions[row['region']], tile),
                    f'{row["id"]}: material slot overlaps an approach, another site, or blocked/out-of-region cell at {tile}')
        for name, fishing in self.water_sites.items():
            for tile in fishing:
                require(tile in self.allowed_land and tile not in self.permanent_water and tile not in all_occupied,
                        f'{name}: fishing foot anchor is blocked at {tile}')

        start = self.pois['home_xiaohe.door']
        market = self.pois['market_gate']
        geo = PHASES[:-1]
        for i, milestone in enumerate(geo):
            for freight in (False, True):
                flags = set(geo[:i + 1])
                if freight:
                    flags.add('main_bridge_freight')
                label = f'{milestone} ({"freight" if freight else "pedestrian"})'
                self.validate_phase(flags, label, start, market)
        for capability in geo[2:]:
            self.validate_phase({'day_1', capability}, f'independent {capability}', start, market)
        self.phase_count = 2 * len(geo) + len(geo[2:])

        flags = set(PHASES)
        _, network, walk = self.geometry(flags)
        filled = set().union(*(footprint for _, footprint in self.fields))
        require(not filled & network, f'planned lots block public roads: {sample(filled & network)}')
        reached = flood(start, walk - filled)
        require(all(p in reached for _, p, _ in self.goals(flags)), 'completed planned lot blocks another site')

        for trip in self.rows('trips'):
            name = trip['id']
            require(name in self.pois, f'{name}: unknown sample trip destination')
            path = [valid_tile(t, name + ' trip') for t in trip['path']]
            require(path and path[0] == start and path[-1] == self.pois[name] and
                    all(distance(a, b) == 1 for a, b in zip(path, path[1:])) and set(path) <= network & walk,
                    f'{name}: invalid sample trip')
            require(len(path) - 1 == trip['tiles'] and (len(path) - 1) * 10 == trip['world_seconds'],
                    f'{name}: sample trip duration does not match edge count')

    def geometry(self, flags):
        deck = self.active_deck(flags)
        road = set().union(*(r.tiles for r in self.roads.values() if r.id not in self.bridges and r.phase in flags))
        occupied = set().union(*(footprint for footprint, phase, _ in self.static_collision if phase in flags))
        walk = self.allowed_land - ((self.permanent_water - deck) | occupied)
        return deck, road | deck, walk

    def goals(self, flags):
        for key, pos in [('pois', 'tile'), ('crops', 'work'), ('lots', 'access')]:
            for row in self.rows(key):
                if row['phase'] in flags:
                    yield row['id'], tuple(row[pos]), row['phase']

    def validate_phase(self, flags, label, start, market):
        deck, network, walk = self.geometry(flags)
        invalid = network - walk
        require(not invalid, f'{label}: road or bridge crosses blocked tiles: {sample(invalid)}')
        reached = flood(start, network & walk)
        for name, pos, _ in self.goals(flags):
            require(pos in reached, f'{label}: active {name} cannot be reached through exported approaches')
        require(self.spawn in reached, f'{label}: external boundary entrance is disconnected')
        reached_on_land = flood(start, walk)
        require(market not in flood(start, walk - deck), f'{label}: river can be bypassed without a bridge')
        for slot_id, phase, path in self.outdoor_slots:
            if phase in flags:
                require(set(path) <= walk, f'{label}: outdoor slot {slot_id} has a blocked approach')
                require(path[0] in reached_on_land, f'{label}: outdoor slot {slot_id} is disconnected')

    def pack(self):
        areas = {tile: i for i, (_, rect) in enumerate(sorted(self.regions.items())) for tile in cells(rect)}
        road_masks, deck_masks, solid_masks, scenic_masks = {}, {}, {}, {}

        def add_mask(masks, tiles, phase):
            bit = self.phase_bits[phase]
            for tile in tiles:
                masks[tile] = masks.get(tile, 0) | bit

        for road in self.roads.values():
            if road.id in self.bridges:
                bridge = self.bridges[road.id]
                if bridge.upgrade:
                    add_mask(deck_masks, bridge.initial, bridge.open_phase)
                    add_mask(deck_masks, road.tiles - bridge.initial, bridge.upgrade)
                else:
                    add_mask(deck_masks, road.tiles, bridge.open_phase)
            else:
                add_mask(road_masks, road.tiles, road.phase)
        for tiles, phase, _ in self.static_collision:
            add_mask(solid_masks, tiles, phase)
        for row in self.scenic:
            if row['solid']:
                add_mask(scenic_masks, {tuple(row['tile'])}, row['phase'])

        packed = bytearray()
        rules = {}
        require(len(self.regions) < NO_REGION, 'region index exceeds compact tile format')
        for y in range(HEIGHT):
            for x in range(WIDTH):
                tile = x, y
                base = 'Boundary' if tile not in self.allowed_land else 'Water' if tile in self.permanent_water else 'Grass'
                rule = (base, road_masks.get(tile, 0), deck_masks.get(tile, 0),
                        solid_masks.get(tile, 0), scenic_masks.get(tile, 0))
                key = rules.setdefault(rule, len(rules))
                require(key < 256, 'tile rule palette exceeds compact tile format')
                packed.extend((areas.get(tile, NO_REGION), key))
        return bytes(packed), list(rules)


def literal(value):
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if type(value) is int:
        return str(value)
    raise TypeError(f'invalid generated literal {value!r}')


def rust_struct(name, **fields):
    return name + ' { ' + ', '.join(f'{key}: {value}' for key, value in fields.items()) + ' }'


def rust_slice(values):
    return '&[' + ', '.join(values) + ']'


def tile_expr(point):
    return f'TilePos::new({point[0]}, {point[1]})'


def rect_expr(rect):
    return 'TileRect::new(' + ', '.join(map(str, rect)) + ')'


def phase_expr(name):
    return f'PhaseId({PHASES.index(name)})'


def option(expr):
    return f'Some({expr})' if expr is not None else 'None'


def emit(layout, source_digest, packed_name):
    d = layout.data
    generated = ['// Generated by tools/compile_map.py. Edit the source blueprint instead.',
                 'use super::*;', '']

    def table(name, rust_type, rows, visibility='pub(super)'):
        generated.append('#[rustfmt::skip]')
        generated.append(f'{visibility} static {name}: &[{rust_type}] = &[')
        generated.extend('    ' + row + ',' for row in rows)
        generated.append('];')
        generated.append('')

    def rows(name):
        return sorted(d[name], key=lambda row: row['id'])

    packed, rules = layout.pack()
    table('PHASES', '&str', map(literal, PHASES))
    table('RULES', 'TileRule', (
        rust_struct('TileRule', base=f'Terrain::{base}', road=road, bridge=bridge, solid=solid, scenic=scenic)
        for base, road, bridge, solid, scenic in rules
    ))
    table('REGIONS', 'RegionDef', (
        rust_struct('RegionDef', id=literal(name), rect=rect_expr(rect))
        for name, rect in sorted(layout.regions.items())
    ))
    table('GROUND_COVER', 'GroundCoverDef', (
        rust_struct('GroundCoverDef', region=literal(row['region']), center=tile_expr(row['center']),
                    radius=tile_expr(row['radius']), color='[' + ', '.join(map(str, row['color'])) + ']')
        for row in sorted(d.get('ground_cover', []), key=lambda r: (r['region'], r['id']))
    ))
    table('POIS', 'PoiDef', (
        rust_struct('PoiDef', id=literal(row['id']), region=literal(row['region']), tile=tile_expr(row['tile']),
                    purpose=literal(row['use']), phase=phase_expr(row['phase'])) for row in rows('pois')
    ))
    table('LOTS', 'LotDef', (
        rust_struct('LotDef', id=literal(row['id']), rect=rect_expr(row['rect']), access=tile_expr(row['access']),
                    capability=literal(row['capability']), phase=phase_expr(row['phase'])) for row in rows('lots')
    ))
    table('BUILDINGS', 'BuildingDef', (
        rust_struct('BuildingDef', id=literal(row['id']), region=literal(row['region']), footprint=rect_expr(row['rect']),
                    solid=rust_slice(map(rect_expr, row['solid_rects'] or [row['rect']])), door=tile_expr(row['door']),
                    door_poi=literal(row['id'] + '.door'), beds=row['beds'], occupants=rust_slice(map(literal, row['occupants'])),
                    phase=phase_expr(row['phase']), kind=literal(row['kind']),
                    envelope=rust_struct('ArtRect', **dict(zip('xywh', valid_art_rect(row['render_envelope'], row['id'])))),
                    material_slot=tile_expr(row['material_slot'])) for row in rows('buildings')
    ))
    table('CROPS', 'CropDef', (
        rust_struct('CropDef', id=literal(row['id']), rect=rect_expr(row['rect']), work=tile_expr(row['work']),
                    phase=phase_expr(row['phase'])) for row in rows('crops')
    ))
    table('RESIDENTS', 'ResidentDef', (
        rust_struct('ResidentDef', id=literal(name), home=literal(home)) for name, home in sorted(layout.residents.items())
    ))
    table('ROADS', 'RoadDef', (
        rust_struct('RoadDef', id=literal(r.id), axis=rust_slice(map(tile_expr, r.points)), width=r.width, phase=phase_expr(r.phase))
        for _, r in sorted(layout.roads.items())
    ))
    table('BRIDGES', 'BridgeDef', (
        rust_struct('BridgeDef', id=literal(name), axis=rust_slice(map(tile_expr, b.road.points)), initial_width=b.initial_width,
                    full_width=b.road.width, open_phase=phase_expr(b.open_phase), upgrade=option(phase_expr(b.upgrade) if b.upgrade else None))
        for name, b in sorted(layout.bridges.items())
    ))
    table('PROPS', 'PropDef', (
        rust_struct('PropDef', id=literal(row['id']), tile=tile_expr(row['tile']), phase=phase_expr(row['phase']),
                    reader_poi=option(literal(row['reader_poi']) if row.get('reader_poi') else None))
        for row in sorted(layout.props, key=lambda row: row['id'])
    ))
    table('SCENIC_OBJECTS', 'ScenicObject', (
        rust_struct('ScenicObject', id=literal(row['id']), tile=tile_expr(row['tile']),
                    kind=f'ScenicKind::{SCENIC_KINDS[row["kind"]]}', solid=str(row['solid']).lower(),
                    phase=phase_expr(row['phase']))
        for row in sorted(layout.scenic, key=lambda row: row['id'])
    ), visibility='pub')

    water = [rust_struct('WaterDef', id=literal(row['id']),
                         shape='WaterShape::Polygon(' + rust_slice(map(tile_expr, row['polygon'])) + ')',
                         fishing=rust_slice(map(tile_expr, row['fishing']))) for row in rows('water')]
    pond = d['pond']
    water.append(rust_struct('WaterDef', id=literal(pond['id']),
                             shape=rust_struct('WaterShape::Pond', center=tile_expr(pond['center']), radius=tile_expr(pond['radius'])),
                             fishing=rust_slice(map(tile_expr, pond['fishing']))))
    table('WATER', 'WaterDef', water)

    def site_expr(row):
        raw_interior = row.get('interior')
        interior = None
        if raw_interior:
            interior = rust_struct('InteriorDef', id=literal(raw_interior['id']), building=literal(raw_interior['building']),
                                   portal=literal(raw_interior['portal']), bounds=rect_expr(raw_interior['bounds']),
                                   threshold=tile_expr(raw_interior['threshold']),
                                   owner=option(literal(raw_interior['owner']) if raw_interior.get('owner') else None))

        def slot_expr(slot):
            tile = tile_expr(slot['tile'])
            if raw_interior:
                position = rust_struct('SlotPos::Interior', interior=literal(raw_interior['id']), local=tile)
            else:
                position = f'SlotPos::Outdoor({tile})'
            return rust_struct('SlotDef', id=literal(slot['id']), role='SlotRole::' + slot['role'].title(),
                               position=position, facing='Facing::' + slot['facing'].title(), capacity=slot['capacity'],
                               access_path=rust_slice(map(tile_expr, slot['access_path'] or [])))

        groups = [rust_struct('ReservationGroupDef', id=literal(group['id']), capacity=group['capacity'],
                              members=rust_slice(map(literal, group['members'])))
                  for group in sorted(row['reservation_groups'], key=lambda group: group['id'])]
        return rust_struct('SiteDef', id=literal(row['site']), phase=phase_expr(row['phase']), interior=option(interior),
                           slots=rust_slice(map(slot_expr, sorted(row['slots'], key=lambda slot: slot['id']))),
                           groups=rust_slice(groups))

    table('SITES', 'SiteDef', (site_expr(row) for _, row in sorted(layout.sites.items())))
    boundary = rust_struct('BoundaryDef', guard_tiles=layout.guard, corridor=rust_slice(map(tile_expr, layout.corridor)),
                           spawn=tile_expr(layout.spawn), reception_poi=literal(layout.reception))
    generated.extend(['#[rustfmt::skip]', 'pub static WILLOW_VILLAGE: MapDef = MapDef {',
                      f'    layout_id: {literal(layout.id)},',
                      f'    map_revision: {layout.revision},',
                      f'    source_sha256: {literal(source_digest)},',
                      '    width: MAP_WIDTH,', '    height: MAP_HEIGHT,', f'    seed: {layout.seed},',
                      '    phases: PHASES,', '    regions: REGIONS,', '    pois: POIS,', '    lots: LOTS,',
                      '    buildings: BUILDINGS,', '    crops: CROPS,', '    residents: RESIDENTS,', '    sites: SITES,',
                      f'    boundary: {boundary},', '    roads: ROADS,', '    bridges: BRIDGES,', '    solid_props: PROPS,', '    water: WATER,',
                      '    scenic: SCENIC_OBJECTS,',
                      '    ground_cover: GROUND_COVER,',
                      f'    tiles: include_bytes!("../assets/maps/{packed_name}"),', '    rules: RULES,', '};', ''])
    return '\n'.join(generated).encode('utf-8'), packed, len(rules)


def reject_nonfinite(value):
    raise MapError(f'invalid number {value}')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='validate and compare outputs without writing')
    parser.add_argument('--source', type=Path, default=SOURCE, help='alternate blueprint for validation')
    parser.add_argument('--validate-only', action='store_true', help='validate without checking or writing output files')
    args = parser.parse_args(argv)
    if args.source != SOURCE and not args.validate_only:
        parser.error('--source requires --validate-only')
    if args.check and args.validate_only:
        parser.error('--check and --validate-only cannot be combined')
    try:
        raw = args.source.read_bytes()
        data = json.loads(raw, parse_float=Fraction, parse_constant=reject_nonfinite, object_pairs_hook=unique_object)
        layout = Layout(data)
        digest = hashlib.sha256(raw).hexdigest()
        rust, packed, rules = emit(layout, digest, PACKED.name)
        if args.check:
            for file, content in ((RUST, rust), (PACKED, packed)):
                require(file.is_file() and file.read_bytes() == content, f'{file.name} is stale. Run tools/compile_map.py.')
        elif not args.validate_only:
            for file, content in ((RUST, rust), (PACKED, packed)):
                if not file.is_file() or file.read_bytes() != content:
                    file.write_bytes(content)
        print(f'{layout.id}: {WIDTH}x{HEIGHT}, {len(layout.roads)} road/spur definitions, '
              f'{len(layout.pois)} POIs, {len(layout.sites)} sites, {len(layout.scenic)} scenery objects, '
              f'{layout.phase_count} verified profiles')
        print(f'source {len(raw)} B / {digest}; packed tiles {len(packed)} B, {rules} rules, generated Rust {len(rust)} B')
    except (OSError, KeyError, TypeError, ValueError, StopIteration) as error:
        print(f'map compilation failed: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
