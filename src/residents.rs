//! Persistent village schedules, finite merchant stock and household outings.

use crate::{
    companionship,
    living_world::{
        ActorKind, CommunityAction, DAY_MS, Fixture, Ground, Item, LivingWorld, Point, Pose,
        TILE_PX, TownEra, YardWeather,
        movement::{Pace, WalkConditions, edge_duration, edge_fraction, valid_travel_edge},
    },
    map_data::{PhaseMask, SlotPos, TilePos, WILLOW_VILLAGE},
    map_nav::{MAX_SEGMENT_EDGES, MapNav, SearchResult},
    map_view::MapView,
    visitors::{
        MAX_HUMAN_VISITORS, Profession, VisitorHeld, VisitorPose, VisitorState, VisitorTool,
        VisitorVisual,
    },
};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const MINUTE: u64 = 60_000;
const HOUR: u64 = 60 * MINUTE;
const STEP: u64 = 10_000;
const RETRY: u64 = 3 * STEP;
const WORK: u64 = 20 * MINUTE;
const REPLAN_DELAY_MAX: u64 = 5 * MINUTE;
const MARKET_TRAVEL_WINDOW: u64 = 175 * MINUTE;
const REMOTE_MARKET_LEAD: u64 = 105 * MINUTE;
const LOCAL_MARKET_LEAD: u64 = 15 * MINUTE;
const HARD_STOCK_CAP: u16 = 64;
const NEIGHBOUR_MEAL_PORTIONS: u16 = 3;
const NEIGHBOUR_PANTRY_TARGET: u16 = 10;
const RESIDENT_COUNT: usize = 20;
const FIRST_RESIDENTS: [usize; 6] = [1, 2, 3, 5, 6, 9];
const EXTERNAL_COUNT: usize = MAX_HUMAN_VISITORS;
pub const VILLAGE_OCCUPANCY: usize = (RESIDENT_COUNT + EXTERNAL_COUNT) * 2;
const HOMEBOUND: [ActorKind; 3] = [ActorKind::Xiaohe, ActorKind::Ayu, ActorKind::Orange];
#[cfg(test)]
const ITEMS: [Item; 8] = [
    Item::Seeds,
    Item::Water,
    Item::Crops,
    Item::Food,
    Item::Wood,
    Item::Stone,
    Item::Rope,
    Item::Fish,
];
const QUEST_START: [u16; RESIDENT_COUNT] = [
    360, 390, 400, 340, 330, 420, 370, 405, 450, 390, 350, 440, 390, 430, 325, 420, 375, 345, 420,
    390,
];
const QUEST_END: [u16; RESIDENT_COUNT] = [
    1110, 1100, 1100, 1130, 1070, 1160, 1095, 1120, 1180, 1085, 1090, 1140, 1080, 1115, 1060, 1125,
    1090, 1150, 1140, 1110,
];
const CATALOG: &str = include_str!("../assets/maps/village-activities.tsv");
const NATIVE_EVENTS: &[u16] = &[
    1, 5, 7, 8, 9, 10, 11, 12, 37, 38, 45, 49, 50, 51, 54, 55, 56, 58, 59, 63, 64, 66, 67, 68, 74,
    75, 76, 77, 78, 79, 80, 81, 82, 83,
];

fn item_index(item: Item) -> usize {
    item as usize
}

fn point(tile: TilePos) -> Point {
    Point::at_tile(i32::from(tile.x), i32::from(tile.y))
}

fn distance(first: TilePos, second: TilePos) -> u32 {
    u32::from(first.x.abs_diff(second.x)) + u32::from(first.y.abs_diff(second.y))
}

fn outer_corridor(tile: TilePos) -> bool {
    WILLOW_VILLAGE.boundary.corridor.windows(2).any(|pair| {
        (pair[0].x.min(pair[1].x)..=pair[0].x.max(pair[1].x)).contains(&tile.x)
            && (pair[0].y.min(pair[1].y)..=pair[0].y.max(pair[1].y)).contains(&tile.y)
    })
}

fn main_bridge_deck(tile: TilePos) -> bool {
    static DECK: OnceLock<(TilePos, TilePos, u16)> = OnceLock::new();
    let (a, b, width) = *DECK.get_or_init(|| {
        let road = WILLOW_VILLAGE
            .roads
            .iter()
            .find(|r| r.id == "bridge_main")
            .expect("main bridge");
        (road.axis[0], road.axis[1], u16::from(road.width))
    });
    (a.x.min(b.x)..=a.x.max(b.x)).contains(&tile.x) && (a.y..a.y + width).contains(&tile.y)
}

fn pantry_service_zone(tile: TilePos) -> bool {
    (33..=36).contains(&tile.x) && (22..=24).contains(&tile.y)
}

fn phase_active(mask: PhaseMask, id: &str) -> bool {
    WILLOW_VILLAGE
        .phase(id)
        .is_some_and(|phase| mask.contains(phase))
}

fn settlement_era(index: usize) -> TownEra {
    if FIRST_RESIDENTS.contains(&index) {
        return TownEra::Humble;
    }
    match index {
        0 | 4 | 7 | 8 => TownEra::Workshop,
        10 | 12 | 13 | 14 | 16 => TownEra::SecondBlock,
        _ => TownEra::PublicDistrict,
    }
}

fn visitor_cap(world: &LivingWorld) -> u8 {
    match world.town_era() {
        TownEra::Humble => 2,
        TownEra::Workshop => 3,
        TownEra::Market => 4,
        TownEra::MarketStreet => 5,
        TownEra::SecondBlock => 6,
        TownEra::PublicDistrict => 7,
        TownEra::Mature | TownEra::Heritage => MAX_HUMAN_VISITORS as u8,
    }
}

fn farmer_production_site(mask: PhaseMask) -> &'static str {
    if phase_active(mask, "cooperative_farm") {
        "south_grain"
    } else {
        "orchard_tools"
    }
}

fn travel_conditions(world: &LivingWorld, loaded: bool) -> WalkConditions {
    WalkConditions {
        loaded,
        tired: false,
        rough_weather: matches!(world.weather(), YardWeather::Rain | YardWeather::Snow),
    }
}

fn personal_tool(id: usize) -> Option<VisitorTool> {
    use VisitorTool::*;
    Some(match id {
        2 | 9 | 12 => Hammer,
        4 => Rod,
        3 | 14 => Knife,
        5 | 11 | 15 => RopeKit,
        6 | 13 | 19 => Basket,
        7 | 10 => Hoe,
        16 => WateringCan,
        _ => return None,
    })
}

fn resolve(mask: PhaseMask, name: &str) -> Option<TilePos> {
    WILLOW_VILLAGE
        .poi(name)
        .filter(|p| mask.contains(p.phase))
        .map(|p| p.tile)
        .or_else(|| {
            WILLOW_VILLAGE
                .crop(name)
                .filter(|p| mask.contains(p.phase))
                .map(|p| p.work)
        })
        .or_else(|| {
            WILLOW_VILLAGE
                .lot(name)
                .filter(|p| mask.contains(p.phase))
                .map(|p| p.access)
        })
        .or_else(|| {
            WILLOW_VILLAGE
                .roads
                .iter()
                .find(|road| road.id == name && mask.contains(road.phase))
                .and_then(|road| road.axis.first().copied())
        })
}

fn slot(mask: PhaseMask, site: &str, name: &str) -> Option<TilePos> {
    let s = WILLOW_VILLAGE.site(site)?;
    if !mask.contains(s.phase) {
        return None;
    }
    s.slots
        .iter()
        .find(|s| s.id.rsplit('.').next() == Some(name))
        .and_then(|s| match s.position {
            SlotPos::Outdoor(p) => Some(p),
            _ => None,
        })
}

