//! Bounded work and custody for the existing village scenery catalog.
use super::{Activity, Item, Tool, MINUTE};
use crate::map_data::{ScenicKind, TILE_COUNT, TilePos, WILLOW_VILLAGE};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const MAX_OBJECTS: usize = 4_096;
const MAX_UNFINISHED: usize = 24;
pub(super) const FIBRE_CAP: u8 = 36;
pub(super) const FIBRE_PER_ROPE: u8 = 3;
const NO_OBJECT: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenicStage {
    Standing,
    Cut,
    Stump,
    Cleared,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScenicResourceView {
    pub stage: ScenicStage,
    pub work_done: u8,
    pub work_total: u8,
    pub pending: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Product {
    Stock(Item),
    Fibre,
}

#[derive(Clone, Copy)]
pub(super) struct Recipe {
    pub tool: Tool,
    pub initial: Activity,
    pub product: Product,
    pub amount: u8,
    pub shifts: u8,
    pub roots: u8,
}

pub(super) fn recipe(id: usize) -> Option<Recipe> {
    if !catalog().harvestable.contains(id) {
        return None;
    }
    let (tool, initial, product, amount, shifts, roots) = match WILLOW_VILLAGE.scenic.get(id)?.kind {
        ScenicKind::Tree => (Tool::Axe, Activity::FellTree, Product::Stock(Item::Wood), 3, 3, 2),
        ScenicKind::Log => (Tool::Axe, Activity::FellTree, Product::Stock(Item::Wood), 2, 1, 0),
        ScenicKind::Rock => (Tool::Hammer, Activity::MineScenic, Product::Stock(Item::Stone), 3, 2, 0),
        ScenicKind::Shrub => (Tool::Hoe, Activity::ClearScenic, Product::Stock(Item::Wood), 1, 2, 0),
        ScenicKind::Grass | ScenicKind::Reeds => (Tool::Hoe, Activity::ClearScenic, Product::Fibre, 2, 1, 0),
        _ => return None,
    };
    Some(Recipe { tool, initial, product, amount, shifts, roots })
}

struct Catalog {
    by_tile: Box<[u16]>,
    harvestable: ClearedScenery,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        assert!(WILLOW_VILLAGE.scenic.len() <= MAX_OBJECTS, "scenery identity exceeds saved bitset");
        let mut by_tile = vec![NO_OBJECT; TILE_COUNT].into_boxed_slice();
        let mut harvestable = ClearedScenery::default();
        for (id, object) in WILLOW_VILLAGE.scenic.iter().enumerate() {
            if super::land_reuse::harvestable_scenic(object) {
                by_tile[object.tile.index().expect("compiled scenery tile").0 as usize] = id as u16;
                harvestable.insert(id);
            }
        }
        Catalog { by_tile, harvestable }
    })
}

