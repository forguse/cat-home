//! Fixed reclamation sites on the published Willow Village map.

use crate::map_data::{
    PhaseMask, ScenicKind, ScenicObject, SlotPos, Terrain, TilePos, TileRect, WILLOW_VILLAGE,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RestoredField {
    pub(crate) id: &'static str,
    pub(crate) rect: TileRect,
    pub(crate) work: TilePos,
}

/// Appended in this order after the 30 original crop beds.
pub(crate) const RESTORED_FIELDS: [RestoredField; 2] = [
    RestoredField {
        id: "crop_reclaimed_south",
        rect: TileRect::new(22, 55, 2, 2),
        work: TilePos::new(23, 57),
    },
    RestoredField {
        id: "crop_reclaimed_east",
        rect: TileRect::new(49, 56, 2, 2),
        work: TilePos::new(50, 58),
    },
];

pub(crate) const RECLAIMED_HOME_ORIGIN: [u8; 2] = [25, 34];
pub(crate) const RECLAIMED_HOME: TileRect = TileRect::new(
    RECLAIMED_HOME_ORIGIN[0] as u16,
    RECLAIMED_HOME_ORIGIN[1] as u16,
    4,
    3,
);
pub(crate) const RECLAIMED_HOME_DOOR: TilePos = TilePos::new(27, 37);
pub(crate) const RECLAIMED_HOME_MATERIAL: TilePos = TilePos::new(28, 37);

/// One general rule applies to authored plants and stone. The landscape
/// ledger also recognizes temporary ground here so real grading can consume
/// it without producing any loose material.
pub(crate) fn harvestable_scenic(object: &ScenicObject) -> bool {
    matches!(
        object.kind,
        ScenicKind::Tree
            | ScenicKind::Log
            | ScenicKind::Rock
            | ScenicKind::Grass
            | ScenicKind::Reeds
            | ScenicKind::Shrub
    ) && [(-1, 0), (0, -1), (0, 0), (0, 1), (1, 0)]
        .into_iter()
        .all(|(dx, dy)| {
            object.tile.offset(dx, dy).is_some_and(|tile| {
                WILLOW_VILLAGE
                    .tile(tile, PhaseMask::DAY_ONE)
                    .is_some_and(|cell| matches!(cell.terrain, Terrain::Grass | Terrain::Road))
            })
        })
}

/// Scenic shifts leave doors, reservations, fishing positions, bridge decks,
/// and narrow entrances clear. Dry road shoulders can be used briefly.
pub(crate) fn work_foot_unreserved(tile: TilePos) -> bool {
    let map = &WILLOW_VILLAGE;
    map.tile(tile, PhaseMask::ALL)
        .is_some_and(|cell| matches!(cell.terrain, Terrain::Grass | Terrain::Road))
        && tile != RECLAIMED_HOME_DOOR
        && tile != RECLAIMED_HOME_MATERIAL
        && !map.boundary.corridor.contains(&tile)
        && !map.water.iter().any(|water| water.fishing.contains(&tile))
        && !map.pois.iter().any(|poi| poi.tile == tile)
        && !map.crops.iter().any(|crop| crop.work == tile)
        && !map
            .buildings
            .iter()
            .any(|building| building.material_slot == tile)
        && !map
            .sites
            .iter()
            .filter(|site| site.interior.is_none())
            .flat_map(|site| site.slots.iter())
            .any(|slot| slot.position == SlotPos::Outdoor(tile) || slot.access_path.contains(&tile))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, VecDeque};

    fn cells(rect: TileRect) -> impl Iterator<Item = TilePos> {
        (rect.y..rect.y + rect.h)
            .flat_map(move |y| (rect.x..rect.x + rect.w).map(move |x| TilePos::new(x, y)))
    }

    #[test]
    fn reclaimed_sites_require_real_clearance_and_preserve_access_after_occupation() {
        let map = &WILLOW_VILLAGE;
        assert_eq!(map.crops.len(), 30);
        let sites = [
            (
                RESTORED_FIELDS[0].rect,
                RESTORED_FIELDS[0].work,
                &[
                    "density.field_inner_island.rock.22_55",
                    "density.field_inner_island.grass.23_55",
                ][..],
            ),
            (
                RESTORED_FIELDS[1].rect,
                RESTORED_FIELDS[1].work,
                &[
                    "density.field_east.tree.49_57",
                    "density.field_east.rock.50_57",
                    "scenic.eastern_headland.flowers.50_56",
                ][..],
            ),
            (
                RECLAIMED_HOME,
                RECLAIMED_HOME_DOOR,
                &[
                    "density.pond_east_slope.rock.28_34",
                    "density.pond_east_slope.shrub.28_36",
                    "scenic.pond_east_meadow.grass.27_34",
                ][..],
            ),
        ];
        let occupied: BTreeSet<_> = sites.iter().flat_map(|(rect, _, _)| cells(*rect)).collect();
        assert_eq!(occupied.len(), 20);
        for (rect, work, ids) in sites {
            assert!(!rect.contains(work));
            for tile in cells(rect) {
                assert_eq!(
                    map.tile(tile, PhaseMask::ALL).unwrap().terrain,
                    Terrain::Grass
                );
                assert!(!map.crops.iter().any(|crop| crop.rect.contains(tile)));
                assert!(
                    !map.buildings
                        .iter()
                        .any(|building| building.footprint.contains(tile))
                );
            }
            let occupants: Vec<_> = map
                .scenic
                .iter()
                .filter(|row| rect.contains(row.tile))
                .collect();
            assert_eq!(occupants.len(), ids.len());
            assert!(occupants.iter().all(|row| ids.contains(&row.id)));
            assert!(occupants.iter().any(|row| row.solid));
            assert!(
                occupants
                    .iter()
                    .all(|row| row.kind == ScenicKind::Flowers || harvestable_scenic(row))
            );
        }
        assert!(!occupied.contains(&RECLAIMED_HOME_MATERIAL));
        for flags in [
            PhaseMask::DAY_ONE.with(map.phase("map_dense_r3").unwrap()),
            PhaseMask::ALL,
        ] {
            let start = map.poi("home_xiaohe.door").unwrap().tile;
            let mut frontier = VecDeque::from([start]);
            let mut reached = BTreeSet::from([start]);
            while let Some(tile) = frontier.pop_front() {
                for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
                    let Some(next) = tile.offset(dx, dy) else {
                        continue;
                    };
                    if !occupied.contains(&next)
                        && map.tile(next, flags).is_some_and(|tile| !tile.blocked)
                        && reached.insert(next)
                    {
                        frontier.push_back(next);
                    }
                }
            }
            for (_, work, _) in sites {
                assert!(reached.contains(&work), "{work:?}");
            }
            assert!(reached.contains(&RECLAIMED_HOME_MATERIAL));
            for poi in map.pois.iter().filter(|poi| flags.contains(poi.phase)) {
                assert!(reached.contains(&poi.tile), "{}", poi.id);
            }
        }
    }

    #[test]
    fn resource_work_has_reachable_dry_stands_and_respects_public_reserves() {
        let map = &WILLOW_VILLAGE;
        let leaves = map
            .scenic
            .iter()
            .filter(|object| harvestable_scenic(object))
            .collect::<Vec<_>>();
        assert_eq!(
            leaves
                .iter()
                .filter(|object| object.temporary_ground)
                .count(),
            732
        );
        assert_eq!(
            leaves
                .iter()
                .filter(|object| !object.temporary_ground)
                .count(),
            2_635
        );
        assert_eq!(map.scenic.len() - leaves.len(), 428);
        for object in leaves {
            assert_eq!(
                map.tile(object.tile, PhaseMask::DAY_ONE).unwrap().terrain,
                Terrain::Grass
            );
            assert!(
                [(0, 1), (1, 0), (0, -1), (-1, 0)]
                    .into_iter()
                    .filter_map(|(dx, dy)| object.tile.offset(dx, dy))
                    .all(|tile| {
                        map.tile(tile, PhaseMask::DAY_ONE).is_some_and(|cell| {
                            !matches!(cell.terrain, Terrain::Bridge | Terrain::Water)
                        })
                    }),
                "{} is too close to a shoreline",
                object.id
            );
        }
        for region in RESTORED_FIELDS
            .map(|field| field.rect)
            .into_iter()
            .chain([RECLAIMED_HOME])
        {
            for object in map
                .scenic
                .iter()
                .filter(|row| region.contains(row.tile) && harvestable_scenic(row))
            {
                assert!(
                    [(0, 1), (1, 0), (0, -1), (-1, 0)]
                        .into_iter()
                        .filter_map(|(dx, dy)| object.tile.offset(dx, dy))
                        .any(|stand| {
                            work_foot_unreserved(stand)
                                && map
                                    .tile(stand, PhaseMask::ALL)
                                    .is_some_and(|cell| !cell.blocked)
                        }),
                    "{} starts without a legal work stand",
                    object.id
                );
            }
        }
    }
}