fn house(index: usize) -> &'static crate::map_data::BuildingDef {
    static HOMES: OnceLock<[&crate::map_data::BuildingDef; RESIDENT_COUNT]> = OnceLock::new();
    HOMES.get_or_init(|| {
        std::array::from_fn(|index| {
            let name = Profession::resident(index).asset_name();
            let address = WILLOW_VILLAGE
                .residents
                .iter()
                .find(|resident| resident.id == name)
                .unwrap()
                .home;
            WILLOW_VILLAGE.building(address).unwrap()
        })
    })[index]
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
enum State {
    Unsettled,
    Inside,
    MovingIn,
    LeavingVillage,
    Away,
    ReturningFromAway,
    Outbound,
    AtWork,
    Leaving,
    Visiting,
    Resting,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct Edge {
    to: TilePos,
    start: u64,
    end: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Walker {
    at: TilePos,
    target: TilePos,
    state: State,
    due: u64,
    edge: Option<Edge>,
    segment: Vec<TilePos>,
    next: u8,
    turn: bool,
    wait: u8,
}

impl Walker {
    fn anim_frame(&self, now: u64, offset: u64) -> usize {
        self.edge
            .map_or(((now / (30 * STEP) + offset) % 3) as usize, |edge| {
                ((edge_fraction(now, edge.start, edge.end) * 3.0) as usize).min(2)
            })
    }

    fn new(at: TilePos, state: State) -> Self {
        Self {
            at,
            target: at,
            state,
            due: 0,
            edge: None,
            segment: Vec::new(),
            next: 0,
            turn: false,
            wait: 0,
        }
    }

    fn hidden(&self) -> bool {
        matches!(self.state, State::Unsettled | State::Inside | State::Away)
    }

    fn moving(&self) -> bool {
        matches!(
            self.state,
            State::MovingIn
                | State::Outbound
                | State::Leaving
                | State::Visiting
                | State::LeavingVillage
                | State::ReturningFromAway
        )
    }

    fn redirect(&mut self, target: TilePos, state: State, now: u64) {
        self.target = target;
        self.state = state;
        self.segment.clear();
        self.next = 0;
        self.wait = 0;
        self.due = now;
    }

    fn position(&self, now: u64) -> Point {
        let mut p = point(self.at);
        if let Some(edge) = self.edge {
            let q = point(edge.to);
            let t = edge_fraction(now, edge.start, edge.end);
            p.x += (q.x - p.x) * t;
            p.y += (q.y - p.y) * t;
        }
        p
    }

    fn holds_corridor(&self) -> bool {
        !self.hidden()
            && (outer_corridor(self.at) || self.edge.is_some_and(|edge| outer_corridor(edge.to)))
    }

    fn valid(&self, now: u64, map: &LivingWorld) -> bool {
        let inside = |p: TilePos| p.index().is_some();
        let walkable = |p: TilePos| map.can_walk(i32::from(p.x), i32::from(p.y));
        let next = usize::from(self.next);
        inside(self.at)
            && inside(self.target)
            && (self.hidden() || walkable(self.at))
            && self.segment.len() <= MAX_SEGMENT_EDGES
            && next <= self.segment.len()
            && self.segment.iter().all(|tile| inside(*tile))
            && self
                .segment
                .windows(2)
                .all(|pair| distance(pair[0], pair[1]) == 1)
            && (next == 0 || self.segment[next - 1] == self.at)
            && self.due <= now.saturating_add(DAY_MS)
            && self.edge.is_none_or(|e| {
                self.moving()
                    && e.start <= now
                    && e.end >= now
                    && e.end == self.due
                    && valid_travel_edge(e.start, e.end)
                    && distance(self.at, e.to) == 1
                    && walkable(e.to)
                    && self.segment.get(next) == Some(&e.to)
            })
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct Cargo {
    item: Item,
    quantity: u16,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct CounterBatch {
    shop: u8,
    cargo: Cargo,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct ServiceMaterial {
    cargo: Cargo,
    household: bool,
    returning: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct NeighbourMeal {
    portions: u16,
    packed_day: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct Resident {
    walk: Walker,
    inventory: [u16; 8],
    produced_day: Option<u64>,
    job: Option<u16>,
    job_stop: u8,
    escrow: Option<Cargo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    returning_cargo: Option<Cargo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    counter: Option<CounterBatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool: Option<VisitorTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    produce_in_transit: Option<Cargo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    service_material: Option<ServiceMaterial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    neighbour_meal: Option<NeighbourMeal>,
    #[serde(default)]
    neighbour_meal_day: Option<u64>,
    break_day: Option<u64>,
    #[serde(default)]
    supper_day: Option<u64>,
    #[serde(default)]
    prepared_meal_day: Option<u64>,
    #[serde(default)]
    foraged_food_day: Option<u64>,
    #[serde(default)]
    tailored_day: Option<u64>,
}

impl Resident {
    fn count(&self, item: Item) -> u16 {
        self.inventory[item_index(item)]
    }

    fn can_accept(&self, cargo: Cargo) -> bool {
        let held = self
            .escrow
            .filter(|paid| paid.item == cargo.item)
            .map_or(0, |paid| paid.quantity);
        let held = held.saturating_add(
            self.counter
                .filter(|batch| batch.cargo.item == cargo.item)
                .map_or(0, |batch| batch.cargo.quantity),
        );
        let held = held.saturating_add(
            self.returning_cargo
                .filter(|batch| batch.item == cargo.item)
                .map_or(0, |batch| batch.quantity),
        );
        let held = held.saturating_add(
            self.produce_in_transit
                .filter(|batch| batch.item == cargo.item)
                .map_or(0, |batch| batch.quantity),
        );
        let held = held.saturating_add(
            self.service_material
                .filter(|batch| !batch.household && batch.cargo.item == cargo.item)
                .map_or(0, |batch| batch.cargo.quantity),
        );
        let held = held.saturating_add(if cargo.item == Item::Food {
            self.neighbour_meal.map_or(0, |batch| batch.portions)
        } else {
            0
        });
        self.count(cargo.item)
            .saturating_add(held)
            .saturating_add(cargo.quantity)
            <= HARD_STOCK_CAP
    }

    fn incoming(&mut self, cargo: Cargo) -> bool {
        if !self.can_accept(cargo) {
            return false;
        }
        self.inventory[item_index(cargo.item)] += cargo.quantity;
        true
    }

    fn counter_goods(&self, shop: usize) -> u16 {
        self.counter
            .filter(|batch| usize::from(batch.shop) == shop)
            .map_or(0, |batch| batch.cargo.quantity)
    }

    fn ready_to_exchange(&self, shop_id: usize, keep_meal: bool) -> bool {
        let shop = shop(shop_id);
        self.counter_goods(shop_id) >= shop.goods.quantity
            && self.can_accept(shop.cost)
            && (!keep_meal || self.count(Item::Food) > 0)
    }

    fn pack_counter(&mut self, shop_id: usize, reserve: u16) -> bool {
        let goods = shop(shop_id).goods;
        if self.counter.is_some()
            || self.count(goods.item) < goods.quantity + reserve
            || !self.outgoing(goods)
        {
            return false;
        }
        self.counter = Some(CounterBatch {
            shop: shop_id as u8,
            cargo: goods,
        });
        true
    }

    fn stow_counter(&mut self) {
        if let Some(batch) = self.counter.take() {
            assert!(
                self.incoming(batch.cargo),
                "a packed lot keeps its private cupboard capacity"
            );
        }
    }

    fn gather_produce(&mut self, goods: Cargo) -> bool {
        if self.produce_in_transit.is_some() || !self.can_accept(goods) {
            return false;
        }
        self.produce_in_transit = Some(goods);
        true
    }

    fn stow_produce(&mut self) {
        if let Some(goods) = self.produce_in_transit.take() {
            assert!(self.incoming(goods));
        }
    }

    fn outgoing(&mut self, cargo: Cargo) -> bool {
        let value = &mut self.inventory[item_index(cargo.item)];
        if *value < cargo.quantity {
            return false;
        }
        *value -= cargo.quantity;
        true
    }

    fn eat_home_supper(&mut self, home: TilePos, day: u64, reserve: u16) {
        if self.walk.state == State::Inside
            && self.walk.at == home
            && self.supper_day != Some(day)
            && self.count(Item::Food) > reserve
            && self.outgoing(cargo(Item::Food, 1))
        {
            self.supper_day = Some(day);
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum TripStage {
    Pack,
    Outbound,
    Exchange,
    Inbound,
    Unload,
}

#[derive(Clone, Copy, Debug)]
pub struct TripVisual {
    pub actor: ActorKind,
    pub stage: TripStage,
    pub position: Point,
    pub target: Point,
    pub held: Option<Item>,
}

#[derive(Clone, Copy, Debug)]
pub struct StallVisual {
    pub site: &'static str,
    pub tile: TilePos,
    pub item: Item,
    pub quantity: u16,
    pub open: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct WildlifeVisual {
    pub sprite: &'static str,
    pub position: Point,
    pub facing_left: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ResidentStatus {
    pub id: &'static str,
    pub home: &'static str,
    pub bed: u8,
    pub settled: bool,
    pub indoors: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct EcoVisit {
    date: Option<u64>,
    visits: u8,
}

#[derive(Clone, Copy)]
struct Animal {
    path: &'static [(u16, u16)],
    frames: &'static [&'static str],
    active_from: u16,
    active_to: u16,
    period: u64,
    weather: u8,
    mature: bool,
}

const ANIMALS: [Animal; 19] = [
    Animal {
        path: &[(90, 17), (90, 18), (91, 18), (92, 18), (92, 19)],
        frames: &[
            "wild_grey_cat_idle_0",
            "wild_grey_cat_walk_0",
            "wild_grey_cat_walk_1",
            "wild_grey_cat_stretch_0",
        ],
        active_from: 500,
        active_to: 840,
        period: 28_000,
        weather: 0,
        mature: false,
    },
    Animal {
        path: &[(144, 79), (145, 79), (145, 80), (145, 81), (146, 81)],
        frames: &[
            "wild_black_cat_look_0",
            "wild_black_cat_walk_0",
            "wild_black_cat_walk_1",
            "wild_black_cat_sleep_0",
        ],
        active_from: 1020,
        active_to: 1290,
        period: 31_000,
        weather: 0,
        mature: true,
    },
    Animal {
        path: &[(32, 73), (33, 73), (34, 73), (34, 72)],
        frames: &[
            "wild_squirrel_forage_0",
            "wild_squirrel_leap_0",
            "wild_squirrel_land_0",
        ],
        active_from: 350,
        active_to: 1070,
        period: 14_000,
        weather: 1,
        mature: false,
    },
    Animal {
        path: &[(29, 85), (29, 86), (28, 86), (28, 87)],
        frames: &[
            "wild_hedgehog_sniff_0",
            "wild_hedgehog_walk_0",
            "wild_hedgehog_walk_1",
            "wild_hedgehog_curl_0",
        ],
        active_from: 1100,
        active_to: 70,
        period: 43_000,
        weather: 4,
        mature: false,
    },
    Animal {
        path: &[(13, 63), (14, 63), (15, 63), (15, 64)],
        frames: &[
            "wild_rabbit_idle_0",
            "wild_rabbit_crouch_0",
            "wild_rabbit_hop_0",
            "wild_rabbit_land_0",
        ],
        active_from: 320,
        active_to: 490,
        period: 25_000,
        weather: 1,
        mature: false,
    },
    Animal {
        path: &[(13, 30), (13, 31), (14, 31), (14, 32)],
        frames: &[
            "wild_frog_call_0",
            "wild_frog_hop_0",
            "wild_frog_hop_1",
            "wild_frog_land_0",
        ],
        active_from: 925,
        active_to: 60,
        period: 25_000,
        weather: 2,
        mature: false,
    },
    Animal {
        path: &[(172, 18), (173, 18), (174, 18), (175, 18)],
        frames: &[
            "wild_lizard_idle_0",
            "wild_lizard_scurry_0",
            "wild_lizard_scurry_1",
            "wild_lizard_flatten_0",
        ],
        active_from: 600,
        active_to: 915,
        period: 16_000,
        weather: 1,
        mature: false,
    },
    Animal {
        path: &[(14, 21), (15, 21), (16, 21), (16, 22)],
        frames: &[
            "wild_field_mouse_forage_0",
            "wild_field_mouse_run_0",
            "wild_field_mouse_run_1",
            "wild_field_mouse_groom_0",
        ],
        active_from: 1140,
        active_to: 260,
        period: 18_000,
        weather: 4,
        mature: false,
    },
    Animal {
        path: &[(19, 16), (20, 16), (20, 15), (20, 14)],
        frames: &[
            "wild_sparrow_peck_0",
            "wild_sparrow_fly_0",
            "wild_sparrow_fly_1",
            "wild_sparrow_land_0",
        ],
        active_from: 360,
        active_to: 575,
        period: 11_000,
        weather: 0,
        mature: false,
    },
    Animal {
        path: &[(67, 39), (66, 39), (65, 39), (64, 39), (64, 40), (64, 41)],
        frames: &[
            "wild_swallow_fly_0",
            "wild_swallow_fly_1",
            "wild_swallow_fly_2",
        ],
        active_from: 440,
        active_to: 1050,
        period: 8_000,
        weather: 1,
        mature: false,
    },
    Animal {
        path: &[(13, 30), (13, 31), (13, 32), (13, 33)],
        frames: &["wild_duckling_walk_0", "wild_duckling_swim_0"],
        active_from: 500,
        active_to: 1040,
        period: 96_000,
        weather: 0,
        mature: false,
    },
    Animal {
        path: &[(180, 20), (181, 20), (182, 20), (183, 20)],
        frames: &[
            "wild_crow_idle_0",
            "wild_crow_fly_0",
            "wild_crow_fly_1",
            "wild_crow_land_0",
        ],
        active_from: 400,
        active_to: 530,
        period: 35_000,
        weather: 0,
        mature: false,
    },
    Animal {
        path: &[(23, 71), (23, 72), (24, 72), (24, 73)],
        frames: &[
            "wild_great_tit_perch_0",
            "wild_great_tit_fly_0",
            "wild_great_tit_fly_1",
            "wild_great_tit_land_0",
        ],
        active_from: 420,
        active_to: 940,
        period: 10_000,
        weather: 0,
        mature: false,
    },
    Animal {
        path: &[(108, 78), (109, 78), (109, 79), (110, 79)],
        frames: &[
            "wild_white_pigeon_walk_0",
            "wild_white_pigeon_fly_0",
            "wild_white_pigeon_fly_1",
            "wild_white_pigeon_land_0",
        ],
        active_from: 490,
        active_to: 690,
        period: 24_000,
        weather: 1,
        mature: false,
    },
    Animal {
        path: &[(19, 22), (19, 23), (20, 23)],
        frames: &[
            "wild_snail_idle_0",
            "wild_snail_crawl_0",
            "wild_snail_feel_0",
            "wild_snail_retract_0",
        ],
        active_from: 450,
        active_to: 1100,
        period: 94_000,
        weather: 2,
        mature: false,
    },
    Animal {
        path: &[(20, 18), (20, 19), (20, 20), (20, 21)],
        frames: &[
            "wild_ladybird_crawl_0",
            "wild_ladybird_unfold_0",
            "wild_ladybird_fly_0",
        ],
        active_from: 560,
        active_to: 990,
        period: 38_000,
        weather: 1,
        mature: false,
    },
    Animal {
        path: &[(18, 34), (18, 35), (19, 35), (19, 34)],
        frames: &[
            "wild_firefly_idle_0",
            "wild_firefly_unfold_0",
            "wild_firefly_fly_0",
            "wild_firefly_fly_1",
        ],
        active_from: 1150,
        active_to: 70,
        period: 21_000,
        weather: 3,
        mature: false,
    },
    Animal {
        path: &[(128, 118), (129, 118), (129, 119), (130, 119)],
        frames: &[
            "wild_honeybee_idle_0",
            "wild_honeybee_fly_0",
            "wild_honeybee_fly_1",
            "wild_honeybee_fly_2",
        ],
        active_from: 520,
        active_to: 925,
        period: 13_000,
        weather: 1,
        mature: true,
    },
    Animal {
        path: &[(111, 85), (112, 85), (112, 86), (113, 86)],
        frames: &[
            "wild_butterfly_perch_0",
            "wild_butterfly_fly_0",
            "wild_butterfly_fly_1",
            "wild_butterfly_fly_2",
        ],
        active_from: 600,
        active_to: 975,
        period: 19_000,
        weather: 1,
        mature: false,
    },
];

fn animal_visible(id: usize, a: Animal, world: &LivingWorld) -> bool {
    use crate::living_world::YardSeason;
    let minute = world.world_ms() % DAY_MS / MINUTE;
    let from = u64::from(a.active_from);
    let to = u64::from(a.active_to);
    let time = if from <= to {
        (from..to).contains(&minute)
    } else {
        minute >= from || minute < to
    };
    let phase = if id == 17 {
        "cooperative_farm"
    } else {
        "resident_arrival"
    };
    time && (!a.mature || phase_active(world.phase_mask(), phase))
        && match a.weather {
            1 => {
                matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds)
                    && world.season() != YardSeason::Winter
            }
            2 => world.weather() == YardWeather::Rain && world.season() != YardSeason::Winter,
            3 => world.weather() == YardWeather::Clear && world.season() == YardSeason::Summer,
            4 => world.weather() != YardWeather::Snow,
            _ => true,
        }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct Trip {
    actor: ActorKind,
    shop: u8,
    started: u64,
    due: u64,
    expires: u64,
    stage: TripStage,
    target: TilePos,
    bag: Option<Cargo>,
    outgoing: Cargo,
    incoming: Cargo,
    accepted: bool,
    leg: u8,
    hold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    regular_meal: bool,
}

#[derive(Clone, Copy)]
struct Shop {
    id: &'static str,
    merchant: usize,
    opens: u16,
    closes: u16,
    cost: Cargo,
    goods: Cargo,
}

const fn cargo(item: Item, quantity: u16) -> Cargo {
    Cargo { item, quantity }
}

const SHOPS: [Shop; 6] = [
    Shop {
        id: "stall_produce.door",
        merchant: 6,
        opens: 510,
        closes: 930,
        cost: cargo(Item::Seeds, 1),
        goods: cargo(Item::Crops, 2),
    },
    Shop {
        id: "stall_seed.door",
        merchant: 1,
        opens: 540,
        closes: 990,
        cost: cargo(Item::Crops, 2),
        goods: cargo(Item::Seeds, 3),
    },
    Shop {
        id: "stall_meal.door",
        merchant: 3,
        opens: 630,
        closes: 900,
        cost: cargo(Item::Crops, 2),
        goods: cargo(Item::Food, 2),
    },
    Shop {
        id: "stall_cloth.door",
        merchant: 5,
        opens: 600,
        closes: 990,
        cost: cargo(Item::Crops, 2),
        goods: cargo(Item::Rope, 1),
    },
    Shop {
        id: "stall_fish.door",
        merchant: 4,
        opens: 450,
        closes: 690,
        cost: cargo(Item::Food, 1),
        goods: cargo(Item::Fish, 1),
    },
    Shop {
        id: "stall_fruit_flower.door",
        merchant: 7,
        opens: 570,
        closes: 960,
        cost: cargo(Item::Rope, 1),
        goods: cargo(Item::Crops, 3),
    },
];
const MOBILE: usize = SHOPS.len();
const MEAL_SHOP: usize = 2;
const MOBILE_SUPPLIERS: [Shop; 3] = [
    Shop {
        id: "home_trade",
        merchant: 9,
        opens: 480,
        closes: 570,
        cost: cargo(Item::Crops, 2),
        goods: cargo(Item::Stone, 3),
    },
    Shop {
        id: "home_trade",
        merchant: 2,
        opens: 690,
        closes: 780,
        cost: cargo(Item::Food, 1),
        goods: cargo(Item::Wood, 3),
    },
    Shop {
        id: "home_trade",
        merchant: 5,
        opens: 780,
        closes: 870,
        cost: cargo(Item::Crops, 2),
        goods: cargo(Item::Rope, 1),
    },
];
const SHOP_COUNT: usize = SHOPS.len() + 1 + MOBILE_SUPPLIERS.len();
const OUTBOUND_WAYPOINTS: [TilePos; 5] = [
    TilePos::new(44, 25),
    TilePos::new(44, 38),
    TilePos::new(56, 38),
    TilePos::new(56, 42),
    TilePos::new(68, 42),
];
const MARKET_ENTRY_LEG: usize = OUTBOUND_WAYPOINTS.len();
const INBOUND_WAYPOINTS: [TilePos; 5] = [
    TilePos::new(68, 42),
    TilePos::new(56, 42),
    TilePos::new(56, 38),
    TilePos::new(44, 38),
    TilePos::new(44, 25),
];

fn shop(index: usize) -> Shop {
    match index {
        MOBILE => Shop {
            id: "home_trade",
            opens: 570,
            closes: 690,
            ..SHOPS[1]
        },
        n if n > MOBILE => MOBILE_SUPPLIERS[n - MOBILE - 1],
        n => SHOPS[n],
    }
}

fn merchant_at(shop_id: usize, now: u64) -> usize {
    if shop_id == 5 && now % DAY_MS / MINUTE >= 750 {
        6
    } else {
        shop(shop_id).merchant
    }
}

fn lane_traffic_site(tile: TilePos) -> bool {
    [
        "home_lane",
        "bridge_main",
        "market_approach",
        "market_south",
        "external_entry",
    ]
    .into_iter()
    .filter_map(|name| WILLOW_VILLAGE.roads.iter().find(|r| r.id == name))
    .any(|road| {
        let width = u16::from(road.width);
        road.axis.windows(2).any(|line| {
            let (a, b) = (line[0], line[1]);
            if a.x == b.x {
                (a.x..a.x + width).contains(&tile.x)
                    && (a.y.min(b.y)..=a.y.max(b.y)).contains(&tile.y)
            } else {
                (a.y..a.y + width).contains(&tile.y)
                    && (a.x.min(b.x)..=a.x.max(b.x)).contains(&tile.x)
            }
        })
    })
}

fn shared_apron(site: &str) -> bool {
    matches!(
        site,
        "home_gate"
            | "home_lane"
            | "bridge_main"
            | "bridge_wait_w"
            | "bridge_wait_e"
            | "market_gate"
            | "market_notice"
            | "outside_gate"
    )
}

fn apron_tiles(world: &LivingWorld, anchor: TilePos) -> impl Iterator<Item = TilePos> + '_ {
    [
        (0, -1),
        (-1, 0),
        (1, 0),
        (0, -2),
        (-1, -1),
        (1, -1),
        (0, 1),
        (0, 2),
    ]
    .into_iter()
    .filter_map(move |(dx, dy)| anchor.offset(dx, dy))
    .filter(|p| {
        !outer_corridor(*p)
            && !lane_traffic_site(*p)
            && world.can_walk(p.x.into(), p.y.into())
            && !WILLOW_VILLAGE.buildings.iter().any(|b| b.door == *p)
    })
}

fn business_open(index: usize, mask: PhaseMask, time: u64) -> bool {
    let hour = (time % DAY_MS / MINUTE) as u16;
    let day = time / DAY_MS % 7;
    let s = shop(index);
    if index > MOBILE {
        return (s.opens..s.closes).contains(&hour) && day != 6;
    }
    if index == MOBILE {
        return !phase_active(mask, "permanent_market")
            && !(day == 5 && phase_active(mask, "weekly_market"))
            && (s.opens..s.closes).contains(&hour);
    }
    if day == 5 && index < 4 && phase_active(mask, "weekly_market") {
        return (540..810).contains(&hour);
    }
    if !phase_active(mask, "permanent_market") {
        return false;
    }
    let close = if day == 6 {
        if index > 2 {
            return false;
        }
        s.closes.min(750)
    } else {
        s.closes
    };
    (s.opens..close).contains(&hour)
}

fn market_cost_available(world: &LivingWorld, cost: Cargo) -> bool {
    let construction_reserve = world
        .next_construction_material_requirement()
        .filter(|(item, _)| *item == cost.item)
        .map_or(0, |(_, quantity)| quantity);
    world.stock().count(cost.item) >= cost.quantity.saturating_add(construction_reserve)
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct Guest {
    identity: u8,
    walk: Walker,
    leave_by: u64,
    cargo: Option<Cargo>,
    bed: Option<u8>,
    wake: Option<u64>,
    received: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Effect {
    Observe,
    Supply,
    Craft,
    Deliver,
    Plot,
    Service,
    Native,
}

#[derive(Clone, Copy)]
struct Contract {
    id: &'static str,
    name: &'static str,
    actor: Option<usize>,
    sites: &'static str,
    phases: &'static str,
    effect: Effect,
    scope: &'static str,
}

fn contracts() -> impl ExactSizeIterator<Item = Contract> {
    static DEFINITIONS: OnceLock<Vec<Contract>> = OnceLock::new();
    DEFINITIONS
        .get_or_init(|| {
            CATALOG
                .lines()
                .skip(1)
                .map(|line| {
                    let mut fields = line.split('\t');
                    let id = fields.next().unwrap();
                    let name = fields.next().unwrap();
                    let actor = fields.next().unwrap().parse().ok();
                    let sites = fields.next().unwrap();
                    let phases = fields.next().unwrap();
                    let effect = match fields.next().unwrap() {
                        "observe" => Effect::Observe,
                        "supply" => Effect::Supply,
                        "craft" => Effect::Craft,
                        "deliver" => Effect::Deliver,
                        "plot" => Effect::Plot,
                        "service" => Effect::Service,
                        "native" => Effect::Native,
                        _ => unreachable!("validated activity definition"),
                    };
                    Contract {
                        id,
                        name,
                        actor,
                        sites,
                        phases,
                        effect,
                        scope: fields.next().unwrap(),
                    }
                })
                .collect()
        })
        .iter()
        .copied()
}

pub const CONTRACT_COUNT: usize = 284;
const TWO_FIELD_WATERING: &str = "ME014";

fn plot_index(name: &str) -> Option<u8> {
    name.strip_prefix("crop_")?.parse().ok()
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct ActivityReceipt {
    code: u16,
    time: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct ServiceWork {
    code: u16,
    revision: u16,
    due: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct WorkOrder {
    opened: u64,
    attempts: u8,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LifeEvidence {
    shared_address: bool,
    weekly_day: Option<u64>,
    fruit_worker: Option<u8>,
    rain: bool,
    previous_rain: Option<u64>,
    cast: Option<u64>,
    refuge: [Option<u64>; 19],
    cached_fruit: u8,
    bridge_departure: Option<u64>,
}

use services::{ServiceLedger, ServiceOp};
#[path = "residents_services.rs"]
mod services;
#[path = "social_story.rs"]
mod social_story;
pub use social_story::SocialProgress;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VillageLife {
    initialized: bool,
    people: [Resident; RESIDENT_COUNT],
    guests: [Option<Guest>; EXTERNAL_COUNT],
    next_guest: u64,
    desired: u8,
    last_window: Option<u64>,
    next_schedule: u64,
    next_trip: u64,
    field_due: u64,
    trip: Option<Trip>,
    market_outings: u64,
    reserved: Option<ActorKind>,
    trips: u64,
    trades: u64,
    visits: u64,
    events: u64,
    #[serde(default)]
    companionship_received: u16,
    #[serde(default)]
    social: social_story::State,
    completed: [u64; 5],
    daily_native: [u64; 2],
    last_receipt: Option<ActivityReceipt>,
    work: [Option<ServiceWork>; 16],
    site_status: [u8; 16],
    pollination: u8,
    orders: [Option<WorkOrder>; 20],
    #[serde(default)]
    services: ServiceLedger,
    #[serde(default)]
    evidence: LifeEvidence,
    ecology: [EcoVisit; 19],
    last_production: Option<u64>,
    remaining_source: [u16; 8],
    setup: u8,
    #[serde(skip)]
    routing: RouteWork,
}

impl Default for VillageLife {
    fn default() -> Self {
        let mut people = std::array::from_fn(|index| Resident {
            walk: Walker::new(
                house(index).door,
                if FIRST_RESIDENTS.contains(&index) {
                    State::Inside
                } else {
                    State::Unsettled
                },
            ),
            inventory: [0; 8],
            produced_day: None,
            job: None,
            job_stop: 0,
            escrow: None,
            returning_cargo: None,
            counter: None,
            tool: None,
            produce_in_transit: None,
            service_material: None,
            neighbour_meal: None,
            neighbour_meal_day: None,
            break_day: None,
            supper_day: None,
            prepared_meal_day: None,
            foraged_food_day: None,
            tailored_day: None,
        });
        for (index, item, quantity) in [
            (1, Item::Seeds, 18),
            (2, Item::Wood, 8),
            (3, Item::Food, 8),
            (4, Item::Fish, 4),
            (5, Item::Rope, 7),
            (6, Item::Crops, 12),
            (7, Item::Crops, 12),
            (10, Item::Seeds, 4),
            (11, Item::Rope, 3),
            (12, Item::Stone, 6),
            (14, Item::Food, 4),
            (15, Item::Rope, 3),
            (16, Item::Water, 4),
            (18, Item::Stone, 4),
        ] {
            people[index].inventory[item_index(item)] = quantity;
        }
        Self {
            initialized: false,
            people,
            guests: std::array::from_fn(|_| None),
            next_guest: 0,
            desired: 0,
            last_window: None,
            next_schedule: 0,
            next_trip: 0,
            field_due: 0,
            trip: None,
            market_outings: 0,
            reserved: None,
            trips: 0,
            trades: 0,
            visits: 0,
            events: 0,
            companionship_received: 0,
            social: social_story::State::default(),
            completed: [0; 5],
            daily_native: [0; 2],
            last_receipt: None,
            work: [None; 16],
            last_production: None,
            site_status: [0; 16],
            pollination: 0,
            orders: [None; 20],
            services: ServiceLedger::default(),
            evidence: LifeEvidence::default(),
            ecology: [EcoVisit::default(); 19],
            remaining_source: [24, 36, 32, 0, 32, 32, 12, 24],
            setup: 0,
            routing: RouteWork::default(),
        }
    }
}

impl VillageLife {
    pub fn migrate_from_legacy(visitors: &VisitorState, world: &mut LivingWorld) -> Self {
        let mut result = Self {
            visits: visitors.visits(),
            trades: visitors.trades(),
            ..Self::default()
        };
        for (index, resident) in result.people.iter_mut().take(10).enumerate() {
            resident.walk = Walker::new(house(index).door, State::Inside);
        }
        for (profession, item, quantity) in visitors.migration_cargo() {
            let _ = result.people[profession as usize].incoming(cargo(item, quantity));
        }
        if let Some(partner) = visitors.meeting_partner() {
            world.recover_activity(&[partner]);
        }
        result
    }

    pub fn reserved_actors(&self) -> &[ActorKind] {
        if let Some(actor) = self.social.host() {
            std::slice::from_ref(actor)
        } else {
            self.reserved.as_slice()
        }
    }

    pub fn residents(&self) -> impl Iterator<Item = ResidentStatus> + '_ {
        self.people.iter().enumerate().map(|(index, resident)| {
            let id = Profession::resident(index).asset_name();
            let home = house(index);
            let bed = home
                .occupants
                .iter()
                .position(|person| *person == id)
                .expect("resident has a compiled bed slot") as u8;
            ResidentStatus {
                id,
                home: home.id,
                bed,
                settled: !self.social.hold_arrival(index)
                    && !matches!(resident.walk.state, State::Unsettled | State::MovingIn),
                indoors: resident.walk.state == State::Inside,
            }
        })
    }
    pub fn visits(&self) -> u64 {
        self.visits
    }
    pub fn trips(&self) -> u64 {
        self.trips
    }
    pub fn trades(&self) -> u64 {
        self.trades
    }
    pub fn events(&self) -> u64 {
        self.events
    }
    pub fn update_companionship(&mut self, active_ms: u64) -> u16 {
        let newly_received = companionship::claimable(self.companionship_received, active_ms);
        self.companionship_received |= newly_received;
        newly_received
    }
    pub fn companionship_received(&self) -> u16 {
        self.companionship_received
    }
    pub fn companionship_valid_for_ms(&self, active_ms: u64) -> bool {
        companionship::valid_received(self.companionship_received, active_ms)
    }
    pub fn village_chapter_completed(&self, chapter: &str) -> bool {
        self.social.completed_id(chapter)
    }

    pub fn social_progress(&self) -> SocialProgress {
        self.social.progress()
    }

    pub fn social_public_corner(&self) -> Option<(TilePos, u8)> {
        self.social.public_corner()
    }

    pub fn reserved_harvest_plot(&self) -> Option<u8> {
        self.social.reserved_harvest_plot()
    }
    pub fn completed_contracts(&self) -> usize {
        self.completed
            .iter()
            .map(|mask| mask.count_ones() as usize)
            .sum()
    }

    fn completed_code(&self, code: usize) -> bool {
        self.completed[code / 64] & (1 << (code % 64)) != 0
    }
    pub fn last_event(&self) -> Option<(&'static str, &'static str, u64)> {
        let receipt = self.last_receipt?;
        let contract = contracts().nth(usize::from(receipt.code))?;
        Some((contract.id, contract.name, receipt.time))
    }
    pub fn latest_service_result(&self) -> Option<&'static str> {
        let (id, _, _) = self.last_event()?;
        services::definitions()
            .find(|row| row.id == id)
            .map(|row| row.description)
    }

    pub fn site_notice(&self, id: &str) -> Option<(u16, u64)> {
        self.services.notice(id)
    }

    pub fn has_completed(&self, id: &str) -> bool {
        contracts()
            .position(|contract| contract.id == id)
            .is_some_and(|code| self.completed_code(code))
    }

    pub fn wear_on(&self, id: &str) -> Option<u8> {
        self.services.wearing_status(id)
    }

    pub fn resident_rapport(&self, id: &str) -> Option<u8> {
        (0..RESIDENT_COUNT)
            .find(|&index| Profession::resident(index).asset_name() == id)
            .map(|index| self.services.rapport(index))
    }

    pub fn observe_basketball(&mut self, completed: u64, active: bool, world: &LivingWorld) {
        let now = world.world_ms();
        let opening = active && !self.services.plugin_active();
        if self.services.observe_basketball(completed, active) {
            self.observe_native(74, now);
        }
        if opening
            && world.village_building_active("commons_hall")
            && animal_visible(13, ANIMALS[13], world)
            && self.evidence.refuge[13].is_none_or(|due| due <= now)
        {
            self.evidence.refuge[13] = Some(now + 2 * HOUR);
            self.observe_native(82, now);
        }
    }

    pub fn navigation_pending(&self) -> bool {
        self.routing.waiting
    }
    pub fn desired(&self) -> u8 {
        self.desired
    }
    pub fn present(&self) -> usize {
        let market_home = booth_slot(MOBILE);
        self.guests.iter().flatten().count()
            + usize::from(self.social.visit_token())
            + (0..RESIDENT_COUNT)
                .filter(|&id| self.counts_as_visitor(id, market_home))
                .count()
    }

    fn counts_as_visitor(&self, id: usize, market_home: TilePos) -> bool {
        let r = &self.people[id];
        !r.walk.hidden()
            && r.walk.state != State::Leaving
            && (r.walk.target == market_home
                || r.job
                    .and_then(|code| contracts().nth(usize::from(code)))
                    .is_some_and(|c| c.sites.split(';').any(|s| s == "home_trade")))
    }

    pub fn trip_visual(&self, world: &LivingWorld) -> Option<TripVisual> {
        let trip = self.trip?;
        Some(TripVisual {
            actor: trip.actor,
            stage: trip.stage,
            position: world
                .actors()
                .iter()
                .find(|actor| actor.kind == trip.actor)?
                .position,
            target: point(trip.target),
            held: trip.bag.map(|batch| batch.item),
        })
    }

    pub fn household_held(&self, actor: ActorKind) -> Option<VisitorHeld> {
        self.trip
            .filter(|trip| trip.actor == actor)
            .and_then(|trip| trip.bag)
            .map(|batch| VisitorHeld::Goods(batch.item))
            .or_else(|| self.social.host_held(actor))
    }

    fn resident_held(&self, id: usize) -> Option<VisitorHeld> {
        let resident = &self.people[id];
        if let Some(good) = self
            .social
            .carried_item(id)
            .or(resident.escrow.map(|c| c.item))
            .or(resident.returning_cargo.map(|c| c.item))
            .or(resident.produce_in_transit.map(|c| c.item))
            .or(resident.service_material.map(|batch| batch.cargo.item))
            .or(resident.neighbour_meal.map(|_| Item::Food))
        {
            return Some(VisitorHeld::Goods(good));
        }
        if resident.walk.state == State::MovingIn {
            return Some(VisitorHeld::MoveBox);
        }
        if let Some(asset) = self.services.carried_asset(id as u8) {
            return Some(match asset {
                "fishing_rod" => VisitorHeld::Tool(VisitorTool::Rod),
                "tool" => VisitorHeld::Tool(VisitorTool::Hoe),
                "basket" => VisitorHeld::Tool(VisitorTool::Basket),
                _ => VisitorHeld::Parcel(asset),
            });
        }
        if let Some(batch) = resident.counter.filter(|batch| {
            resident.walk.at != booth_slot(usize::from(batch.shop))
                || resident.walk.state != State::AtWork
        }) {
            return Some(VisitorHeld::Goods(batch.cargo.item));
        }
        resident.tool.map(VisitorHeld::Tool)
    }

    fn collect_owned_tool_at_home(&mut self, id: usize) {
        if self.people[id].walk.at == house(id).door {
            self.people[id].tool = personal_tool(id);
        }
    }

    pub fn visuals(&self, world: &LivingWorld) -> Vec<VisitorVisual> {
        let now = world.world_ms();
        let mut visuals = Vec::with_capacity(self.people.len() + self.guests.len());
        for (id, r) in self.people.iter().enumerate() {
            if r.walk.hidden() {
                continue;
            }
            let profession = Profession::resident(id);
            let job = r
                .neighbour_meal
                .map(|_| "把自家多煮的饭送去粮仓")
                .or_else(|| {
                    r.job
                        .and_then(|j| contracts().nth(j as usize))
                        .map(|c| c.name)
                })
                .or_else(|| self.social.resident_activity(id));
            let held = self.resident_held(id);
            let moving_pose = if held.is_some_and(|item| !matches!(item, VisitorHeld::Tool(_))) {
                VisitorPose::Carrying
            } else {
                VisitorPose::Walking
            };
            let (pose, activity) = match (r.walk.state, r.walk.edge) {
                (State::MovingIn, Some(_)) => (moving_pose, "搬箱入户"),
                (State::MovingIn, None) => (VisitorPose::Waiting, "搬箱入户"),
                (State::Leaving | State::LeavingVillage | State::ReturningFromAway, Some(_)) => {
                    (moving_pose, "回家")
                }
                (State::Outbound, Some(_)) => (moving_pose, job.unwrap_or("去工作")),
                (State::AtWork, _) => (
                    if self.trip.is_some_and(|t| {
                        t.stage == TripStage::Exchange
                            && r.walk.at == booth_slot(usize::from(t.shop))
                            && merchant_at(t.shop as usize, now) == id
                    }) {
                        VisitorPose::Trading
                    } else {
                        VisitorPose::Working
                    },
                    job.unwrap_or("当班工作"),
                ),
                (State::Resting, _) => (VisitorPose::Resting, "休息"),
                _ => (VisitorPose::Waiting, "候行"),
            };
            visuals.push(VisitorVisual {
                profession,
                pose,
                position: r.walk.position(now),
                facing_left: r.walk.turn,
                animation_frame: r.walk.anim_frame(now, id as u64),
                label: profession.label(),
                activity,
                held,
            });
        }
        for g in self.guests.iter().flatten() {
            if g.walk.hidden() {
                continue;
            }
            let profession = Profession::resident(g.identity as usize);
            let (pose, activity) = match (g.walk.state, g.walk.edge) {
                (State::Leaving, Some(_)) if g.cargo.is_some() => {
                    (VisitorPose::Carrying, "沿村口回程")
                }
                (State::Leaving, Some(_)) => (VisitorPose::Walking, "沿村口回程"),
                (State::Leaving, None) => (VisitorPose::Waiting, "沿村口回程"),
                (State::Visiting, Some(_)) if g.cargo.is_some() => {
                    (VisitorPose::Carrying, "外来送货")
                }
                (State::Visiting, Some(_)) => (VisitorPose::Walking, "入村访客"),
                _ => (VisitorPose::Resting, "外来来客"),
            };
            visuals.push(VisitorVisual {
                profession,
                pose,
                position: g.walk.position(now),
                facing_left: g.walk.turn,
                animation_frame: g.walk.anim_frame(now, u64::from(g.identity)),
                label: "外来访客",
                activity,
                held: g.cargo.map(|load| VisitorHeld::Goods(load.item)),
            });
        }
        visuals
    }

    pub fn validate_for_world(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        let time_ok = |t| t <= now.saturating_add(DAY_MS);
        let occupancy = self.occupied_tiles();
        let distinct = occupancy
            .iter()
            .enumerate()
            .all(|(index, tile)| !occupancy[..index].contains(tile));
        let unique_beds = (0..3).all(|bed| {
            self.guests
                .iter()
                .flatten()
                .filter(|guest| guest.bed == Some(bed))
                .count()
                <= 1
        });
        self.desired as usize <= MAX_HUMAN_VISITORS
            && self.present() <= MAX_HUMAN_VISITORS
            && distinct
            && unique_beds
            && time_ok(self.next_schedule)
            && time_ok(self.next_guest)
            && time_ok(self.next_trip)
            && time_ok(self.field_due)
            && self.market_outings <= self.trips
            && self.companionship_valid_for_ms(u64::MAX)
            && self.social.valid(world)
            && self.setup & !0b1111 == 0
            && self.site_status.iter().all(|&status| status <= 12)
            && self.pollination <= 12
            && self.services.validate(now)
            && self.services.carriers().all(|(owner, returning)| {
                self.people.get(usize::from(owner)).is_some_and(|r| {
                    if returning {
                        r.job.is_none()
                    } else {
                        r.job.is_some()
                    }
                })
            })
            && self.evidence.cached_fruit <= 8
            && self
                .evidence
                .refuge
                .iter()
                .flatten()
                .all(|&until| time_ok(until))
            && self.evidence.previous_rain.is_none_or(|when| when <= now)
            && self
                .evidence
                .cast
                .is_none_or(|casts| casts <= world.fishing_casts())
            && self
                .evidence
                .weekly_day
                .is_none_or(|day| day <= now / DAY_MS)
            && self
                .evidence
                .bridge_departure
                .is_none_or(|time| time <= now)
            && self
                .ecology
                .iter()
                .all(|entry| entry.date.is_none_or(|day| day <= now / DAY_MS) && entry.visits <= 12)
            && self
                .last_window
                .is_none_or(|window| window <= now / (3 * HOUR))
            && self.last_production.is_none_or(|day| day <= now / DAY_MS)
            && self.people.iter().enumerate().all(|(id, r)| {
                r.inventory
                    .iter()
                    .all(|&quantity| quantity <= HARD_STOCK_CAP)
                    && r.walk.valid(now, world)
                    && (!r.walk.hidden()
                        || r.walk.at == house(id).door
                        || self.social.temporary_bed(id, r.walk.at)
                        || self.social.returning_tailor_at_boundary(id, &r.walk)
                        || id == 8
                            && r.walk.state == State::Away
                            && r.walk.at == WILLOW_VILLAGE.boundary.spawn)
                    && r.job.is_none_or(|v| usize::from(v) < CONTRACT_COUNT)
                    && r.job_stop < 8
                    && r.escrow.is_none_or(|cargo| {
                        r.job.is_some()
                            && cargo.quantity == 1
                            && r.count(cargo.item) < HARD_STOCK_CAP
                    })
                    && r.returning_cargo.is_none_or(|cargo| {
                        r.escrow.is_none()
                            && r.job.is_none()
                            && cargo.quantity == 1
                            && r.count(cargo.item) < HARD_STOCK_CAP
                    })
                    && r.service_material.is_none_or(|batch| {
                        r.escrow.is_none()
                            && r.returning_cargo.is_none()
                            && (1..=2).contains(&batch.cargo.quantity)
                            && (batch.household
                                || r.count(batch.cargo.item) + batch.cargo.quantity
                                    <= HARD_STOCK_CAP)
                            && if batch.returning {
                                r.job.is_none()
                            } else {
                                r.job
                                    .and_then(|code| contracts().nth(usize::from(code)))
                                    .and_then(|c| self.service_materials(c))
                                    == Some((batch.cargo, batch.household))
                            }
                    })
                    && (r.job.is_some() == self.orders[id].is_some())
                    && r.neighbour_meal.is_none_or(|batch| {
                        id == 10
                            && batch.portions == NEIGHBOUR_MEAL_PORTIONS
                            && batch.packed_day <= now / DAY_MS
                            && r.neighbour_meal_day == Some(batch.packed_day)
                            && r.count(Item::Food) + batch.portions <= HARD_STOCK_CAP
                            && r.service_material.is_none()
                            && r.escrow.is_none()
                    })
                    && r.neighbour_meal_day
                        .is_none_or(|day| id == 10 && day <= now / DAY_MS)
                    && r.tool.is_none_or(|tool| personal_tool(id) == Some(tool))
                    && r.produce_in_transit.is_none_or(|batch| {
                        batch.item == Item::Crops
                            && matches!(id, 3 | 6 | 10 | 11)
                            && (1..=5).contains(&batch.quantity)
                            && r.count(batch.item) + batch.quantity <= HARD_STOCK_CAP
                    })
                    && r.counter.is_none_or(|batch| {
                        let index = usize::from(batch.shop);
                        index < SHOP_COUNT
                            && batch.cargo == shop(index).goods
                            && (shop(index).merchant == id || index == 5 && id == 6)
                            && r.count(batch.cargo.item)
                                + batch.cargo.quantity
                                + r.escrow
                                    .filter(|item| item.item == batch.cargo.item)
                                    .map_or(0, |item| item.quantity)
                                <= HARD_STOCK_CAP
                    })
                    && self.orders[id]
                        .is_none_or(|order| order.opened <= now && order.attempts <= 2)
                    && r.produced_day.is_none_or(|day| day <= now / DAY_MS)
                    && r.break_day.is_none_or(|day| day <= now / DAY_MS)
                    && r.supper_day.is_none_or(|day| day <= now / DAY_MS)
                    && r.prepared_meal_day.is_none_or(|day| day <= now / DAY_MS)
                    && r.foraged_food_day.is_none_or(|day| day <= now / DAY_MS)
                    && r.tailored_day.is_none_or(|day| day <= now / DAY_MS)
            })
            && self.guests.iter().flatten().all(|g| {
                usize::from(g.identity) < RESIDENT_COUNT
                    && g.walk.valid(now, world)
                    && (!g.walk.hidden()
                        || g.walk.state == State::Inside
                            && g.bed.is_some()
                            && self.guest_bed(g.bed.unwrap()) == Some(g.walk.at)
                            && g.wake.is_some())
                    && g.bed.is_none_or(|bed| bed < 3)
                    && g.wake.is_none_or(time_ok)
                    && time_ok(g.leave_by)
                    && g.cargo
                        .is_none_or(|cargo| cargo.quantity != 0 && cargo.quantity <= 4)
            })
            && self.trip.is_none_or(|t| {
                usize::from(t.shop) < SHOP_COUNT
                    && (!t.regular_meal || usize::from(t.shop) == MEAL_SHOP)
                    && t.actor != ActorKind::Orange
                    && t.target.index().is_some()
                    && t.started <= now
                    && t.leg <= (MARKET_ENTRY_LEG + 2) as u8
                    && time_ok(t.due)
                    && time_ok(t.expires)
                    && t.bag.is_none_or(|current| {
                        current == if t.accepted { t.incoming } else { t.outgoing }
                    })
                    && (t.bag.is_some() || t.stage == TripStage::Pack)
                    && t.outgoing == shop(usize::from(t.shop)).cost
                    && t.incoming == shop(usize::from(t.shop)).goods
                    && t.hold == (self.reserved == Some(t.actor))
                    && (!t.hold || world.activity_deadline(t.actor).is_some())
            })
            && (self.trip.is_some() || self.reserved.is_none())
            && !(self.trip.is_some() && self.social.host().is_some())
            && self
                .last_receipt
                .is_none_or(|r| usize::from(r.code) < CONTRACT_COUNT && r.time <= now)
            && self
                .work
                .iter()
                .flatten()
                .all(|w| usize::from(w.code) < CONTRACT_COUNT && time_ok(w.due))
    }

    pub fn next_update_ms(&self, world: &LivingWorld) -> Option<u64> {
        let now = world.world_ms();
        if !self.initialized {
            return Some(now);
        }
        let mut next = self.next_schedule.min(self.next_guest).min(self.next_trip);
        if let Some(t) = self.trip {
            next = next.min(t.due);
            if let Some(due) = world.activity_deadline(t.actor) {
                next = next.min(due);
            }
        }
        for walk in self
            .people
            .iter()
            .map(|r| &r.walk)
            .chain(self.guests.iter().flatten().map(|g| &g.walk))
        {
            if !walk.hidden() && !matches!(walk.state, State::AtWork | State::Resting) {
                next = next.min(walk.due);
            }
        }
        for g in self.guests.iter().flatten() {
            if let Some(wake) = g.wake
                && g.walk.state == State::Inside
            {
                next = next.min(wake);
            }
            if matches!(g.walk.state, State::Resting | State::Visiting)
                && g.walk.edge.is_none()
                && !g
                    .bed
                    .and_then(|bed| self.guest_bed(bed))
                    .is_some_and(|door| door == g.walk.target && g.walk.moving())
            {
                next = next.min(g.leave_by);
            }
        }
        Some(next.max(now))
    }

    pub fn advance(&mut self, world: &mut LivingWorld) {
        let now = world.world_ms();
        let mask = world.phase_mask();
        if !self.initialized {
            self.initialized = true;
            self.next_schedule = now;
            self.next_trip = now + 15 * MINUTE;
        }
        self.advance_trip(world);
        if now >= self.next_schedule {
            self.next_schedule = next_boundary(now, MINUTE);
            let mut social = std::mem::take(&mut self.social);
            social.update(self, world, mask, now);
            self.social = social;
            world.set_social_harvest_plot(self.social.reserved_harvest_plot());
            self.apply_supply_requests(world);
            self.schedule(world, mask, now);
        }
        if now >= self.field_due {
            self.field_due = now + 5 * MINUTE;
            self.observe_household(world, now);
        }
        if now >= self.next_guest {
            self.admit(world, mask, now);
        }
        if now >= self.next_trip {
            self.next_trip = now + 10 * MINUTE;
            self.offer_trip(world, mask, now);
        }
        let mut routes = std::mem::take(&mut self.routing);
        routes.budget = 2_048;
        routes.waiting = false;
        for id in 0..RESIDENT_COUNT {
            if !self.people[id].walk.moving() || self.people[id].walk.due > now {
                continue;
            }
            let previous_wait = self.people[id].walk.wait;
            let previous_state = self.people[id].walk.state;
            let mut walking = std::mem::replace(
                &mut self.people[id].walk,
                Walker::new(house(id).door, State::Unsettled),
            );
            let laden = previous_state == State::MovingIn
                || self.people[id].counter.is_some()
                || self.people[id].escrow.is_some()
                || self.people[id].returning_cargo.is_some()
                || self.people[id].produce_in_transit.is_some()
                || self.people[id].service_material.is_some()
                || self.people[id].neighbour_meal.is_some()
                || self.social.carried_item(id).is_some()
                || self.services.carried_asset(id as u8).is_some();
            if self.move_walker(
                world,
                &mut routes,
                &mut walking,
                Some(id),
                None,
                mask,
                now,
                laden,
            ) {
                self.arrive(id, &mut walking, previous_state, now);
            }
            if walking.wait > previous_wait
                && (55..=69).contains(&walking.at.x)
                && (41..=43).contains(&walking.at.y)
            {
                self.observe_native(38, now);
            }
            self.people[id].walk = walking;
            if routes.waiting {
                self.routing = routes;
                return;
            }
        }
        for id in 0..EXTERNAL_COUNT {
            let Some(mut g) = self.guests[id].take() else {
                continue;
            };
            if g.walk.state == State::Inside {
                if g.wake.is_some_and(|wake| wake <= now)
                    && !self.occupied(world, g.walk.at, None, Some(id))
                {
                    g.walk
                        .redirect(WILLOW_VILLAGE.boundary.spawn, State::Leaving, now);
                    g.wake = None;
                    g.bed = None;
                } else {
                    if g.wake.is_some_and(|wake| wake <= now) {
                        g.wake = Some(now + RETRY);
                    }
                    self.guests[id] = Some(g);
                    continue;
                }
            }
            if now >= g.leave_by && g.walk.edge.is_none() && g.walk.state != State::Leaving {
                if let Some(bed) = g.bed {
                    if let Some(destination) = self.guest_bed(bed)
                        && g.walk.target != destination
                    {
                        g.walk.redirect(destination, State::Visiting, now);
                    }
                } else {
                    g.walk
                        .redirect(WILLOW_VILLAGE.boundary.spawn, State::Leaving, now);
                }
            }
            if g.walk.moving()
                && g.walk.due <= now
                && self.move_walker(
                    world,
                    &mut routes,
                    &mut g.walk,
                    None,
                    Some(id),
                    mask,
                    now,
                    g.cargo.is_some(),
                )
            {
                if g.walk.state == State::Leaving {
                    if let Some(goods) = g.cargo {
                        let source = &mut self.remaining_source[item_index(goods.item)];
                        *source = source.saturating_add(goods.quantity).min(48);
                    }
                    self.visits = self.visits.saturating_add(1);
                    continue;
                }
                if g.bed.and_then(|bed| self.guest_bed(bed)) == Some(g.walk.at) && now >= g.leave_by
                {
                    g.walk.state = State::Inside;
                    g.walk.segment.clear();
                    g.walk.next = 0;
                    g.wake = Some(next_boundary(now, DAY_MS) + 7 * HOUR);
                    self.observe_native(67, now);
                } else if g.walk.target == TilePos::new(188, 56) {
                    let target = if g.cargo.is_some() && phase_active(mask, "resident_arrival") {
                        TilePos::new(187, 57 + (id % 2) as u16)
                    } else {
                        self.social
                            .guide_target(mask, id)
                            .filter(|p| !self.occupied(world, *p, None, Some(id)))
                            .unwrap_or(TilePos::new(81 + (id % 4) as u16, 51 + (id / 4) as u16))
                    };
                    g.walk.redirect(target, State::Visiting, now);
                } else {
                    g.walk.state = State::Resting;
                    if !g.received {
                        g.received = true;
                        self.observe_native(66, now);
                    }
                }
            }
            if g.walk.state == State::Resting
                && let Some(goods) = g.cargo
                && !self.people[19].walk.hidden()
                && distance(self.people[19].walk.at, g.walk.at) <= 2
                && distance(g.walk.at, TilePos::new(188, 56)) <= 3
                && self.people[19].incoming(goods)
            {
                g.cargo = None;
                self.record(191, now);
            }
            self.guests[id] = Some(g);
            if routes.waiting {
                self.routing = routes;
                return;
            }
        }
        self.routing = routes;
    }

    pub fn apply_supply_requests(&self, world: &mut LivingWorld) {
        world.set_market_crop_reserve(self.social.crop_reserve_for_market(world));
        world.set_neighbour_food_reserve(self.social.neighbour_food_needed());
    }

    fn guest_bed(&self, bed: u8) -> Option<TilePos> {
        WILLOW_VILLAGE
            .poi(if bed < 2 {
                "craft_inn.door"
            } else {
                "new_single_home.door"
            })
            .map(|p| p.tile)
    }

    pub fn occupied_tiles(&self) -> Vec<TilePos> {
        let mut tiles = [TilePos::new(0, 0); VILLAGE_OCCUPANCY];
        let len = self.write_occupied_tiles(&mut tiles);
        tiles[..len].to_vec()
    }

    pub fn write_occupied_tiles(&self, tiles: &mut [TilePos; VILLAGE_OCCUPANCY]) -> usize {
        let mut len = 0;
        for walker in self
            .people
            .iter()
            .map(|r| &r.walk)
            .chain(self.guests.iter().flatten().map(|g| &g.walk))
        {
            if walker.hidden() {
                continue;
            }
            tiles[len] = walker.at;
            len += 1;
            if let Some(edge) = walker.edge {
                tiles[len] = edge.to;
                len += 1;
            }
        }
        len
    }

    pub fn stall_visuals(&self, world: &LivingWorld) -> Vec<StallVisual> {
        let mask = world.phase_mask();
        let now = world.world_ms();
        let visual = |id: usize| {
            let s = shop(id);
            let tile = slot(mask, s.id, "tool")?;
            let merchant = &self.people[merchant_at(id, now)];
            let open = self.business_open(id, mask, now)
                && merchant.walk.state == State::AtWork
                && booth_slot(id) == merchant.walk.at
                && merchant.counter_goods(id) >= s.goods.quantity;
            (id < MOBILE || open).then(|| StallVisual {
                site: s.id,
                tile,
                item: s.goods.item,
                quantity: if open { merchant.counter_goods(id) } else { 0 },
                open,
            })
        };
        (0..MOBILE)
            .filter_map(visual)
            .chain((MOBILE..SHOP_COUNT).find_map(visual))
            .collect()
    }

    pub fn wildlife_visuals(&self, world: &LivingWorld) -> Vec<WildlifeVisual> {
        let now = world.world_ms();
        ANIMALS
            .iter()
            .enumerate()
            .filter_map(|(id, &animal)| {
                if self.evidence.refuge[id].is_some_and(|until| until > now) {
                    let (sprite, where_at) = match id {
                        3 => ("wild_hedgehog_curl_0", TilePos::new(28, 87)),
                        5 => ("wild_frog_hop_0", TilePos::new(14, 31)),
                        9 => ("wild_swallow_perch_0", TilePos::new(72, 40)),
                        13 => {
                            let hall = WILLOW_VILLAGE.building("commons_hall")?;
                            ("wild_white_pigeon_idle_0", hall.door.offset(0, -4)?)
                        }
                        14 => ("wild_snail_retract_0", TilePos::new(19, 23)),
                        _ => return None,
                    };
                    return Some(WildlifeVisual {
                        sprite,
                        position: point(where_at),
                        facing_left: false,
                    });
                }
                if !animal_visible(id, animal, world) {
                    return None;
                }
                let cycle = 2 * (animal.path.len() - 1);
                let phase = now.saturating_add(1_951 * id as u64);
                let step = (phase / animal.period) as usize;
                let idx = |i: usize| {
                    if i % cycle < animal.path.len() {
                        i % cycle
                    } else {
                        cycle - i % cycle
                    }
                };
                let a = animal.path[idx(step)];
                let b = animal.path[idx(step + 1)];
                let frame = ((phase / 24_000) % animal.frames.len() as u64) as usize;
                let shift = (phase % animal.period) as f32 / animal.period as f32;
                let position = |row: (u16, u16)| {
                    if id == 16 && phase_active(world.phase_mask(), "late") {
                        point(TilePos::new(row.0 + 57, row.1 + 77))
                    } else {
                        point(TilePos::new(row.0, row.1))
                    }
                };
                let start = position(a);
                let end = position(b);
                let position = Point {
                    x: start.x + (end.x - start.x) * shift,
                    y: start.y + (end.y - start.y) * shift,
                };
                let sprite = if id == 10 {
                    let x = (position.x / TILE_PX as f32).floor() as i32;
                    let y = (position.y / TILE_PX as f32).floor() as i32;
                    animal.frames[usize::from(world.ground(x, y) == Ground::Water)]
                } else {
                    animal.frames[frame]
                };
                Some(WildlifeVisual {
                    sprite,
                    position,
                    facing_left: b.0 < a.0,
                })
            })
            .collect()
    }

    fn schedule(&mut self, world: &mut LivingWorld, mask: PhaseMask, now: u64) {
        let minute = (now % DAY_MS / MINUTE) as u16;
        let day = now / DAY_MS;
        if self.last_production != Some(day) {
            self.last_production = Some(day);
            self.setup = 0;
            self.daily_native = [0; 2];
            for (index, amount, limit) in [
                (1, 4, 36),
                (2, 3, 32),
                (4, 4, 32),
                (5, 5, 40),
                (6, 5, 48),
                (7, 3, 24),
            ] {
                self.remaining_source[index] = self.remaining_source[index]
                    .saturating_add(amount)
                    .min(limit);
            }
        }
        let weekly_setup = day % 7 == 5 && (350..810).contains(&minute);
        let setup_window = weekly_setup && !phase_active(mask, "weekly_market");
        if day % 7 == 5
            && (810..840).contains(&minute)
            && self.evidence.weekly_day == Some(day)
            && SHOPS
                .iter()
                .take(4)
                .any(|shop| self.people[shop.merchant].count(shop.goods.item) > 0)
            && SHOPS.iter().take(4).all(|shop| {
                self.people[shop.merchant].walk.state != State::AtWork
                    || self.people[shop.merchant].walk.at
                        != booth_slot(SHOPS.iter().position(|s| s.id == shop.id).unwrap())
            })
            && self.services.close_weekly(day)
        {
            self.observe_native(56, now);
        }
        let needed = world
            .next_construction_material_shortfall()
            .map(|(item, _)| item);
        let retail_home = booth_slot(MOBILE);
        let mut visitors = self.present();
        let planning_guest = self.social.market_planning_guest();
        for id in 0..RESIDENT_COUNT {
            let before = self.counts_as_visitor(id, retail_home);
            if self.people[id].walk.edge.is_some() {
                continue;
            }
            let home = house(id).door;
            let state = self.people[id].walk.state;
            if self.social.following(id)
                || self.social.protect_trial_candidate(id) && state == State::Inside
            {
                continue;
            }
            let counter_out = self.people[id].counter.is_some();
            if main_bridge_deck(self.people[id].walk.at) && self.people[id].walk.moving() {
                continue;
            }
            if id == 10 && self.schedule_neighbour_meal(world, id, mask, now) {
                continue;
            }
            if !self.return_service_material(world, id, mask, now) {
                continue;
            }
            if let Some((asset, owner, original)) = self.services.returning_to(id as u8) {
                let target = original.unwrap_or_else(|| {
                    if usize::from(owner) < RESIDENT_COUNT {
                        house(usize::from(owner)).door
                    } else {
                        home
                    }
                });
                if self.people[id].walk.at == target {
                    assert!(self.services.return_asset(asset, id as u8, target));
                } else {
                    if self.people[id].walk.target != target || !self.people[id].walk.moving() {
                        self.people[id].walk.redirect(target, State::Outbound, now);
                    }
                    continue;
                }
            }
            let early_setup =
                weekly_setup && SHOPS.iter().take(4).any(|stall| stall.merchant == id);
            let evening = minute >= QUEST_END[id] || minute < QUEST_START[id] && !early_setup;
            let produce_store = if id == 3 && !evening {
                resolve(mask, "home_kitchen.door").unwrap_or(home)
            } else {
                home
            };
            if state == State::Inside && !self.occupied(world, home, Some(id), None) {
                self.people[id].stow_counter();
            }
            if self.people[id].walk.at == produce_store {
                self.people[id].stow_produce();
            }
            if self.people[id].walk.at == home
                && let Some(batch) = self.people[id].returning_cargo.take()
            {
                assert!(self.people[id].incoming(batch));
            }
            if !evening
                && self.people[id].walk.at == home
                && !matches!(state, State::Unsettled | State::MovingIn)
            {
                self.collect_owned_tool_at_home(id);
                let queued = self.people[id]
                    .job
                    .and_then(|code| contracts().nth(usize::from(code)));
                if let Some(order) = queued
                    && order.effect == Effect::Deliver
                    && !early_setup
                    && self.people[id].escrow.is_none()
                    && let Some(item) = order
                        .scope
                        .split_once('>')
                        .and_then(|(_, name)| parse_item(name))
                {
                    let goods = cargo(item, 1);
                    if self.people[id].outgoing(goods) {
                        self.people[id].escrow = Some(goods);
                    } else {
                        self.finish_daily_order(id, day);
                    }
                }
            }
            if id == 8 {
                if state == State::Away
                    && now >= self.people[id].walk.due
                    && !self.occupied(world, WILLOW_VILLAGE.boundary.spawn, Some(id), None)
                    && !self.corridor_reserved(Some(id), None)
                {
                    self.people[id]
                        .walk
                        .redirect(home, State::ReturningFromAway, now);
                }
                if state == State::Away
                    || state == State::LeavingVillage
                    || state == State::ReturningFromAway
                {
                    continue;
                }
                if day % 7 == 2
                    && (630..720).contains(&minute)
                    && !evening
                    && !self.people[id].walk.hidden()
                {
                    self.people[id].job = None;
                    self.people[id].job_stop = 0;
                    self.clear_order(id);
                    self.people[id].walk.redirect(
                        WILLOW_VILLAGE.boundary.spawn,
                        State::LeavingVillage,
                        now,
                    );
                    continue;
                }
            }
            if state == State::Unsettled {
                if !mask.contains(house(id).phase)
                    || !world.town_allows(settlement_era(id))
                    || self.social.hold_arrival(id)
                    || evening
                    || self.occupied(world, WILLOW_VILLAGE.boundary.spawn, None, None)
                    || self.corridor_reserved(None, None)
                {
                    continue;
                }
                let walker = &mut self.people[id].walk;
                walker.at = WILLOW_VILLAGE.boundary.spawn;
                walker.redirect(home, State::MovingIn, now);
                continue;
            }
            if evening {
                if !matches!(state, State::Inside | State::Leaving | State::MovingIn) {
                    self.people[id].job = None;
                    self.people[id].job_stop = 0;
                    self.clear_order(id);
                    self.people[id].walk.redirect(home, State::Leaving, now);
                }
                if minute >= QUEST_END[id] {
                    if state == State::Inside {
                        self.people[id].stow_counter();
                        self.people[id].tool = None;
                    }
                    let reserve = SHOPS
                        .iter()
                        .find(|s| s.merchant == id && s.goods.item == Item::Food)
                        .map_or(0, |s| s.goods.quantity);
                    self.people[id].eat_home_supper(home, day, reserve);
                }
                if before {
                    visitors = visitors.saturating_sub(1);
                }
                continue;
            }
            if state == State::MovingIn || state == State::Leaving {
                continue;
            }
            if planning_guest == Some(id)
                && self.people[id].escrow.is_none()
                && self.people[id].service_material.is_none()
                && self.services.carried_asset(id as u8).is_none()
            {
                // Finish the current edge, put the seller's original lot away,
                // then keep the next work shift free for the actual appointment.
                self.people[id].job = None;
                self.people[id].job_stop = 0;
                self.clear_order(id);
                if self.people[id].walk.at == home {
                    self.people[id].stow_counter();
                } else if self.people[id].walk.target != home || !self.people[id].walk.moving() {
                    self.people[id].walk.redirect(home, State::Outbound, now);
                }
                continue;
            }
            if state == State::AtWork
                && now >= self.people[id].walk.due
                && self.people[id].returning_cargo.is_none()
                && (personal_tool(id).is_none() || self.people[id].tool.is_some())
                && (!early_setup || self.people[id].job.is_none())
            {
                self.do_work(world, id, mask, now);
                self.people[id].walk.due = now + WORK;
            }

            let weekly_shop = SHOPS.iter().take(4).position(|s| s.merchant == id);
            let preparing_weekly_meal =
                id == shop(MEAL_SHOP).merchant && self.weekly_meal_needs_preparation(world, now);
            let wants_setup = early_setup && weekly_shop.is_some();
            if wants_setup {
                let i = weekly_shop.unwrap();
                if self.people[id].walk.state == State::AtWork
                    && self.people[id].walk.at == booth_slot(i)
                    && self.people[id].counter_goods(i) >= SHOPS[i].goods.quantity
                {
                    self.setup |= 1 << i;
                }
            }
            let shop_open = (0..SHOP_COUNT).find(|&i| {
                merchant_at(i, now) == id
                    && !preparing_weekly_meal
                    && !(self.social.autumn_cook_preparing(world, id)
                        && self.people[id].count(Item::Food) < shop(i).goods.quantity)
                    && (self.business_open(i, mask, now)
                        || self.business_open(i, mask, now + REMOTE_MARKET_LEAD))
                    && (i <= MOBILE
                        || needed == Some(shop(i).goods.item)
                        || self.trip.is_some_and(|t| t.shop as usize == i))
            });
            let staging_seed =
                id == 1 && self.social.should_stage_seed_merchant(world, day) && minute < 650;
            let assigned_shop = if id == 1 && self.social.market_active(day) {
                Some(MOBILE)
            } else if staging_seed {
                None
            } else if wants_setup {
                weekly_shop
            } else {
                shop_open
            };
            let counter_stored = self.people[id].counter.is_none();
            let default_site = self.work_site(id, mask, world, now);
            let at_market = assigned_shop
                .filter(|_| {
                    self.people[id].returning_cargo.is_none()
                        && self.people[id].produce_in_transit.is_none()
                        && self.people[id].escrow.is_none()
                        && self.people[id].service_material.is_none()
                        && self.services.carried_asset(id as u8).is_none()
                })
                .and_then(|index| {
                    let s = shop(index);
                    let reserve =
                        u16::from(index == MEAL_SHOP && self.weekly_meal_demand(world, now));
                    (self.people[id].counter_goods(index) >= s.goods.quantity
                        || (counter_stored
                            && self.people[id].count(s.goods.item) >= s.goods.quantity + reserve))
                        .then_some(())
                        .filter(|_| {
                            index < MOBILE
                                || visitors
                                    < if index > MOBILE {
                                        MAX_HUMAN_VISITORS
                                    } else {
                                        usize::from(self.desired).min(MAX_HUMAN_VISITORS)
                                    }
                                || self.people[id].walk.target == booth_slot(index)
                        })
                        .map(|()| (index, reserve))
                });
            let at_market = at_market.map(|(index, reserve)| {
                let seller = &mut self.people[id];
                let booth = booth_slot(index);
                if seller.counter_goods(index) >= shop(index).goods.quantity {
                    return booth;
                }
                if seller.walk.at == home && seller.escrow.is_none() {
                    seller.pack_counter(index, reserve);
                    if seller.counter_goods(index) >= shop(index).goods.quantity {
                        return booth;
                    }
                }
                home
            });
            let contract = self.people[id]
                .job
                .and_then(|code| contracts().nth(code as usize));
            let pending_pickup = contract.is_some_and(|order| order.effect == Effect::Deliver)
                && self.people[id].escrow.is_none();
            let weave_at_home = id == 5
                && self.people[id].count(Item::Rope) < 4
                && self.people[id].produced_day != Some(day)
                && minute < 640;
            let ready_to_help = id == social_story::HELPER && self.social.helper_route(world);
            let preparing_water = self.social.prepare_visitor_water(id, day);
            let preparing_autumn = self.social.autumn_cook_preparing(world, id)
                && self.people[id].count(Item::Food) < shop(MEAL_SHOP).goods.quantity;
            let has_tailoring = id == 11
                && ((645..900).contains(&minute) || self.social.tailoring_cover_needed(day))
                && self.people[id].tailored_day != Some(day);
            let preparing_supper = matches!(id, 10 | 11)
                && self.people[id].count(Item::Food) < 3
                && (minute < 650 || id == 10 && !phase_active(mask, "cooperative_farm"));
            let destination = if at_market.is_none() && counter_out {
                if self.people[id].walk.at == home {
                    self.people[id].stow_counter();
                }
                Some(home)
            } else {
                at_market
            }
            .or_else(|| {
                contract
                    .filter(|_| {
                        !weave_at_home
                            && !ready_to_help
                            && !preparing_water
                            && !preparing_autumn
                            && !preparing_weekly_meal
                            && !has_tailoring
                            && !staging_seed
                            && !preparing_supper
                    })
                    .and_then(|c| self.job_site(world, id, c, mask))
            })
            .unwrap_or(default_site);
            let destination = if self.people[id].produce_in_transit.is_some() {
                produce_store
            } else if self.people[id].returning_cargo.is_some()
                || pending_pickup && at_market.is_none()
                || self.people[id].tool.is_none()
                    && personal_tool(id).is_some()
                    && at_market.is_none()
                    && self.people[id].escrow.is_none()
                    && self.services.carried_asset(id as u8).is_none()
            {
                home
            } else {
                destination
            };
            let destination = if self.social.is_destination_reserved(destination) {
                apron_tiles(world, destination)
                    .find(|p| {
                        !self.social.is_destination_reserved(*p)
                            && !self.occupied(world, *p, Some(id), None)
                    })
                    .unwrap_or(home)
            } else {
                destination
            };
            let destination = if main_bridge_deck(destination) {
                apron_tiles(world, destination)
                    .find(|p| !main_bridge_deck(*p) && !self.occupied(world, *p, Some(id), None))
                    .unwrap_or(home)
            } else {
                destination
            };
            if state == State::Inside && !self.occupied(world, home, Some(id), None) {
                let walker = &mut self.people[id].walk;
                walker.redirect(destination, State::Outbound, now);
            } else if state == State::Inside
                && matches!(id, 10 | 11)
                && !self.people[21 - id].walk.hidden()
                && self.people[21 - id].walk.at == home
            {
                self.observe_native(63, now);
            } else if matches!(state, State::AtWork | State::Outbound | State::Resting)
                && self.people[id].walk.target != destination
            {
                self.people[id]
                    .walk
                    .redirect(destination, State::Outbound, now);
            }
            let after = self.counts_as_visitor(id, retail_home);
            if before != after {
                visitors = if after {
                    visitors.saturating_add(1)
                } else {
                    visitors.saturating_sub(1)
                };
            }
        }
        if setup_window && self.setup == 0b1111 && world.activate_weekly_market() {
            self.evidence.weekly_day = Some(day);
            self.observe_native(50, now);
        }
        if day % 7 == 5
            && self.evidence.weekly_day != Some(day)
            && self
                .stall_visuals(world)
                .iter()
                .filter(|visual| visual.open && visual.site != "home_trade")
                .count()
                >= 4
        {
            self.evidence.weekly_day = Some(day);
            self.observe_native(50, now);
        }
        if phase_active(mask, "permanent_market") && business_open(5, mask, now) {
            let id = merchant_at(5, now);
            if self.people[id].walk.state == State::AtWork
                && self.people[id].walk.at == booth_slot(5)
            {
                if id == 6 && self.evidence.fruit_worker == Some(7) {
                    self.observe_native(58, now);
                }
                self.evidence.fruit_worker = Some(id as u8);
            }
        } else {
            self.evidence.fruit_worker = None;
        }
    }

    fn work_site(&self, id: usize, mask: PhaseMask, world: &LivingWorld, now: u64) -> TilePos {
        let day = now / DAY_MS;
        let minute = now % DAY_MS / MINUTE;
        let wet = matches!(world.weather(), YardWeather::Rain | YardWeather::Snow);
        let site = match id {
            0 => {
                if day % 7 == 0 {
                    "residence_seed_merchant.door"
                } else {
                    "market_notice"
                }
            }
            1 => {
                if self.social.should_stage_seed_merchant(world, day) && minute < 650 {
                    "residence_seed_merchant.door"
                } else if day % 7 == 1 {
                    "crop_00"
                } else {
                    "residence_seed_merchant.door"
                }
            }
            2 => {
                if wet {
                    "home_workshop.door"
                } else if day % 7 == 3 {
                    "craft_carpentry.door"
                } else {
                    "forest_loading"
                }
            }
            3 => {
                let meal_counter = shop(MEAL_SHOP);
                let needed = meal_counter.goods.quantity + 1;
                if (self.weekly_meal_needs_preparation(world, now)
                    || self.social.autumn_cook_preparing(world, id)
                        && self.people[id].count(Item::Food) <= meal_counter.goods.quantity)
                    && self.people[id].count(Item::Crops) < needed
                {
                    "orchard_tools"
                } else {
                    "home_kitchen.door"
                }
            }
            4 => {
                if wet {
                    "residence_fisher.door"
                } else {
                    "creek_fishing"
                }
            }
            5 => {
                if self.people[id].count(Item::Rope) < 4
                    && self.people[id].produced_day != Some(day)
                    && minute < 640
                {
                    house(id).door_poi
                } else {
                    "craft_textile.door"
                }
            }
            6 => {
                if minute < 870 {
                    "orchard_tools"
                } else {
                    "orchard_rest"
                }
            }
            7 => {
                if self.social.helper_route(world) {
                    "home_workshop.door"
                } else if wet {
                    "residence_florist.door"
                } else {
                    "commons_garden"
                }
            }
            8 => {
                if wet {
                    "quarry_shelter.door"
                } else if day % 7 == 6 {
                    "quarry_view"
                } else {
                    "home_fishing"
                }
            }
            9 => {
                if wet {
                    "bridge_wait_e"
                } else if day % 7 == 3 {
                    "bridge_wait_w"
                } else {
                    "quarry_work"
                }
            }
            10 => {
                if self.people[id].count(Item::Food) < 3
                    && self.people[id].count(Item::Crops) >= 2
                    && (minute < 650 || !phase_active(mask, "cooperative_farm"))
                {
                    house(id).door_poi
                } else if phase_active(mask, "cooperative_farm") && day % 7 == 2 {
                    "crop_09"
                } else {
                    farmer_production_site(mask)
                }
            }
            11 => {
                if self.social.tailoring_cover_needed(day) && (645..950).contains(&minute) {
                    "craft_textile.door"
                } else if minute < 620 && self.people[id].count(Item::Food) < 3 {
                    if self.people[id].count(Item::Crops) >= 2 {
                        house(id).door_poi
                    } else {
                        "orchard_tools"
                    }
                } else {
                    "craft_textile.door"
                }
            }
            12 => {
                if wet {
                    "east_duplex_2.door"
                } else {
                    "future_156_100"
                }
            }
            13 => {
                if minute < 660 {
                    "market_loading"
                } else {
                    "market_notice"
                }
            }
            14 => "craft_bakery.door",
            15 => {
                if wet {
                    "craft_textile.door"
                } else {
                    "wetland_observe"
                }
            }
            16 => {
                if wet && !self.social.prepare_visitor_water(id, day) {
                    "crop_18"
                } else {
                    "home_well"
                }
            }
            17 => {
                if wet {
                    "quarry_shelter.door"
                } else if phase_active(mask, "late") {
                    "wetland_observe"
                } else {
                    "quarry_view"
                }
            }
            18 => "craft_pottery.door",
            _ => {
                if day % 7 == 1 {
                    "outside_gate"
                } else {
                    "craft_post.door"
                }
            }
        };
        let p = self
            .interaction_position(mask, site)
            .unwrap_or_else(|| house(id).door);
        if shared_apron(site) {
            return apron_tiles(world, p)
                .find(|p| !self.occupied(world, *p, Some(id), None))
                .unwrap_or_else(|| house(id).door);
        }
        // A resident has a distinct workplace side when its route stops at
        // another household's door. Leave the actual entrance available.
        if WILLOW_VILLAGE.buildings.iter().any(|b| b.door == p) {
            return [(-1, 1), (1, 1), (0, 2), (-2, 1), (2, 1)]
                .into_iter()
                .cycle()
                .skip(id % 5)
                .take(5)
                .filter_map(|(dx, dy)| p.offset(dx, dy))
                .find(|p| {
                    world.can_walk(i32::from(p.x), i32::from(p.y))
                        && !WILLOW_VILLAGE.buildings.iter().any(|b| b.door == *p)
                })
                .unwrap_or(p);
        }
        p
    }

    fn admit(&mut self, world: &LivingWorld, mask: PhaseMask, now: u64) {
        let world_window = now / (3 * HOUR);
        self.next_guest = now + (15 + mix(now / MINUTE) % 6) * MINUTE;
        if self.last_window != Some(world_window) {
            self.last_window = Some(world_window);
            self.desired = guest_goal(world, mask, world_window).min(visitor_cap(world));
        }
        let minute = (now % DAY_MS / MINUTE) as u16;
        let is_rain = matches!(world.weather(), YardWeather::Rain | YardWeather::Snow);
        let cap = if is_rain {
            self.desired.min(2)
        } else {
            self.desired
        };
        if self.present() >= usize::from(cap)
            || !(420..1080).contains(&minute)
            || self.occupied(world, WILLOW_VILLAGE.boundary.spawn, None, None)
            || world.can_walk(191, 56) == false
            || self.corridor_reserved(None, None)
        {
            return;
        }
        let Some(index) = self.guests.iter().position(Option::is_none) else {
            return;
        };
        let bed = if minute >= 950 && index % 2 == 0 {
            (0..3).find(|&bed| {
                phase_active(
                    mask,
                    if bed < 2 {
                        "village_development"
                    } else {
                        "later_guest_home"
                    },
                ) && self
                    .guests
                    .iter()
                    .flatten()
                    .all(|guest| guest.bed != Some(bed))
                    && !(bed < 2 && self.social.uses_guest_bed())
            })
        } else {
            None
        };
        let mut walk = Walker::new(WILLOW_VILLAGE.boundary.spawn, State::Visiting);
        walk.redirect(TilePos::new(188, 56), State::Visiting, now);
        let source = &mut self.remaining_source[item_index(Item::Rope)];
        let cargo = if index % 3 == 0 && *source > 0 {
            *source -= 1;
            Some(cargo(Item::Rope, 1))
        } else {
            None
        };
        self.guests[index] = Some(Guest {
            identity: (mix(now / MINUTE ^ 0x197d) as usize % RESIDENT_COUNT) as u8,
            walk,
            leave_by: now + 3 * HOUR,
            cargo,
            bed,
            wake: None,
            received: false,
        });
    }

    fn weekly_meal_demand(&self, world: &LivingWorld, now: u64) -> bool {
        self.social.regular_market_week(world, now)
            && Self::meal_reserves_allow_exchange(world, false)
    }

    fn weekly_meal_needs_preparation(&self, world: &LivingWorld, now: u64) -> bool {
        let counter = shop(MEAL_SHOP);
        self.weekly_meal_demand(world, now)
            && self.people[counter.merchant].count(Item::Food) <= counter.goods.quantity
    }

    fn meal_reserves_allow_exchange(world: &LivingWorld, price_is_packed: bool) -> bool {
        let unpacked = if price_is_packed {
            0
        } else {
            shop(MEAL_SHOP).cost.quantity
        };
        world.stock().count(Item::Crops) >= 6 + unpacked && world.stock().count(Item::Food) <= 8
    }

    fn offer_trip(&mut self, world: &mut LivingWorld, mask: PhaseMask, now: u64) {
        if self.trip.is_some()
            || self.social.host().is_some()
            || self.social.market_planning_guest().is_some()
            || !(450..960).contains(&(now % DAY_MS / MINUTE))
            || matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            return;
        }
        let active_session = self.social.market_active(now / DAY_MS);
        let reserve = self.social.crop_reserve_for_market(world);
        let proposals = [MOBILE + 1, MOBILE + 2, MOBILE + 3, 1, 2, 3, 4, 0, 5, MOBILE];
        let needed = world.next_construction_material_shortfall();
        let trial_market = self.social.fixed_market_needs_sale(world)
            || self.social.autumn_market_needs_sale(world)
            || self.social.maturity_needs_sale(world);
        let weekly_meal = self.weekly_meal_demand(world, now);
        let surplus_order = world.village_chapter_completed("Q15")
            && !world.village_chapter_completed("Q16")
            && world.stock().count(Item::Crops) >= 8
            && world.stock().count(Item::Food) >= 6
            && world.stock().count(Item::Seeds) >= 2;
        let selected = active_session
            .then_some(MOBILE)
            .into_iter()
            .chain(proposals)
            .find(|&i| {
                let s = shop(i);
                let merchant = &self.people[merchant_at(i, now)];
                let budget = if i >= MOBILE {
                    LOCAL_MARKET_LEAD
                } else {
                    REMOTE_MARKET_LEAD
                };
                self.business_open(i, mask, now + budget)
                    && market_cost_available(world, s.cost)
                    && world.stock_can_accept(s.goods.item, s.goods.quantity)
                    && (!self.social.neighbour_food_needed()
                        || s.cost.item != Item::Food
                        || world.stock().count(Item::Food) >= 6 + s.cost.quantity
                        || needed.is_some_and(|(item, _)| item == s.goods.item))
                    && (s.cost.item != Item::Crops
                        || reserve.is_none()
                        || i == MOBILE && active_session
                        || matches!(i, 1 | 2) && trial_market
                        || i > MOBILE && needed.is_some_and(|(item, _)| item == s.goods.item)
                        || s.goods.item == Item::Food && world.stock().count(Item::Food) < 3
                        || s.goods.item == Item::Seeds && world.stock().count(Item::Seeds) < 2
                        || world.stock().count(Item::Crops) >= reserve.unwrap() + s.cost.quantity)
                    && (i == MOBILE && world.stock().count(Item::Seeds) < 6
                        || i == MOBILE && self.social.market_active(now / DAY_MS)
                        || matches!(i, 1 | 2)
                            && trial_market
                            && world.stock().count(Item::Crops) >= 8
                        || i == MEAL_SHOP && weekly_meal
                        || i <= MOBILE && surplus_order && s.cost.item == Item::Crops
                        || i > MOBILE && needed.is_some_and(|(item, _)| item == s.goods.item)
                        || i < MOBILE
                            && self.market_outings == 0
                            && now / DAY_MS % 7 == 5
                            && i == 1
                        || i < MOBILE
                            && world.stock().count(s.goods.item)
                                < if s.goods.item == Item::Crops { 4 } else { 6 })
                    && merchant.ready_to_exchange(i, i == MEAL_SHOP && weekly_meal)
                    && merchant.walk.state == State::AtWork
                    && merchant.walk.at == booth_slot(i)
                    && (i != 4
                        || self
                            .services
                            .notice("fishing_suspend")
                            .is_none_or(|(v, issued)| v != 2 || now.saturating_sub(issued) >= HOUR)
                            && self
                                .services
                                .notice("fishing_window")
                                .is_none_or(|(v, issued)| {
                                    v != 0 || now.saturating_sub(issued) >= HOUR
                                }))
            });
        let Some(index) = selected else {
            return;
        };
        let Some(load) = slot(mask, "pantry_loading", "load") else {
            return;
        };
        let s = shop(index);
        let candidates = if s.goods.item == Item::Seeds || s.goods.item == Item::Food {
            [ActorKind::Xiaohe, ActorKind::Ayu]
        } else {
            [ActorKind::Ayu, ActorKind::Xiaohe]
        };
        let Some(actor) = candidates.into_iter().find(|&actor| {
            let p = world.actor_tile(actor);
            p.x < 52 && p.y < 64 && world.reserve_market_trip(actor)
        }) else {
            self.next_trip = now + STEP;
            return;
        };
        if !world.move_for_activity(actor, point(load)) {
            world.end_activity(&[actor], 0);
            return;
        }
        self.reserved = Some(actor);
        if index < MOBILE {
            self.services.acknowledge("market_hours");
            self.services.acknowledge("bridge_load");
        }
        let regular_meal = index == MEAL_SHOP && weekly_meal;
        self.trip = Some(Trip {
            actor,
            shop: index as u8,
            started: now,
            due: now + STEP,
            expires: now + MARKET_TRAVEL_WINDOW,
            stage: TripStage::Pack,
            target: load,
            bag: None,
            outgoing: s.cost,
            incoming: s.goods,
            accepted: false,
            leg: 0,
            hold: true,
            regular_meal,
        });
    }

    fn advance_trip(&mut self, world: &mut LivingWorld) {
        let Some(mut t) = self.trip.take() else {
            return;
        };
        let now = world.world_ms();
        if now < t.due {
            self.trip = Some(t);
            return;
        }
        let mask = world.phase_mask();
        let shop_id = usize::from(t.shop);
        let s = shop(shop_id);
        if t.stage == TripStage::Inbound
            && t.hold
            && now >= t.expires.saturating_add(2 * HOUR)
            && world.activity_deadline(t.actor).is_some()
        {
            // Let the granted edge finish and pause for household needs.
            // Escrow remains attached to the same carrier and is recovered
            // when that actor can resume the route.
            self.release_trip(world, t.actor, 0);
            t.hold = false;
            t.expires = now + HOUR;
            t.due = now + RETRY;
            self.trip = Some(t);
            return;
        }
        let arrived =
            t.hold && world.activity_arrived(t.actor) && world.actor_tile(t.actor) == t.target;
        let idle = t.hold && world.activity_arrived(t.actor);
        let wet = matches!(world.weather(), YardWeather::Rain | YardWeather::Snow);
        t.due = now + STEP;
        if t.hold && world.activity_deadline(t.actor).is_none() {
            self.reserved = None;
            t.hold = false;
            if t.bag.is_none() {
                return;
            }
        }
        if !t.hold {
            if t.stage == TripStage::Unload
                && t.bag
                    .is_some_and(|bag| !world.stock_can_accept(bag.item, bag.quantity))
            {
                t.due = now + 15 * MINUTE;
                self.trip = Some(t);
                return;
            }
            if world.reserve_market_trip(t.actor) {
                self.reserved = Some(t.actor);
                t.hold = true;
                t.stage = TripStage::Inbound;
                self.send_home(world, &mut t, mask);
            } else {
                t.due = now + RETRY;
            }
            self.trip = Some(t);
            return;
        }
        if matches!(t.stage, TripStage::Outbound | TripStage::Exchange)
            && (wet || now >= t.expires || !self.business_open(shop_id, mask, now))
        {
            if wet && shop_id == 4 {
                self.observe_native(45, now);
            }
            if !self.business_open(shop_id, mask, now) && shop_id == 3 {
                self.observe_native(59, now);
            }
            if (wet || now >= t.expires) && !idle {
                // The household retains the step already granted; our
                // escrow follows the same owner until it can return safely.
                self.release_trip(world, t.actor, 0);
                t.hold = false;
                t.stage = TripStage::Inbound;
                t.due = now + STEP;
                self.trip = Some(t);
                return;
            } else if idle {
                t.stage = TripStage::Inbound;
                self.send_home(world, &mut t, mask);
            }
        }
        match t.stage {
            TripStage::Pack if arrived => {
                if wet
                    || now >= t.expires
                    || !market_cost_available(world, t.outgoing)
                    || t.regular_meal && !self.weekly_meal_demand(world, now)
                    || !world.spend_stock(t.outgoing.item, t.outgoing.quantity)
                {
                    self.release_trip(world, t.actor, 0);
                    return;
                }
                t.bag = Some(t.outgoing);
                let target = if shop_id >= MOBILE {
                    slot(mask, s.id, "queue0").or_else(|| slot(mask, s.id, "customer"))
                } else {
                    Some(OUTBOUND_WAYPOINTS[0])
                };
                if let Some(target) = target
                    && world.move_for_activity_with_load(t.actor, point(target), true)
                {
                    t.stage = TripStage::Outbound;
                    t.leg = if shop_id >= MOBILE {
                        (MARKET_ENTRY_LEG + 1) as u8
                    } else {
                        0
                    };
                    t.target = target;
                } else {
                    t.stage = TripStage::Unload;
                }
            }
            TripStage::Pack if idle => {
                if now >= t.expires || !world.move_for_activity(t.actor, point(t.target)) {
                    self.release_trip(world, t.actor, 0);
                    return;
                }
            }
            TripStage::Outbound if arrived && usize::from(t.leg) <= MARKET_ENTRY_LEG + 1 => {
                if t.target == TilePos::new(68, 42) && shop_id < MOBILE {
                    self.evidence.bridge_departure = Some(t.started);
                }
                let leg = usize::from(t.leg) + 1;
                let target = match leg {
                    n if n < MARKET_ENTRY_LEG => Some(OUTBOUND_WAYPOINTS[n]),
                    n if n == MARKET_ENTRY_LEG => Self::market_entry(mask),
                    n if n == MARKET_ENTRY_LEG + 1 => slot(mask, s.id, "queue0"),
                    _ => slot(mask, s.id, "customer"),
                };
                self.step_trip(world, &mut t, target, leg as u8);
            }
            TripStage::Outbound if arrived => {
                world.external_pose(t.actor as usize, Pose::Work);
                t.stage = TripStage::Exchange;
                t.due = now + MINUTE;
            }
            TripStage::Outbound if idle => {
                if !world.move_for_activity_with_load(t.actor, point(t.target), t.bag.is_some()) {
                    t.stage = TripStage::Inbound;
                    self.send_home(world, &mut t, mask);
                }
            }
            TripStage::Exchange if arrived => {
                let sufficient_reserve =
                    !t.regular_meal || Self::meal_reserves_allow_exchange(world, true);
                let merchant = &mut self.people[merchant_at(shop_id, now)];
                if merchant.walk.state == State::AtWork
                    && merchant.walk.at == booth_slot(shop_id)
                    && merchant.ready_to_exchange(shop_id, t.regular_meal)
                    && t.bag == Some(t.outgoing)
                    && sufficient_reserve
                {
                    let paid = merchant.incoming(t.outgoing);
                    assert!(paid);
                    let batch = merchant
                        .counter
                        .take()
                        .expect("counter sold its reserved batch");
                    assert_eq!(batch.cargo, t.incoming);
                    t.bag = Some(t.incoming);
                    t.accepted = true;
                    self.trades = self.trades.saturating_add(1);
                    self.services.wear(16);
                    if t.outgoing.item == Item::Crops {
                        self.services.wear(2);
                    }
                    world.note_village_trade();
                    self.social.witness_sale(shop_id, world, now);
                    if shop_id == MOBILE {
                        self.observe_native(49, now);
                    } else if shop_id == 1 {
                        self.observe_native(51, now);
                    } else if shop_id == 2 {
                        self.observe_native(55, now);
                    } else if shop_id == 4
                        && self.people[merchant_at(shop_id, now)].counter_goods(shop_id) == 0
                    {
                        self.observe_native(54, now);
                    }
                }
                t.stage = TripStage::Inbound;
                self.send_home(world, &mut t, mask);
            }
            TripStage::Inbound if arrived && usize::from(t.leg) < INBOUND_WAYPOINTS.len() => {
                let leg = usize::from(t.leg) + 1;
                let target = INBOUND_WAYPOINTS
                    .get(leg)
                    .copied()
                    .or_else(|| slot(mask, "pantry_loading", "load"));
                self.step_trip(world, &mut t, target, leg as u8);
            }
            TripStage::Inbound if arrived => {
                t.stage = TripStage::Unload;
                t.due = now + STEP;
            }
            TripStage::Inbound if idle => {
                let _ =
                    world.move_for_activity_with_load(t.actor, point(t.target), t.bag.is_some());
            }
            TripStage::Unload if idle => {
                let dock = slot(mask, "pantry_loading", "load").expect("pantry dock");
                if world.actor_tile(t.actor) != dock {
                    t.target = dock;
                    world.move_for_activity_with_load(t.actor, point(dock), t.bag.is_some());
                    self.trip = Some(t);
                    return;
                }
                let received = t
                    .bag
                    .is_none_or(|g| world.exchange_stock_atomic(&[], &[(g.item, g.quantity)]));
                if t.hold {
                    self.release_trip(world, t.actor, 4);
                    t.hold = false;
                }
                if received {
                    if t.accepted {
                        world.note_village_surplus_trade(t.outgoing.item, t.outgoing.quantity);
                    }
                    self.trips = self.trips.saturating_add(1);
                    if shop_id < MOBILE {
                        self.market_outings = self.market_outings.saturating_add(1);
                        if self.evidence.bridge_departure == Some(t.started)
                            && self.market_outings == 1
                        {
                            self.observe_native(37, now);
                        }
                    }
                    return;
                }
                t.due = now + 15 * MINUTE;
                self.observe_native(8, now);
            }
            _ => {}
        }
        self.trip = Some(t);
    }

    fn send_home(&self, world: &mut LivingWorld, t: &mut Trip, mask: PhaseMask) {
        let p = world.actor_tile(t.actor);
        let leg = if usize::from(t.shop) >= MOBILE {
            INBOUND_WAYPOINTS.len()
        } else if p.x >= 68 {
            0
        } else if p.x >= 56 && p.y >= 42 {
            1
        } else if p.x >= 53 {
            2
        } else {
            INBOUND_WAYPOINTS.len()
        };
        let target = INBOUND_WAYPOINTS
            .get(leg)
            .copied()
            .or_else(|| slot(mask, "pantry_loading", "load"));
        if let Some(target) = target {
            t.leg = leg as u8;
            t.target = target;
            let _ = world.move_for_activity_with_load(t.actor, point(target), t.bag.is_some());
        }
    }

    fn market_entry(mask: PhaseMask) -> Option<TilePos> {
        resolve(mask, "market_gate")
    }

    fn step_trip(&self, world: &mut LivingWorld, t: &mut Trip, target: Option<TilePos>, leg: u8) {
        if let Some(target) = target
            && !self.occupied_by_village(target)
            && world.move_for_activity_with_load(t.actor, point(target), t.bag.is_some())
        {
            t.target = target;
            t.leg = leg;
        }
    }

    fn release_trip(&mut self, world: &mut LivingWorld, actor: ActorKind, effort: u8) {
        world.end_activity(&[actor], effort);
        self.reserved = None;
    }

    fn occupied_by_village(&self, p: TilePos) -> bool {
        self.people
            .iter()
            .map(|r| &r.walk)
            .chain(self.guests.iter().flatten().map(|g| &g.walk))
            .any(|w| !w.hidden() && (w.at == p || w.edge.is_some_and(|e| e.to == p)))
    }

    fn business_open(&self, index: usize, mask: PhaseMask, time: u64) -> bool {
        if index == MOBILE && self.social.market_active(time / DAY_MS) {
            return (535..760).contains(&(time % DAY_MS / MINUTE));
        }
        if index == MOBILE && self.social.market_shuttered(time / DAY_MS) {
            return false;
        }
        business_open(index, mask, time)
    }

    fn interaction_position(&self, mask: PhaseMask, id: &str) -> Option<TilePos> {
        match id {
            "home_trade" => slot(mask, id, "merchant"),
            "home_well" => slot(mask, id, "draw"),
            "home_table" => slot(mask, id, "seat_a"),
            "pantry_loading" => slot(mask, id, "load"),
            _ => slot(mask, id, "merchant").or_else(|| resolve(mask, id)),
        }
    }

    fn contract_site(
        &self,
        world: &LivingWorld,
        c: Contract,
        mask: PhaseMask,
        next: u8,
    ) -> Option<TilePos> {
        if !c.phases.split('+').all(|p| phase_active(mask, p)) {
            return None;
        }
        let id = c.sites.split(';').nth(usize::from(next))?;
        let pos = self.interaction_position(mask, id)?;
        let occupant = c.actor;
        if self.social.is_destination_reserved(pos) {
            return apron_tiles(world, pos).find(|p| {
                !self.social.is_destination_reserved(*p)
                    && !self.occupied(world, *p, occupant, None)
            });
        }
        // Leave gate and crossing cells available while a resident stops
        // to work. The work still takes place beside the authored anchor.
        if shared_apron(id) {
            return apron_tiles(world, pos).find(|p| !self.occupied(world, *p, occupant, None));
        }
        if id == "pantry_loading" {
            // NPC consultations and handoffs leave the loading stance and
            // its tray accessible whenever the household is using them.
            let reserved = |p| {
                HOMEBOUND[..2]
                    .iter()
                    .any(|&kind| world.actor_destination(kind) == Some(p))
                    || self.trip.is_some_and(|t| t.target == p)
                    || self.occupied(world, p, occupant, None)
            };
            if reserved(pos) {
                let tray = slot(mask, id, "tray");
                return apron_tiles(world, pos)
                    .find(|&p| Some(p) != tray && !pantry_service_zone(p) && !reserved(p));
            }
        }
        if !self.occupied(world, pos, occupant, None) {
            return Some(pos);
        }
        [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (-1, 1)]
            .into_iter()
            .filter_map(|(dx, dy)| pos.offset(dx, dy))
            .find(|p| {
                world.can_walk(i32::from(p.x), i32::from(p.y))
                    && !self.occupied(world, *p, occupant, None)
            })
    }

    fn service_materials(&self, c: Contract) -> Option<(Cargo, bool)> {
        services::definitions()
            .find(|service| service.id == c.id)?
            .op
            .materials()
    }

    fn schedule_neighbour_meal(
        &mut self,
        world: &mut LivingWorld,
        id: usize,
        mask: PhaseMask,
        now: u64,
    ) -> bool {
        let day = now / DAY_MS;
        let minute = world.minute_of_day();
        let home = house(id).door;
        let resident = &mut self.people[id];
        let dock = slot(mask, "pantry_loading", "load").expect("pantry dock");
        if let Some(meal) = resident.neighbour_meal {
            let deliver =
                meal.packed_day == day && world.stock().count(Item::Food) < NEIGHBOUR_PANTRY_TARGET;
            let target = if deliver { dock } else { home };
            if resident.walk.at != target {
                if resident.walk.target != target || !resident.walk.moving() {
                    resident.walk.redirect(target, State::Outbound, now);
                }
                return true;
            }
            if deliver && !world.exchange_stock_atomic(&[], &[(Item::Food, meal.portions)]) {
                return true;
            }
            resident.neighbour_meal = None;
            if !deliver {
                assert!(resident.incoming(cargo(Item::Food, meal.portions)));
            }
            return false;
        }
        if !self.social.neighbour_food_needed()
            || !(650..850).contains(&minute)
            || world.stock().count(Item::Food) >= 8
            || resident.neighbour_meal_day == Some(day)
            || matches!(
                resident.walk.state,
                State::Unsettled | State::MovingIn | State::Leaving | State::Away
            )
            || resident.count(Item::Crops) < NEIGHBOUR_MEAL_PORTIONS + 2
            || resident.service_material.is_some()
            || resident.escrow.is_some()
            || resident.produce_in_transit.is_some()
            || self.services.carried_asset(id as u8).is_some()
        {
            return false;
        }
        if resident.walk.at != home {
            if resident.walk.target != home || !resident.walk.moving() {
                resident.walk.redirect(home, State::Outbound, now);
            }
            return true;
        }
        if resident.walk.state != State::AtWork || resident.walk.due > now {
            return resident.walk.state == State::AtWork;
        }
        if resident.count(Item::Food) < 2 {
            assert!(resident.outgoing(cargo(Item::Crops, 2)));
            assert!(resident.incoming(cargo(Item::Food, 2)));
        }
        let batch = cargo(Item::Food, NEIGHBOUR_MEAL_PORTIONS);
        if !resident.can_accept(batch) || !resident.outgoing(cargo(Item::Crops, batch.quantity)) {
            return false;
        }
        resident.neighbour_meal = Some(NeighbourMeal {
            portions: batch.quantity,
            packed_day: day,
        });
        resident.job = None;
        resident.job_stop = 0;
        self.orders[id] = None;
        resident.neighbour_meal_day = Some(day);
        resident.walk.redirect(dock, State::Outbound, now);
        true
    }

    fn material_source(&self, mask: PhaseMask, id: usize, household: bool) -> Option<TilePos> {
        if household {
            self.interaction_position(mask, "home_well")
        } else {
            Some(house(id).door)
        }
    }

    fn return_service_material(
        &mut self,
        world: &mut LivingWorld,
        id: usize,
        mask: PhaseMask,
        now: u64,
    ) -> bool {
        let Some(batch) = self.people[id]
            .service_material
            .filter(|batch| batch.returning)
        else {
            return true;
        };
        let Some(source) = self.material_source(mask, id, batch.household) else {
            return false;
        };
        let walk = &mut self.people[id].walk;
        if walk.at != source {
            if walk.target != source || !walk.moving() {
                walk.redirect(source, State::Outbound, now);
            }
            return false;
        }
        if batch.household
            && !world.exchange_stock_atomic(&[], &[(batch.cargo.item, batch.cargo.quantity)])
        {
            // Wait here only until another user may need the well; the visible
            // water stays with this carrier and becomes eligible next schedule.
            return false;
        }
        self.people[id].service_material = None;
        if !batch.household {
            assert!(self.people[id].incoming(batch.cargo));
        }
        true
    }

    fn asset_pickup_site(
        &self,
        world: &LivingWorld,
        c: Contract,
        mask: PhaseMask,
    ) -> Option<TilePos> {
        let ServiceOp::Move(asset, owner, _) = services::definitions().find(|s| s.id == c.id)?.op
        else {
            return None;
        };
        let site = if let Some(station) = self.services.stored_at(asset) {
            station
        } else if usize::from(owner) < RESIDENT_COUNT {
            house(usize::from(owner)).door
        } else {
            self.contract_site(world, c, mask, 0)?
        };
        world.can_walk(site.x.into(), site.y.into()).then_some(site)
    }

    fn job_site(
        &self,
        world: &LivingWorld,
        id: usize,
        c: Contract,
        mask: PhaseMask,
    ) -> Option<TilePos> {
        if let Some((_, household)) = self.service_materials(c)
            && self.people[id].service_material.is_none()
        {
            return self.material_source(mask, id, household);
        }
        if let Some(services::Definition {
            op: ServiceOp::Move(asset, _, _),
            ..
        }) = services::definitions().find(|service| service.id == c.id)
            && !self.services.carried_by(asset, id as u8)
        {
            self.asset_pickup_site(world, c, mask)
        } else {
            self.contract_site(world, c, mask, self.people[id].job_stop)
        }
    }

    fn do_work(&mut self, world: &mut LivingWorld, id: usize, mask: PhaseMask, now: u64) {
        let day = now / DAY_MS;
        let minute = now % DAY_MS / MINUTE;
        if self.social.prepare_visitor_water(id, day) {
            self.social
                .fill_visitor_water(world, self.people[id].walk.at);
        }
        if matches!(id, 10 | 11)
            && self.people[id].prepared_meal_day != Some(day)
            && distance(self.people[id].walk.at, house(id).door) <= 3
            && self.people[id].count(Item::Food) < 3
            && self.convert(id, cargo(Item::Crops, 2), cargo(Item::Food, 2))
        {
            self.people[id].prepared_meal_day = Some(day);
        }
        if id == 11
            && self.people[id].foraged_food_day != Some(day)
            && self.people[id].count(Item::Food) < 3
            && self
                .interaction_position(mask, "orchard_tools")
                .is_some_and(|p| distance(self.people[id].walk.at, p) <= 3)
        {
            let source = &mut self.remaining_source[item_index(Item::Crops)];
            if *source >= 2 && self.people[id].gather_produce(cargo(Item::Crops, 2)) {
                *source -= 2;
                self.people[id].foraged_food_day = Some(day);
            }
        }
        if self.people[id].produced_day != Some(day) && self.production(world, id, mask) {
            self.people[id].produced_day = Some(day);
        }
        if id == 11
            && self.people[id].tailored_day != Some(day)
            && self
                .interaction_position(mask, "craft_textile.door")
                .is_some_and(|p| distance(self.people[id].walk.at, p) <= 3)
        {
            let input = item_index(Item::Crops);
            let has_pending_repair = self.services.damaged(14);
            let repaired = has_pending_repair
                && self.people[id].count(Item::Rope) > 0
                && self.services.repair(14, 1)
                && self.people[id].outgoing(cargo(Item::Rope, 1));
            let produced = !repaired
                && self.remaining_source[input] > 0
                && self.people[id].count(Item::Rope) < HARD_STOCK_CAP
                && self.people[id].incoming(cargo(Item::Rope, 1));
            if repaired || produced {
                if produced {
                    self.remaining_source[input] -= 1;
                }
                self.people[id].tailored_day = Some(day);
                self.social.note_neighbour_work(id, day, false);
            }
        }
        if (MOBILE + 1..SHOP_COUNT).any(|index| {
            let supplier = shop(index);
            id == supplier.merchant
                && self.people[id].walk.at == booth_slot(index)
                && business_open(index, mask, now)
                && (world
                    .next_construction_material_shortfall()
                    .is_some_and(|(item, _)| item == supplier.goods.item)
                    || self
                        .trip
                        .is_some_and(|trip| usize::from(trip.shop) == index))
        }) {
            return;
        }
        if let Some(code) = self.people[id].job
            && let Some(c) = contracts().nth(usize::from(code))
        {
            if self.orders[id].is_none_or(|o| now > o.opened + 5 * HOUR) {
                self.finish_daily_order(id, day);
                return;
            }
            if let Some((batch, household)) = self.service_materials(c)
                && self.people[id].service_material.is_none()
            {
                if self.material_source(mask, id, household) == Some(self.people[id].walk.at) {
                    let taken = if household {
                        world.spend_stock(batch.item, batch.quantity)
                    } else {
                        self.people[id].outgoing(batch)
                    };
                    if taken {
                        self.people[id].service_material = Some(ServiceMaterial {
                            cargo: batch,
                            household,
                            returning: false,
                        });
                    }
                }
                return;
            }
            if let Some(services::Definition {
                op: ServiceOp::Move(asset, owner, _),
                ..
            }) = services::definitions().find(|service| service.id == c.id)
                && !self.services.carried_by(asset, id as u8)
            {
                let at = self.people[id].walk.at;
                if self.asset_pickup_site(world, c, mask) == Some(at) {
                    self.services.assign_carrier(asset, owner, id as u8, at);
                }
                return;
            }
            let stop = self.people[id].job_stop;
            let at_stop = self
                .contract_site(world, c, mask, stop)
                .is_some_and(|p| distance(p, self.people[id].walk.at) <= 1);
            if !at_stop {
                return;
            }
            if c.id == TWO_FIELD_WATERING {
                if let Some(plot) = c
                    .sites
                    .split(';')
                    .nth(usize::from(stop))
                    .and_then(plot_index)
                    && world.village_local_work("water", Some(plot))
                {
                    if stop == 0 {
                        self.people[id].job_stop = 1;
                        return;
                    }
                    self.record(code, now);
                }
                self.finish_daily_order(id, day);
                return;
            }
            if c.effect != Effect::Plot
                && c.sites.split(';').nth(usize::from(stop) + 1).is_some()
                && self.contract_site(world, c, mask, stop + 1).is_some()
            {
                self.people[id].job_stop += 1;
                return;
            }
            if self.apply_contract(world, id, c, now) {
                self.record(code, now);
            } else if let Some(order) = &mut self.orders[id]
                && order.attempts < 2
                && now < order.opened + 3 * HOUR
            {
                order.attempts += 1;
                return;
            }
            self.finish_daily_order(id, day);
        }
        if self.people[id].break_day == Some(day)
            || !(480..960).contains(&minute)
            || (0..SHOP_COUNT).any(|i| merchant_at(i, now) == id && business_open(i, mask, now))
        {
            return;
        }
        let weekday = day % 7;
        let rotation = ((day / 7 + id as u64) % 3 * 7 + weekday) % 10;
        let officer = id as u64;
        let personal = id * 10 + rotation as usize;
        let scheduled = contracts()
            .enumerate()
            .skip(200)
            .filter(|(_, c)| c.actor == Some(id) && c.effect != Effect::Native)
            .collect::<Vec<_>>();
        let code = if !scheduled.is_empty() && (day + officer) % 3 == 0 {
            scheduled[(day as usize / 3 + id) % scheduled.len()].0
        } else {
            personal
        };
        let candidate = std::iter::once(code)
            .chain((0..10).map(|offset| id * 10 + (rotation as usize + offset) % 10))
            .find(|&candidate| {
                contracts()
                    .nth(candidate)
                    .is_some_and(|c| self.can_accept_job(world, id, c, mask, now))
            });
        if let Some(code) = candidate
            && let Some(c) = contracts().nth(code)
            && c.effect != Effect::Native
            && (!c.sites.split(';').any(|s| s == "home_trade")
                || self.present() < usize::from(self.desired))
            && self.contract_site(world, c, mask, 0).is_some()
        {
            if c.effect == Effect::Deliver {
                if self.people[id].escrow.is_some() || self.people[id].returning_cargo.is_some() {
                    return;
                }
            }
            self.people[id].job = Some(code as u16);
            self.people[id].job_stop = 0;
            self.orders[id] = Some(WorkOrder {
                opened: now,
                attempts: 0,
            });
        }
    }

    fn can_accept_job(
        &self,
        world: &LivingWorld,
        id: usize,
        c: Contract,
        mask: PhaseMask,
        now: u64,
    ) -> bool {
        if c.effect == Effect::Native
            || self.contract_site(world, c, mask, 0).is_none()
            || c.sites.split(';').any(|s| s == "home_trade")
                && self.present() >= usize::from(self.desired)
        {
            return false;
        }
        if let Some(site) = c.sites.split(';').last()
            && self.interaction_position(mask, site).is_none()
        {
            return false;
        }
        if c.id == TWO_FIELD_WATERING {
            return world.stock().count(Item::Water) >= 2
                && c.sites.split(';').all(|site| {
                    plot_index(site).is_some_and(|plot| world.village_plot_needs_water(plot))
                });
        }
        if c.id == "ME003" && !world.cat_bowl_needs_cleaning() {
            return false;
        }
        let have = |cargo: Cargo| self.people[id].count(cargo.item) >= cargo.quantity;
        match c.effect {
            Effect::Service => services::definitions()
                .find(|def| def.id == c.id)
                .is_some_and(|d| match d.op {
                    ServiceOp::Mend(f, b) => self.services.damaged(f) && have(b),
                    ServiceOp::Prepare(asset, b) => {
                        have(b) && self.services.room_for(asset, id as u8)
                    }
                    ServiceOp::Home(_, b) => have(b),
                    ServiceOp::HomeStock(_, b) => world.stock().count(b.item) >= b.quantity,
                    ServiceOp::Move(asset, owner, _) => {
                        self.services.holds(asset, owner)
                            && self.asset_pickup_site(world, c, mask).is_some()
                    }
                    ServiceOp::Publish(board, m) => self
                        .service_metric(
                            world,
                            c.sites.split(';').last().unwrap_or_default(),
                            m,
                            now,
                        )
                        .is_some_and(|val| self.services.new_notice(board, val)),
                    ServiceOp::Survey(_, m) => self
                        .service_metric(
                            world,
                            c.sites.split(';').last().unwrap_or_default(),
                            m,
                            now,
                        )
                        .is_some(),
                    _ => true,
                }),
            Effect::Deliver => c
                .scope
                .split_once('>')
                .and_then(|(_, item)| parse_item(item))
                .is_some_and(|item| have(cargo(item, 1))),
            Effect::Craft => c
                .scope
                .split_once(':')
                .and_then(|(_, recipe)| recipe.split_once('>'))
                .and_then(|(item, _)| parse_item(item))
                .is_some_and(|item| have(cargo(item, 1))),
            _ => true,
        }
    }

    fn clear_order(&mut self, id: usize) {
        self.orders[id] = None;
        self.services.request_return(id as u8);
        if let Some(batch) = &mut self.people[id].service_material {
            batch.returning = true;
        }
        if let Some(cargo) = self.people[id].escrow.take() {
            if self.people[id].walk.at == house(id).door {
                assert!(self.people[id].incoming(cargo));
            } else {
                self.people[id].returning_cargo = Some(cargo);
            }
        }
    }

    fn finish_daily_order(&mut self, id: usize, day: u64) {
        self.people[id].job = None;
        self.people[id].job_stop = 0;
        self.clear_order(id);
        self.people[id].break_day = Some(day);
    }

    fn observe_household(&mut self, world: &LivingWorld, now: u64) {
        use crate::living_world::{Activity, YardSeason};
        let actors = world.actors();
        let day = now / DAY_MS;
        let time = world.minute_of_day();
        let wet = matches!(world.weather(), YardWeather::Rain | YardWeather::Snow);
        let mask = world.phase_mask();
        self.services
            .note_conditions(now, wet, phase_active(mask, "weekly_market"));
        if wet {
            self.evidence.previous_rain = Some(now);
        }
        if world.last_shared_meal_day() == Some(day) {
            if time < 660 {
                self.observe_native(1, now);
            } else {
                self.observe_native(9, now);
            }
        }
        if wet
            && actors[..2]
                .iter()
                .any(|a| a.activity == Activity::Cook && a.pose == Pose::Work)
        {
            self.observe_native(5, now);
        }
        let shared = WILLOW_VILLAGE.building("new_shared_home").unwrap().door;
        if !self.evidence.shared_address
            && phase_active(mask, "late_shared_home")
            && HOMEBOUND[..2]
                .iter()
                .all(|&actor| world.home_of(actor) == Some(point(shared)))
        {
            self.evidence.shared_address = true;
            self.observe_native(7, now);
        }
        if actors[0].activity == Activity::Sleep
            && actors[1].activity == Activity::Sleep
            && actors[0].inside()
            && actors[1].inside()
        {
            self.observe_native(10, now);
        }
        let casts = world.fishing_casts();
        if self.evidence.cast.is_some_and(|n| n < casts) {
            self.observe_native(11, now);
        }
        self.evidence.cast = Some(casts);
        if self
            .trip
            .is_some_and(|trip| trip.stage == TripStage::Outbound)
            && actors[2].activity == Activity::Drink
        {
            self.observe_native(12, now);
        }
        if !wet && self.evidence.rain && world.season() != YardSeason::Winter {
            self.evidence.refuge[5] = Some(now + HOUR);
            self.observe_native(76, now);
        }
        if wet
            && !self.evidence.rain
            && world.season() != YardSeason::Winter
            && (450..1050).contains(&time)
        {
            self.evidence.refuge[9] = Some(now + HOUR);
            self.observe_native(78, now);
        }
        self.evidence.rain = wet;
        if time >= 1140
            && !actors[1].inside()
            && distance(world.actor_tile(ActorKind::Ayu), TilePos::new(29, 85)) <= 3
            && self.evidence.refuge[3].is_none_or(|until| until <= now)
        {
            self.evidence.refuge[3] = Some(now + HOUR);
            self.observe_native(77, now);
        }
        if animal_visible(7, ANIMALS[7], world)
            && distance(world.actor_tile(ActorKind::Orange), TilePos::new(15, 21)) <= 3
            && self.evidence.refuge[7].is_none_or(|until| until <= now)
        {
            self.evidence.refuge[7] = Some(now + HOUR);
            self.observe_native(79, now);
        }
        if animal_visible(2, ANIMALS[2], world)
            && world.season() == YardSeason::Autumn
            && self.evidence.cached_fruit < 8
            && self.remaining_source[item_index(Item::Crops)] > 0
            && distance(self.people[6].walk.at, TilePos::new(33, 73)) < 5
        {
            self.remaining_source[item_index(Item::Crops)] -= 1;
            self.evidence.cached_fruit += 1;
            self.observe_native(80, now);
        }
        if animal_visible(14, ANIMALS[14], world)
            && distance(world.actor_tile(ActorKind::Xiaohe), TilePos::new(19, 22)) < 4
            && self.evidence.refuge[14].is_none_or(|until| until <= now)
        {
            self.evidence.refuge[14] = Some(now + 20 * MINUTE);
            self.observe_native(81, now);
        }
        if phase_active(mask, "late")
            && animal_visible(16, ANIMALS[16], world)
            && !self.people[17].walk.hidden()
            && distance(self.people[17].walk.at, TilePos::new(76, 112)) <= 2
        {
            self.observe_native(83, now);
        }
        for (id, &animal) in ANIMALS.iter().enumerate() {
            if !animal_visible(id, animal, world)
                || !matches!(id, 2 | 5 | 6 | 7 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18)
            {
                continue;
            }
            let e = &mut self.ecology[id];
            if e.date != Some(day) {
                e.date = Some(day);
                e.visits = 0;
            }
            let progress = e.visits;
            if progress >= 12 || mix(now / (5 * MINUTE) ^ id as u64) % 5 != 0 {
                continue;
            }
            e.visits += 1;
            // Pollination is a finite orchard work input collected on site.
            if matches!(id, 17 | 18) && progress == 1 {
                self.pollination = self.pollination.saturating_add(1).min(12);
                if id == 17 && phase_active(mask, "community") {
                    self.observe_native(75, now);
                }
            }
        }
    }

    fn observe_native(&mut self, number: u16, now: u64) {
        assert!(
            NATIVE_EVENTS.contains(&number),
            "native event must be declared in the catalogue"
        );
        let bit = usize::from(number - 1);
        if matches!(number, 7 | 37) && self.completed_code(200 + bit) {
            return;
        }
        let mask = 1 << (bit % 64);
        if self.daily_native[bit / 64] & mask == 0 {
            self.daily_native[bit / 64] |= mask;
            self.record(200 + bit as u16, now);
        }
    }

    fn production(&mut self, world: &LivingWorld, id: usize, mask: PhaseMask) -> bool {
        let at = self.people[id].walk.at;
        let weather = world.weather();
        let clear = !matches!(weather, YardWeather::Rain | YardWeather::Snow);
        let meal_counter = shop(MEAL_SHOP);
        if id == meal_counter.merchant
            && (self.weekly_meal_needs_preparation(world, world.world_ms())
                || self.social.autumn_cook_preparing(world, id)
                    && self.people[id].count(Item::Food) <= meal_counter.goods.quantity)
        {
            let portions = meal_counter.goods.quantity + 1;
            if self
                .interaction_position(mask, "orchard_tools")
                .is_some_and(|pos| distance(at, pos) <= 3)
                && self.people[id].count(Item::Crops) < portions
            {
                let gather = portions - self.people[id].count(Item::Crops);
                let crops = &mut self.remaining_source[item_index(Item::Crops)];
                if *crops >= gather && self.people[id].gather_produce(cargo(Item::Crops, gather)) {
                    *crops -= gather;
                    // Collecting supplies is the first half of the meal order,
                    // so leave today's kitchen production slot available.
                    return false;
                }
            }
            if self
                .interaction_position(mask, "home_kitchen.door")
                .is_some_and(|pos| distance(at, pos) <= 3)
            {
                return self.convert(
                    id,
                    cargo(Item::Crops, portions),
                    cargo(Item::Food, portions),
                );
            }
        }
        if id == 5 && distance(at, house(id).door) <= 3 && self.people[id].count(Item::Rope) < 4 {
            // 青梭 can braid the stalks kept in the sheltered houseyard.
            // The same finite plant pool feeds field and orchard production.
            let rope = cargo(Item::Rope, 1);
            let stalks = &mut self.remaining_source[item_index(Item::Crops)];
            if *stalks >= 2 && self.people[id].incoming(rope) {
                *stalks -= 2;
                return true;
            }
        }
        let specification = match id {
            1 => ("residence_seed_merchant.door", Item::Seeds, 2),
            2 if clear => ("forest_loading", Item::Wood, 2),
            4 if clear => (
                "creek_fishing",
                Item::Fish,
                if self.services.damaged(4) { 1 } else { 2 },
            ),
            6 if clear => (
                "orchard_tools",
                Item::Crops,
                4 + u16::from(self.pollination > 0),
            ),
            7 if clear => ("commons_garden", Item::Crops, 2),
            9 if clear => ("quarry_work", Item::Stone, 3),
            10 => (
                farmer_production_site(mask),
                Item::Crops,
                if clear && phase_active(mask, "cooperative_farm") {
                    4
                } else {
                    2
                },
            ),
            16 => ("home_well", Item::Water, 2),
            _ => ("", Item::Water, 0),
        };
        if specification.2 != 0
            && self
                .interaction_position(mask, specification.0)
                .is_some_and(|p| distance(at, p) <= 3)
            && let Some(remaining) = self.remaining_source.get_mut(item_index(specification.1))
            && *remaining >= specification.2
            && if matches!(id, 6 | 10) {
                self.people[id].gather_produce(cargo(specification.1, specification.2))
            } else {
                self.people[id].incoming(cargo(specification.1, specification.2))
            }
        {
            *remaining -= specification.2;
            if id == 10 {
                let day = world.world_ms() / DAY_MS;
                self.social.note_neighbour_work(id, day, true);
                // After a week of wear, another real field shift can scuff the
                // same coat used by normal and winter repair appointments.
                if day % 7 == 0 {
                    self.services.wear(14);
                }
            }
            if id == 4 {
                self.services.wear(4);
            }
            if id == 6 && specification.2 > 4 {
                self.pollination -= 1;
            }
            return true;
        }
        let mut turn = match id {
            1 => (
                house(id).door_poi,
                cargo(Item::Crops, 1),
                cargo(Item::Seeds, 3),
            ),
            3 => (
                "home_kitchen.door",
                cargo(Item::Crops, 2),
                cargo(Item::Food, 2),
            ),
            5 => (
                "craft_textile.door",
                cargo(Item::Crops, 2),
                cargo(Item::Rope, 1),
            ),
            14 => (
                "craft_bakery.door",
                cargo(Item::Crops, 2),
                cargo(Item::Food, 2),
            ),
            _ => return false,
        };
        if id == 5 && !phase_active(mask, "village_development") {
            turn.0 = house(id).door_poi;
        }
        if self
            .interaction_position(mask, turn.0)
            .is_none_or(|p| distance(at, p) > 3)
        {
            return false;
        }
        self.convert(id, turn.1, turn.2)
    }

    fn convert(&mut self, id: usize, input: Cargo, output: Cargo) -> bool {
        let r = &mut self.people[id];
        if !r.can_accept(output) || !r.outgoing(input) {
            return false;
        }
        r.incoming(output)
    }

    fn apply_contract(
        &mut self,
        world: &mut LivingWorld,
        id: usize,
        c: Contract,
        now: u64,
    ) -> bool {
        let Some((scope, extra)) = c.scope.split_once(':') else {
            return false;
        };
        let Ok(scope) = scope.parse::<usize>() else {
            return false;
        };
        if scope >= self.site_status.len() {
            return false;
        }
        let observation = || match extra {
            "wet" => matches!(world.weather(), YardWeather::Rain | YardWeather::Snow),
            "clear" => !matches!(world.weather(), YardWeather::Rain | YardWeather::Snow),
            "crowd" => self.present() > 0,
            "plant" => world.plots().iter().any(|p| {
                matches!(
                    p.stage,
                    crate::living_world::PlotStage::Growing
                        | crate::living_world::PlotStage::Mature
                )
            }),
            _ => true,
        };
        match c.effect {
            Effect::Native => false,
            Effect::Observe => {
                if !observation() {
                    return false;
                }
                let raw = self.read_site(world, c, extra);
                let changed = self.site_status[scope] != raw || self.work[scope].is_none();
                self.site_status[scope] = raw;
                changed
            }
            Effect::Supply => {
                if c.id == "RI151"
                    && self
                        .services
                        .notice("reed_boundary")
                        .is_some_and(|(v, issued)| v > 0 && now.saturating_sub(issued) < DAY_MS)
                {
                    return false;
                }
                let Some(item) = parse_item(extra) else {
                    return false;
                };
                let source = &mut self.remaining_source[item_index(item)];
                if *source == 0 || matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
                {
                    return false;
                }
                if !self.people[id].incoming(cargo(item, 1)) {
                    return false;
                }
                *source -= 1;
                true
            }
            Effect::Craft => {
                let Some((before, after)) = extra.split_once('>') else {
                    return false;
                };
                let (Some(before), Some(after)) = (parse_item(before), parse_item(after)) else {
                    return false;
                };
                self.convert(id, cargo(before, 1), cargo(after, 1))
            }
            Effect::Deliver => {
                let Some((receiver, material)) = extra.split_once('>') else {
                    return false;
                };
                let Some(item) = parse_item(material) else {
                    return false;
                };
                let batch = cargo(item, 1);
                if self.people[id].escrow != Some(batch) {
                    return false;
                }
                if receiver == "home" {
                    if !HOMEBOUND[..2].iter().any(|&a| {
                        let actor = &world.actors()[a as usize];
                        !actor.inside()
                            && actor.walking_segment().is_none()
                            && world.activity_deadline(a).is_none()
                            && distance(world.actor_tile(a), self.people[id].walk.at) <= 2
                    }) || !world.exchange_stock_atomic(&[], &[(item, 1)])
                    {
                        return false;
                    }
                } else {
                    let Ok(receiver) = receiver.parse::<usize>() else {
                        return false;
                    };
                    if receiver >= RESIDENT_COUNT
                        || receiver == id
                        || self.people[receiver].walk.hidden()
                        || self.people[receiver].walk.edge.is_some()
                        || distance(self.people[id].walk.at, self.people[receiver].walk.at) > 2
                        || !self.people[receiver].incoming(batch)
                    {
                        return false;
                    }
                }
                self.people[id].escrow = None;
                true
            }
            Effect::Plot => world.village_local_work(
                extra,
                c.sites
                    .split(';')
                    .find(|id| id.starts_with("crop_"))
                    .and_then(|name| {
                        let plot = WILLOW_VILLAGE.crop(name)?;
                        (distance(plot.work, self.people[id].walk.at) <= 1
                            && (c.id != "ME017" || self.services.storm_recent(now)))
                        .then(|| name[5..].parse().ok())
                        .flatten()
                    }),
            ),
            Effect::Service => self.service(world, id, c, scope, now),
        }
    }

    fn read_site(&self, world: &LivingWorld, c: Contract, condition: &str) -> u8 {
        if condition == "crowd" {
            return self.present() as u8;
        }
        let site = c.sites.split(';').next().unwrap_or_default();
        if let Some(index) = site
            .strip_prefix("crop_")
            .and_then(|text| text.parse::<usize>().ok())
        {
            return world.plots().get(index).map_or(0, |p| 1 + p.stage as u8);
        }
        if site.contains("bridge") {
            return 1 + u8::from(phase_active(world.phase_mask(), "main_bridge_freight"));
        }
        if site.starts_with("orchard") {
            return (8..12)
                .filter(|&n| {
                    world.orchard_phase(n).is_some_and(|phase| {
                        matches!(
                            phase,
                            crate::living_world::OrchardPhase::Flower
                                | crate::living_world::OrchardPhase::Ripe
                        )
                    })
                })
                .count() as u8;
        }
        if site.contains("market") || site.starts_with("stall_") {
            return self
                .stall_visuals(world)
                .into_iter()
                .filter(|s| s.open && s.site != "home_trade")
                .count() as u8;
        }
        if condition == "wet" {
            return if world.weather() == YardWeather::Rain {
                1
            } else {
                2
            };
        }
        u8::from(
            self.interaction_position(world.phase_mask(), site)
                .is_some_and(|p| world.can_walk(p.x.into(), p.y.into())),
        )
    }

    fn service(
        &mut self,
        world: &mut LivingWorld,
        id: usize,
        c: Contract,
        _scope: usize,
        now: u64,
    ) -> bool {
        use services::Audience;
        let Some(order) = self.orders[id] else {
            return false;
        };
        if now < order.opened + WORK || now > order.opened + 5 * HOUR {
            return false;
        }
        let site = c
            .sites
            .split(';')
            .nth(usize::from(self.people[id].job_stop))
            .unwrap_or_default();
        let position = self.people[id].walk.at;
        let Some(def) = services::definitions().find(|def| def.id == c.id) else {
            return false;
        };
        if let Some((cargo, household)) = def.op.materials()
            && self.people[id].service_material
                != Some(ServiceMaterial {
                    cargo,
                    household,
                    returning: false,
                })
        {
            return false;
        }
        if c.id == "ME073" && now / DAY_MS % 7 != 6 {
            return false;
        }
        if c.id == "ME041" && !phase_active(world.phase_mask(), "main_bridge_freight") {
            return false;
        }
        if matches!(c.id, "RI124" | "ME006")
            && world.home_of(ActorKind::Ayu)
                != Some(point(WILLOW_VILLAGE.building("home_ayu").unwrap().door))
        {
            return false;
        }
        let meet = |other: usize| {
            other != id
                && !self.people[other].walk.hidden()
                && self.people[other].walk.edge.is_none()
                && distance(position, self.people[other].walk.at) <= 3
        };
        let house = HOMEBOUND[..2].iter().copied().find(|&actor| {
            let who = &world.actors()[actor as usize];
            !who.inside()
                && who.walking_segment().is_none()
                && world.activity_deadline(actor).is_none()
                && distance(position, world.actor_tile(actor)) <= 3
        });
        let transfer_near = |owner| match owner {
            services::FROM_CUPBOARD => house.is_some(),
            services::PREMISES => true,
            services::OUTSIDE_CUSTODY => self.guests.iter().flatten().any(|guest| {
                !guest.walk.hidden()
                    && guest.walk.edge.is_none()
                    && distance(position, guest.walk.at) <= 3
            }),
            other if usize::from(other) == id => true,
            other if usize::from(other) < RESIDENT_COUNT => meet(usize::from(other)),
            _ => false,
        };
        match def.op {
            ServiceOp::Mend(fixture, _) => {
                if matches!(c.id, "RI186" | "RI021") {
                    let action = if c.id == "RI186" {
                        CommunityAction::Mend(Fixture::WellRope)
                    } else {
                        CommunityAction::Mend(Fixture::WorkshopToolBoard)
                    };
                    if world.apply_community(action, position) {
                        self.people[id].service_material = None;
                        return true;
                    }
                }
                if !self.services.repair(fixture, 1) {
                    return false;
                }
                self.people[id].service_material = None;
                true
            }
            ServiceOp::Prepare(asset, _) => {
                if !self.services.make_asset(asset, id as u8, position, now) {
                    return false;
                }
                self.people[id].service_material = None;
                true
            }
            ServiceOp::Move(asset, from, to) => {
                if c.id == "RI075"
                    && !self
                        .guests
                        .iter()
                        .flatten()
                        .any(|g| g.walk.state == State::Inside)
                {
                    return false;
                }
                if matches!(c.id, "RI009" | "ME061") && !self.evidence.shared_address {
                    return false;
                }
                if c.id == "ME071" && !self.services.has_asset("lost_property", 9) {
                    return false;
                }
                self.services.carried_by(asset, id as u8)
                    && transfer_near(to)
                    && self
                        .services
                        .move_asset(asset, from, to, id as u8, position, now)
            }
            ServiceOp::Home(action, _) | ServiceOp::HomeStock(action, _) => {
                if !world.apply_community(action, position) {
                    return false;
                }
                self.people[id].service_material = None;
                true
            }
            ServiceOp::Talk(audience) => {
                if let Some(actor) =
                    house.filter(|_| matches!(audience, Audience::House | Audience::Open))
                {
                    if !world.reserve_activity(&[actor]) {
                        return false;
                    }
                    let changed = self.services.build_rapport(id);
                    world.end_activity(&[actor], 0);
                    changed
                } else {
                    let other = match audience {
                        Audience::Person(person) => Some(usize::from(person)).filter(|&p| meet(p)),
                        Audience::Open => (0..RESIDENT_COUNT).find(|&p| meet(p)),
                        Audience::House => None,
                    };
                    if let Some(other) = other {
                        let changed = self.services.build_rapport(id);
                        self.services.build_rapport(other) || changed
                    } else {
                        false
                    }
                }
            }
            ServiceOp::Publish(board, metric) => {
                let Some(value) = self.service_metric(world, site, metric, now) else {
                    return false;
                };
                let changed = self.services.publish(board, value, now);
                if changed && c.id == "RI173" {
                    self.services.acknowledge("reed_boundary");
                }
                changed
            }
            ServiceOp::Survey(fixture, metric) => {
                let Some(value) = self.service_metric(world, site, metric, now) else {
                    return false;
                };
                let Some(wear) = self.services.observe_fixture(fixture) else {
                    return false;
                };
                self.services
                    .publish(17, (value.min(8191) << 3) | wear.min(7), now)
            }
        }
    }

    fn service_metric(
        &self,
        world: &LivingWorld,
        site: &str,
        metric: &str,
        now: u64,
    ) -> Option<u16> {
        let mask = world.phase_mask();
        let wet = matches!(world.weather(), YardWeather::Rain | YardWeather::Snow);
        Some(match metric {
            "open_count" => self
                .stall_visuals(world)
                .iter()
                .filter(|s| s.open && s.site != "home_trade")
                .count() as u16,
            "bridge_state" => 1 + u16::from(phase_active(mask, "main_bridge_freight")),
            "crop_stage" => {
                world
                    .plots()
                    .get(site.strip_prefix("crop_")?.parse::<usize>().ok()?)?
                    .stage as u16
            }
            "seed_count" => self.people[1].count(Item::Seeds),
            "weather_clear" if !wet => 1,
            "fishing_window" => {
                if wet {
                    0
                } else {
                    1
                }
            }
            "rain" if wet => 2,
            "pollen" if self.pollination > 0 => u16::from(self.pollination),
            "frost" if world.season() == crate::living_world::YardSeason::Winter => 1,
            "homes" => u16::from(phase_active(mask, "late_shared_home")) + 2,
            "community" if phase_active(mask, "community") => 1,
            "boundary" => u16::from(world.can_walk(191, 56)),
            "south_open" => u16::from(phase_active(mask, "late")),
            "lot_open" if phase_active(mask, "late") => {
                u16::from(self.interaction_position(mask, site).is_some())
            }
            "load_stock" => self
                .stall_visuals(world)
                .iter()
                .filter(|s| s.open)
                .map(|s| s.quantity)
                .sum(),
            "shared_home" if phase_active(mask, "late_shared_home") => 1,
            "trades" if self.trades > 0 => self.trades.min(u16::MAX as u64) as u16,
            "birds" if self.ecology[12].date == Some(now / DAY_MS) => {
                u16::from(self.ecology[12].visits)
            }
            "guest_count" if self.present() > 0 => self.present() as u16,
            "storm_recent" if self.services.storm_recent(now) => 1,
            _ => return None,
        })
    }

    fn record(&mut self, code: u16, time: u64) {
        if usize::from(code) >= CONTRACT_COUNT {
            return;
        }
        let bit = usize::from(code);
        self.completed[bit / 64] |= 1 << (bit % 64);
        self.events = self.events.saturating_add(1);
        self.last_receipt = Some(ActivityReceipt { code, time });
        if let Some(c) = contracts().nth(bit)
            && let Some(scope) = c
                .scope
                .split_once(':')
                .and_then(|(slot, _)| slot.parse::<usize>().ok())
            && let Some(entry) = self.work.get_mut(scope)
        {
            let revision = entry.map_or(1, |w| w.revision.saturating_add(1));
            *entry = Some(ServiceWork {
                code,
                revision,
                due: time,
            });
        }
    }

    fn occupied(
        &self,
        world: &LivingWorld,
        p: TilePos,
        skip_resident: Option<usize>,
        skip_guest: Option<usize>,
    ) -> bool {
        HOMEBOUND
            .iter()
            .any(|&k| world.actor_tile(k) == p || world.actor_next_tile(k) == Some(p))
            || self.people.iter().enumerate().any(|(i, r)| {
                Some(i) != skip_resident
                    && !r.walk.hidden()
                    && (r.walk.at == p || r.walk.edge.is_some_and(|e| e.to == p))
            })
            || self.guests.iter().enumerate().any(|(i, r)| {
                Some(i) != skip_guest
                    && r.as_ref()
                        .is_some_and(|r| r.walk.at == p || r.walk.edge.is_some_and(|e| e.to == p))
            })
    }

    fn move_walker(
        &self,
        world: &LivingWorld,
        routes: &mut RouteWork,
        walker: &mut Walker,
        resident: Option<usize>,
        guest: Option<usize>,
        mask: PhaseMask,
        now: u64,
        loaded: bool,
    ) -> bool {
        let stride = edge_duration(now, Pace::Human, travel_conditions(world, loaded));
        if let Some(edge) = walker.edge.take() {
            walker.at = edge.to;
            walker.next += 1;
        }
        if walker.at == walker.target {
            return true;
        }
        // Repair a pre-reservation checkpoint where an inbound traveler at
        // the gate is facing an outbound walker. The outbound traveler
        // leaves the doorway without abandoning their departure.
        if walker.target == WILLOW_VILLAGE.boundary.spawn
            && walker.at == WILLOW_VILLAGE.boundary.corridor[0]
            && walker.wait >= 2
            && self.corridor_reserved(resident, guest)
            && let Some(side) = walker.at.offset(-1, 0).filter(|p| {
                world.can_walk(i32::from(p.x), i32::from(p.y))
                    && !self.occupied(world, *p, resident, guest)
            })
        {
            walker.segment.clear();
            walker.segment.push(side);
            walker.next = 0;
            walker.due = now + stride;
            walker.edge = Some(Edge {
                to: side,
                start: now,
                end: walker.due,
            });
            return false;
        }
        if walker
            .segment
            .get(usize::from(walker.next))
            .is_some_and(|&next| !world.can_walk(i32::from(next.x), i32::from(next.y)))
        {
            walker.segment.clear();
            walker.next = 0;
        }
        if usize::from(walker.next) >= walker.segment.len() {
            if !routes.find(world, walker, mask, (resident, guest), |p| {
                self.occupied(world, p, resident, guest)
            }) {
                if !routes.waiting {
                    walker.wait = walker.wait.saturating_add(1);
                    walker.due =
                        now.saturating_add((RETRY << walker.wait.min(4)).min(REPLAN_DELAY_MAX));
                }
                return false;
            }
        }
        let next = walker.segment[usize::from(walker.next)];
        if main_bridge_deck(walker.at)
            && walker.wait >= 3
            && self.occupied(world, next, resident, guest)
            && let Some(back) = walker
                .at
                .offset(if walker.at.x < walker.target.x { -1 } else { 1 }, 0)
            && world.can_walk(back.x.into(), back.y.into())
            && !self.occupied(world, back, resident, guest)
        {
            // Step toward the shore and let the traveler already facing
            // us leave the single lane. The original destination stays.
            walker.segment.clear();
            walker.segment.push(back);
            walker.next = 0;
            walker.due = now + stride;
            walker.edge = Some(Edge {
                to: back,
                start: now,
                end: walker.due,
            });
            return false;
        }
        let crossing_zone: Option<fn(TilePos) -> bool> =
            if main_bridge_deck(next) && !main_bridge_deck(walker.at) {
                Some(main_bridge_deck)
            } else if pantry_service_zone(next)
                && !pantry_service_zone(walker.at)
                && pantry_service_zone(walker.target)
            {
                Some(pantry_service_zone)
            } else {
                None
            };
        let shared_zone_busy = crossing_zone.is_some_and(|zone| {
            HOMEBOUND[..2].iter().any(|&actor| {
                !world.actors()[actor as usize].inside()
                    && (zone(world.actor_tile(actor))
                        || world.actor_next_tile(actor).is_some_and(zone))
            }) || self
                .people
                .iter()
                .enumerate()
                .any(|(id, other)| Some(id) != resident && holds_zone(&other.walk, zone))
                || self.guests.iter().enumerate().any(|(id, other)| {
                    Some(id) != guest && other.as_ref().is_some_and(|g| holds_zone(&g.walk, zone))
                })
        });
        if (!outer_corridor(walker.at)
            && outer_corridor(next)
            && self.corridor_reserved(resident, guest))
            || shared_zone_busy
            || self.occupied(world, next, resident, guest)
        {
            walker.wait = walker.wait.saturating_add(1);
            let delay = if walker.wait <= 2 {
                STEP
            } else {
                (RETRY << walker.wait.min(4)).min(REPLAN_DELAY_MAX)
            };
            walker.due = now.saturating_add(delay);
            if walker.wait >= 2 {
                walker.segment.clear();
                walker.next = 0;
            }
            return false;
        }
        walker.wait = 0;
        if next.x != walker.at.x {
            walker.turn = next.x < walker.at.x;
        }
        walker.due = now.saturating_add(stride);
        walker.edge = Some(Edge {
            to: next,
            start: now,
            end: walker.due,
        });
        false
    }

    fn corridor_reserved(&self, resident: Option<usize>, guest: Option<usize>) -> bool {
        self.people
            .iter()
            .enumerate()
            .any(|(id, r)| Some(id) != resident && r.walk.holds_corridor())
            || self.guests.iter().enumerate().any(|(id, g)| {
                Some(id) != guest && g.as_ref().is_some_and(|g| g.walk.holds_corridor())
            })
    }

    fn arrive(&mut self, id: usize, walker: &mut Walker, previous_state: State, now: u64) {
        walker.segment.clear();
        walker.next = 0;
        match walker.state {
            State::Leaving | State::MovingIn => {
                walker.state = State::Inside;
                walker.due = 0;
            }
            State::Outbound => {
                walker.state = State::AtWork;
                walker.due = now + WORK;
            }
            State::LeavingVillage => {
                walker.state = State::Away;
                walker.due = now + 5 * HOUR;
            }
            State::ReturningFromAway => {
                walker.state = State::Inside;
                walker.due = 0;
                self.observe_native(68, now);
            }
            _ => {}
        }
        if id >= 10 && walker.state == State::Inside && previous_state == State::MovingIn {
            self.observe_native(64, now);
        }
    }
}

fn holds_zone(walk: &Walker, in_zone: fn(TilePos) -> bool) -> bool {
    !walk.hidden() && (in_zone(walk.at) || walk.edge.is_some_and(|edge| in_zone(edge.to)))
}

fn next_boundary(now: u64, interval: u64) -> u64 {
    now.saturating_sub(now % interval).saturating_add(interval)
}

fn booth_slot(index: usize) -> TilePos {
    static SLOTS: OnceLock<[TilePos; SHOP_COUNT]> = OnceLock::new();
    SLOTS.get_or_init(|| {
        std::array::from_fn(|index| {
            slot(PhaseMask::ALL, shop(index).id, "merchant")
                .expect("compiled outdoor merchant slot")
        })
    })[index]
}

fn mix(seed: u64) -> u64 {
    let mut value = seed.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn guest_goal(world: &LivingWorld, mask: PhaseMask, window: u64) -> u8 {
    let hour = world.world_ms() % DAY_MS / HOUR;
    let day = world.world_ms() / DAY_MS % 7;
    let cdf: &[u64] = if !(7..20).contains(&hour) {
        &[
            1_367_210_523,
            3_430_528_943,
            4_218_886_089,
            4_293_303_505,
            4_294_967_296,
        ]
    } else if day == 5 && phase_active(mask, "weekly_market") {
        &[
            21_079_791,
            102_768_345,
            320_129_210,
            818_556_633,
            1_601_893_389,
            2_513_578_590,
            3_296_915_346,
            3_795_342_769,
            4_294_967_296,
        ]
    } else if day == 6 {
        &[
            261_891_584,
            1_306_606_356,
            2_961_538_573,
            4_006_253_346,
            4_268_144_930,
            4_293_976_599,
            4_294_967_296,
        ]
    } else {
        &[
            36_457_175,
            200_000_192,
            677_450_337,
            1_585_228_478,
            2_709_946_818,
            3_617_724_959,
            4_095_175_104,
            4_258_718_121,
            4_294_967_296,
        ]
    };
    let sample = mix(WILLOW_VILLAGE.seed ^ window) as u32;
    cdf.partition_point(|&value| value <= u64::from(sample)) as u8
}

fn parse_item(name: &str) -> Option<Item> {
    Some(match name {
        "seeds" => Item::Seeds,
        "water" => Item::Water,
        "crops" => Item::Crops,
        "food" => Item::Food,
        "wood" => Item::Wood,
        "stone" => Item::Stone,
        "rope" => Item::Rope,
        "fish" => Item::Fish,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pond_duck_keeps_swimming_pose_across_animation_cycles() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let start = world.world_ms() + 150 * MINUTE;
        run_until(&mut world, &mut life, start, |_, _| {});
        let mut previous: Option<Point> = None;
        // Cover an entire out-and-back route, including the old rapid frame
        // boundaries. A duck may glide; it must not switch to a taller land pose.
        for _ in 0..=288 {
            let duck = life
                .wildlife_visuals(&world)
                .into_iter()
                .find(|animal| animal.sprite.starts_with("wild_duckling_"))
                .expect("the authored pond visit is active");
            assert_eq!(duck.sprite, "wild_duckling_swim_0");
            assert_eq!(
                world.ground(
                    (duck.position.x / TILE_PX as f32).floor() as i32,
                    (duck.position.y / TILE_PX as f32).floor() as i32,
                ),
                Ground::Water,
            );
            if let Some(before) = previous {
                assert!((duck.position.x - before.x).abs() <= 0.18);
                assert!((duck.position.y - before.y).abs() <= 0.18);
            }
            previous = Some(duck.position);
            let next = world.world_ms() + 2_000;
            run_until(&mut world, &mut life, next, |_, _| {});
        }
    }

    fn run_until(
        world: &mut LivingWorld,
        life: &mut VillageLife,
        end: u64,
        mut observe: impl FnMut(&LivingWorld, &VillageLife),
    ) {
        let budget = (end - world.world_ms()).div_ceil(DAY_MS).max(1) * 50_000;
        world.queue_world_time(end - world.world_ms());
        for _ in 0..budget {
            let now = world.world_ms();
            world.set_external_occupied(&life.occupied_tiles());
            world.set_social_harvest_plot(life.reserved_harvest_plot());
            if !world.advance_pending_to(now) || world.navigation_pending() {
                continue;
            }
            life.advance(world);
            if life.navigation_pending() {
                continue;
            }
            world.record_village_social(life.social_progress());
            observe(world, life);
            if now == end {
                return;
            }
            world.set_external_occupied(&life.occupied_tiles());
            let next = world
                .next_update_ms()
                .min(life.next_update_ms(world).unwrap_or(end))
                .min(end)
                .max(now + 1);
            let _ = world.advance_pending_to(next);
        }
        panic!(
            "simulation failed to advance to the intended cut: {}",
            world.world_ms()
        );
    }

    #[test]
    fn catalogue_references_compiled_map_and_residents() {
        let all = contracts().collect::<Vec<_>>();
        assert_eq!(all.len(), CONTRACT_COUNT);
        let service_defs = services::definitions().collect::<Vec<_>>();
        let native: Vec<_> = all
            .iter()
            .filter(|c| c.effect == Effect::Native)
            .map(|c| c.id[2..].parse::<u16>().unwrap())
            .collect();
        assert_eq!(native, NATIVE_EVENTS);
        assert_eq!(
            all.iter().filter(|c| c.effect == Effect::Service).count(),
            service_defs.len()
        );
        for service in &service_defs {
            assert!(
                all.iter()
                    .any(|c| c.id == service.id && c.effect == Effect::Service)
            );
            assert_eq!(
                service_defs.iter().filter(|d| d.id == service.id).count(),
                1
            );
            assert!(!service.description.is_empty());
        }
        for (index, c) in all.iter().enumerate() {
            if index < 200 {
                assert_eq!(c.actor, Some(index / 10));
            }
            assert!(
                c.phases
                    .split('+')
                    .all(|p| WILLOW_VILLAGE.phase(p).is_some()),
                "{}",
                c.id
            );
            assert!(
                c.sites
                    .split(';')
                    .all(|id| resolve(PhaseMask::ALL, id).is_some()
                        || WILLOW_VILLAGE.water.iter().any(|w| w.id == id)),
                "{} {}",
                c.id,
                c.sites
            );
        }
        let life = VillageLife::default();
        let homes = life.residents().collect::<Vec<_>>();
        assert_eq!(homes.len(), 20);
        assert_eq!(
            homes.iter().filter(|home| home.settled).count(),
            FIRST_RESIDENTS.len()
        );
        assert_eq!(homes.iter().filter(|room| room.bed == 1).count(), 5);
        for (index, home) in homes.iter().enumerate() {
            assert!(
                homes[..index]
                    .iter()
                    .all(|other| other.home != home.home || other.bed != home.bed)
            );
        }
    }

    #[test]
    fn companionship_claims_survive_save_and_do_not_touch_goods_or_world_receipts() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let before = serde_json::to_vec(world.stock()).unwrap();
        let first = companionship::REWARDS[0];
        let three = companionship::REWARDS[2].threshold_ms;
        assert_eq!(life.update_companionship(first.threshold_ms - 1), 0);
        world.queue_world_time(730 * DAY_MS);
        life.advance(&mut world);
        assert_eq!(life.companionship_received(), 0);
        let events = life.events();
        assert_eq!(life.update_companionship(first.threshold_ms), first.bit);
        assert_eq!(life.update_companionship(first.threshold_ms), 0);
        assert_eq!(life.update_companionship(three), 0b110);
        assert_eq!(life.events(), events);
        assert_eq!(serde_json::to_vec(world.stock()).unwrap(), before);

        let state = serde_json::to_vec(&life).unwrap();
        let mut loaded: VillageLife = serde_json::from_slice(&state).unwrap();
        assert_eq!(loaded.companionship_received(), 0b111);
        assert!(loaded.companionship_valid_for_ms(three));
        assert!(!loaded.companionship_valid_for_ms(three - 1));
        assert_eq!(loaded.update_companionship(three), 0);
        assert_eq!(loaded.update_companionship(u64::MAX), 0b111111000);
        assert_eq!(loaded.update_companionship(u64::MAX), 0);
    }

    #[test]
    fn service_repair_consumes_real_resident_stock_and_cannot_repeat_on_clean_fixture() {
        let mut life = VillageLife::default();
        let mut world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        let c = contracts().find(|c| c.id == "RI021").unwrap();
        let id = c.actor.unwrap();
        life.people[id].walk.at = WILLOW_VILLAGE.building("home_workshop").unwrap().door;
        life.orders[id] = Some(WorkOrder {
            opened: now - WORK,
            attempts: 0,
        });
        let original = life.people[id].count(Item::Wood);
        let wear = life.wear_on("tool_handle").unwrap();
        assert_eq!(wear, 2);
        assert!(!life.service(&mut world, id, c, 1, now));
        for _ in 0..2 {
            assert!(life.people[id].outgoing(cargo(Item::Wood, 1)));
            life.people[id].service_material = Some(ServiceMaterial {
                cargo: cargo(Item::Wood, 1),
                household: false,
                returning: false,
            });
            assert_eq!(life.resident_held(id), Some(VisitorHeld::Goods(Item::Wood)));
            assert!(life.service(&mut world, id, c, 1, now));
        }
        assert_eq!(life.wear_on("tool_handle"), Some(0));
        assert_eq!(life.people[id].count(Item::Wood), original - 2);
        assert!(!life.service(&mut world, id, c, 1, now));
        assert_eq!(life.people[id].count(Item::Wood), original - 2);
    }

    #[test]
    fn actual_household_water_is_charged_once_for_bowl_service() {
        let mut life = VillageLife::default();
        let mut world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        let c = contracts().find(|c| c.id == "ME002").unwrap();
        let id = c.actor.unwrap();
        let mask = world.phase_mask();
        let source = life.material_source(mask, id, true).unwrap();
        let site = resolve(mask, "cat_water").unwrap();
        let (code, _) = contracts()
            .enumerate()
            .find(|(_, c)| c.id == "ME002")
            .unwrap();
        life.people[id].job = Some(code as u16);
        life.people[id].walk.at = site;
        life.orders[id] = Some(WorkOrder {
            opened: now - WORK,
            attempts: 0,
        });
        let original = world.stock().count(Item::Water);
        let bowl = world.bowl_water();
        assert!(!life.service(&mut world, id, c, 1, now));
        life.people[id].walk.at = source;
        life.do_work(&mut world, id, mask, now);
        assert_eq!(
            life.resident_held(id),
            Some(VisitorHeld::Goods(Item::Water))
        );
        assert_eq!(world.stock().count(Item::Water), original - 1);
        assert_eq!(world.bowl_water(), bowl);
        life.people[id].walk.at = site;
        life.people[id].job_stop = 1;
        assert!(life.service(&mut world, id, c, 1, now));
        assert_eq!(world.stock().count(Item::Water), original - 1);
        assert_eq!(world.bowl_water(), bowl + 1);
        assert_eq!(life.resident_held(id), None);
    }

    #[test]
    fn only_collected_tools_and_packed_counter_batches_appear_in_hands() {
        let world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let carpenter = 2;
        let cupboard = house(carpenter).door;
        life.people[carpenter].walk = Walker::new(TilePos::new(44, 25), State::AtWork);
        let carpenter_visual = |life: &VillageLife| {
            life.visuals(&world)
                .into_iter()
                .find(|v| v.profession == Profession::resident(carpenter))
                .unwrap()
        };
        assert_eq!(carpenter_visual(&life).pose, VisitorPose::Working);
        assert_eq!(carpenter_visual(&life).held, None);
        life.collect_owned_tool_at_home(carpenter);
        assert_eq!(carpenter_visual(&life).held, None);
        life.people[carpenter].walk.at = cupboard;
        life.collect_owned_tool_at_home(carpenter);
        assert_eq!(
            carpenter_visual(&life).held,
            Some(VisitorHeld::Tool(VisitorTool::Hammer))
        );

        let merchant = 1;
        let s = shop(1);
        let before = life.people[merchant].count(s.goods.item);
        life.people[merchant].walk = Walker::new(house(merchant).door, State::Outbound);
        life.people[merchant].walk.target = booth_slot(1);
        assert_eq!(life.resident_held(merchant), None);
        assert!(life.people[merchant].pack_counter(1, 0));
        assert_eq!(
            life.people[merchant].count(s.goods.item),
            before - s.goods.quantity
        );
        assert_eq!(
            life.resident_held(merchant),
            Some(VisitorHeld::Goods(s.goods.item))
        );
        let stored = serde_json::to_vec(&life).unwrap();
        let mut reopened: VillageLife = serde_json::from_slice(&stored).unwrap();
        assert_eq!(
            reopened.resident_held(merchant),
            life.resident_held(merchant)
        );
        reopened.people[merchant].walk = Walker::new(booth_slot(1), State::AtWork);
        assert_eq!(reopened.resident_held(merchant), None);
        assert_eq!(reopened.people[merchant].counter_goods(1), s.goods.quantity);
        reopened.people[merchant].walk = Walker::new(house(merchant).door, State::AtWork);
        reopened.people[merchant].stow_counter();
        assert_eq!(reopened.people[merchant].count(s.goods.item), before);
        assert_eq!(reopened.people[merchant].counter_goods(1), 0);
    }

    #[test]
    fn bowl_cleaning_finishes_at_the_bowl_and_does_not_refill_it() {
        let mut life = VillageLife::default();
        let world = LivingWorld::new_village_wall_ms(0);
        let mut fixture = serde_json::to_value(&world).unwrap();
        fixture["village"]["cat_bowl_dirty"] = true.into();
        let mut world: LivingWorld = serde_json::from_value(fixture).unwrap();
        let mask = world.phase_mask();
        let now = world.world_ms();
        let contract = contracts().find(|c| c.id == "ME003").unwrap();
        let resident = contract.actor.unwrap();
        let well = life.interaction_position(mask, "home_well").unwrap();
        let bowl = resolve(mask, "cat_water").unwrap();
        assert_eq!(life.contract_site(&world, contract, mask, 0), Some(well));
        assert_eq!(
            life.interaction_position(mask, contract.sites.split(';').last().unwrap()),
            Some(bowl)
        );
        life.people[resident].walk.at = well;
        life.people[resident].job_stop = 1;
        life.orders[resident] = Some(WorkOrder {
            opened: now - WORK,
            attempts: 0,
        });
        let stock = life.people[resident].count(Item::Water);
        let bowl_water = world.bowl_water();
        assert!(!life.service(&mut world, resident, contract, 2, now));
        assert!(world.cat_bowl_needs_cleaning());
        assert_eq!(life.people[resident].count(Item::Water), stock);
        assert!(life.people[resident].outgoing(cargo(Item::Water, 1)));
        life.people[resident].service_material = Some(ServiceMaterial {
            cargo: cargo(Item::Water, 1),
            household: false,
            returning: false,
        });
        world.test_camera_actor(ActorKind::Orange, point(well), Pose::Idle, false);
        assert_eq!(life.contract_site(&world, contract, mask, 1), Some(bowl));
        life.people[resident].walk.at = bowl;
        assert!(life.service(&mut world, resident, contract, 2, now));
        assert!(!world.cat_bowl_needs_cleaning());
        assert_eq!(life.people[resident].count(Item::Water), stock - 1);
        assert_eq!(world.bowl_water(), bowl_water);
        assert!(!life.service(&mut world, resident, contract, 2, now));
        assert_eq!(life.people[resident].count(Item::Water), stock - 1);
    }

    fn new_plot_contract_fixture(
        fields: &[(usize, crate::living_world::PlotStage)],
    ) -> LivingWorld {
        let world = LivingWorld::new_village_wall_ms(0);
        let mut fixture = serde_json::to_value(&world).unwrap();
        let flags = world
            .phase_mask()
            .with(WILLOW_VILLAGE.phase("farm_expansion").unwrap())
            .with(WILLOW_VILLAGE.phase("resident_arrival").unwrap())
            .bits();
        fixture["village"]["region_flags"] = flags.into();
        for &(index, stage) in fields {
            fixture["plots"][index]["stage"] = serde_json::to_value(stage).unwrap();
            fixture["plots"][index]["watered"] = false.into();
        }
        serde_json::from_value(fixture).unwrap()
    }

    #[test]
    fn double_watering_uses_two_actual_stops_with_one_debit_per_watered_plot() {
        use crate::living_world::PlotStage;

        for interrupted in [false, true] {
            let mut world =
                new_plot_contract_fixture(&[(6, PlotStage::Seed), (15, PlotStage::Seed)]);
            let mut life = VillageLife::default();
            let mask = world.phase_mask();
            let now = world.world_ms();
            let contract = contracts().find(|c| c.id == TWO_FIELD_WATERING).unwrap();
            let resident = contract.actor.unwrap();
            let code = contracts().position(|c| c.id == contract.id).unwrap() as u16;
            let initial = world.stock().count(Item::Water);
            assert!(life.can_accept_job(&world, resident, contract, mask, now));
            life.people[resident].job = Some(code);
            life.orders[resident] = Some(WorkOrder {
                opened: now - WORK,
                attempts: 0,
            });
            let first = WILLOW_VILLAGE.crops[6].work;
            let second = WILLOW_VILLAGE.crops[15].work;
            life.people[resident].walk.at = first;
            life.do_work(&mut world, resident, mask, now);
            assert_eq!(life.people[resident].job_stop, 1);
            assert!(!world.village_plot_needs_water(6));
            assert!(world.village_plot_needs_water(15));
            assert_eq!(world.stock().count(Item::Water), initial - 1);
            assert!(!life.has_completed(TWO_FIELD_WATERING));

            if interrupted {
                assert!(world.village_local_work("water", Some(15)));
            }
            life.people[resident].walk.at = second;
            life.do_work(&mut world, resident, mask, now + WORK);
            assert!(!world.village_plot_needs_water(15));
            assert_eq!(world.stock().count(Item::Water), initial - 2);
            assert_eq!(life.has_completed(TWO_FIELD_WATERING), !interrupted);
            assert!(life.people[resident].job.is_none());
        }
    }

    #[test]
    fn resident_new_bed_tilling_prepares_the_named_plot_for_sowing_once() {
        use crate::living_world::PlotStage;

        let mut world = new_plot_contract_fixture(&[(6, PlotStage::Cleared)]);
        let mut life = VillageLife::default();
        let contract = contracts().find(|c| c.id == "RI101").unwrap();
        let resident = contract.actor.unwrap();
        let now = world.world_ms();
        life.people[resident].walk.at = WILLOW_VILLAGE.crops[6].work;
        assert_eq!(contract.name, "给新菜畦翻土");
        assert!(life.apply_contract(&mut world, resident, contract, now));
        assert_eq!(world.plot_stage(6), Some(PlotStage::Tilled));
        assert!(!life.apply_contract(&mut world, resident, contract, now));
    }

    #[test]
    fn occupied_household_partner_does_not_participate_in_other_resident_service() {
        let mut life = VillageLife::default();
        let mut world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        let actor = ActorKind::Xiaohe;
        assert!(world.reserve_activity(&[actor]));
        let c = contracts().find(|c| c.id == "RI140").unwrap();
        let id = c.actor.unwrap();
        life.people[id].walk.at = world.actor_tile(actor);
        life.orders[id] = Some(WorkOrder {
            opened: now - WORK,
            attempts: 0,
        });
        assert!(!life.service(&mut world, id, c, 1, now));
        assert!(world.activity_deadline(actor).is_some());
    }

    #[test]
    fn native_basketball_reaction_only_fires_for_actual_plugin_lifecycle() {
        let mut life = VillageLife::default();
        let mut world = LivingWorld::new_village_wall_ms(0);
        life.observe_basketball(0, false, &world);
        assert_eq!(life.events(), 0);
        life.observe_basketball(0, true, &world);
        assert_eq!(life.events(), 0);
        life.observe_basketball(0, false, &world);
        assert_eq!(life.events(), 0, "a canceled booking has no completion");
        life.observe_basketball(1, false, &world);
        assert_eq!(life.last_event().map(|(id, _, _)| id), Some("ME074"));
        world.advance_world_ms(STEP);
        life.observe_basketball(1, false, &world);
        assert_eq!(life.events(), 1);
    }

    #[test]
    fn actual_court_start_moves_present_pigeon_to_a_roof_perch() {
        let mut life = VillageLife::default();
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.test_market_visual_stage(8);
        life.ecology[13] = EcoVisit {
            date: Some(world.world_ms() / DAY_MS),
            visits: 1,
        };
        life.observe_basketball(0, true, &world);
        assert!(!life.has_completed("ME082"));
        assert!(life.evidence.refuge[13].is_none());
        life.observe_basketball(0, false, &world);
        for _ in 0..(2 * 24 * 60 / 5) {
            if animal_visible(13, ANIMALS[13], &world) {
                break;
            }
            world.advance_world_ms(5 * MINUTE);
        }
        assert!(animal_visible(13, ANIMALS[13], &world));
        life.observe_basketball(0, true, &world);
        assert!(life.has_completed("ME082"));
        let hall = WILLOW_VILLAGE.building("commons_hall").unwrap();
        let perch = point(hall.door.offset(0, -4).unwrap());
        assert!(life.wildlife_visuals(&world).iter().any(|entry| {
            entry.sprite == "wild_white_pigeon_idle_0" && entry.position == perch
        }));
        assert!(!life.has_completed("ME074"));
    }

    #[test]
    fn saturday_opens_real_stalls_with_finite_stock_and_guests() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let mut open_count = 0;
        let mut guests = 0;
        let mut saw_market_trip = false;
        let mut observed = [0; 5];
        let mut furthest = 0;
        let mut debug_market = (0, 0, [false; 2]);
        let mut trip_legs = Vec::new();
        let start = world.world_ms();
        run_until(&mut world, &mut life, start + 38 * HOUR, |world, life| {
            if let Some(t) = life.trip.filter(|t| usize::from(t.shop) < MOBILE) {
                observed[t.stage as usize] += 1;
                furthest = furthest.max(world.actor_tile(t.actor).x);
                if trip_legs
                    .last()
                    .is_none_or(|&(stage, leg, _, _, _)| stage != t.stage || leg != t.leg)
                {
                    trip_legs.push((
                        t.stage,
                        t.leg,
                        world.minute_of_day(),
                        world.actor_tile(t.actor),
                        world.weather(),
                    ));
                }
            }
            if world.world_ms() % MINUTE == 0 {
                open_count = open_count.max(
                    life.stall_visuals(world)
                        .iter()
                        .filter(|s| s.open && s.site != "home_trade")
                        .count(),
                );
                guests = guests.max(life.present());
                assert!(guests <= MAX_HUMAN_VISITORS);
                saw_market_trip |= life
                    .trip
                    .is_some_and(|trip| usize::from(trip.shop) < MOBILE);
                if world.minute_of_day() == 620 {
                    debug_market = (
                        world.stock().count(Item::Crops),
                        world.stock().count(Item::Seeds),
                        [
                            world.activity_available(ActorKind::Xiaohe),
                            world.activity_available(ActorKind::Ayu),
                        ],
                    );
                }
            }
        });
        assert!(
            phase_active(world.phase_mask(), "weekly_market"),
            "setup mask: {:04b}",
            life.setup
        );
        assert_eq!(open_count, 4);
        assert!(guests > 0);
        assert!(guests <= 2);
        assert!(
            saw_market_trip,
            "stock, household readiness and market window must enable a real outing: {:?} trips {}",
            debug_market, life.trips
        );
        assert!(
            life.trades() > 0,
            "at least one actual inventory exchange; stages {observed:?} x={furthest} trips={} weather={:?}",
            life.trips,
            world.weather()
        );
        assert!(
            life.has_completed("ME037")
                && life.has_completed("ME050")
                && life.has_completed("ME051"),
            "bridge={} opening={} seed={} movement={trip_legs:?}",
            life.has_completed("ME037"),
            life.has_completed("ME050"),
            life.has_completed("ME051")
        );
        assert!(life.validate_for_world(&world));
    }

    #[test]
    fn a_full_merchant_does_not_prevent_a_later_staffed_counter_from_trading() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.test_market_visual_stage(8);
        let first_day = world.world_ms() / DAY_MS;
        let slot_time = (first_day..first_day + 2)
            .flat_map(|day| [660, 735].map(move |minute| day * DAY_MS + minute * MINUTE))
            .find(|&time| {
                let mut snapshot = serde_json::to_value(&world).unwrap();
                snapshot["now_ms"] = time.into();
                let at_time: LivingWorld = serde_json::from_value(snapshot).unwrap();
                !matches!(at_time.weather(), YardWeather::Rain | YardWeather::Snow)
            })
            .unwrap();
        let mut snapshot = serde_json::to_value(&world).unwrap();
        snapshot["now_ms"] = slot_time.into();
        snapshot["target_ms"] = slot_time.into();
        world = serde_json::from_value(snapshot).unwrap();
        let phase = world.phase_mask();
        let mut life = VillageLife::default();
        let seed = shop(1);
        let meal = shop(2);
        for (idx, stall) in [(1, seed), (2, meal)] {
            let seller = &mut life.people[stall.merchant];
            seller.walk = Walker::new(booth_slot(idx), State::AtWork);
            seller.inventory[item_index(stall.goods.item)] = stall.goods.quantity;
            assert!(seller.pack_counter(idx, 0));
        }
        life.people[seed.merchant].inventory[item_index(Item::Crops)] = HARD_STOCK_CAP - 1;
        let stock = world.stock().count(Item::Crops);
        assert!(world.spend_stock(Item::Food, 8));
        assert!(world.spend_stock(Item::Seeds, 2));
        life.offer_trip(&mut world, phase, slot_time);
        let trip = life
            .trip
            .expect("later valid meal counter still has a buyer");
        assert_eq!(trip.shop, 2);
        assert!(trip.bag.is_none());
        assert_eq!(world.stock().count(Item::Crops), stock);

        let actor = trip.actor;
        let mut mature_life = life.clone();
        let mut mature_world = world.clone();
        mature_life.trip.as_mut().unwrap().regular_meal = true;
        let needed = mature_world.stock().count(Item::Crops) - 7;
        assert!(mature_world.spend_stock(Item::Crops, needed));
        let before = mature_world.stock().clone();
        let dock = trip.target;
        world_to_trade_stance(&mut mature_world, actor, dock);
        mature_life.trip.as_mut().unwrap().due = slot_time;
        mature_life.advance_trip(&mut mature_world);
        assert!(mature_life.trip.is_none());
        assert_eq!(mature_life.trades(), 0);
        for item in ITEMS {
            assert_eq!(before.count(item), mature_world.stock().count(item));
        }

        let meal_seller = &mut life.people[meal.merchant];
        meal_seller.inventory[item_index(Item::Crops)] = HARD_STOCK_CAP;
        let original_food = meal_seller.count(Item::Food);
        let handoff = slot(phase, meal.id, "customer").unwrap();
        world_to_trade_stance(&mut world, actor, handoff);
        assert!(world.activity_arrived(actor));
        assert!(world.spend_stock(trip.outgoing.item, trip.outgoing.quantity));
        let trip = life.trip.as_mut().unwrap();
        trip.target = handoff;
        trip.stage = TripStage::Exchange;
        trip.bag = Some(trip.outgoing);
        trip.due = slot_time;
        life.advance_trip(&mut world);
        let returned = life
            .trip
            .expect("failed checkout returns the original basket");
        assert_eq!(returned.stage, TripStage::Inbound);
        assert_eq!(returned.bag, Some(returned.outgoing));
        assert!(!returned.accepted);
        assert_eq!(life.trades(), 0);
        assert_eq!(life.people[meal.merchant].count(Item::Food), original_food);
        assert_eq!(
            life.people[meal.merchant].count(Item::Crops),
            HARD_STOCK_CAP
        );

        let mut fixture = life.people[meal.merchant].clone();
        assert_eq!(fixture.counter_goods(MEAL_SHOP), 2);
        fixture.inventory[item_index(Item::Crops)] = 0;
        assert!(fixture.ready_to_exchange(MEAL_SHOP, false));
        assert!(!fixture.ready_to_exchange(MEAL_SHOP, true));
        fixture.inventory[item_index(Item::Food)] = 1;
        assert!(fixture.ready_to_exchange(MEAL_SHOP, true));
        fixture.inventory[item_index(Item::Crops)] = HARD_STOCK_CAP - 1;
        assert!(!fixture.ready_to_exchange(MEAL_SHOP, true));
    }

    fn world_to_trade_stance(world: &mut LivingWorld, actor: ActorKind, tile: TilePos) {
        world.test_camera_actor(actor, point(tile), Pose::Idle, false);
        let mut at_handoff = serde_json::to_value(&*world).unwrap();
        let actor_state = &mut at_handoff["actors"][actor as usize];
        actor_state["route"] = serde_json::json!([]);
        actor_state["route_next"] = 0.into();
        actor_state["route_goal"] = serde_json::Value::Null;
        actor_state["task"] = serde_json::Value::Null;
        actor_state["stage"] = "Idle".into();
        *world = serde_json::from_value(at_handoff).unwrap();
    }

    #[test]
    fn original_residents_preserve_identity_as_real_age_admits_new_arrivals() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let later = 8;
        assert_eq!(life.residents().filter(|r| r.settled).count(), 6);
        world.set_test_town_age(6);
        let early = world.world_ms() + 10 * MINUTE;
        run_until(&mut world, &mut life, early, |_, _| {});
        assert_eq!(life.people[later].walk.state, State::Unsettled);
        world.set_test_town_age(7);
        let end = world.world_ms() + 8 * HOUR;
        let mut arrived_from_east = false;
        run_until(&mut world, &mut life, end, |_, life| {
            let arrival = &life.people[later].walk;
            arrived_from_east |=
                arrival.state == State::MovingIn && arrival.at == WILLOW_VILLAGE.boundary.spawn;
        });
        assert!(arrived_from_east);
        assert!(life.residents().nth(later).unwrap().settled);
        assert!(life.residents().nth(FIRST_RESIDENTS[0]).unwrap().settled);
        assert_eq!(life.people[12].walk.state, State::Unsettled);
        assert!(life.validate_for_world(&world));
    }

    #[test]
    fn same_clock_replays_after_serializing_mid_walk() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let checkpoint = world.world_ms() + 2_345;
        run_until(&mut world, &mut life, checkpoint, |_, _| {});
        assert!(life.people.iter().any(|r| r.walk.edge.is_some()));
        let mut restored_world: LivingWorld =
            serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        let mut restored_life: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(restored_life.validate_for_world(&restored_world));
        let end = checkpoint + 70 * MINUTE;
        run_until(&mut world, &mut life, end, |_, _| {});
        run_until(&mut restored_world, &mut restored_life, end, |_, _| {});
        assert_eq!(
            serde_json::to_vec(&life).unwrap(),
            serde_json::to_vec(&restored_life).unwrap()
        );
    }

    #[test]
    fn a_week_runs_without_overlapping_residents_or_stranded_home_trades() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(7);
        let mut life = VillageLife::default();
        let end = world.world_ms() + 7 * DAY_MS;
        let mut last_validated_day = None;
        let mut departures = 0;
        run_until(&mut world, &mut life, end, |world, life| {
            let day = world.world_ms() / DAY_MS;
            if world.minute_of_day() == 1420 && last_validated_day != Some(day) {
                assert!(life.validate_for_world(world), "invalid save on day {day}");
                last_validated_day = Some(day);
            }
            if life.people[8].walk.state == State::Away {
                departures += 1;
            }
        });
        assert!(
            departures > 0,
            "the traveling painter must use the exterior boundary"
        );
        assert!(life.present() <= MAX_HUMAN_VISITORS);
        assert!(life.has_completed("ME068"));
        assert!(life.validate_for_world(&world));
    }

    #[test]
    fn delivery_escrow_cannot_be_overwritten_by_another_stock_transfer() {
        let mut life = VillageLife::default();
        let resident = &mut life.people[1];
        let seed = cargo(Item::Seeds, 1);
        resident.inventory[item_index(seed.item)] = HARD_STOCK_CAP;
        assert!(resident.outgoing(seed));
        resident.escrow = Some(seed);
        assert!(!resident.incoming(seed));
        life.clear_order(1);
        assert_eq!(life.people[1].count(seed.item), HARD_STOCK_CAP);
        assert!(life.people[1].escrow.is_none());
    }

    #[test]
    fn exterior_checkpoint_with_opposing_walkers_yields_and_drains() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let now = world.world_ms();
        life.people[8].walk = Walker::new(TilePos::new(189, 56), State::ReturningFromAway);
        life.people[8]
            .walk
            .redirect(house(8).door, State::ReturningFromAway, now);
        life.people[8].walk.wait = 2;
        let mut visitor = Walker::new(TilePos::new(188, 56), State::Leaving);
        visitor.redirect(WILLOW_VILLAGE.boundary.spawn, State::Leaving, now);
        visitor.wait = 2;
        life.guests[0] = Some(Guest {
            identity: 2,
            walk: visitor,
            leave_by: now,
            bed: None,
            wake: None,
            cargo: None,
            received: true,
        });
        run_until(&mut world, &mut life, now + 3 * HOUR, |_, life| {
            assert!(life.occupied_tiles().len() <= VILLAGE_OCCUPANCY);
        });
        assert!(
            life.guests[0].is_none(),
            "permitted exterior throat must drain"
        );
        assert!(life.has_completed("ME068"));
    }

    #[test]
    fn resident_cannot_start_across_an_occupied_main_bridge_until_holder_exits() {
        let world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let now = world.world_ms();
        let mask = world.phase_mask();
        let mut routes = RouteWork::default();
        life.people[0].walk = Walker::new(TilePos::new(65, 42), State::Outbound);
        let mut entering = Walker::new(TilePos::new(55, 42), State::Outbound);
        entering.target = TilePos::new(56, 42);
        entering.segment.push(entering.target);
        assert!(!life.move_walker(
            &world,
            &mut routes,
            &mut entering,
            Some(1),
            None,
            mask,
            now,
            false
        ));
        assert!(entering.edge.is_none());
        life.people[0].walk = Walker::new(TilePos::new(69, 42), State::Outbound);
        entering.wait = 0;
        assert!(!life.move_walker(
            &world,
            &mut routes,
            &mut entering,
            Some(1),
            None,
            mask,
            now + STEP,
            false
        ));
        assert_eq!(entering.edge.unwrap().to, TilePos::new(56, 42));
    }

    #[test]
    fn maker_braids_real_stalks_at_the_houseyard_when_material_runs_low() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let resident = 5;
        let home = house(resident).door;
        let plants = item_index(Item::Crops);
        life.people[resident].inventory[item_index(Item::Rope)] = 0;
        let stock_before = world.stock().count(Item::Rope);
        let plants_before = life.remaining_source[plants];
        let midday = world.world_ms() + 4 * HOUR;
        let mask = world.phase_mask();
        life.people[resident].walk = Walker::new(booth_slot(MOBILE), State::AtWork);
        assert!(!life.production(&world, resident, mask));
        life.people[resident].walk = Walker::new(home, State::AtWork);
        life.do_work(&mut world, resident, mask, midday);
        assert_eq!(life.remaining_source[plants], plants_before - 2);
        assert_eq!(life.people[resident].count(Item::Rope), 1);
        assert_eq!(world.stock().count(Item::Rope), stock_before);
    }

    #[test]
    fn orchard_yield_funds_one_real_farmers_meal_before_south_fields_open() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let mask = world.phase_mask();
        let now = world.world_ms();
        let fruit_grower = 6;
        let farmer = 10;
        let orchard = resolve(mask, "orchard_tools").unwrap();
        let home = house(farmer).door;
        let source = item_index(Item::Crops);
        let original_farmer = life.people[farmer].count(Item::Crops);
        let original_grower = life.people[fruit_grower].count(Item::Crops);
        let original_food = life.people[farmer].count(Item::Food);
        life.remaining_source[source] = 5;
        life.people[fruit_grower].walk.at = TilePos::new(0, 0);
        life.people[farmer].walk.at = home;
        assert!(!life.production(&world, farmer, mask));
        assert_eq!(life.remaining_source[source], 5);
        life.people[farmer].walk.at = orchard;
        life.do_work(&mut world, farmer, mask, now);
        assert_eq!(life.remaining_source[source], 3);
        assert_eq!(life.people[farmer].count(Item::Crops), original_farmer);
        assert_eq!(
            life.resident_held(farmer),
            Some(VisitorHeld::Goods(Item::Crops))
        );
        life.do_work(&mut world, farmer, mask, now + WORK);
        assert_eq!(life.remaining_source[source], 3);
        life.people[fruit_grower].walk.at = orchard;
        assert!(!life.production(&world, fruit_grower, mask));
        assert_eq!(
            life.people[fruit_grower].count(Item::Crops),
            original_grower
        );
        assert_eq!(life.people[farmer].count(Item::Food), original_food);
        life.people[farmer].walk.at = home;
        life.people[farmer].stow_produce();
        life.do_work(&mut world, farmer, mask, now + 2 * WORK);
        assert_eq!(life.people[farmer].count(Item::Food), original_food + 2);
        assert_eq!(life.people[farmer].count(Item::Crops), original_farmer);

        let next_mask = mask.with(WILLOW_VILLAGE.phase("cooperative_farm").unwrap());
        life.people[farmer].walk.at = orchard;
        assert!(!life.production(&world, farmer, next_mask));
        let south = resolve(next_mask, "south_grain").unwrap();
        life.people[farmer].walk.at = south;
        life.remaining_source[source] = 4;
        let batch = if matches!(world.weather(), YardWeather::Rain | YardWeather::Snow) {
            2
        } else {
            4
        };
        assert!(life.production(&world, farmer, next_mask));
        assert_eq!(life.remaining_source[source], 4 - batch);
        assert_eq!(
            life.people[farmer].produce_in_transit,
            Some(cargo(Item::Crops, batch))
        );
        life.people[farmer].walk.at = home;
        life.people[farmer].stow_produce();
        assert_eq!(
            life.people[farmer].count(Item::Crops),
            original_farmer + batch
        );
    }

    #[test]
    fn supper_debits_only_the_residents_uncommitted_stock_at_home_once_daily() {
        let mut life = VillageLife::default();
        let cook = &mut life.people[3];
        let home = house(3).door;
        let reserve = shop(2).goods.quantity;
        cook.inventory[item_index(Item::Food)] = reserve + 2;
        cook.walk = Walker::new(TilePos::new(1, 1), State::AtWork);
        cook.eat_home_supper(home, 3, reserve);
        assert_eq!(cook.count(Item::Food), reserve + 2);
        cook.walk = Walker::new(home, State::Inside);
        cook.eat_home_supper(home, 3, reserve);
        cook.eat_home_supper(home, 3, reserve);
        assert_eq!(cook.count(Item::Food), reserve + 1);
        cook.eat_home_supper(home, 4, reserve);
        cook.eat_home_supper(home, 5, reserve);
        assert_eq!(cook.count(Item::Food), reserve);
        assert_eq!(cook.supper_day, Some(4));
    }

    #[test]
    #[ignore = "replays the ten construction permits through real day 60"]
    fn construction_material_barter_unblocks_market_square() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(60);
        let mut life = VillageLife::default();
        let start = world.world_ms();
        let mut supplied_stone = false;
        let mut paid_stone_project = false;
        run_until(
            &mut world,
            &mut life,
            start + 365 * DAY_MS,
            |world, life| {
                if world.world_ms() % DAY_MS == 0 && world.world_ms() / DAY_MS % 14 == 0 {
                    let ayu = &world.actors()[1];
                    eprintln!(
                        "day={} projects={} trades={} trips={} stock(s/w/r/food)={}/{}/{}/{} shortfall={:?} active={:?} reserve={} surplus_sale={} Ayu={:?} tile={:?} goal={:?} needs(h/t/s)={}/{}/{} gate_occupied={}",
                        world.world_ms() / DAY_MS,
                        world.story().completed(),
                        life.trades(),
                        life.trips(),
                        world.stock().count(Item::Stone),
                        world.stock().count(Item::Wood),
                        world.stock().count(Item::Rope),
                        world.stock().count(Item::Food),
                        world.next_construction_material_shortfall(),
                        world.story().active(),
                        world.village_chapter_completed("Q15"),
                        world.village_chapter_completed("Q16"),
                        ayu.activity,
                        world.actor_tile(ActorKind::Ayu),
                        world.actor_destination(ActorKind::Ayu),
                        ayu.hunger(),
                        ayu.thirst(),
                        ayu.stamina(),
                        resolve(world.phase_mask(), "market_gate")
                            .is_some_and(|gate| life.occupied_by_village(gate)),
                    );
                    if ayu.thirst() == 100 && ayu.hunger() == 100 {
                        let dock = slot(world.phase_mask(), "pantry_loading", "load").unwrap();
                        let residents: Vec<_> = life
                            .people
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| !r.walk.hidden() && distance(r.walk.at, dock) <= 2)
                            .map(|(id, r)| (id, r.walk.at, r.walk.state, r.walk.target))
                            .collect();
                        eprintln!(
                            "pantry nearby residents={residents:?} Ayu task={} Xiaohe={:?}/{:?} next={:?}",
                            serde_json::to_value(ayu).unwrap()["task"],
                            world.actors()[0].activity,
                            world.actor_tile(ActorKind::Xiaohe),
                            world.actor_next_tile(ActorKind::Xiaohe),
                        );
                    }
                }
                supplied_stone |= life
                    .trip
                    .is_some_and(|t| t.accepted && t.shop as usize == MOBILE + 1);
                paid_stone_project |= supplied_stone
                    && world.story().active().is_some_and(|p| {
                        crate::living_world::story::CHAPTERS[usize::from(p.id)].material
                            == crate::living_world::story::Material::Stone
                    });
            },
        );
        assert!(
            supplied_stone,
            "the project must acquire stone from actual local barter"
        );
        assert!(
            paid_stone_project,
            "a stone-consuming permit must start after genuine barter"
        );
        assert_eq!(
            world.story().completed(),
            10,
            "construction under the first ten permits stalled: {:?}",
            world.next_construction_material_shortfall()
        );
        assert!(world.story().active().is_none());
        assert!(life.validate_for_world(&world));
    }

    #[test]
    #[ignore = "advances all construction, village market and neighbour seasons"]
    fn autonomous_social_appointments_progress_through_two_resident_moves() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(365);
        let mut life = VillageLife::default();
        life.apply_supply_requests(&mut world);
        // At the slower walking pace Q35 first finished near world day 980.
        // The old 1,000-day cutoff left little time for its dependent visits.
        let end = world.world_ms() + 2_000 * DAY_MS;
        let mut reported = 0;
        let mut reserve = [0_u64; 5];
        let mut max_crops_with_food = 0;
        let mut qualifying_trips = [0_u16; 5];
        let mut previous = None;
        let mut q47_days = [None; 4];
        let mut q47_windows = [0_u64; 5];
        let mut q47_max_crop = 0;
        let mut q47_last_stage = None;
        let mut q47_stages = Vec::new();
        let mut q65_nights = [0_u16; 4];
        let mut q65_last_day = None;
        let mut q65_food_floor = u16::MAX;
        let mut q65_food_ceiling = 0;
        run_until(&mut world, &mut life, end, |world, life| {
            let day = world.world_ms() / DAY_MS;
            if life.village_chapter_completed("Q64")
                && !life.village_chapter_completed("Q65")
                && world.minute_of_day() >= 1435
                && q65_last_day != Some(day)
            {
                q65_last_day = Some(day);
                let food = world.stock().count(Item::Food);
                q65_food_floor = q65_food_floor.min(food);
                q65_food_ceiling = q65_food_ceiling.max(food);
                let dinner = life.people[10..12].iter().all(|resident| {
                    resident.supper_day == Some(day) && resident.walk.state == State::Inside
                });
                q65_nights[usize::from(dinner) * 2 + usize::from(food >= 6)] += 1;
            }
            if day >= 112
                && !life.village_chapter_completed("Q47")
                && (q47_days[0].is_some()
                    || (life.village_chapter_completed("Q41")
                        && world.village_chapter_completed("Q46")
                        && world.village_chapter_completed("Q16")))
            {
                q47_days[0].get_or_insert(day);
                let crops = world.stock().count(Item::Crops);
                q47_max_crop = q47_max_crop.max(crops);
                let enough = crops >= 8;
                let minute = world.minute_of_day();
                let daytime = (465..870).contains(&minute);
                let free = life.trip.is_none() && life.present() < MAX_HUMAN_VISITORS;
                let seed = life.people[shop(MOBILE).merchant].count(Item::Seeds)
                    >= shop(MOBILE).goods.quantity;
                for (entry, reached) in q47_days[1..].iter_mut().zip([
                    enough,
                    enough && daytime,
                    enough && daytime && free && seed,
                ]) {
                    if reached {
                        entry.get_or_insert(day);
                    }
                }
                for (entry, reached) in q47_windows.iter_mut().zip([
                    enough,
                    enough && daytime,
                    enough && daytime && free,
                    enough && daytime && free && seed,
                    enough
                        && daytime
                        && free
                        && seed
                        && world.actors()[..2].iter().any(|host| {
                            host.stamina() >= 42 && host.hunger() < 60 && host.thirst() < 66
                        }),
                ]) {
                    *entry += u64::from(reached);
                }
                let stage = life
                    .social
                    .market_probe()
                    .filter(|(chapter, _)| *chapter == 47);
                if stage != q47_last_stage && q47_stages.len() < 24 {
                    q47_stages.push((day, minute, stage, crops));
                }
                q47_last_stage = stage;
            }
            if world.village_chapter_completed("Q15") && !world.village_chapter_completed("Q16") {
                let crops = world.stock().count(Item::Crops);
                let food = world.stock().count(Item::Food);
                if food >= 6 {
                    max_crops_with_food = max_crops_with_food.max(crops);
                }
                for (bucket, condition) in reserve.iter_mut().zip([
                    crops >= 8,
                    food >= 6,
                    world.stock().count(Item::Seeds) >= 2,
                    crops >= 8 && food >= 6 && world.stock().count(Item::Seeds) >= 2,
                    life.trip.is_none()
                        && crops >= 8
                        && food >= 6
                        && world.stock().count(Item::Seeds) >= 2,
                ]) {
                    *bucket += u64::from(condition);
                }
                let stage = life.trip.map(|trip| (trip.started, trip.stage));
                if stage != previous {
                    if let Some(trip) = life.trip
                        && trip.outgoing.item == Item::Crops
                        && crops >= 6
                        && food >= 6
                        && world.stock().count(Item::Seeds) >= 2
                    {
                        qualifying_trips[trip.stage as usize] += 1;
                    }
                    previous = stage;
                }
            }
            if day >= reported + 28 {
                reported = day;
                let own: Vec<_> = (36..=71)
                    .filter(|q| life.village_chapter_completed(&format!("Q{q}")))
                    .collect();
                let core: Vec<_> = (1..=59)
                    .filter(|q| world.village_chapter_completed(&format!("Q{q:02}")))
                    .collect();
                eprintln!(
                    "day={day} own={own:?} core={core:?} social={:?} trips={} trades={}",
                    life.social,
                    life.trips(),
                    life.trades()
                );
                if day >= 700 && !life.village_chapter_completed("Q65") {
                    for id in [10, 11] {
                        let r = &life.people[id];
                        eprintln!(
                            "neighbor {id} at={:?} target={:?} state={:?} wait={} food={} crops={} supper={:?} made={:?}",
                            r.walk.at,
                            r.walk.target,
                            r.walk.state,
                            r.walk.wait,
                            r.count(Item::Food),
                            r.count(Item::Crops),
                            r.supper_day,
                            r.tailored_day
                        );
                    }
                }
            }
        });
        eprintln!(
            "Q16 reserve windows={reserve:?}, max crops with food={max_crops_with_food}, qualifying trip stages={qualifying_trips:?}"
        );
        eprintln!(
            "Q47 first eligible days={q47_days:?}, reserve windows={q47_windows:?}, peak crops={q47_max_crop}, stages={q47_stages:?}"
        );
        eprintln!(
            "Q65 nights [neither, stock only, dinners only, both]={q65_nights:?}; bedtime food range={q65_food_floor}–{q65_food_ceiling}"
        );
        if !world.village_chapter_completed("Q35") {
            let data = serde_json::to_value(&world).unwrap();
            eprintln!(
                "core guest trial: {} pantry={} stock={} actors={}",
                data["village"]["renovation"],
                data["village"]["pantry_plan"],
                data["stock"],
                data["actors"]
            );
        }
        if !world.village_chapter_completed("Q58") {
            let data = serde_json::to_value(&world).unwrap();
            eprintln!(
                "core market: {} grain={} stock={} actors={}",
                data["village"]["market_story"],
                data["village"]["grain"],
                data["stock"],
                data["actors"]
            );
        }
        assert!(life.validate_for_world(&world));
        let required = [
            36, 37, 38, 39, 40, 41, 47, 48, 49, 50, 51, 52, 53, 60, 61, 62, 63, 64, 65, 66, 67, 68,
            69,
        ];
        let missing: Vec<_> = required
            .into_iter()
            .map(|n| format!("Q{n}"))
            .filter(|id| !life.village_chapter_completed(id))
            .collect();
        let last_day = world.world_ms() / DAY_MS;
        assert!(
            missing.is_empty(),
            "world day {last_day}: missing {missing:?}; {:?}",
            life.social
        );
    }

    #[test]
    fn market_inventory_is_identical_after_checkpointing_a_loaded_trip() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let mut checkpoint = None;
        let end = world.world_ms() + 38 * HOUR;
        run_until(&mut world, &mut life, end, |world, life| {
            if checkpoint.is_none()
                && !world.navigation_pending()
                && life.trip.is_some_and(|trip| {
                    usize::from(trip.shop) < MOBILE
                        && trip.stage == TripStage::Outbound
                        && trip.bag.is_some()
                })
            {
                checkpoint = Some((
                    serde_json::to_vec(world).unwrap(),
                    serde_json::to_vec(life).unwrap(),
                ));
            }
        });
        assert!(life.market_outings > 0 && life.trades() > 0);
        let (w, l) = checkpoint.expect("test must observe a real loaded trip");
        let mut loaded_world: LivingWorld = serde_json::from_slice(&w).unwrap();
        let mut loaded_life: VillageLife = serde_json::from_slice(&l).unwrap();
        assert!(loaded_life.validate_for_world(&loaded_world));
        run_until(&mut loaded_world, &mut loaded_life, end, |_, _| {});
        assert_eq!(
            world.story().market_trades(),
            loaded_world.story().market_trades()
        );
        assert_eq!(
            serde_json::to_vec(&life).unwrap(),
            serde_json::to_vec(&loaded_life).unwrap()
        );
        for item in ITEMS {
            assert_eq!(world.stock().count(item), loaded_world.stock().count(item));
        }
    }

    #[test]
    fn canceled_order_preserves_the_original_bag() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let (actor, before) = [ActorKind::Xiaohe, ActorKind::Ayu]
            .into_iter()
            .find_map(|actor| {
                world
                    .reserve_market_trip(actor)
                    .then_some((actor, world.stock().count(Item::Crops)))
            })
            .expect("fresh protagonist can accept the fixture");
        let now = world.world_ms();
        let mut t = Trip {
            actor,
            shop: 1,
            started: now,
            due: now,
            expires: now,
            stage: TripStage::Outbound,
            target: OUTBOUND_WAYPOINTS[0],
            bag: Some(SHOPS[1].cost),
            outgoing: SHOPS[1].cost,
            incoming: SHOPS[1].goods,
            accepted: false,
            leg: 0,
            hold: true,
            regular_meal: false,
        };
        assert!(world.spend_stock(t.outgoing.item, t.outgoing.quantity));
        life.reserved = Some(actor);
        life.trip = Some(t);
        life.advance_trip(&mut world);
        t = life.trip.unwrap();
        assert_eq!(t.stage, TripStage::Inbound);
        assert_eq!(t.bag, Some(t.outgoing));
        assert!(!t.accepted);
        assert_eq!(
            world.stock().count(Item::Crops) + t.bag.unwrap().quantity,
            before
        );
        assert_eq!(life.trades(), 0);
    }

    #[test]
    fn service_letter_waits_at_its_source_and_returns_to_the_same_address() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let id = 0;
        let home = house(id).door;
        let mask = world.phase_mask();
        let (code, contract) = contracts()
            .enumerate()
            .find(|(_, c)| c.id == "RI001")
            .unwrap();
        let destination = life.contract_site(&world, contract, mask, 0).unwrap();
        assert_ne!(home, destination);
        let now = world.world_ms();
        life.people[id].walk = Walker::new(destination, State::AtWork);
        life.people[id].job = Some(code as u16);
        life.orders[id] = Some(WorkOrder {
            opened: now,
            attempts: 0,
        });
        assert_eq!(life.job_site(&world, id, contract, mask), Some(home));
        life.do_work(&mut world, id, mask, now);
        assert_eq!(life.resident_held(id), None);
        assert!(!life.has_completed("RI001"));

        life.people[id].walk = Walker::new(home, State::AtWork);
        life.do_work(&mut world, id, mask, now);
        assert_eq!(life.resident_held(id), Some(VisitorHeld::Parcel("letter")));
        life.people[id].walk = Walker::new(destination, State::AtWork);
        life.finish_daily_order(id, now / DAY_MS);
        assert_eq!(life.resident_held(id), Some(VisitorHeld::Parcel("letter")));
        let mut life: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(life.validate_for_world(&world));
        life.schedule(&mut world, mask, now);
        assert_eq!(life.people[id].walk.target, home);
        assert!(life.people[id].walk.moving());
        for _ in 0..150 {
            world.advance_world_ms(MINUTE);
            life.advance(&mut world);
            if life.resident_held(id).is_none() {
                break;
            }
        }
        assert_eq!(life.people[id].walk.at, home);
        assert_eq!(life.resident_held(id), None);
        assert!(!life.has_completed("RI001"));
        assert!(life.services.has_asset("letter", id as u8));
    }

    #[test]
    fn cancelled_repair_keeps_material_through_save_and_returns_it_to_the_cupboard() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let mask = world.phase_mask();
        let (code, contract) = contracts()
            .enumerate()
            .find(|(_, c)| c.id == "RI021")
            .unwrap();
        let id = contract.actor.unwrap();
        let home = house(id).door;
        let site = resolve(mask, "home_workshop.door").unwrap();
        let now = world.world_ms();
        let before = life.people[id].count(Item::Wood);
        let wear = life.wear_on("tool_handle");
        life.people[id].walk = Walker::new(site, State::AtWork);
        life.people[id].job = Some(code as u16);
        life.orders[id] = Some(WorkOrder {
            opened: now,
            attempts: 0,
        });
        life.do_work(&mut world, id, mask, now);
        assert_eq!(life.people[id].count(Item::Wood), before);
        assert!(life.resident_held(id).is_none());
        life.people[id].walk = Walker::new(home, State::AtWork);
        life.do_work(&mut world, id, mask, now);
        assert_eq!(life.people[id].count(Item::Wood), before - 1);
        assert_eq!(life.resident_held(id), Some(VisitorHeld::Goods(Item::Wood)));
        life.people[id].walk = Walker::new(site, State::AtWork);
        life.finish_daily_order(id, now / DAY_MS);
        let mut life: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(life.validate_for_world(&world));
        assert!(!life.return_service_material(&mut world, id, mask, now));
        assert_eq!(life.people[id].walk.target, home);
        assert_eq!(life.resident_held(id), Some(VisitorHeld::Goods(Item::Wood)));
        let mut routes = RouteWork::default();
        for step in 0..250 {
            let due = now + u64::try_from(step).unwrap() * MINUTE;
            let mut walker =
                std::mem::replace(&mut life.people[id].walk, Walker::new(home, State::Inside));
            routes.budget = 2_048;
            if walker.due <= due
                && life.move_walker(
                    &world,
                    &mut routes,
                    &mut walker,
                    Some(id),
                    None,
                    mask,
                    due,
                    true,
                )
            {
                life.arrive(id, &mut walker, State::Outbound, due);
            }
            life.people[id].walk = walker;
            if life.people[id].walk.edge.is_none()
                && life.return_service_material(&mut world, id, mask, due)
            {
                break;
            }
        }
        assert_eq!(life.people[id].walk.at, home);
        assert_eq!(life.people[id].count(Item::Wood), before);
        assert!(life.resident_held(id).is_none());
        assert_eq!(life.wear_on("tool_handle"), wear);
        assert!(!life.has_completed("RI021"));
    }

    #[test]
    fn neighbour_meals_are_cooked_from_private_harvest_and_deposited_only_at_the_dock() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let start = world.world_ms();
        run_until(
            &mut world,
            &mut VillageLife::default(),
            start + 6 * HOUR,
            |_, _| {},
        );
        let now = world.world_ms();
        let mask = world.phase_mask();
        let mut life = VillageLife::default();
        let mut evidence = serde_json::to_value(&life.social).unwrap();
        // Q61/Q62/Q63/Q64 precede this household contribution.
        evidence["completed"] = (0b1111_u32 << 14).into();
        evidence["neighbours"]["first_day"] = (now / DAY_MS).into();
        evidence["neighbours"]["second_day"] = (now / DAY_MS).into();
        life.social = serde_json::from_value(evidence).unwrap();
        let stock = world.stock().count(Item::Food);
        assert!(world.spend_stock(Item::Food, stock));
        let id = 10;
        let home = house(id).door;
        let dock = slot(mask, "pantry_loading", "load").unwrap();
        life.people[id].inventory[item_index(Item::Crops)] = 6;
        life.people[id].walk = Walker::new(home, State::AtWork);
        assert!(life.schedule_neighbour_meal(&mut world, id, mask, now));
        assert_eq!(life.people[id].count(Item::Crops), 1);
        assert_eq!(life.people[id].count(Item::Food), 2);
        assert_eq!(life.resident_held(id), Some(VisitorHeld::Goods(Item::Food)));
        assert_eq!(world.stock().count(Item::Food), 0);
        let mut life: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(life.validate_for_world(&world));
        assert!(life.schedule_neighbour_meal(&mut world, id, mask, now));
        assert_eq!(world.stock().count(Item::Food), 0);
        life.people[id].walk = Walker::new(dock, State::AtWork);
        assert!(!life.schedule_neighbour_meal(&mut world, id, mask, now));
        assert_eq!(world.stock().count(Item::Food), NEIGHBOUR_MEAL_PORTIONS);
        assert_eq!(life.people[id].count(Item::Food), 2);
        assert_eq!(life.resident_held(id), None);
        assert!(!life.schedule_neighbour_meal(&mut world, id, mask, now));
        assert_eq!(world.stock().count(Item::Food), NEIGHBOUR_MEAL_PORTIONS);
    }

