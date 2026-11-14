//! Authoritative static terrain plus a bounded transient obstacle overlay.
//!
//! This is the new-map foundation. The legacy world deliberately keeps its own
//! validation until save migration is proven. Rendering reads this view; it
//! cannot advance the clock or change occupancy.
use crate::map_data::{MapDef, PhaseMask, TileInfo, TilePos};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_MAP_WIDTH: u16 = 256;
pub const MAX_MAP_HEIGHT: u16 = 192;
const MAX_TILES: usize = MAX_MAP_WIDTH as usize * MAX_MAP_HEIGHT as usize;
const CHUNK_SIDE: u16 = 16;
const WORLD_PIXELS_PER_TILE: i64 = 8;
static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

/// Half-open tile coverage. Empty rectangles are valid, including off-map views.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileWindow {
    pub left: u16,
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
}

impl TileWindow {
    pub fn is_empty(self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }
}

pub struct MapView<'a> {
    map: &'a MapDef,
    phases: PhaseMask,
    // One fixed 6 KiB overlay, never one bitmap for every resident/search.
    blocked: [u64; MAX_TILES.div_ceil(64)],
    revision: u64,
    instance_id: u64,
}

impl<'a> MapView<'a> {
    pub fn new(map: &'a MapDef, phases: PhaseMask) -> Self {
        assert!(map.width > 0 && map.width <= MAX_MAP_WIDTH);
        assert!(map.height > 0 && map.height <= MAX_MAP_HEIGHT);
        // Current TileIndex and the compiled byte array use this layout's
        // stride. The larger limits only bound scratch storage, not support
        // for constructing a second map with a different byte-array stride.
        assert_eq!(map.width, crate::map_data::MAP_WIDTH);
        assert_eq!(map.height, crate::map_data::MAP_HEIGHT);
        let instance_id = NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed);
        assert_ne!(instance_id, 0, "map view identity exhausted");
        Self {
            map,
            phases,
            blocked: [0; MAX_TILES.div_ceil(64)],
            revision: 1,
            instance_id,
        }
    }

    pub fn definition(&self) -> &'a MapDef {
        self.map
    }
    pub fn width(&self) -> u16 {
        self.map.width
    }
    pub fn height(&self) -> u16 {
        self.map.height
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }
    pub fn phases(&self) -> PhaseMask {
        self.phases
    }

    fn index(&self, pos: TilePos) -> Option<usize> {
        (pos.x < self.width() && pos.y < self.height())
            .then(|| pos.y as usize * self.width() as usize + pos.x as usize)
    }

    pub fn tile(&self, pos: TilePos) -> Option<TileInfo> {
        let index = self.index(pos)?;
        let mut info = self.map.tile(pos, self.phases)?;
        info.blocked |= self.blocked[index / 64] & (1u64 << (index % 64)) != 0;
        Some(info)
    }

    pub fn can_walk(&self, pos: TilePos) -> bool {
        self.tile(pos).is_some_and(|tile| !tile.blocked)
    }

    /// Resolve an enabled content point to its approach, never to a tabletop
    /// or an interior bed coordinate. This checks geometry/content phase only;
    /// a shop still needs accepted construction, staff and opening hours.
    pub fn poi_approach(&self, id: &str) -> Option<TilePos> {
        let poi = self.map.poi(id)?;
        (self.phases.contains(poi.phase) && self.can_walk(poi.tile)).then_some(poi.tile)
    }

    /// Neither diagonal motion nor stepping out of a blocked origin is legal.
    /// When a facility closes, the world must evacuate occupants BEFORE commit.
    pub fn can_step(&self, from: TilePos, to: TilePos) -> bool {
        from.x.abs_diff(to.x) as u32 + from.y.abs_diff(to.y) as u32 == 1
            && self.can_walk(from)
            && self.can_walk(to)
    }

    /// Administrative terrain commit. Callers must first prove that residents,
    /// held cargo and in-flight edges remain legal. Navigation sees a revision
    /// change and cannot publish a route from the old geometry.
    pub fn set_phases(&mut self, phases: PhaseMask) -> bool {
        if self.phases == phases {
            return false;
        }
        self.phases = phases;
        self.bump_revision();
        true
    }

    /// Removing an overlay never makes a static wall or water tile walkable.
    /// Moving people use reservations, not this topology-changing obstacle map.
    pub fn set_blocked(&mut self, pos: TilePos, blocked: bool) -> bool {
        let Some(index) = self.index(pos) else {
            return false;
        };
        let mask = 1u64 << (index % 64);
        let cell = &mut self.blocked[index / 64];
        let old = *cell;
        if blocked {
            *cell |= mask;
        } else {
            *cell &= !mask;
        }
        if old == *cell {
            return false;
        }
        self.bump_revision();
        true
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }

    /// World-pixel rectangle -> intersecting tiles. No scaling, allocation or
    /// full-map traversal. Negative origins and an entirely off-map view work.
    pub fn visible_tiles(&self, x: i64, y: i64, width: u32, height: u32) -> TileWindow {
        let left = x
            .div_euclid(WORLD_PIXELS_PER_TILE)
            .clamp(0, self.width() as i64) as u16;
        let top = y
            .div_euclid(WORLD_PIXELS_PER_TILE)
            .clamp(0, self.height() as i64) as u16;
        if width == 0 || height == 0 {
            return TileWindow {
                left,
                top,
                right: left,
                bottom: top,
            };
        }
        // ceil((x + width) / 8), without x+width+7 overflow at i64::MAX.
        let end_x = x.saturating_add(width as i64);
        let end_y = y.saturating_add(height as i64);
        let right = (end_x.div_euclid(8) + i64::from(end_x.rem_euclid(8) != 0))
            .clamp(0, self.width() as i64) as u16;
        let bottom = (end_y.div_euclid(8) + i64::from(end_y.rem_euclid(8) != 0))
            .clamp(0, self.height() as i64) as u16;
        TileWindow {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Visit each intersecting chunk once, row-major. Object art bounds must be
    /// indexed into every covered chunk separately; tile anchors alone cannot
    /// decide whether a roof/canopy is visible.
    pub fn for_each_visible_chunk(&self, window: TileWindow, mut visit: impl FnMut(u16, u16)) {
        let right = window.right.min(self.width());
        let bottom = window.bottom.min(self.height());
        if window.left >= right || window.top >= bottom {
            return;
        }
        for cy in window.top / CHUNK_SIDE..bottom.div_ceil(CHUNK_SIDE) {
            for cx in window.left / CHUNK_SIDE..right.div_ceil(CHUNK_SIDE) {
                visit(cx, cy);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_data::WILLOW_VILLAGE;

    #[test]
    fn overlay_cannot_turn_water_into_land_or_walk_diagonally() {
        let mut view = MapView::new(&WILLOW_VILLAGE, PhaseMask::DAY_ONE);
        let water = TilePos { x: 13, y: 33 };
        assert!(!view.can_walk(water));
        assert!(view.set_blocked(water, true));
        assert!(view.set_blocked(water, false));
        assert!(!view.can_walk(water));
        let door = TilePos { x: 12, y: 16 };
        let below = TilePos { x: 12, y: 17 };
        assert!(view.can_step(door, below));
        assert!(!view.can_step(door, TilePos { x: 13, y: 17 }));
        let revision = view.revision();
        assert!(view.set_blocked(below, true));
        assert!(!view.can_step(door, below));
        assert!(view.revision() > revision);
        let revision = view.revision();
        assert!(!view.set_blocked(below, true));
        assert_eq!(view.revision(), revision);
        assert!(view.set_blocked(below, false));
        assert!(view.can_step(door, below));
        assert!(!view.set_blocked(TilePos { x: u16::MAX, y: 0 }, true));
    }

    #[test]
    fn viewport_expands_coverage_without_changing_map_or_scale() {
        let view = MapView::new(&WILLOW_VILLAGE, PhaseMask::ALL);
        let small = view.visible_tiles(80, 96, 160, 100);
        let large = view.visible_tiles(80, 96, 320, 200);
        assert_eq!((small.left, small.top), (large.left, large.top));
        assert_eq!(
            small,
            TileWindow {
                left: 10,
                top: 12,
                right: 30,
                bottom: 25
            }
        );
        assert_eq!(
            large,
            TileWindow {
                left: 10,
                top: 12,
                right: 50,
                bottom: 37
            }
        );
        assert_eq!(view.width(), 192);
        assert_eq!(view.height(), 128);
        let mut chunks = Vec::new();
        view.for_each_visible_chunk(small, |x, y| chunks.push((x, y)));
        assert_eq!(chunks, vec![(0, 0), (1, 0), (0, 1), (1, 1)]);
        assert!(view.visible_tiles(-100, -100, 80, 80).is_empty());
        assert!(
            view.visible_tiles(i64::MAX, i64::MAX, u32::MAX, u32::MAX)
                .is_empty()
        );
        assert!(view.visible_tiles(0, 0, 0, 20).is_empty());
        assert_eq!(
            view.visible_tiles(-1, -1, 2, 2),
            TileWindow {
                left: 0,
                top: 0,
                right: 1,
                bottom: 1
            }
        );
    }

    #[test]
    fn fixed_overlay_does_not_grow_with_time_or_window_size() {
        assert_eq!(std::mem::size_of::<[u64; MAX_TILES.div_ceil(64)]>(), 6144);
        let mut view = MapView::new(&WILLOW_VILLAGE, PhaseMask::DAY_ONE);
        let revision = view.revision();
        assert!(!view.set_phases(PhaseMask::DAY_ONE));
        assert_eq!(view.revision(), revision);
        assert!(view.set_phases(PhaseMask::ALL));
        let mut count = 0;
        view.for_each_visible_chunk(view.visible_tiles(0, 0, u32::MAX, u32::MAX), |_, _| {
            count += 1
        });
        assert_eq!(count, 96);
    }

    #[test]
    fn visible_land_does_not_activate_a_future_service() {
        let mut view = MapView::new(&WILLOW_VILLAGE, PhaseMask::DAY_ONE);
        assert!(view.poi_approach("home_xiaohe.door").is_some());
        assert!(view.can_walk(TilePos::new(76, 112)));
        assert_eq!(view.poi_approach("wetland_observe"), None);
        view.set_phases(PhaseMask::ALL);
        assert_eq!(
            view.poi_approach("wetland_observe"),
            Some(TilePos::new(76, 112))
        );
        assert_eq!(view.poi_approach("nonexistent"), None);
        assert_eq!(view.poi_approach("home_table"), Some(TilePos::new(17, 20)));
        assert!(!view.can_walk(TilePos::new(17, 19)));
        assert!(!view.can_walk(TilePos::new(30, 23)));
    }
}