pub(super) fn object_at(tile: TilePos) -> Option<u16> {
    let id = catalog().by_tile[tile.index()?.0 as usize];
    (id != NO_OBJECT).then_some(id)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum WorkKind {
    Cut,
    Gather,
    Roots,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct Work {
    pub id: u16,
    pub stand: TilePos,
    pub kind: WorkKind,
}

impl Work {
    pub fn valid(self) -> bool {
        let id = usize::from(self.id);
        recipe(id).is_some()
            && !WILLOW_VILLAGE.scenic[id].temporary_ground
            && self.stand.index().is_some()
            && super::land_reuse::work_foot_unreserved(self.stand)
            && distance(self.stand, WILLOW_VILLAGE.scenic[id].tile) == 1
    }

    pub fn tool(self) -> Option<Tool> {
        match self.kind {
            WorkKind::Cut => recipe(usize::from(self.id)).map(|record| record.tool),
            WorkKind::Roots => Some(Tool::Hoe),
            WorkKind::Gather => None,
        }
    }

    pub fn activity(self) -> Activity {
        match self.kind {
            WorkKind::Cut => recipe(usize::from(self.id)).map_or(Activity::ClearScenic, |record| record.initial),
            WorkKind::Roots => Activity::ClearScenic,
            WorkKind::Gather => Activity::GatherScenic,
        }
    }

    pub fn duration(self) -> u64 {
        match self.kind {
            WorkKind::Cut => 8 * MINUTE,
            WorkKind::Roots => 6 * MINUTE,
            WorkKind::Gather => 3 * MINUTE,
        }
    }
}

pub(super) fn distance(a: TilePos, b: TilePos) -> u16 {
    a.x.abs_diff(b.x) + a.y.abs_diff(b.y)
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct ClearedScenery([[u64; 32]; 2]);

impl ClearedScenery {
    fn contains(&self, id: usize) -> bool {
        id < MAX_OBJECTS && self.0[id / 2048][(id / 64) % 32] & (1 << (id % 64)) != 0
    }

    fn insert(&mut self, id: usize) {
        self.0[id / 2048][(id / 64) % 32] |= 1 << (id % 64);
    }

    fn is_empty(&self) -> bool {
        self.0.iter().flatten().all(|&word| word == 0)
    }

    pub(crate) fn clears_tile(&self, tile: TilePos) -> bool {
        object_at(tile).is_some_and(|id| self.contains(usize::from(id)))
    }

    fn ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().flatten().copied().enumerate().flat_map(|(i, mut word)| {
            std::iter::from_fn(move || {
                if word == 0 { return None; }
                let bit = word.trailing_zeros();
                word &= word - 1;
                Some(i * 64 + bit as usize)
            })
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Progress {
    id: u16,
    ticks: u8,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    #[serde(skip_serializing_if = "ClearedScenery::is_empty")]
    pub cleared: ClearedScenery,
    #[serde(skip_serializing_if = "Vec::is_empty", deserialize_with = "read_progress")]
    unfinished: Vec<Progress>,
    pub fibre_stock: u8,
    pub fibre_held: [u8; 2],
    pub solid_cleared: u16,
    pub restored_harvests: u8,
    /// Paid cut/clearing slices on one world day. Pending loads do not vanish
    /// at day change and may still be taken home after this allowance is used.
    day: u64,
    shifts: u8,
}

const SHIFTS_PER_DAY: u8 = 3;

fn read_progress<'de, D: serde::Deserializer<'de>>(input: D) -> Result<Vec<Progress>, D::Error> {
    struct Entries;
    impl<'de> serde::de::Visitor<'de> for Entries {
        type Value = Vec<Progress>;
        fn expecting(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(out, "at most {MAX_UNFINISHED} incomplete scenery jobs")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::new();
            while let Some(entry) = input.next_element()? {
                if entries.len() == MAX_UNFINISHED {
                    return Err(serde::de::Error::invalid_length(entries.len() + 1, &self));
                }
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    input.deserialize_seq(Entries)
}

impl State {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn find(&self, id: u16) -> Result<usize, usize> {
        self.unfinished.binary_search_by_key(&id, |entry| entry.id)
    }

    fn ticks(&self, id: u16) -> u8 {
        self.find(id).map_or(0, |index| self.unfinished[index].ticks)
    }

    pub fn permits_shift(&self, day: u64) -> bool {
        self.day != day || self.shifts < SHIFTS_PER_DAY
    }

    pub fn fibre_total(&self) -> u8 {
        self.fibre_stock.saturating_add(self.fibre_held[0]).saturating_add(self.fibre_held[1])
    }

    pub fn view(&self, id: usize) -> Option<ScenicResourceView> {
        let rule = recipe(id)?;
        let ticks = self.ticks(id as u16);
        let (stage, work_done, work_total, pending) = if self.cleared.contains(id) {
            (ScenicStage::Cleared, 0, 0, 0)
        } else if ticks < rule.shifts {
            (ScenicStage::Standing, ticks, rule.shifts, 0)
        } else if ticks == rule.shifts {
            (ScenicStage::Cut, 0, 0, rule.amount)
        } else {
            (ScenicStage::Stump, ticks - rule.shifts - 1, rule.roots, 0)
        };
        Some(ScenicResourceView { stage, work_done, work_total, pending })
    }

    pub fn unfinished_ids(&self) -> impl Iterator<Item = u16> + '_ {
        self.unfinished.iter().map(|entry| entry.id)
    }

    pub fn next(&self, id: u16, day: u64) -> Option<WorkKind> {
        if WILLOW_VILLAGE.scenic.get(usize::from(id))?.temporary_ground {
            return None;
        }
        let view = self.view(usize::from(id))?;
        match view.stage {
            ScenicStage::Standing if self.permits_shift(day)
                && (view.work_done > 0 || self.unfinished.len() < MAX_UNFINISHED) => Some(WorkKind::Cut),
            ScenicStage::Cut => Some(WorkKind::Gather),
            ScenicStage::Stump if self.permits_shift(day) => Some(WorkKind::Roots),
            _ => None,
        }
    }

    /// A real field or foundation shift incorporates prior soft ground
    /// vegetation into the soil. These slots never own a loose fibre bundle.
    pub fn grade(&mut self, rect: crate::map_data::TileRect) {
        for y in rect.y..rect.y + rect.h {
            for x in rect.x..rect.x + rect.w {
                if let Some(id) = object_at(TilePos::new(x, y)).map(usize::from)
                    && WILLOW_VILLAGE.scenic[id].temporary_ground
                {
                    self.cleared.insert(id);
                }
            }
        }
    }

    /// Called once per completed physical shift. The caller takes the whole
    /// pending output in the same transaction as the Gather transition.
    pub fn commit(&mut self, work: Work, day: u64) -> bool {
        if self.next(work.id, day) != Some(work.kind) {
            return false;
        }
        let id = usize::from(work.id);
        let rule = recipe(id).expect("validated natural scenery");
        let ticks = self.ticks(work.id) + 1;
        if work.kind != WorkKind::Gather {
            if self.day != day {
                self.day = day;
                self.shifts = 0;
            }
            self.shifts += 1;
        }
        let index = self.find(work.id);
        if ticks == rule.shifts + 1 + rule.roots {
            self.unfinished.remove(index.expect("a cleared resource first had pending material"));
            self.cleared.insert(id);
            self.solid_cleared += u16::from(WILLOW_VILLAGE.scenic[id].solid);
        } else if let Ok(index) = index {
            self.unfinished[index].ticks = ticks;
        } else {
            self.unfinished.insert(index.unwrap_err(), Progress { id: work.id, ticks });
        }
        true
    }

    pub fn valid(&self, today: u64) -> bool {
        self.day <= today && self.shifts <= SHIFTS_PER_DAY
            && self.restored_harvests >> super::land_reuse::RESTORED_FIELDS.len() == 0
            && self.fibre_total() <= FIBRE_CAP && self.fibre_held.iter().all(|&held| held <= FIBRE_PER_ROPE)
            && self.unfinished.len() <= MAX_UNFINISHED
            && self.unfinished.windows(2).all(|pair| pair[0].id < pair[1].id)
            && self.unfinished.iter().all(|entry| {
                let id = usize::from(entry.id);
                !self.cleared.contains(id) && entry.ticks > 0
                    && recipe(id).is_some_and(|rule| entry.ticks < rule.shifts + 1 + rule.roots)
                    && !WILLOW_VILLAGE.scenic[id].temporary_ground
            })
            && self.cleared.ids().all(|id| recipe(id).is_some())
            && usize::from(self.solid_cleared)
                == self.cleared.ids().filter(|&id| WILLOW_VILLAGE.scenic[id].solid).count()
    }
}
