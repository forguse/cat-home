//! Village layout bindings and disposable route-search workspaces.

use super::*;
use std::sync::Mutex;

pub(super) const BUILDING_IDS: [&str; 5] = [
    "home_xiaohe",
    "home_ayu",
    "home_kitchen",
    "home_pantry",
    "home_workshop",
];
pub(super) const DEVELOPMENT_POIS: [&str; 8] = [
    "home_gate",
    "bridge_wait_w",
    "market_gate",
    "market_notice",
    "craft_inn.door",
    "market_loading",
    "market_well",
    "commons_garden",
];
pub(super) const TREE_COUNT: usize = 16;
pub(super) const VILLAGE_CAT_SPOTS: usize = 6;
pub(super) const COMPATIBLE_HOME_ORIGINS: [[u8; 2]; 3] = [[6, 4], [35, 32], [18, 5]];

pub(super) fn is_false(value: &bool) -> bool {
    !value
}
pub(super) fn is_zero(value: &u8) -> bool {
    *value == 0
}

pub(super) fn read_plots<'de, D: serde::Deserializer<'de>>(
    input: D,
) -> Result<Vec<Plot>, D::Error> {
    struct PlotList;
    impl<'de> serde::de::Visitor<'de> for PlotList {
        type Value = Vec<Plot>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("at most thirty plot records")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Vec<Plot>, A::Error> {
            let mut plots = Vec::new();
            while let Some(plot) = seq.next_element()? {
                if plots.len() == 30 {
                    return Err(serde::de::Error::invalid_length(31, &self));
                }
                plots.push(plot);
            }
            Ok(plots)
        }
    }
    input.deserialize_seq(PlotList)
}

