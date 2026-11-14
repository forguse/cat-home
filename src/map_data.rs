//! Immutable geometry compiled from the Willow Village blueprint.
//!
//! This module has no desktop, save, or simulation dependencies. Its static
//! collision view does not include live actors, orders, or construction state.

use serde::{Deserialize, Serialize};

pub const MAP_WIDTH: u16 = 192;
pub const MAP_HEIGHT: u16 = 128;
pub const CHUNK_SIDE: u16 = 16;
pub const TILE_WORLD_PIXELS: u16 = 8;
pub const ART_PIXELS_PER_WORLD_PIXEL: u16 = 2;
pub const TILE_COUNT: usize = MAP_WIDTH as usize * MAP_HEIGHT as usize;
pub const CHUNK_COLUMNS: u16 = MAP_WIDTH / CHUNK_SIDE;
pub const CHUNK_ROWS: u16 = MAP_HEIGHT / CHUNK_SIDE;
pub const CHUNK_COUNT: u16 = CHUNK_COLUMNS * CHUNK_ROWS;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(from = "[u16; 2]", into = "[u16; 2]")]
pub struct TilePos {
    pub x: u16,
    pub y: u16,
}

impl From<[u16; 2]> for TilePos {
    fn from([x, y]: [u16; 2]) -> Self {
        Self::new(x, y)
    }
}

impl From<TilePos> for [u16; 2] {
    fn from(pos: TilePos) -> Self {
        [pos.x, pos.y]
    }
}