    #[test]
    fn a_full_pantry_requires_the_original_carrier_to_return_again() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let actor = ActorKind::Ayu;
        let mask = world.phase_mask();
        let dock = slot(mask, "pantry_loading", "load").unwrap();
        let away = resolve(mask, "home_workshop.door").unwrap();
        let shop = shop(1);
        let bag = shop.goods;
        while world.exchange_stock_atomic(&[], &[(bag.item, 1)]) {}
        let full = world.stock().count(bag.item);
        assert!(world.reserve_market_trip(actor));
        assert!(world.move_for_activity_with_load(actor, point(dock), true));
        for _ in 0..45 {
            world.advance_world_ms(MINUTE);
            if world.activity_arrived(actor) {
                break;
            }
        }
        assert_eq!(world.actor_tile(actor), dock);
        let now = world.world_ms();
        life.reserved = Some(actor);
        life.trip = Some(Trip {
            actor,
            shop: 1,
            started: now,
            due: now,
            expires: now + HOUR,
            stage: TripStage::Unload,
            target: dock,
            bag: Some(bag),
            outgoing: shop.cost,
            incoming: bag,
            accepted: true,
            leg: INBOUND_WAYPOINTS.len() as u8,
            hold: true,
            regular_meal: false,
        });
        life.advance_trip(&mut world);
        assert_eq!(world.stock().count(bag.item), full);
        assert!(!life.trip.unwrap().hold);
        assert_eq!(
            life.household_held(actor),
            Some(VisitorHeld::Goods(bag.item))
        );