#[derive(Default, Clone, Debug)]
pub(super) struct Occupancy(Vec<TilePos>);
impl PartialEq for Occupancy {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}
impl Occupancy {
    pub fn set(&mut self, list: &[TilePos]) {
        self.0.clear();
        self.0.extend(list.iter().copied().take(64));
    }
    pub fn contains(&self, tile: TilePos) -> bool {
        self.0.contains(&tile)
    }
    pub fn iter(&self) -> impl Iterator<Item = TilePos> + '_ {
        self.0.iter().copied()
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ReservedPlot(pub Option<u8>);

impl PartialEq for ReservedPlot {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct SupplyRequests {
    pub market_crops: Option<u16>,
    pub neighbour_meals: bool,
}

impl PartialEq for SupplyRequests {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub revision: u16,
    #[serde(default, skip_serializing_if = "is_false")]
    pub imported_pedestrian_bridge: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub freight_inspected: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub freight_material_paid: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub freight_shifts: u8,
    pub region_flags: u16,
    pub tree_due: [u64; TREE_COUNT],
    #[serde(default = "initial_orchard")]
    pub orchard: [OrchardTree; 4],
    #[serde(default)]
    pub flooded_plots: u32,
    #[serde(default)]
    pub awake_ms: [[u32; 2]; 7],
    #[serde(default)]
    pub home_ms: [[u32; 2]; 7],
    #[serde(default)]
    pub tracked_day: Option<u64>,
    #[serde(default)]
    pub opening: Opening,
    #[serde(default)]
    pub grain: grain::State,
    #[serde(default)]
    pub soil: soil::State,
    #[serde(default)]
    pub workshop_story: workshop_story::State,
    #[serde(default)]
    pub pantry_plan: pantry_plan::State,
    #[serde(default)]
    pub preserves: preserves::State,
    #[serde(default)]
    pub renovation: renovation::State,
    #[serde(default)]
    pub road_story: road_story::State,
    #[serde(default)]
    pub market_story: road_story::Market,
    #[serde(default)]
    pub social: crate::residents::SocialProgress,
    #[serde(default)]
    pub memories: memories::State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar: Option<town_calendar::TownCalendar>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub cat_bowl_dirty: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub(super) struct Opening {
    pub boxes_stowed: u8,
    pub slept_at_home: u8,
    pub rod_on_rack: bool,
    pub rod_borrowed: bool,
    pub rod_used: bool,
    pub rod_returned: bool,
    pub cat_food: bool,
    pub cat_ate: bool,
    pub cat_visited: u8,
    pub sown: u32,
    pub harvest_carrier: Option<u8>,
    pub shared_until: u64,
    pub shared_serving: bool,
    pub shared_eaten: u8,
    pub kitchen_shift: Option<ShiftEvidence>,
    pub field_shift: Option<ShiftEvidence>,
    pub parallel_work: bool,
    pub late_meal_until: u64,
    pub late_meal_eaten: bool,
    pub rest_completed: [u64; 2],
    pub rest_water: u8,
    pub shared_rest: bool,
    pub review_day: Option<u64>,
    pub daily_needs: u8,
    pub stable_days: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ShiftEvidence {
    pub worker: u8,
    pub start: u64,
    pub end: u64,
}

impl Opening {
    pub fn completed(&self, chapter: &str) -> bool {
        let homes = self.boxes_stowed == 0b11 && self.slept_at_home == 0b11;
        match chapter {
            "Q01" => homes,
            "Q02" => homes && self.rod_returned,
            "Q03" => homes && self.rod_returned && self.shared_eaten == 0b11,
            "Q04" => homes && self.cat_ate && self.cat_visited == 0b11,
            "Q05" => self.completed("Q03") && self.parallel_work,
            "Q06" => self.completed("Q05") && self.late_meal_eaten,
            "Q07" => self.completed("Q05") && self.completed("Q04") && self.shared_rest,
            "Q08" => self.completed("Q06") && self.completed("Q07") && self.stable_days == 7,
            _ => false,
        }
    }

    pub fn validate(&self, now: u64) -> bool {
        self.boxes_stowed <= 0b11
            && self.slept_at_home & !self.boxes_stowed == 0
            && self.cat_visited <= 0b11
            && (!self.rod_returned || self.rod_used)
            && (!self.rod_used || self.rod_borrowed)
            && (!self.rod_borrowed || self.rod_on_rack)
            && (!self.rod_on_rack || self.completed("Q01"))
            && (!(self.cat_ate || self.cat_food || self.cat_visited != 0) || self.completed("Q01"))
            && self.sown >> 30 == 0
            && self
                .harvest_carrier
                .is_none_or(|actor| actor < 2 && self.completed("Q02"))
            && self.shared_until <= now.saturating_add(DAY_MS)
            && (self.shared_eaten == 0 || self.shared_serving || self.shared_eaten == 0b11)
            && self.shared_eaten <= 0b11
            && (!self.shared_serving || self.shared_until != 0 && self.completed("Q02"))
            && [self.kitchen_shift, self.field_shift]
                .into_iter()
                .flatten()
                .all(|shift| {
                    shift.worker < 2
                        && shift.start < shift.end
                        && shift.end <= now
                        && shift.end - shift.start <= HOUR
                })
            && (!self.parallel_work || self.shifts_overlap() && self.completed("Q03"))
            && (self.late_meal_until == 0
                || self.late_meal_until >= now
                    && self.late_meal_until <= now.saturating_add(3 * HOUR)
                    && self.completed("Q05")
                    && !self.late_meal_eaten)
            && (!self.late_meal_eaten || self.completed("Q05"))
            && self.rest_water <= 0b11
            && self
                .rest_completed
                .into_iter()
                .enumerate()
                .all(|(worker, time)| {
                    time <= now && (self.rest_water & (1 << worker) == 0 || time != 0)
                })
            && (!self.shared_rest
                || self.rest_water == 0b11
                    && self.breaks_grouped()
                    && self.completed("Q04")
                    && self.completed("Q05"))
            && self.stable_days <= 7
            && self.daily_needs <= 0b11_1111
            && self.review_day.is_none_or(|day| {
                day <= now / DAY_MS && self.completed("Q06") && self.completed("Q07")
            })
    }

    pub fn work_finished(&mut self, kitchen: bool, worker: u8, start: u64, end: u64) {
        if self.parallel_work || !self.completed("Q03") || worker >= 2 || end <= start {
            return;
        }
        let shift = Some(ShiftEvidence { worker, start, end });
        if kitchen {
            self.kitchen_shift = shift;
        } else {
            self.field_shift = shift;
        }
        self.parallel_work = self.shifts_overlap();
    }

    fn shifts_overlap(&self) -> bool {
        matches!((self.kitchen_shift, self.field_shift), (Some(kitchen), Some(field))
            if kitchen.worker != field.worker && kitchen.start.max(field.start) < kitchen.end.min(field.end))
    }

    pub fn note_break(&mut self, worker: usize, time: u64) {
        if worker < 2 && !self.shared_rest {
            self.rest_completed[worker] = time;
            self.rest_water &= !(1 << worker);
        }
    }

    pub fn needs_break_water(&self, worker: usize, now: u64) -> bool {
        let time = self.rest_completed[worker];
        !self.shared_rest
            && time > 0
            && now.saturating_sub(time) <= HOUR
            && self.rest_water & (1 << worker) == 0
    }

    pub fn consumed_break_water(&mut self, worker: usize, now: u64) {
        if self.needs_break_water(worker, now) {
            self.rest_water |= 1 << worker;
            self.shared_rest = self.rest_water == 0b11 && self.breaks_grouped();
        }
    }

    fn breaks_grouped(&self) -> bool {
        let [a, b] = self.rest_completed;
        a > 0 && b > 0 && a.abs_diff(b) <= 26 * MINUTE
    }

    pub fn review_day(&mut self, day: u64) {
        if self.stable_days == 7 || !self.completed("Q06") || !self.completed("Q07") {
            return;
        }
        if let Some(previous) = self.review_day {
            if previous == day {
                return;
            }
            self.stable_days = if previous + 1 == day && self.daily_needs == 0b11_1111 {
                self.stable_days + 1
            } else {
                0
            };
        }
        self.daily_needs = 0;
        self.review_day = Some(day);
    }

    pub fn note_need(&mut self, worker: usize, need: u8) {
        if self.review_day.is_some() && worker < 2 && self.stable_days < 7 {
            self.daily_needs |= 1 << (worker * 3 + usize::from(need));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OrchardTree {
    pub phase: OrchardPhase,
    pub age_days: u16,
    pub health: u8,
    pub fruit: u8,
    pub last_care_day: u64,
}

fn initial_orchard() -> [OrchardTree; 4] {
    std::array::from_fn(|index| OrchardTree {
        phase: if index == 3 {
            OrchardPhase::Sapling
        } else {
            OrchardPhase::Dormant
        },
        age_days: if index == 3 { 5 } else { 180 },
        health: 80,
        fruit: 0,
        last_care_day: 0,
    })
}

impl State {
    pub fn new() -> Self {
        Self {
            revision: WILLOW_VILLAGE.map_revision,
            imported_pedestrian_bridge: false,
            freight_inspected: false,
            freight_material_paid: false,
            freight_shifts: 0,
            region_flags: PhaseMask::DAY_ONE.bits(),
            tree_due: [0; TREE_COUNT],
            orchard: initial_orchard(),
            flooded_plots: 0,
            awake_ms: [[0; 2]; 7],
            home_ms: [[0; 2]; 7],
            tracked_day: None,
            opening: Opening::default(),
            grain: grain::State::default(),
            soil: soil::State::default(),
            workshop_story: workshop_story::State::default(),
            pantry_plan: pantry_plan::State::default(),
            preserves: preserves::State::default(),
            renovation: renovation::State::default(),
            road_story: road_story::State::default(),
            market_story: road_story::Market::default(),
            social: crate::residents::SocialProgress::default(),
            memories: memories::State::default(),
            calendar: None,
            cat_bowl_dirty: false,
        }
    }

    pub fn phases(&self) -> PhaseMask {
        WILLOW_VILLAGE
            .phases
            .iter()
            .enumerate()
            .fold(PhaseMask::DAY_ONE, |mask, (index, id)| {
                if self.region_flags & (1 << index) != 0 {
                    mask.with(WILLOW_VILLAGE.phase(id).expect("compiled village phase"))
                } else {
                    mask
                }
            })
    }

    pub fn grant(&mut self, id: &str) {
        self.region_flags = self
            .phases()
            .with(WILLOW_VILLAGE.phase(id).expect("compiled village phase"))
            .bits();
    }

    pub fn apply_development(&mut self, finished: u8, importing: bool) {
        if finished >= 3 {
            self.grant("farm_expansion");
        }
        if finished >= 6 {
            if importing {
                self.imported_pedestrian_bridge = true;
            } else if !self.imported_pedestrian_bridge {
                self.freight_inspected = true;
            }
        }
        if self.freight_inspected {
            self.grant("main_bridge_freight");
        }
        if importing && finished >= 9 {
            self.grant("weekly_market");
        }
        if finished >= 12
            && self
                .phases()
                .contains(WILLOW_VILLAGE.phase("weekly_market").unwrap())
        {
            self.grant("permanent_market");
            // The paid market street opens shop shells. Residence and inn
            // occupancy still depend on the later staff/bed authorities.
            self.grant("village_development");
        }
        if finished >= 15 {
            self.grant("resident_arrival");
        }
        if finished >= 18 {
            self.grant("cooperative_farm");
        }
        if finished >= 21 {
            self.grant("community");
        }
        if finished >= 24 {
            self.grant("late");
        }
    }

    pub fn advance_orchard(&mut self, day: u64) {
        for (index, tree) in self.orchard.iter_mut().enumerate() {
            tree.age_days = tree.age_days.saturating_add(1);
            if day > tree.last_care_day + 25 && day % 7 == 0 {
                tree.health = tree.health.saturating_sub(1).max(30);
            }
            let phase = if tree.age_days < 28 {
                OrchardPhase::Sapling
            } else {
                match (day + index as u64 * 3) % 112 {
                    5..=14 => OrchardPhase::Bud,
                    15..=27 => OrchardPhase::Flower,
                    28..=48 => OrchardPhase::GreenFruit,
                    49..=76 => {
                        if tree.phase == OrchardPhase::AfterHarvest {
                            OrchardPhase::AfterHarvest
                        } else {
                            OrchardPhase::Ripe
                        }
                    }
                    77..=83 => OrchardPhase::AfterHarvest,
                    _ => OrchardPhase::Dormant,
                }
            };
            if phase == OrchardPhase::Ripe && tree.phase != phase {
                tree.fruit = (tree.health / 15).clamp(2, 7);
            }
            if phase != OrchardPhase::Ripe {
                tree.fruit = 0;
            }
            tree.phase = phase;
        }
    }
}

pub(super) fn poi(id: &str) -> Cell {
    Cell::from(WILLOW_VILLAGE.poi(id).expect("compiled village point").tile)
}

pub(super) fn building(index: usize) -> Cell {
    Cell::from(
        WILLOW_VILLAGE
            .building(BUILDING_IDS[index])
            .expect("compiled village building")
            .door,
    )
}

pub(super) fn slot(site: &str, id: &str) -> Cell {
    let slot = WILLOW_VILLAGE
        .slot(site, id)
        .expect("compiled village slot");
    let crate::map_data::SlotPos::Outdoor(pos) = slot.position else {
        panic!("household route requested an indoor slot as a tile");
    };
    Cell::from(pos)
}

/// Stable first-to-last tree identities. Growth mutates a slot, never the set.
/// The offset grid hangs off the forest loading point and keeps its roads clear.
pub(super) fn tree_cells() -> [Cell; TREE_COUNT] {
    let loading = poi("forest_loading");
    let orchard = poi("orchard_tools");
    std::array::from_fn(|i| {
        if let Some(&(dx, dy)) = [(-5, 1), (1, 1), (2, 6), (-6, 7)].get(i.wrapping_sub(8)) {
            return Cell(
                (i32::from(orchard.0) + dx) as u16,
                (i32::from(orchard.1) + dy) as u16,
            );
        }
        let x = loading.0 - 24 + (i % 4) as u16 * 5;
        let y = loading.1 - 8 + (i / 4) as u16 * 5;
        Cell(x, y)
    })
}

pub(super) fn cat_spot(index: usize) -> Cell {
    match index {
        0 => poi("cat_rest"),
        1 => poi("home_fishing"),
        2 => poi("cat_water"),
        _ => poi("home_table"),
    }
}

#[derive(Default)]
pub(super) struct Navigation(Mutex<Option<Search>>);

impl Clone for Navigation {
    fn clone(&self) -> Self {
        Self::default()
    }
}
impl PartialEq for Navigation {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}
impl fmt::Debug for Navigation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Navigation(search workspace)")
    }
}

struct Search {
    view: MapView<'static>,
    nav: MapNav,
    static_revision: u32,
    key: Option<(usize, u32, Cell, Cell, bool, bool)>,
}

pub(super) enum RouteSegment {
    Ready(Vec<Cell>),
    Pending,
    Blocked,
}

impl Navigation {
    pub fn pump(
        &self,
        world: &LivingWorld,
        actor: usize,
        start: Cell,
        target: Cell,
        avoid_occupied: bool,
        budget: usize,
    ) -> RouteSegment {
        let mut saved = self.0.lock().expect("village search workspace");
        let revision = world.terrain_revision();
        let search = saved.get_or_insert_with(|| Search {
            view: world.village_map_view(),
            nav: MapNav::new(),
            static_revision: revision,
            key: None,
        });
        // Someone fetching water owns the stand, so other trips pass around
        // it. The user's route can enter and an actor already there can leave.
        let well = LivingWorld::well_draw_stand();
        let route_around_well = start != well && target != well && world.well_stand_reserved(actor);
        let key = (actor, revision, start, target, avoid_occupied, route_around_well);
        let result = if search.key != Some(key) {
            if avoid_occupied
                || search.static_revision != revision
                || search.key.is_some_and(|previous| previous.4 || previous.5)
            {
                search.view = world.village_map_view();
                search.static_revision = revision;
            }
            if avoid_occupied {
                world.overlay_occupied(actor, start, target, &mut search.view);
            }
            if route_around_well {
                search.view.set_blocked(well.tile(), true);
            }
            search.key = Some(key);
            search.nav.begin(&search.view, start.tile(), target.tile())
        } else {
            SearchResult::Pending
        };
        let result = match result {
            SearchResult::Pending => search.nav.advance(&search.view, budget),
            settled => settled,
        };
        match result {
            SearchResult::Found(segment) => {
                search.key = None;
                if avoid_occupied || route_around_well {
                    search.view = world.village_map_view();
                }
                RouteSegment::Ready(segment.steps().iter().copied().map(Cell::from).collect())
            }
            SearchResult::Pending => RouteSegment::Pending,
            _ => {
                search.key = None;
                if avoid_occupied || route_around_well {
                    search.view = world.village_map_view();
                }
                RouteSegment::Blocked
            }
        }
    }
}