impl TilePos {
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }

    pub const fn index(self) -> Option<TileIndex> {
        if self.x < MAP_WIDTH && self.y < MAP_HEIGHT {
            Some(TileIndex(self.y as u32 * MAP_WIDTH as u32 + self.x as u32))
        } else {
            None
        }
    }

    pub const fn chunk(self) -> Option<ChunkId> {
        if self.x < MAP_WIDTH && self.y < MAP_HEIGHT {
            Some(ChunkId(
                self.y / CHUNK_SIDE * CHUNK_COLUMNS + self.x / CHUNK_SIDE,
            ))
        } else {
            None
        }
    }

    pub const fn offset(self, dx: i32, dy: i32) -> Option<Self> {
        if self.x >= MAP_WIDTH || self.y >= MAP_HEIGHT {
            return None;
        }
        let Some(x) = (self.x as i32).checked_add(dx) else {
            return None;
        };
        let Some(y) = (self.y as i32).checked_add(dy) else {
            return None;
        };
        if x >= 0 && x < MAP_WIDTH as i32 && y >= 0 && y < MAP_HEIGHT as i32 {
            Some(Self::new(x as u16, y as u16))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TileIndex(pub u32);

impl TileIndex {
    pub const fn position(self) -> Option<TilePos> {
        if self.0 < TILE_COUNT as u32 {
            Some(TilePos::new(
                (self.0 % MAP_WIDTH as u32) as u16,
                (self.0 / MAP_WIDTH as u32) as u16,
            ))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChunkId(pub u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileRect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl TileRect {
    pub const fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { x, y, w, h }
    }

    pub const fn contains(self, tile: TilePos) -> bool {
        tile.x >= self.x && tile.x - self.x < self.w && tile.y >= self.y && tile.y - self.y < self.h
    }
}

/// A half-open rectangle in art texels, including the largest authored roof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhaseId(u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhaseMask(u16);

impl PhaseMask {
    pub const DAY_ONE: Self = Self(1);
    pub const ALL: Self = Self(u16::MAX >> (16 - generated::PHASES.len()));

    pub const fn contains(self, phase: PhaseId) -> bool {
        self.0 & (1 << phase.0) != 0
    }

    pub const fn with(self, phase: PhaseId) -> Self {
        Self(self.0 | (1 << phase.0))
    }

    /// Remove an optional capability while keeping the day-one base layout.
    pub const fn without(self, phase: PhaseId) -> Self {
        Self((self.0 & !(1 << phase.0)) | Self::DAY_ONE.0)
    }

    pub const fn bits(self) -> u16 {
        self.0
    }
}

impl Default for PhaseMask {
    fn default() -> Self {
        Self::DAY_ONE
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Terrain {
    Grass,
    Water,
    Road,
    Bridge,
    Boundary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileInfo {
    pub terrain: Terrain,
    pub blocked: bool,
    pub scenic_obstacle: bool,
    pub region: Option<&'static RegionDef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionDef {
    pub id: &'static str,
    pub rect: TileRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PoiDef {
    pub id: &'static str,
    pub region: &'static str,
    pub tile: TilePos,
    pub purpose: &'static str,
    pub phase: PhaseId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LotDef {
    pub id: &'static str,
    pub rect: TileRect,
    pub access: TilePos,
    pub capability: &'static str,
    pub phase: PhaseId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildingDef {
    pub id: &'static str,
    pub region: &'static str,
    pub footprint: TileRect,
    pub solid: &'static [TileRect],
    pub door: TilePos,
    pub door_poi: &'static str,
    pub beds: u8,
    pub occupants: &'static [&'static str],
    pub phase: PhaseId,
    pub kind: &'static str,
    pub envelope: ArtRect,
    pub material_slot: TilePos,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CropDef {
    pub id: &'static str,
    pub rect: TileRect,
    pub work: TilePos,
    pub phase: PhaseId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResidentDef {
    pub id: &'static str,
    pub home: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteriorDef {
    pub id: &'static str,
    pub building: &'static str,
    pub portal: &'static str,
    pub bounds: TileRect,
    pub threshold: TilePos,
    pub owner: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotPos {
    Outdoor(TilePos),
    Interior {
        interior: &'static str,
        local: TilePos,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotRole {
    Stand,
    Wait,
    Load,
    Prop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Facing {
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    Nw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlotDef {
    pub id: &'static str,
    pub role: SlotRole,
    pub position: SlotPos,
    pub facing: Facing,
    pub capacity: u8,
    pub access_path: &'static [TilePos],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReservationGroupDef {
    pub id: &'static str,
    pub capacity: u8,
    pub members: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SiteDef {
    pub id: &'static str,
    pub phase: PhaseId,
    pub interior: Option<InteriorDef>,
    pub slots: &'static [SlotDef],
    pub groups: &'static [ReservationGroupDef],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundaryDef {
    pub guard_tiles: u16,
    pub corridor: &'static [TilePos],
    pub spawn: TilePos,
    pub reception_poi: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BridgeDef {
    pub id: &'static str,
    pub axis: &'static [TilePos],
    pub initial_width: u8,
    pub full_width: u8,
    pub open_phase: PhaseId,
    pub upgrade: Option<PhaseId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoadDef {
    pub id: &'static str,
    pub axis: &'static [TilePos],
    pub width: u8,
    pub phase: PhaseId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropDef {
    pub id: &'static str,
    pub tile: TilePos,
    pub phase: PhaseId,
    pub reader_poi: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenicKind {
    Tree,
    Shrub,
    Rock,
    Fence,
    Bench,
    Crate,
    Flowers,
    Grass,
    Sign,
    Lamp,
    Reeds,
    Log,
    WoodPile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScenicObject {
    pub id: &'static str,
    pub tile: TilePos,
    pub kind: ScenicKind,
    pub solid: bool,
    pub phase: PhaseId,
}

/// Low ground vegetation only: does not alter walking, ownership or saved geometry.
pub struct GroundCoverDef {
    pub region: &'static str,
    pub center: TilePos,
    pub radius: TilePos,
    pub color: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaterShape {
    Polygon(&'static [TilePos]),
    Pond { center: TilePos, radius: TilePos },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaterDef {
    pub id: &'static str,
    pub shape: WaterShape,
    pub fishing: &'static [TilePos],
}

#[derive(Clone, Copy)]
struct TileRule {
    base: Terrain,
    road: u16,
    bridge: u16,
    solid: u16,
    scenic: u16,
}

pub struct MapDef {
    pub layout_id: &'static str,
    pub map_revision: u16,
    pub source_sha256: &'static str,
    pub width: u16,
    pub height: u16,
    pub seed: u64,
    pub phases: &'static [&'static str],
    pub regions: &'static [RegionDef],
    pub pois: &'static [PoiDef],
    pub lots: &'static [LotDef],
    pub buildings: &'static [BuildingDef],
    pub crops: &'static [CropDef],
    pub residents: &'static [ResidentDef],
    pub sites: &'static [SiteDef],
    pub boundary: BoundaryDef,
    pub roads: &'static [RoadDef],
    pub bridges: &'static [BridgeDef],
    pub solid_props: &'static [PropDef],
    pub scenic: &'static [ScenicObject],
    pub ground_cover: &'static [GroundCoverDef],
    pub water: &'static [WaterDef],
    tiles: &'static [u8; TILE_COUNT * 2],
    rules: &'static [TileRule],
}

impl MapDef {
    pub fn phase(&self, id: &str) -> Option<PhaseId> {
        self.phases
            .iter()
            .position(|&name| name == id)
            .map(|i| PhaseId(i as u8))
    }

    pub fn tile(&self, pos: TilePos, phases: PhaseMask) -> Option<TileInfo> {
        let index = pos.index()?.0 as usize * 2;
        let region = self.tiles[index];
        let rule = self.rules[self.tiles[index + 1] as usize];
        let bits = phases.bits();
        let terrain = if rule.bridge & bits != 0 {
            Terrain::Bridge
        } else if rule.road & bits != 0 && rule.base == Terrain::Grass {
            Terrain::Road
        } else {
            rule.base
        };
        Some(TileInfo {
            terrain,
            blocked: rule.solid & bits != 0
                || matches!(terrain, Terrain::Water | Terrain::Boundary),
            scenic_obstacle: rule.scenic & bits != 0,
            region: self.regions.get(region as usize),
        })
    }

    pub fn poi(&self, id: &str) -> Option<&'static PoiDef> {
        self.pois
            .binary_search_by_key(&id, |def| def.id)
            .ok()
            .map(|i| &self.pois[i])
    }

    pub fn lot(&self, id: &str) -> Option<&'static LotDef> {
        self.lots
            .binary_search_by_key(&id, |def| def.id)
            .ok()
            .map(|i| &self.lots[i])
    }

    pub fn site(&self, id: &str) -> Option<&'static SiteDef> {
        self.sites
            .binary_search_by_key(&id, |def| def.id)
            .ok()
            .map(|i| &self.sites[i])
    }

    pub fn building(&self, id: &str) -> Option<&'static BuildingDef> {
        self.buildings
            .binary_search_by_key(&id, |def| def.id)
            .ok()
            .map(|i| &self.buildings[i])
    }

    pub fn crop(&self, id: &str) -> Option<&'static CropDef> {
        self.crops
            .binary_search_by_key(&id, |def| def.id)
            .ok()
            .map(|i| &self.crops[i])
    }

    pub fn slot(&self, site: &str, id: &str) -> Option<&'static SlotDef> {
        let slots = self.site(site)?.slots;
        slots
            .binary_search_by_key(&id, |def| def.id)
            .ok()
            .map(|i| &slots[i])
    }

    pub const fn packed_tile_bytes(&self) -> usize {
        self.tiles.len()
    }
}

#[path = "map_data_generated.rs"]
mod generated;

pub use generated::{SCENIC_OBJECTS, WILLOW_VILLAGE};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_indices_and_phase_flags_respect_map_bounds() {
        let corner = TilePos::new(191, 127);
        assert_eq!(corner.index(), Some(TileIndex(24_575)));
        assert_eq!(corner.chunk(), Some(ChunkId(95)));
        assert_eq!(corner.index().and_then(TileIndex::position), Some(corner));
        assert_eq!(corner.offset(1, 0), None);
        assert_eq!(TilePos::new(255, 191).index(), None);
        assert_eq!(TilePos::new(192, 127).offset(-1, 0), None);
        assert_eq!(TilePos::new(0, 0).offset(-1, 0), None);
        assert_eq!(serde_json::to_string(&corner).unwrap(), "[191,127]");

        let day_one = WILLOW_VILLAGE.phase("day_1").unwrap();
        assert!(PhaseMask::ALL.without(day_one).contains(day_one));
        assert!(PhaseMask::default().contains(day_one));
        assert!(WILLOW_VILLAGE.phase("unknown").is_none());
    }

    #[test]
    fn compile_time_tiles_keep_water_bridge_and_door_rules() {
        let map = &WILLOW_VILLAGE;
        let day_one = PhaseMask::DAY_ONE;
        let freight = map.phase("main_bridge_freight").unwrap();
        let late = map.phase("late").unwrap();
        let tile = |x, y, phase| map.tile(TilePos::new(x, y), phase).unwrap();

        assert_eq!(tile(62, 42, day_one).terrain, Terrain::Bridge);
        assert_eq!(tile(62, 43, day_one).terrain, Terrain::Water);
        assert_eq!(tile(62, 43, day_one.with(freight)).terrain, Terrain::Bridge);
        assert_eq!(tile(62, 108, day_one).terrain, Terrain::Water);
        assert_eq!(tile(62, 108, day_one.with(late)).terrain, Terrain::Bridge);
        assert_eq!(tile(191, 55, day_one).terrain, Terrain::Boundary);
        assert!(!tile(191, 56, day_one).blocked);
        assert!(tile(12, 15, day_one).blocked);
        assert!(!tile(12, 16, day_one).blocked);
        assert!(tile(21, 18, day_one).blocked);
        assert!(!tile(21, 19, day_one).blocked);
        assert!(tile(17, 19, day_one).blocked);
        assert!(!tile(17, 20, day_one).blocked);
    }

    #[test]
    fn interior_slots_and_outdoor_entrances_have_different_spaces() {
        let map = &WILLOW_VILLAGE;
        let door = map.poi("home_xiaohe.door").unwrap();
        let bedside = map.slot(door.id, "home_xiaohe.door.bedside").unwrap();
        assert_eq!(door.tile, TilePos::new(12, 16));
        assert_eq!(
            bedside.position,
            SlotPos::Interior {
                interior: "home_xiaohe.interior",
                local: TilePos::new(1, 1),
            }
        );
        assert_eq!(
            map.site(door.id).unwrap().interior.unwrap().owner,
            Some("xiaohe")
        );
        let table = map.slot("home_table", "home_table.tabletop").unwrap();
        assert_eq!(table.position, SlotPos::Outdoor(TilePos::new(17, 19)));
        assert_eq!(table.role, SlotRole::Prop);
        assert!(table.access_path.is_empty());
        assert_eq!(map.residents.len(), 20);
        assert_eq!(map.packed_tile_bytes(), 49_152);
    }
}
