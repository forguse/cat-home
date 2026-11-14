//! Fixed terrain and household simulation. Rendering reads this state without advancing it.

use crate::map_data::{PhaseMask, Terrain, TilePos, WILLOW_VILLAGE};
use crate::map_nav::{MapNav, SearchResult};
use crate::map_view::MapView;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;

#[path = "story.rs"]
pub mod story;
use story::housing::{LOTS as HOUSING_LOTS, Order as HousingOrder};
use story::improvements::{MAX_STRUCTURE_LEVEL, structure_work};
use story::{CHAPTERS, CropBatches, Material, Project, SITES, StoryState};
#[path = "world_layout.rs"]
mod village;
#[path = "grain.rs"]
mod grain;
#[path = "soil.rs"]
mod soil;
#[path = "workshop_story.rs"]
mod workshop_story;
#[path = "pantry_plan.rs"]
mod pantry_plan;
#[path = "preserves.rs"]
mod preserves;
#[path = "renovation.rs"]
mod renovation;
#[path = "road_story.rs"]
mod road_story;
#[path = "memories.rs"]
mod memories;
#[path = "town_calendar.rs"]
mod town_calendar;
#[path = "movement.rs"]
pub mod movement;
#[path = "footprints.rs"]
mod footprints;
pub use footprints::Footprint;
pub use town_calendar::TownEra;
pub use story::{
    DevelopmentKind, Fixture, FixtureStatus, FormerHomeUse, HousePlan, HousingPhase,
    ImprovementOrder,
};

pub const MAP_WIDTH: usize = 96;
pub const MAP_HEIGHT: usize = 64;
pub const TILE_PX: i32 = 8;
pub const DAY_MS: u64 = 86_400_000;
const MINUTE: u64 = 60_000;
const HOUR: u64 = 60 * MINUTE;
const MAX_EVENTS_PER_ADVANCE: usize = 4_096;
const MAX_ROUTE: usize = MAP_WIDTH + MAP_HEIGHT;
const CELLS: usize = MAP_WIDTH * MAP_HEIGHT;
const START_MS: u64 = 8 * HOUR;
const PANTRY: Cell = Cell(30, 14);
const ROAD_TRADE: Cell = Cell(6, 25);
const MARKET_TRADE: Cell = Cell(19, 34);
const BRIDGE_X: i32 = 12;
const WELL: Cell = Cell(31, 17);
const STOVE: Cell = Cell(19, 12);
const BOWL: Cell = Cell(14, 16);
const CAT_BED: Cell = Cell(15, 16);
const HOUSE_DOORS: [Cell; 2] = [Cell(12, 14), Cell(24, 14)];
const HOUSE_SIDE: [Cell; 2] = [Cell(14, 14), Cell(26, 14)];
const LEGACY_WORKSHOP_DOOR: Cell = Cell(34, 13);
const WORKSHOP_DOOR: Cell = Cell(36, 13);
const WORKSHOP_SITE: usize = 4;
const POND_BANK: Cell = Cell(12, 17);
const CAT_SPOTS: [Cell; 4] = [Cell(14, 13), Cell(10, 17), Cell(8, 15), Cell(20, 15)];
const TABLE: [Cell; 3] = [Cell(16, 16), Cell(19, 16), Cell(16, 17)];
const STOCK_CAP: u16 = 128;
const BOWL_CAP: u16 = 3;
const WELL_CAP: u16 = 12;
const NEIGHBOUR_MEAL_TARGET: u16 = 10;
const OLD_WALK_WORLD_MS: [u64; 3] = [10_000, 10_000, 8_000];
const EDGE_RETRY_WORLD_MS: u64 = 24_000;
const PASS_CLEAR_LIMIT: u64 = 6 * MINUTE;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ActorKind {
    Xiaohe,
    Ayu,
    Orange,
}

impl ActorKind {
    fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Xiaohe => "小禾",
            Self::Ayu => "阿榆",
            Self::Orange => "橘子",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Pose {
    Idle,
    Walk,
    Carry,
    Work,
    Eat,
    Drink,
    Rest,
    Sleep,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Activity {
    Idle,
    GetTool,
    Clear,
    Till,
    Sow,
    Water,
    Harvest,
    Store,
    Cook,
    Eat,
    GetWater,
    FillBowl,
    Drink,
    FellTree,
    CarryWood,
    Build,
    Rest,
    Sleep,
    Fish,
    ReturnTool,
    ChangeClothes,
    Explore,
    Roadwork,
    Trade,
    TendGarden,
    OutsideActivity,
    WashHarvest,
    SortHarvest,
    DryHarvest,
    PackHarvest,
    Orchard,
    DrainField,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommunityAction {
    ShareFood(ActorKind),
    ShareWater(ActorKind),
    CleanCatBowl,
    FillCatBowl,
    FishRelease,
    Mend(Fixture),
}

impl Activity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "歇脚",
            Self::GetTool => "取工具",
            Self::Clear => "开垦",
            Self::Till => "翻土",
            Self::Sow => "播种",
            Self::Water => "浇水",
            Self::Harvest => "收获",
            Self::Store => "入库",
            Self::Cook => "做饭",
            Self::Eat => "吃饭",
            Self::GetWater => "打水",
            Self::FillBowl => "添水",
            Self::Drink => "饮水",
            Self::FellTree => "砍树",
            Self::CarryWood => "搬木料",
            Self::Build => "修造",
            Self::Rest => "休息",
            Self::Sleep => "睡觉",
            Self::Fish => "钓鱼",
            Self::ReturnTool => "归还工具",
            Self::ChangeClothes => "回屋换衣",
            Self::Explore => "在院子里散步",
            Self::Roadwork => "修建村路与公共设施",
            Self::Trade => "运送菜篮换取材料",
            Self::TendGarden => "照料公共花园",
            Self::OutsideActivity => "参加活动",
            Self::WashHarvest => "清洗收获",
            Self::SortHarvest => "分拣收获",
            Self::DryHarvest => "晾晒收获",
            Self::PackHarvest => "收晒装袋",
            Self::Orchard => "果园养护与采果",
            Self::DrainField => "疏沟排水",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Tool {
    Hoe,
    WateringCan,
    Axe,
    Hammer,
    HarvestBasket,
    FishingRod,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Item {
    Seeds,
    Water,
    Crops,
    Food,
    Wood,
    Stone,
    Rope,
    Fish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ground {
    Grass,
    Path,
    Soil,
    Water,
    Rock,
    Building,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardSurface {
    Doorstep,
    Dining,
    Loading,
    Bank,
    GardenAisle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Outfit {
    Garden,
    Work,
    Raincoat,
    Home,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardWeather {
    Clear,
    Clouds,
    Rain,
    Snow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardSeason {
    Spring,
    Summer,
    Autumn,
    Winter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PlotStage {
    Grass,
    Cleared,
    Tilled,
    Seed,
    Growing,
    Mature,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TreeStage {
    Standing,
    Felled,
    Stump,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OrchardPhase {
    Sapling,
    Dormant,
    Bud,
    Flower,
    GreenFruit,
    Ripe,
    AfterHarvest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BuildingKind {
    WestHouse,
    EastHouse,
    Kitchen,
    Storehouse,
    Workshop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn at_tile(x: i32, y: i32) -> Self {
        Self {
            x: (x * TILE_PX + TILE_PX / 2) as f32,
            y: (y * TILE_PX + TILE_PX / 2) as f32,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Cell(u16, u16);

impl Cell {
    fn point(self) -> Point {
        Point::at_tile(i32::from(self.0), i32::from(self.1))
    }
    fn index(self) -> usize {
        usize::from(self.1) * MAP_WIDTH + usize::from(self.0)
    }
    fn valid(self) -> bool {
        usize::from(self.0) < MAP_WIDTH && usize::from(self.1) < MAP_HEIGHT
    }
    fn at(point: Point) -> Option<Self> {
        if !point.x.is_finite() || !point.y.is_finite() || point.x < 0.0 || point.y < 0.0 {
            return None;
        }
        let x = (point.x / TILE_PX as f32) as usize;
        let y = (point.y / TILE_PX as f32) as usize;
        (x < usize::from(crate::map_data::MAP_WIDTH)
            && y < usize::from(crate::map_data::MAP_HEIGHT))
        .then_some(Self(x as u16, y as u16))
    }
    fn from_index(index: usize) -> Self {
        Self((index % MAP_WIDTH) as u16, (index / MAP_WIDTH) as u16)
    }
    fn distance(self, other: Self) -> u16 {
        self.0.abs_diff(other.0) + self.1.abs_diff(other.1)
    }
    fn tile(self) -> TilePos {
        TilePos::new(self.0, self.1)
    }
}

impl From<TilePos> for Cell {
    fn from(pos: TilePos) -> Self {
        Self(pos.x, pos.y)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Passing {
    partner: ActorKind,
    approach: Cell,
    entrance: Cell,
    expires: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Actor {
    pub kind: ActorKind,
    pub position: Point,
    pub pose: Pose,
    pub activity: Activity,
    pub tool: Option<Tool>,
    pub carried: Option<Item>,
    pub facing_left: bool,
    cell: Cell,
    #[serde(deserialize_with = "read_route")]
    route: Vec<Cell>,
    route_next: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    route_goal: Option<Cell>,
    #[serde(default, skip_serializing_if = "village::is_false")]
    waiting_edge: bool,
    #[serde(default, skip_serializing_if = "village::is_false")]
    external_load: bool,
    #[serde(default, skip_serializing_if = "village::is_zero")]
    waited_edges: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    traced_wood: Option<u8>,
    #[serde(default, skip_serializing_if = "village::is_false")]
    avoid_occupied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    passing: Option<Passing>,
    #[serde(default, skip_serializing_if = "village::is_zero")]
    harvest_processing: u8,
    task: Option<Task>,
    stage: Stage,
    since: u64,
    until: u64,
    stamina: u8,
    hunger: u8,
    thirst: u8,
    amount: u16,
    seeds_in_basket: bool,
    #[serde(default)]
    moving_box: bool,
    outfit: Outfit,
    inside: bool,
    outside: bool,
}

fn read_route<'de, D: serde::Deserializer<'de>>(input: D) -> Result<Vec<Cell>, D::Error> {
    struct RouteVisitor;

    impl<'de> serde::de::Visitor<'de> for RouteVisitor {
        type Value = Vec<Cell>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "a route with at most {MAX_ROUTE} waypoints")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut route = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(MAX_ROUTE));
            while let Some(waypoint) = seq.next_element()? {
                if route.len() == MAX_ROUTE {
                    return Err(serde::de::Error::invalid_length(MAX_ROUTE + 1, &self));
                }
                route.push(waypoint);
            }
            Ok(route)
        }
    }

    input.deserialize_seq(RouteVisitor)
}

impl Actor {
    fn new(kind: ActorKind, cell: Cell) -> Self {
        Self {
            kind,
            position: cell.point(),
            pose: Pose::Idle,
            activity: Activity::Idle,
            tool: None,
            carried: None,
            facing_left: false,
            cell,
            route: Vec::new(),
            route_next: 0,
            route_goal: None,
            waiting_edge: false,
            external_load: false,
            waited_edges: 0,
            traced_wood: None,
            avoid_occupied: false,
            passing: None,
            harvest_processing: 0,
            task: None,
            stage: Stage::Idle,
            since: START_MS,
            until: START_MS,
            stamina: 100,
            hunger: 26,
            thirst: 25,
            amount: 0,
            seeds_in_basket: false,
            moving_box: false,
            outfit: match kind {
                ActorKind::Xiaohe => Outfit::Garden,
                ActorKind::Ayu => Outfit::Work,
                ActorKind::Orange => Outfit::Home,
            },
            inside: false,
            outside: false,
        }
    }

    pub fn stamina(&self) -> u8 {
        self.stamina
    }
    pub fn hunger(&self) -> u8 {
        self.hunger
    }
    pub fn thirst(&self) -> u8 {
        self.thirst
    }
    pub fn carried_amount(&self) -> u16 {
        self.amount
    }

    pub fn has_moving_box(&self) -> bool {
        self.moving_box
    }
    pub fn inside(&self) -> bool {
        self.inside
    }

    pub fn outfit(&self) -> Outfit {
        self.outfit
    }

    /// The two world-space tile centers occupied by the current walking edge.
    pub fn walking_segment(&self) -> Option<(Point, Point)> {
        if self.waiting_edge {
            return None;
        }
        if !matches!(self.stage, Stage::WalkForTool | Stage::WalkToSite) {
            return None;
        }
        let target = self.route.get(self.route_next)?;
        Some((self.cell.point(), target.point()))
    }

    pub fn walking_progress(&self, world_ms: u64) -> Option<f32> {
        self.walking_segment()?;
        Some(movement::edge_fraction(world_ms, self.since, self.until))
    }

    /// The cast has settled and the actor is showing its visible result.
    pub fn fishing_result(&self) -> Option<bool> {
        (self.task == Some(Task::Fish) && self.stage == Stage::Finish)
            .then_some(self.carried == Some(Item::Fish))
    }

    /// Three evenly spaced poses across the result window of a single cast.
    pub fn fishing_result_frame(&self, now_ms: u64) -> Option<(bool, usize)> {
        let success = self.fishing_result()?;
        let elapsed = now_ms.saturating_sub(self.since);
        let duration = self.until.saturating_sub(self.since).max(1);
        let phase = elapsed.min(duration).saturating_mul(3) / duration;
        Some((success, phase.min(2) as usize))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plot {
    pub position: Point,
    pub stage: PlotStage,
    due: u64,
    watered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tree {
    pub position: Point,
    pub stage: TreeStage,
    timber: u16,
    pub protected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Building {
    pub kind: BuildingKind,
    pub position: Point,
    pub level: u8,
    /// Percentage of the current upgrade. Zero means no partial upgrade.
    pub progress: u8,
    step: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Development {
    pub kind: DevelopmentKind,
    pub position: Point,
    pub progress: u8,
    pub phase: u8,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HousingView {
    pub kind: HousePlan,
    pub position: Point,
    pub lot_origin: Point,
    pub material_position: Option<Point>,
    pub footprint: [u8; 2],
    pub phase: HousingPhase,
    pub occupied: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarketLayout {
    pub origin: Point,
    pub footprint: [u8; 2],
    pub counter: Point,
    pub approach: Point,
    pub bins: [Point; 2],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Stock([u16; 8]);

impl Stock {
    pub fn count(&self, item: Item) -> u16 {
        self.0[item as usize]
    }

    fn fits(&self, item: Item, amount: u16) -> bool {
        amount <= STOCK_CAP.saturating_sub(self.count(item))
    }

    fn take(&mut self, item: Item, amount: u16) -> bool {
        let value = &mut self.0[item as usize];
        let Some(rest) = value.checked_sub(amount) else {
            return false;
        };
        *value = rest;
        true
    }

    fn add(&mut self, item: Item, amount: u16) -> bool {
        if !self.fits(item, amount) {
            return false;
        }
        self.0[item as usize] += amount;
        true
    }
}

const PLOTS: [Cell; 6] = [
    Cell(21, 18),
    Cell(23, 18),
    Cell(25, 18),
    Cell(21, 20),
    Cell(23, 20),
    Cell(25, 20),
];
const TREES: [Cell; 16] = [
    Cell(29, 21),
    Cell(32, 22),
    Cell(34, 19),
    Cell(36, 23),
    Cell(38, 20),
    Cell(40, 24),
    Cell(38, 16),
    Cell(41, 18),
    Cell(43, 13),
    Cell(37, 12),
    Cell(42, 22),
    Cell(44, 20),
    Cell(40, 26),
    Cell(42, 27),
    Cell(41, 25),
    Cell(44, 26),
];

fn protected_tree(index: usize) -> bool {
    (8..12).contains(&index)
}
const BUILDINGS: [(BuildingKind, Cell); 5] = [
    (BuildingKind::WestHouse, Cell(12, 14)),
    (BuildingKind::EastHouse, Cell(24, 14)),
    (BuildingKind::Kitchen, Cell(18, 13)),
    (BuildingKind::Storehouse, PANTRY),
    (BuildingKind::Workshop, WORKSHOP_DOOR),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Task {
    Clear(u8),
    Till(u8),
    Sow(u8),
    Water(u8),
    Harvest(u8),
    Store,
    Cook,
    Eat,
    SharedEat,
    GetWater,
    GetHomeWater,
    FillBowl,
    Drink,
    DrinkAtWell,
    FellTree(u8),
    TakeTimber(u8),
    Build(u8),
    Improve(ImprovementOrder),
    BuildHome(HousingOrder),
    Rest,
    Sleep,
    Fish,
    ReturnTool,
    ChangeClothes(Outfit),
    Explore(u8),
    Develop(u8),
    FreightBridge,
    ProcessHarvest(u8),
    CareOrchard(u8),
    Drain(u8),
    Trade(Item, u16),
    Garden,
    External(Cell),
    ReturnHome,
    UnpackHome,
    RackRod,
    ServeCat,
    CatEat,
    CookHomeHarvest,
    EatSavedPortion,
    EatWhileDrying,
    CourtyardBreak,
    Grain(grain::Work),
    Soil(soil::Work),
    WorkshopStory(workshop_story::Work),
    Preserve(preserves::Work),
    Renovate(renovation::Work),
    RoadStory(road_story::Work),
    MarketStory(road_story::MarketWork),
    Memory(memories::Work),
}

impl Task {
    fn activity(self) -> Activity {
        match self {
            Self::Clear(_) => Activity::Clear,
            Self::Till(_) => Activity::Till,
            Self::Sow(_) => Activity::Sow,
            Self::Water(_) => Activity::Water,
            Self::Harvest(_) => Activity::Harvest,
            Self::Store => Activity::Store,
            Self::Cook | Self::CookHomeHarvest => Activity::Cook,
            Self::Eat | Self::SharedEat | Self::EatSavedPortion | Self::EatWhileDrying => Activity::Eat,
            Self::GetWater | Self::GetHomeWater => Activity::GetWater,
            Self::FillBowl => Activity::FillBowl,
            Self::Drink | Self::DrinkAtWell => Activity::Drink,
            Self::FellTree(_) => Activity::FellTree,
            Self::TakeTimber(_) => Activity::CarryWood,
            Self::BuildHome(order) if order.phase == HousingPhase::Clear => Activity::Clear,
            Self::BuildHome(order) if order.phase == HousingPhase::MoveIn => Activity::Store,
            Self::Build(_) | Self::Improve(_) | Self::BuildHome(_) => Activity::Build,
            Self::Rest | Self::CourtyardBreak => Activity::Rest,
            Self::Sleep => Activity::Sleep,
            Self::Fish => Activity::Fish,
            Self::ReturnTool => Activity::ReturnTool,
            Self::ChangeClothes(_) => Activity::ChangeClothes,
            Self::Explore(_) => Activity::Explore,
            Self::Develop(_) | Self::FreightBridge => Activity::Roadwork,
            Self::Trade(..) => Activity::Trade,
            Self::Garden => Activity::TendGarden,
            Self::External(_) => Activity::OutsideActivity,
            Self::ReturnHome => Activity::Rest,
            Self::ProcessHarvest(1) => Activity::WashHarvest,
            Self::ProcessHarvest(2) => Activity::SortHarvest,
            Self::ProcessHarvest(3) => Activity::DryHarvest,
            Self::ProcessHarvest(_) => Activity::PackHarvest,
            Self::CareOrchard(_) => Activity::Orchard,
            Self::Drain(_) => Activity::DrainField,
            Self::UnpackHome | Self::ServeCat => Activity::Store,
            Self::RackRod => Activity::ReturnTool,
            Self::CatEat => Activity::Eat,
            Self::Grain(work) => work.activity(),
            Self::Soil(soil::Work::ClearDrain(_) | soil::Work::TestDrain(_)) => Activity::DrainField,
            Self::Soil(_) => Activity::TendGarden,
            Self::WorkshopStory(workshop_story::Work::OilRod) => Activity::ReturnTool,
            Self::WorkshopStory(workshop_story::Work::ChooseTree | workshop_story::Work::Replant) => Activity::TendGarden,
            Self::WorkshopStory(workshop_story::Work::MillBeam | workshop_story::Work::FitBench) => Activity::Build,
            Self::WorkshopStory(workshop_story::Work::CutOrder | workshop_story::Work::FinishOrder) => Activity::Build,
            Self::WorkshopStory(_) => Activity::Store,
            Self::Preserve(preserves::Work::CookBatch) => Activity::Cook,
            Self::Preserve(preserves::Work::TurnRack) => Activity::DryHarvest,
            Self::Preserve(preserves::Work::RepairRack) => Activity::Build,
            Self::Preserve(_) => Activity::Store,
            Self::Renovate(renovation::Work::Repair(_) | renovation::Work::LayPath | renovation::Work::BuildCorner) => Activity::Build,
            Self::Renovate(renovation::Work::RestAtCorner) => Activity::Rest,
            Self::Renovate(_) => Activity::Store,
            Self::RoadStory(road_story::Work::Build) => Activity::Roadwork,
            Self::RoadStory(_) => Activity::Store,
            Self::MarketStory(road_story::MarketWork::Base | road_story::MarketWork::Pave
                | road_story::MarketWork::Drain | road_story::MarketWork::MoveLamp | road_story::MarketWork::SetMill) => Activity::Roadwork,
            Self::MarketStory(_) => Activity::Store,
            Self::Memory(_) => Activity::Rest,
        }
    }

    fn tool(self) -> Option<Tool> {
        match self {
            Self::Clear(_) | Self::Till(_) | Self::Drain(_) => Some(Tool::Hoe),
            Self::Water(_) | Self::FillBowl | Self::GetWater | Self::GetHomeWater => Some(Tool::WateringCan),
            Self::FellTree(_) => Some(Tool::Axe),
            Self::Build(_) | Self::Improve(_) => Some(Tool::Hammer),
            Self::BuildHome(order) if order.phase == HousingPhase::Clear => Some(Tool::Hoe),
            Self::BuildHome(order) if order.phase != HousingPhase::MoveIn => Some(Tool::Hammer),
            Self::Develop(_) | Self::FreightBridge => Some(Tool::Hammer),
            Self::Garden => Some(Tool::Hoe),
            Self::Trade(..) => Some(Tool::HarvestBasket),
            Self::Harvest(_) => Some(Tool::HarvestBasket),
            Self::Fish | Self::RackRod => Some(Tool::FishingRod),
            Self::WorkshopStory(workshop_story::Work::OilRod) => Some(Tool::FishingRod),
            Self::WorkshopStory(workshop_story::Work::MillBeam | workshop_story::Work::FitBench) => Some(Tool::Hammer),
            Self::WorkshopStory(workshop_story::Work::CutOrder | workshop_story::Work::FinishOrder) => Some(Tool::Hammer),
            Self::WorkshopStory(workshop_story::Work::Replant) => Some(Tool::Hoe),
            Self::Grain(grain::Work::MountMill) => Some(Tool::Hammer),
            Self::Preserve(preserves::Work::RepairRack) => Some(Tool::Hammer),
            Self::Renovate(renovation::Work::Repair(_) | renovation::Work::LayPath | renovation::Work::BuildCorner) => Some(Tool::Hammer),
            Self::RoadStory(road_story::Work::Build) => Some(Tool::Hammer),
            Self::Memory(memories::Work::RenewCorner) => Some(Tool::Hammer),
            Self::MarketStory(road_story::MarketWork::Base | road_story::MarketWork::Pave
                | road_story::MarketWork::Drain | road_story::MarketWork::SetMill) => Some(Tool::Hammer),
            _ => None,
        }
    }

    fn uses_plot(self, id: u8) -> bool {
        matches!(self, Self::Clear(i)|Self::Till(i)|Self::Sow(i)|Self::Water(i)|Self::Harvest(i)|Self::Drain(i)
            | Self::Soil(soil::Work::Inspect(i) | soil::Work::InspectDrain(i) | soil::Work::ClearDrain(i)
                | soil::Work::TestDrain(i) | soil::Work::Apply(i)) if i == id)
    }

    fn duration(self) -> u64 {
        match self {
            Self::Clear(_) => 10 * MINUTE,
            Self::Till(_) => 8 * MINUTE,
            Self::Sow(_) | Self::Water(_) => 4 * MINUTE,
            Self::Harvest(_) => 6 * MINUTE,
            Self::Store | Self::FillBowl => MINUTE,
            Self::Cook | Self::CookHomeHarvest => 22 * MINUTE,
            Self::Eat | Self::SharedEat | Self::EatSavedPortion | Self::EatWhileDrying => 10 * MINUTE,
            Self::GetWater | Self::GetHomeWater | Self::TakeTimber(_) => 3 * MINUTE,
            Self::Drink | Self::DrinkAtWell => 2 * MINUTE,
            Self::FellTree(_) => 22 * MINUTE,
            Self::Build(_) => 24 * MINUTE,
            Self::Improve(order) => u64::from(order.minutes()) * MINUTE,
            Self::BuildHome(order) => order.phase.minutes(order.plan) * MINUTE,
            Self::Rest | Self::CourtyardBreak => 32 * MINUTE,
            Self::Sleep => 3 * HOUR,
            Self::Fish => 25 * MINUTE,
            Self::ReturnTool => MINUTE,
            Self::ChangeClothes(_) => 3 * MINUTE,
            Self::Explore(_) => 8 * MINUTE,
            Self::Develop(_) | Self::FreightBridge => 40 * MINUTE,
            Self::Trade(..) => 9 * MINUTE,
            Self::Garden => 12 * MINUTE,
            Self::External(_) => 0,
            Self::ReturnHome => 0,
            Self::UnpackHome => 5 * MINUTE,
            Self::RackRod | Self::ServeCat => MINUTE,
            Self::CatEat => 4 * MINUTE,
            Self::Grain(work) => work.duration(),
            Self::Soil(_) => 8 * MINUTE,
            Self::WorkshopStory(_) => 25 * MINUTE,
            Self::Preserve(_) => 20 * MINUTE,
            Self::Renovate(_) => 30 * MINUTE,
            Self::RoadStory(_) => 32 * MINUTE,
            Self::MarketStory(_) => 36 * MINUTE,
            Self::Memory(_) => 20 * MINUTE,
            Self::ProcessHarvest(1) => 4 * MINUTE,
            Self::ProcessHarvest(2) => 6 * MINUTE,
            Self::ProcessHarvest(3) => 10 * MINUTE,
            Self::ProcessHarvest(_) => 3 * MINUTE,
            Self::CareOrchard(_) => 12 * MINUTE,
            Self::Drain(_) => 7 * MINUTE,
        }
    }

    fn valid_for(self, village: bool) -> bool {
        match self {
            Self::Clear(i) | Self::Till(i) | Self::Sow(i) | Self::Water(i) | Self::Harvest(i) => {
                usize::from(i)
                    < if village {
                        WILLOW_VILLAGE.crops.len()
                    } else {
                        PLOTS.len()
                    }
            }
            Self::FellTree(i) | Self::TakeTimber(i) => usize::from(i) < TREES.len(),
            Self::Build(i) => usize::from(i) < BUILDINGS.len(),
            Self::Improve(order) => order.phase <= 1,
            Self::BuildHome(order) => order.valid_for(village),
            Self::Explore(i) => usize::from(i) < if village { village::VILLAGE_CAT_SPOTS } else { CAT_SPOTS.len() },
            Self::UnpackHome | Self::RackRod | Self::ServeCat | Self::CatEat | Self::CookHomeHarvest
            | Self::EatSavedPortion | Self::CourtyardBreak | Self::EatWhileDrying | Self::GetHomeWater => village,
            Self::Grain(_) | Self::WorkshopStory(_) | Self::Preserve(_)
            | Self::Soil(soil::Work::Mix | soil::Work::Turn) => village,
            Self::Soil(soil::Work::Inspect(id) | soil::Work::InspectDrain(id) | soil::Work::ClearDrain(id)
                | soil::Work::TestDrain(id) | soil::Work::Apply(id)) => village && id < 30,
            Self::Renovate(work) => village && work.owner() < 2,
            Self::RoadStory(_) | Self::MarketStory(_) => village,
            Self::Memory(memories::Work::Review(i) | memories::Work::ReadTogether(i)) => village && i < 2,
            Self::Memory(memories::Work::Celebrate(i)) => village && i < 4,
            Self::Memory(_) => village,
            Self::Develop(i) => usize::from(i) < CHAPTERS.len(),
            Self::Trade(item, amount) => {
                matches!(item, Item::Wood | Item::Stone | Item::Rope) && (4..=7).contains(&amount)
            }
            Self::External(cell) => {
                if village {
                    cell.tile().index().is_some()
                } else {
                    cell.valid()
                }
            }
            Self::ReturnHome => village,
            Self::FreightBridge => village,
            Self::ProcessHarvest(phase) => village && (1..=4).contains(&phase),
            Self::CareOrchard(tree) => village && tree < 4,
            Self::Drain(plot) => village && usize::from(plot) < WILLOW_VILLAGE.crops.len(),
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Stage {
    Idle,
    WalkForTool,
    Tool,
    WalkToSite,
    Prepare,
    Meet,
    Work,
    Finish,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LivingWorld {
    now_ms: u64,
    target_ms: u64,
    last_wall_ms: u64,
    /// The day advances at half the rate of night. Keep a half-millisecond remainder.
    clock_fraction: u8,
    next_needs: u64,
    next_well: u64,
    next_fish: u64,
    well: u16,
    bowl: u16,
    fish_in_pond: u8,
    casts: u64,
    next_cat_spot: u8,
    meal_escrow: u16,
    meal_until: u64,
    last_shared_meal: Option<u64>,
    actors: [Actor; 3],
    #[serde(deserialize_with = "village::read_plots")]
    plots: Vec<Plot>,
    trees: [Tree; TREES.len()],
    buildings: [Building; BUILDINGS.len()],
    stock: Stock,
    harvested: u64,
    meals: u64,
    drinks: u64,
    #[serde(default)]
    story: StoryState,
    #[serde(default)]
    crop_batches: CropBatches,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    village: Option<village::State>,
    #[serde(skip)]
    village_navigation: village::Navigation,
    #[serde(skip)]
    external_occupied: village::Occupancy,
    #[serde(skip)]
    social_harvest_plot: village::ReservedPlot,
    #[serde(skip)]
    supply_requests: village::SupplyRequests,
    #[serde(skip)]
    footprints: footprints::Tracks,
}

impl LivingWorld {
    pub fn new(wall_secs: u64) -> Self {
        Self::new_wall_ms(wall_secs.saturating_mul(1_000))
    }

    pub fn new_wall_ms(wall_ms: u64) -> Self {
        Self {
            now_ms: START_MS,
            target_ms: START_MS,
            last_wall_ms: wall_ms,
            clock_fraction: 0,
            next_needs: START_MS + 10 * MINUTE,
            next_well: START_MS + 20 * MINUTE,
            next_fish: START_MS + 12 * HOUR,
            well: 9,
            bowl: 1,
            fish_in_pond: 5,
            casts: 0,
            next_cat_spot: 0,
            meal_escrow: 0,
            meal_until: 0,
            last_shared_meal: None,
            actors: [
                Actor::new(ActorKind::Xiaohe, HOUSE_SIDE[0]),
                Actor::new(ActorKind::Ayu, HOUSE_SIDE[1]),
                Actor::new(ActorKind::Orange, CAT_BED),
            ],
            plots: PLOTS
                .iter()
                .map(|cell| Plot {
                    position: cell.point(),
                    stage: PlotStage::Grass,
                    due: 0,
                    watered: false,
                })
                .collect(),
            trees: std::array::from_fn(|i| Tree {
                position: TREES[i].point(),
                stage: TreeStage::Standing,
                timber: 3,
                protected: protected_tree(i),
            }),
            buildings: BUILDINGS.map(|(kind, cell)| Building {
                kind,
                position: cell.point(),
                level: 1,
                progress: 0,
                step: 0,
            }),
            stock: Stock([10, 5, 2, 8, 0, 15, 6, 0]),
            harvested: 0,
            meals: 0,
            drinks: 0,
            story: StoryState::default(),
            crop_batches: CropBatches::default(),
            village: None,
            village_navigation: village::Navigation::default(),
            external_occupied: village::Occupancy::default(),
            social_harvest_plot: village::ReservedPlot::default(),
            supply_requests: village::SupplyRequests::default(),
            footprints: footprints::Tracks::default(),
        }
    }

    pub fn new_village_wall_ms(wall_ms: u64) -> Self {
        let mut world = Self::new_wall_ms(wall_ms);
        world.install_village_layout();
        world.village.as_mut().unwrap().memories = memories::State::new(Some(wall_ms));
        let start = 4 * DAY_MS + 6 * HOUR;
        world.now_ms = start;
        world.target_ms = start;
        world.initialize_town_calendar(wall_ms);
        world.next_needs = start + 10 * MINUTE;
        world.next_well = start + 20 * MINUTE;
        world.next_fish = start + 12 * HOUR;
        world.stock.0[Item::Seeds as usize] = 6;
        world.stock.0[Item::Water as usize] = 12;
        world.stock.0[Item::Crops as usize] = 12;
        world.stock.0[Item::Food as usize] = 12;
        world.record_crops(12);
        world.plots[0].stage = PlotStage::Mature;
        world.plots[1].stage = PlotStage::Mature;
        world.plots[2].stage = PlotStage::Growing;
        world.plots[3].stage = PlotStage::Growing;
        for (i, cell) in [
            world.home_side(0),
            world.home_side(1),
            village::poi("cat_rest"),
        ]
        .into_iter()
        .enumerate()
        {
            world.actors[i].cell = cell;
            world.actors[i].position = cell.point();
            world.actors[i].since = start;
            world.actors[i].until = start;
        }
        world
    }

    fn install_village_layout(&mut self) {
        self.village = Some(village::State::new());
        for (i, building) in self.buildings.iter_mut().enumerate() {
            building.position = village::building(i).point();
        }
        for (i, crop) in WILLOW_VILLAGE.crops.iter().enumerate() {
            let position = Cell(crop.rect.x, crop.rect.y).point();
            if let Some(plot) = self.plots.get_mut(i) {
                plot.position = position;
            } else {
                self.plots.push(Plot {
                    position,
                    stage: PlotStage::Grass,
                    due: 0,
                    watered: false,
                });
            }
        }
        for (tree, cell) in self.trees.iter_mut().zip(village::tree_cells()) {
            tree.position = cell.point();
        }
    }

    pub fn is_village(&self) -> bool {
        self.village.is_some()
    }

    pub fn map_extent(&self) -> (u16, u16) {
        if self.is_village() {
            (WILLOW_VILLAGE.width, WILLOW_VILLAGE.height)
        } else {
            (MAP_WIDTH as u16, MAP_HEIGHT as u16)
        }
    }

    pub fn phase_mask(&self) -> PhaseMask {
        self.village
            .as_ref()
            .map_or(PhaseMask::DAY_ONE, village::State::phases)
    }

    pub fn navigation_view(&self) -> Option<MapView<'static>> {
        self.is_village().then(|| self.village_map_view())
    }

    pub fn activate_weekly_market(&mut self) -> bool {
        let Some(mut state) = self.village.clone() else {
            return false;
        };
        state.grant("weekly_market");
        state.apply_development(self.story.finished, false);
        if !self.can_commit_phases(state.phases()) {
            return false;
        }
        self.village = Some(state);
        true
    }

    pub fn last_wall_ms(&self) -> u64 {
        self.last_wall_ms
    }

    pub fn resolve_poi(&self, id: &str) -> Option<Point> {
        let poi = WILLOW_VILLAGE.poi(id)?;
        (self.is_village()
            && self.phase_mask().contains(poi.phase)
            && self.can_walk(poi.tile.x.into(), poi.tile.y.into()))
        .then(|| Cell::from(poi.tile).point())
    }

    pub fn village_building_active(&self, id: &str) -> bool {
        self.is_village()
            && WILLOW_VILLAGE
                .building(id)
                .is_some_and(|building| self.phase_mask().contains(building.phase))
    }

    pub fn actor_tile(&self, kind: ActorKind) -> TilePos {
        self.actors[kind.index()].cell.tile()
    }

    pub fn actor_next_tile(&self, kind: ActorKind) -> Option<TilePos> {
        let actor = &self.actors[kind.index()];
        (!actor.waiting_edge)
            .then(|| actor.route.get(actor.route_next).copied().map(Cell::tile))
            .flatten()
    }

    pub fn actor_destination(&self, kind: ActorKind) -> Option<TilePos> {
        let actor = &self.actors[kind.index()];
        actor
            .route_goal
            .or_else(|| actor.route.last().copied())
            .map(Cell::tile)
    }

    pub fn set_external_occupied(&mut self, tiles: &[TilePos]) {
        self.external_occupied.set(tiles);
    }

    /// Rebuilt by the resident scheduler from its saved appointment before simulation.
    pub fn set_social_harvest_plot(&mut self, plot: Option<u8>) {
        self.social_harvest_plot.0 = plot.filter(|&plot| self.social_eligible_harvest(plot));
    }

    /// The live market can request ordinary farm work while gathering a batch.
    /// This advisory target owns no stock and is rebuilt by the market on load.
    pub fn set_market_crop_reserve(&mut self, target: Option<u16>) {
        self.supply_requests.market_crops = target.filter(|_| self.is_village()).map(|amount| amount.min(12));
    }

    /// A new neighbour's trial can ask the household to prepare extra meals.
    /// The ordinary kitchen still has to obtain, cook and carry real inputs.
    pub fn set_neighbour_food_reserve(&mut self, requested: bool) {
        self.supply_requests.neighbour_meals = requested && self.is_village();
    }

    pub fn social_eligible_harvest(&self, plot: u8) -> bool {
        let index = usize::from(plot);
        WILLOW_VILLAGE.crops.get(index).is_some_and(|crop| self.phase_mask().contains(crop.phase))
            && self.village.as_ref().is_some_and(|state| state.flooded_plots & (1 << index) == 0)
            && plot != grain::FIELD && self.plots.get(index).is_some_and(|plot| plot.stage == PlotStage::Mature)
            && self.actors.iter().all(|actor| !actor.task.is_some_and(|task| task.uses_plot(plot)))
    }

    pub fn note_village_trade(&mut self) {
        if self.is_village() {
            self.story.trades = self.story.trades.saturating_add(1);
        }
    }

    /// Called at home unload after an accepted household sale, once per trip.
    pub fn note_village_surplus_trade(&mut self, sold: Item, qty: u16) {
        if let Some(state) = &mut self.village {
            state.pantry_plan.sale(sold, qty, &self.stock);
        }
    }

    /// Apply one completed service. The caller supplies the physical input
    /// from the worker's custody and debits it only on success.
    pub fn apply_community(&mut self, action: CommunityAction, position: TilePos) -> bool {
        if !self.is_village() {return false;}
        let nearby=|a:TilePos,b:TilePos|u32::from(a.x.abs_diff(b.x))+u32::from(a.y.abs_diff(b.y))<=2;
        match action {
            CommunityAction::ShareFood(kind) | CommunityAction::ShareWater(kind) => {
                let actor=&mut self.actors[kind.index()];
                if actor.inside || !nearby(position,actor.cell.tile()) || !actor.outside
                    || actor.stage!=Stage::Idle || kind==ActorKind::Orange {return false;}
                let need=if matches!(action,CommunityAction::ShareFood(_)) {&mut actor.hunger} else {&mut actor.thirst};
                if *need<8 {return false;}
                *need=need.saturating_sub(22);
                true
            }
            CommunityAction::CleanCatBowl | CommunityAction::FillCatBowl => {
                let at=village::poi("cat_water").tile();
                if !nearby(position,at)
                    || self.actors[ActorKind::Orange.index()].cell.tile()==at {return false;}
                if action == CommunityAction::CleanCatBowl {
                    std::mem::take(&mut self.village.as_mut().expect("village bowl").cat_bowl_dirty)
                } else if self.bowl < BOWL_CAP {
                    self.bowl += 1;
                    true
                } else { false }
            }
            CommunityAction::FishRelease => {
                if !nearby(position,village::poi("home_fishing").tile()) || self.fish_in_pond>=8 {
                    return false;
                }
                self.fish_in_pond+=1;
                true
            }
            CommunityAction::Mend(fixture) => {
                let Some((_,anchor))=self.building_fixtures().find(|(status,_)| status.fixture==fixture) else {return false;};
                if !Cell::at(anchor).is_some_and(|anchor|nearby(position,anchor.tile())) {return false;}
                self.story.improvements.restore_use(fixture)
            }
        }
    }

    pub fn last_shared_meal_day(&self) -> Option<u64> {self.last_shared_meal}
    pub fn fishing_casts(&self) -> u64 {self.casts}

    pub fn cat_bowl_needs_cleaning(&self) -> bool {
        self.village.as_ref().is_some_and(|state| state.cat_bowl_dirty)
    }

    pub fn village_plot_needs_water(&self, plot: u8) -> bool {
        let i = usize::from(plot);
        plot != grain::FIELD
            && self.village.as_ref().is_some_and(|state| {
                WILLOW_VILLAGE.crops.get(i).is_some_and(|p| state.phases().contains(p.phase))
                    && state.flooded_plots & (1 << i) == 0
            })
            && self.plots.get(i).is_some_and(|p| matches!(p.stage, PlotStage::Seed | PlotStage::Growing) && !p.watered)
    }

    /// Transfers one bucket to an arrived visitor's custody.
    pub fn village_draw_visitor_water(&mut self, at: TilePos) -> bool {
        if !self.is_village() || self.well <= 2 || at != village::poi("home_well").tile() {
            return false;
        }
        self.well -= 1;
        true
    }

    /// Apply an orchard or field worker's completed task. Their scheduler must
    /// verify arrival and its own reservation before committing this operation.
    pub fn village_local_work(&mut self, operation: &str, plot: Option<u8>) -> bool {
        let Some(index) = plot.map(usize::from) else {
            return false;
        };
        let Some(definition) = WILLOW_VILLAGE.crops.get(index) else {
            return false;
        };
        if !self.is_village()
            || !self.phase_mask().contains(definition.phase)
            || index == usize::from(grain::FIELD) && operation != "inspect"
            || self
                .actors
                .iter()
                .any(|actor| actor.task.is_some_and(|task| task.uses_plot(index as u8)))
        {
            return false;
        }
        let phase = self.plots[index].stage;
        let bit = 1 << index;
        if operation == "drain" {
            let state = self.village.as_mut().expect("village plots");
            let present = state.flooded_plots & bit != 0;
            state.flooded_plots &= !bit;
            return present;
        }
        if operation != "inspect" && self.village.as_ref().unwrap().flooded_plots & bit != 0 {
            return false;
        }
        match (operation, phase) {
            ("inspect", _) => true,
            ("weed", PlotStage::Grass) => {
                self.plots[index].stage = PlotStage::Cleared;
                true
            }
            ("till", PlotStage::Cleared) => {
                self.plots[index].stage = PlotStage::Tilled;
                true
            }
            ("sow", PlotStage::Tilled) if self.take_stock(Item::Seeds, 1) => {
                self.plots[index].stage = PlotStage::Seed;
                true
            }
            ("water", PlotStage::Seed | PlotStage::Growing)
                if !self.plots[index].watered && self.take_stock(Item::Water, 1) =>
            {
                let plot = &mut self.plots[index];
                plot.watered = true;
                plot.due = self.now_ms
                    + if phase == PlotStage::Seed {
                        55 * MINUTE
                    } else {
                        105 * MINUTE
                    };
                true
            }
            ("harvest", PlotStage::Mature)
                if self.social_harvest_plot.0 != Some(index as u8) && self.stock.fits(Item::Crops, 3)
                    && self.stock.fits(Item::Seeds, 1) =>
            {
                self.plots[index].stage = PlotStage::Tilled;
                self.village.as_mut().unwrap().opening.sown &= !bit;
                self.stock.add(Item::Crops, 3);
                self.stock.add(Item::Seeds, 1);
                self.record_crops(3);
                self.harvested = self.harvested.saturating_add(3);
                true
            }
            _ => false,
        }
    }

    /// The resident scheduler owns the route, reserved plot stance and
    /// transfer of the returned batch. No household stock is credited here.
    pub fn harvest_for_resident(&mut self, plot: u8) -> Option<[(Item, u16); 2]> {
        let index = usize::from(plot);
        if !self.social_eligible_harvest(plot) {
            return None;
        }
        let state = self.village.as_mut()?;
        let bit = 1 << index;
        state.opening.sown &= !bit;
        self.plots[index].stage = PlotStage::Tilled;
        self.harvested = self.harvested.saturating_add(3);
        Some([(Item::Crops, 3), (Item::Seeds, 1)])
    }

    /// Migrate a validated legacy authority without advancing either clock.
    /// The caller publishes the candidate only after all other save domains migrate.
    pub fn migrate_to_village(&self) -> Result<Self, String> {
        if !self.validate() {
            return Err("旧世界状态校验未通过".into());
        }
        if self.is_village() {
            return Ok(self.clone());
        }

        let mut candidate = self.clone();
        candidate.install_village_layout();
        candidate
            .village
            .as_mut()
            .expect("new layout")
            .apply_development(self.story.finished, true);
        let origins = candidate.story.housing.migrate_origins()?;
        if (0..2).all(|i| {
            candidate
                .story
                .housing
                .order_for_person(i)
                .is_some_and(|order| order.plan == HousePlan::Shared)
        }) {
            candidate
                .village
                .as_mut()
                .expect("new layout")
                .grant("late_shared_home");
        }
        for (i, tree) in candidate.trees.iter().enumerate() {
            if tree.stage == TreeStage::Stump {
                candidate.village.as_mut().unwrap().tree_due[i] = self.now_ms + 21 * DAY_MS;
            }
        }

        let mut claimed = Vec::with_capacity(candidate.actors.len() * 2);
        for i in 0..candidate.actors.len() {
            let previous = &self.actors[i];
            let mut task = previous.task;
            if let Some(Task::BuildHome(order)) = task {
                let mapped = origins[usize::from(order.lot)];
                let origin = if mapped == [0, 0] {
                    village::COMPATIBLE_HOME_ORIGINS[if order.plan == HousePlan::Shared {
                        0
                    } else {
                        2
                    }]
                } else {
                    mapped
                };
                task = Some(Task::BuildHome(order.with_village_origin(origin)));
            }
            if let Some(Task::External(cell)) = task {
                task = Some(Task::External(candidate.map_legacy_cell(cell).ok_or_else(
                    || format!("{}的活动目的地没有兼容落点", previous.kind.label()),
                )?));
            }
            let walking = matches!(previous.stage, Stage::WalkForTool | Stage::WalkToSite);
            let abandoned_trade =
                matches!(task, Some(Task::Trade(..))) && previous.stage != Stage::Finish;
            if abandoned_trade {
                task = Some(Task::Store);
            }
            candidate.actors[i].task = task;
            let new_goal = task.map(|task| match previous.stage {
                Stage::WalkForTool | Stage::Tool if !abandoned_trade => {
                    candidate.tool_pickup(i, task)
                }
                _ => candidate.site(i, task),
            });
            let position = if !walking && !abandoned_trade && previous.stage != Stage::Idle {
                new_goal.filter(|&cell| candidate.valid_world_cell(cell))
            } else {
                candidate.map_legacy_cell(previous.cell)
            }
            .ok_or_else(|| format!("{}没有可用的兼容脚点", previous.kind.label()))?;

            let old_edge = previous.route.get(previous.route_next).copied();
            let new_edge = if let Some(old_next) = old_edge {
                let dx = i32::from(old_next.0) - i32::from(previous.cell.0);
                let dy = i32::from(old_next.1) - i32::from(previous.cell.1);
                let translated = position.tile().offset(dx, dy).map(Cell::from);
                let from_alias = candidate.map_legacy_cell(old_next);
                translated
                    .into_iter()
                    .chain(from_alias)
                    .find(|next| {
                        position.distance(*next) == 1
                            && !claimed.contains(next)
                            && candidate.valid_world_cell(*next)
                    })
                    .ok_or_else(|| format!("{}的途中步无法安全迁移", previous.kind.label()))?
                    .into()
            } else {
                None
            };

            let entering_inside = task.is_some_and(|task| {
                previous.stage == Stage::Work && candidate.work_is_indoors(i, task)
            });
            let position = if claimed.contains(&position) && !entering_inside {
                if previous.stage != Stage::Idle || new_edge.is_some() {
                    return Err(format!(
                        "{}的兼容脚位与已保留位置冲突",
                        previous.kind.label()
                    ));
                }
                let waiting = if i < 2 {
                    candidate.home_side(i)
                } else {
                    village::poi("cat_rest")
                };
                if claimed.contains(&waiting) || !candidate.valid_world_cell(waiting) {
                    return Err(format!(
                        "{}原地暂时没有可用的兼容候位",
                        previous.kind.label()
                    ));
                }
                waiting
            } else {
                position
            };
            if !entering_inside {
                claimed.push(position);
                if let Some(edge) = new_edge {
                    claimed.push(edge);
                }
            }
            let actor = &mut candidate.actors[i];
            actor.cell = position;
            actor.route = new_edge.into_iter().collect();
            actor.route_next = 0;
            actor.route_goal = walking.then_some(new_goal).flatten();
            actor.waiting_edge = false;
            actor.waited_edges = 0;
            actor.inside = entering_inside;
            if abandoned_trade {
                actor.stage = Stage::WalkToSite;
                actor.activity = Activity::Store;
                actor.route_goal = new_goal;
                if actor.route.is_empty() {
                    actor.until = candidate.now_ms;
                    actor.since = candidate.now_ms;
                }
            }
            update_position(actor, candidate.now_ms);
        }
        if !candidate.validate() {
            return Err("兼容布局无法保留当前任务和已预约状态".into());
        }
        Ok(candidate)
    }

    pub fn legacy_camera_anchor(&self, old: Point) -> Option<Point> {
        if !self.is_village() {
            return None;
        }
        let cell = Cell::at(old)?;
        if !cell.valid() {
            return None;
        }
        let source = cell.point();
        let target = self.map_legacy_cell(cell)?.point();
        Some(Point {
            x: target.x + old.x - source.x,
            y: target.y + old.y - source.y,
        })
    }

    fn map_legacy_cell(&self, source: Cell) -> Option<Cell> {
        let mut aliases = [
            (HOUSE_DOORS[0], self.initial_home_door(0)),
            (HOUSE_DOORS[1], self.initial_home_door(1)),
            (HOUSE_SIDE[0], self.home_side(0)),
            (HOUSE_SIDE[1], self.home_side(1)),
            (PANTRY, self.pantry()),
            (STOVE, self.building_door(2)),
            (WORKSHOP_DOOR, self.building_door(4)),
            (LEGACY_WORKSHOP_DOOR, self.building_door(4)),
            (WELL, village::poi("home_well")),
            (POND_BANK, village::poi("home_fishing")),
            (BOWL, village::poi("cat_water")),
            (CAT_BED, village::poi("cat_rest")),
            (ROAD_TRADE, village::poi("home_trade")),
            (MARKET_TRADE, village::poi("market_gate")),
            (Cell(12, 28), village::poi("bridge_wait_w")),
            (Cell(35, 35), village::poi("market_well")),
            (Cell(40, 35), village::poi("commons_garden")),
            (Cell(45, 14), village::poi("outside_gate")),
        ];
        aliases.sort_by_key(|(old, _)| old.distance(source));
        let (old, new) = aliases[0];
        let dx = i32::from(source.0) - i32::from(old.0);
        let dy = i32::from(source.1) - i32::from(old.1);
        let translated = new.tile().offset(dx, dy).map(Cell::from);
        // Preserve a relative position near a recognizable legacy destination.
        // For cells outside that anchor's approach, retain the same legal tile.
        let fallback = (old.distance(source) <= 2).then_some(new);
        translated
            .into_iter()
            .chain([source])
            .chain(fallback)
            .find(|&cell| self.valid_world_cell(cell))
    }

    pub fn world_ms(&self) -> u64 {
        self.now_ms
    }
    pub fn minute_of_day(&self) -> u16 {
        ((self.now_ms % DAY_MS) / MINUTE) as u16
    }

    pub fn weather(&self) -> YardWeather {
        Self::weather_at(self.now_ms)
    }

    fn wet_spell(spell: u64) -> bool {
        (spell ^ (spell >> 5)).wrapping_mul(0x9E37_79B9) % 20 < 3
    }

    fn weather_at(world_ms: u64) -> YardWeather {
        let spell = world_ms / (3 * HOUR);
        let sample = (spell ^ (spell >> 5)).wrapping_mul(0x9E37_79B9) % 20;
        match sample {
            0..=2 if world_ms / DAY_MS / 28 % 4 == 3 => YardWeather::Snow,
            0..=2 => YardWeather::Rain,
            3..=7 => YardWeather::Clouds,
            _ => YardWeather::Clear,
        }
    }

    pub fn village_footprints(&self) -> impl Iterator<Item = Footprint> + '_ {
        self.footprints.visible(self.now_ms, self.weather())
    }

    fn record_footstep(&mut self, from: Cell, to: Cell, kind: ActorKind) {
        if !self.is_village() || from.distance(to) != 1 {
            return;
        }
        let ground = self.ground(from.0.into(), from.1.into());
        let spell = self.now_ms / (3 * HOUR);
        let snow = self.weather() == YardWeather::Snow
            || self.season() == YardSeason::Winter && spell > 0 && Self::wet_spell(spell - 1);
        let after_rain = spell > 0 && Self::wet_spell(spell - 1)
            && self.now_ms % (3 * HOUR) < HOUR;
        if matches!(ground, Ground::Soil | Ground::Grass) && (snow || after_rain) {
            self.footprints.add(from.tile(), to.tile(), kind, snow, self.now_ms);
        }
    }

    pub fn season(&self) -> YardSeason {
        [
            YardSeason::Spring,
            YardSeason::Summer,
            YardSeason::Autumn,
            YardSeason::Winter,
        ][((self.now_ms / DAY_MS / 28) % 4) as usize]
    }
    pub fn actors(&self) -> &[Actor; 3] {
        &self.actors
    }
    pub fn plots(&self) -> &[Plot] {
        &self.plots
    }
    pub fn trees(&self) -> &[Tree] {
        &self.trees
    }
    pub fn orchard_phase(&self, tree_index: usize) -> Option<OrchardPhase> {
        let index = tree_index.checked_sub(8)?;
        self.village
            .as_ref()?
            .orchard
            .get(index)
            .map(|tree| tree.phase)
    }

    pub fn seven_day_home_time(&self, actor: ActorKind) -> Option<(u64, u64)> {
        let i = actor.index();
        let state = self.village.as_ref()?;
        if i >= 2 {
            return None;
        }
        Some((
            state.home_ms.iter().map(|day| u64::from(day[i])).sum(),
            state.awake_ms.iter().map(|day| u64::from(day[i])).sum(),
        ))
    }
    pub fn buildings(&self) -> &[Building] {
        &self.buildings
    }
    pub fn stock(&self) -> &Stock {
        &self.stock
    }

    pub fn stock_can_accept(&self, item: Item, amount: u16) -> bool {
        let escrow = if item == Item::Food { self.meal_escrow } else { 0 };
        self.stock.fits(item, amount.saturating_add(escrow))
    }

    pub fn next_construction_material_requirement(&self) -> Option<(Item, u16)> {
        let next_development = CHAPTERS.get(self.story.completed()).filter(|chapter| {
            self.story.running.is_none() && self.town_permits_project(self.story.finished)
                && self.now_ms / DAY_MS + 1 >= u64::from(chapter.earliest_day)
        });
        next_development.map(|chapter| (chapter_item(chapter.material), chapter.amount))
            .or_else(|| self.village_project_material_requirement())
    }

    fn village_project_material_requirement(&self) -> Option<(Item, u16)> {
        let state = self.village.as_ref()?;
        let eligible = (0..2).flat_map(|i| {
            [
                (state.opening.completed("Q08") && state.workshop_story.completed("Q20")
                    && self.ongoing_town_work(TownEra::Workshop, state.renovation.progress_started(), 4))
                    .then(|| state.renovation.task(i, self.now_ms).or_else(|| self.common_renovation_task(i)))
                    .flatten().map(Task::Renovate),
                (state.renovation.completed("Q35") && self.road_town_permit())
                    .then(|| state.road_story.task(i, self.now_ms))
                    .flatten().filter(|&work| work == road_story::Work::Build).map(Task::RoadStory),
                self.current_market_work(i).map(Task::MarketStory),
            ]
        });
        eligible.flatten().filter_map(|task| self.input(task)).find(|(item, _)| {
            matches!(item, Item::Stone | Item::Wood | Item::Rope)
        })
    }

    /// Outstanding materials for the next actual development order. The market
    /// uses this to quote a real delivery from the supplier's own inventory.
    pub fn next_construction_material_shortfall(&self) -> Option<(Item, u16)> {
        [self.next_construction_material_requirement(), self.village_project_material_requirement()]
            .into_iter().flatten().find_map(|(item, requirement)| {
                let missing = requirement.saturating_sub(self.stock.count(item));
                (missing > 0).then_some((item, missing))
            })
    }

    fn mill_can_use_stone(&self) -> bool {
        let project_reserve = CHAPTERS.get(self.story.completed()).map_or(0, |next| {
            if next.material == Material::Stone && self.story.running.is_none() { next.amount } else { 0 }
        });
        self.stock.count(Item::Stone) > project_reserve
    }

    pub fn story(&self) -> &StoryState {
        &self.story
    }

    pub fn opening_chapter_completed(&self, chapter: &str) -> bool {
        self.village.as_ref().is_some_and(|state| state.opening.completed(chapter))
    }

    pub fn opening_chapter_count(&self) -> usize {
        ["Q01", "Q02", "Q03", "Q04", "Q05", "Q06", "Q07", "Q08"]
            .into_iter()
            .filter(|chapter| self.opening_chapter_completed(chapter))
            .count()
    }

    pub fn village_chapter_completed(&self, chapter: &str) -> bool {
        self.village.as_ref().is_some_and(|state| {
            if chapter == "Q23" {
                return state.pantry_plan.completed(chapter) && state.pantry_plan.completed("Q15")
                    && state.soil.completed("Q18") && state.workshop_story.completed("Q20")
                    && state.soil.completed("Q21") && state.grain.completed("Q22")
                    && self.now_ms / DAY_MS >= 56;
            }
            state.opening.completed(chapter) || state.grain.completed(chapter) || state.soil.completed(chapter)
                || state.workshop_story.completed(chapter) || state.pantry_plan.completed(chapter)
                || state.preserves.completed(chapter) || state.renovation.completed(chapter) || state.road_story.completed(chapter)
                || state.market_story.completed(chapter)
                || state.memories.completed(chapter)
        })
    }

    pub fn plot_stage(&self, index: usize) -> Option<PlotStage> {
        let stage = self.plots.get(index)?.stage;
        Some(if index == usize::from(grain::FIELD) && let Some(state) = &self.village {
            state.grain.crop_stage().unwrap_or(stage)
        } else {
            stage
        })
    }

    pub fn record_village_social(&mut self, progress: crate::residents::SocialProgress) {
        if let Some(state) = &mut self.village { state.social = progress; }
    }

    pub fn record_companionship_clock(&mut self, now:u64, active_ms:u64) {
        if let Some(state) = &mut self.village { state.memories.observe_clock(now,active_ms); }
    }

    pub fn note_first_village_observation(&mut self, now: u64) {
        if let Some(state) = &mut self.village {
            state.memories.note_first_v4(now);
        }
    }

    /// Create the calendar ledger after decoding a valid pre-calendar world.
    pub fn initialize_town_calendar(&mut self, now: u64) {
        let housing_in_progress = self.story.housing.active_order().is_some();
        if let Some(state) = &mut self.village && state.calendar.is_none() {
            // Already running tasks and projects continue by their saved receipts.
            // Open phases and completed stories remain persisted in their own domains.
            let inherited_bits = u16::from(state.grain.mill_installed())
                | (u16::from(state.preserves.journey_started()) << 1)
                | (u16::from(state.renovation.progress_started()) << 2)
                | (u16::from(state.road_story.stage > 0) << 3)
                | (u16::from(housing_in_progress) << 5);
            let market_finish = match state.market_story.stage {
                2..=3 => 4,
                4..=5 => 6,
                7 => 8,
                9..=10 => 11,
                11 if state.market_story.clearings > 0 => 12,
                _ => 0,
            };
            let origin = state.memories.calendar_origin().unwrap_or(now);
            state.calendar = Some(town_calendar::TownCalendar::new(origin, self.last_wall_ms, self.now_ms, inherited_bits, market_finish));
        }
    }

    pub fn town_real_days(&self) -> u64 {
        self.village.as_ref().and_then(|state| state.calendar.as_ref()).map_or(0, |calendar| calendar.days())
    }

    pub fn town_allows(&self, era: TownEra) -> bool {
        self.village.as_ref().and_then(|state| state.calendar.as_ref())
            .map_or(era == TownEra::Humble, |calendar| calendar.allows(era, self.now_ms))
    }

    pub fn town_chapter_allows(&self, id: &str) -> bool {
        let Ok(number) = id.strip_prefix('Q').unwrap_or_default().parse::<u8>() else { return false; };
        if !(1..=72).contains(&number) {
            return false;
        }
        if self.village_chapter_completed(id) || self.town_allows(TownEra::for_chapter(number)) {
            return true;
        }
        let Some(state) = &self.village else { return false; };
        let Some(clock) = &state.calendar else { return false; };
        match number {
            25 | 28 => self.ongoing_town_work(TownEra::Workshop, state.grain.mill_installed(), 1),
            26 | 27 => self.ongoing_town_work(TownEra::Workshop, state.preserves.journey_started(), 2),
            29 => state.workshop_story.order.is_some(),
            30..=35 => self.ongoing_town_work(TownEra::Workshop, state.renovation.progress_started(), 4),
            42..=46 => self.road_town_permit(),
            54..=59 => clock.permits_public_work(state.market_story.stage, self.now_ms),
            _ => false,
        }
    }

    pub fn town_next_construction_day(&self) -> Option<u64> {
        self.is_village().then_some(())?;
        if self.story.running.is_some() || self.town_permits_project(self.story.finished) {
            return None;
        }
        town_calendar::PROJECT_DAYS.get(self.story.completed()).copied()
    }

    /// Age opens work opportunities. The displayed stage also requires the
    /// corresponding completed service or an inherited open district.
    pub fn town_era(&self) -> TownEra {
        let Some(state) = &self.village else { return TownEra::Humble; };
        let phases = state.phases();
        let has = |id| phases.contains(WILLOW_VILLAGE.phase(id).expect("compiled town phase"));
        if state.calendar.as_ref().is_some_and(|c| c.renewed) {
            TownEra::Heritage
        } else if has("late") && state.social.matured_village {
            TownEra::Mature
        } else if has("community") && has("cooperative_farm") {
            TownEra::PublicDistrict
        } else if has("resident_arrival") {
            TownEra::SecondBlock
        } else if has("permanent_market") && has("village_development") {
            TownEra::MarketStreet
        } else if self.story.finished >= 9 && state.social.market_cleanup {
            TownEra::Market
        } else if has("farm_expansion") {
            TownEra::Workshop
        } else {
            TownEra::Humble
        }
    }

    pub fn town_corner_renewed(&self) -> bool {
        self.village.as_ref().and_then(|state| state.calendar.as_ref())
            .is_some_and(|calendar| calendar.renewed)
    }

    fn ongoing_town_work(&self, era: TownEra, started: bool, inherited: u16) -> bool {
        self.town_allows(era) || started && self.village.as_ref().and_then(|state| state.calendar.as_ref())
            .is_some_and(|calendar| calendar.inherited_bits & inherited != 0)
    }

    fn town_permits_project(&self, id: u8) -> bool {
        !self.is_village() || self.story.running.is_some_and(|project| project.id == id)
            || self.village.as_ref().and_then(|state| state.calendar.as_ref())
                .is_some_and(|calendar| calendar.permits_project(id, self.now_ms))
    }

    /// Tests can drive development age apart from the accelerated world–clock
    /// replay. This path cannot be used by a release build or grant rewards.
    #[cfg(test)]
    pub(crate) fn set_test_town_age(&mut self, age_days: u64) {
        let state = self.village.as_mut().expect("village test clock");
        let origin = state.memories.calendar_origin().unwrap_or_default();
        self.last_wall_ms = origin.saturating_add(age_days * DAY_MS);
        state.calendar = Some(town_calendar::TownCalendar::new(origin, self.last_wall_ms, self.now_ms, 0, 0));
    }

    #[cfg(test)]
    pub(crate) fn test_camera_actor(&mut self, kind: ActorKind, point: Point, pose: Pose, inside: bool) {
        let actor = &mut self.actors[kind.index()];
        actor.cell = Cell::at(point).expect("camera fixture point on map");
        actor.position = point;
        actor.pose = pose;
        actor.inside = inside;
    }

    pub fn market_lamps_stage(&self) -> u8 {
        self.village.as_ref().map_or(0,|state| state.market_story.lamps)
    }

    pub fn market_lamp_tiles(&self) -> &'static [TilePos] {
        &road_story::MARKET_LAMPS[..usize::from(self.market_lamps_stage()).min(6)]
    }

    pub fn market_construction_stage(&self) -> u8 {
        self.village.as_ref().map_or(0,|state| state.market_story.stage)
    }

    pub fn market_construction_paving(&self) -> (&'static [TilePos], u8) {
        if !self.is_village() {
            return (&[], 0);
        }
        (&road_story::MARKET_LANE, self.market_construction_stage().min(3))
    }

    #[cfg(test)]
    pub(crate) fn test_market_visual_stage(&mut self, stage: u8) {
        let state = self.village.as_mut().expect("village render fixture");
        for phase in ["main_bridge_freight", "weekly_market", "permanent_market", "village_development",
            "resident_arrival", "community"] {
            state.grant(phase);
        }
        state.freight_inspected = true;
        state.road_story.stage = 12;
        state.market_story.stage = stage.min(11);
        state.market_story.lamps = match stage { 0..=6 => 0, 7 => 3, _ => 6 };
        if stage >= 8 {
            state.grain.prepare_mill_render_fixture();
        }
    }

    #[cfg(test)]
    pub(crate) fn test_house_visual_stage(&mut self, plan: HousePlan, stage: HousingPhase) {
        self.story.housing.stage_render_fixture(plan, stage, self.now_ms);
        if plan == HousePlan::Shared && stage == HousingPhase::MoveIn {
            self.village.as_mut().expect("village render fixture").grant("late_shared_home");
        }
    }

    #[cfg(test)]
    pub(crate) fn test_forest_visual_worker(&mut self) -> Point {
        let (index, _) = self.trees.iter().enumerate().rev()
            .find(|(_, tree)| !tree.protected && tree.stage == TreeStage::Standing)
            .expect("standing forestry tree");
        let i = ActorKind::Ayu.index();
        let task = Task::FellTree(index as u8);
        let foot = self.site(i, task);
        let actor = &mut self.actors[i];
        actor.cell = foot;
        actor.position = foot.point();
        actor.task = Some(task);
        actor.activity = task.activity();
        actor.tool = Some(Tool::Axe);
        self.stage(i, Stage::Work, Pose::Work, task.duration());
        foot.point()
    }

    pub fn mill_display_site(&self) -> Option<&'static str> {
        let village = self.village.as_ref()?;
        if !village.grain.mill_installed() {
            return None;
        }
        match village.market_story.stage {
            ..=8 if !self.actors.iter().any(|actor| {
                actor.moving_box && actor.task == Some(Task::MarketStory(road_story::MarketWork::CarryMill))
            }) => Some("home_workshop"),
            10.. => Some("craft_carpentry"),
            _ => None,
        }
    }

    pub fn memory_celebrations(&self) -> u8 {
        self.village.as_ref().map_or(0,|state|state.memories.celebrated)
    }

    /// The two shop rows keep pedestrian paths open. Once the tested drain
    /// is installed this certifies covered low-water market operation only.
    pub fn village_market_drained(&self) -> bool {
        self.village.as_ref().is_some_and(|state|state.market_story.completed("Q56"))
    }

    pub fn road_construction_stage(&self) -> u8 {
        self.village.as_ref().map_or(0,|state| state.road_story.stage)
    }

    /// The authority that owns the receiving reservation calls this only
    /// after the intended recipient has reached the post handoff point.
    pub fn take_public_order(&mut self) -> Option<(Item,u16)> {
        let state = &mut self.village.as_mut()?.road_story;
        if state.stage != 11 { return None; }
        let payload = state.loaded.take()?;
        state.commit(self.now_ms);
        Some(payload)
    }

    pub fn public_order_ready(&self) -> bool {
        self.village.as_ref().is_some_and(|state| state.road_story.stage == 11 && state.road_story.loaded.is_some())
    }

    pub fn wheat_drying_at(&self, tile: TilePos) -> bool {
        self.village.as_ref().is_some_and(|state| state.grain.drying_at(tile))
    }

    pub fn courtyard_guest_table_set(&self) -> bool {
        self.village.as_ref().is_some_and(|state| state.renovation.guest_table == 2)
    }

    pub fn home_renovation_stage(&self, owner: ActorKind) -> Option<u8> {
        let state = &self.village.as_ref()?.renovation;
        (state.displaced == Some(owner.index() as u8)).then_some(state.progress)
    }

    pub fn home_of(&self, actor: ActorKind) -> Option<Point> {
        (actor != ActorKind::Orange).then(|| self.home_door(actor.index()).point())
    }

    fn initial_home_door(&self, i: usize) -> Cell {
        if self.is_village() {
            village::building(i)
        } else {
            HOUSE_DOORS[i]
        }
    }

    fn pantry(&self) -> Cell {
        if self.is_village() {
            village::poi("pantry_loading")
        } else {
            PANTRY
        }
    }

    fn home_door(&self, i: usize) -> Cell {
        if self.village.as_ref().is_some_and(|state| state.renovation.displaced == Some(i as u8)
            && !state.renovation.displaced_room_empty) {
            return self.temporary_home_door();
        }
        self.story.housing.order_for_person(i).map_or_else(
            || self.initial_home_door(i),
            |order| {
                let [x, y] = order.door();
                Cell(x.into(), y.into())
            },
        )
    }

    fn temporary_home_door(&self) -> Cell {
        self.building_door(3)
    }

    fn home_side(&self, i: usize) -> Cell {
        if self.village.as_ref().is_some_and(|state| state.renovation.displaced == Some(i as u8)
            && !state.renovation.displaced_room_empty) {
            let door = self.temporary_home_door();
            return Cell(door.0, door.1 + 1);
        }
        self.story.housing.order_for_person(i).map_or_else(
            || {
                if self.is_village() {
                    let door = self.initial_home_door(i);
                    Cell(door.0 + 1, door.1 + 1)
                } else {
                    HOUSE_SIDE[i]
                }
            },
            |order| {
                let [x, y] = order.door();
                Cell(if i == 0 { x - 1 } else { x + 1 }.into(), y.into())
            },
        )
    }

    pub fn previous_home_use(&self, actor: ActorKind) -> Option<FormerHomeUse> {
        self.story.housing.order_for_person(actor.index())?;
        Some(if actor == ActorKind::Xiaohe {
            FormerHomeUse::GrainStorage
        } else {
            FormerHomeUse::ToolRoom
        })
    }

    fn tool_pickup(&self, i: usize, task: Task) -> Cell {
        if self.is_village() && matches!(task, Task::GetWater | Task::GetHomeWater) {
            return self.site(i, task);
        }
        if task == Task::UnpackHome {
            return self.home_side(i);
        }
        if let Task::Renovate(work) = task {
            if work == renovation::Work::MoveOut(i as u8) {
                return self.initial_home_door(i);
            }
            if work == renovation::Work::MoveBack(i as u8) {
                return self.temporary_home_door();
            }
        }
        if let Task::Grain(work) = task {
            return match work {
                grain::Work::CarrySheaf => Cell::from(WILLOW_VILLAGE.crops[usize::from(grain::FIELD)].work),
                grain::Work::StoreRoot => Cell::from(WILLOW_VILLAGE.crops[usize::from(grain::FIELD)].work),
                grain::Work::Spread => self.building_door(4),
                grain::Work::CookRoot => self.building_door(3),
                grain::Work::CookFlour | grain::Work::CookTraineeFlour => self.building_door(4),
                grain::Work::Cover | grain::Work::Store => self.site(i, Task::Grain(grain::Work::Turn)),
                _ => self.pantry(),
            };
        }
        if task == Task::WorkshopStory(workshop_story::Work::DeliverOrder) {
            return self.building_door(4);
        }
        if task == Task::MarketStory(road_story::MarketWork::CarryMill) {
            return self.building_door(4);
        }
        if matches!(task, Task::Fish | Task::WorkshopStory(workshop_story::Work::OilRod))
            && self.village.as_ref().is_some_and(|state| state.opening.rod_on_rack)
        {
            return self.building_door(4);
        }
        if self.is_village() && i == ActorKind::Ayu.index() && matches!(task, Task::FellTree(_))
            && self.previous_home_use(ActorKind::Ayu) != Some(FormerHomeUse::ToolRoom)
        {
            return self.building_door(4);
        }
        if task == Task::WorkshopStory(workshop_story::Work::MillBeam) {
            return self.building_door(4);
        }
        if i < 2 && matches!(task, Task::BuildHome(order) if order.phase == HousingPhase::MoveIn) {
            return self.home_door(i);
        }
        if self.input(task).is_none()
            && task.tool().is_some()
            && self.previous_home_use(ActorKind::Ayu) == Some(FormerHomeUse::ToolRoom)
        {
            return self.initial_home_door(1);
        }
        if self.is_village() && i == ActorKind::Ayu.index()
            && matches!(task, Task::Develop(_) | Task::FreightBridge | Task::RoadStory(_) | Task::MarketStory(_)
                | Task::FillBowl | Task::ServeCat)
        {
            return self.building_door(3);
        }
        self.pantry()
    }

    pub fn housing_sites(&self) -> impl Iterator<Item = HousingView> + '_ {
        (0..HOUSING_LOTS.len()).filter_map(|lot| {
            let (order, phase, occupied) = self.story.housing.housing_site(lot).or_else(|| {
                self.actors.iter().find_map(|actor| match actor.task {
                    Some(Task::BuildHome(order))
                        if order.lot as usize == lot && order.phase == HousingPhase::Clear =>
                    {
                        Some((order, HousingPhase::Clear, 0))
                    }
                    _ => None,
                })
            })?;
            let [x, y] = order.door();
            let [left, top] = order.lot_origin();
            let material_position = self.is_village().then(|| {
                let id = match order.plan {
                    HousePlan::Shared => "new_shared_home",
                    HousePlan::Single => "new_single_home",
                };
                let building = WILLOW_VILLAGE.building(id).expect("compiled house material site");
                Cell(
                    u16::from(left) + building.material_slot.x - building.footprint.x,
                    u16::from(top) + building.material_slot.y - building.footprint.y,
                ).point()
            });
            Some(HousingView {
                kind: order.plan,
                position: Cell(x.into(), y.into()).point(),
                lot_origin: Point {
                    x: (i32::from(left) * TILE_PX) as f32,
                    y: (i32::from(top) * TILE_PX) as f32,
                },
                material_position,
                footprint: order.plan.footprint(),
                phase,
                occupied: occupied != 0,
            })
        })
    }

    pub fn terrain_revision(&self) -> u32 {
        if self.is_village() {
            return self.housing_sites().fold(
                u32::from(self.phase_mask().bits()),
                |value, site| {
                    value.rotate_left(7)
                        ^ (site.lot_origin.x as u32).rotate_left(16)
                        ^ site.lot_origin.y as u32
                        ^ site.phase as u32
                },
            ) ^ u32::from(self.village.as_ref().unwrap().renovation.path).rotate_left(11)
                ^ u32::from(self.village.as_ref().unwrap().road_story.stage).rotate_left(13)
                ^ u32::from(self.village.as_ref().unwrap().market_story.stage).rotate_left(18)
                ^ u32::from(self.market_lamps_stage()).rotate_left(22)
                ^ self.trees.iter().enumerate().fold(0, |bits, (i, tree)| {
                bits | (u32::from(tree.stage != TreeStage::Stump) << (i + 16))
            });
        }
        self.housing_sites().enumerate().fold(
            u32::from(self.story.finished)
                | (u32::from(self.workshop_door() == WORKSHOP_DOOR) << 5),
            |value, (index, site)| {
                let x = (site.lot_origin.x as u32 / TILE_PX as u32) & 0b11_1111;
                let kind = u32::from(site.kind == HousePlan::Shared);
                let blocked = u32::from(site.phase >= HousingPhase::Foundation);
                value | ((x | kind << 6 | blocked << 7) << (8 * (index + 1)))
            },
        )
    }

    pub fn market_layout(&self) -> Option<MarketLayout> {
        if self.is_village() {
            return None;
        }
        self.story
            .is_open(DevelopmentKind::Market)
            .then_some(MarketLayout {
                origin: Point {
                    x: 17.0 * TILE_PX as f32,
                    y: 33.0 * TILE_PX as f32,
                },
                footprint: [5, 3],
                counter: Point::at_tile(19, 33),
                approach: MARKET_TRADE.point(),
                bins: [Point::at_tile(18, 33), Point::at_tile(20, 33)],
            })
    }

    pub fn active_building_improvement(&self) -> Option<(ImprovementOrder, Point)> {
        let order = self
            .actors
            .iter()
            .find_map(|actor| match actor.task {
                Some(Task::Improve(order)) if actor.stage != Stage::Finish => Some(order),
                _ => None,
            })
            .or_else(|| self.story.improvements.active())?;
        Some((order, self.improvement_site(order.fixture).point()))
    }

    pub fn building_fixtures(&self) -> impl Iterator<Item = (FixtureStatus, Point)> + '_ {
        self.story
            .improvements
            .installed()
            .map(|status| (status, self.improvement_site(status.fixture).point()))
    }

    fn building_door(&self, site: usize) -> Cell {
        Cell::at(self.buildings[site].position).expect("validated building position")
    }

    fn workshop_door(&self) -> Cell {
        self.building_door(WORKSHOP_SITE)
    }

    fn improvement_site(&self, fixture: Fixture) -> Cell {
        let site = usize::from(fixture.site());
        if site < BUILDINGS.len() {
            return self.building_door(site);
        }
        self.development_site(site - 2)
    }

    fn development_site(&self, index: usize) -> Cell {
        if self.is_village() {
            return village::poi(village::DEVELOPMENT_POIS[index]);
        }
        let site = SITES[index];
        Cell(site.x.into(), site.y.into())
    }

    /// Moves the original workshop after any work in flight has finished and
    /// the destination is free. The host supplies visitor clearance.
    pub fn relocate_legacy_workshop(&mut self, visitor_clear: bool) -> bool {
        if !visitor_clear || self.buildings[WORKSHOP_SITE].position != LEGACY_WORKSHOP_DOOR.point()
            || self.actors.iter().any(|actor| {
                matches!(actor.task, Some(Task::Build(site)) if usize::from(site) == WORKSHOP_SITE)
                    || matches!(actor.task, Some(Task::Improve(order)) if usize::from(order.fixture.site()) == WORKSHOP_SITE)
                    || std::iter::once(actor.cell)
                        .chain(actor.route[actor.route_next..].iter().copied())
                        .any(|cell| workshop_wall(WORKSHOP_DOOR, cell.0.into(), cell.1.into()))
            })
        {
            return false;
        }
        let walkable = |x, y| {
            walkable_at(x, y, self.story.finished)
                && !self.story.housing.blocks(x, y)
                && !workshop_wall(WORKSHOP_DOOR, x, y)
        };
        if self
            .actors
            .iter()
            .map(|actor| actor.cell)
            .chain([WORKSHOP_DOOR, LEGACY_WORKSHOP_DOOR])
            .any(|cell| route_by(PANTRY, cell, walkable).is_none())
        {
            return false;
        }
        self.buildings[WORKSHOP_SITE].position = WORKSHOP_DOOR.point();
        true
    }

    pub fn workshop_relocation_tiles(&self) -> Option<[[i32; 2]; 2]> {
        (self.buildings[WORKSHOP_SITE].position == LEGACY_WORKSHOP_DOOR.point())
            .then_some([[35, 12], [36, 12]])
    }

    pub fn developments(&self) -> [Development; SITES.len()] {
        std::array::from_fn(|index| {
            let site = SITES[index];
            if index == 1
                && let Some(state) = &self.village
                && state.imported_pedestrian_bridge
                && !state.freight_inspected
            {
                return Development {
                    kind: site.kind,
                    position: self.development_site(index).point(),
                    progress: state.freight_shifts * 33,
                    phase: state.freight_shifts,
                    complete: false,
                };
            }
            let past = self.story.completed().saturating_sub(index * 3).min(3);
            let active = self
                .story
                .active()
                .filter(|active| usize::from(active.id) / 3 == index);
            Development {
                kind: site.kind,
                position: self.development_site(index).point(),
                phase: active.map_or(past as u8, |project| project.phase),
                progress: ((past * 100 + active.map_or(0, |project| usize::from(project.progress)))
                    / 3) as u8,
                complete: past == 3,
            }
        })
    }
    pub fn bowl_water(&self) -> u16 {
        self.bowl
    }
    pub fn is_catching_up(&self) -> bool {
        self.now_ms < self.target_ms || self.next_deadline() <= self.target_ms
    }

    pub fn ground(&self, x: i32, y: i32) -> Ground {
        if self.is_village() {
            let Some(tile) = self.village_tile(x, y) else {
                return Ground::Rock;
            };
            if self.story.housing.blocks(x, y) {
                return Ground::Building;
            }
            if tile.blocked && !tile.scenic_obstacle && tile.terrain == Terrain::Grass {
                let pos = TilePos::new(x as u16, y as u16);
                return if WILLOW_VILLAGE
                    .crops
                    .iter()
                    .any(|crop| crop.rect.contains(pos))
                {
                    Ground::Soil
                } else {
                    Ground::Building
                };
            }
            let state = self.village.as_ref().unwrap();
            let at = TilePos::new(x as u16, y as u16);
            if state.renovation.walk_surface(at) || state.road_story.covered(at) || state.market_story.covered(at) {
                return Ground::Path;
            }
            return match tile.terrain {
                Terrain::Grass => Ground::Grass,
                Terrain::Water => Ground::Water,
                Terrain::Road | Terrain::Bridge => Ground::Path,
                Terrain::Boundary => Ground::Rock,
            };
        }
        if workshop_wall(self.workshop_door(), x, y) || self.story.housing.blocks(x, y) {
            Ground::Building
        } else if self.housing_approach(x, y) || self.workshop_apron(x, y) {
            Ground::Path
        } else {
            developed_ground(x, y, self.story.finished)
        }
    }

    pub fn yard_surface(&self, x: i32, y: i32) -> Option<YardSurface> {
        if self.is_village() {
            return None;
        }
        if self.housing_approach(x, y) {
            Some(YardSurface::Doorstep)
        } else if self.workshop_apron(x, y) {
            Some(YardSurface::Loading)
        } else {
            yard_surface(x, y)
        }
    }

    fn workshop_apron(&self, x: i32, y: i32) -> bool {
        let door = self.workshop_door();
        y == i32::from(door.1) && ((i32::from(door.0) - 1)..=i32::from(door.0) + 1).contains(&x)
    }

    fn housing_approach(&self, x: i32, y: i32) -> bool {
        self.story.housing.has_sites()
            && !self.story.housing.blocks(x, y)
            && developed_ground(x, y, self.story.finished) == Ground::Grass
            && walkable_at(x, y, self.story.finished)
            && self.story.housing.orders().any(|order| {
                if order.phase < HousingPhase::Frame {
                    return false;
                }
                let [dx, dy] = order.door().map(i32::from);
                match order.lot_origin() {
                    [1, 5] => {
                        (x == dx && (dy..=14).contains(&y)) || (y == 14 && (dx..7).contains(&x))
                    }
                    [41, 4] => {
                        (x == dx && (dy..=11).contains(&y))
                            || (y == 11 && (dx..=44).contains(&x))
                            || (x == 44 && (11..14).contains(&y))
                    }
                    [25, 2] => {
                        (x == dx && (dy..=10).contains(&y))
                            || (y == 10 && (dx.min(27)..=dx.max(27)).contains(&x))
                            || (x == 27 && (10..14).contains(&y))
                    }
                    _ => x == dx && (dy..dy + 2).contains(&y),
                }
            })
    }
    pub fn can_walk(&self, x: i32, y: i32) -> bool {
        if self.is_village() {
            return self.village_tile(x, y).is_some_and(|tile| !tile.blocked)
                && !self.market_lamp_tiles().get(3..).is_some_and(|lamps| lamps.contains(&TilePos::new(x as u16,y as u16)))
                && !self.story.housing.blocks(x, y)
                && (y < 64
                    || !self.trees.iter().any(|tree| {
                        tree.stage != TreeStage::Stump
                            && Cell::at(tree.position) == Some(Cell(x as u16, y as u16))
                    }));
        }
        let market_bin =
            y == 33 && matches!(x, 18 | 20) && self.story.is_open(DevelopmentKind::Market);
        !market_bin
            && !workshop_wall(self.workshop_door(), x, y)
            && !self.story.housing.blocks(x, y)
            && walkable_at(x, y, self.story.finished)
    }

    fn village_tile(&self, x: i32, y: i32) -> Option<crate::map_data::TileInfo> {
        let pos = TilePos::new(x.try_into().ok()?, y.try_into().ok()?);
        WILLOW_VILLAGE.tile(pos, self.phase_mask())
    }

    fn village_map_view(&self) -> MapView<'static> {
        let mut view = MapView::new(&WILLOW_VILLAGE, self.phase_mask());
        for &tile in self.market_lamp_tiles().get(3..).unwrap_or_default() {
            view.set_blocked(tile,true);
        }
        for tree in &self.trees {
            if tree.stage != TreeStage::Stump {
                if let Some(cell) = Cell::at(tree.position) {
                    view.set_blocked(cell.tile(), true);
                }
            }
        }
        for order in self
            .story
            .housing
            .orders()
            .filter(|order| order.phase >= HousingPhase::Frame)
        {
            let [x, y] = order.lot_origin();
            let [w, h] = order.plan.footprint();
            for ty in y..y + h {
                for tx in x..x + w {
                    view.set_blocked(TilePos::new(tx.into(), ty.into()), true);
                }
            }
        }
        view
    }

    fn overlay_occupied(&self, i: usize, start: Cell, target: Cell, map: &mut MapView<'_>) {
        let occupied = self.external_occupied.iter().chain(
            self.actors
                .iter()
                .enumerate()
                .filter(|(j, actor)| *j != i && !actor.inside)
                .flat_map(|(_, actor)| {
                    std::iter::once(actor.cell.tile()).chain(
                        actor
                            .route
                            .get(actor.route_next)
                            .filter(|_| !actor.waiting_edge)
                            .map(|cell| cell.tile()),
                    )
                }),
        );
        for tile in occupied.filter(|&tile| tile != start.tile() && tile != target.tile()) {
            map.set_blocked(tile, true);
        }
        if !self.pantry_approach(start) && !self.pantry_approach(target) {
            let dock = self.pantry();
            for y in dock.1 - 1..=dock.1 + 1 {
                for x in dock.0 - 2..=dock.0 + 1 {
                    map.set_blocked(TilePos::new(x, y), true);
                }
            }
        }
    }

    fn well_draw_stand() -> Cell {
        static STAND: std::sync::OnceLock<Cell> = std::sync::OnceLock::new();
        *STAND.get_or_init(|| village::slot("home_well", "home_well.draw"))
    }

    fn well_stand_reserved(&self, i: usize) -> bool {
        let stand = Self::well_draw_stand();
        self.actors.iter().enumerate().any(|(j, other)| {
            j != i && matches!(other.task, Some(Task::GetWater | Task::GetHomeWater | Task::DrinkAtWell))
                && (other.route_goal == Some(stand) || other.cell == stand)
        })
    }

    fn can_commit_phases(&self, phases: PhaseMask) -> bool {
        self.actors.iter().all(|actor| {
            std::iter::once(actor.cell)
                .chain(actor.route.iter().skip(actor.route_next).copied())
                .all(|at| {
                    WILLOW_VILLAGE
                        .tile(at.tile(), phases)
                        .is_some_and(|tile| !tile.blocked)
                })
        }) && self.external_occupied.iter().all(|at| {
            WILLOW_VILLAGE
                .tile(at, phases)
                .is_some_and(|tile| !tile.blocked)
        })
    }

    pub fn next_update_ms(&self) -> u64 {
        self.next_deadline()
    }

    /// Pays an already validated outside transaction. Failure leaves stock untouched.
    pub fn spend_stock(&mut self, item: Item, amount: u16) -> bool {
        self.take_stock(item, amount)
    }

    /// Pays all inputs together, including when the same item occurs more than once.
    pub fn spend_stock_bundle(&mut self, costs: &[(Item, u16)]) -> bool {
        self.exchange_stock_atomic(costs, &[])
    }

    /// Applies the complete exchange or leaves the stock and its freshness ledger intact.
    pub fn exchange_stock_atomic(&mut self, costs: &[(Item, u16)], goods: &[(Item, u16)]) -> bool {
        let mut result = self.stock.clone();
        for &(item, amount) in costs {
            if !result.take(item, amount) {
                return false;
            }
        }
        let crops_before = self.stock.count(Item::Crops);
        let crops_withdrawn = crops_before - result.count(Item::Crops);
        for &(item, amount) in goods {
            if !result.add(item, amount)
                || item == Item::Food && !result.fits(item, self.meal_escrow)
            {
                return false;
            }
        }
        self.consume_crop_batches(crops_withdrawn, crops_before);
        for &(item, amount) in goods {
            if item == Item::Crops {
                self.record_crops(amount);
            }
        }
        self.stock = result;
        true
    }

    fn take_stock(&mut self, item: Item, amount: u16) -> bool {
        let before = self.stock.count(item);
        if !self.stock.take(item, amount) {
            return false;
        }
        if item == Item::Crops {
            self.consume_crop_batches(amount, before);
        }
        true
    }

    fn consume_crop_batches(&mut self, amount: u16, total_before: u16) {
        // Crops from older saves had no date. Consume them before the tracked harvests.
        let legacy = total_before.saturating_sub(self.crop_batches.units());
        self.crop_batches.take(amount.saturating_sub(legacy));
    }

    fn record_crops(&mut self, amount: u16) {
        self.record_crops_at(amount, false);
    }

    fn store_recovered_seed(&mut self) {
        if !self.stock.add(Item::Seeds, 1) && let Some(state) = &mut self.village {
            // Surplus seeds return to the same bounded organic matter stream.
            // A full seed box cannot indefinitely block a separate food payload.
            state.soil.scraps();
        }
    }

    fn record_crops_at(&mut self, amount: u16, former_home: bool) {
        let base = if self.story.is_open(DevelopmentKind::Depot) {
            6
        } else {
            3
        };
        let local = if self.story.improvements.working(Fixture::PantryCoolShelf) {
            self.story.improvements.used(Fixture::PantryCoolShelf);
            2
        } else if self.story.improvements.working(Fixture::PantryDryRack)
            && self.village.as_ref().is_none_or(|state| !state.preserves.rack_reserved())
        {
            self.story.improvements.used(Fixture::PantryDryRack);
            1
        } else {
            0
        };
        let depot = self.story.improvements.working(Fixture::DepotVentRack);
        if depot {
            self.story.improvements.used(Fixture::DepotVentRack);
        }
        let structural = u64::from(self.buildings[3].level.saturating_sub(2));
        let shelf_days = base + local + u64::from(depot) + structural + u64::from(former_home);
        let expires = (self.now_ms / DAY_MS + shelf_days) * DAY_MS;
        self.crop_batches.add(amount, expires);
    }

    fn assess_story_day(&mut self) {
        let day = self.now_ms / DAY_MS;
        if self.story.last_assessed_day == Some(day) {
            return;
        }
        self.story.last_assessed_day = Some(day);
        let season = self.season();
        if let Some(village) = &mut self.village {
            village.advance_orchard(day);
            village.opening.review_day(day);
            village.grain.day(day, season);
            let eligible = village.grain.completed("Q13") && village.grain.completed("Q14")
                && village.soil.completed("Q19");
            let preceding_season = [YardSeason::Spring, YardSeason::Summer, YardSeason::Autumn, YardSeason::Winter]
                [((day.saturating_sub(1) / 28) % 4) as usize];
            village.pantry_plan.day(day, preceding_season == YardSeason::Winter, eligible, &mut self.stock);
        }
        // Milestones acknowledge elapsed seasons only after the corresponding service exists.
        for (bit, threshold, chapter) in [(1, 28, 3), (2, 56, 6), (4, 112, 12), (8, 224, 24)] {
            if day >= threshold && self.story.completed() >= chapter {
                self.story.milestones |= bit;
            }
        }
    }

    fn finish_development(&mut self) {
        if self
            .story
            .finish_due()
            .is_some_and(|due| due <= self.now_ms)
        {
            if self.market_handoff_cells_in_use() {
                return;
            }
            if let Some(mut village) = self.village.clone() {
                village.apply_development(self.story.finished + 1, false);
                if !self.can_commit_phases(village.phases()) {
                    return;
                }
                self.village = Some(village);
            }
            self.story.finished += 1;
            self.story.running = None;
        }
    }

    fn development_task(&self, i: usize) -> Option<Task> {
        if let Some(state) = &self.village {
            let can_work = self.actors[i].stamina >= 45
                && self.stock.count(Item::Food) >= 6
                && self.tool_available(i, Tool::Hammer)
                && !matches!(self.weather(), YardWeather::Rain | YardWeather::Snow);
            if state.imported_pedestrian_bridge
                && !state.freight_inspected
                && (state.freight_material_paid || self.town_allows(TownEra::Workshop))
                && can_work
                && !self.task_in_use(Task::FreightBridge)
                && (state.freight_material_paid || self.stock.count(Item::Stone) >= 3)
            {
                return Some(Task::FreightBridge);
            }
        }
        let id = self.story.finished;
        let chapter = CHAPTERS.get(usize::from(id))?;
        let task = Task::Develop(id);
        if !self.town_permits_project(id) || self.now_ms / DAY_MS + 1 < u64::from(chapter.earliest_day)
            || self.stock.count(Item::Food) < 6
            || self.actors[i].stamina < 45
            || !self.tool_available(i, Tool::Hammer)
            || self.task_in_use(task)
            || self.is_village() && matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            return None;
        }
        if let Some(project) = self.story.running {
            return (project.shifts < chapter.work_shifts && project.next_shift <= self.now_ms)
                .then_some(task);
        }
        (self.stock.count(chapter_item(chapter.material)) >= chapter.amount).then_some(task)
    }

    fn trade_task(&self, i: usize) -> Option<Task> {
        if self.is_village()
            || !self.story.is_open(DevelopmentKind::Road)
            || self.story.next_trade > self.now_ms
            || self.stock.count(Item::Food) < 7
            || self.actors[i].stamina < 45
            || !self.tool_available(i, Tool::HarvestBasket)
            || self
                .actors
                .iter()
                .any(|actor| matches!(actor.task, Some(Task::Trade(..))))
        {
            return None;
        }
        let crops = self.stock.count(Item::Crops);
        let needs = CHAPTERS
            .get(self.story.completed())
            .map(|chapter| (chapter_item(chapter.material), chapter.amount))
            .filter(|&(item, amount)| self.stock.count(item) < amount)
            .or_else(|| {
                self.story
                    .housing
                    .next_order(self.now_ms)
                    .and_then(|order| order.phase.material(order.plan))
                    .map(|(material, amount)| (chapter_item(material), amount))
                    .filter(|&(item, amount)| self.stock.count(item) < amount)
            })
            .or_else(|| {
                self.structural_candidate()
                    .map(|(site, _, _)| self.build_input(site))
                    .filter(|&(item, amount)| self.stock.count(item) < amount)
            })
            .or_else(|| {
                self.story
                    .improvements
                    .orders(self.now_ms, self.season() as usize)
                    .filter(|order| self.improvement_unlocked(order.fixture))
                    .map(|order| order.material())
                    .map(|(material, amount)| (chapter_item(material), amount))
                    .find(|&(item, amount)| self.stock.count(item) < amount)
            });
        let item = needs
            .filter(|&(item, amount)| self.stock.count(item) < amount)
            .map(|(item, _)| item)
            .or_else(|| {
                (crops >= 21)
                    .then(|| {
                        [Item::Wood, Item::Stone, Item::Rope]
                            .into_iter()
                            .filter(|&item| self.stock.count(item) < 12)
                            .min_by_key(|&item| self.stock.count(item))
                    })
                    .flatten()
            })?;
        let amount = match item {
            Item::Stone => 4,
            Item::Wood if self.story.improvements.working(Fixture::StallStockBins) => 6,
            Item::Wood if self.story.is_open(DevelopmentKind::Stall) => 7,
            _ => 5,
        };
        (crops >= 9 + amount).then_some(Task::Trade(item, amount))
    }

    fn development_commit(&mut self, i: usize, id: u8) {
        let chapter = &CHAPTERS[usize::from(id)];
        if self.story.finished != id || !self.town_permits_project(id) {
            return;
        }
        if self.story.running.is_none() {
            let item = chapter_item(chapter.material);
            if !self.has_carry(i, item, chapter.amount) {
                return;
            }
            self.drop_carry(i);
            self.story.running = Some(Project {
                id,
                begun: self.now_ms,
                shifts: 0,
                next_shift: self.now_ms,
            });
        }
        if let Some(project) = &mut self.story.running
            && project.next_shift <= self.now_ms
            && project.shifts < chapter.work_shifts
        {
            project.shifts += 1;
            project.next_shift = self.now_ms + DAY_MS / 2;
        }
    }

    fn route(&self, start: Cell, target: Cell) -> Option<Vec<Cell>> {
        route_by(start, target, |x, y| self.can_walk(x, y))
    }

    fn can_reach_static(&self, start: Cell, target: Cell) -> bool {
        if self.is_village() {
            self.valid_world_cell(start) && self.valid_world_cell(target)
        } else {
            self.route(start, target).is_some()
        }
    }

    fn valid_world_cell(&self, cell: Cell) -> bool {
        (self.is_village() || cell.valid()) && self.can_walk(cell.0.into(), cell.1.into())
    }

    pub fn advance_to(&mut self, wall_secs: u64) {
        self.advance_to_wall_ms(wall_secs.saturating_mul(1_000));
    }

    pub fn advance_to_wall_ms(&mut self, wall_ms: u64) {
        self.queue_wall_time(wall_ms);
        self.advance_pending_to(self.target_ms);
    }

    /// Retain the real-time anchor without consuming any simulation event yet.
    pub fn queue_wall_time(&mut self, wall_ms: u64) {
        let elapsed = wall_ms.saturating_sub(self.last_wall_ms);
        if let Some(calendar) = self.village.as_mut().and_then(|state| state.calendar.as_mut()) {
            calendar.observe(wall_ms, self.target_ms, self.clock_fraction);
        }
        self.last_wall_ms = self.last_wall_ms.max(wall_ms);
        let weighted = u128::from(elapsed) * 96 + u128::from(self.clock_fraction);
        let (target, carry) = advance_clock(self.target_ms, weighted);
        self.target_ms = target;
        self.clock_fraction = carry;
    }

    pub fn pending_world_target_ms(&self) -> u64 {
        self.target_ms
    }

    pub fn advance_ms(&mut self, delta_ms: u64) {
        self.advance_world_ms(delta_ms);
    }

    /// Advance with an externally driven world clock. Call with zero to continue catching up.
    pub fn advance_world_ms(&mut self, delta_ms: u64) {
        self.queue_world_time(delta_ms);
        self.advance_pending_to(self.target_ms);
    }

    pub fn queue_world_time(&mut self, delta_ms: u64) {
        self.target_ms = self
            .target_ms
            .saturating_add(delta_ms)
            .min(u64::MAX - DAY_MS);
    }

    /// Advance as far as a host deadline or the queued target, whichever is earlier.
    /// A false result means the event budget was exhausted before that deadline.
    pub fn advance_pending_to(&mut self, checkpoint_ms: u64) -> bool {
        let target = checkpoint_ms.min(self.target_ms).max(self.now_ms);
        let budget = if self.is_village() {
            128
        } else {
            MAX_EVENTS_PER_ADVANCE
        };
        let mut node_budget = 2_048;
        for _ in 0..budget {
            if let Some(i) = self.pending_route_actor() {
                if node_budget == 0 || !self.pump_route(i, &mut node_budget) {
                    break;
                }
                continue;
            }
            let next = self.next_deadline();
            if next > target {
                self.record_home_time(target);
                self.now_ms = target;
                break;
            }
            let entered_weather_spell = next / (3 * HOUR) != self.now_ms / (3 * HOUR);
            self.record_home_time(next);
            self.now_ms = next;
            if entered_weather_spell && Self::wet_spell(next / (3 * HOUR)) {
                self.footprints.erase();
            }
            self.environment();
            for i in 0..self.actors.len() {
                if self.actors[i].until == self.now_ms && self.actors[i].stage != Stage::Idle {
                    self.step(i);
                }
            }
            self.meet_for_meal();
            self.finish_development();
            self.offer_shared_meal();
            for i in 0..self.actors.len() {
                if self.actors[i].until == self.now_ms && self.actors[i].stage == Stage::Idle {
                    self.plan(i);
                }
            }
        }
        for actor in &mut self.actors {
            update_position(actor, self.now_ms);
        }
        self.now_ms == target && self.next_deadline() > target
    }

    fn record_home_time(&mut self, until: u64) {
        if !self.is_village() || until <= self.now_ms {
            return;
        }
        let activity: [(bool, bool); 2] = std::array::from_fn(|i| {
            let actor = &self.actors[i];
            (
                actor.pose != Pose::Sleep,
                self.in_household_region(actor.cell),
            )
        });
        let state = self.village.as_mut().expect("village time stats");
        let mut cursor = self.now_ms;
        while cursor < until {
            let day = cursor / DAY_MS;
            let slot = (day % 7) as usize;
            if state.tracked_day != Some(day) {
                state.tracked_day = Some(day);
                state.awake_ms[slot] = [0; 2];
                state.home_ms[slot] = [0; 2];
            }
            let end = until.min((day + 1) * DAY_MS);
            let elapsed = (end - cursor) as u32;
            for (i, (awake, at_home)) in activity.into_iter().enumerate() {
                if awake {
                    state.awake_ms[slot][i] += elapsed;
                    if at_home {
                        state.home_ms[slot][i] += elapsed;
                    }
                }
            }
            cursor = end;
        }
    }

    pub fn navigation_pending(&self) -> bool {
        self.pending_route_actor().is_some()
    }

    fn pending_route_actor(&self) -> Option<usize> {
        self.is_village()
            .then(|| {
                self.actors.iter().position(|actor| {
                    actor.route_goal.is_some()
                        && actor.route.is_empty()
                        && !actor.waiting_edge
                        && actor.passing.is_none()
                        && actor.until <= self.now_ms
                })
            })
            .flatten()
    }

    fn pump_route(&mut self, i: usize, budget: &mut usize) -> bool {
        let actor = &self.actors[i];
        let target = actor.route_goal.expect("pending destination");
        let batch = (*budget).min(64);
        *budget -= batch;
        match self
            .village_navigation
            .pump(self, i, actor.cell, target, actor.avoid_occupied, batch)
        {
            village::RouteSegment::Ready(route) => {
                self.actors[i].avoid_occupied = false;
                if route.is_empty() {
                    self.actors[i].route_goal = None;
                    self.step(i);
                    return true;
                }
                self.actors[i].route = route;
                self.actors[i].route_next = 0;
                self.next_edge(i);
                true
            }
            village::RouteSegment::Pending => *budget != 0,
            village::RouteSegment::Blocked => {
                if actor.avoid_occupied {
                    self.actors[i].avoid_occupied = false;
                    return true;
                }
                let task = actor.task;
                // An external owner must distinguish an unsuccessful route from arrival.
                self.actors[i].moving_box = false;
                self.idle(i, MINUTE);
                if matches!(task, Some(Task::External(_))) {
                    self.actors[i].outside = false;
                }
                true
            }
        }
    }

    fn next_deadline(&self) -> u64 {
        self.actors
            .iter()
            .map(|actor| actor.until)
            .chain([self.next_needs, self.next_well, self.next_fish])
            .chain(self.crop_batches.next_expiry())
            .chain(self.village.as_ref().and_then(|state| {
                let opening = &state.opening;
                (!opening.shared_serving && opening.shared_until != 0).then_some(opening.shared_until)
            }))
            .chain(self.village.as_ref().map(|state| state.opening.late_meal_until).filter(|&time| time != 0))
            .chain(self.village.as_ref().and_then(|state| {
                (!state.grain.serving && state.grain.ready_until != 0).then_some(state.grain.ready_until)
            }))
            .chain(self.village.as_ref().and_then(|state| state.grain.root_due()))
            .chain(self.village.as_ref().and_then(|state| state.calendar.as_ref())
                .and_then(|calendar| calendar.next_boundary(self.now_ms)))
            .chain(
                self.village
                    .as_ref()
                    .into_iter()
                    .flat_map(|state| state.tree_due)
                    .filter(|&due| due > 0),
            )
            .chain(self.story.finish_due().and_then(|due| {
                (!self.market_handoff_cells_in_use()).then_some(due.max(self.now_ms))
            }))
            .chain(
                self.plots
                    .iter()
                    .filter_map(|plot| (plot.due != 0).then_some(plot.due)),
            )
            .min()
            .expect("fixed household has deadlines")
    }

    fn environment(&mut self) {
        self.assess_story_day();
        let weather = self.weather();
        if let Some(village) = &mut self.village {
            village.soil.tick(self.now_ms);
            if village.preserves.advance(self.now_ms) {
                self.story.spoiled = self.story.spoiled.saturating_add(u64::from(preserves::CROP_UNITS));
            }
            if self.next_needs == self.now_ms && self.now_ms / DAY_MS / 28 % 4 == 3 {
                village.pantry_plan.replenish(&mut self.stock);
            }
            self.story.spoiled = self.story.spoiled.saturating_add(u64::from(village.grain.expire_roots(self.now_ms)));
            if self.next_needs == self.now_ms {
                village.grain.elapsed_drying(self.now_ms, weather);
            }
            if !village.grain.serving && village.grain.ready_until != 0 && village.grain.ready_until <= self.now_ms {
                village.grain.ready_until = 0;
                self.story.spoiled = self.story.spoiled.saturating_add(2);
            }
            let opening = &mut village.opening;
            if !opening.shared_serving && opening.shared_until != 0 && opening.shared_until <= self.now_ms {
                self.story.spoiled = self.story.spoiled.saturating_add(2);
                opening.shared_until = 0;
            }
            if opening.late_meal_until != 0 && opening.late_meal_until <= self.now_ms {
                self.story.spoiled = self.story.spoiled.saturating_add(1);
                opening.late_meal_until = 0;
            }
            for (tree, due) in self.trees.iter_mut().zip(&mut village.tree_due) {
                if *due == 0 || *due > self.now_ms {
                    continue;
                }
                let Some(cell) = Cell::at(tree.position) else {
                    continue;
                };
                let in_use = self.external_occupied.contains(cell.tile())
                    || self.actors.iter().any(|actor| {
                        actor.cell == cell
                            || actor
                                .route
                                .iter()
                                .skip(actor.route_next)
                                .any(|&step| step == cell)
                    });
                if in_use {
                    *due = self.now_ms + HOUR;
                } else {
                    tree.stage = TreeStage::Standing;
                    tree.timber = 3;
                    *due = 0;
                }
            }
        }
        if self
            .crop_batches
            .next_expiry()
            .is_some_and(|expires| expires <= self.now_ms)
        {
            let expired = self.crop_batches.expire(self.now_ms);
            let removed = self.stock.take(Item::Crops, expired);
            debug_assert!(removed);
            self.story.spoiled = self.story.spoiled.saturating_add(u64::from(expired));
        }
        if self.next_fish == self.now_ms {
            if self.fish_in_pond >= 2 {
                self.fish_in_pond = (self.fish_in_pond + 1).min(8);
            }
            self.next_fish = self.now_ms + 12 * HOUR;
        }
        if self.next_well == self.now_ms {
            let recharge = if self.story.is_open(DevelopmentKind::Well) {
                2
            } else {
                1
            };
            self.well = (self.well + recharge).min(WELL_CAP);
            self.next_well = self.now_ms + 20 * MINUTE;
        }
        if self.next_needs == self.now_ms {
            if self.is_village() && matches!(self.weather(), YardWeather::Rain) {
                let phases = self.phase_mask();
                let rain_spell = self.now_ms / (3 * HOUR);
                let state = self.village.as_mut().expect("rain state");
                for (i, (plot, definition)) in
                    self.plots.iter_mut().zip(WILLOW_VILLAGE.crops).enumerate()
                {
                    if !phases.contains(definition.phase)
                        || !matches!(plot.stage, PlotStage::Seed | PlotStage::Growing)
                    {
                        continue;
                    }
                    if rain_spell % 4 == 0 && i >= 6 && (rain_spell + i as u64) % 5 == 0
                        && !state.soil.has_clear_outlet(i)
                    {
                        state.flooded_plots |= 1 << i;
                    } else if !plot.watered && state.flooded_plots & (1 << i) == 0 {
                        plot.watered = true;
                        plot.due = self.now_ms
                            + if plot.stage == PlotStage::Seed {
                                55 * MINUTE
                            } else {
                                105 * MINUTE
                            };
                    }
                }
            }
            let cold = matches!(self.season(), YardSeason::Autumn | YardSeason::Winter);
            for actor in &mut self.actors {
                let warm = weatherstrip(actor.kind)
                    .is_some_and(|fixture| self.story.improvements.working(fixture))
                    && cold;
                actor.hunger = actor
                    .hunger
                    .saturating_add(match (actor.pose, warm) {
                        (Pose::Sleep, true) => 0,
                        (Pose::Sleep, false) => 1,
                        _ => 2,
                    })
                    .min(100);
                actor.thirst = actor
                    .thirst
                    .saturating_add(if actor.pose == Pose::Work {
                        4
                    } else if actor.kind == ActorKind::Ayu
                        && actor.pose == Pose::Sleep
                        && self.buildings[1].level >= 4
                    {
                        1
                    } else {
                        2
                    })
                    .min(100);
            }
            self.next_needs = self.now_ms + 10 * MINUTE;
        }
        for (i, plot) in self.plots.iter_mut().enumerate() {
            if plot.due == self.now_ms && plot.due != 0 {
                if self
                    .village
                    .as_ref()
                    .is_some_and(|state| state.flooded_plots & (1 << i) != 0)
                {
                    plot.due += 30 * MINUTE;
                    continue;
                }
                plot.due = 0;
                plot.stage = match plot.stage {
                    PlotStage::Seed => PlotStage::Growing,
                    PlotStage::Growing => PlotStage::Mature,
                    other => other,
                };
                plot.watered = false;
            }
        }
    }

    fn market_handoff_cells_in_use(&self) -> bool {
        if let Some(state) = &self.village {
            let mut candidate = state.clone();
            candidate.apply_development(self.story.finished.saturating_add(1), false);
            return !self.can_commit_phases(candidate.phases());
        }
        self.story.finished == 8
            && self.actors.iter().any(|actor| {
                std::iter::once(actor.cell)
                    .chain(actor.route[actor.route_next..].iter().copied())
                    .any(|cell| cell.1 == 33 && matches!(cell.0, 18 | 20))
            })
    }

    /// Rest breaks can be shared with outside activities. Production steps retain their actor.
    pub fn activity_available(&self, kind: ActorKind) -> bool {
        let actor = &self.actors[kind.index()];
        !actor.outside
            && actor.carried.is_none()
            && actor.tool.is_none()
            && actor.hunger < 72
            && actor.thirst < 78
            && actor.stamina >= 38
            && (actor.stage == Stage::Idle
                || (actor.task == Some(Task::Rest) && actor.stage == Stage::Work))
    }

    pub fn reserve_activity(&mut self, kinds: &[ActorKind]) -> bool {
        let mut selected = [false; 3];
        for &kind in kinds {
            if selected[kind.index()] || !self.activity_available(kind) {
                return false;
            }
            selected[kind.index()] = true;
        }
        for &kind in kinds {
            self.hold_for_outing(kind);
        }
        true
    }

    /// Market coordination can take a safe stationary gap between farm jobs.
    /// Work with consumed inputs, tools, escrow, or an in-flight edge retains priority.
    pub fn reserve_market_trip(&mut self, kind: ActorKind) -> bool {
        if !self.is_village() || kind == ActorKind::Orange {
            return false;
        }
        let actor = &self.actors[kind.index()];
        if actor.outside
            || actor.inside
            || actor.carried.is_some()
            || actor.moving_box
            || actor.hunger >= 65
            || actor.thirst >= 71
            || actor.stamina < 38
            || self.meal_until != 0
            || !matches!(
                actor.stage,
                Stage::Idle | Stage::Finish | Stage::Prepare | Stage::Work
            )
            || matches!(actor.stage, Stage::Prepare | Stage::Work) && actor.task != Some(Task::Rest)
        {
            return false;
        }
        if actor.tool.is_some() {
            // Take a normal gap to physically return the tool before accepting
            // a market parcel. Market coordination can retry after arrival.
            self.begin(kind.index(), Task::ReturnTool);
            return false;
        }
        self.hold_for_outing(kind);
        true
    }

    fn hold_for_outing(&mut self, kind: ActorKind) {
        let i = kind.index();
        self.idle(i, 5 * MINUTE);
        self.actors[i].outside = true;
        self.actors[i].activity = Activity::OutsideActivity;
    }

    pub fn move_for_activity(&mut self, kind: ActorKind, point: Point) -> bool {
        self.move_for_activity_with_load(kind, point, false)
    }

    /// The external activity keeps ownership of its own parcel throughout travel.
    pub fn move_for_activity_with_load(&mut self, kind: ActorKind, point: Point, loaded: bool) -> bool {
        let Some(target) = Cell::at(point) else {
            return false;
        };
        let i = kind.index();
        if !self.actors[i].outside || self.actors[i].stage != Stage::Idle {
            return false;
        }
        if !self.begin(i, Task::External(target)) {
            return false;
        }
        self.actors[i].external_load = self.is_village() && loaded;
        true
    }

    pub fn can_reach_for_activity(&self, kind: ActorKind, point: Point) -> bool {
        Cell::at(point)
            .is_some_and(|target| self.can_reach_static(self.actors[kind.index()].cell, target))
    }

    pub fn activity_deadline(&self, kind: ActorKind) -> Option<u64> {
        let actor = &self.actors[kind.index()];
        actor.outside.then_some(actor.until)
    }

    pub fn activity_arrived(&self, kind: ActorKind) -> bool {
        let actor = &self.actors[kind.index()];
        actor.outside && actor.stage == Stage::Idle
    }

    pub fn end_activity(&mut self, kinds: &[ActorKind], effort: u8) {
        let village = self.is_village();
        for &kind in kinds {
            let i = kind.index();
            let actor = &mut self.actors[i];
            if !actor.outside {
                continue;
            }
            actor.outside = false;
            actor.stamina = actor.stamina.saturating_sub(effort.min(25));
            if actor.stage == Stage::Idle {
                self.idle(i, 0);
            } else if matches!(actor.task, Some(Task::External(_))) {
                actor.route.truncate(actor.route_next + 1);
                actor.route_goal = None;
                if village && (actor.waiting_edge || actor.route.is_empty()) {
                    self.idle(i, MINUTE);
                }
            }
        }
    }

    pub fn recover_activity(&mut self, kinds: &[ActorKind]) {
        self.end_activity(kinds, 0);
    }

    pub fn external_hold(&mut self, actor: usize, holding: bool) -> bool {
        let Some(kind) = self.actors.get(actor).map(|actor| actor.kind) else {
            return false;
        };
        if holding {
            return self.actors[actor].outside || self.reserve_activity(&[kind]);
        }
        self.end_activity(&[kind], 5);
        true
    }

    pub fn external_move(&mut self, actor: usize, target: Point) -> bool {
        self.actors.get(actor).is_some_and(|actor| actor.outside)
            && self.move_for_activity(self.actors[actor].kind, target)
    }

    pub fn external_arrived(&self, actor: usize) -> bool {
        self.actors
            .get(actor)
            .is_some_and(|actor| self.activity_arrived(actor.kind))
    }

    pub fn external_pose(&mut self, actor: usize, pose: Pose) -> bool {
        let Some(actor) = self.actors.get_mut(actor) else {
            return false;
        };
        if !actor.outside || actor.stage != Stage::Idle {
            return false;
        }
        actor.pose = pose;
        true
    }

    pub fn spend_many(&mut self, costs: &[(Item, u16)]) -> bool {
        self.spend_stock_bundle(costs)
    }

    fn plan(&mut self, i: usize) {
        let actor = &self.actors[i];
        if actor.outside {
            self.actors[i].until = self.now_ms + 5 * MINUTE;
            return;
        }
        let hour = self.minute_of_day() / 60;
        let sleeping_hours = hour >= 21 || hour < 6;
        let outfit = self.outfit_for(i, sleeping_hours);
        let food = self.stock.count(Item::Food);
        let water = self.stock.count(Item::Water);
        let prepare_neighbour_meals = self.neighbour_meals_needed() && (8..20).contains(&hour);
        let pantry_needs_water = self.village.as_ref().is_some_and(|state| {
            !state.pantry_plan.provisioned && state.pantry_plan.last_day.is_some()
        });
        let replenish_trade_reserve = self.trade_reserve_pending()
            && self.stock.count(Item::Crops) < 11 && actor.hunger < 75;
        let courtyard_pending = self.village.as_ref().is_some_and(|state| {
            state.pantry_plan.completed("Q16") && state.renovation.path == 4
                && !state.renovation.completed("Q35")
        });
        let review_due = |need| self.village.as_ref().is_some_and(|state| {
            let opening = &state.opening;
            opening.review_day.is_some() && opening.stable_days < 7
                && opening.daily_needs & (1u8 << (i * 3 + need)) == 0
                || self.season() == YardSeason::Winter && state.pantry_plan.last_day.is_some() && !state.pantry_plan.ready
                    && (need == 1 && state.pantry_plan.food_days & (1 << i) == 0
                        || need == 2 && actor.thirst >= 48)
        });
        let daytime_sleep = i < 2
            && (6..10).contains(&hour)
            && self.village.as_ref().is_some_and(|state| {
                (state.opening.review_day.is_some() && state.opening.stable_days < 7
                    && state.opening.daily_needs & (1 << (i * 3)) == 0)
                    || (self.season() == YardSeason::Winter && state.pantry_plan.last_day.is_some()
                        && !state.pantry_plan.ready && state.pantry_plan.nights_covered & (1 << i) == 0)
            });
        let paused_drying = actor.harvest_processing == 3 && actor.carried == Some(Item::Crops)
            && {
                let drying_end = self.now_ms / DAY_MS * DAY_MS + 18 * HOUR;
                let drying_travel = (u64::from(actor.cell.distance(self.site(i, Task::ProcessHarvest(3))))
                    + 8) * MINUTE;
                !(6..18).contains(&hour)
                    || self.now_ms + drying_travel + Task::ProcessHarvest(3).duration() >= drying_end
                    || actor.stamina < 22
                    || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            };
        let finish_preserve = (i == 0 && (7..20).contains(&hour)
            && actor.carried.is_none() && actor.stamina >= 20 && actor.thirst <= 85)
            .then(|| self.village.as_ref().and_then(|state| {
                let rack = self.story.improvements.working(Fixture::PantryDryRack);
                state.preserves.work(self.now_ms, rack).filter(|work| {
                    matches!(work, preserves::Work::TurnRack | preserves::Work::JarBatch | preserves::Work::CookBatch)
                })
            })).flatten();
        let urgent_thirst = self.is_village() && i < 2 && actor.thirst >= 85;
        let finish_move = self.village.as_ref().and_then(|state| {
            state.renovation.task(i, self.now_ms)
                .filter(|work| matches!(work, renovation::Work::MoveBack(_))
                    || matches!(work, renovation::Work::Repair(_))
                        && !matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
                        && self.tool_available(i, Tool::Hammer)
                        && self.stock.count(state.renovation.repair_cost().0) > 0)
                .filter(|_| actor.carried.is_none() && (8..19).contains(&hour))
                .map(Task::Renovate)
        });
        let timely_grain = if i == ActorKind::Xiaohe.index() && actor.carried.is_none()
            && actor.thirst <= 80 && actor.hunger <= 80 && !sleeping_hours
        {
            self.grain_task(i).filter(|task| matches!(task, Task::Water(grain::FIELD)))
        } else { None };
        let evening_walk = (i == 0 && actor.carried.is_none() && actor.stamina > 45
            && (18..20).contains(&hour) && self.current_market_work(i) == Some(road_story::MarketWork::NightWalk))
            .then_some(Task::MarketStory(road_story::MarketWork::NightWalk));
        let pending_gather = if i < 2 && !sleeping_hours && actor.carried.is_none() {
            self.village.as_ref().and_then(|state| {
                if state.memories.gathering.is_some() { Some(Task::Memory(memories::Work::Gather)) }
                else if (14..18).contains(&hour) && state.memories.pages_reviewed == 0b11 && !state.memories.completed("Q70") {
                    Some(Task::Memory(memories::Work::ReadTogether(i as u8)))
                } else { None }
            })
        } else { None };
        let reliable_infra_shift = (self.is_village()
            && i == ActorKind::Ayu.index()
            && (7..15).contains(&hour)
            && !sleeping_hours
            && actor.carried.is_none()
            && actor.hunger < 60
            && actor.thirst < 68)
            .then(|| self.development_task(i))
            .flatten();
        let public_work = if i < 2 && actor.carried.is_none() && reliable_infra_shift.is_none() {
            let road_work = (actor.hunger <= 70 && actor.thirst <= 70)
                .then(|| self.road_story_task(i)).flatten();
            road_work.or_else(|| self.market_story_task(i))
        } else {None};
        let essential = if let Some(task) = self.committed_road_action(i).or(finish_move)
            .or(evening_walk).or(pending_gather).or(public_work).or(timely_grain) {
            Some(task)
        } else if let Some(work) = finish_preserve {
            Some(Task::Preserve(work))
        } else if paused_drying && actor.thirst >= 82 && water > 0 {
            Some(Task::Drink)
        } else if paused_drying && self.well > 0 && (actor.thirst >= 75 || review_due(2)) {
            Some(Task::DrinkAtWell)
        } else if paused_drying && food > 0 && (actor.hunger >= 70 || review_due(1)) {
            Some(Task::EatWhileDrying)
        } else if paused_drying && actor.thirst >= 65 && water > 0 {
            Some(Task::Drink)
        } else if paused_drying && sleeping_hours && (hour >= 21 || hour < 3) {
            // Leave the numbered batch in its basket overnight and resume
            // drying the same produce after sunrise.
            Some(Task::Sleep)
        } else if paused_drying {
            Some(Task::Rest)
        } else if actor.harvest_processing > 0 {
            Some(Task::ProcessHarvest(actor.harvest_processing))
        } else if self.village.as_ref().is_some_and(|state| {
            state.opening.harvest_carrier == Some(i as u8) && actor.carried == Some(Item::Crops)
        }) {
            Some(Task::CookHomeHarvest)
        } else if actor.traced_wood.is_some() {
            Some(Task::WorkshopStory(workshop_story::Work::MillBeam))
        } else if actor.carried.is_some() {
            if actor.carried == Some(Item::Water)
                && self.bowl < BOWL_CAP
                && actor.kind != ActorKind::Orange
            {
                Some(Task::FillBowl)
            } else {
                Some(Task::Store)
            }
        } else if self.is_village()
            && i < 2
            && !self.in_household_region(actor.cell)
            && (sleeping_hours || actor.stamina < 30)
        {
            Some(Task::ReturnHome)
        } else if i == ActorKind::Ayu.index() && self.village.as_ref().is_some_and(|state| {
            let due = state.opening.late_meal_until;
            let until_ready = due.saturating_sub(3 * HOUR).saturating_add(20 * MINUTE);
            self.now_ms >= until_ready && self.now_ms + 10 * MINUTE < due
        }) {
            Some(Task::EatSavedPortion)
        } else if urgent_thirst && water > 0 {
            Some(Task::Drink)
        } else if urgent_thirst && self.well > 0 {
            Some(Task::DrinkAtWell)
        } else if i < 2 && self.is_village() && food == 0 && actor.hunger >= 80
            && !self.task_in_use(Task::Cook)
            && (self.stock.count(Item::Crops) >= 2 || self.stock.count(Item::Fish) > 0)
        {
            Some(Task::Cook)
        } else if actor.kind == ActorKind::Orange
            && self.village.as_ref().is_some_and(|state| state.opening.cat_food)
        {
            Some(Task::CatEat)
        } else if food > 0 && (i < 2 || !self.is_village())
            && (actor.hunger >= 57 || i < 2 && (11..18).contains(&hour) && review_due(1))
        {
            Some(Task::Eat)
        } else if actor.kind == ActorKind::Orange && actor.thirst >= 52 && self.bowl > 0 {
            Some(Task::Drink)
        } else if i < 2 && (8..20).contains(&hour) && review_due(2) && (water > 0 || self.well > 0) {
            // During the first week's review, make a deliberate water stop
            // before afternoon work and record the portion actually consumed.
            Some(if water > 0 { Task::Drink } else { Task::DrinkAtWell })
        } else if i < 2 && water > 0 && self.village.as_ref()
            .is_some_and(|state| state.opening.needs_break_water(i, self.now_ms))
        {
            Some(Task::Drink)
        } else if actor.kind != ActorKind::Orange && actor.thirst >= 67 && water > 0 {
            Some(Task::Drink)
        } else if actor.kind != ActorKind::Orange && actor.thirst >= 67 && self.well > 0 {
            Some(Task::DrinkAtWell)
        } else if i < 2 && hour < 20 && review_due(1) && food == 0
            && !self.task_in_use(Task::Cook)
            && (self.stock.count(Item::Crops) >= 2 || self.stock.count(Item::Fish) > 0)
        {
            Some(Task::Cook)
        } else if actor.tool.is_some() && (sleeping_hours || actor.stamina < 30) {
            Some(Task::ReturnTool)
        } else if i < 2 && actor.outfit != outfit && actor.tool.is_none() {
            Some(Task::ChangeClothes(outfit))
        } else if sleeping_hours || daytime_sleep {
            Some(Task::Sleep)
        } else if let Some(task) = self.opening_task(i) {
            Some(task)
        } else if actor.stamina < 30 {
            Some(Task::Rest)
        } else if self.mature_cat_meal_needed(i) {
            Some(Task::ServeCat)
        } else if actor.kind == ActorKind::Orange {
            let rests_midday = (11..14).contains(&hour);
            let bad_weather = matches!(self.weather(), YardWeather::Rain | YardWeather::Snow);
            let goal = self.cat_spot(usize::from(self.next_cat_spot));
            if actor.stamina >= 65 && !rests_midday && !bad_weather && actor.cell != goal {
                Some(Task::Explore(self.next_cat_spot))
            } else {
                Some(Task::Rest)
            }
        } else if self.bowl == 0 && water > 0 && !self.task_in_use(Task::FillBowl) {
            Some(Task::FillBowl)
        } else if courtyard_pending
            && let Some(task @ Task::Renovate(renovation::Work::PrepareGuests | renovation::Work::CloseGuestTrial)) =
                self.renovation_task(i)
        {
            // Use the actual guest portions while the pantry has enough.
            // Ordinary cooking should not outbid a ready table trial.
            Some(task)
        } else if self.village.as_ref().is_some_and(|state| state.grain.needs_leaf_first(self.now_ms))
            && self.stock.count(Item::Crops) >= 2 && self.stock.fits(Item::Food, 5)
            && !self.task_in_use(Task::Cook)
        {
            Some(Task::Cook)
        } else if self.village.as_ref().is_some_and(|state| {
            state.grain.should_cook_root(self.stock.count(Item::Crops), self.now_ms)
        }) && self.stock.fits(Item::Food, 3) && !self.task_in_use(Task::Grain(grain::Work::CookRoot))
        {
            Some(Task::Grain(grain::Work::CookRoot))
        } else if (food <= 7 || prepare_neighbour_meals)
            && !self.task_in_use(Task::Cook)
            && (self.stock.count(Item::Crops) >= 2 || self.stock.count(Item::Fish) > 0)
        {
            Some(Task::Cook)
        } else if i == ActorKind::Ayu.index() && self.village.as_ref().is_some_and(|state| {
            state.opening.completed("Q03") && (state.soil.aims_to_fish()
                || self.season() == YardSeason::Summer && state.pantry_plan.needs_season_event(YardSeason::Summer))
        }) && (9..13).contains(&hour) && self.fish_in_pond > 2 && food >= 5
            && !matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            && self.tool_available(i, Tool::FishingRod) && !self.task_in_use(Task::Fish)
        {
            Some(Task::Fish)
        } else if (courtyard_pending && food < 8
            || prepare_neighbour_meals && self.stock.count(Item::Crops) < 2)
            && i == ActorKind::Ayu.index()
            && self.stock.count(Item::Fish) == 0 && (8..15).contains(&hour)
            && self.fish_in_pond > 2 && self.tool_available(i, Tool::FishingRod)
            && !self.task_in_use(Task::Fish)
            && !matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            Some(Task::Fish)
        } else if water < (if pantry_needs_water { 9 } else { 2 })
            && self.well > 0 && !self.water_collection_in_use()
            && self.tool_available(i, Tool::WateringCan)
        {
            Some(self.water_collection_task(i))
        } else if let Some(task) = reliable_infra_shift {
            Some(task)
        } else if courtyard_pending && let Some(task) = self.renovation_task(i) {
            Some(task)
        } else if replenish_trade_reserve && (6..18).contains(&hour) {
            // Book regular farming while the household works up an actual
            // saleable surplus. All needs and already-paid shifts come first.
            self.farm_task(i)
        } else if i == ActorKind::Xiaohe.index() && (6..18).contains(&hour)
            && (self.market_crop_shortfall() && food >= 3 && actor.hunger < 75
                || prepare_neighbour_meals && self.stock.count(Item::Crops) < 5)
            && actor.thirst < 75
            && let Some(task) = self.farm_task(i)
        {
            Some(task)
        } else {
            None
        };
        if let Some(task) = essential {
            if self.begin(i, self.return_before_replacing_tool(i, task)) {
                return;
            }
        }

        let task = match self.actors[i].kind {
            ActorKind::Orange => Task::Rest,
            _ if (15..18).contains(&hour) => Task::Rest,
            ActorKind::Xiaohe => self
                .soil_task(i)
                .or_else(|| self.road_story_task(i))
                .or_else(|| self.market_story_task(i))
                .or_else(|| self.memory_task(i))
                .or_else(|| self.renovation_task(i))
                .or_else(|| self.workshop_order(i))
                .or_else(|| self.preserve_task(i))
                .or_else(|| self.mill_task(i))
                .or_else(|| self.grain_task(i))
                .or_else(|| self.housing_task(i))
                .or_else(|| self.orchard_task(i))
                .or_else(|| self.farm_task(i))
                .or_else(|| self.support_task(i))
                .or_else(|| self.trade_task(i))
                .or_else(|| self.garden_task(i))
                .unwrap_or(Task::Rest),
            ActorKind::Ayu => self
                .paid_development_shift(i)
                .or_else(|| self.road_story_task(i))
                .or_else(|| self.market_story_task(i))
                .or_else(|| self.memory_task(i))
                .or_else(|| self.workshop_story_task(i))
                .or_else(|| self.renovation_task(i))
                .or_else(|| self.workshop_order(i))
                .or_else(|| self.preserve_rack_order(i))
                .or_else(|| self.mill_task(i))
                .or_else(|| self.build_task(i))
                .or_else(|| self.housing_task(i))
                .or_else(|| self.advanced_build_task(i))
                .or_else(|| self.improvement_task(i))
                .or_else(|| self.development_task(i))
                .or_else(|| self.trade_task(i))
                .or_else(|| self.support_task(i))
                .or_else(|| self.farm_task(i))
                .or_else(|| self.garden_task(i))
                .unwrap_or(Task::Rest),
        };
        let task = if task == Task::Rest && self.actors[i].tool.is_some() {
            Task::ReturnTool
        } else {
            self.return_before_replacing_tool(i, task)
        };
        if !self.begin(i, task) && !self.begin(i, Task::Rest) {
            self.idle(i, 5 * MINUTE);
        }
    }

    fn return_before_replacing_tool(&self, i: usize, task: Task) -> Task {
        let active_rod = self.village.as_ref().is_some_and(|state| state.opening.rod_on_rack);
        let held = self.actors[i].tool;
        if task == Task::RackRod && held.is_some() && held != Some(Tool::FishingRod)
            || active_rod && held == Some(Tool::FishingRod)
                && task.tool().is_some_and(|tool| tool != Tool::FishingRod)
        {
            Task::ReturnTool
        } else {
            task
        }
    }

    fn opening_task(&self, i: usize) -> Option<Task> {
        let opening = &self.village.as_ref()?.opening;
        if i < 2 && opening.boxes_stowed & (1 << i) == 0 {
            return Some(Task::UnpackHome);
        }
        if !opening.completed("Q01") {
            return None;
        }
        if opening.completed("Q03") && !opening.parallel_work
            && (8..14).contains(&(self.minute_of_day() / 60))
            && self.actors[i].stamina >= 35
        {
            if i == ActorKind::Xiaohe.index() && self.stock.count(Item::Water) > 0
                && self.tool_available(i, Tool::WateringCan)
                && let Some((plot, _)) = self.plots.iter().take(6).enumerate()
                    .filter(|(plot, bed)| {
                        matches!(bed.stage, PlotStage::Seed | PlotStage::Growing)
                            && !bed.watered && !self.task_in_use(Task::Water(*plot as u8))
                    })
                    .min_by_key(|(plot, _)| self.actors[i].cell.distance(self.site(i, Task::Water(*plot as u8))))
            {
                return Some(Task::Water(plot as u8));
            }
            if i == ActorKind::Ayu.index() && self.stock.count(Item::Crops) >= 2
                && self.stock.count(Item::Food) <= 13 && !self.task_in_use(Task::Cook)
                && self.plots.iter().take(6).any(|bed| {
                    matches!(bed.stage, PlotStage::Seed | PlotStage::Growing) && !bed.watered
                })
            {
                return Some(Task::Cook);
            }
        }
        if i < 2 && opening.completed("Q05") && opening.completed("Q04") && !opening.shared_rest
            && (13..18).contains(&(self.minute_of_day() / 60)) && self.actors[i].stamina < 81
            && (opening.rest_completed[i] == 0 || self.now_ms - opening.rest_completed[i] > HOUR)
            && self.stock.count(Item::Water) >= 2
        {
            return Some(if self.actors[i].tool.is_some() { Task::ReturnTool } else { Task::CourtyardBreak });
        }
        if i == ActorKind::Ayu.index()
            && !opening.rod_on_rack && self.tool_available(i, Tool::FishingRod)
        {
            return Some(Task::RackRod);
        }
        if i < 2 && !opening.cat_food && !self.task_in_use(Task::ServeCat)
            && if opening.cat_ate {
                self.actors[ActorKind::Orange.index()].hunger >= 50 && self.stock.count(Item::Food) >= 5
            } else {
                i == ActorKind::Xiaohe.index() && self.stock.count(Item::Food) >= 3
            }
        {
            return Some(Task::ServeCat);
        }
        // The rack has to survive a full pickup, use and return. Borrow it for
        // a bounded normal visit to the existing pond when the water is safe.
        if i < 2 && opening.rod_on_rack && !opening.rod_returned {
            if self.actors[i].tool == Some(Tool::FishingRod) && opening.rod_used {
                return Some(Task::ReturnTool);
            }
            if i == ActorKind::Ayu.index() && self.tool_available(i, Tool::FishingRod)
                && !matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
                && self.fish_in_pond > 2
            {
                return Some(Task::Fish);
            }
        }
        None
    }

    fn mature_cat_meal_needed(&self, i: usize) -> bool {
        // The usual bowl visit remains part of learning the new household's
        // routine. In an established town, a hungry cat can use a spare meal
        // even when the pantry is between cooking shifts.
        (i < 2 && self.town_era() >= TownEra::PublicDistrict)
            && self.actors[i].hunger < 57 && self.actors[i].thirst < 67
            && self.actors[ActorKind::Orange.index()].hunger >= 90
            && self.stock.count(Item::Food) > 0
            && self.village.as_ref().is_some_and(|state| {
                state.opening.completed("Q08") && !state.opening.cat_food
            })
            && !self.task_in_use(Task::ServeCat)
    }

    fn grain_task(&self, i: usize) -> Option<Task> {
        let village = self.village.as_ref()?;
        if !village.opening.completed("Q02") || self.actors[i].stamina < 30
            || !(7..19).contains(&(self.minute_of_day() / 60))
            || !self.phase_mask().contains(WILLOW_VILLAGE.crops[usize::from(grain::FIELD)].phase)
        {
            return None;
        }
        let id = grain::FIELD;
        let field = self.plots[usize::from(id)].stage;
        let task = if village.flooded_plots & (1 << id) != 0 {
            Task::Drain(id)
        } else {
            village.grain.task(self.minute_of_day() / 60, self.now_ms / DAY_MS, self.weather(),
                field, village.opening.completed("Q03"), village.soil.completed("Q19"))?
        };
        (!self.task_in_use(task)
            && task.tool().is_none_or(|tool| self.tool_available(i, tool))
            && self.input(task).is_none_or(|(item, units)| self.stock.count(item) >= units))
            .then_some(task)
    }

    fn paid_development_shift(&self, i: usize) -> Option<Task> {
        (self.is_village() && self.story.running.is_some()).then(|| self.development_task(i)).flatten()
    }

    fn mill_task(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        if !self.ongoing_town_work(TownEra::Workshop, state.grain.mill_installed(), 1)
            || !state.workshop_story.completed("Q20") || self.actors[i].stamina < 44
            || !(8..17).contains(&(self.minute_of_day() / 60))
        {
            return None;
        }
        let instructor = &self.actors[ActorKind::Ayu.index()];
        let lesson = instructor.task == Some(Task::Grain(grain::Work::DemonstrateMill))
            && instructor.stage == Stage::Work;
        let work = state.grain.mill_work(i, lesson)?;
        if work == grain::Work::ObserveMill && i == 0
            && self.actors[i].cell.distance(self.building_door(4)) > 24
        {
            return None;
        }
        let task = Task::Grain(work);
        if work == grain::Work::MountMill && !self.mill_can_use_stone()
            || matches!(work, grain::Work::CookFlour | grain::Work::CookTraineeFlour) && !self.stock.fits(Item::Food, 3)
        {
            return None;
        }
        (!self.task_in_use(task) && task.tool().is_none_or(|tool| self.tool_available(i, tool))
            && self.input(task).is_none_or(|(item, qty)| self.stock.count(item) >= qty)).then_some(task)
    }

    fn preserve_task(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        if !self.ongoing_town_work(TownEra::Workshop, state.preserves.journey_started(), 2)
            || !state.grain.completed("Q14") || !state.workshop_story.completed("Q24")
            || self.actors[i].stamina < 40 || self.stock.count(Item::Food) < 6
            || !(8..17).contains(&(self.minute_of_day() / 60))
        {
            return None;
        }
        let work = state.preserves.work(self.now_ms, self.story.improvements.working(Fixture::PantryDryRack))?;
        if work == preserves::Work::LoadRack && self.stock.count(Item::Crops) < 3 + preserves::CROP_UNITS
            || work == preserves::Work::CookBatch && !self.stock.fits(Item::Food, 4)
        {
            return None;
        }
        Some(Task::Preserve(work))
    }

    fn preserve_rack_order(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        let order = ImprovementOrder {
            fixture: Fixture::PantryDryRack,
            phase: u8::from(self.story.improvements.active()
                .is_some_and(|order| order.fixture == Fixture::PantryDryRack)),
        };
        if self.story.improvements.active().is_none()
            && !self.ongoing_town_work(TownEra::Workshop, state.preserves.journey_started(), 2)
            || !state.grain.completed("Q14") || !state.workshop_story.completed("Q24")
            || !state.preserves.wants_rack() || self.story.improvements.working(order.fixture)
            || self.actors[i].stamina < 45 || self.stock.count(Item::Food) < 6
            || !self.improvement_unlocked(order.fixture) || !self.tool_available(i, Tool::Hammer)
            || self.buildings[usize::from(order.fixture.site())].step > 0
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            return None;
        }
        let repair = Task::Preserve(preserves::Work::RepairRack);
        if self.story.improvements.installed().any(|status| status.fixture == order.fixture)
            && self.story.improvements.active().is_none() && self.stock.count(Item::Wood) >= 1
            && !self.task_in_use(repair)
        {
            return Some(repair);
        }
        if !self.story.improvements.accepts(order, self.now_ms) || self.task_in_use(Task::Improve(order)) {
            return None;
        }
        let (item, qty) = order.material();
        (self.stock.count(chapter_item(item)) >= qty).then_some(Task::Improve(order))
    }

    fn preserve_requires_storehouse(&self) -> bool {
        self.buildings[3].level < 2 && self.village.as_ref().is_some_and(|state| {
            state.grain.completed("Q14") && state.workshop_story.completed("Q24") && state.preserves.wants_rack()
        })
    }

    fn soil_task(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        if self.actors[i].stamina < 38 || !(7..17).contains(&(self.minute_of_day() / 60)) {
            return None;
        }
        if let Some(id) = state.soil.inspected_plot()
            && !self.plots[usize::from(id)].watered && self.stock.count(Item::Water) != 0
            && matches!(self.plots[usize::from(id)].stage, PlotStage::Seed | PlotStage::Growing)
            && self.tool_available(i, Tool::WateringCan)
            && !self.task_in_use(Task::Water(id))
        {
            return Some(Task::Water(id));
        }
        let available = WILLOW_VILLAGE.crops.iter().enumerate().fold(0u32, |mask, (id, plot)| {
            if self.phase_mask().contains(plot.phase) { mask | (1 << id) } else { mask }
        });
        state.soil.task(self.now_ms, &self.plots, state.flooded_plots, available,
            state.opening.completed("Q02"), state.opening.completed("Q03"))
            .map(Task::Soil).filter(|&task| !self.task_in_use(task))
    }

    fn workshop_story_task(&self, i: usize) -> Option<Task> {
        use workshop_story::Work;
        let state = self.village.as_ref()?;
        let progress = &state.workshop_story;
        if !state.opening.completed("Q05") || self.actors[i].stamina < 45
            || self.stock.count(Item::Food) < 5 || !(7..16).contains(&(self.minute_of_day() / 60))
        {
            return None;
        }
        let outdoors = !matches!(self.weather(), YardWeather::Rain | YardWeather::Snow);
        let task = if progress.rod_wear >= 2 && !progress.rod_oiled {
            Task::WorkshopStory(Work::OilRod)
        } else if progress.rod_oiled && !progress.rod_rehung && outdoors && self.fish_in_pond > 2 {
            if progress.rod_tested && self.actors[i].tool == Some(Tool::FishingRod) { Task::ReturnTool } else { Task::Fish }
        } else if let Some(task) = progress.priority() {
            task
        } else if let Some(tree) = progress.tree.filter(|_| !progress.timber) {
            match self.trees[usize::from(tree)].stage {
                TreeStage::Standing if outdoors => Task::FellTree(tree),
                TreeStage::Felled => Task::TakeTimber(tree),
                _ => return None,
            }
        } else if progress.tree.is_none() && outdoors && self.working_trees() >= 4 {
            Task::WorkshopStory(Work::ChooseTree)
        } else {
            return None;
        };
        (!self.task_in_use(task) && task.tool().is_none_or(|tool| self.tool_available(i, tool))).then_some(task)
    }

    fn working_trees(&self) -> usize {
        self.trees.iter().filter(|tree| !tree.protected && tree.stage == TreeStage::Standing).count()
    }

    fn workshop_order(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        if !self.town_allows(TownEra::Workshop) && state.workshop_story.order.is_none()
            || !state.preserves.completed("Q26") || !state.preserves.completed("Q27")
            || !state.grain.completed("Q28") || self.actors[i].stamina < 42
            || self.stock.count(Item::Food) < 6 || !(8..18).contains(&(self.minute_of_day() / 60))
        {
            return None;
        }
        let work = state.workshop_story.order_work(i, self.now_ms)?;
        let task = Task::WorkshopStory(work);
        (task.tool().is_none_or(|tool| self.tool_available(i, tool))
            && self.input(task).is_none_or(|(item, qty)| self.stock.count(item) >= qty)).then_some(task)
    }

    fn renovation_task(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        if !self.ongoing_town_work(TownEra::Workshop, state.renovation.progress_started(), 4)
            || !state.opening.completed("Q08") || !state.workshop_story.completed("Q20")
            || !(8..17).contains(&(self.minute_of_day() / 60))
        {
            return None;
        }
        let work = state.renovation.task(i, self.now_ms).or_else(|| self.common_renovation_task(i))?;
        let actor = &self.actors[i];
        let can_use_corner = match work {
            renovation::Work::RestAtCorner => actor.hunger < 75 && actor.thirst < 75,
            renovation::Work::BuildCorner => actor.hunger < 70 && actor.thirst < 70 && actor.stamina >= 47,
            renovation::Work::PrepareGuests => actor.hunger < 75 && actor.thirst < 75
                && actor.stamina >= 47 && self.stock.count(Item::Food) >= 7,
            renovation::Work::CloseGuestTrial => actor.hunger < 75 && actor.thirst < 75
                && actor.stamina >= 47 && self.stock_can_accept(Item::Food, 2),
            _ => actor.stamina >= 47 && self.stock.count(Item::Food) >= 7,
        };
        if !can_use_corner {
            return None;
        }
        if matches!(work, renovation::Work::MoveOut(_))
            && self.actors.iter().any(|actor| {
                matches!(actor.task, Some(Task::BuildHome(order)) if order.phase == HousingPhase::MoveIn)
            })
        {
            return None;
        }
        if matches!(work, renovation::Work::Repair(_) | renovation::Work::LayPath | renovation::Work::BuildCorner)
            && matches!(self.weather(), YardWeather::Rain | YardWeather::Snow) {
            return None;
        }
        let task = Task::Renovate(work);
        (task.tool().is_none_or(|tool| self.tool_available(i, tool))
            && self.input(task).is_none_or(|(item, amount)| self.stock.count(item) >= amount)
            && self.valid_world_cell(self.temporary_home_door())).then_some(task)
    }

    fn common_renovation_task(&self, i: usize) -> Option<renovation::Work> {
        let state = self.village.as_ref()?;
        state.renovation.common_task(i,self.now_ms,state.soil.completed("Q18"), state.opening.completed("Q07"),
            self.village_chapter_completed("Q23") && state.pantry_plan.completed("Q16"))
    }

    fn road_story_task(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        if !self.road_town_permit() || !state.renovation.completed("Q35") || self.actors[i].stamina < 38
            || !(8..18).contains(&(self.minute_of_day() / 60)) {
            return None;
        }
        let work = state.road_story.task(i,self.now_ms)?;
        if work == road_story::Work::Approve && !state.workshop_story.completed("Q20")
            || work == road_story::Work::PackOrder && !state.workshop_story.completed("Q29")
            || matches!(work,road_story::Work::Build | road_story::Work::CarryTest)
                && matches!(self.weather(),YardWeather::Rain|YardWeather::Snow) { return None; }
        let task=Task::RoadStory(work);
        (task.tool().is_none_or(|tool| self.tool_available(i,tool))
            && self.input(task).is_none_or(|(item,qty)| self.stock.count(item)>=qty)).then_some(task)
    }

    fn committed_road_action(&self, i: usize) -> Option<Task> {
        let state = &self.village.as_ref()?.road_story;
        state.loaded?;
        state.task(i,self.now_ms).map(Task::RoadStory)
    }

    fn road_town_permit(&self) -> bool {
        let Some(state) = &self.village else { return false; };
        self.ongoing_town_work(TownEra::Workshop, state.road_story.stage > 0, 8)
    }

    fn current_market_work(&self, i: usize) -> Option<road_story::MarketWork> {
        let state = self.village.as_ref()?;
        if !state.road_story.completed("Q42") { return None; }
        if !state.calendar.as_ref().is_some_and(|calendar| calendar.permits_public_work(state.market_story.stage, self.now_ms)) {
            return None;
        }
        let cleanup = state.social.market_cleanup.then_some(state.social.last_market_cleanup_day).flatten();
        let mill_ready = self.resolve_poi("craft_carpentry.door").is_some() && state.grain.can_share_mill();
        let task = state.market_story.task(i,self.now_ms,state.social.market_after_review,
            state.social.fixed_market,cleanup,mill_ready)?;
        if task == road_story::MarketWork::Drain && !state.soil.completed("Q18")
            || task == road_story::MarketWork::Base && !state.workshop_story.completed("Q20") { return None; }
        Some(task)
    }

    fn market_story_task(&self, i: usize) -> Option<Task> {
        let work = self.current_market_work(i)?;
        let hour = self.minute_of_day() / 60;
        if self.actors[i].stamina < 38 || !(8..20).contains(&hour)
            || matches!(work,road_story::MarketWork::Base | road_story::MarketWork::Pave | road_story::MarketWork::Drain)
                && matches!(self.weather(),YardWeather::Rain|YardWeather::Snow) { return None; }
        let task = Task::MarketStory(work);
        (task.tool().is_none_or(|tool|self.tool_available(i,tool))
            && self.input(task).is_none_or(|(item,qty)|self.stock.count(item)>=qty)).then_some(task)
    }

    fn memory_task(&self,i:usize)->Option<Task> {
        let state=self.village.as_ref()?;
        if !self.village_chapter_completed("Q23") || !state.renovation.completed("Q34")
            || self.actors[i].stamina < 35 || self.stock.count(Item::Food) < 7
            || !(8..19).contains(&(self.minute_of_day()/60)) { return None; }
        let revisit=self.now_ms/DAY_MS>=700 && state.social.matured_village && state.social.familiar_visitor;
        state.memories.task(i,self.now_ms,revisit).map(Task::Memory)
            .or_else(|| self.town_renewal_task(i))
    }

    fn town_renewal_task(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        let task = Task::Memory(memories::Work::RenewCorner);
        (i == ActorKind::Ayu.index() && self.town_allows(TownEra::Heritage)
            && self.town_era() == TownEra::Mature && state.renovation.corner
            && self.stock.count(Item::Wood) > 0 && self.tool_available(i, Tool::Hammer))
            .then_some(task)
    }

    fn outfit_for(&self, i: usize, sleeping_hours: bool) -> Outfit {
        if sleeping_hours || i == 2 {
            return Outfit::Home;
        }
        if matches!(self.weather(), YardWeather::Rain | YardWeather::Snow) {
            return Outfit::Raincoat;
        }
        if i == 0 { Outfit::Garden } else { Outfit::Work }
    }

    fn offer_shared_meal(&mut self) {
        let day = self.now_ms / DAY_MS;
        let hour = self.minute_of_day() / 60;
        let home_harvest = self.village.as_ref().is_some_and(|state| {
            !state.opening.shared_serving && state.opening.shared_until > self.now_ms
        });
        let wheat = !home_harvest && self.village.as_ref().is_some_and(|state| {
            !state.grain.serving && state.grain.ready_until > self.now_ms
        });
        let reserved = home_harvest || wheat;
        if !(if reserved { 8..20 } else { 11..15 }).contains(&hour)
            || self.last_shared_meal == Some(day)
            || self.meal_until != 0
            || !reserved && self.stock.count(Item::Food) < 2
        {
            return;
        }
        let available = self.actors[..2].iter().all(|actor| {
            !actor.outside
                && actor.carried.is_none()
                && (actor.hunger >= 22 || reserved && actor.hunger >= 10)
                && (actor.stage == Stage::Idle || reserved && actor.stage == Stage::Finish
                    || (actor.stage == Stage::Work && actor.task == Some(Task::Rest)))
                && (self.is_village() || self.route(actor.cell, self.pantry()).is_some())
        });
        if !available {
            return;
        }
        let travel_time = if self.is_village() {
            // The serving is at an actual seat, and a portion is fetched
            // from its own pantry station. Use both legs of the rendezvous.
            let longest = (0..2)
                .map(|i| {
                    let pickup = self.tool_pickup(i, Task::SharedEat);
                    self.actors[i].cell.distance(pickup) + pickup.distance(self.site(i, Task::SharedEat))
                })
                .max().unwrap_or(0) as u64;
            (longest * 2 * MINUTE + 10 * MINUTE).clamp(22 * MINUTE, 4 * HOUR)
        } else {
            22 * MINUTE
        };
        if home_harvest {
            // The two earmarked portions already exist outside general stock.
            self.village.as_mut().unwrap().opening.shared_serving = true;
        } else if wheat {
            self.village.as_mut().unwrap().grain.serving = true;
        } else if !self.take_stock(Item::Food, 2) {
            return;
        }
        self.meal_escrow = 2;
        self.meal_until = self.now_ms + travel_time;
        for i in 0..2 {
            let started = self.begin(i, Task::SharedEat);
            debug_assert!(started);
        }
    }

    fn meet_for_meal(&mut self) {
        if self.meal_until == 0 {
            return;
        }
        if self.actors[..2]
            .iter()
            .all(|actor| actor.task == Some(Task::SharedEat) && actor.stage == Stage::Meet)
        {
            self.last_shared_meal = Some(self.now_ms / DAY_MS);
            self.meal_until = 0;
            for i in 0..2 {
                self.stage(i, Stage::Work, Pose::Eat, Task::SharedEat.duration());
            }
        } else if self.meal_until <= self.now_ms {
            // Retry a named serving only while both portions are still at its
            // pickup station and fresh. A lifted portion stays in its carrier's
            // hands and can be stored by the actor after the meetup ends.
            let mut named_expiry = None;
            if let Some(state) = &mut self.village && state.opening.shared_serving {
                state.opening.shared_serving = false;
                state.opening.shared_eaten = 0;
                named_expiry = Some(state.opening.shared_until);
                if self.meal_escrow != 2 || state.opening.shared_until <= self.now_ms {
                    state.opening.shared_until = 0;
                }
            }
            if let Some(state) = &mut self.village && state.grain.serving {
                state.grain.serving = false;
                state.grain.eaten = 0;
                named_expiry = Some(state.grain.ready_until);
                if self.meal_escrow != 2 || state.grain.ready_until <= self.now_ms {
                    state.grain.ready_until = 0;
                }
            }
            if named_expiry.is_some_and(|due| due <= self.now_ms) {
                self.story.spoiled = self.story.spoiled.saturating_add(u64::from(self.meal_escrow));
            } else if named_expiry.is_none() || self.meal_escrow < 2 {
                self.stock.add(Item::Food, self.meal_escrow);
            }
            self.meal_escrow = 0;
            self.meal_until = 0;
            for i in 0..2 {
                if self.actors[i].task == Some(Task::SharedEat) {
                    self.finish_edge_then_release(i);
                }
            }
        }
    }

    fn finish_edge_then_release(&mut self, i: usize) {
        let actor = &mut self.actors[i];
        actor.route_goal = None;
        if let Some(&next) = actor
            .route
            .get(actor.route_next)
            .filter(|_| !actor.waiting_edge)
        {
            actor.route.truncate(actor.route_next + 1);
            actor.task = Some(Task::External(next));
            actor.stage = Stage::WalkToSite;
        } else {
            self.idle(i, MINUTE);
        }
    }

    fn task_in_use(&self, task: Task) -> bool {
        self.actors.iter().any(|actor| actor.task == Some(task))
    }

    fn trade_reserve_pending(&self) -> bool {
        self.village.as_ref().is_some_and(|village| {
            village.pantry_plan.ready && !village.pantry_plan.completed("Q16")
        })
    }

    fn market_crop_shortfall(&self) -> bool {
        self.supply_requests.market_crops.is_some_and(|target| self.stock.count(Item::Crops) < target)
    }

    fn neighbour_meals_needed(&self) -> bool {
        self.supply_requests.neighbour_meals && self.stock.count(Item::Food) < NEIGHBOUR_MEAL_TARGET
    }

    fn tool_available(&self, i: usize, tool: Tool) -> bool {
        self.actors.iter().enumerate().all(|(j, actor)| {
            j == i || (actor.tool != Some(tool) && actor.task.and_then(Task::tool) != Some(tool))
        })
    }

    fn farm_task(&self, i: usize) -> Option<Task> {
        let crops = self.stock.count(Item::Crops);
        let planting_limit = if self.story.is_open(DevelopmentKind::Road) {
            22
        } else {
            17
        } + u16::from(self.buildings[0].level.saturating_sub(2)) * 2
            + if self.story.improvements.working(Fixture::WestSeedDrawer) {
                2
            } else {
                0
            };
        let prefer_nearby = self.trade_reserve_pending() || self.market_crop_shortfall() || self.neighbour_meals_needed();
        self.plots
            .iter()
            .enumerate()
            .filter_map(|(index, plot)| {
                if self.is_village() && index == usize::from(grain::FIELD) {
                    return None;
                }
                if self.is_village()
                    && !self
                        .phase_mask()
                        .contains(WILLOW_VILLAGE.crops[index].phase)
                {
                    return None;
                }
                let id = index as u8;
                if self
                    .actors
                    .iter()
                    .any(|actor| actor.task.is_some_and(|task| task.uses_plot(id)))
                {
                    return None;
                }
                if self
                    .village
                    .as_ref()
                    .is_some_and(|state| state.flooded_plots & (1 << index) != 0)
                {
                    return self
                        .tool_available(i, Tool::Hoe)
                        .then_some((0, id, Task::Drain(id)));
                }
                let (priority, task) = match plot.stage {
                    PlotStage::Mature
                        if self.social_harvest_plot.0 != Some(id) && crops < planting_limit + 4
                            && self.stock.fits(Item::Crops, 3)
                            && (self.is_village() || self.stock.fits(Item::Seeds, 1)) =>
                    {
                        (0, Task::Harvest(id))
                    }
                    PlotStage::Seed | PlotStage::Growing
                        if crops < planting_limit
                            && !plot.watered
                            && self.stock.count(Item::Water) > 0 =>
                    {
                        (1, Task::Water(id))
                    }
                    PlotStage::Tilled
                        if crops < planting_limit && self.stock.count(Item::Seeds) > 0 =>
                    {
                        (2, Task::Sow(id))
                    }
                    PlotStage::Cleared if crops < planting_limit => (3, Task::Till(id)),
                    PlotStage::Grass if crops < planting_limit => (4, Task::Clear(id)),
                    _ => return None,
                };
                task.tool()
                    .is_none_or(|tool| self.tool_available(i, tool))
                    .then_some((priority, id, task))
            })
            .min_by_key(|&(priority, id, task)| {
                let distance = if prefer_nearby {
                    self.actors[i].cell.distance(self.site(i, task))
                } else {
                    0
                };
                (priority, distance, id)
            })
            .map(|(_, _, task)| task)
    }

    fn garden_task(&self, i: usize) -> Option<Task> {
        (self.story.is_open(DevelopmentKind::Garden)
            && self.story.next_garden <= self.now_ms
            && self.stock.count(Item::Food) >= 7
            && self.actors[i].stamina >= 50
            && !self.task_in_use(Task::Garden)
            && self.tool_available(i, Tool::Hoe))
        .then_some(Task::Garden)
    }

    fn support_task(&self, i: usize) -> Option<Task> {
        if self.bowl < 2
            && self.stock.count(Item::Water) > 0
            && !self.task_in_use(Task::FillBowl)
            && self.tool_available(i, Tool::WateringCan)
        {
            return Some(Task::FillBowl);
        }
        if self.stock.count(Item::Water) < 5
            && self.well > 0
            && !self.water_collection_in_use()
            && self.tool_available(i, Tool::WateringCan)
        {
            return Some(self.water_collection_task(i));
        }
        (self.stock.count(Item::Food) < 8
            && self.stock.count(Item::Crops) < 4
            && self.fish_in_pond > 2
            && !self.task_in_use(Task::Fish)
            && self.tool_available(i, Tool::FishingRod))
        .then_some(Task::Fish)
    }

    fn water_collection_task(&self, i: usize) -> Task {
        let actor = &self.actors[i];
        if actor.kind == ActorKind::Ayu && self.story.is_open(DevelopmentKind::Well)
            && self.town_era() >= TownEra::PublicDistrict
            && actor.cell.distance(Self::well_draw_stand()) < actor.cell.distance(village::poi("market_well"))
        {
            // The home water run has its own stable destination. An older
            // saved GetWater trip to the public well still finishes there.
            Task::GetHomeWater
        } else {
            Task::GetWater
        }
    }

    fn water_collection_in_use(&self) -> bool {
        self.actors.iter().any(|actor| matches!(actor.task, Some(Task::GetWater | Task::GetHomeWater)))
    }

    fn orchard_task(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        if !(13..16).contains(&(self.minute_of_day() / 60))
            || self.actors[i].stamina < 55
            || self.stock.count(Item::Crops) >= 17
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            return None;
        }
        state.orchard.iter().enumerate().find_map(|(index, tree)| {
            let task = Task::CareOrchard(index as u8);
            (!self.task_in_use(task)
                && (tree.fruit > 0 || self.now_ms / DAY_MS >= tree.last_care_day + 14))
                .then_some(task)
        })
    }

    fn build_task(&self, i: usize) -> Option<Task> {
        let base_order = if self.preserve_requires_storehouse() { [3, 4, 2, 0, 1] } else { [4, 3, 2, 0, 1] };
        let base = base_order
            .into_iter()
            .find(|&index| self.buildings[index].level < 2);
        if let Some(site) = base {
            let build = Task::Build(site as u8);
            let (item, amount) = self.build_input(site);
            if self.stock.count(item) >= amount
                && self.tool_available(i, Tool::Hammer)
                && !self.task_in_use(build)
            {
                return Some(build);
            }
        }
        // Maintain a limited common reserve for later household or community work.
        if self.stock.count(Item::Wood) >= 9 {
            return None;
        }
        if let Some(index) = self.trees.iter().enumerate().position(|(index, tree)| {
            tree.stage == TreeStage::Felled && !self.task_in_use(Task::TakeTimber(index as u8))
                && self.village.as_ref().is_none_or(|state| state.workshop_story.tree != Some(index as u8))
        }) {
            return Some(Task::TakeTimber(index as u8));
        }
        if !self.tool_available(i, Tool::Axe) {
            return None;
        }
        self.trees
            .iter()
            .enumerate()
            .position(|(index, tree)| {
                !tree.protected
                    && tree.stage == TreeStage::Standing
                    && self.village.as_ref().is_none_or(|state| state.workshop_story.tree != Some(index as u8))
                    && !self.task_in_use(Task::FellTree(index as u8))
            })
            .map(|index| Task::FellTree(index as u8))
    }

    fn build_input(&self, index: usize) -> (Item, u16) {
        let building = &self.buildings[index];
        let phase = structure_work(index, building.level, building.step);
        (chapter_item(phase.material), phase.units)
    }

    fn structural_candidate(&self) -> Option<(usize, u8, u8)> {
        let order = [4, 3, 2, 0, 1];
        let priority = order
            .into_iter()
            .find(|&site| self.buildings[site].step != 0 && self.buildings[site].level >= 2);
        order
            .into_iter()
            .filter(|&site| priority.is_none_or(|chosen| chosen == site))
            .filter_map(|site| {
                let building = self.buildings[site];
                let day = self.now_ms / DAY_MS;
                let age = match building.level {
                    2 => [49, 60, 28, 36, 21][site],
                    3 => [157, 175, 133, 145, 114][site],
                    _ => return None,
                };
                (day >= age && self.story.improvements.structure_due(site) <= self.now_ms)
                    .then_some((site, building.level, building.step))
            })
            .min_by_key(|&(_, level, step)| (level, u8::MAX - step))
    }

    fn advanced_build_task(&self, i: usize) -> Option<Task> {
        if self.stock.count(Item::Food) < 8
            || self.actors[i].stamina < 53
            || self.actors[i].hunger >= 48
            || self.actors[i].thirst >= 54
            || !self.tool_available(i, Tool::Hammer)
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            || self.story.improvements.active().is_some()
        {
            return None;
        }
        let (site, _, _) = self.structural_candidate()?;
        let (item, amount) = self.build_input(site);
        let task = Task::Build(site as u8);
        (self.stock.count(item) >= amount && !self.task_in_use(task)).then_some(task)
    }

    fn improvement_unlocked(&self, fixture: Fixture) -> bool {
        let site = usize::from(fixture.site());
        fixture.requires().map_or_else(
            || self.buildings[site].level >= 2,
            |facility| self.story.is_open(facility),
        )
    }

    fn improvement_task(&self, i: usize) -> Option<Task> {
        if self.stock.count(Item::Food) < 7
            || self.actors[i].stamina < 48
            || self.actors[i].hunger >= 48
            || self.actors[i].thirst >= 54
            || !self.tool_available(i, Tool::Hammer)
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            || self
                .actors
                .iter()
                .any(|actor| matches!(actor.task, Some(Task::Improve(_))))
        {
            return None;
        }
        if self.story.improvements.active().is_none()
            && self
                .buildings
                .iter()
                .any(|building| building.level >= 2 && building.step > 0)
        {
            return None;
        }
        self.story
            .improvements
            .orders(self.now_ms, self.season() as usize)
            .find(|&order| {
                let (material, count) = order.material();
                self.improvement_unlocked(order.fixture)
                    && !(order.fixture.site() == Fixture::PantryDryRack.site()
                        && order.fixture != Fixture::PantryDryRack
                        && self.village.as_ref().is_some_and(|state| state.preserves.rack_reserved()))
                    && self.stock.count(chapter_item(material)) >= count
                    && self.can_reach_static(self.pantry(), self.improvement_site(order.fixture))
            })
            .map(Task::Improve)
    }

    fn housing_task(&self, i: usize) -> Option<Task> {
        if self.stock.count(Item::Food) < 7
            || self.actors[i].stamina < 48
            || self.actors[i].hunger >= 51
            || self.actors[i].thirst >= 56
            || self.village.as_ref().is_some_and(|state| state.renovation.displaced.is_some())
            || self
                .actors
                .iter()
                .any(|actor| matches!(actor.task, Some(Task::BuildHome(_))))
        {
            return None;
        }
        let next = self.story.housing.next_order(self.now_ms);
        if let Some(order) = next.filter(|order| order.phase == HousingPhase::MoveIn) {
            return (self.story.housing.target_occupant(order) == Some(i))
                .then_some(Task::BuildHome(order));
        }
        if next.is_none() && self.village.as_ref().is_some_and(|state| {
            !self.town_allows(TownEra::SecondBlock) || !state.renovation.completed("Q35")
        }) {
            return None;
        }
        if i != ActorKind::Ayu.index()
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            return None;
        }
        next.into_iter()
            .chain(
                self.story
                    .housing
                    .proposals_for(self.now_ms, self.is_village()),
            )
            .find(|&order| {
                Task::BuildHome(order)
                    .tool()
                    .is_none_or(|tool| self.tool_available(i, tool))
                    && order
                        .phase
                        .material(order.plan)
                        .is_none_or(|(material, amount)| {
                            self.stock.count(chapter_item(material)) >= amount
                        })
                    && self.housing_site_legal(order)
            })
            .map(Task::BuildHome)
    }

    fn housing_site_legal(&self, order: HousingOrder) -> bool {
        let [x, y] = order.site();
        let site = Cell(x.into(), y.into());
        if self.is_village() {
            if !self.valid_world_cell(site) {
                return false;
            }
            if order.phase > HousingPhase::Foundation {
                return true;
            }
            let [x, y] = order.lot_origin();
            let [w, h] = order.plan.footprint();
            let available = (y..y + h).all(|ty| {
                (x..x + w).all(|tx| {
                    self.can_walk(tx.into(), ty.into())
                        && self.ground(tx.into(), ty.into()) == Ground::Grass
                })
            });
            return available
                && !self.actors.iter().any(|actor| {
                    std::iter::once(actor.cell)
                        .chain(actor.route.iter().skip(actor.route_next).copied())
                        .any(|cell| order.inside(cell.0.into(), cell.1.into()))
                })
                && !self
                    .external_occupied
                    .iter()
                    .any(|cell| order.inside(cell.x.into(), cell.y.into()));
        }
        if self.route(PANTRY, site).is_none() {
            return false;
        }
        if order.phase != HousingPhase::Clear && order.phase != HousingPhase::Foundation {
            return true;
        }
        if order.is_new_site()
            && (!housing_sight_clear_of_buildings(order, self.workshop_door())
                || self.story.housing.orders().any(|other| {
                    other.lot != order.lot
                        && !housing_sight_bounds(order)
                            .separated_from(housing_sight_bounds(other), 8)
                }))
        {
            return false;
        }
        let [x, y] = order.lot_origin();
        let [width, height] = order.plan.footprint();
        if !(y..y + height).all(|y| {
            (x..x + width).all(|x| {
                self.ground(x.into(), y.into()) == Ground::Grass
                    && self.can_walk(x.into(), y.into())
            })
        }) {
            return false;
        }
        if self.actors.iter().any(|actor| {
            std::iter::once(actor.cell)
                .chain(actor.route[actor.route_next..].iter().copied())
                .any(|cell| order.inside(cell.0.into(), cell.1.into()))
        }) {
            return false;
        }
        let walkable = |x, y| self.can_walk(x, y) && !order.inside(x, y);
        self.actors
            .iter()
            .map(|actor| actor.cell)
            .chain([site, HOUSE_DOORS[0], HOUSE_DOORS[1], STOVE, WELL, POND_BANK])
            .all(|target| route_by(PANTRY, target, walkable).is_some())
    }

    fn work_duration(&mut self, i: usize, task: Task) -> u64 {
        let (fixture, reduction) = match task {
            Task::Cook if self.buildings[2].level >= 3 && self.has_carry(i, Item::Crops, 3) => {
                (Some(Fixture::KitchenBatchStove), 3 * MINUTE)
            }
            Task::Cook if self.has_carry(i, Item::Crops, 2) || self.has_carry(i, Item::Fish, 1) => {
                (Some(Fixture::KitchenQuickStove), 6 * MINUTE)
            }
            Task::Build(_) | Task::Develop(_) => (Some(Fixture::WorkshopJig), 6 * MINUTE),
            Task::Improve(order) if order.fixture.site() != 4 => {
                (Some(Fixture::WorkshopJig), 6 * MINUTE)
            }
            _ => (None, 0),
        };
        let discount = if let Some(fixture) = fixture
            && self.story.improvements.working(fixture)
        {
            self.story.improvements.used(fixture);
            reduction
        } else {
            0
        };
        let structure = match task {
            Task::Build(site) => {
                let site = usize::from(site);
                let building = &self.buildings[site];
                u64::from(structure_work(site, building.level, building.step).minutes) * MINUTE
            }
            Task::Cook if self.buildings[2].level >= 4 => task.duration() - 4 * MINUTE,
            Task::Develop(_) | Task::Improve(_) if self.buildings[4].level >= 4 => {
                task.duration() - 4 * MINUTE
            }
            _ => task.duration(),
        };
        structure.saturating_sub(discount)
    }

    fn input(&self, task: Task) -> Option<(Item, u16)> {
        match task {
            Task::Eat | Task::ServeCat => Some((Item::Food, 1)),
            Task::Sow(_) => Some((Item::Seeds, 1)),
            Task::Water(_) | Task::FillBowl => Some((Item::Water, 1)),
            Task::Cook if self.stock.count(Item::Fish) > 0 && self.village.as_ref().is_some_and(|state| {
                state.soil.aims_to_cook_fish()
            }) => Some((Item::Fish, 1)),
            Task::Cook if self.trade_reserve_pending() && self.stock.count(Item::Fish) > 0
                && self.stock.count(Item::Crops) < 10 => Some((Item::Fish, 1)),
            Task::Cook if self.market_crop_shortfall() && self.stock.count(Item::Fish) > 0
                && self.village.as_ref().is_some_and(|state| !state.grain.needs_leaf_first(self.now_ms)) => {
                Some((Item::Fish, 1))
            }
            Task::Cook
                if self.stock.count(Item::Crops) >= 3
                    && (self.buildings[2].level >= 3
                        || self.story.improvements.working(Fixture::KitchenBatchStove)) =>
            {
                Some((Item::Crops, 3))
            }
            Task::Cook if self.stock.count(Item::Crops) >= 2 => Some((Item::Crops, 2)),
            Task::Cook => Some((Item::Fish, 1)),
            Task::Grain(grain::Work::MountMill) => Some((Item::Stone, 1)),
            Task::Grain(grain::Work::RecoverRootSeed) => Some((Item::Seeds, 1)),
            Task::Preserve(preserves::Work::LoadRack) => Some((Item::Crops, preserves::CROP_UNITS)),
            Task::Preserve(preserves::Work::RepairRack) => Some((Item::Wood, 1)),
            Task::WorkshopStory(workshop_story::Work::AcceptOrder) => Some((Item::Wood, 2)),
            Task::Renovate(renovation::Work::Repair(_)) => Some(self.village.as_ref()?.renovation.repair_cost()),
            Task::Renovate(renovation::Work::LayPath) => Some((Item::Stone, 1)),
            Task::Renovate(renovation::Work::BuildCorner) => Some((Item::Wood, 1)),
            Task::Renovate(renovation::Work::PrepareGuests) => Some((Item::Food, 2)),
            Task::RoadStory(road_story::Work::Build | road_story::Work::CarryTest | road_story::Work::PackOrder)
                => self.village.as_ref()?.road_story.cost(),
            Task::MarketStory(work) => road_story::Market::cost(work),
            Task::Memory(memories::Work::CareKeepsake) => Some((Item::Wood,1)),
            Task::Memory(memories::Work::RenewCorner) => Some((Item::Wood, 1)),
            Task::Memory(memories::Work::Celebrate(_)) => Some((Item::Food,2)),
            Task::Build(index) => Some(self.build_input(usize::from(index))),
            Task::Improve(order) => {
                let (material, amount) = order.material();
                Some((chapter_item(material), amount))
            }
            Task::BuildHome(order) => order
                .phase
                .material(order.plan)
                .map(|(material, amount)| (chapter_item(material), amount)),
            Task::Develop(id) if self.story.running.is_none() => {
                let chapter = &CHAPTERS[usize::from(id)];
                Some((chapter_item(chapter.material), chapter.amount))
            }
            Task::FreightBridge
                if self
                    .village
                    .as_ref()
                    .is_some_and(|state| !state.freight_material_paid) =>
            {
                Some((Item::Stone, 3))
            }
            Task::Trade(_, amount) => Some((Item::Crops, amount)),
            _ => None,
        }
    }

    fn site(&self, i: usize, task: Task) -> Cell {
        if self.is_village() {
            return self.village_site(i, task);
        }
        match task {
            Task::Clear(id)
            | Task::Till(id)
            | Task::Sow(id)
            | Task::Water(id)
            | Task::Harvest(id) => {
                let plot = PLOTS[usize::from(id)];
                Cell(plot.0, plot.1 + 1)
            }
            Task::FellTree(id) | Task::TakeTimber(id) => {
                let tree = TREES[usize::from(id)];
                Cell(tree.0, tree.1 + 1)
            }
            Task::Build(id) => self.building_door(usize::from(id)),
            Task::Improve(order) => self.improvement_site(order.fixture),
            Task::BuildHome(order) => {
                let [x, y] = order.site();
                Cell(x.into(), y.into())
            }
            Task::Develop(id) => {
                let site = SITES[usize::from(CHAPTERS[usize::from(id)].site)];
                Cell(site.x.into(), site.y.into())
            }
            Task::Trade(..) => {
                if self.story.is_open(DevelopmentKind::Market) {
                    MARKET_TRADE
                } else {
                    ROAD_TRADE
                }
            }
            Task::Garden => Cell(40, 35),
            Task::Store
                if self.previous_home_use(ActorKind::Xiaohe)
                    == Some(FormerHomeUse::GrainStorage)
                    && self.actors[i].carried == Some(Item::Crops) =>
            {
                HOUSE_DOORS[0]
            }
            Task::ReturnTool
                if self.previous_home_use(ActorKind::Ayu) == Some(FormerHomeUse::ToolRoom) =>
            {
                HOUSE_DOORS[1]
            }
            Task::Store | Task::ReturnTool => PANTRY,
            Task::Cook => STOVE,
            Task::Eat | Task::SharedEat => TABLE[i],
            Task::GetWater => {
                if self.story.is_open(DevelopmentKind::Well) && i == 1 {
                    Cell(35, 35)
                } else {
                    WELL
                }
            }
            Task::GetHomeWater => WELL,
            Task::FillBowl => Cell(13, 16),
            Task::Drink if i == ActorKind::Orange.index() => BOWL,
            Task::Drink => PANTRY,
            Task::DrinkAtWell => WELL,
            Task::Fish => POND_BANK,
            Task::Rest => {
                let actor = &self.actors[i];
                if self.story.is_open(DevelopmentKind::Garden)
                    && i < 2
                    && actor.cell.1 >= 33
                    && actor.cell.distance(Cell(40, 35)) <= 10
                {
                    Cell(40, 36)
                } else if self.story.is_open(DevelopmentKind::Guesthouse)
                    && i < 2
                    && actor.cell.1 >= 33
                {
                    Cell(27, 36)
                } else {
                    if i < 2 { self.home_side(i) } else { CAT_BED }
                }
            }
            Task::Sleep | Task::ChangeClothes(_) => {
                if i < 2 {
                    self.home_door(i)
                } else {
                    CAT_BED
                }
            }
            Task::Explore(index) => CAT_SPOTS[usize::from(index)],
            Task::External(target) => target,
            Task::ReturnHome => self.home_side(i),
            Task::FreightBridge => Cell(12, 28),
            Task::ProcessHarvest(_) | Task::CareOrchard(_) => self.pantry(),
            Task::Drain(_) => self.pantry(),
            Task::UnpackHome | Task::RackRod | Task::ServeCat | Task::CatEat | Task::CookHomeHarvest | Task::EatSavedPortion
            | Task::EatWhileDrying | Task::CourtyardBreak | Task::Grain(_) | Task::Soil(_) | Task::WorkshopStory(_)
            | Task::Preserve(_) | Task::Renovate(_) | Task::RoadStory(_) | Task::MarketStory(_) | Task::Memory(_) => {
                unreachable!("opening tasks only exist in the village")
            }
        }
    }

    fn village_site(&self, i: usize, task: Task) -> Cell {
        match task {
            Task::Clear(id)
            | Task::Till(id)
            | Task::Sow(id)
            | Task::Water(id)
            | Task::Harvest(id)
            | Task::Drain(id) => Cell::from(WILLOW_VILLAGE.crops[usize::from(id)].work),
            Task::FellTree(id) | Task::TakeTimber(id) => {
                let tree =
                    Cell::at(self.trees[usize::from(id)].position).expect("village tree anchor");
                Cell(tree.0, tree.1 + 1)
            }
            Task::Build(id) => self.building_door(usize::from(id)),
            Task::Improve(order) => self.improvement_site(order.fixture),
            Task::BuildHome(order) => {
                let [x, y] = order.site();
                Cell(x.into(), y.into())
            }
            Task::Develop(id) => self.development_site(usize::from(CHAPTERS[usize::from(id)].site)),
            Task::FreightBridge => {
                let state = self.village.as_ref().expect("village project");
                if state.freight_shifts == 1 {
                    village::poi("bridge_wait_e")
                } else {
                    village::poi("bridge_wait_w")
                }
            }
            Task::ProcessHarvest(1) => village::poi("home_well"),
            Task::ProcessHarvest(3) if i == ActorKind::Ayu.index() => {
                let rack = grain::drying_site();
                Cell(rack.0 + 1, rack.1)
            }
            Task::ProcessHarvest(3) => {
                let door = village::poi("pantry_loading");
                Cell(door.0, door.1 + 1)
            }
            Task::ProcessHarvest(_) => self.pantry(),
            Task::CareOrchard(tree) => {
                let tree =
                    Cell::at(self.trees[8 + usize::from(tree)].position).expect("orchard position");
                Cell(tree.0, tree.1 + 1)
            }
            Task::Trade(..) => village::poi("home_trade"),
            Task::Garden => village::poi("commons_garden"),
            Task::UnpackHome => self.home_door(i),
            Task::RackRod => self.building_door(4),
            Task::ServeCat | Task::CatEat => village::poi("cat_water"),
            Task::Preserve(preserves::Work::CookBatch) => self.building_door(2),
            Task::Preserve(_) => self.building_door(3),
            Task::RoadStory(work) => match work {
                road_story::Work::SurveyWest | road_story::Work::Approve | road_story::Work::Build => village::poi("bridge_wait_w"),
                road_story::Work::SurveyEast | road_story::Work::InspectEast | road_story::Work::CarryTest => village::poi("bridge_wait_e"),
                road_story::Work::ReturnTest | road_story::Work::PackOrder => self.pantry(),
                road_story::Work::DeliverOrder => village::poi("market_gate"),
            },
            Task::MarketStory(work) => match work {
                road_story::MarketWork::Survey | road_story::MarketWork::Walk | road_story::MarketWork::NightWalk | road_story::MarketWork::Sweep => village::poi("market_gate"),
                road_story::MarketWork::TestWater | road_story::MarketWork::Drain => village::poi("market_well"),
                road_story::MarketWork::Base => Cell::from(road_story::MARKET_LANE[0]),
                road_story::MarketWork::Pave => Cell::from(road_story::MARKET_LANE[5]),
                road_story::MarketWork::MoveLamp => village::poi("market_notice"),
                road_story::MarketWork::CarryMill | road_story::MarketWork::SetMill | road_story::MarketWork::InspectMill => village::poi("craft_carpentry.door"),
            },
            Task::Memory(work) => match work {
                memories::Work::CareKeepsake => Cell(9,18),
                memories::Work::RenewCorner => village::poi("home_table"),
                memories::Work::NoteReturn => village::poi("market_notice"),
                _ if i == 0 => village::slot("home_table","home_table.seat_a"),
                _ => village::slot("home_table","home_table.seat_b"),
            },
            Task::Renovate(work) => {
                use renovation::Work;
                match work {
                    Work::MoveOut(_) => self.temporary_home_door(),
                    Work::SurveyPath => Cell::from(renovation::PATH[0]),
                    Work::LayPath => {
                        let at = if self.village.as_ref().unwrap().renovation.path <= 1 { 2 } else { 5 };
                        Cell::from(renovation::PATH[at])
                    }
                    Work::TestPath => Cell::from(renovation::PATH[7]),
                    Work::BuildCorner | Work::PrepareGuests | Work::CloseGuestTrial => village::poi("home_table"),
                    Work::RestAtCorner if i == 0 => village::slot("home_table", "home_table.seat_a"),
                    Work::RestAtCorner => village::slot("home_table", "home_table.seat_b"),
                    _ => self.initial_home_door(work.owner()),
                }
            }
            Task::Grain(work) => match work {
                grain::Work::Sow | grain::Work::Cut | grain::Work::SowRoot | grain::Work::LiftRoot => {
                    Cell::from(WILLOW_VILLAGE.crops[usize::from(grain::FIELD)].work)
                }
                grain::Work::CarrySheaf | grain::Work::Thresh => self.building_door(4),
                grain::Work::MountMill | grain::Work::DemonstrateMill | grain::Work::MillWheat
                | grain::Work::MillIndependently => self.building_door(4),
                grain::Work::ObserveMill => {
                    let door = self.building_door(4);
                    Cell(door.0, door.1 + 1)
                }
                grain::Work::Cook | grain::Work::CookRoot | grain::Work::CookFlour | grain::Work::CookTraineeFlour => self.building_door(2),
                grain::Work::Spread | grain::Work::Uncover | grain::Work::Turn => grain::drying_site(),
                grain::Work::Cover | grain::Work::Store | grain::Work::SaveSeed => self.pantry(),
                grain::Work::StoreRoot => self.building_door(3),
                grain::Work::RecoverRootSeed => self.pantry(),
            },
            Task::Soil(work) => match work {
                soil::Work::Inspect(id) | soil::Work::InspectDrain(id) | soil::Work::ClearDrain(id)
                | soil::Work::TestDrain(id) | soil::Work::Apply(id) => {
                    Cell::from(WILLOW_VILLAGE.crops[usize::from(id)].work)
                }
                soil::Work::Mix | soil::Work::Turn => {
                    let pantry = self.pantry();
                    Cell(pantry.0, pantry.1 + 1)
                }
            },
            Task::WorkshopStory(work) => match work {
                workshop_story::Work::OilRod | workshop_story::Work::MillBeam => self.building_door(4),
                workshop_story::Work::FitBench => village::poi("home_table"),
                workshop_story::Work::ChooseTree => village::poi("forest_loading"),
                workshop_story::Work::Replant => {
                    let tree = self.village.as_ref().and_then(|state| state.workshop_story.tree).unwrap_or(0);
                    let root = village::tree_cells()[usize::from(tree)];
                    Cell(root.0, root.1 + 1)
                }
                workshop_story::Work::AcceptOrder | workshop_story::Work::CutOrder | workshop_story::Work::FinishOrder => self.building_door(4),
                workshop_story::Work::DeliverOrder | workshop_story::Work::UseOrder => self.pantry(),
            },
            Task::ReturnTool
                if self.actors[i].tool == Some(Tool::FishingRod)
                    && self.village.as_ref().is_some_and(|state| state.opening.rod_on_rack) =>
            {
                self.building_door(4)
            }
            Task::Store
                if self.previous_home_use(ActorKind::Xiaohe)
                    == Some(FormerHomeUse::GrainStorage)
                    && self.actors[i].carried == Some(Item::Crops) =>
            {
                self.initial_home_door(0)
            }
            Task::ReturnTool
                if self.previous_home_use(ActorKind::Ayu) == Some(FormerHomeUse::ToolRoom) =>
            {
                self.initial_home_door(1)
            }
            Task::Store | Task::Drink | Task::ReturnTool if i == ActorKind::Ayu.index() => self.building_door(3),
            Task::Store | Task::ReturnTool | Task::Drink if i != 2 => self.pantry(),
            Task::Store | Task::ReturnTool => self.pantry(),
            Task::Cook | Task::CookHomeHarvest => self.building_door(2),
            Task::CourtyardBreak if i == 0
                && self.village.as_ref().is_some_and(|state| state.workshop_story.built) => village::poi("home_table"),
            Task::Eat | Task::SharedEat | Task::EatSavedPortion | Task::EatWhileDrying | Task::CourtyardBreak => match i {
                0 => village::slot("home_table", "home_table.seat_a"),
                1 => village::slot("home_table", "home_table.seat_b"),
                _ => village::poi("cat_water"),
            },
            Task::GetWater if i == 1 && self.story.is_open(DevelopmentKind::Well) => {
                village::poi("market_well")
            }
            Task::GetWater | Task::GetHomeWater | Task::DrinkAtWell => Self::well_draw_stand(),
            Task::FillBowl => {
                let bowl = village::poi("cat_water");
                Cell(bowl.0 - 1, bowl.1)
            }
            Task::Drink => village::poi("cat_water"),
            Task::Fish => village::poi("home_fishing"),
            Task::Rest | Task::ReturnHome if i < 2 => self.home_side(i),
            Task::Sleep | Task::ChangeClothes(_) if i < 2 => self.home_door(i),
            Task::Rest | Task::ReturnHome | Task::Sleep | Task::ChangeClothes(_) => {
                village::poi("cat_rest")
            }
            Task::Explore(index) => self.cat_spot(usize::from(index)),
            Task::External(target) => target,
        }
    }

    fn cat_spot(&self, index: usize) -> Cell {
        if self.is_village() {
            match index {
                4 | 5 => {
                    let door = self.initial_home_door(index - 4);
                    Cell(door.0 + 1, door.1 + 1)
                }
                _ => village::cat_spot(index),
            }
        } else {
            CAT_SPOTS[index]
        }
    }

    fn work_is_indoors(&self, i: usize, task: Task) -> bool {
        i < 2
            && (matches!(task, Task::Sleep | Task::ChangeClothes(_))
                || self.is_village() && matches!(task, Task::Cook | Task::CookHomeHarvest | Task::UnpackHome
                    | Task::Grain(grain::Work::Cook | grain::Work::CookRoot | grain::Work::CookFlour | grain::Work::CookTraineeFlour)
                    | Task::Preserve(preserves::Work::CookBatch))
                || self.is_village() && i == ActorKind::Ayu.index() && matches!(task, Task::Store | Task::Drink))
    }

    fn in_household_region(&self, cell: Cell) -> bool {
        WILLOW_VILLAGE
            .tile(cell.tile(), self.phase_mask())
            .and_then(|tile| tile.region)
            .is_some_and(|region| {
                region.id == "home"
                    || region.id == "farm"
                        && self.phase_mask().contains(
                            WILLOW_VILLAGE
                                .phase("farm_expansion")
                                .expect("farm capability"),
                        )
            })
    }

    fn begin(&mut self, i: usize, task: Task) -> bool {
        if i == ActorKind::Orange.index() && matches!(task, Task::Grain(_) | Task::Soil(_) | Task::WorkshopStory(_) | Task::Preserve(_) | Task::Renovate(_) | Task::RoadStory(_) | Task::MarketStory(_)) {
            return false;
        }
        let actor = &self.actors[i];
        let tool = task.tool();
        if tool.is_some_and(|tool| !self.tool_available(i, tool)) {
            return false;
        }
        if self.input(task).is_some_and(|(item, count)| {
            actor.carried != Some(item) && self.stock.count(item) < count
        }) {
            return false;
        }
        let stop_at_pantry = (tool.is_some() && actor.tool != tool)
            || (actor.carried.is_none() && self.input(task).is_some())
            || task == Task::SharedEat;
        let (target, stage) = if (task == Task::UnpackHome
            || matches!(task, Task::BuildHome(order) if order.phase == HousingPhase::MoveIn)
            || task == Task::WorkshopStory(workshop_story::Work::DeliverOrder)
            || task == Task::MarketStory(road_story::MarketWork::CarryMill)
            || matches!(task, Task::Renovate(work) if work.transports())
            || matches!(task, Task::Grain(work) if work.transports()))
            && !actor.moving_box
        {
            (self.tool_pickup(i, task), Stage::WalkForTool)
        } else if stop_at_pantry {
            (self.tool_pickup(i, task), Stage::WalkForTool)
        } else {
            (self.site(i, task), Stage::WalkToSite)
        };
        if self.is_village() {
            if !self.valid_world_cell(actor.cell) || !self.valid_world_cell(target) {
                return false;
            }
            self.actors[i].task = Some(task);
            self.request_route(i, stage, target);
            return true;
        }
        let Some(route) = self.route(actor.cell, target) else {
            return false;
        };
        self.actors[i].task = Some(task);
        self.walk(i, stage, route);
        true
    }

    fn walk(&mut self, i: usize, stage: Stage, route: Vec<Cell>) {
        let actor = &mut self.actors[i];
        actor.route = route;
        actor.route_next = 0;
        actor.route_goal = None;
        actor.waiting_edge = false;
        actor.avoid_occupied = false;
        actor.passing = None;
        actor.stage = stage;
        actor.since = self.now_ms;
        actor.inside = false;
        actor.activity = if stage == Stage::WalkForTool {
            Activity::GetTool
        } else {
            actor.task.map_or(Activity::Idle, Task::activity)
        };
        self.next_edge(i);
    }

    fn request_route(&mut self, i: usize, stage: Stage, target: Cell) {
        self.walk(i, stage, Vec::new());
        self.actors[i].route_goal = Some(target);
    }

    fn travel_to(&mut self, i: usize, target: Cell) -> bool {
        if self.is_village() {
            if !self.valid_world_cell(target) {
                return false;
            }
            self.request_route(i, Stage::WalkToSite, target);
            return true;
        }
        let Some(route) = self.route(self.actors[i].cell, target) else {
            return false;
        };
        self.walk(i, Stage::WalkToSite, route);
        true
    }

    fn next_edge(&mut self, i: usize) {
        if self.is_village() {
            let actor = &self.actors[i];
            if let Some(&next) = actor.route.get(actor.route_next) {
                // Give well users their operating stand. A trip already
                // underway (including from a save) replans at the edge boundary.
                if !self.valid_world_cell(next)
                    || (next == Self::well_draw_stand() && actor.route_goal != Some(next)
                        && self.well_stand_reserved(i))
                {
                    let goal = actor.route_goal;
                    let stage = actor.stage;
                    if let Some(goal) = goal {
                        self.request_route(i, stage, goal);
                    } else {
                        self.idle(i, MINUTE);
                    }
                    return;
                }
                let is_main_deck = |at: TilePos| (42..=43).contains(&at.y) && (56..=68).contains(&at.x);
                let waiting_for_deck = is_main_deck(next.tile()) && !is_main_deck(actor.cell.tile())
                    && (self.external_occupied.iter().any(is_main_deck)
                        || self.actors.iter().enumerate().any(|(j, other)| {
                            j != i && !other.inside && (is_main_deck(other.cell.tile())
                                || !other.waiting_edge && other.route.get(other.route_next)
                                    .is_some_and(|next| is_main_deck(next.tile())))
                        }));
                let occupied = waiting_for_deck || self.waiting_for_pantry(i, next)
                    || self.external_occupied.contains(next.tile())
                    || self.actors.iter().enumerate().any(|(j, other)| {
                        j != i
                            && !other.inside
                            && (other.cell == next
                                || (!other.waiting_edge
                                    && other.route.get(other.route_next) == Some(&next)))
                    });
                if occupied {
                    let actor = &mut self.actors[i];
                    actor.waiting_edge = true;
                    actor.waited_edges = actor.waited_edges.saturating_add(1);
                    actor.position = actor.cell.point();
                    actor.pose = Pose::Idle;
                    actor.since = self.now_ms;
                    actor.until = self.now_ms + EDGE_RETRY_WORLD_MS;
                    return;
                }
            }
        }
        let duration = if self.is_village() {
            let actor = &self.actors[i];
            let pace = if actor.kind == ActorKind::Orange {
                movement::Pace::Cat
            } else {
                movement::Pace::Human
            };
            movement::edge_duration(
                self.now_ms,
                pace,
                movement::WalkConditions {
                    loaded: actor.external_load || actor.moving_box || actor.carried.is_some(),
                    tired: actor.stamina < 35,
                    rough_weather: matches!(self.weather(), YardWeather::Rain | YardWeather::Snow),
                },
            )
        } else {
            OLD_WALK_WORLD_MS[i]
        };
        let actor = &mut self.actors[i];
        actor.waiting_edge = false;
        actor.waited_edges = 0;
        actor.since = self.now_ms;
        actor.position = actor.cell.point();
        if let Some(&cell) = actor.route.get(actor.route_next) {
            if cell.0 != actor.cell.0 {
                actor.facing_left = cell.0 < actor.cell.0;
            }
            actor.pose = if actor.carried.is_some() || actor.moving_box || actor.external_load {
                Pose::Carry
            } else {
                Pose::Walk
            };
            actor.until = self.now_ms + duration;
        } else {
            actor.route.clear();
            actor.route_next = 0;
            actor.until = self.now_ms;
        }
    }

    fn idle(&mut self, i: usize, delay: u64) {
        let actor = &mut self.actors[i];
        actor.since = self.now_ms;
        actor.until = self.now_ms + delay;
        actor.stage = Stage::Idle;
        actor.task = None;
        actor.pose = Pose::Idle;
        actor.activity = if actor.outside {
            Activity::OutsideActivity
        } else {
            Activity::Idle
        };
        actor.inside = false;
        actor.route.clear();
        actor.route_next = 0;
        actor.route_goal = None;
        actor.waiting_edge = false;
        actor.waited_edges = 0;
        actor.avoid_occupied = false;
        actor.passing = None;
        actor.external_load = false;
    }

    fn stage(&mut self, i: usize, stage: Stage, pose: Pose, duration: u64) {
        let actor = &mut self.actors[i];
        actor.stage = stage;
        actor.pose = pose;
        actor.since = self.now_ms;
        actor.until = self.now_ms + duration;
    }

    fn step(&mut self, i: usize) {
        let Some(task) = self.actors[i].task else {
            self.idle(i, MINUTE);
            return;
        };
        if self.actors[i].passing.is_some() && self.actors[i].route.is_empty() {
            self.hold_until_partner_passes(i);
            return;
        }
        match self.actors[i].stage {
            stage @ (Stage::WalkForTool | Stage::WalkToSite) => {
                if self.actors[i].waiting_edge {
                    let swap = self.reciprocal_edge(i);
                    if swap.is_none() && self.actors[i].waited_edges == 2
                        && let Some(goal) = self.actors[i].route_goal
                        && self.actors[i].route.get(self.actors[i].route_next) != Some(&goal)
                    {
                        self.request_route(i, stage, goal);
                        self.actors[i].avoid_occupied = true;
                        return;
                    }
                    if self.actors[i].waited_edges >= 2 && swap.is_none_or(|other| self.yields_to(i, other)) {
                        let destination = self.actors[i].route_goal;
                        if let Some(destination) = destination {
                            let sidestep = self.waiting_sidestep(i);
                            if let Some(side) = sidestep {
                                let passing = swap.filter(|&other| self.waits_at_other_goal(i, other))
                                    .map(|other| Passing {
                                        partner: self.actors[other].kind,
                                        approach: self.actors[other].cell,
                                        entrance: self.actors[i].cell,
                                        expires: self.now_ms + PASS_CLEAR_LIMIT,
                                    });
                                let actor = &mut self.actors[i];
                                if let Some(Task::Explore(_)) = actor.task {
                                    actor.task = Some(Task::External(side));
                                    actor.route_goal = None;
                                    actor.avoid_occupied = false;
                                } else {
                                    actor.route_goal = Some(destination);
                                    actor.avoid_occupied = true;
                                    actor.passing = passing;
                                }
                                actor.route = vec![side];
                                actor.route_next = 0;
                            }
                        }
                    }
                    if self.actors[i].waited_edges >= 12 && !self.actors[i].outside {
                        self.actors[i].moving_box = false;
                        self.idle(i, MINUTE);
                        return;
                    }
                    self.next_edge(i);
                    if self.actors[i].waiting_edge {
                        self.actors[i].passing = None;
                    }
                    return;
                }
                if let Some(&cell) = self.actors[i].route.get(self.actors[i].route_next) {
                    let actor = &self.actors[i];
                    self.record_footstep(actor.cell, cell, actor.kind);
                    self.actors[i].cell = cell;
                    self.actors[i].route_next += 1;
                }
                if self.actors[i].route_next < self.actors[i].route.len() {
                    self.next_edge(i);
                    return;
                }
                if self.hold_until_partner_passes(i) {
                    return;
                }
                if let Some(goal) = self.actors[i].route_goal {
                    if self.actors[i].cell != goal {
                        let avoid_occupied = self.actors[i].avoid_occupied;
                        let completed_sidestepping = avoid_occupied && !self.actors[i].route.is_empty();
                        self.request_route(i, stage, goal);
                        self.actors[i].avoid_occupied = avoid_occupied;
                        if completed_sidestepping {
                            let actor = &mut self.actors[i];
                            actor.pose = Pose::Idle;
                            actor.until = self.now_ms + 3 * EDGE_RETRY_WORLD_MS;
                        }
                        return;
                    }
                    self.actors[i].route_goal = None;
                }
                let actor = &mut self.actors[i];
                actor.position = actor.cell.point();
                actor.route.clear();
                actor.route_next = 0;
                match (stage, task) {
                    (Stage::WalkForTool, _) => {
                        let fixture = if self.actors[i].kind == ActorKind::Ayu
                            && self.story.improvements.working(Fixture::EastToolRack)
                        {
                            Some(Fixture::EastToolRack)
                        } else if self.story.improvements.working(Fixture::WorkshopToolBoard) {
                            Some(Fixture::WorkshopToolBoard)
                        } else {
                            None
                        };
                        let changing = task
                            .tool()
                            .is_some_and(|tool| self.actors[i].tool != Some(tool));
                        let rack =
                            self.previous_home_use(ActorKind::Ayu) == Some(FormerHomeUse::ToolRoom);
                        let base = if changing {
                            if rack {
                                40_000
                            } else if self.buildings[4].level >= 3 {
                                45_000
                            } else {
                                MINUTE
                            }
                        } else {
                            MINUTE
                        };
                        let duration = if let Some(fixture) = fixture
                            && changing
                        {
                            self.story.improvements.used(fixture);
                            base - 30_000
                        } else {
                            base
                        };
                        self.stage(i, Stage::Tool, Pose::Work, duration);
                    }
                    (_, Task::External(_)) => self.idle(i, 5 * MINUTE),
                    (_, Task::ReturnHome) => self.idle(i, MINUTE),
                    _ => self.stage(i, Stage::Prepare, Pose::Idle, 20_000),
                }
            }
            Stage::Tool => {
                if task == Task::WorkshopStory(workshop_story::Work::DeliverOrder)
                    || task == Task::MarketStory(road_story::MarketWork::CarryMill)
                    || matches!(task, Task::Renovate(work) if work.transports())
                {
                    self.actors[i].moving_box = true;
                    if !self.travel_to(i, self.site(i, task)) {
                        self.actors[i].moving_box = false;
                        self.idle(i, MINUTE);
                    }
                    return;
                }
                if matches!(task, Task::Grain(work) if work.transports()) {
                    self.actors[i].moving_box = true;
                    if !self.travel_to(i, self.site(i, task)) {
                        self.actors[i].moving_box = false;
                        self.idle(i, MINUTE);
                    }
                    return;
                }
                if task == Task::UnpackHome {
                    if self.actors[i].cell == self.home_side(i) {
                        self.actors[i].moving_box = true;
                        if self.travel_to(i, self.site(i, task)) {
                            return;
                        }
                    }
                    self.actors[i].moving_box = false;
                    self.idle(i, MINUTE);
                    return;
                }
                if let Task::BuildHome(order) = task
                    && order.phase == HousingPhase::MoveIn
                {
                    if self.actors[i].cell == self.home_door(i) {
                        self.actors[i].moving_box = true;
                        if !self.travel_to(i, self.site(i, task)) {
                            self.actors[i].moving_box = false;
                            self.idle(i, MINUTE);
                        }
                    } else {
                        self.idle(i, MINUTE);
                    }
                    return;
                }
                if let Some(tool) = task.tool() {
                    self.actors[i].tool = Some(tool);
                    if task == Task::Fish
                        && let Some(state) = &mut self.village
                        && state.opening.rod_on_rack
                    {
                        state.opening.rod_borrowed = true;
                    }
                }
                if let Task::Trade(_, amount) = task
                    && (self.stock.count(Item::Crops) < 9 + amount
                        || self.stock.count(Item::Food) < 7)
                {
                    self.idle(i, MINUTE);
                    return;
                }
                if task == Task::SharedEat && self.meal_escrow > 0 {
                    self.meal_escrow -= 1;
                    self.carry(i, Item::Food, 1);
                } else if self.actors[i].carried.is_none()
                    && let Some((item, count)) = self.input(task)
                {
                    if !self.take_stock(item, count) {
                        self.idle(i, MINUTE);
                        return;
                    }
                    self.carry(i, item, count);
                }
                if !self.travel_to(i, self.site(i, task)) {
                    self.idle(i, MINUTE);
                }
            }
            Stage::Prepare => {
                if self.is_village()
                    && matches!(task, Task::Develop(_) | Task::FreightBridge)
                    && matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
                {
                    self.idle(i, MINUTE);
                    return;
                }
                if let Task::Improve(order) = task
                    && (!self.story.improvements.accepts(order, self.now_ms)
                        || !self.improvement_unlocked(order.fixture)
                        || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow))
                {
                    // Keep the carried lot. Normal planning returns it to the pantry.
                    self.idle(i, MINUTE);
                    return;
                }
                if let Task::Build(site) = task
                    && self.buildings[usize::from(site)].level >= 2
                    && matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
                {
                    self.idle(i, MINUTE);
                    return;
                }
                if task == Task::SharedEat {
                    self.stage(
                        i,
                        Stage::Meet,
                        Pose::Rest,
                        self.meal_until.saturating_sub(self.now_ms),
                    );
                    return;
                }
                let pose = match task {
                    Task::BuildHome(order) if order.phase == HousingPhase::MoveIn => Pose::Carry,
                    Task::Grain(work) if work.transports() => Pose::Carry,
                    Task::Renovate(work) if work.transports() => Pose::Carry,
                    Task::Rest | Task::CourtyardBreak => Pose::Rest,
                    Task::Renovate(renovation::Work::RestAtCorner) => Pose::Rest,
                    Task::Explore(_) => Pose::Idle,
                    Task::Sleep => Pose::Sleep,
                    Task::Eat | Task::CatEat | Task::EatSavedPortion | Task::EatWhileDrying => Pose::Eat,
                    Task::Drink | Task::DrinkAtWell => Pose::Drink,
                    _ => Pose::Work,
                };
                self.actors[i].inside = self.work_is_indoors(i, task);
                let duration = self.work_duration(i, task);
                self.stage(i, Stage::Work, pose, duration);
            }
            Stage::Meet => (),
            Stage::Work => {
                self.commit(i, task);
                let recovery = if task == Task::Fish { MINUTE } else { 20_000 };
                self.stage(i, Stage::Finish, Pose::Idle, recovery);
                self.actors[i].inside = false;
            }
            Stage::Finish => self.idle(i, MINUTE),
            Stage::Idle => (),
        }
    }

    fn waiting_sidestep(&self, i: usize) -> Option<Cell> {
        let actor = &self.actors[i];
        let next = actor.route.get(actor.route_next).copied()?;
        let oncoming = self.reciprocal_edge(i).map(|j| &self.actors[j]);
        [(0, -1), (-1, 0), (0, 1), (1, 0)]
            .into_iter()
            .filter_map(|(dx, dy)| actor.cell.tile().offset(dx, dy))
            .filter(|pos| {
                *pos != next.tile()
                    && !(*pos == Self::well_draw_stand().tile() && self.well_stand_reserved(i))
                    && !self.waiting_for_pantry(i, Cell::from(*pos))
                    && self.can_walk(pos.x.into(), pos.y.into())
                    && !self.external_occupied.contains(*pos)
                    && !self.actors.iter().enumerate().any(|(j, other)| {
                        j != i
                            && !other.inside
                            && (other.cell.tile() == *pos
                                || other
                                    .route
                                    .get(other.route_next)
                                    .is_some_and(|cell| cell.tile() == *pos))
                    })
            })
            .min_by_key(|&position| {
                let candidate = Cell::from(position);
                let near = |cell: Cell| candidate.distance(cell);
                let oncoming_lane = oncoming.map_or(0, |other| {
                    // Leave the other person's planned side of the corridor clear.
                    let beyond = other.route.get(other.route_next + 1).copied().unwrap_or(other.cell);
                    u32::MAX - (near(other.cell) + near(beyond)) as u32
                });
                (oncoming_lane, actor.route_goal.map_or(0, near))
            })
            .map(Cell::from)
    }

    fn reciprocal_edge(&self, i: usize) -> Option<usize> {
        let actor = &self.actors[i];
        let next = actor.route.get(actor.route_next)?;
        self.actors.iter().enumerate().find_map(|(j, other)| {
            (i != j && other.waiting_edge && other.cell == *next
                && other.route.get(other.route_next) == Some(&actor.cell)).then_some(j)
        })
    }

    fn waits_at_other_goal(&self, i: usize, other: usize) -> bool {
        i < 2 && other < 2
            && !self.actors[i].outside && !self.actors[other].outside
            && (self.actors[i].route_goal == Some(self.actors[other].cell)
                || self.actors[other].route_goal == Some(self.actors[i].cell))
    }

    fn hold_until_partner_passes(&mut self, i: usize) -> bool {
        let Some(passing) = self.actors[i].passing else { return false; };
        let partner = &self.actors[passing.partner.index()];
        if self.now_ms >= passing.expires || partner.inside
            || (partner.cell != passing.approach && partner.cell != passing.entrance)
        {
            self.actors[i].passing = None;
            return false;
        }
        let actor = &mut self.actors[i];
        actor.route.clear();
        actor.route_next = 0;
        actor.position = actor.cell.point();
        actor.pose = Pose::Idle;
        actor.since = self.now_ms;
        actor.until = (self.now_ms + EDGE_RETRY_WORLD_MS).min(passing.expires);
        true
    }

    fn yields_to(&self, i: usize, other: usize) -> bool {
        let actor = &self.actors[i];
        let partner = &self.actors[other];
        let leaving_pantry = |p: &Actor| {
            self.pantry_approach(p.cell) && p.route_goal.is_some_and(|goal| !self.pantry_approach(goal))
        };
        match (leaving_pantry(actor), leaving_pantry(partner)) {
            (false, true) => true,
            (true, false) => false,
            _ if self.waits_at_other_goal(i, other) => {
                // Someone already standing at the other person's destination
                // goes through first. The arriving actor waits off the lane.
                match (actor.route_goal == Some(partner.cell), partner.route_goal == Some(actor.cell)) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => i > other,
                }
            }
            _ => i > other,
        }
    }

    fn pantry_approach(&self, cell: Cell) -> bool {
        let dock = self.pantry();
        (dock.0 - 2..=dock.0 + 1).contains(&cell.0)
            && (dock.1 - 1..=dock.1 + 1).contains(&cell.1)
    }

    fn waiting_for_pantry(&self, i: usize, next: Cell) -> bool {
        if !self.is_village() || !self.pantry_approach(next)
            || self.pantry_approach(self.actors[i].cell) {
            return false;
        }
        let actor = &self.actors[i];
        if !actor.route_goal.is_some_and(|target| self.pantry_approach(target)) {
            return false;
        }
        self.actors.iter().enumerate().any(|(j, other)| {
            if i == j || other.inside { return false; }
            let incoming = other.route.get(other.route_next).copied();
            if !self.pantry_approach(other.cell) {
                return !other.waiting_edge && incoming.is_some_and(|candidate| self.pantry_approach(candidate));
            }
            !matches!(other.stage, Stage::Idle | Stage::Finish)
        }) || self.external_occupied.iter().any(|at| self.pantry_approach(Cell::from(at)))
    }

    fn carry(&mut self, i: usize, item: Item, count: u16) {
        self.actors[i].carried = Some(item);
        self.actors[i].amount = count;
    }

    fn drop_carry(&mut self, i: usize) {
        self.actors[i].carried = None;
        self.actors[i].amount = 0;
        self.actors[i].seeds_in_basket = false;
        self.actors[i].harvest_processing = 0;
        self.actors[i].traced_wood = None;
    }

    fn has_carry(&self, i: usize, item: Item, amount: u16) -> bool {
        self.actors[i].carried == Some(item) && self.actors[i].amount == amount
    }

    fn note_opening_shift(&mut self, i: usize, kitchen: bool) {
        if let Some(state) = &mut self.village && self.actors[i].stage == Stage::Work {
            state.opening.work_finished(kitchen, i as u8, self.actors[i].since, self.now_ms);
        }
    }

    fn commit(&mut self, i: usize, task: Task) {
        if self.actors[i].cell != self.site(i, task) {
            return;
        }
        match task {
            Task::Rest | Task::CourtyardBreak => {
                let mut bonus = if self.actors[i].cell.1 == 36 { 36 } else { 28 };
                if i == ActorKind::Ayu.index()
                    && self.actors[i].cell == self.home_side(i)
                    && self.buildings[1].level >= 3
                {
                    bonus += 6;
                }
                if self.actors[i].cell == Cell(27, 36)
                    && self.actors[i].stamina < 90
                    && self.story.improvements.working(Fixture::GuesthouseBench)
                {
                    self.story.improvements.used(Fixture::GuesthouseBench);
                    bonus += 10;
                }
                let stamina = self.actors[i].stamina;
                self.actors[i].stamina = stamina.saturating_add(bonus).min(100);
                if task == Task::CourtyardBreak && stamina < self.actors[i].stamina
                    && let Some(state) = &mut self.village
                {
                    state.opening.note_break(i, self.now_ms);
                }
            }
            Task::Sleep => {
                self.actors[i].stamina = 100;
                if i < 2 && let Some(state) = &mut self.village {
                    state.opening.slept_at_home |= state.opening.boxes_stowed & (1 << i);
                    state.opening.note_need(i, 0);
                    state.pantry_plan.rested(i);
                    if state.renovation.displaced == Some(i as u8) && !state.renovation.displaced_room_empty {
                        state.renovation.had_sleep = true;
                    }
                }
                if matches!(self.season(), YardSeason::Autumn | YardSeason::Winter)
                    && let Some(fixture) = weatherstrip(self.actors[i].kind)
                {
                    self.story.improvements.used(fixture);
                }
            }
            Task::Eat | Task::SharedEat => {
                if self.has_carry(i, Item::Food, 1) {
                    self.drop_carry(i);
                    self.actors[i].hunger = self.actors[i].hunger.saturating_sub(56);
                    self.meals = self.meals.saturating_add(1);
                    if let Some(state) = &mut self.village {
                        state.opening.note_need(i, 1);
                        state.pantry_plan.meal(i);
                    }
                    if task == Task::SharedEat && i < 2
                        && let Some(state) = &mut self.village && state.opening.shared_serving
                    {
                        let opening = &mut state.opening;
                        opening.shared_eaten |= 1 << i;
                        if opening.shared_eaten == 0b11 {
                            opening.shared_serving = false;
                            opening.shared_until = 0;
                        }
                    }
                    if task == Task::SharedEat && i < 2
                        && let Some(state) = &mut self.village && state.grain.serving
                    {
                        state.grain.eaten |= 1 << i;
                        if state.grain.eaten == 0b11 {
                            state.grain.serving = false;
                            state.grain.ready_until = 0;
                        }
                    }
                }
            }
            Task::EatSavedPortion => {
                let opening = &mut self.village.as_mut().expect("saved portion").opening;
                if i == ActorKind::Ayu.index() && opening.late_meal_until > self.now_ms {
                    opening.late_meal_until = 0;
                    opening.late_meal_eaten = true;
                    self.actors[i].hunger = self.actors[i].hunger.saturating_sub(56);
                    self.meals = self.meals.saturating_add(1);
                    opening.note_need(i, 1);
                    self.village.as_mut().unwrap().pantry_plan.meal(i);
                }
            }
            Task::EatWhileDrying => {
                if i < 2 && self.actors[i].harvest_processing == 3 && self.take_stock(Item::Food, 1) {
                    self.actors[i].hunger = self.actors[i].hunger.saturating_sub(56);
                    self.meals = self.meals.saturating_add(1);
                    let state = self.village.as_mut().expect("drying meal");
                    state.opening.note_need(i, 1);
                    state.pantry_plan.meal(i);
                }
            }
            Task::Drink => {
                let drank = if i == ActorKind::Orange.index() {
                    if self.bowl == 0 {
                        false
                    } else {
                        self.bowl -= 1;
                        true
                    }
                } else {
                    self.take_stock(Item::Water, 1)
                };
                if drank {
                    self.actors[i].thirst = self.actors[i].thirst.saturating_sub(62);
                    if i == ActorKind::Orange.index() {
                        self.drinks = self.drinks.saturating_add(1);
                        if let Some(state) = &mut self.village {
                            state.cat_bowl_dirty = true;
                        }
                    } else if let Some(state) = &mut self.village {
                        state.opening.consumed_break_water(i, self.now_ms);
                        state.opening.note_need(i, 2);
                    }
                }
            }
            Task::DrinkAtWell => {
                if self.well > 0 {
                    self.well -= 1;
                    self.actors[i].thirst = self.actors[i].thirst.saturating_sub(62);
                    if let Some(state) = &mut self.village {
                        state.opening.note_need(i, 2);
                    }
                }
            }
            Task::GetWater | Task::GetHomeWater => {
                if self.actors[i].carried.is_none() && self.well > 0 {
                    let public_well = if self.is_village() {
                        village::poi("market_well")
                    } else {
                        Cell(35, 35)
                    };
                    let pulley = self.actors[i].cell == public_well
                        && self.well >= 4
                        && self.story.improvements.working(Fixture::WellRope);
                    let amount = self.well.min(if pulley { 4 } else { 3 });
                    if pulley {
                        self.story.improvements.used(Fixture::WellRope);
                    }
                    self.well -= amount;
                    self.carry(i, Item::Water, amount);
                }
            }
            Task::FillBowl => {
                if self.actors[i].carried == Some(Item::Water) && self.bowl < BOWL_CAP {
                    self.bowl += 1;
                    self.actors[i].amount -= 1;
                    if self.actors[i].amount == 0 {
                        self.drop_carry(i);
                    }
                }
            }
            Task::Store => {
                let actor = &self.actors[i];
                let amount = actor.amount;
                let seeds = actor.seeds_in_basket;
                if let Some(item) = actor.carried
                    && self.stock.fits(
                        item,
                        amount
                            + if item == Item::Food {
                                self.meal_escrow
                            } else {
                                0
                            },
                    )
                    && (!seeds || self.is_village() || self.stock.fits(Item::Seeds, 1))
                {
                    self.stock.add(item, amount);
                    if item == Item::Crops {
                        self.record_crops_at(amount, actor.cell == self.initial_home_door(0));
                    }
                    if seeds {
                        self.store_recovered_seed();
                    }
                    self.drop_carry(i);
                }
            }
            Task::Cook => {
                let cooked_fish = self.has_carry(i, Item::Fish, 1);
                let portions = if self.has_carry(i, Item::Crops, 3) {
                    7
                } else if self.has_carry(i, Item::Crops, 2) {
                    5
                } else if self.has_carry(i, Item::Fish, 1) {
                    3
                } else {
                    0
                };
                if portions > 0 && self.stock.fits(Item::Food, portions) {
                    if portions == 7 && self.buildings[2].level < 3 {
                        self.story.improvements.used(Fixture::KitchenBatchStove);
                    }
                    self.drop_carry(i);
                    let reserved = i == ActorKind::Xiaohe.index() && self.village.as_mut().is_some_and(|state| {
                        let opening = &mut state.opening;
                        if opening.parallel_work && !opening.late_meal_eaten && opening.late_meal_until == 0 {
                            opening.late_meal_until = self.now_ms + 3 * HOUR;
                            true
                        } else {
                            false
                        }
                    });
                    self.carry(i, Item::Food, portions - u16::from(reserved));
                    self.note_opening_shift(i, true);
                    if let Some(state) = &mut self.village {
                        if cooked_fish {
                            state.soil.fish_cooked();
                        } else {
                            state.soil.scraps();
                            state.grain.note_leaf_before_root(self.now_ms);
                        }
                    }
                    if self.season() == YardSeason::Winter {
                        self.note_seasonal_work(YardSeason::Winter);
                    }
                }
            }
            Task::CookHomeHarvest => {
                let opening = &mut self.village.as_mut().expect("opening meal").opening;
                if opening.harvest_carrier == Some(i as u8)
                    && self.actors[i].carried == Some(Item::Crops)
                    && self.actors[i].amount >= 2 && self.actors[i].harvest_processing == 0
                    && opening.shared_until == 0
                {
                    opening.harvest_carrier = None;
                    opening.shared_until = self.now_ms + DAY_MS;
                    let seeds = self.actors[i].seeds_in_basket;
                    self.drop_carry(i);
                    if seeds {
                        self.store_recovered_seed();
                    }
                    self.carry(i, Item::Food, 5);
                    self.village.as_mut().unwrap().soil.scraps();
                }
            }
            Task::Grain(work) => {
                if work.transports() && !std::mem::take(&mut self.actors[i].moving_box)
                {
                    return;
                }
                use grain::Work;
                if matches!(work, Work::MountMill | Work::DemonstrateMill | Work::ObserveMill | Work::MillWheat
                    | Work::MillIndependently | Work::CookFlour | Work::CookTraineeFlour)
                {
                    let instructor = &self.actors[ActorKind::Ayu.index()];
                    let lesson_here = instructor.task == Some(Task::Grain(Work::DemonstrateMill))
                        && instructor.stage == Stage::Work && instructor.cell == self.building_door(4);
                    let allowed = self.village.as_ref().unwrap().grain.mill_work(i, lesson_here) == Some(work);
                    if !allowed || work == Work::MountMill && !self.has_carry(i, Item::Stone, 1) {
                        return;
                    }
                    if matches!(work, Work::CookFlour | Work::CookTraineeFlour) && !self.stock.fits(Item::Food, 3) {
                        return;
                    }
                    if work == Work::MountMill { self.drop_carry(i); }
                    let portions = self.village.as_mut().unwrap().grain.mill_commit(work);
                    if portions > 0 { self.carry(i, Item::Food, portions); }
                    return;
                }
                let weather = self.weather();
                if work == Work::RecoverRootSeed {
                    if !self.has_carry(i, Item::Seeds, 1) { return; }
                    self.drop_carry(i);
                }
                let state = self.village.as_mut().expect("village grain");
                let field = &mut self.plots[usize::from(grain::FIELD)];
                let portions = state.grain.commit(work, self.now_ms, weather, field);
                if portions > 0 {
                    self.carry(i, Item::Food, portions);
                }
            }
            Task::Soil(work) => {
                let state = self.village.as_mut().expect("village soil");
                state.soil.commit(work, self.now_ms, &self.plots, state.flooded_plots, &mut self.stock);
            }
            Task::WorkshopStory(work) => self.commit_workshop_story(i, work),
            Task::Renovate(work) => self.commit_renovation(i, work),
            Task::RoadStory(work) => self.commit_road_story(i,work),
            Task::MarketStory(work) => self.commit_market_story(i,work),
            Task::Memory(work) => self.commit_memory(i,work),
            Task::Preserve(work) => {
                use preserves::Work;
                if work == Work::RepairRack {
                    if self.has_carry(i, Item::Wood, 1) && self.story.improvements.restore_use(Fixture::PantryDryRack) {
                        self.drop_carry(i);
                    }
                    return;
                }
                let rack = self.story.improvements.working(Fixture::PantryDryRack);
                let expected = self.village.as_ref().unwrap().preserves.work(self.now_ms, rack);
                if expected != Some(work) || work == Work::LoadRack && !self.has_carry(i, Item::Crops, preserves::CROP_UNITS)
                    || work == Work::CookBatch && !self.stock.fits(Item::Food, 4)
                {
                    return;
                }
                if work == Work::LoadRack { self.drop_carry(i); }
                if matches!(work, Work::LoadRack | Work::TurnRack | Work::JarBatch) {
                    self.story.improvements.used(Fixture::PantryDryRack);
                }
                let damp = matches!(self.weather(), YardWeather::Rain | YardWeather::Snow);
                self.village.as_mut().unwrap().preserves.commit(work, self.now_ms, damp);
                if work == Work::CookBatch { self.carry(i, Item::Food, 4); }
            }
            Task::Clear(id)
            | Task::Till(id)
            | Task::Sow(id)
            | Task::Water(id)
            | Task::Harvest(id)
            | Task::Drain(id) => {
                self.commit_plot(i, usize::from(id), task);
            }
            Task::FellTree(id) => {
                let tree = &mut self.trees[usize::from(id)];
                if tree.stage == TreeStage::Standing && !tree.protected {
                    tree.stage = TreeStage::Felled;
                }
            }
            Task::TakeTimber(id) => {
                let tree = &mut self.trees[usize::from(id)];
                if tree.stage == TreeStage::Felled && self.actors[i].carried.is_none() {
                    tree.stage = TreeStage::Stump;
                    let timber = std::mem::take(&mut tree.timber);
                    let tracked = self.village.as_ref().is_some_and(|state| state.workshop_story.tree == Some(id)
                        && !state.workshop_story.timber);
                    if let Some(village) = &mut self.village && !tracked {
                        village.tree_due[usize::from(id)] = self.now_ms + 21 * DAY_MS;
                    }
                    self.carry(i, Item::Wood, timber);
                    if tracked {
                        self.actors[i].traced_wood = Some(id);
                        self.village.as_mut().unwrap().workshop_story.timber = true;
                    }
                }
            }
            Task::Build(id) => {
                let index = usize::from(id);
                let (item, amount) = self.build_input(index);
                let old_level = self.buildings[index].level;
                let can_work = old_level == 1
                    || self
                        .structural_candidate()
                        .is_some_and(|(site, _, _)| site == index);
                if old_level < MAX_STRUCTURE_LEVEL
                    && can_work
                    && self.actors[i].cell == self.building_door(index)
                    && self.has_carry(i, item, amount)
                {
                    self.drop_carry(i);
                    let building = &mut self.buildings[index];
                    building.step += 1;
                    if building.step == 3 {
                        building.level += 1;
                        building.step = 0;
                    }
                    building.progress = building.step * 33;
                    if old_level >= 2 {
                        let interval = if building.step == 0 {
                            14
                        } else if old_level == 2 {
                            2
                        } else {
                            4
                        };
                        self.story.improvements.structure_completed_stage(
                            index,
                            self.now_ms.saturating_add(interval * DAY_MS),
                        );
                    }
                }
            }
            Task::Improve(order) => {
                let (material, amount) = order.material();
                if self.improvement_unlocked(order.fixture)
                    && self.actors[i].cell == self.improvement_site(order.fixture)
                    && self.has_carry(i, chapter_item(material), amount)
                    && self.story.improvements.accepts(order, self.now_ms)
                {
                    self.drop_carry(i);
                    self.story.improvements.commit(order, self.now_ms);
                }
            }
            Task::BuildHome(order) => {
                if !self
                    .story
                    .housing
                    .accepts_for(order, self.now_ms, self.is_village())
                    || !self.housing_site_legal(order)
                {
                    self.actors[i].moving_box = false;
                    return;
                }
                if order.phase == HousingPhase::MoveIn {
                    if !self.actors[i].moving_box
                        || self.story.housing.target_occupant(order) != Some(i)
                    {
                        self.actors[i].moving_box = false;
                        return;
                    }
                    self.actors[i].moving_box = false;
                } else if let Some((material, amount)) = order.phase.material(order.plan) {
                    if !self.has_carry(i, chapter_item(material), amount) {
                        return;
                    }
                    self.drop_carry(i);
                }
                self.story.housing.commit(order, i, self.now_ms);
                if order.phase == HousingPhase::MoveIn
                    && self
                        .story
                        .housing
                        .order_for_person(i)
                        .is_some_and(|home| home.lot == order.lot)
                    && let Some(village) = &mut self.village
                    && order.plan == HousePlan::Shared
                {
                    village.grant("late_shared_home");
                }
            }
            Task::Develop(id) => self.development_commit(i, id),
            Task::FreightBridge => {
                let Some(mut state) = self.village.clone() else {
                    return;
                };
                if state.freight_inspected {
                    return;
                }
                if !state.freight_material_paid {
                    if !self.has_carry(i, Item::Stone, 3) {
                        return;
                    }
                    self.drop_carry(i);
                    state.freight_material_paid = true;
                }
                state.freight_shifts += 1;
                if state.freight_shifts >= 3 {
                    state.freight_inspected = true;
                    state.grant("main_bridge_freight");
                }
                self.village = Some(state);
            }
            Task::ProcessHarvest(phase) => {
                if self.actors[i].harvest_processing != phase
                    || self.actors[i].carried != Some(Item::Crops)
                {
                    return;
                }
                if phase == 1 && !self.take_stock(Item::Water, 1) {
                    if self.well == 0 {
                        return;
                    }
                    self.well -= 1;
                }
                if phase == 3
                    && (matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
                        || !(6..18).contains(&(self.minute_of_day() / 60)))
                {
                    return;
                }
                self.actors[i].harvest_processing = if phase < 4 { phase + 1 } else { 0 };
            }
            Task::CareOrchard(tree) => {
                let tree =
                    &mut self.village.as_mut().expect("orchard state").orchard[usize::from(tree)];
                tree.last_care_day = self.now_ms / DAY_MS;
                tree.health = tree.health.saturating_add(7).min(100);
                if tree.fruit > 0 && self.actors[i].carried.is_none() {
                    let yield_units = tree.fruit.min(3);
                    tree.fruit -= yield_units;
                    if tree.fruit == 0 {
                        tree.phase = OrchardPhase::AfterHarvest;
                    }
                    self.carry(i, Item::Crops, u16::from(yield_units));
                    self.actors[i].harvest_processing = 1;
                    self.harvested = self.harvested.saturating_add(u64::from(yield_units));
                }
            }
            Task::Trade(item, amount) => {
                let reward = if item == Item::Wood && matches!(amount, 6 | 7) {
                    2
                } else {
                    1
                };
                if self.story.next_trade <= self.now_ms
                    && self.stock.count(Item::Food) >= 6
                    && self.has_carry(i, Item::Crops, amount)
                    && self.stock.fits(item, reward)
                {
                    if item == Item::Wood && amount == 6 {
                        self.story.improvements.used(Fixture::StallStockBins);
                    }
                    self.drop_carry(i);
                    self.carry(i, item, reward);
                    self.story.trades = self.story.trades.saturating_add(1);
                    self.story.next_trade = self.now_ms + DAY_MS / 2;
                }
            }
            Task::Garden => {
                self.story.tended_garden = self.story.tended_garden.saturating_add(1);
                let planter = self.story.improvements.working(Fixture::GardenPlanter);
                if planter {
                    self.story.improvements.used(Fixture::GardenPlanter);
                }
                self.story.next_garden =
                    self.now_ms + if planter { 5 * DAY_MS } else { 3 * DAY_MS };
            }
            Task::Fish => {
                self.casts = self.casts.saturating_add(1);
                if let Some(state) = &mut self.village
                    && self.actors[i].tool == Some(Tool::FishingRod)
                    && state.opening.rod_borrowed
                {
                    state.opening.rod_used = true;
                    state.workshop_story.rod_wear = state.workshop_story.rod_wear.saturating_add(1).min(3);
                    if state.workshop_story.rod_oiled {
                        state.workshop_story.rod_tested = true;
                    }
                }
                let bite = (self.now_ms / MINUTE).wrapping_mul(23) ^ self.casts.wrapping_mul(17);
                let caught = self.fish_in_pond > 2 && bite % 5 < 3 && self.actors[i].carried.is_none();
                if caught {
                    self.fish_in_pond -= 1;
                    self.carry(i, Item::Fish, 1);
                }
                if let Some(state) = &mut self.village && state.opening.completed("Q03") {
                    state.soil.fishing(caught);
                }
                if self.season() == YardSeason::Summer && caught {
                    self.note_seasonal_work(YardSeason::Summer);
                }
            }
            Task::UnpackHome => {
                if self.actors[i].moving_box && i < 2 {
                    self.actors[i].moving_box = false;
                    self.village.as_mut().expect("opening task").opening.boxes_stowed |= 1 << i;
                }
            }
            Task::RackRod => {
                if self.actors[i].tool == Some(Tool::FishingRod) {
                    self.actors[i].tool = None;
                    self.village.as_mut().expect("opening task").opening.rod_on_rack = true;
                }
            }
            Task::ServeCat => {
                let opening = &self.village.as_ref().expect("opening task").opening;
                if !opening.cat_food && self.has_carry(i, Item::Food, 1) {
                    self.drop_carry(i);
                    self.village.as_mut().expect("opening task").opening.cat_food = true;
                }
            }
            Task::CatEat => {
                let opening = &mut self.village.as_mut().expect("village cat meal").opening;
                let served = opening.cat_food;
                if served {
                    opening.cat_food = false;
                    opening.cat_ate = true;
                    self.actors[i].hunger = self.actors[i].hunger.saturating_sub(56);
                }
            }
            Task::ReturnTool => {
                if let Some(state) = &mut self.village
                    && self.actors[i].tool == Some(Tool::FishingRod) && state.opening.rod_used
                {
                    state.opening.rod_returned = true;
                    state.workshop_story.rod_rehung |= state.workshop_story.rod_tested;
                }
                self.actors[i].tool = None;
            }
            Task::ChangeClothes(outfit) => self.actors[i].outfit = outfit,
            Task::Explore(spot) => {
                let count = if self.is_village() { village::VILLAGE_CAT_SPOTS } else { CAT_SPOTS.len() };
                self.next_cat_spot = (spot + 1) % count as u8;
                if i == ActorKind::Orange.index() && (4..=5).contains(&spot)
                    && let Some(state) = &mut self.village && state.opening.completed("Q01")
                {
                    state.opening.cat_visited |= 1 << (spot - 4);
                }
            }
            Task::External(_) | Task::ReturnHome => (),
        }
        let effort = match task {
            Task::FellTree(_) => 18,
            Task::Build(_)
            | Task::Improve(_)
            | Task::BuildHome(_)
            | Task::Clear(_)
            | Task::Till(_)
            | Task::Develop(_)
            | Task::FreightBridge => 14,
            Task::Rest
            | Task::CourtyardBreak
            | Task::Sleep
            | Task::Eat
            | Task::SharedEat
            | Task::EatSavedPortion
            | Task::Drink
            | Task::DrinkAtWell
            | Task::External(_)
            | Task::ReturnHome
            | Task::ReturnTool
            | Task::ChangeClothes(_) => 0,
            Task::Explore(_) => 14,
            _ => 5,
        };
        self.actors[i].stamina = self.actors[i].stamina.saturating_sub(effort);
    }

    fn commit_plot(&mut self, i: usize, id: usize, task: Task) {
        if let Some(state) = &mut self.village {
            let bit = 1 << id;
            if task == Task::Drain(id as u8) {
                if state.flooded_plots & bit != 0 {
                    state.soil.drained(id as u8);
                }
                state.flooded_plots &= !bit;
                return;
            }
            if state.flooded_plots & bit != 0 {
                return;
            }
        }
        if id == usize::from(grain::FIELD) && self.is_village() && task == Task::Water(grain::FIELD) {
            if self.has_carry(i, Item::Water, 1)
                && self.village.as_ref().unwrap().grain.can_water(self.now_ms / DAY_MS)
            {
                self.drop_carry(i);
                self.village.as_mut().unwrap().grain.water();
            }
            return;
        }
        let stage = self.plots[id].stage;
        match (stage, task) {
            (PlotStage::Grass, Task::Clear(_)) => {
                self.plots[id].stage = PlotStage::Cleared;
                if let Some(state) = &mut self.village { state.soil.leaves(); }
            }
            (PlotStage::Cleared, Task::Till(_)) => self.plots[id].stage = PlotStage::Tilled,
            (PlotStage::Tilled, Task::Sow(_)) if self.has_carry(i, Item::Seeds, 1) => {
                self.drop_carry(i);
                self.plots[id].stage = PlotStage::Seed;
                if let Some(state) = &mut self.village {
                    state.opening.sown |= 1 << id;
                }
                if self.season() == YardSeason::Spring {
                    self.note_seasonal_work(YardSeason::Spring);
                }
            }
            (PlotStage::Seed | PlotStage::Growing, Task::Water(_))
                if !self.plots[id].watered && self.has_carry(i, Item::Water, 1) =>
            {
                self.drop_carry(i);
                self.plots[id].watered = true;
                let growth = if self.buildings[0].level >= 4 {
                    10 * MINUTE
                } else {
                    0
                };
                let growth = if let Some(state) = &mut self.village {
                    state.soil.water(id as u8, self.plots.iter().enumerate().any(|(index, plot)| index != id && plot.watered));
                    growth + if state.soil.fertilizer(id) { 5 * MINUTE } else { 0 }
                } else { growth };
                self.plots[id].due = self.now_ms
                    + (if stage == PlotStage::Seed {
                        55 * MINUTE
                    } else {
                        105 * MINUTE
                    })
                    - growth;
                self.note_opening_shift(i, false);
            }
            (PlotStage::Mature, Task::Harvest(_)) if self.actors[i].carried.is_none() => {
                self.plots[id].stage = PlotStage::Tilled;
                self.carry(i, Item::Crops, 3);
                if let Some(state) = &mut self.village {
                    self.actors[i].harvest_processing = 1;
                    let opening = &mut state.opening;
                    if opening.sown & (1 << id) != 0 && opening.completed("Q02")
                        && !opening.completed("Q03") && opening.harvest_carrier.is_none()
                        && opening.shared_until == 0
                    {
                        opening.harvest_carrier = Some(i as u8);
                    }
                    opening.sown &= !(1 << id);
                }
                self.actors[i].seeds_in_basket = true;
                self.story.improvements.used(Fixture::WestSeedDrawer);
                self.harvested = self.harvested.saturating_add(3);
                self.note_opening_shift(i, false);
                if self.season() == YardSeason::Autumn {
                    self.note_seasonal_work(YardSeason::Autumn);
                }
            }
            _ => (),
        }
    }

    fn commit_road_story(&mut self, i: usize, work: road_story::Work) {
        use road_story::Work;
        let state = &self.village.as_ref().unwrap().road_story;
        if state.task(i,self.now_ms) != Some(work) { return; }
        if work == Work::Approve && !road_story::SHOULDER.into_iter().all(|tile| self.valid_world_cell(Cell::from(tile))) { return; }
        if matches!(work,Work::Build|Work::CarryTest) && matches!(self.weather(),YardWeather::Rain|YardWeather::Snow) { return; }
        let mut load = state.loaded;
        if let Some((item,qty)) = state.cost() {
            if !self.has_carry(i,item,qty) { return; }
            if matches!(work,Work::CarryTest|Work::PackOrder) { load = Some((item,qty)); }
            if work == Work::Build { self.drop_carry(i); }
        }
        if matches!(work,Work::ReturnTest|Work::DeliverOrder) {
            let Some((item,qty)) = load else { return; };
            if !self.has_carry(i,item,qty) { return; }
            if work == Work::ReturnTest {
                if !self.stock.add(item,qty) { return; }
                self.record_crops(qty);
                load = None;
            }
            self.drop_carry(i);
        }
        let state = &mut self.village.as_mut().unwrap().road_story;
        state.loaded = load;
        state.commit(self.now_ms);
    }

    fn commit_market_story(&mut self, i: usize, work: road_story::MarketWork) {
        use road_story::MarketWork as Work;
        if self.current_market_work(i) != Some(work) { return; }
        if matches!(work, Work::Survey | Work::Walk)
            && !road_story::MARKET_LANE.into_iter().all(|at| self.valid_world_cell(Cell::from(at)))
        {
            return;
        }
        if matches!(work, Work::Base | Work::Pave | Work::Drain)
            && matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            return;
        }
        if work == Work::MoveLamp {
            let index = usize::from(self.market_lamps_stage());
            let tile = road_story::MARKET_LAMPS[index];
            if index >= 3 && (self.external_occupied.contains(tile)
                || self.actors.iter().any(|actor| actor.cell.tile() == tile
                    || actor.route.iter().skip(actor.route_next).any(|step| step.tile() == tile))) {
                return;
            }
        }
        if work == Work::CarryMill && !std::mem::take(&mut self.actors[i].moving_box) { return; }
        if work == Work::InspectMill && !self.village.as_mut().unwrap().grain.produce_community_flour() { return; }
        if let Some((item, qty)) = road_story::Market::cost(work) {
            if !self.has_carry(i, item, qty) { return; }
            self.drop_carry(i);
        }
        let state=self.village.as_mut().unwrap();
        state.market_story.commit(work, self.now_ms, state.social.last_market_cleanup_day);
    }

    fn commit_memory(&mut self,i:usize,work:memories::Work) {
        use memories::Work;
        if work == Work::RenewCorner {
            if i == ActorKind::Ayu.index() && self.town_allows(TownEra::Heritage)
                && self.town_era() == TownEra::Mature && self.has_carry(i, Item::Wood, 1)
            {
                self.drop_carry(i);
                self.village.as_mut().unwrap().calendar.as_mut().unwrap().renewed = true;
            }
            return;
        }
        let state=self.village.as_ref().expect("memory task");
        let revisit=self.now_ms/DAY_MS>=700 && state.social.matured_village && state.social.familiar_visitor;
        if state.memories.task(i,self.now_ms,revisit) != Some(work) { return; }
        if work == Work::CareKeepsake
            && !WILLOW_VILLAGE.scenic.iter().any(|item| item.id == "scenic.home_west_hedge.tree.8_18"
                && self.phase_mask().contains(item.phase))
        {
            return;
        }
        let amount = self.input(Task::Memory(work));
        if amount.is_some_and(|(item,qty)| !self.has_carry(i,item,qty)) { return; }
        if amount.is_some() { self.drop_carry(i); }
        let readable=[1u16,3,4,12,18,20,21,23,24,32,33,34];
        let other=if matches!(work,Work::Gather|Work::ReadTogether(_)) {
            let paired=if work==Work::Gather {Work::Gather}else{Work::ReadTogether((1-i) as u8)};
            let other=self.actors[1-i].cell;
            other == self.site(1-i,Task::Memory(paired))
                && self.actors[1-i].task==Some(Task::Memory(paired))
                && self.actors[1-i].stage==Stage::Work
        } else {false};
        let pages=if matches!(work,Work::Review(_)) {
            readable.map(|chapter| {
                let id=format!("Q{chapter:02}");
                if self.village_chapter_completed(&id) {chapter} else {0}
            })
        } else { [0;12] };
        let state=&mut self.village.as_mut().unwrap().memories;
        match work {
            Work::RenewCorner => unreachable!("handled by calendar renewal"),
            Work::Review(owner) => {
                for (stored,page) in state.notebook_pages.iter_mut().zip(pages) { if page!=0 { *stored=page; } }
                state.pages_reviewed|=1<<owner;
            }
            Work::ReadTogether(_) if other=>state.pages_read=0b11,
            Work::NoteReturn=>state.familiar_return=true,
            Work::CareKeepsake=>state.polished=true,
            Work::Celebrate(mark)=>state.gathering=Some(mark),
            Work::Gather if other=>{
                let mark=state.gathering.take().unwrap();
                state.celebrated|=1<<mark;
                state.polished=false;
                for actor in &mut self.actors[..2] { actor.hunger=actor.hunger.saturating_sub(44); }
                self.meals=self.meals.saturating_add(2);
            }
            _=>{}
        }
    }

    fn note_seasonal_work(&mut self, season: YardSeason) {
        if let Some(state) = &mut self.village {
            state.pantry_plan.season_event(season);
        }
    }

    fn commit_renovation(&mut self, i: usize, work: renovation::Work) {
        use renovation::Work;
        let state = &self.village.as_ref().expect("village renovation").renovation;
        if self.common_renovation_task(i) == Some(work) {
            if matches!(work, Work::SurveyPath | Work::TestPath)
                && !renovation::PATH.into_iter().all(|tile| self.valid_world_cell(Cell::from(tile)))
            {
                return;
            }
            if matches!(work, Work::LayPath | Work::BuildCorner | Work::PrepareGuests) {
                let Some((item, qty)) = self.input(Task::Renovate(work)) else { return; };
                if !self.has_carry(i, item, qty) { return; }
                self.drop_carry(i);
            }
            if work == Work::CloseGuestTrial {
                if !self.stock_can_accept(Item::Food, 2) { return; }
                self.stock.add(Item::Food, 2);
            }
            let state = &mut self.village.as_mut().unwrap().renovation;
            state.commit(work, self.now_ms);
            if work == Work::RestAtCorner {
                state.park_rest(i, self.now_ms / DAY_MS);
                self.actors[i].stamina = self.actors[i].stamina.saturating_add(24).min(100);
            }
            return;
        }
        let carried = !work.transports() || std::mem::take(&mut self.actors[i].moving_box);
        if !carried || state.task(i, self.now_ms) != Some(work) { return; }
        if matches!(work, Work::Repair(_)) {
            let (item, qty) = state.repair_cost();
            if !self.has_carry(i, item, qty) || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow) {
                return;
            }
            self.drop_carry(i);
        }
        let cleared_empty_room = matches!(work, Work::MoveOut(owner)
            if self.story.housing.order_for_person(usize::from(owner)).is_some());
        let renovation = &mut self.village.as_mut().unwrap().renovation;
        renovation.commit(work, self.now_ms);
        if cleared_empty_room { renovation.displaced_room_empty = true; }
        if let Work::TryCorner(owner) = work {
            let fixture = if owner == 0 { Fixture::WestSeedDrawer } else { Fixture::EastToolRack };
            self.story.improvements.restore_use(fixture);
        }
    }

    fn commit_workshop_story(&mut self, i: usize, work: workshop_story::Work) {
        use workshop_story::Work;
        let state = &mut self.village.as_mut().expect("workshop story").workshop_story;
        match work {
            Work::OilRod if self.actors[i].tool == Some(Tool::FishingRod) && state.rod_wear >= 2 => {
                state.rod_wear = 0;
                state.rod_oiled = true;
            }
            Work::ChooseTree if state.tree.is_none() => {
                state.tree = self.trees.iter().enumerate().rev().find_map(|(id, tree)| {
                    (!tree.protected && tree.stage == TreeStage::Standing).then_some(id as u8)
                });
            }
            Work::MillBeam if state.timber && !state.milled && state.tree == self.actors[i].traced_wood
                && self.actors[i].carried == Some(Item::Wood) && self.actors[i].amount == 3 => {
                state.milled = true;
                self.drop_carry(i);
                // Two units become part of the bench, one goes back as offcuts.
                self.stock.add(Item::Wood, 1);
            }
            Work::FitBench if state.milled && !state.built => {
                state.built = true;
            }
            Work::Replant if state.built && !state.planted => {
                let Some(tree) = state.tree.map(usize::from) else { return; };
                if self.trees[tree].stage == TreeStage::Stump {
                    state.planted = true;
                    self.village.as_mut().unwrap().tree_due[tree] = self.now_ms + 21 * DAY_MS;
                }
            }
            work @ (Work::AcceptOrder | Work::CutOrder | Work::FinishOrder | Work::DeliverOrder | Work::UseOrder)
                if state.order_work(i, self.now_ms) == Some(work) =>
            {
                match work {
                    Work::AcceptOrder if self.actors[i].carried == Some(Item::Wood) && self.actors[i].amount == 2 => {
                        state.order = Some((0, self.now_ms));
                        self.drop_carry(i);
                    }
                    Work::CutOrder => state.order = Some((1, self.now_ms)),
                    Work::FinishOrder => state.order = Some((2, self.now_ms)),
                    Work::DeliverOrder if std::mem::take(&mut self.actors[i].moving_box) => {
                        state.order = Some((3, self.now_ms));
                    }
                    Work::UseOrder => state.order_used = true,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Validate before running a deserialized state. Plot and facility IDs are their array slots.
    pub fn validate(&self) -> bool {
        let village = self.is_village();
        let expected_plots = if village {
            WILLOW_VILLAGE.crops.len()
        } else {
            PLOTS.len()
        };
        if self.plots.len() != expected_plots
            || self.village.as_ref().is_some_and(|state| {
                state.revision != WILLOW_VILLAGE.map_revision
                    || (state.region_flags & !PhaseMask::ALL.bits()) != 0
                    || (state.region_flags & PhaseMask::DAY_ONE.bits()) == 0
                    || state.freight_shifts > 3
                    || state.flooded_plots >> 30 != 0
                    || !state.opening.validate(self.now_ms)
                    || !state.grain.validate(self.now_ms)
                    || !state.soil.validate(self.now_ms)
                    || !state.workshop_story.validate(self.now_ms)
                    || !state.pantry_plan.validate(self.now_ms)
                    || !state.preserves.validate(self.now_ms)
                    || !state.renovation.validate(self.now_ms)
                    || !state.road_story.validate(self.now_ms)
                    || !state.market_story.validate(self.now_ms)
                    || !state.memories.validate()
                    || state.calendar.as_ref().is_some_and(|calendar| !calendar.validate(self.target_ms, self.last_wall_ms))
                    || state.opening.harvest_carrier.is_some_and(|id| {
                        self.actors.get(usize::from(id)).is_none_or(|actor| actor.carried != Some(Item::Crops))
                    })
                    || state
                        .tracked_day
                        .is_some_and(|day| day > self.now_ms / DAY_MS)
                    || state
                        .awake_ms
                        .iter()
                        .zip(&state.home_ms)
                        .any(|(awake, home)| {
                            (0..2).any(|i| u64::from(awake[i]) > DAY_MS || home[i] > awake[i])
                        })
                    || state.orchard.iter().any(|tree| {
                        tree.health > 100
                            || tree.fruit > 7
                            || tree.last_care_day > self.now_ms / DAY_MS
                            || tree.phase != OrchardPhase::Ripe && tree.fruit != 0
                    })
                    || state.freight_shifts != 0 && !state.freight_material_paid
                    || state.imported_pedestrian_bridge
                        && state.freight_inspected
                        && state.freight_shifts != 3
                    || state.freight_inspected
                        != state
                            .phases()
                            .contains(WILLOW_VILLAGE.phase("main_bridge_freight").unwrap())
                    || state.tree_due.iter().zip(&self.trees).any(|(&due, tree)| {
                        due != 0 && (due < self.now_ms || tree.stage != TreeStage::Stump)
                    })
            })
            || self.now_ms < START_MS
            || self.now_ms > self.target_ms
            || self.target_ms > u64::MAX - DAY_MS
            || self.clock_fraction > 1
            || self.next_needs < self.now_ms
            || self.next_well < self.now_ms
            || self.next_fish < self.now_ms
            || self.well > WELL_CAP
            || self.bowl > BOWL_CAP
            || self.fish_in_pond > 8
            || usize::from(self.next_cat_spot) >= if village { village::VILLAGE_CAT_SPOTS } else { CAT_SPOTS.len() }
            || self.meal_escrow > 2
            || self.meal_until != 0 && self.meal_until < self.now_ms
            || self.meal_until == 0 && self.meal_escrow != 0
            || self
                .stock
                .count(Item::Food)
                .saturating_add(self.meal_escrow)
                > STOCK_CAP
            || self.stock.0.iter().any(|&count| count > STOCK_CAP)
            || !self.story.validate_for(self.now_ms, village)
            || self
                .buildings
                .iter()
                .filter(|building| building.level >= 2 && building.step > 0)
                .count()
                > 1
            || self.story.improvements.active().is_some_and(|order| {
                let rack_exception = village && order.fixture == Fixture::PantryDryRack;
                self.buildings.iter().enumerate().any(|(site, building)| {
                    building.level >= 2 && building.step > 0
                        && (!rack_exception || site == usize::from(order.fixture.site()))
                })
            })
            || self
                .story
                .improvements
                .installed()
                .any(|status| !self.improvement_unlocked(status.fixture))
            || self
                .story
                .improvements
                .active()
                .is_some_and(|order| !self.improvement_unlocked(order.fixture))
            || !self
                .crop_batches
                .validate(self.stock.count(Item::Crops), self.now_ms)
        {
            return false;
        }
        for (i, actor) in self.actors.iter().enumerate() {
            if actor.kind.index() != i
                || !self.valid_world_cell(actor.cell)
                || actor.since > self.now_ms
                || actor.until < self.now_ms
                || actor.until.saturating_sub(actor.since) > 4 * HOUR
                || actor.route.len()
                    > if village {
                        crate::map_nav::MAX_SEGMENT_EDGES
                    } else {
                        MAX_ROUTE
                    }
                || actor.route_next > actor.route.len()
                || actor.task.is_some_and(|task| !task.valid_for(village))
                || (i == 2 && matches!(actor.task, Some(Task::UnpackHome | Task::RackRod | Task::ServeCat | Task::CookHomeHarvest | Task::CourtyardBreak)))
                || (i < 2 && actor.task == Some(Task::CatEat))
                || (i != ActorKind::Ayu.index() && actor.task == Some(Task::EatSavedPortion))
                || (i == 2 && actor.task == Some(Task::EatWhileDrying))
                || (!village
                    && (actor.route_goal.is_some()
                        || actor.waiting_edge
                        || actor.waited_edges != 0))
                || (actor.stage == Stage::Idle) != actor.task.is_none()
                || actor.stamina > 100
                || actor.hunger > 100
                || actor.thirst > 100
                || actor.amount > 7
                || actor.external_load && (!village || !matches!(actor.task, Some(Task::External(_))))
                || actor.harvest_processing > 4
                || actor.harvest_processing != 0 && (!village || actor.carried != Some(Item::Crops))
                || actor.avoid_occupied
                    && (!village
                        || actor.route_goal.is_none()
                        || !matches!(actor.stage, Stage::WalkForTool | Stage::WalkToSite)
                        || actor.route.len() > 1)
                || actor.passing.is_some_and(|passing| {
                    !village || i >= 2 || passing.partner == actor.kind || passing.partner == ActorKind::Orange
                        || !actor.avoid_occupied || actor.waiting_edge
                        || passing.approach.distance(passing.entrance) != 1
                        || !self.valid_world_cell(passing.approach) || !self.valid_world_cell(passing.entrance)
                        || passing.expires < self.now_ms || passing.expires > self.now_ms + PASS_CLEAR_LIMIT
                })
                || actor.carried.is_some() != (actor.amount > 0)
                || actor.traced_wood.is_some_and(|id| !village || actor.carried != Some(Item::Wood)
                    || actor.amount != 3 || self.village.as_ref().unwrap().workshop_story.tree != Some(id))
                || actor.seeds_in_basket && actor.carried != Some(Item::Crops)
                || (actor.moving_box
                    && (actor.carried.is_some()
                        || !(matches!(actor.task, Some(Task::BuildHome(order)) if order.phase == HousingPhase::MoveIn)
                            || matches!(actor.task, Some(Task::UnpackHome | Task::WorkshopStory(workshop_story::Work::DeliverOrder))) && i < 2
                            || matches!(actor.task, Some(Task::Renovate(work)) if work.transports()) && i < 2
                            || actor.task == Some(Task::MarketStory(road_story::MarketWork::CarryMill)) && i == 1
                            || matches!(actor.task, Some(Task::Grain(work)) if work.transports()) && i < 2)))
                || actor.inside
                    != (actor.stage == Stage::Work
                        && actor.task.is_some_and(|task| self.work_is_indoors(i, task)))
                || actor.stage == Stage::Meet
                    && (actor.task != Some(Task::SharedEat) || self.meal_until == 0)
                || self.meal_until != 0 && i < 2 && actor.task != Some(Task::SharedEat)
                || actor.outside
                    && actor
                        .task
                        .is_some_and(|task| !matches!(task, Task::External(_)))
            {
                return false;
            }
            let walking = matches!(actor.stage, Stage::WalkForTool | Stage::WalkToSite);
            let mut previous = actor.cell;
            for &cell in &actor.route[actor.route_next..] {
                if !walking || !self.valid_world_cell(cell) || previous.distance(cell) != 1 {
                    return false;
                }
                previous = cell;
            }
            if actor.waiting_edge
                && (!walking
                    || actor.route.get(actor.route_next).is_none()
                    || actor.waited_edges == 0)
            {
                return false;
            }
            if village
                && walking
                && !actor.waiting_edge
                && actor.route.get(actor.route_next).is_some()
                && !movement::valid_travel_edge(actor.since, actor.until)
            {
                return false;
            }
            if walking {
                let mut target = actor
                    .task
                    .map_or(self.pantry(), |task| self.tool_pickup(i, task));
                if actor.stage == Stage::WalkToSite {
                    let Some(task) = actor.task else { return false };
                    target = self.site(i, task);
                }
                if village
                    && actor
                        .route_goal
                        .is_some_and(|goal| goal != target || !self.valid_world_cell(goal))
                {
                    return false;
                }
                if actor.route_goal.is_none()
                    && previous != target
                    && !(actor.stage == Stage::WalkForTool
                        && matches!(previous, PANTRY | Cell(24, 14))
                        && !matches!(actor.task, Some(Task::BuildHome(order)) if order.phase == HousingPhase::MoveIn))
                    && !matches!(
                        actor.task,
                        Some(
                            Task::External(_)
                                | Task::Rest
                                | Task::Trade(..)
                                | Task::GetWater
                                | Task::Store
                                | Task::ReturnTool
                        )
                    )
                {
                    return false;
                }
            } else if !actor.route.is_empty() || actor.route_next != 0 || actor.route_goal.is_some()
            {
                return false;
            }
            let mut expected = actor.clone();
            update_position(&mut expected, self.now_ms);
            if expected.position != actor.position
                || actor.tool.is_some_and(|tool| {
                    self.actors[..i]
                        .iter()
                        .any(|other| other.tool == Some(tool))
                })
            {
                return false;
            }
        }
        self.plots.iter().enumerate().all(|(i, plot)| {
            let cell = if village {
                let rect = WILLOW_VILLAGE.crops[i].rect;
                Cell(rect.x, rect.y)
            } else {
                PLOTS[i]
            };
            plot.position == cell.point()
                && (plot.watered == (plot.due != 0))
                && (plot.due == 0
                    || (plot.due >= self.now_ms
                        && matches!(plot.stage, PlotStage::Seed | PlotStage::Growing)))
        }) && self
            .trees
            .iter()
            .zip(if village {
                village::tree_cells()
            } else {
                TREES
            })
            .enumerate()
            .all(|(i, (tree, cell))| {
                tree.position == cell.point()
                    && tree.protected == protected_tree(i)
                    && (!tree.protected || tree.stage == TreeStage::Standing)
                    && tree.timber == if tree.stage == TreeStage::Stump { 0 } else { 3 }
            })
            && self.buildings.iter().zip(BUILDINGS).enumerate().all(
                |(i, (building, (kind, cell)))| {
                    building.kind == kind
                        && (building.position
                            == if village {
                                village::building(i).point()
                            } else {
                                cell.point()
                            }
                            || (!village
                                && kind == BuildingKind::Workshop
                                && building.position == LEGACY_WORKSHOP_DOOR.point()))
                        && (1..=MAX_STRUCTURE_LEVEL).contains(&building.level)
                        && building.step < 3
                        && building.progress == building.step * 33
                        && (building.level < MAX_STRUCTURE_LEVEL || building.step == 0)
                },
            )
    }
}

#[derive(Clone, Copy)]
struct SightBounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl SightBounds {
    fn from_anchor(x: i32, y: i32, [left, top, right, bottom]: [i32; 4]) -> Self {
        Self {
            left: x - left,
            top: y - top,
            right: x + right,
            bottom: y + bottom,
        }
    }

    fn separated_from(self, other: Self, gap: i32) -> bool {
        self.right + gap <= other.left
            || other.right + gap <= self.left
            || self.bottom + gap <= other.top
            || other.bottom + gap <= self.top
    }
}

fn housing_sight_bounds(order: HousingOrder) -> SightBounds {
    let [x, y] = order.door();
    SightBounds::from_anchor(
        i32::from(x) * TILE_PX + TILE_PX / 2,
        i32::from(y) * TILE_PX,
        order.plan.sight_extents(),
    )
}

fn housing_sight_clear_of_buildings(order: HousingOrder, workshop: Cell) -> bool {
    // Conservative union of the embedded 2× stage/level atlas frames at each door anchor.
    const EXTENTS: [[i32; 4]; BUILDINGS.len()] = [
        [24, 50, 32, 4],
        [24, 51, 34, 4],
        [22, 46, 30, 4],
        [25, 47, 26, 4],
        [16, 31, 19, 4],
    ];
    let bounds = housing_sight_bounds(order);
    BUILDINGS
        .iter()
        .zip(EXTENTS)
        .all(|(&(kind, door), extent)| {
            let Cell(x, y) = if kind == BuildingKind::Workshop {
                workshop
            } else {
                door
            };
            let baseline = i32::from(y) * TILE_PX
                - if kind == BuildingKind::Kitchen {
                    TILE_PX
                } else {
                    0
                };
            bounds.separated_from(
                SightBounds::from_anchor(i32::from(x) * TILE_PX + TILE_PX / 2, baseline, extent),
                8,
            )
        })
}

fn advance_clock(target: u64, elapsed_weight: u128) -> (u64, u8) {
    let sunrise = 5 * HOUR;
    let dusk = 20 * HOUR;
    let daylight = dusk - sunrise;
    let cycle_weight = u128::from(DAY_MS + daylight);
    let offset = (target % DAY_MS + DAY_MS - sunrise) % DAY_MS;
    let initial = u128::from(offset.min(daylight) * 2 + offset.saturating_sub(daylight));
    let weighted = initial + elapsed_weight;
    let within = weighted % cycle_weight;
    let daylight_weight = u128::from(daylight * 2);
    let advance = weighted / cycle_weight * u128::from(DAY_MS)
        + within.min(daylight_weight) / 2
        + within.saturating_sub(daylight_weight)
        - u128::from(offset);
    let carry = u8::from(within < daylight_weight && within % 2 != 0);
    let target = target
        .saturating_add(u64::try_from(advance).unwrap_or(u64::MAX))
        .min(u64::MAX - DAY_MS);
    (target, carry)
}

fn ground(x: i32, y: i32) -> Ground {
    if !(0..MAP_WIDTH as i32).contains(&x) || !(0..MAP_HEIGHT as i32).contains(&y) {
        return Ground::Rock;
    }
    if (9..14).contains(&x) && (18..22).contains(&y)
        || (10..15).contains(&x) && (29..34).contains(&y)
    {
        return Ground::Water;
    }
    if (11..14).contains(&y)
        && ((10..14).contains(&x) || (22..26).contains(&x) || (28..32).contains(&x))
        || (10..12).contains(&y) && (16..20).contains(&x)
    {
        return Ground::Building;
    }
    if ((29..31).contains(&x) && (16..18).contains(&y))
        || ((32..36).contains(&x) && (5..10).contains(&y))
        || ((17..19).contains(&x) && y == 16)
    {
        return Ground::Rock;
    }
    let main_lane = y == 14 && (6..46).contains(&x)
        || y == 15 && ((7..13).contains(&x) || (19..25).contains(&x) || (27..46).contains(&x));
    if main_lane || (6..8).contains(&x) && (15..28).contains(&y) {
        return Ground::Path;
    }
    if let Some(surface) = yard_surface(x, y) {
        return match surface {
            YardSurface::Doorstep | YardSurface::Loading | YardSurface::Bank => Ground::Path,
            YardSurface::Dining | YardSurface::GardenAisle => Ground::Grass,
        };
    }
    if (21..27).contains(&x) && (18..22).contains(&y) {
        return Ground::Soil;
    }
    Ground::Grass
}

fn yard_surface(x: i32, y: i32) -> Option<YardSurface> {
    use YardSurface::*;
    if (y == 14 || y == 15) && matches!(x, 11..=13 | 23..=25)
        || matches!((x, y), (18, 13..=14) | (19, 12..=14) | (17, 13..=14))
    {
        return Some(Doorstep);
    }
    if (y == 14 && (28..=37).contains(&x))
        || matches!((x, y), (30..=33, 15) | (31..=32, 16) | (31..=32, 17))
    {
        return Some(Loading);
    }
    if matches!((x, y), (14..=18, 15) | (15..=16 | 19, 16) | (16..=18, 17)) {
        return Some(Dining);
    }
    if matches!((x, y), (9..=13, 17) | (13..=14, 16) | (8..=9, 18)) {
        return Some(Bank);
    }
    if matches!((x, y), (20..=22 | 26..=28, 17) | (20 | 27, 18..=21) | (22..=26, 20)) {
        return Some(GardenAisle);
    }
    None
}

fn chapter_item(material: Material) -> Item {
    match material {
        Material::Wood => Item::Wood,
        Material::Stone => Item::Stone,
        Material::Rope => Item::Rope,
    }
}

fn workshop_wall(door: Cell, x: i32, y: i32) -> bool {
    y == i32::from(door.1) - 1 && ((i32::from(door.0) - 1)..=i32::from(door.0)).contains(&x)
}

fn weatherstrip(actor: ActorKind) -> Option<Fixture> {
    match actor {
        ActorKind::Xiaohe => Some(Fixture::WestWeatherstrip),
        ActorKind::Ayu => Some(Fixture::EastWeatherstrip),
        ActorKind::Orange => None,
    }
}

fn developed_ground(x: i32, y: i32, finished: u8) -> Ground {
    if (6..8).contains(&x)
        && (23..29).contains(&y)
        && story::site_open(finished, DevelopmentKind::Road)
        || (BRIDGE_X..BRIDGE_X + 2).contains(&x)
            && (28..35).contains(&y)
            && story::site_open(finished, DevelopmentKind::Bridge)
        || (BRIDGE_X..43).contains(&x)
            && y == 34
            && story::site_open(finished, DevelopmentKind::Bridge)
        || (17..22).contains(&x)
            && (33..36).contains(&y)
            && story::site_open(finished, DevelopmentKind::Market)
    {
        return Ground::Path;
    }
    ground(x, y)
}

fn walkable_at(x: i32, y: i32, finished: u8) -> bool {
    matches!(
        developed_ground(x, y, finished),
        Ground::Grass | Ground::Soil | Ground::Path
    ) && x < 48
        && (y < 29
            || (y < 32 && !(10..15).contains(&x))
            || y < 38
                && ((BRIDGE_X..BRIDGE_X + 2).contains(&x) || y >= 34)
                && story::site_open(finished, DevelopmentKind::Bridge))
        && !TREES
            .iter()
            .any(|&cell| x == i32::from(cell.0) && y == i32::from(cell.1))
        && !PLOTS
            .iter()
            .any(|&cell| x == i32::from(cell.0) && y == i32::from(cell.1))
}

#[cfg(test)]
fn valid_cell_at(cell: Cell, finished: u8) -> bool {
    cell.valid() && walkable_at(i32::from(cell.0), i32::from(cell.1), finished)
}

fn route_by(start: Cell, target: Cell, walkable: impl Fn(i32, i32) -> bool) -> Option<Vec<Cell>> {
    if !start.valid()
        || !target.valid()
        || !walkable(start.0.into(), start.1.into())
        || !walkable(target.0.into(), target.1.into())
    {
        return None;
    }
    if start == target {
        return Some(Vec::new());
    }
    const UNVISITED: u16 = u16::MAX;
    let mut parents = [UNVISITED; CELLS];
    let mut open = VecDeque::new();
    parents[start.index()] = start.index() as u16;
    open.push_back(start);
    while let Some(current) = open.pop_front() {
        if current == target {
            let mut result = Vec::new();
            let mut point = target;
            while point != start && result.len() < MAX_ROUTE {
                result.push(point);
                point = Cell::from_index(usize::from(parents[point.index()]));
            }
            if point != start {
                return None;
            }
            result.reverse();
            return Some(result);
        }
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            let x = i32::from(current.0) + dx;
            let y = i32::from(current.1) + dy;
            if !walkable(x, y) {
                continue;
            }
            let cell = Cell(x as u16, y as u16);
            if parents[cell.index()] == UNVISITED {
                parents[cell.index()] = current.index() as u16;
                open.push_back(cell);
            }
        }
    }
    None
}

#[cfg(test)]
fn valid_cell(cell: Cell) -> bool {
    valid_cell_at(cell, 0)
}

#[cfg(test)]
fn route(start: Cell, target: Cell) -> Option<Vec<Cell>> {
    route_by(start, target, |x, y| walkable_at(x, y, 0))
}

fn update_position(actor: &mut Actor, now: u64) {
    let origin = actor.cell.point();
    if let Some(target) = actor
        .route
        .get(actor.route_next)
        .filter(|_| !actor.waiting_edge)
    {
        let target = target.point();
        let progress = movement::edge_fraction(now, actor.since, actor.until);
        actor.position = Point {
            x: origin.x + (target.x - origin.x) * progress,
            y: origin.y + (target.y - origin.y) * progress,
        };
    } else {
        actor.position = origin;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(world: &mut LivingWorld) {
        while world.is_catching_up() {
            world.advance_world_ms(0);
        }
    }

    #[test]
    fn town_calendar_separates_queued_real_age_and_simulated_permission() {
        let start = 9_000;
        let mut world = LivingWorld::new_village_wall_ms(start);
        let original = world.world_ms();
        world.queue_wall_time(start + 60 * DAY_MS - 1);
        assert_eq!(world.town_real_days(), 59);
        assert_eq!(world.world_ms(), original);
        assert!(!world.town_allows(TownEra::Workshop));
        let previous_target = world.pending_world_target_ms();
        world.queue_wall_time(start);
        assert_eq!(world.pending_world_target_ms(), previous_target);
        world.queue_wall_time(start + 60 * DAY_MS);
        assert_eq!(world.town_real_days(), 60);
        assert!(!world.town_allows(TownEra::MarketStreet));
        assert_eq!(world.town_era(), TownEra::Humble);
        assert_eq!(world.memory_celebrations(), 0);
        let json = serde_json::to_vec(&world).unwrap();
        let restored: LivingWorld = serde_json::from_slice(&json).unwrap();
        assert!(restored.validate());
        assert_eq!(world, restored);
    }

    #[test]
    fn town_calendar_inherited_open_districts_and_paid_work_are_preserved() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let village = world.village.as_mut().unwrap();
        village.calendar = None;
        village.road_story.stage = 2;
        village.market_story.stage = 2;
        assert!(world.activate_weekly_market());
        world.story.running = Some(Project { id: 0, begun: world.now_ms, shifts: 0, next_shift: world.now_ms });
        world.initialize_town_calendar(0);
        assert!(world.town_permits_project(0));
        assert!(!world.town_permits_project(1));
        assert!(world.road_town_permit());
        let state = world.village.as_ref().unwrap();
        let calendar = state.calendar.as_ref().unwrap();
        assert!(calendar.permits_public_work(3, world.now_ms));
        assert!(!calendar.permits_public_work(4, world.now_ms));
        assert!(state.phases().contains(WILLOW_VILLAGE.phase("weekly_market").unwrap()));
        assert_eq!(world.town_era(), TownEra::Humble);
        assert!(world.validate());
    }

    #[test]
    fn town_calendar_work_requires_actual_resource_and_authorized_date() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let task = Task::Develop(0);
        let worker = ActorKind::Ayu.index();
        let site = world.site(worker, task);
        world.actors[worker].cell = site;
        world.carry(worker, Item::Stone, 1);
        world.commit(worker, task);
        assert!(world.story.running.is_none());
        assert!(world.has_carry(worker, Item::Stone, 1));
        world.set_test_town_age(7);
        world.drop_carry(worker);
        world.commit(worker, task);
        assert!(world.story.running.is_none());
        let stock_before = world.stock.count(Item::Stone);
        assert!(world.take_stock(Item::Stone, 1));
        world.carry(worker, Item::Stone, 1);
        world.commit(worker, task);
        assert_eq!(world.story.running.unwrap().shifts, 1);
        assert_eq!(world.stock.count(Item::Stone), stock_before - 1);
        assert!(!world.town_permits_project(1));
        assert_eq!(world.town_era(), TownEra::Humble);
    }

    #[test]
    fn village_full_seed_box_cannot_trap_a_crop_basket_at_the_pantry() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let worker = ActorKind::Xiaohe.index();
        world.stock.0[Item::Seeds as usize] = STOCK_CAP;
        let before = world.stock.count(Item::Crops);
        world.carry(worker, Item::Crops, 3);
        world.actors[worker].seeds_in_basket = true;
        world.actors[worker].cell = world.site(worker, Task::Store);
        world.commit(worker, Task::Store);
        assert_eq!(world.stock.count(Item::Crops), before + 3);
        assert_eq!(world.stock.count(Item::Seeds), STOCK_CAP);
        assert!(world.actors[worker].carried.is_none());
        assert!(!world.actors[worker].seeds_in_basket);
    }

    #[test]
    fn village_opposing_carriers_keep_stock_and_walk_clear_of_the_pantry() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.bowl = BOWL_CAP;
        let dock = world.pantry();
        let ayu = Cell(dock.0 + 1, dock.1 + 1);
        let xiaohe = Cell(dock.0, dock.1 + 2);
        world.actors[0].cell = xiaohe;
        world.actors[1].cell = ayu;
        world.carry(0, Item::Crops, 3);
        world.carry(1, Item::Water, 3);
        assert!(world.begin(0, Task::Store));
        assert!(world.begin(1, Task::Store));
        assert!(world.waiting_for_pantry(0, Cell(dock.0, dock.1 + 1)));
        let before = world.stock.clone();
        for _ in 0..90 {
            world.advance_world_ms(10_000);
            if world.stock.count(Item::Crops) == before.count(Item::Crops) + 3
                && world.stock.count(Item::Water) == before.count(Item::Water) + 3 {
                assert!(world.validate());
                return;
            }
        }
        panic!("carriers did not reach their storage positions: {:?}", world.actors);
    }

    #[test]
    fn village_head_on_at_a_work_slot_yields_until_the_loaded_carrier_has_passed() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let worker = ActorKind::Xiaohe.index();
        let carrier = ActorKind::Ayu.index();
        let slot = Cell(23, 19);
        let approach = Cell(24, 19);
        let wash = LivingWorld::well_draw_stand();
        for (i, at) in [(worker, approach), (carrier, slot)] {
            world.actors[i].cell = at;
            world.actors[i].position = at.point();
        }
        world.plots[1].stage = PlotStage::Tilled;
        world.plots[1].due = 0;
        world.carry(worker, Item::Seeds, 1);
        world.carry(carrier, Item::Crops, 3);
        world.actors[carrier].harvest_processing = 1;
        world.actors[worker].task = Some(Task::Sow(1));
        world.actors[carrier].task = Some(Task::ProcessHarvest(1));
        // Exactly the opposing initial edge observed in the native replay.
        world.walk(worker, Stage::WalkToSite, vec![slot]);
        world.actors[worker].route_goal = Some(slot);
        let path = route_by(slot, wash, |x, y| world.can_walk(x, y)).expect("map route to wash stand");
        assert_eq!(path.first(), Some(&approach));
        world.walk(carrier, Stage::WalkToSite, path);
        world.actors[carrier].route_goal = Some(wash);
        world.actors[ActorKind::Orange.index()].until = world.now_ms + HOUR;
        assert!(world.validate());
        let mut saved_during_pass = false;
        for _ in 0..300 {
            world.advance_world_ms(5_000);
            assert_ne!(world.actors[worker].cell, world.actors[carrier].cell);
            if !saved_during_pass && world.actors[worker].passing.is_some()
                && world.actors[worker].route.is_empty()
            {
                assert_ne!(world.actors[worker].cell, approach);
                assert_eq!(world.actors[worker].carried, Some(Item::Seeds));
                world = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
                assert!(world.validate());
                saved_during_pass = true;
            }
            if world.plots[1].stage == PlotStage::Seed && world.actors[carrier].harvest_processing >= 2 {
                assert!(saved_during_pass);
                assert_eq!(world.actors[carrier].carried, Some(Item::Crops));
                assert_eq!(world.actors[carrier].amount, 3);
                assert!(world.validate());
                return;
            }
        }
        panic!("opposing work routes did not clear: {:?}", world.actors);
    }

    #[test]
    fn village_uses_the_well_stand_from_both_sides_and_can_leave_it() {
        let stand = LivingWorld::well_draw_stand();
        assert_eq!(stand, Cell(30, 24));
        for start in [Cell(29, 24), Cell(31, 24)] {
            let mut world = LivingWorld::new_village_wall_ms(0);
            let worker = ActorKind::Xiaohe.index();
            world.actors[worker].cell = start;
            world.actors[worker].position = start.point();
            world.actors[worker].thirst = 100;
            assert!(world.begin(worker, Task::DrinkAtWell));
            for _ in 0..100 {
                world.advance_world_ms(10_000);
                if world.actors[worker].thirst < 70 { break; }
            }
            assert_eq!(world.actors[worker].cell, stand);
            assert!(world.actors[worker].thirst < 70);
            let mut world: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
            assert!(world.validate());
            assert!(world.begin(worker, Task::Rest));
            for _ in 0..160 {
                world.advance_world_ms(10_000);
                if world.actors[worker].stage == Stage::Work { break; }
            }
            assert_eq!(world.actors[worker].cell, world.home_side(worker));
            assert!(world.validate());
        }
    }

    #[test]
    fn village_old_pantry_route_yields_the_well_stand_without_losing_food_or_water() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let stand = LivingWorld::well_draw_stand();
        let pantry = Cell(35, 23);
        let xiaohe = ActorKind::Xiaohe.index();
        let ayu = ActorKind::Ayu.index();
        assert_eq!(world.tool_pickup(ayu, Task::Eat), pantry);
        for (worker, cell) in [(xiaohe, Cell(31, 24)), (ayu, Cell(28, 24))] {
            world.actors[worker].cell = cell;
            world.actors[worker].position = cell.point();
        }
        world.actors[xiaohe].thirst = 100;
        world.actors[ayu].hunger = 100;
        world.bowl = BOWL_CAP;
        world.stock.0[Item::Water as usize] = 0;
        world.actors[ActorKind::Orange.index()].until = world.now_ms + HOUR;
        world.next_well = world.now_ms + HOUR;
        let well_before = world.well;
        let food_before = world.stock.count(Item::Food);
        assert!(world.begin(xiaohe, Task::DrinkAtWell));
        world.actors[ayu].task = Some(Task::Eat);
        world.walk(ayu, Stage::WalkForTool, vec![
            Cell(29, 24), stand, Cell(31, 24), Cell(32, 24), Cell(33, 24), Cell(34, 24),
            Cell(35, 24), pantry,
        ]);
        world.actors[ayu].route_goal = Some(pantry);
        assert!(world.validate());
        let mut world: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        for _ in 0..220 {
            world.advance_world_ms(10_000);
            assert_ne!(world.actors[ayu].cell, stand);
            if world.actors[xiaohe].thirst < 70 && world.actors[ayu].hunger < 70 {
                assert_eq!(world.well, well_before - 1);
                assert_eq!(world.stock.count(Item::Food), food_before - 1);
                assert!(world.validate());
                return;
            }
        }
        panic!("well and pantry work did not finish: {:?}", world.actors);
    }

    #[test]
    fn village_established_cat_meal_carries_and_consumes_exactly_one_portion() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let xiaohe = ActorKind::Xiaohe.index();
        let ayu = ActorKind::Ayu.index();
        let cat = ActorKind::Orange.index();
        for _ in 0..100 {
            if world.village_chapter_completed("Q08") { break; }
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
        }
        assert!(world.village_chapter_completed("Q08"));
        if world.minute_of_day() < 8 * 60 || world.minute_of_day() >= 15 * 60 {
            let elapsed = (32 * 60 - u64::from(world.minute_of_day())) % (24 * 60);
            world.advance_world_ms(elapsed * MINUTE);
            drain(&mut world);
        }
        // Authorize only this test's established-district geometry after
        // letting the opening actually run. No production age is advanced.
        for phase in ["community", "cooperative_farm"] {
            world.village.as_mut().unwrap().grant(phase);
        }
        for (worker, foot) in [(xiaohe, world.pantry()), (ayu, world.home_side(1)),
                              (cat, village::poi("cat_rest"))] {
            world.drop_carry(worker);
            world.idle(worker, HOUR);
            world.actors[worker].cell = foot;
            world.actors[worker].position = foot.point();
            world.actors[worker].tool = None;
            world.actors[worker].stamina = 100;
            world.actors[worker].thirst = 20;
            world.actors[worker].hunger = if worker == cat { 90 } else { 20 };
        }
        world.meal_until = 0;
        world.meal_escrow = 0;
        world.village.as_mut().unwrap().opening.cat_food = false;
        world.stock.0[Item::Food as usize] = 1;
        world.stock.0[Item::Water as usize] = 3;
        for (needs, task) in [((95, 20), Task::Eat), ((20, 95), Task::Drink)] {
            let mut copy = world.clone();
            copy.actors[xiaohe].outfit = copy.outfit_for(xiaohe, false);
            (copy.actors[xiaohe].hunger, copy.actors[xiaohe].thirst) = needs;
            assert!(!copy.mature_cat_meal_needed(xiaohe));
            copy.plan(xiaohe);
            assert_eq!(copy.actors[xiaohe].task, Some(task));
        }
        assert!(world.mature_cat_meal_needed(xiaohe));
        assert_ne!(world.opening_task(xiaohe), Some(Task::ServeCat));
        let mut early = world.clone();
        early.village.as_mut().unwrap().region_flags = PhaseMask::DAY_ONE.bits();
        assert!(!early.mature_cat_meal_needed(xiaohe));
        world.actors[xiaohe].outfit = world.outfit_for(xiaohe, false);
        world.plan(xiaohe);
        assert_eq!(world.actors[xiaohe].task, Some(Task::ServeCat));
        assert_eq!(world.stock.count(Item::Food), 1);
        assert!(!world.mature_cat_meal_needed(ayu));
        let bowl = village::poi("cat_water");
        let mut carried = false;
        let mut served = false;
        for _ in 0..500 {
            world.advance_world_ms(5_000);
            let worker = &world.actors[xiaohe];
            if worker.task == Some(Task::ServeCat) && worker.carried == Some(Item::Food) {
                assert_eq!(worker.amount, 1);
                assert_eq!(world.stock.count(Item::Food), 0);
                carried = true;
            }
            if !served && world.village.as_ref().unwrap().opening.cat_food {
                assert!(carried);
                assert_eq!(worker.cell, bowl);
                assert_eq!(worker.carried, None);
                served = true;
                world.actors[cat].until = world.now_ms;
            }
            if served && world.actors[cat].hunger < 60 {
                assert_eq!(world.actors[cat].cell, bowl);
                assert!(!world.village.as_ref().unwrap().opening.cat_food);
                assert_eq!(world.stock.count(Item::Food), 0);
                return;
            }
        }
        panic!("the reserved portion did not reach the cat: {:?}", world.actors);
    }

    #[test]
    fn village_water_runs_choose_a_nearby_well_and_keep_saved_destinations() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.story.finished = 21;
        world.village.as_mut().unwrap().apply_development(21, false);
        let ayu = ActorKind::Ayu.index();
        let home = LivingWorld::well_draw_stand();
        let market = village::poi("market_well");
        for (start, task, destination) in [
            (village::poi("pantry_loading"), Task::GetHomeWater, home),
            (village::poi("market_gate"), Task::GetWater, market),
        ] {
            let mut after = world.clone();
            after.actors[ayu].cell = start;
            after.actors[ayu].position = start.point();
            assert_eq!(after.water_collection_task(ayu), task);
            assert!(after.begin(ayu, task));
            assert_eq!(after.actors[ayu].route_goal, Some(destination));
            assert!(after.validate());
        }
        world.actors[ayu].cell = world.pantry();
        world.actors[ayu].position = world.pantry().point();
        assert!(world.begin(ayu, Task::GetWater));
        let old: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert!(old.validate());
        assert_eq!(old.actors[ayu].route_goal, Some(market));
    }

    #[test]
    fn village_market_crop_request_is_bounded_and_never_creates_stock() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        assert!(world.spend_stock(Item::Crops, 3));
        assert!(world.stock.add(Item::Fish, 1));
        let original = serde_json::to_vec(&world).unwrap();
        assert_eq!(world.input(Task::Cook), Some((Item::Crops, 2)));
        world.set_market_crop_reserve(Some(u16::MAX));
        assert_eq!(world.supply_requests.market_crops, Some(12));
        assert!(world.market_crop_shortfall());
        assert_eq!(world.input(Task::Cook), Some((Item::Fish, 1)));
        assert_eq!(serde_json::to_vec(&world).unwrap(), original);
        assert!(world.spend_stock(Item::Fish, 1));
        assert_eq!(world.input(Task::Cook), Some((Item::Crops, 2)));
        world.set_market_crop_reserve(None);
        assert!(!world.market_crop_shortfall());
    }

    #[test]
    fn village_neighbour_meal_request_cooks_real_inputs_instead_of_filling_stock() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let worker = ActorKind::Xiaohe.index();
        world.advance_world_ms(3 * HOUR);
        drain(&mut world);
        world.village.as_mut().unwrap().opening.boxes_stowed = 0b11;
        world.stock.0[Item::Food as usize] = 9;
        world.bowl = BOWL_CAP;
        world.drop_carry(worker);
        world.idle(worker, HOUR);
        world.actors[worker].hunger = 20;
        world.actors[worker].thirst = 20;
        world.actors[worker].stamina = 100;
        world.actors[worker].outfit = world.outfit_for(worker, false);
        assert!(world.stock.count(Item::Crops) >= 2);
        let stock = world.stock.clone();
        let before = serde_json::to_vec(&world).unwrap();
        let mut regular = world.clone();
        regular.plan(worker);
        assert_ne!(regular.actors[worker].task, Some(Task::Cook));
        world.set_neighbour_food_reserve(true);
        assert!(world.neighbour_meals_needed());
        assert_eq!(before, serde_json::to_vec(&world).unwrap());
        world.plan(worker);
        assert_eq!(world.actors[worker].task, Some(Task::Cook));
        assert_eq!(world.stock, stock);
        for _ in 0..600 {
            world.advance_world_ms(5_000);
            if world.actors[worker].task == Some(Task::Cook) && world.actors[worker].carried == Some(Item::Crops) {
                assert_eq!(world.actors[worker].amount, 2);
                assert_eq!(world.stock.count(Item::Crops), stock.count(Item::Crops) - 2);
                assert_eq!(world.stock.count(Item::Food), stock.count(Item::Food));
                return;
            }
        }
        panic!("meal task did not pick up its cooking ingredients");
    }

    #[test]
    fn village_lean_season_uses_reserved_stock_before_counting_market_surplus() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(30);
        world.note_village_surplus_trade(Item::Crops, 4);
        assert!(!world.village_chapter_completed("Q16"));
        for _ in 0..260 {
            world.advance_world_ms(DAY_MS);
            for _ in 0..20_000 {
                if !world.is_catching_up() { break; }
                world.advance_world_ms(0);
            }
            assert!(!world.is_catching_up(), "lean-season clock stalled at {} / {}, {:?}", world.now_ms, world.target_ms,
                world.actors.iter().map(|actor| (actor.kind, actor.cell, actor.task, actor.stage, actor.until)).collect::<Vec<_>>());
            assert!(world.validate());
            if world.village_chapter_completed("Q23") { break; }
        }
        let state = &world.village.as_ref().unwrap().pantry_plan;
        assert!(world.village_chapter_completed("Q15"), "{state:?}");
        assert!(world.village_chapter_completed("Q23"), "{state:?}");
        let before = world.clone();
        assert!(!world.village_chapter_completed("Q16"));
        world.note_village_surplus_trade(Item::Wood, 5);
        assert_eq!(before, world);
    }

    #[test]
    fn village_preserves_only_use_a_working_rack_and_a_single_sealed_jar() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(30);
        for _ in 0..180 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            if world.village_chapter_completed("Q14") && world.village_chapter_completed("Q24") { break; }
        }
        assert!(world.village_chapter_completed("Q14") && world.village_chapter_completed("Q24"));
        assert!(world.stock.add(Item::Stone, 6));
        assert!(world.stock.add(Item::Rope, 4));
        for _ in 0..55 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            if world.village_chapter_completed("Q26") { break; }
        }
        let preserves = &world.village.as_ref().unwrap().preserves;
        assert!(world.village_chapter_completed("Q27"), "{preserves:?}, Q14={}, Q24={}, {:?}, {:?}",
            world.village_chapter_completed("Q14"), world.village_chapter_completed("Q24"), world.stock,
            world.story.improvements.installed().collect::<Vec<_>>());
        assert!(world.village_chapter_completed("Q26"), "{preserves:?}");
        assert!(world.validate());
    }

    #[test]
    fn village_custom_order_moves_paid_wood_as_one_used_tray() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(30);
        let mut included = false;
        for _ in 0..185 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            if !included && world.village_chapter_completed("Q14") {
                included = true;
            }
            if included {
                for (item, remaining) in [(Item::Stone, 5u16), (Item::Rope, 4), (Item::Crops, 13)] {
                    let deficit = remaining.saturating_sub(world.stock.count(item));
                    if deficit > 0 { assert!(world.exchange_stock_atomic(&[], &[(item, deficit)])); }
                }
            }
            if world.village_chapter_completed("Q29") { break; }
        }
        let state = world.village.as_ref().unwrap();
        assert!(world.village_chapter_completed("Q29"), "{:?} / {:?} / {:?}, pantry={:?}, stock={:?}",
            state.workshop_story, state.grain, state.preserves, world.story.improvements.installed().collect::<Vec<_>>(), world.stock);
        assert!(world.validate());
    }

    #[test]
    fn village_home_repairs_keep_a_real_temporary_sleep_place_and_use_each_room() {
        let world=build_village_through_road_story_for_test();
        assert!(world.village_chapter_completed("Q46"));
    }

    #[test]
    fn village_guest_trial_uses_the_first_safe_food_window() {
        use renovation::Work;

        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(30);
        world.now_ms = 60 * DAY_MS + 9 * HOUR;
        let state = world.village.as_mut().unwrap();
        state.opening.boxes_stowed = 3;
        state.opening.slept_at_home = 3;
        state.opening.rod_returned = true;
        state.opening.shared_eaten = 3;
        state.opening.cat_ate = true;
        state.opening.cat_visited = 3;
        state.opening.parallel_work = true;
        state.opening.late_meal_eaten = true;
        state.opening.shared_rest = true;
        state.opening.stable_days = 7;
        state.workshop_story.planted = true;
        state.pantry_plan.ready = true;
        state.pantry_plan.surplus_units = 2;
        state.pantry_plan.seasonal = 15;
        state.soil = serde_json::from_value(serde_json::json!({
            "water_proven": true, "drain_tested": true, "fish_success": true,
            "fish_empty": true, "cooked_fish": true,
        })).unwrap();
        let mut grain = serde_json::to_value(&state.grain).unwrap();
        grain["rotated"] = serde_json::json!(true);
        state.grain = serde_json::from_value(grain).unwrap();
        state.renovation.done = 3;
        state.renovation.path = 4;
        state.renovation.corner = true;
        state.renovation.corner_used = [2, 2];
        assert!(world.village_chapter_completed("Q23"));
        for i in 0..2 {
            let outfit = world.outfit_for(i, false);
            world.idle(i, MINUTE);
            world.actors[i].outfit = outfit;
            world.actors[i].hunger = 30;
            world.actors[i].thirst = 30;
            world.actors[i].stamina = 75;
        }

        world.stock.0[Item::Food as usize] = 6;
        assert_eq!(world.renovation_task(0), None);
        let mut needs_food = world.clone();
        needs_food.plan(0);
        assert_eq!(needs_food.actors[0].task, Some(Task::Cook));

        world.stock.0[Item::Food as usize] = 7;
        assert_eq!(world.renovation_task(0), Some(Task::Renovate(Work::PrepareGuests)));
        world.plan(0);
        assert_eq!(world.actors[0].task, Some(Task::Renovate(Work::PrepareGuests)));
        assert_eq!(world.stock.count(Item::Food), 7);

        world.now_ms += DAY_MS;
        let renovation = &mut world.village.as_mut().unwrap().renovation;
        renovation.guest_day = Some(60);
        renovation.guest_table = 2;
        world.stock.0[Item::Food as usize] = 5;
        assert_eq!(world.renovation_task(1), Some(Task::Renovate(Work::CloseGuestTrial)));
        world.commit_renovation(1, Work::CloseGuestTrial);
        assert_eq!(world.stock.count(Item::Food), 7);
        assert!(world.village_chapter_completed("Q35"));
    }

    fn build_village_through_road_story_for_test() -> LivingWorld {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(30);
        let mut ready = false;
        for _ in 0..260 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            if !ready && world.village_chapter_completed("Q08") && world.village_chapter_completed("Q20") {
                ready = true;
            }
            if ready {
                for (item, remaining) in [(Item::Stone, 4u16), (Item::Rope, 4)] {
                    let deficit = remaining.saturating_sub(world.stock.count(item));
                    if deficit > 0 { assert!(world.exchange_stock_atomic(&[], &[(item, deficit)])); }
                }
            }
            if world.village_chapter_completed("Q15") && !world.village_chapter_completed("Q16") {
                // The resident host records the receipt after this real debit
                // is stowed. Substitute the counterpart in the core fixture.
                if world.stock.count(Item::Crops) >= 8 && world.spend_stock(Item::Crops, 2) {
                    world.note_village_surplus_trade(Item::Crops, 2);
                }
            }
            if let Some((item,qty)) = world.take_public_order() {
                assert_eq!((item,qty), (Item::Food,2));
            }
            assert!(world.validate(), "day {} state {:?}, actors {:?}, checks {:?}, meal {:?} stock {:?} build {:?} active {:?}",
                world.world_ms()/DAY_MS, world.village.as_ref().unwrap().renovation, world.actors,
                world.village.as_ref().map(|state| (state.renovation.validate(world.now_ms), state.opening.validate(world.now_ms),
                    state.grain.validate(world.now_ms), state.soil.validate(world.now_ms), state.workshop_story.validate(world.now_ms),
                    state.pantry_plan.validate(world.now_ms), state.preserves.validate(world.now_ms),
                    world.story.validate_for(world.now_ms, true), world.crop_batches.validate(world.stock.count(Item::Crops), world.now_ms))),
                (world.meal_until,world.meal_escrow), world.stock, world.buildings, world.story.improvements.active());
            if world.village_chapter_completed("Q46") { break; }
        }
        let renovation = &world.village.as_ref().unwrap().renovation;
        assert!(world.village_chapter_completed("Q30"), "{renovation:?}");
        assert!(world.village_chapter_completed("Q31"), "{renovation:?}; actors {:?}; stock {:?}", world.actors, world.stock);
        assert!(world.village_chapter_completed("Q32"), "{renovation:?}; actors {:?}; stock {:?}", world.actors, world.stock);
        assert!(world.village_chapter_completed("Q33"), "{renovation:?}");
        assert!(world.village_chapter_completed("Q34"), "{renovation:?}");
        assert!(world.village_chapter_completed("Q35"), "{renovation:?}; pantry {:?}; actors {:?}; stock {:?}",
            world.village.as_ref().unwrap().pantry_plan, world.actors, world.stock);
        for chapter in ["Q42", "Q43", "Q44", "Q45", "Q46"] {
            assert!(world.village_chapter_completed(chapter), "{chapter}: {:?}, actor {:?}, stock {:?}",
                world.village.as_ref().unwrap().road_story, world.actors[1], world.stock);
        }
        for tile in renovation::PATH {
            assert_eq!(world.ground(i32::from(tile.x), i32::from(tile.y)), Ground::Path);
        }
        assert_eq!(world.home_of(ActorKind::Xiaohe), Some(world.initial_home_door(0).point()));
        assert_eq!(world.home_of(ActorKind::Ayu), Some(world.initial_home_door(1).point()));
        world
    }

    #[test]
    fn village_public_surface_lamps_and_memories_follow_recorded_prerequisites() {
        let mut world=build_village_through_road_story_for_test();
        world.set_test_town_age(365);
        assert!(world.activate_weekly_market(), "the seeded market needs its opened canopy sites");
        let supplied_for_mill = world.exchange_stock_atomic(&[], &[(Item::Stone, 3)]);
        assert!(supplied_for_mill);
        for _ in 0..455 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            let day=world.world_ms()/DAY_MS;
            if day >= 112 {
                for (item, remaining) in [(Item::Stone,5u16),(Item::Rope,4)] {
                    let short=remaining.saturating_sub(world.stock.count(item));
                    if short>0 { assert!(world.exchange_stock_atomic(&[],&[(item,short)])); }
                }
            }
            if day>=224 {
                // Offer only the resident-side thresholds. The ground project
                // itself still has to run through hauling and weather.
                world.record_village_social(crate::residents::SocialProgress {
                    market_after_review:true,market_cleanup:true,fixed_market:day>=336,
                    last_market_cleanup_day: (day>=336 && day % 7 <= 1).then_some(day-day%7),
                    ..Default::default()
                });
            }
            if day>=112 { world.record_companionship_clock(366*DAY_MS,12*HOUR); }
            if world.village_chapter_completed("Q59") && world.village_chapter_completed("Q72") { break; }
        }
        let state=world.village.as_ref().unwrap();
        for chapter in ["Q54","Q55","Q56","Q57","Q58","Q59","Q70","Q72"] {
            assert!(world.village_chapter_completed(chapter), "{chapter}: {:?} {:?} / {:?} {:?}, grain {:?}, projects {}, actors {:?}, stock {:?}, social {:?}",
                state.market_story,state.memories,state.renovation,state.road_story,state.grain,world.story.completed(),world.actors,world.stock,state.social);
        }
        assert_eq!(world.memory_celebrations(),1);
        assert!(world.validate());
    }

    #[test]
    fn village_mill_uses_a_real_stone_and_teaches_a_second_cook() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(30);
        // The full host trades for stone. This core fixture provides the
        // spare stone separately so construction and mill can both be paid.
        assert!(world.stock.add(Item::Stone, 20));
        for _ in 0..125 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            assert!(world.validate());
            if world.village_chapter_completed("Q28") { break; }
        }
        let grain = &world.village.as_ref().unwrap().grain;
        assert!(world.village_chapter_completed("Q25"), "{grain:?}");
        assert!(world.village_chapter_completed("Q28"), "{grain:?}");
    }

    #[test]
    fn village_workshop_uses_traced_wood_and_services_the_same_rod() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        for _ in 0..100 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            assert!(world.validate());
            if world.village_chapter_completed("Q20") && world.village_chapter_completed("Q24") {
                break;
            }
        }
        let workshop = &world.village.as_ref().unwrap().workshop_story;
        for chapter in ["Q20", "Q24"] {
            assert!(world.village_chapter_completed(chapter), "{chapter}: {workshop:?}");
        }
    }

    #[test]
    fn village_soil_proves_targeted_water_drain_test_and_mature_compost() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        for _ in 0..90 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            assert!(world.validate());
        }
        let soil = &world.village.as_ref().unwrap().soil;
        for chapter in ["Q17", "Q18", "Q19", "Q21"] {
            assert!(world.village_chapter_completed(chapter), "{chapter}: {soil:?}");
        }
    }

    #[test]
    fn village_wheat_goes_from_named_seed_to_pantry_shared_meal_and_replanting() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(30);
        for _ in 0..140 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            assert!(world.validate());
            if world.village_chapter_completed("Q14") && world.village_chapter_completed("Q22") {
                break;
            }
        }
        let grain = &world.village.as_ref().unwrap().grain;
        for chapter in ["Q09", "Q10", "Q11", "Q12", "Q13", "Q14", "Q22"] {
            assert!(world.village_chapter_completed(chapter), "{chapter}: {grain:?}");
        }
        let bytes = serde_json::to_vec(&world).unwrap();
        let reopened: LivingWorld = serde_json::from_slice(&bytes).unwrap();
        assert!(reopened.validate());
        assert_eq!(reopened, world);
    }

    #[test]
    fn village_opening_replays_real_household_work_meals_rest_and_weekly_review() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        assert!(!world.opening_chapter_completed("Q01"));
        assert!(!world.opening_chapter_completed("Q02"));
        assert!(!world.opening_chapter_completed("Q03"));
        assert!(!world.opening_chapter_completed("Q04"));
        for _ in 0..35 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            assert!(world.validate());
            let bytes = serde_json::to_vec(&world).unwrap();
            let reopened: LivingWorld = serde_json::from_slice(&bytes).unwrap();
            assert!(reopened.validate());
            assert_eq!(reopened, world);
        }
        let opening = &world.village.as_ref().unwrap().opening;
        assert!(world.opening_chapter_completed("Q01"), "{opening:?}");
        assert!(world.opening_chapter_completed("Q02"), "{opening:?}");
        assert!(world.opening_chapter_completed("Q04"), "{opening:?}");
        assert!(world.opening_chapter_completed("Q03"), "{opening:?}; actors {:?}; stock {:?}", world.actors, world.stock);
        assert!(world.opening_chapter_completed("Q05"), "{opening:?}");
        assert!(world.opening_chapter_completed("Q06"), "{opening:?}");
        assert!(world.opening_chapter_completed("Q07"), "{opening:?}");
        assert!(world.opening_chapter_completed("Q08"), "{opening:?}");
    }

    #[test]
    fn village_scenery_shares_collision_and_leaves_production_feet_clear() {
        let world = LivingWorld::new_village_wall_ms(0);
        let view = world.navigation_view().unwrap();
        for object in WILLOW_VILLAGE.scenic {
            let (x, y) = (i32::from(object.tile.x), i32::from(object.tile.y));
            assert_eq!(world.can_walk(x, y), !object.solid, "{}", object.id);
            assert_eq!(view.can_walk(object.tile), !object.solid, "{}", object.id);
            assert_eq!(world.ground(x, y), Ground::Grass, "{}", object.id);
        }
        for tree in village::tree_cells() {
            let foot = tree.tile().offset(0, 1).unwrap();
            assert!(
                WILLOW_VILLAGE.tile(tree.tile(), PhaseMask::ALL).is_some_and(|info| !info.blocked),
                "production tree at {tree:?} must have independent dynamic occupancy"
            );
            assert!(view.can_walk(foot), "production work foot: {foot:?}");
        }
        for (parcel, [x, y]) in village::COMPATIBLE_HOME_ORIGINS.into_iter().enumerate() {
            let plan = if parcel == 0 { HousePlan::Shared } else { HousePlan::Single };
            let [width, height] = plan.footprint();
            for object in WILLOW_VILLAGE.scenic {
                let (tx, ty) = (object.tile.x, object.tile.y);
                let inside = (u16::from(x)..u16::from(x + width)).contains(&tx)
                    && (u16::from(y)..u16::from(y + height)).contains(&ty);
                assert!(!inside, "legacy relocation site overlaps {}", object.id);
            }
            let door = TilePos::new(u16::from(x + width / 2), u16::from(y + height));
            assert!(view.can_walk(door), "legacy relocation door: {door:?}");
        }
    }

    #[test]
    fn village_household_keeps_stage_and_stock_across_a_saved_remote_step() {
        let mut world = LivingWorld::new_village_wall_ms(42);
        assert!(world.validate());
        assert_eq!(world.world_ms(), 4 * DAY_MS + 6 * HOUR);
        assert_eq!(world.map_extent(), (192, 128));
        assert_eq!(world.plots().len(), 30);
        assert_eq!(world.stock.0[..4], [6, 12, 12, 12]);
        assert_eq!(world.crop_batches.units(), 12);
        assert_eq!(
            world.plots[..6]
                .iter()
                .map(|plot| plot.stage)
                .collect::<Vec<_>>(),
            [
                PlotStage::Mature,
                PlotStage::Mature,
                PlotStage::Growing,
                PlotStage::Growing,
                PlotStage::Grass,
                PlotStage::Grass
            ]
        );
        let quarry = world.resolve_poi("quarry_work").unwrap();
        assert!(world.reserve_activity(&[ActorKind::Xiaohe]));
        assert!(world.move_for_activity(ActorKind::Xiaohe, quarry));
        world.queue_world_time(500);
        assert!(world.navigation_pending());
        let mut slices = 0;
        while world.is_catching_up() {
            world.advance_pending_to(world.pending_world_target_ms());
            slices += 1;
            assert!(slices < 100);
        }
        assert!(slices > 1, "long route must yield before finishing search");
        assert_ne!(
            world.actor_tile(ActorKind::Xiaohe),
            world.actor_destination(ActorKind::Xiaohe).unwrap()
        );
        assert!(world.validate());
        let checkpoint = serde_json::to_vec(&world).unwrap();
        let mut reopened: LivingWorld = serde_json::from_slice(&checkpoint).unwrap();
        assert!(reopened.validate());
        let target = Cell::at(quarry).unwrap().tile();
        for _ in 0..600 {
            if world.activity_arrived(ActorKind::Xiaohe) {
                break;
            }
            let due = world.next_update_ms();
            world.queue_world_time(due.saturating_sub(world.pending_world_target_ms()));
            reopened.queue_world_time(due.saturating_sub(reopened.pending_world_target_ms()));
            drain(&mut world);
            drain(&mut reopened);
        }
        assert_eq!(world.actor_tile(ActorKind::Xiaohe), target);
        assert!(world.activity_arrived(ActorKind::Xiaohe));
        assert_eq!(world, reopened);
        let (at_home, awake) = world.seven_day_home_time(ActorKind::Xiaohe).unwrap();
        assert!(0 < at_home && at_home < awake);
    }

    #[test]
    fn village_migration_retains_the_first_edge_and_its_clock_fraction() {
        let mut legacy = LivingWorld::new_wall_ms(100);
        legacy.actors[0].cell = HOUSE_SIDE[0];
        legacy.actors[0].position = HOUSE_SIDE[0].point();
        legacy.actors[0].outside = true;
        assert!(legacy.begin(0, Task::External(Cell(15, 14))));
        legacy.advance_world_ms(2_500);
        drain(&mut legacy);
        let old = &legacy.actors[0];
        assert_ne!(old.position, old.cell.point());
        assert!(legacy.validate());
        let saved = serde_json::to_vec(&legacy).unwrap();
        let mapped = legacy.migrate_to_village().unwrap();
        let new = &mapped.actors[0];
        assert!(mapped.validate());
        assert_eq!(mapped.world_ms(), legacy.world_ms());
        assert_eq!(
            mapped.pending_world_target_ms(),
            legacy.pending_world_target_ms()
        );
        assert_eq!(new.until, old.until);
        assert_eq!(new.since, old.since);
        assert_eq!(
            new.position.x - new.cell.point().x,
            old.position.x - old.cell.point().x
        );
        assert_eq!(mapped.stock(), legacy.stock());
        assert_eq!(serde_json::to_vec(&legacy).unwrap(), saved);
    }

    #[test]
    fn village_loaded_edge_keeps_fraction_and_ownership_through_save() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let actor = ActorKind::Xiaohe;
        let home = world.home_side(actor.index());
        world.actors[actor.index()].cell = home;
        world.actors[actor.index()].position = home.point();
        assert!(world.reserve_market_trip(actor));
        let target = [(0, 1), (-1, 0), (1, 0), (0, -1)].into_iter()
            .filter_map(|(dx, dy)| home.tile().offset(dx, dy).map(Cell::from))
            .find(|&cell| world.valid_world_cell(cell) && world.actors.iter().all(|other| other.cell != cell))
            .unwrap();
        assert!(world.move_for_activity_with_load(actor, target.point(), true));
        let start = world.world_ms();
        world.advance_world_ms(48 * 590);
        drain(&mut world);
        let walking = &world.actors[actor.index()];
        let duration = walking.until - walking.since;
        assert_eq!(duration, movement::edge_duration(start, movement::Pace::Human,
            movement::WalkConditions {
                loaded: true,
                rough_weather: matches!(world.weather(), YardWeather::Rain | YardWeather::Snow),
                ..Default::default()
            }));
        assert_eq!(walking.pose, Pose::Carry);
        assert_eq!(walking.carried, None, "only the external owner holds the cargo");
        assert_eq!(walking.cell, home);
        assert!(walking.position != home.point() && walking.position != target.point());
        assert!(world.validate());
        let mut reopened: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert!(reopened.validate());
        for current in [&mut world, &mut reopened] {
            current.advance_world_ms(duration);
            drain(current);
            assert_eq!(current.actor_tile(actor), target.tile());
            assert!(current.activity_arrived(actor));
            assert!(!current.actors[actor.index()].external_load);
        }
        assert_eq!(world, reopened);
    }

    #[test]
    fn completed_soft_ground_step_leaves_a_transient_mark() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let origin = world.home_side(0);
        let (origin, target) = (-5..=5)
            .flat_map(|dy| (-5..=5).map(move |dx| (dx, dy)))
            .filter_map(|(dx, dy)| origin.tile().offset(dx, dy).map(Cell::from))
            .filter(|&at| world.valid_world_cell(at) && matches!(world.ground(at.0.into(), at.1.into()), Ground::Soil | Ground::Grass))
            .find_map(|at| at.tile().offset(1, 0).map(Cell::from).filter(|&to| world.valid_world_cell(to)).map(|to| (at, to)))
            .unwrap();
        let spell = (1_600..1_900)
            .find(|&spell| LivingWorld::wet_spell(spell - 1) && !LivingWorld::wet_spell(spell))
            .unwrap();
        let start = spell * 3 * HOUR + 5 * MINUTE;
        world.advance_world_ms(start - world.world_ms());
        drain(&mut world);
        world.idle(0, HOUR);
        world.idle(1, HOUR);
        world.idle(2, HOUR);
        world.footprints.erase();
        world.actors[0].cell = origin;
        world.actors[0].position = origin.point();
        world.actors[0].task = Some(Task::External(target));
        world.walk(0, Stage::WalkToSite, vec![target]);
        let remaining = world.actors[0].until - world.now_ms;
        world.advance_world_ms(remaining / 2);
        assert_eq!(world.village_footprints().count(), 0);
        world.advance_world_ms(remaining - remaining / 2);
        assert_eq!(world.actor_tile(ActorKind::Xiaohe), target.tile());
        let marks: Vec<_> = world.village_footprints().collect();
        assert_eq!(marks.len(), 1);
        assert_eq!((marks[0].tile, marks[0].toward), (origin.tile(), target.tile()));
        let reopened: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert_eq!(reopened.village_footprints().count(), 0);
        assert_eq!(world, reopened);
    }

    #[test]
    fn village_weekly_market_activates_without_changing_freight_or_clock() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        assert!(world.resolve_poi("stall_produce.door").is_none());
        assert!(world.activate_weekly_market());
        assert!(world.resolve_poi("stall_produce.door").is_some());
        assert!(
            !world
                .phase_mask()
                .contains(WILLOW_VILLAGE.phase("main_bridge_freight").unwrap())
        );
        assert_eq!(world.world_ms(), now);
        assert!(world.validate());
    }

    #[test]
    fn village_day_advances_farm_food_and_real_building_progress_without_input() {
        let mut world = LivingWorld::new_village_wall_ms(900);
        world.set_test_town_age(7);
        world.advance_world_ms(5 * DAY_MS);
        drain(&mut world);
        assert!(world.validate());
        assert!(
            world.harvested > 0,
            "household never brought a crop to maturity"
        );
        assert!(world.meals > 0, "household did not eat");
        assert!(world.drinks > 0, "cat did not drink");
        assert!(
            world
                .buildings
                .iter()
                .any(|building| building.step > 0 || building.level > 1)
        );
        assert!(
            world.story.completed() > 0 || world.story.active().is_some(),
            "no actual village project has gotten labor and material"
        );
    }

    #[test]
    fn village_exactly_retains_inflight_legacy_build_material() {
        let mut legacy = LivingWorld::new_wall_ms(1234);
        assert!(legacy.begin(1, Task::Build(4)));
        for _ in 0..20 {
            let due = legacy.next_update_ms();
            legacy.advance_world_ms(due.saturating_sub(legacy.world_ms()));
            drain(&mut legacy);
            if legacy.actors[1].carried.is_some() && legacy.actors[1].stage == Stage::WalkToSite {
                break;
            }
        }
        assert_eq!(legacy.actors[1].task, Some(Task::Build(4)));
        assert!(legacy.actors[1].carried.is_some());
        assert!(legacy.validate());
        let migrated = legacy.migrate_to_village().unwrap();
        assert_eq!(migrated.actors[1].carried, legacy.actors[1].carried);
        assert_eq!(migrated.actors[1].amount, legacy.actors[1].amount);
        assert_eq!(migrated.stock(), legacy.stock());
        assert_eq!(migrated.story(), legacy.story());
        assert_eq!(
            migrated.actor_destination(ActorKind::Ayu),
            Some(village::building(4).tile())
        );
    }

    #[test]
    fn village_migrates_each_legacy_housing_origin_and_shared_partial_handoff() {
        for lot in 0..3 {
            for original in [false, true] {
                let mut world = LivingWorld::new_wall_ms(99);
                const START: u64 = 250 * DAY_MS;
                world.now_ms = START;
                world.target_ms = START;
                world.next_needs = START + 10 * MINUTE;
                world.next_well = START + 20 * MINUTE;
                world.next_fish = START + 12 * HOUR;
                for actor in &mut world.actors {
                    actor.since = START;
                    actor.until = START;
                }
                let mut order = HousingOrder::new(HousePlan::Single, lot);
                if original {
                    order = order.with_original_legacy_origin();
                }
                world.story.housing.commit(order, 1, world.now_ms);
                for _ in 0..4 {
                    world.now_ms += 4 * DAY_MS;
                    let pending = world.story.housing.next_order(world.now_ms).unwrap();
                    world.story.housing.commit(pending, 1, world.now_ms);
                }
                world.now_ms += 4 * DAY_MS;
                let move_in = world.story.housing.next_order(world.now_ms).unwrap();
                world.story.housing.commit(move_in, 0, world.now_ms);

                world.now_ms += 55 * DAY_MS;
                let shared_lot = (lot + 1) % 3;
                let mut shared = HousingOrder::new(HousePlan::Shared, shared_lot);
                if original {
                    shared = shared.with_original_legacy_origin();
                }
                world.story.housing.commit(shared, 1, world.now_ms);
                for _ in 0..4 {
                    world.now_ms += 4 * DAY_MS;
                    let pending = world.story.housing.next_order(world.now_ms).unwrap();
                    world.story.housing.commit(pending, 1, world.now_ms);
                }
                world.now_ms += 4 * DAY_MS;
                let shared_move_in = world.story.housing.next_order(world.now_ms).unwrap();
                world.story.housing.commit(shared_move_in, 0, world.now_ms);
                assert_eq!(world.story.housing.target_occupant(shared_move_in), Some(1));
                world.target_ms = world.now_ms;
                world.next_needs = world.now_ms + 10 * MINUTE;
                world.next_well = world.now_ms + 20 * MINUTE;
                world.next_fish = world.now_ms + 12 * HOUR;
                for actor in &mut world.actors {
                    actor.since = world.now_ms;
                    actor.until = world.now_ms;
                }
                assert!(world.validate());
                let migrated = world.migrate_to_village().unwrap();
                let new_old_home = migrated.story.housing.order_for_person(0).unwrap();
                let new_shared = migrated.story.housing.next_order(migrated.now_ms).unwrap();
                assert_eq!(new_old_home.lot, lot);
                assert_eq!(
                    new_old_home.lot_origin(),
                    village::COMPATIBLE_HOME_ORIGINS[2]
                );
                assert_eq!(new_shared.lot, shared_lot);
                assert_eq!(new_shared.lot_origin(), village::COMPATIBLE_HOME_ORIGINS[0]);
                assert_eq!(migrated.story.housing.target_occupant(new_shared), Some(1));
                assert_eq!(migrated.stock(), world.stock());
            }
        }
    }

    #[test]
    #[ignore = "set CAT_HOME_TEST_LEGACY_FIXTURE to an isolated v2/v3 copy"]
    fn village_migrates_isolated_player_fixture() {
        let source =
            std::env::var_os("CAT_HOME_TEST_LEGACY_FIXTURE").expect("isolated fixture path");
        let bytes = std::fs::read(&source).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let world: LivingWorld = serde_json::from_value(value["world"].clone()).unwrap();
        let migrated = world.migrate_to_village().unwrap();
        assert!(migrated.validate());
        assert_eq!(migrated.world_ms(), world.world_ms());
        assert_eq!(
            migrated.pending_world_target_ms(),
            world.pending_world_target_ms()
        );
        assert_eq!(migrated.stock(), world.stock());
        assert_eq!(migrated.story.completed(), world.story.completed());
        for (after, before) in migrated.actors.iter().zip(&world.actors) {
            assert_eq!(
                (after.carried, after.amount, after.tool),
                (before.carried, before.amount, before.tool)
            );
        }
        assert_eq!(bytes, std::fs::read(&source).unwrap());
    }

    #[test]
    #[ignore = "release whole-world 730-day regression"]
    fn village_lifetime_730_days_remains_bounded_and_replays_after_one_year() {
        let mut continuous = LivingWorld::new_village_wall_ms(0);
        continuous.advance_world_ms(365 * DAY_MS);
        drain(&mut continuous);
        assert!(continuous.validate());
        let first = serde_json::to_vec(&continuous).unwrap();
        assert!(first.len() < 256 * 1024);
        let mut sliced: LivingWorld = serde_json::from_slice(&first).unwrap();
        continuous.advance_world_ms(365 * DAY_MS);
        drain(&mut continuous);
        for _ in 0..365 {
            sliced.advance_world_ms(DAY_MS);
            drain(&mut sliced);
        }
        let second = serde_json::to_vec(&continuous).unwrap();
        assert!(second.len() < 256 * 1024);
        assert!(continuous.validate());
        assert_eq!(continuous.town_era(), TownEra::Humble);
        assert_eq!(continuous.story.completed(), 0);
        assert!(continuous.harvested > 0 && continuous.meals > 0);
        assert!(sliced.validate());
        assert_eq!(continuous, sliced);
        assert!(continuous.harvested > 0);
        eprintln!(
            "household snapshots after 365/730 world days: {}/{} bytes",
            first.len(),
            second.len()
        );
    }

    #[test]
    fn village_short_harvest_waits_for_real_processing_and_safe_storage() {
        let mut world = LivingWorld::new_village_wall_ms(250);
        let farmer = 0;
        let water = world.stock.count(Item::Water);
        let crops = world.stock.count(Item::Crops);
        let move_to = |world: &mut LivingWorld, task| {
            let cell = world.site(farmer, task);
            world.actors[farmer].cell = cell;
            world.actors[farmer].position = cell.point();
        };
        let harvest = Task::Harvest(0);
        move_to(&mut world, harvest);
        world.commit(farmer, harvest);
        assert_eq!(world.stock.count(Item::Crops), crops);
        assert_eq!(world.actors[farmer].harvest_processing, 1);
        for phase in 1..=4 {
            let task = Task::ProcessHarvest(phase);
            // Find an ordinary daylight slot with drying weather for this isolated operation.
            while phase == 3
                && (matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
                    || !(6..18).contains(&(world.minute_of_day() / 60)))
            {
                world.now_ms += HOUR;
            }
            move_to(&mut world, task);
            world.commit(farmer, task);
            assert_eq!(world.stock.count(Item::Crops), crops);
        }
        assert_eq!(world.stock.count(Item::Water), water - 1);
        move_to(&mut world, Task::Store);
        world.commit(farmer, Task::Store);
        assert_eq!(world.stock.count(Item::Crops), crops + 3);
        assert_eq!(world.crop_batches.units(), crops + 3);
    }

    #[test]
    fn built_in_household_earns_food_builds_and_cares_for_the_cat() {
        let mut world = LivingWorld::new(1_790_000_000);
        let mut saw_early_farm_stages = [false; 5];
        let mut saw_household_activities = [false; 7];
        let mut saw_build_progress = [false; 2];
        for _ in 0..(4 * DAY_MS / MINUTE) {
            world.advance_world_ms(MINUTE);
            for plot in world.plots() {
                if (plot.stage as usize) < 5 {
                    saw_early_farm_stages[plot.stage as usize] = true;
                }
            }
            for actor in world.actors() {
                let index = match actor.activity {
                    Activity::GetTool => 0,
                    Activity::Harvest => 1,
                    Activity::Store => 2,
                    Activity::Cook => 3,
                    Activity::Sleep => 4,
                    Activity::FellTree => 5,
                    Activity::CarryWood => 6,
                    _ => continue,
                };
                saw_household_activities[index] = true;
            }
            let progress = world.buildings()[4].progress;
            if progress > 0 {
                saw_build_progress[usize::from(progress > 33)] = true;
            }
        }
        assert_eq!(saw_early_farm_stages, [true; 5]);
        assert_eq!(saw_household_activities, [true; 7]);
        assert_eq!(saw_build_progress, [true; 2]);
        assert!(world.harvested >= 3 && world.meals > 8 && world.drinks > 1);
        assert!(world.buildings()[4].level >= 2);
        assert!(
            world
                .trees()
                .iter()
                .filter(|tree| tree.protected)
                .all(|tree| tree.stage == TreeStage::Standing)
        );
        assert!(world.validate());
    }

    #[test]
    fn dry_seeds_and_unfinished_work_do_not_grant_materials() {
        let mut world = LivingWorld::new(0);
        world.plots[0].stage = PlotStage::Seed;
        world.stock.0[Item::Water as usize] = 0;
        world.well = 0;
        world.next_well = START_MS + DAY_MS;
        world.advance_world_ms(8 * HOUR);
        drain(&mut world);
        assert_eq!(world.plots[0].stage, PlotStage::Seed);
        assert_eq!(world.harvested, 0);

        let before = world.stock.clone();
        assert!(!world.spend_many(&[(Item::Rope, 4), (Item::Rope, 4)]));
        assert_eq!(world.stock, before);
        assert!(world.spend_many(&[(Item::Rope, 2), (Item::Rope, 1)]));
        assert_eq!(world.stock.count(Item::Rope), before.count(Item::Rope) - 3);
    }

    #[test]
    fn offline_budget_checkpoint_and_small_slices_produce_same_state() {
        let mut direct = LivingWorld::new(400);
        let mut sliced = direct.clone();
        let span = 14 * DAY_MS;
        direct.advance_world_ms(span);
        assert!(direct.is_catching_up());
        assert!(direct.validate());
        let encoded = serde_json::to_vec(&direct).unwrap();
        let mut direct: LivingWorld = serde_json::from_slice(&encoded).unwrap();
        assert!(direct.validate());
        drain(&mut direct);
        for _ in 0..span / (17 * MINUTE) {
            sliced.advance_world_ms(17 * MINUTE);
        }
        sliced.advance_world_ms(span % (17 * MINUTE));
        drain(&mut sliced);
        assert_eq!(direct, sliced);
    }

    #[test]
    fn actors_walk_around_the_pond_and_moves_restore_mid_edge() {
        let mut world = LivingWorld::new(0);
        let index = ActorKind::Orange.index();
        let start = world.actors[index].position;
        assert!(world.external_hold(index, true));
        assert!(!world.external_move(index, Point::at_tile(11, 19)));
        let goal = Point::at_tile(10, 23);
        assert!(world.external_move(index, goal));
        world.advance_world_ms(4_000);
        let halfway = world.actors[index].position;
        assert_ne!(halfway, start);
        assert_ne!(halfway, goal);
        assert!(!world.external_arrived(index));
        assert!(world.validate());
        let encoded = serde_json::to_vec(&world).unwrap();
        let mut loaded: LivingWorld = serde_json::from_slice(&encoded).unwrap();
        for _ in 0..200 {
            let previous = world.actors[index].position;
            world.advance_world_ms(1_000);
            loaded.advance_world_ms(1_000);
            let current = world.actors[index].position;
            assert!((previous.x - current.x).abs() + (previous.y - current.y).abs() <= 2.001);
            assert!(valid_cell(Cell::at(current).unwrap()));
            if world.external_arrived(index) {
                break;
            }
        }
        assert_eq!(world, loaded);
        assert_eq!(world.actors[index].position, goal);
        assert!(world.external_arrived(index));
        assert!(world.external_hold(index, false));
        assert!(world.validate());
    }

    #[test]
    fn orange_explores_safe_pond_approaches_without_user_input() {
        let mut world = LivingWorld::new(0);
        let mut visited = [false; CAT_SPOTS.len()];
        for _ in 0..(2 * DAY_MS / MINUTE) {
            world.advance_world_ms(MINUTE);
            let cat = &world.actors[ActorKind::Orange.index()];
            if cat.activity == Activity::Explore && cat.pose == Pose::Idle {
                if let Some(index) = CAT_SPOTS.iter().position(|&spot| cat.cell == spot) {
                    visited[index] = true;
                }
            }
            assert!(valid_cell(cat.cell));
        }
        assert_eq!(visited, [true; CAT_SPOTS.len()]);
        for point in [Cell(7, 17), Cell(15, 17), Cell(15, 23), Cell(7, 23)] {
            assert!(route(CAT_BED, point).is_some());
        }
    }

    #[test]
    fn fishing_uses_one_finite_pond_stock_and_has_empty_casts() {
        let mut world = LivingWorld::new(0);
        let fisherman = ActorKind::Ayu.index();
        world.actors[fisherman].cell = POND_BANK;
        world.actors[fisherman].position = POND_BANK.point();
        let mut successes = 0;
        let mut failures = 0;
        for minute in 0..12 {
            world.now_ms = START_MS + minute * MINUTE;
            let before = world.fish_in_pond;
            world.commit(fisherman, Task::Fish);
            if world.actors[fisherman].carried == Some(Item::Fish) {
                successes += 1;
                assert_eq!(world.fish_in_pond + 1, before);
                world.actors[fisherman].cell = PANTRY;
                world.commit(fisherman, Task::Store);
                world.actors[fisherman].cell = POND_BANK;
            } else {
                failures += 1;
                assert_eq!(world.fish_in_pond, before);
            }
        }
        assert_eq!(successes, 3);
        assert!(failures > 0);
        assert_eq!(world.stock.count(Item::Fish), successes);
        assert_eq!(world.fish_in_pond, 2);
    }

    #[test]
    fn cancelling_an_external_move_finishes_the_current_edge() {
        let mut world = LivingWorld::new(0);
        let kind = ActorKind::Xiaohe;
        let index = kind.index();
        let target = Point::at_tile(25, 17);
        assert!(world.reserve_activity(&[kind]));
        assert!(world.move_for_activity(kind, target));
        world.advance_world_ms(2_500);
        let position = world.actors[index].position;
        assert_ne!(position, world.actors[index].cell.point());
        world.recover_activity(&[kind]);
        assert_eq!(world.actors[index].position, position);
        assert_eq!(world.activity_deadline(kind), None);
        world.advance_world_ms(7_500);
        assert_eq!(
            world.actors[index].position,
            world.actors[index].cell.point()
        );
        assert_ne!(world.actors[index].position, target);
        assert!(!world.actors[index].outside);
        assert!(world.validate());
    }

    #[test]
    fn oversized_saved_route_is_rejected_before_world_validation() {
        let mut state = serde_json::to_value(LivingWorld::new(0)).unwrap();
        state["actors"][0]["route"] = serde_json::json!(vec![[0, 0]; MAX_ROUTE + 1]);
        assert!(serde_json::from_value::<LivingWorld>(state).is_err());
    }

    #[test]
    fn day_and_night_rates_follow_the_same_weighted_clock_on_reopen() {
        let mut morning = LivingWorld::new(10);
        morning.advance_to(11);
        assert_eq!(morning.world_ms(), START_MS + 48_000);
        morning.advance_world_ms(12 * HOUR - 48_000);
        drain(&mut morning);
        morning.advance_to(12);
        assert_eq!(morning.world_ms(), 20 * HOUR + 96_000);
        morning.advance_to(9);
        assert_eq!(morning.world_ms(), 20 * HOUR + 96_000);
        morning.advance_to(13);
        assert_eq!(morning.world_ms(), 20 * HOUR + 192_000);
    }

    #[test]
    fn host_can_pause_at_plugin_deadline_without_discarding_queued_time() {
        let mut world = LivingWorld::new_wall_ms(99);
        world.queue_wall_time(199);
        assert_eq!(world.pending_world_target_ms(), START_MS + 4_800);
        assert_eq!(world.world_ms(), START_MS);
        assert!(world.advance_pending_to(START_MS + 900));
        assert_eq!(world.world_ms(), START_MS + 900);
        assert!(world.is_catching_up());
        assert!(world.advance_pending_to(START_MS + 9_000));
        assert_eq!(world.world_ms(), START_MS + 4_800);
        assert!(!world.is_catching_up());
    }

    #[test]
    fn shared_lunch_waits_for_two_portions_at_two_seats_after_restore() {
        let mut world = LivingWorld::new(0);
        world.now_ms = 12 * HOUR;
        world.target_ms = world.now_ms;
        world.next_well = world.now_ms + HOUR;
        world.next_fish = world.now_ms + HOUR;
        world.next_needs = world.now_ms + HOUR;
        for actor in &mut world.actors {
            actor.until = world.now_ms;
            actor.hunger = 35;
        }
        let initial_food = world.stock.count(Item::Food);
        world.advance_world_ms(0);
        assert_eq!(world.stock.count(Item::Food), initial_food - 2);
        assert!(
            world.actors[..2]
                .iter()
                .all(|actor| actor.pose != Pose::Eat)
        );
        for _ in 0..200 {
            world.advance_world_ms(5_000);
            if world.actors[..2]
                .iter()
                .any(|actor| actor.stage == Stage::Meet)
            {
                break;
            }
        }
        assert!(
            world.actors[..2]
                .iter()
                .any(|actor| actor.stage == Stage::Meet)
        );
        assert!(
            world.actors[..2]
                .iter()
                .all(|actor| actor.pose != Pose::Eat)
        );
        let mut restored: LivingWorld =
            serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert!(restored.validate());
        for _ in 0..200 {
            world.advance_world_ms(5_000);
            restored.advance_world_ms(5_000);
            if world.actors[0].pose == Pose::Eat {
                break;
            }
        }
        assert_eq!(world, restored);
        for (actor, seat) in world.actors[..2].iter().zip(TABLE) {
            assert_eq!(
                (actor.pose, actor.position, actor.carried),
                (Pose::Eat, seat.point(), Some(Item::Food))
            );
        }
        world.advance_world_ms(10 * MINUTE);
        assert_eq!(world.meals, 2);
        assert!(
            world.actors[..2]
                .iter()
                .all(|actor| actor.carried.is_none())
        );
        assert_eq!(world.stock.count(Item::Food), initial_food - 2);
    }

    #[test]
    fn changing_clothes_and_sleep_happen_at_the_correct_doors() {
        let mut world = LivingWorld::new(0);
        let mut changed = [false; 2];
        let mut slept = [false; 2];
        let mut rested_outside = [false; 2];
        let mut prior = [Outfit::Garden, Outfit::Work];
        for _ in 0..(DAY_MS / 20_000) {
            world.advance_world_ms(20_000);
            for (i, actor) in world.actors[..2].iter().enumerate() {
                if actor.outfit() != prior[i] {
                    assert_eq!(actor.cell, [Cell(12, 14), Cell(24, 14)][i]);
                    assert!(matches!(
                        actor.activity,
                        Activity::ChangeClothes | Activity::Idle
                    ));
                    prior[i] = actor.outfit();
                    changed[i] = true;
                }
                if actor.pose == Pose::Sleep {
                    assert_eq!(actor.cell, [Cell(12, 14), Cell(24, 14)][i]);
                    assert_eq!(actor.outfit(), Outfit::Home);
                    assert!(actor.inside());
                    slept[i] = true;
                }
                if actor.task == Some(Task::Rest) && actor.pose == Pose::Rest {
                    assert!(!actor.inside());
                    rested_outside[i] = true;
                }
            }
        }
        assert_eq!(changed, [true, true]);
        assert_eq!(slept, [true, true]);
        assert_eq!(rested_outside, [true, true]);
    }

    #[test]
    fn full_world_year_preserves_bounds_and_protected_terrain() {
        let mut world = LivingWorld::new(0);
        let initial_timber = world.trees.iter().map(|tree| tree.timber).sum::<u16>();
        world.advance_world_ms(365 * DAY_MS);
        let mut checkpoints = 0;
        while world.is_catching_up() {
            assert!(world.validate());
            assert!(serde_json::to_vec(&world).unwrap().len() < 10_000);
            for actor in world.actors() {
                let point = Cell::at(actor.position).unwrap();
                assert!(world.can_walk(i32::from(point.0), i32::from(point.1)));
                assert_ne!(
                    world.ground(i32::from(point.0), i32::from(point.1)),
                    Ground::Building
                );
            }
            world.advance_world_ms(0);
            checkpoints += 1;
            assert!(checkpoints < 500);
        }
        assert_eq!(world.world_ms(), START_MS + 365 * DAY_MS);
        assert_eq!(
            world.story.completed(),
            24,
            "story: {:?}, stock: {:?}",
            world.story,
            world.stock
        );
        assert_eq!(world.story.milestones(), 0b1111);
        assert!(world.story.market_trades() > 8 && world.story.garden_visits() > 0);
        assert!(world.stock.count(Item::Crops) < 35);
        let saved = serde_json::to_vec(&world).unwrap();
        assert!(saved.len() < 10_000);
        assert!(world.validate());
        assert!(world.meals > 500);
        let remaining_timber = world.trees.iter().map(|tree| tree.timber).sum::<u16>();
        assert!(world.stock.count(Item::Wood) <= initial_timber - remaining_timber);
        assert!(!world.can_walk(12, 12));
        assert!(!world.can_walk(12, 20));
        assert!(world.can_walk(12, 14));
        assert!(world.can_walk(12, 17));
    }

    #[test]
    fn building_fixtures_use_materials_at_the_original_site_and_wait_a_day() {
        let mut world = LivingWorld::new(0);
        let work_site = world.improvement_site(Fixture::KitchenBatchStove);
        let phase_zero = ImprovementOrder {
            fixture: Fixture::KitchenBatchStove,
            phase: 0,
        };
        let phase_one = ImprovementOrder {
            phase: 1,
            ..phase_zero
        };
        let actor = ActorKind::Ayu.index();
        let kitchen = usize::from(Fixture::KitchenBatchStove.site());
        world.buildings[kitchen].level = 2;
        world.now_ms += 8 * DAY_MS;
        world.target_ms = world.now_ms;
        world.actors[actor].cell = PANTRY;
        assert!(world.take_stock(Item::Stone, 2));
        world.carry(actor, Item::Stone, 2);
        world.commit(actor, Task::Improve(phase_zero));
        assert_eq!(world.story.improvements.active(), None);
        assert!(world.has_carry(actor, Item::Stone, 2));
        world.actors[actor].cell = work_site;
        world.commit(actor, Task::Improve(phase_zero));
        assert_eq!(world.story.improvements.active(), Some(phase_one));
        assert!(world.actors[actor].carried.is_none());
        assert_eq!(world.building_fixtures().count(), 0);
        world.carry(actor, Item::Wood, 1);
        world.commit(actor, Task::Improve(phase_one));
        assert!(world.has_carry(actor, Item::Wood, 1));
        world.now_ms += DAY_MS;
        world.commit(actor, Task::Improve(phase_one));
        assert!(world.actors[actor].carried.is_none());
        assert_eq!(world.story.improvements.completed(), 1);
        assert_eq!(
            world.building_fixtures().next().unwrap().0.fixture,
            Fixture::KitchenBatchStove
        );
        assert_eq!(world.buildings[kitchen].level, 2);
    }

    #[test]
    fn building_fixtures_change_real_recipe_capacity_and_use_count() {
        let mut world = LivingWorld::new(0);
        let kitchen = usize::from(Fixture::KitchenBatchStove.site());
        world.buildings[kitchen].level = 2;
        let phase_zero = ImprovementOrder {
            fixture: Fixture::KitchenBatchStove,
            phase: 0,
        };
        let phase_one = ImprovementOrder {
            phase: 1,
            ..phase_zero
        };
        world.now_ms += 8 * DAY_MS;
        world.story.improvements.commit(phase_zero, world.now_ms);
        world.now_ms += DAY_MS;
        world.story.improvements.commit(phase_one, world.now_ms);
        assert!(world.stock.add(Item::Crops, 3));
        assert_eq!(world.input(Task::Cook), Some((Item::Crops, 3)));
        let i = ActorKind::Ayu.index();
        world.actors[i].cell = STOVE;
        assert!(world.take_stock(Item::Crops, 3));
        world.carry(i, Item::Crops, 3);
        world.commit(i, Task::Cook);
        assert!(world.has_carry(i, Item::Food, 7));
        assert_eq!(
            world
                .story
                .improvements
                .installed()
                .next()
                .unwrap()
                .uses_left,
            47
        );
    }

    #[test]
    fn building_structures_advance_in_three_paid_stages_and_stop_at_four() {
        let mut world = LivingWorld::new(0);
        for building in &mut world.buildings {
            building.level = 2;
        }
        assert!(world.stock.add(Item::Wood, 3));
        let site = Fixture::WorkshopJig.site() as usize;
        let actor = ActorKind::Ayu.index();
        world.actors[actor].cell = BUILDINGS[site].1;
        let task = Task::Build(site as u8);
        world.now_ms += 21 * DAY_MS;
        assert_eq!(world.structural_candidate(), Some((site, 2, 0)));
        for phase in 0..3 {
            let (item, count) = world.build_input(site);
            assert!(world.take_stock(item, count));
            world.carry(actor, item, count);
            world.commit(actor, task);
            assert!(world.actors[actor].carried.is_none());
            if phase < 2 {
                assert_eq!(world.buildings[site].level, 2);
                assert_eq!(world.buildings[site].step, phase + 1);
                assert_eq!(world.structural_candidate(), None);
                world.now_ms += 2 * DAY_MS;
            }
        }
        assert_eq!(world.buildings[site].level, 3);
        assert_eq!(world.buildings[site].position, BUILDINGS[site].1.point());
        assert_eq!(
            world.story.improvements.structure_due(site),
            world.now_ms + 14 * DAY_MS
        );
        world.buildings[site].level = MAX_STRUCTURE_LEVEL;
        world.carry(actor, Item::Stone, 2);
        world.commit(actor, task);
        assert_eq!(world.buildings[site].level, MAX_STRUCTURE_LEVEL);
        assert!(world.has_carry(actor, Item::Stone, 2));
    }

    #[test]
    fn building_fixtures_continue_after_a_year_and_slices_replay_the_same() {
        let mut world = LivingWorld::new(0);
        for _ in 0..365 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
        }
        assert!(world.validate());
        assert_eq!(world.story.completed(), CHAPTERS.len());
        assert!(
            world
                .buildings
                .iter()
                .all(|building| building.level == MAX_STRUCTURE_LEVEL)
        );
        let first = world.story.improvements.completed();
        assert!(
            first >= 6,
            "only {first} fixtures in a year: {:?}",
            world.story.improvements
        );
        assert!(world.story.improvements.conversions() > 0);
        assert_eq!(
            world.home_of(ActorKind::Xiaohe),
            world.home_of(ActorKind::Ayu)
        );
        assert_ne!(
            world.home_of(ActorKind::Xiaohe),
            Some(HOUSE_DOORS[0].point())
        );
        eprintln!(
            "day 365: structure {:?}, fittings {} repairs {} conversions {}",
            world.buildings.map(|building| building.level),
            first,
            world.story.improvements.repairs(),
            world.story.improvements.conversions(),
        );
        let snapshot = serde_json::to_vec(&world).unwrap();
        let mut restored: LivingWorld = serde_json::from_slice(&snapshot).unwrap();
        for _ in 0..120 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
            for _ in 0..4 {
                restored.advance_world_ms(DAY_MS / 4);
                drain(&mut restored);
            }
        }
        assert_eq!(world, restored);
        assert!(
            world
                .buildings
                .iter()
                .all(|building| building.level == MAX_STRUCTURE_LEVEL)
        );
        assert!(world.story.improvements.completed() > first);
        eprintln!(
            "day 485: structure {:?}, fittings {} repairs {} conversions {}",
            world.buildings.map(|building| building.level),
            world.story.improvements.completed(),
            world.story.improvements.repairs(),
            world.story.improvements.conversions(),
        );
        assert!(world.story.improvements.installed().count() <= story::IMPROVEMENT_SITES);
        assert!(serde_json::to_vec(&world.story.improvements).unwrap().len() < 2_048);
        let mut legacy = serde_json::to_value(world).unwrap();
        legacy["story"]
            .as_object_mut()
            .unwrap()
            .remove("improvements");
        let decoded: LivingWorld = serde_json::from_value(legacy).unwrap();
        assert_eq!(decoded.story.improvements.completed(), 0);
    }

    #[test]
    fn housing_lot_rejects_occupied_cells_and_uses_larger_shared_footprint() {
        let mut world = LivingWorld::new(0);
        world.now_ms += 110 * DAY_MS;
        let order = world.story.housing.proposals(world.now_ms).next().unwrap();
        assert_eq!(order.plan.footprint(), [4, 3]);
        assert_eq!(HousePlan::Shared.footprint(), [6, 4]);
        assert!(world.housing_site_legal(order));
        let [x, y] = order.lot_origin();
        world.actors[0].cell = Cell((x + 1).into(), (y + 1).into());
        assert!(!world.housing_site_legal(order));
        world.actors[0].cell = HOUSE_SIDE[0];
        world.story.housing.commit(order, 1, world.now_ms);
        world.now_ms += 3 * DAY_MS;
        let foundation = world.story.housing.next_order(world.now_ms).unwrap();
        let [door_x, door_y] = foundation.door();
        world.actors[1].cell = Cell(door_x.into(), door_y.into());
        world.carry(1, Item::Stone, 3);
        let revision = world.terrain_revision();
        world.commit(1, Task::BuildHome(foundation));
        assert_ne!(world.terrain_revision(), revision);
        assert!(!world.can_walk(x.into(), y.into()));
        assert_eq!(world.ground(x.into(), y.into()), Ground::Building);
        assert_eq!(
            world.yard_surface(door_x.into(), door_y.into()),
            Some(YardSurface::Doorstep)
        );
        assert!(world.can_walk(door_x.into(), door_y.into()));
        assert!(
            world
                .route(PANTRY, Cell(door_x.into(), door_y.into()))
                .is_some()
        );
        assert_eq!(
            world.home_of(ActorKind::Xiaohe),
            Some(HOUSE_DOORS[0].point())
        );
    }

    #[test]
    fn housing_sites_preserve_sightlines_including_roof_and_leave_two_approaches() {
        let world = LivingWorld::new(0);
        for plan in [HousePlan::Single, HousePlan::Shared] {
            let orders = std::array::from_fn::<_, 3, _>(|lot| HousingOrder::new(plan, lot as u8));
            for (i, order) in orders.into_iter().enumerate() {
                assert!(world.housing_site_legal(order), "{order:?}");
                let [x, y] = order.door();
                for side in [x - 1, x + 1] {
                    assert!(world.can_walk(side.into(), y.into()));
                    assert!(world.route(PANTRY, Cell(side.into(), y.into())).is_some());
                }
                for other in orders.into_iter().skip(i + 1) {
                    assert!(
                        housing_sight_bounds(order).separated_from(housing_sight_bounds(other), 8)
                    );
                }
            }
        }

        let legacy: HousingOrder =
            serde_json::from_str(r#"{"plan":"single","lot":0,"phase":"foundation"}"#).unwrap();
        assert_eq!(legacy.lot_origin(), [16, 5]);
        assert!(!housing_sight_clear_of_buildings(legacy, WORKSHOP_DOOR));
    }

    #[test]
    fn housing_legacy_lot_keeps_position_for_inflight_orders_and_existing_homes() {
        let mut world = LivingWorld::new(0);
        world.now_ms += 110 * DAY_MS;
        let legacy: HousingOrder =
            serde_json::from_str(r#"{"plan":"single","lot":0,"phase":"clear"}"#).unwrap();
        assert!(world.story.housing.accepts(legacy, world.now_ms));
        assert!(world.housing_site_legal(legacy));
        world.story.housing.commit(legacy, 1, world.now_ms);
        while let Some(order) = world.story.housing.next_order(world.now_ms + 4 * DAY_MS) {
            world.now_ms += 4 * DAY_MS;
            world.story.housing.commit(
                order,
                if order.phase == HousingPhase::MoveIn {
                    0
                } else {
                    1
                },
                world.now_ms,
            );
        }
        assert!(world.story.housing.validate(world.now_ms));
        assert_eq!(
            world.home_of(ActorKind::Xiaohe),
            Some(Point::at_tile(18, 8))
        );
        let restored: story::HousingState =
            serde_json::from_slice(&serde_json::to_vec(&world.story.housing).unwrap()).unwrap();
        world.story.housing = restored;
        assert_eq!(
            world.home_of(ActorKind::Xiaohe),
            Some(Point::at_tile(18, 8))
        );
        assert_ne!(
            HousingOrder::new(HousePlan::Single, 0).door(),
            legacy.door()
        );
    }

    #[test]
    fn yard_surfaces_connect_doors_work_area_and_bank_without_a_parallel_road() {
        let world = LivingWorld::new(0);
        assert_eq!(world.yard_surface(12, 14), Some(YardSurface::Doorstep));
        assert_eq!(world.yard_surface(24, 14), Some(YardSurface::Doorstep));
        assert_eq!(world.yard_surface(19, 12), Some(YardSurface::Doorstep));
        assert_eq!(world.yard_surface(16, 16), Some(YardSurface::Dining));
        assert_eq!(world.yard_surface(36, 13), Some(YardSurface::Loading));
        assert_eq!(world.yard_surface(31, 17), Some(YardSurface::Loading));
        assert_eq!(world.yard_surface(12, 17), Some(YardSurface::Bank));
        assert_eq!(world.yard_surface(24, 20), Some(YardSurface::GardenAisle));
        assert_eq!(world.ground(16, 17), Ground::Grass);
        assert_eq!(world.ground(20, 17), Ground::Grass);
        assert_eq!(world.ground(28, 17), Ground::Grass);
        assert_eq!(world.ground(6, 22), Ground::Path);
        assert_eq!(world.ground(7, 22), Ground::Path);
        assert_eq!(world.ground(5, 22), Ground::Grass);
        assert_eq!(world.ground(8, 22), Ground::Grass);
        assert!(world.can_walk(5, 22) && world.can_walk(8, 22));
        for (start, end) in [
            (HOUSE_DOORS[0], TABLE[0]),
            (HOUSE_DOORS[1], TABLE[1]),
            (STOVE, TABLE[0]),
            (TABLE[0], POND_BANK),
            (PANTRY, BUILDINGS[4].1),
            (PANTRY, WELL),
            (POND_BANK, WELL),
        ] {
            assert!(world.route(start, end).is_some(), "{start:?} to {end:?}");
        }
        for x in 9..14 {
            for y in 18..22 {
                assert_eq!(world.ground(x, y), Ground::Water);
                assert!(!world.can_walk(x, y));
            }
        }
    }

    #[test]
    fn legacy_workshop_waits_for_cargo_and_routes_then_moves_without_resetting_state() {
        let mut world = LivingWorld::new(123);
        world.buildings[WORKSHOP_SITE].position = LEGACY_WORKSHOP_DOOR.point();
        world.stock.add(Item::Wood, 3);
        let worker = ActorKind::Ayu.index();
        let task = Task::Build(WORKSHOP_SITE as u8);
        assert!(world.begin(worker, task));
        assert_eq!(world.site(worker, task), LEGACY_WORKSHOP_DOOR);
        assert!(!world.relocate_legacy_workshop(true));
        assert!(world.validate());
        for _ in 0..200 {
            let next = world.next_update_ms();
            world.advance_world_ms(next.saturating_sub(world.world_ms()));
            if world.actors[worker].carried == Some(Item::Stone)
                && world.actors[worker].stage == Stage::WalkToSite
            {
                break;
            }
        }
        assert_eq!(world.actors[worker].carried, Some(Item::Stone));
        let mut loaded: LivingWorld =
            serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert!(loaded.validate());
        assert_eq!(loaded.actors[worker], world.actors[worker]);
        assert!(!loaded.relocate_legacy_workshop(true));
        let end = loaded.world_ms() + 3 * DAY_MS;
        loop {
            if loaded.workshop_relocation_tiles().is_none() {
                panic!("the saved workshop was already moved");
            }
            let before = loaded.clone();
            assert!(!loaded.relocate_legacy_workshop(false));
            if loaded.relocate_legacy_workshop(true) {
                assert_eq!(loaded.actors, before.actors);
                assert_eq!(loaded.stock, before.stock);
                assert_eq!(loaded.world_ms(), before.world_ms());
                assert_eq!(
                    loaded.buildings[WORKSHOP_SITE].level,
                    before.buildings[WORKSHOP_SITE].level
                );
                assert_eq!(
                    loaded.buildings[WORKSHOP_SITE].progress,
                    before.buildings[WORKSHOP_SITE].progress
                );
                assert!(!loaded.can_walk(35, 12) && !loaded.can_walk(36, 12));
                assert!(loaded.can_walk(33, 12) && loaded.can_walk(34, 12));
                assert!(loaded.can_walk(36, 13));
                assert_eq!(loaded.site(worker, task), WORKSHOP_DOOR);
                assert!(loaded.validate());
                return;
            }
            assert_eq!(loaded, before);
            assert!(loaded.world_ms() < end);
            let next = loaded.next_update_ms().max(loaded.world_ms() + 1);
            loaded.advance_world_ms(next - loaded.world_ms());
        }
    }

    #[test]
    fn housing_move_uses_old_door_and_box_and_restores_mid_route() {
        let mut world = LivingWorld::new(0);
        for _ in 0..225 {
            world.advance_world_ms(DAY_MS);
            drain(&mut world);
        }
        let initial = world.home_of(ActorKind::Xiaohe);
        let original_ayu = world.home_of(ActorKind::Ayu);
        assert_ne!(initial, original_ayu);
        let deadline = world.world_ms() + 140 * DAY_MS;
        while world.world_ms() < deadline
            && !world.actors[0].task.is_some_and(|task| {
                matches!(task, Task::BuildHome(order) if order.phase == HousingPhase::MoveIn && order.plan == HousePlan::Shared)
                    && world.actors[0].has_moving_box()
            })
        {
            let next = world.next_update_ms().max(world.world_ms() + 1);
            world.advance_world_ms(next - world.world_ms());
        }
        assert!(world.actors[0].has_moving_box());
        assert_eq!(world.home_of(ActorKind::Xiaohe), initial);
        assert_eq!(world.home_of(ActorKind::Ayu), original_ayu);
        assert!(world.validate());
        let mut restored: LivingWorld =
            serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert_eq!(restored, world);
        while world.world_ms() < deadline
            && world.home_of(ActorKind::Xiaohe) != world.home_of(ActorKind::Ayu)
        {
            let next = world.next_update_ms().max(world.world_ms() + 1);
            let delta = next - world.world_ms();
            world.advance_world_ms(delta);
            restored.advance_world_ms(delta / 2);
            restored.advance_world_ms(delta - delta / 2);
        }
        assert_eq!(world, restored);
        assert_eq!(
            world.home_of(ActorKind::Xiaohe),
            world.home_of(ActorKind::Ayu)
        );
        assert_eq!(
            world.previous_home_use(ActorKind::Xiaohe),
            Some(FormerHomeUse::GrainStorage)
        );
        assert_eq!(
            world.previous_home_use(ActorKind::Ayu),
            Some(FormerHomeUse::ToolRoom)
        );
        assert_eq!(world.site(1, Task::ReturnTool), HOUSE_DOORS[1]);
        assert_eq!(
            world.housing_sites().filter(|site| site.occupied).count(),
            1
        );
        assert!(world.validate());
    }

    #[test]
    fn bridge_requires_transported_material_five_shifts_and_twenty_days() {
        let mut world = LivingWorld::new(0);
        world.now_ms = 19 * DAY_MS;
        world.story.finished = 5;
        let id = 5;
        let actor = 1;
        world.actors[actor].cell = world.site(actor, Task::Develop(id));
        assert!(!world.can_walk(12, 31));
        world.commit(actor, Task::Develop(id));
        assert!(world.story.active().is_none());
        assert!(world.take_stock(Item::Rope, 1));
        world.carry(actor, Item::Rope, 1);
        world.commit(actor, Task::Develop(id));
        assert_eq!(world.story.running.unwrap().shifts, 1);
        assert!(world.actors[actor].carried.is_none());
        world.commit(actor, Task::Develop(id));
        assert_eq!(world.story.running.unwrap().shifts, 1);
        let started = world.now_ms;
        for _ in 0..4 {
            world.now_ms += DAY_MS / 2;
            world.commit(actor, Task::Develop(id));
        }
        world.now_ms = started + 20 * DAY_MS - 1;
        world.finish_development();
        assert_eq!(world.story.completed(), 5);
        assert!(!world.can_walk(12, 31));
        world.now_ms += 1;
        world.finish_development();
        assert_eq!(world.story.completed(), 6);
        assert!(world.can_walk(12, 31));
        assert!(world.route(Cell(12, 28), MARKET_TRADE).is_some());
    }

    #[test]
    fn legacy_crop_quantity_is_retained_and_new_crops_expire_fifo() {
        let mut old = serde_json::to_value(LivingWorld::new(0)).unwrap();
        old.as_object_mut().unwrap().remove("story");
        old.as_object_mut().unwrap().remove("crop_batches");
        old["stock"][Item::Crops as usize] = 12.into();
        let mut world: LivingWorld = serde_json::from_value(old).unwrap();
        assert!(world.validate());
        assert_eq!(world.stock.count(Item::Crops), 12);
        assert!(world.exchange_stock_atomic(&[], &[(Item::Crops, 3)]));
        assert!(world.spend_stock(Item::Crops, 11));
        assert_eq!(world.crop_batches.units(), 3);
        let expires = world.crop_batches.next_expiry().unwrap();
        world.now_ms = DAY_MS;
        assert!(world.exchange_stock_atomic(&[], &[(Item::Crops, 3)]));
        assert!(!world.exchange_stock_atomic(&[(Item::Crops, 2)], &[(Item::Stone, 128)]));
        assert_eq!(world.stock.count(Item::Crops), 7);
        assert!(world.spend_stock(Item::Crops, 2));
        world.now_ms = expires;
        world.environment();
        assert_eq!(world.story.waste(), 2);
        assert_eq!(world.stock.count(Item::Crops), 3);
        assert_eq!(world.crop_batches.units(), 3);
    }
}