        assert!(world.reserve_market_trip(actor));
        assert!(world.move_for_activity_with_load(actor, point(away), true));
        for _ in 0..45 {
            world.advance_world_ms(MINUTE);
            if world.activity_arrived(actor) {
                break;
            }
        }
        assert_eq!(world.actor_tile(actor), away);
        world.end_activity(&[actor], 0);
        life.trip.as_mut().unwrap().due = world.world_ms();
        life.advance_trip(&mut world);
        assert!(!life.trip.unwrap().hold);

        let space = bag.quantity + 6;
        assert!(world.spend_stock(bag.item, space));
        let snapshot = serde_json::to_vec(&life).unwrap();
        let mut life: VillageLife = serde_json::from_slice(&snapshot).unwrap();
        assert!(life.validate_for_world(&world));
        life.trip.as_mut().unwrap().due = world.world_ms();
        life.advance_trip(&mut world);
        assert_eq!(world.stock().count(bag.item), full - space);
        assert_eq!(
            life.household_held(actor),
            Some(VisitorHeld::Goods(bag.item))
        );
        assert_eq!(life.trip.unwrap().actor, actor);

        for _ in 0..45 {
            world.advance_world_ms(MINUTE);
            life.advance_trip(&mut world);
            if life.trip.is_none() {
                break;
            }
        }
        assert_eq!(world.actor_tile(actor), dock);
        assert!(life.trip.is_none());
        assert!(life.household_held(actor).is_none());
        assert_eq!(world.stock().count(bag.item), full - space + bag.quantity);
    }
}

