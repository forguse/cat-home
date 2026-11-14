"""Author low vegetation in functional clusters, without changing saved collision geometry.

The compiled map still validates every door, work position, crop bed and approach.
This edits the blueprint only; the normal compiler owns generated assets.
"""
import json
from fractions import Fraction
from compile_map import Layout, SOURCE, cells


def main():
    data = json.loads(SOURCE.read_bytes())
    previous_layout = Layout(json.loads(json.dumps(data), parse_float=Fraction))
    patches = [
        ('home', 'pond_east_meadow', (25, 32), (6, 5), (107, 137, 76), 'flowers'),
        ('home', 'pond_south_reeds', (15, 38), (7, 2), (98, 132, 84), 'reeds'),
        ('home', 'east_herb_border', (45, 30), (5, 7), (112, 139, 80), 'flowers'),
        ('home', 'pantry_herbs', (29, 28), (4, 3), (120, 141, 79), 'flowers'),
        ('home', 'north_cottage_edge', (19, 7), (5, 3), (109, 134, 72), 'grass'),
        ('farm', 'western_headland', (7, 50), (3, 9), (118, 139, 78), 'grass'),
        ('farm', 'eastern_headland', (48, 52), (3, 8), (112, 137, 77), 'flowers'),
        ('orchard', 'orchard_understory', (25, 78), (12, 7), (109, 130, 73), 'grass'),
        ('north_homes', 'neighbor_garden_west', (86, 24), (8, 4), (110, 138, 79), 'flowers'),
        ('north_homes', 'neighbor_garden_east', (122, 24), (12, 4), (110, 138, 79), 'flowers'),
        ('market', 'market_garden_edge', (112, 59), (6, 3), (119, 140, 80), 'flowers'),
        ('craft', 'craft_verge', (146, 58), (14, 4), (126, 137, 88), 'grass'),
        ('east_homes', 'east_homes_garden', (147, 88), (14, 6), (113, 139, 80), 'flowers'),
        ('commons', 'commons_meadow', (107, 85), (10, 4), (119, 141, 83), 'flowers'),
        ('wetland', 'wetland_reed_islands', (79, 111), (6, 10), (96, 128, 81), 'reeds'),
        ('quarry', 'quarry_verge', (163, 22), (15, 7), (141, 138, 108), 'grass'),
        ('east_woods', 'east_woodland_floor', (180, 65), (6, 15), (101, 125, 71), 'grass'),
    ]
    data['ground_cover'] = [dict(id='cover.'+name, region=region, center=list(center), radius=list(radius), color=list(color))
                            for region, name, center, radius, color, kind in patches]
    authored = {p[1] for p in patches}
    data['scenic_objects'] = [r for r in data['scenic_objects'] if r['cluster'] not in authored]
    layout = Layout(json.loads(json.dumps(data), parse_float=Fraction))
    reserved = set(layout.corridor_tiles) | set(layout.pois.values()) | set(layout.outside_occupancy)
    reserved.update(t for _, _, path in layout.outdoor_slots for t in path)
    reserved.update(t for feet in layout.water_sites.values() for t in feet)
    reserved.update(tuple(b['material_slot']) for b in layout.buildings.values())
    for road in layout.roads.values(): reserved.update(road.tiles)
    for collection in ('buildings', 'crops', 'lots'):
        for row in data[collection]: reserved.update(cells(tuple(row['rect'])))
    for footprint, _, _ in layout.static_collision: reserved.update(footprint)
    # Dynamic productive trees and retained legacy relocation plots are owned by
    # world_layout.rs, rather than static collision. Protect their physical feet too.
    ox, oy = layout.pois['orchard_tools']
    lx, ly = layout.pois['forest_loading']
    trees = [(ox+dx,oy+dy) for dx,dy in ((-5,1),(1,1),(2,6),(-6,7))]
    trees += [(lx-24+(i%4)*5,ly-8+(i//4)*5) for i in range(16) if i not in range(8,12)]
    reserved.update(trees)
    reserved.update((x,y+1) for x,y in trees)
    for rect in ((6,4,6,4),(35,32,4,3),(18,5,4,3)): reserved.update(cells(rect))
    # Don't hide door furniture under foreground plants.
    protected = {(x+dx,y+dy) for x,y in reserved for dx,dy in ((0,0),(-1,0),(1,0),(0,-1),(0,1))}
    assigned = {tuple(row['tile']) for row in data['scenic_objects']}
    added = 0
    for region, name, (cx,cy), (rx,ry), color, kind in patches:
        candidates = [(x,y) for y in range(cy-ry, cy+ry+1) for x in range(cx-rx,cx+rx+1)]
        rank = lambda t: ((t[0]*73856093) ^ (t[1]*19349663) ^ 20261113) & 0xffffffff
        for x,y in sorted(candidates, key=rank):
            if ((x-cx)/rx)**2 + ((y-cy)/ry)**2 > .86: continue
            if rank((x,y)) % 100 > 48: continue
            tile = x,y
            if tile in assigned or tile in protected or tile not in layout.interior or tile in layout.permanent_water: continue
            if any(abs(x-xx)+abs(y-yy)<3 for xx,yy in assigned): continue
            variant = 'grass' if rank((x,y))%5<3 else kind
            data['scenic_objects'].append(dict(id=f'scenic.{name}.{variant}.{x}_{y}', cluster=name,
                kind=variant, tile=[x,y], phase='day_1', solid=False))
            assigned.add(tile)
            added += 1
    checked = Layout(json.loads(json.dumps(data), parse_float=Fraction))
    assert checked.pack()[0] == previous_layout.pack()[0], 'groundcover must not alter collision or map state'
    SOURCE.write_text(json.dumps(data, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
    print(f'Groundcover patches={len(patches)}, new low plants={added}, total scenery={len(data["scenic_objects"])}; collision unchanged')


if __name__ == '__main__': main()