#[derive(Default)]
struct RouteWork {
    nav: Option<MapNav>,
    active: Option<RouteSearch>,
    budget: usize,
    waiting: bool,
}

struct RouteSearch {
    owner: (Option<usize>, Option<usize>),
    start: TilePos,
    target: TilePos,
    map: MapView<'static>,
}

impl Clone for RouteWork {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for RouteWork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouteWork")
            .field("waiting", &self.waiting)
            .finish()
    }
}

impl RouteWork {
    fn find(
        &mut self,
        world: &LivingWorld,
        walker: &mut Walker,
        mask: PhaseMask,
        owner: (Option<usize>, Option<usize>),
        busy: impl Fn(TilePos) -> bool,
    ) -> bool {
        if self.budget == 0 {
            self.waiting = true;
            return false;
        }
        let nav = self.nav.get_or_insert_with(MapNav::new);
        let result = if self
            .active
            .as_ref()
            .is_some_and(|p| p.owner == owner && p.start == walker.at && p.target == walker.target)
        {
            nav.advance(&self.active.as_ref().unwrap().map, self.budget)
        } else {
            let mut map = world
                .navigation_view()
                .unwrap_or_else(|| MapView::new(&WILLOW_VILLAGE, mask));
            if walker.wait >= 2 {
                for y in walker.at.y.saturating_sub(3)..=(walker.at.y + 3).min(map.height() - 1) {
                    for x in walker.at.x.saturating_sub(3)..=(walker.at.x + 3).min(map.width() - 1)
                    {
                        let p = TilePos::new(x, y);
                        if p != walker.at && p != walker.target && busy(p) {
                            map.set_blocked(p, true);
                        }
                    }
                }
            }
            let request = nav.begin(&map, walker.at, walker.target);
            let result = if request == SearchResult::Pending {
                nav.advance(&map, self.budget)
            } else {
                request
            };
            self.active = Some(RouteSearch {
                owner,
                start: walker.at,
                target: walker.target,
                map,
            });
            result
        };
        self.budget -= nav.last_work();
        match result {
            SearchResult::Pending => {
                self.waiting = true;
                false
            }
            SearchResult::Found(segment) => {
                self.active = None;
                walker.segment = segment.steps().to_vec();
                walker.next = 0;
                !walker.segment.is_empty()
            }
            _ => {
                self.active = None;
                false
            }
        }
    }
}
