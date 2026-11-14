//! Fixed-capacity visitor appointments. The household owns all stock writes.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::living_world::{
    ActorKind, Ground, Item, LivingWorld, PlotStage, Point, TILE_PX, YardWeather,
    movement::{Pace, WalkConditions, edge_duration, edge_fraction, valid_travel_edge},
};

pub const MAX_HUMAN_VISITORS: usize = 8;
const MINUTE: u64 = 60_000;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
const WINDOW: u64 = 3 * HOUR;
const WALK_TIME: u64 = 10_000;
const POLL_TIME: u64 = MINUTE;
const MAX_STAY: u64 = 150 * MINUTE;
const COLS: usize = 48;
const ROWS: usize = 32;
const CELLS: usize = COLS * ROWS;
const ENTRY: Cell = Cell(45, 14);

// Cumulative u32-domain weights from the discrete truncated normal in the
// visitor specification. The end point is 2^32 and requires u64 storage.
const STANDARD: [u64; 9] = [
    36_457_175,
    200_000_192,
    677_450_337,
    1_585_228_478,
    2_709_946_818,
    3_617_724_959,
    4_095_175_104,
    4_258_718_121,
    4_294_967_296,
];
const QUIET: [u64; 7] = [
    261_891_584,
    1_306_606_356,
    2_961_538_573,
    4_006_253_346,
    4_268_144_930,
    4_293_976_599,
    4_294_967_296,
];
const NIGHT: [u64; 5] = [
    1_367_210_523,
    3_430_528_943,
    4_218_886_089,
    4_293_303_505,
    4_294_967_296,
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Profession {
    MailCarrier,
    SeedMerchant,
    Carpenter,
    Cook,
    Fisher,
    Weaver,
    FruitGrower,
    Florist,
    TravelingPainter,
    RoadWorker,
    FarmerApprentice,
    Tailor,
    Roofer,
    ScaleMerchant,
    Baker,
    ReedWeaver,
    Irrigator,
    BirdObserver,
    Potter,
    Courier,
}

const PROFESSIONS: [Profession; 10] = [
    Profession::MailCarrier,
    Profession::SeedMerchant,
    Profession::Carpenter,
    Profession::Cook,
    Profession::Fisher,
    Profession::Weaver,
    Profession::FruitGrower,
    Profession::Florist,
    Profession::TravelingPainter,
    Profession::RoadWorker,
];

impl Profession {
    pub const fn resident(index: usize) -> Self {
        const RESIDENTS: [Profession; 20] = [
            Profession::MailCarrier,
            Profession::SeedMerchant,
            Profession::Carpenter,
            Profession::Cook,
            Profession::Fisher,
            Profession::Weaver,
            Profession::FruitGrower,
            Profession::Florist,
            Profession::TravelingPainter,
            Profession::RoadWorker,
            Profession::FarmerApprentice,
            Profession::Tailor,
            Profession::Roofer,
            Profession::ScaleMerchant,
            Profession::Baker,
            Profession::ReedWeaver,
            Profession::Irrigator,
            Profession::BirdObserver,
            Profession::Potter,
            Profession::Courier,
        ];
        RESIDENTS[index]
    }

    pub const fn asset_name(self) -> &'static str {
        match self {
            Self::MailCarrier => "mail_carrier",
            Self::SeedMerchant => "seed_merchant",
            Self::Carpenter => "carpenter",
            Self::Cook => "cook",
            Self::Fisher => "fisher",
            Self::Weaver => "weaver",
            Self::FruitGrower => "fruit_grower",
            Self::Florist => "florist",
            Self::TravelingPainter => "traveling_painter",
            Self::RoadWorker => "road_worker",
            Self::FarmerApprentice => "farmer_apprentice",
            Self::Tailor => "tailor",
            Self::Roofer => "roofer",
            Self::ScaleMerchant => "scale_merchant",
            Self::Baker => "baker",
            Self::ReedWeaver => "reed_weaver",
            Self::Irrigator => "irrigator",
            Self::BirdObserver => "bird_observer",
            Self::Potter => "potter",
            Self::Courier => "courier",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MailCarrier => "米信·邮差",
            Self::SeedMerchant => "谷芽·种商",
            Self::Carpenter => "槐青·木匠",
            Self::Cook => "苏灶·厨师",
            Self::Fisher => "陆澜·渔人",
            Self::Weaver => "青梭·织工",
            Self::FruitGrower => "林枝·果农",
            Self::Florist => "花晴·花匠",
            Self::TravelingPainter => "夏墨·旅行画家",
            Self::RoadWorker => "石砾·修路人",
            Self::FarmerApprentice => "阿谷·耕作学徒",
            Self::Tailor => "青缝·裁缝",
            Self::Roofer => "梁雨·瓦匠",
            Self::ScaleMerchant => "邵衡·秤商",
            Self::Baker => "白柚·烘焙师",
            Self::ReedWeaver => "许苇·芦编匠",
            Self::Irrigator => "祁苗·园圃水工",
            Self::BirdObserver => "栖雨·鸟类观察员",
            Self::Potter => "唐铃·陶工",
            Self::Courier => "余舟·递货员",
        }
    }

    fn plan(self) -> Plan {
        use Item::*;
        use Profession::*;
        let trade = |site, minutes, activity, costs, goods| Plan {
            site,
            duration: minutes * MINUTE,
            activity,
            purpose: Purpose::Trade { costs, goods },
        };
        let inspect = |site, minutes, activity, report| Plan {
            site,
            duration: minutes * MINUTE,
            activity,
            purpose: Purpose::Inspect(report),
        };
        match self {
            MailCarrier => inspect(Cell(29, 15), 5, "核对仓前投递点", ReportKind::DeliverySite),
            SeedMerchant => trade(Cell(31, 15), 12, "换种子", (Crops, 2), (Seeds, 3)),
            Carpenter => trade(Cell(34, 14), 14, "交接带来的木料", (Food, 2), (Wood, 2)),
            Cook => trade(Cell(18, 14), 12, "交接外村熟食", (Crops, 3), (Food, 2)),
            Fisher => trade(Cell(10, 17), 14, "交接外村鲜鱼", (Food, 2), (Fish, 1)),
            Weaver => trade(Cell(33, 14), 14, "交接织好的绳索", (Crops, 3), (Rope, 1)),
            FruitGrower => trade(Cell(36, 14), 12, "用绳索换外村收成", (Rope, 1), (Crops, 2)),
            Florist => inspect(Cell(22, 17), 15, "记录地块植被", ReportKind::PlantCover),
            TravelingPainter => inspect(Cell(15, 17), 20, "观察池岸水面", ReportKind::PondStudy),
            RoadWorker => inspect(Cell(37, 14), 14, "测查进村道路", ReportKind::RoadSurvey),
            _ => unreachable!("only the original professions enter the legacy visitor scheduler"),
        }
    }
}

struct Plan {
    site: Cell,
    duration: u64,
    activity: &'static str,
    purpose: Purpose,
}

#[derive(Clone, Copy)]
enum Purpose {
    Trade {
        costs: (Item, u16),
        goods: (Item, u16),
    },
    Inspect(ReportKind),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    DeliverySite,
    PlantCover,
    PondStudy,
    RoadSurvey,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum VisitOutcome {
    Trade {
        paid: Item,
        paid_units: u16,
        received: Item,
        received_units: u16,
    },
    Report {
        kind: ReportKind,
        observed: u16,
    },
    LeftWithoutExchange,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VisitReceipt {
    pub profession: Profession,
    pub world_ms: u64,
    pub result: VisitOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisitorPose {
    Walking,
    Waiting,
    Working,
    Trading,
    Carrying,
    Resting,
}

impl VisitorPose {
    pub const fn asset_name(self) -> &'static str {
        match self {
            Self::Walking => "walk",
            Self::Waiting => "idle",
            Self::Working => "work",
            Self::Trading => "greet",
            Self::Carrying => "carry",
            Self::Resting => "rest",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VisitorVisual {
    pub profession: Profession,
    pub pose: VisitorPose,
    pub position: Point,
    pub facing_left: bool,
    pub animation_frame: usize,
    pub label: &'static str,
    pub activity: &'static str,
    pub held: Option<VisitorHeld>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum VisitorTool {
    Hammer,
    Rod,
    Hoe,
    WateringCan,
    Basket,
    Knife,
    RopeKit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisitorHeld {
    Goods(Item),
    Tool(VisitorTool),
    MoveBox,
    Parcel(&'static str),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Cell(u8, u8);

impl Cell {
    fn from_point(point: Point) -> Option<Self> {
        if point.x < 0.0 || point.y < 0.0 || !point.x.is_finite() || !point.y.is_finite() {
            return None;
        }
        let x = (point.x / TILE_PX as f32) as usize;
        let y = (point.y / TILE_PX as f32) as usize;
        (x < COLS && y < ROWS).then_some(Self(x as u8, y as u8))
    }

    fn index(self) -> usize {
        usize::from(self.1) * COLS + usize::from(self.0)
    }

    fn inside(self) -> bool {
        usize::from(self.0) < COLS && usize::from(self.1) < ROWS
    }

    fn walkable(self, world: &LivingWorld) -> bool {
        self.inside() && world.can_walk(self.0.into(), self.1.into())
    }

    fn point(self) -> Point {
        Point::at_tile(self.0.into(), self.1.into())
    }

    fn adjacent(self, other: Self) -> bool {
        u16::from(self.0.abs_diff(other.0)) + u16::from(self.1.abs_diff(other.1)) == 1
    }

    fn at_offset(self, x: i16, y: i16) -> Option<Self> {
        let x = i16::from(self.0) + x;
        let y = i16::from(self.1) + y;
        ((0..COLS as i16).contains(&x) && (0..ROWS as i16).contains(&y))
            .then_some(Self(x as u8, y as u8))
    }

    fn trading_distance(self, other: Self) -> bool {
        let dx = self.0.abs_diff(other.0);
        let dy = self.1.abs_diff(other.1);
        (dx == 0 && (1..=2).contains(&dy)) || (dy == 0 && (1..=2).contains(&dx))
    }

    fn handoff_sites(self) -> impl Iterator<Item = Self> {
        [
            (-2, 0),
            (2, 0),
            (0, -2),
            (0, 2),
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
        ]
        .into_iter()
        .filter_map(move |(x, y)| self.at_offset(x, y))
    }

    fn neighbors(self) -> impl Iterator<Item = Self> {
        let (x, y) = (i16::from(self.0), i16::from(self.1));
        [(x - 1, y), (x, y - 1), (x, y + 1), (x + 1, y)]
            .into_iter()
            .filter_map(|(x, y)| {
                ((0..COLS as i16).contains(&x) && (0..ROWS as i16).contains(&y))
                    .then_some(Self(x as u8, y as u8))
            })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Stage {
    Arriving,
    Waiting,
    Meeting,
    Working,
    Departing,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Edge {
    to: Cell,
    started: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Visit {
    profession: Profession,
    stage: Stage,
    cell: Cell,
    edge: Option<Edge>,
    admitted: u64,
    due: u64,
    cargo: u16,
    facing_left: bool,
    partner: Option<ActorKind>,
    rendezvous: Option<Cell>,
}

impl Visit {
    fn visual(self, now: u64) -> VisitorVisual {
        let mut position = self.cell.point();
        if let Some(edge) = self.edge {
            let next = edge.to.point();
            let t = edge_fraction(now, edge.started, self.due);
            position.x += (next.x - position.x) * t;
            position.y += (next.y - position.y) * t;
        }
        let plan = self.profession.plan();
        let walking = if self.cargo == 0 {
            VisitorPose::Walking
        } else {
            VisitorPose::Carrying
        };
        let (pose, activity) = match (self.stage, self.edge.is_some()) {
            (Stage::Arriving, true) => (walking, "赴约"),
            (Stage::Waiting | Stage::Meeting, _) => (VisitorPose::Resting, "等候会合"),
            (Stage::Arriving, false) => (VisitorPose::Waiting, "等候"),
            (Stage::Working, _) => {
                let pose = if matches!(plan.purpose, Purpose::Trade { .. }) {
                    VisitorPose::Trading
                } else {
                    VisitorPose::Working
                };
                (pose, plan.activity)
            }
            (Stage::Departing, true) => (walking, "沿路回村"),
            (Stage::Departing, false) => (VisitorPose::Waiting, "等路通行"),
        };
        VisitorVisual {
            profession: self.profession,
            pose,
            position,
            facing_left: self.facing_left,
            animation_frame: self.edge.map_or(0, |edge| {
                ((edge_fraction(now, edge.started, self.due) * 3.0) as usize).min(2)
            }),
            label: self.profession.label(),
            activity,
            held: match plan.purpose {
                Purpose::Trade { goods, .. } if self.cargo != 0 => {
                    Some(VisitorHeld::Goods(goods.0))
                }
                _ => None,
            },
        }
    }

    fn occupied(self, cell: Cell) -> bool {
        self.cell == cell || self.edge.is_some_and(|edge| edge.to == cell)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VisitorState {
    seed: u64,
    slots: [Option<Visit>; MAX_HUMAN_VISITORS],
    next_admission: u64,
    last_departed: [Option<u64>; PROFESSIONS.len()],
    visits: u64,
    trades: u64,
    reports: u64,
    last_receipt: Option<VisitReceipt>,
    last_window: Option<u64>,
    desired: u8,
}

impl Default for VisitorState {
    fn default() -> Self {
        Self {
            seed: 0x5649_5349_544f_5253,
            slots: [None; MAX_HUMAN_VISITORS],
            next_admission: 0,
            last_departed: [None; PROFESSIONS.len()],
            visits: 0,
            trades: 0,
            reports: 0,
            last_receipt: None,
            last_window: None,
            desired: 0,
        }
    }
}

impl VisitorState {
    pub fn validate(&self) -> bool {
        self.desired as usize <= MAX_HUMAN_VISITORS
            && self.trades.saturating_add(self.reports) <= self.visits
            && self.slots.iter().enumerate().all(|(index, visit)| {
                let Some(visit) = visit else {
                    return true;
                };
                if !PROFESSIONS.contains(&visit.profession) {
                    return false;
                }
                let plan = visit.profession.plan();
                let cargo_valid = match plan.purpose {
                    Purpose::Trade { goods, .. } if visit.stage == Stage::Departing => {
                        matches!(visit.cargo, 0) || visit.cargo == goods.1
                    }
                    Purpose::Trade { goods, .. } => visit.cargo == goods.1,
                    Purpose::Inspect(_) => visit.cargo == 0,
                };
                let meeting_valid = match (visit.partner, visit.rendezvous) {
                    (Some(partner), Some(site)) => {
                        matches!(partner, ActorKind::Xiaohe | ActorKind::Ayu)
                            && matches!(visit.stage, Stage::Meeting | Stage::Working)
                            && matches!(plan.purpose, Purpose::Trade { .. })
                            && site.inside()
                            && site.trading_distance(visit.cell)
                            && self.slots.iter().enumerate().all(|(other_index, other)| {
                                other_index == index
                                    || other.is_none_or(|other| {
                                        other.partner.is_none() && !other.occupied(site)
                                    })
                            })
                    }
                    (None, None) => {
                        visit.stage != Stage::Meeting
                            && (visit.stage != Stage::Working
                                || !matches!(plan.purpose, Purpose::Trade { .. }))
                    }
                    _ => false,
                };
                cargo_valid
                    && meeting_valid
                    && visit.cell.inside()
                    && visit.admitted <= visit.due
                    && (matches!(visit.stage, Stage::Arriving | Stage::Departing)
                        || visit.cell == plan.site)
                    && visit.edge.is_none_or(|edge| {
                        matches!(visit.stage, Stage::Arriving | Stage::Departing)
                            && edge.to.inside()
                            && visit.cell.adjacent(edge.to)
                            && edge.started >= visit.admitted
                            && valid_travel_edge(edge.started, visit.due)
                    })
                    && self.slots[..index].iter().flatten().all(|other| {
                        other.profession != visit.profession
                            && !other.occupied(visit.cell)
                            && visit.edge.is_none_or(|edge| !other.occupied(edge.to))
                    })
            })
    }

    pub fn validate_for_world(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        self.validate()
            && self
                .last_window
                .is_none_or(|window| window <= window_id(now))
            && self
                .last_departed
                .iter()
                .all(|last| last.is_none_or(|last| last <= now))
            && self
                .last_receipt
                .is_none_or(|receipt| receipt.world_ms <= now)
            && self.next_admission <= now.saturating_add(24 * HOUR)
            && self.slots.iter().flatten().all(|visit| {
                visit.admitted <= now
                    && visit.due <= now.saturating_add(MAX_STAY)
                    && visit.cell.walkable(world)
                    && visit.edge.is_none_or(|edge| edge.to.walkable(world))
                    && visit
                        .partner
                        .is_none_or(|partner| world.activity_deadline(partner).is_some())
                    && visit.rendezvous.is_none_or(|site| site.walkable(world))
            })
    }

    pub fn present(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    pub fn meeting_partner(&self) -> Option<ActorKind> {
        self.slots.iter().flatten().find_map(|visit| visit.partner)
    }

    pub fn clear_of_tiles(&self, tiles: &[[i32; 2]]) -> bool {
        self.slots.iter().flatten().all(|visit| {
            tiles.iter().all(|&[x, y]| {
                let cell = Cell(x as u8, y as u8);
                !visit.occupied(cell) && visit.rendezvous != Some(cell)
            })
        })
    }

    pub fn desired(&self) -> u8 {
        self.desired
    }

    pub fn visits(&self) -> u64 {
        self.visits
    }

    pub fn trades(&self) -> u64 {
        self.trades
    }

    pub fn reports(&self) -> u64 {
        self.reports
    }

    pub fn last_receipt(&self) -> Option<VisitReceipt> {
        self.last_receipt
    }

    pub(crate) fn migration_cargo(&self) -> impl Iterator<Item = (Profession, Item, u16)> + '_ {
        self.slots
            .iter()
            .flatten()
            .filter_map(|visit| match visit.profession.plan().purpose {
                Purpose::Trade { goods, .. } if visit.cargo != 0 => {
                    Some((visit.profession, goods.0, visit.cargo))
                }
                _ => None,
            })
    }

    pub fn visuals(&self, world: &LivingWorld) -> Vec<VisitorVisual> {
        self.slots
            .iter()
            .flatten()
            .map(|visit| visit.visual(world.world_ms()))
            .collect()
    }

    pub fn next_update_ms(&self, world: &LivingWorld) -> Option<u64> {
        let now = world.world_ms();
        let window = window_id(now);
        let mut due = if self.last_window == Some(window) {
            window_end(window)
        } else {
            now
        };
        if self.desired > self.present() as u8 && human_admission_hours(now) {
            due = due.min(self.next_admission);
        }
        Some(
            self.slots
                .iter()
                .flatten()
                .map(|visit| visit.due)
                .min()
                .unwrap_or(u64::MAX)
                .min(due)
                .max(now),
        )
    }

    /// Call after advancing the household to the next deadline.
    pub fn advance(&mut self, world: &mut LivingWorld) {
        let now = world.world_ms();
        let window = window_id(now);
        if self.last_window != Some(window) {
            self.last_window = Some(window);
            self.desired = desired_count(self.seed, window, now);
        }
        for index in 0..self.slots.len() {
            let Some(mut visit) = self.slots[index].take() else {
                continue;
            };
            let yielding = matches!(
                visit.stage,
                Stage::Waiting | Stage::Meeting | Stage::Working
            ) && household_occupies(world, visit.cell, visit.partner)
                && self.yield_to_household(world, visit.cell).is_some();
            if (visit.due > now && !yielding) || self.advance_visit(world, &mut visit) {
                self.slots[index] = Some(visit);
            }
        }
        if now >= self.next_admission
            && self.present() < usize::from(self.desired)
            && human_admission_hours(now)
        {
            self.admit(world, window);
        }
    }

    fn admit(&mut self, world: &LivingWorld, window: u64) {
        let now = world.world_ms();
        let jitter = draw(self.seed, now / MINUTE, 1) % 6;
        // At most two admissions in each half-hour even at the minimum gap.
        self.next_admission = now.saturating_add((15 + jitter) * MINUTE);
        if household_occupies(world, ENTRY, None)
            || self
                .slots
                .iter()
                .flatten()
                .any(|visit| visit.occupied(ENTRY))
        {
            return;
        }
        let start = (draw(self.seed, window ^ now, 2) % PROFESSIONS.len() as u64) as usize;
        let weather = world.weather();
        for offset in 0..PROFESSIONS.len() {
            let profession = PROFESSIONS[(start + offset) % PROFESSIONS.len()];
            if self.last_departed[profession as usize]
                .is_some_and(|left| now.saturating_sub(left) < DAY)
                || self
                    .slots
                    .iter()
                    .flatten()
                    .any(|visit| visit.profession == profession)
            {
                continue;
            }
            let plan = profession.plan();
            let cargo = match plan.purpose {
                Purpose::Trade { costs, goods } => {
                    if world.stock().count(costs.0) < costs.1 {
                        continue;
                    }
                    goods.1
                }
                Purpose::Inspect(_) if matches!(weather, YardWeather::Rain | YardWeather::Snow) => {
                    continue;
                }
                Purpose::Inspect(_) => 0,
            };
            let Some(first) = self.next_step(world, ENTRY, plan.site) else {
                continue;
            };
            let mut visit = Visit {
                profession,
                stage: Stage::Arriving,
                cell: ENTRY,
                edge: None,
                admitted: now,
                due: now,
                cargo,
                facing_left: false,
                partner: None,
                rendezvous: None,
            };
            self.walk_edge(world, &mut visit, first, now);
            let slot = self
                .slots
                .iter_mut()
                .find(|slot| slot.is_none())
                .expect("admission fits");
            *slot = Some(visit);
            self.visits = self.visits.saturating_add(1);
            break;
        }
    }

    fn advance_visit(&mut self, world: &mut LivingWorld, visit: &mut Visit) -> bool {
        let now = world.world_ms();
        if let Some(edge) = visit.edge.take() {
            visit.cell = edge.to;
        }
        let plan = visit.profession.plan();
        if visit.stage != Stage::Departing && now.saturating_sub(visit.admitted) >= MAX_STAY {
            release_partner(world, visit);
            self.complete(visit, now, VisitOutcome::LeftWithoutExchange);
        }
        if visit.stage == Stage::Departing && visit.cell == ENTRY {
            self.last_departed[visit.profession as usize] = Some(now);
            return false;
        }
        if visit.stage == Stage::Arriving && visit.cell == plan.site {
            visit.stage = Stage::Waiting;
        }
        if matches!(
            visit.stage,
            Stage::Waiting | Stage::Meeting | Stage::Working
        ) && household_occupies(world, visit.cell, visit.partner)
            && let Some(next) = self.yield_to_household(world, visit.cell)
        {
            release_partner(world, visit);
            visit.stage = Stage::Arriving;
            self.walk_edge(world, visit, next, now);
            return true;
        }
        match visit.stage {
            Stage::Arriving => self.walk_to(world, visit, plan.site),
            Stage::Departing => self.walk_to(world, visit, ENTRY),
            Stage::Waiting => {
                if let Purpose::Inspect(_) = plan.purpose
                    && matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
                {
                    self.complete(visit, now, VisitOutcome::LeftWithoutExchange);
                    self.walk_to(world, visit, ENTRY);
                } else if matches!(plan.purpose, Purpose::Trade { .. }) {
                    if self.start_meeting(world, visit, &plan) {
                        visit.stage = Stage::Meeting;
                        visit.due = now.saturating_add(WALK_TIME);
                    } else {
                        visit.due = now.saturating_add(5 * MINUTE);
                    }
                } else {
                    visit.stage = Stage::Working;
                    visit.due = now.saturating_add(plan.duration);
                }
            }
            Stage::Meeting => {
                let partner = visit.partner.expect("meeting has partner");
                if urgent(world, partner) {
                    release_partner(world, visit);
                    visit.stage = Stage::Waiting;
                    visit.due = now.saturating_add(5 * MINUTE);
                } else if partner_at_site(world, visit) {
                    visit.stage = Stage::Working;
                    visit.due = now.saturating_add(plan.duration);
                } else {
                    visit.due = world
                        .activity_deadline(partner)
                        .unwrap_or(now)
                        .max(now.saturating_add(WALK_TIME));
                }
            }
            Stage::Working => {
                match plan.purpose {
                    Purpose::Trade { costs, goods }
                        if partner_at_site(world, visit)
                            && visit.partner.is_some_and(|partner| !urgent(world, partner))
                            && visit.cargo == goods.1
                            && world.exchange_stock_atomic(&[costs], &[goods]) =>
                    {
                        visit.cargo = 0;
                        release_partner(world, visit);
                        self.trades = self.trades.saturating_add(1);
                        self.complete(
                            visit,
                            now,
                            VisitOutcome::Trade {
                                paid: costs.0,
                                paid_units: costs.1,
                                received: goods.0,
                                received_units: goods.1,
                            },
                        );
                    }
                    Purpose::Inspect(kind) if dry(world) => {
                        self.reports = self.reports.saturating_add(1);
                        self.complete(
                            visit,
                            now,
                            VisitOutcome::Report {
                                kind,
                                observed: observe(world, kind),
                            },
                        );
                    }
                    _ => {
                        release_partner(world, visit);
                        visit.stage = Stage::Waiting;
                        visit.due = now.saturating_add(5 * MINUTE);
                        return true;
                    }
                }
                self.walk_to(world, visit, ENTRY);
            }
        }
        true
    }

    fn start_meeting(&self, world: &mut LivingWorld, visit: &mut Visit, plan: &Plan) -> bool {
        let Purpose::Trade { costs, .. } = plan.purpose else {
            return false;
        };
        if world.stock().count(costs.0) < costs.1
            || self
                .slots
                .iter()
                .flatten()
                .any(|other| other.partner.is_some())
        {
            return false;
        }
        for partner in [ActorKind::Xiaohe, ActorKind::Ayu] {
            if !world.activity_available(partner) || urgent(world, partner) {
                continue;
            }
            for site in visit.cell.handoff_sites() {
                if !site.walkable(world)
                    || household_occupies(world, site, Some(partner))
                    || !clear_handoff(world, visit.cell, site)
                    || self
                        .slots
                        .iter()
                        .flatten()
                        .any(|other| other.occupied(site))
                    || !world.can_reach_for_activity(partner, site.point())
                    || !world.reserve_activity(&[partner])
                {
                    continue;
                }
                if near_site(world, partner, site) || world.move_for_activity(partner, site.point())
                {
                    visit.partner = Some(partner);
                    visit.rendezvous = Some(site);
                    return true;
                }
                world.recover_activity(&[partner]);
            }
        }
        false
    }

    fn complete(&mut self, visit: &mut Visit, now: u64, result: VisitOutcome) {
        self.last_receipt = Some(VisitReceipt {
            profession: visit.profession,
            world_ms: now,
            result,
        });
        visit.stage = Stage::Departing;
    }

    fn walk_to(&self, world: &LivingWorld, visit: &mut Visit, destination: Cell) {
        let now = world.world_ms();
        if let Some(next) = self.next_step(world, visit.cell, destination) {
            self.walk_edge(world, visit, next, now);
        } else {
            visit.due = now.saturating_add(POLL_TIME);
        }
    }

    fn walk_edge(&self, world: &LivingWorld, visit: &mut Visit, next: Cell, now: u64) {
        if next.0 != visit.cell.0 {
            visit.facing_left = next.0 < visit.cell.0;
        }
        visit.edge = Some(Edge {
            to: next,
            started: now,
        });
        visit.due = now.saturating_add(edge_duration(
            now,
            Pace::Human,
            WalkConditions {
                loaded: visit.cargo != 0,
                tired: false,
                rough_weather: matches!(world.weather(), YardWeather::Rain | YardWeather::Snow),
            },
        ));
    }

    fn yield_to_household(&self, world: &LivingWorld, cell: Cell) -> Option<Cell> {
        cell.neighbors().find(|&neighbor| {
            neighbor.walkable(world)
                && !household_occupies(world, neighbor, None)
                && !self
                    .slots
                    .iter()
                    .flatten()
                    .any(|other| other.occupied(neighbor) || other.rendezvous == Some(neighbor))
        })
    }

    fn next_step(&self, world: &LivingWorld, start: Cell, destination: Cell) -> Option<Cell> {
        if start == destination || !destination.walkable(world) {
            return None;
        }
        let mut used = [false; CELLS];
        for visit in self.slots.iter().flatten() {
            used[visit.cell.index()] = true;
            if let Some(edge) = visit.edge {
                used[edge.to.index()] = true;
            }
            if let Some(site) = visit.rendezvous {
                used[site.index()] = true;
            }
        }
        for cell in household_cells(world, None) {
            used[cell.index()] = true;
        }
        if used[destination.index()] {
            return None;
        }
        used[start.index()] = true;
        let mut queue = VecDeque::new();
        for cell in start.neighbors() {
            if !used[cell.index()] && cell.walkable(world) {
                used[cell.index()] = true;
                queue.push_back((cell, cell));
            }
        }
        while let Some((cell, first)) = queue.pop_front() {
            if cell == destination {
                return Some(first);
            }
            for neighbor in cell.neighbors() {
                if !used[neighbor.index()] && neighbor.walkable(world) {
                    used[neighbor.index()] = true;
                    queue.push_back((neighbor, first));
                }
            }
        }
        None
    }
}

fn dry(world: &LivingWorld) -> bool {
    !matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
}

fn household_cells(world: &LivingWorld, except: Option<ActorKind>) -> impl Iterator<Item = Cell> {
    world
        .actors()
        .iter()
        .filter(move |actor| !actor.inside() && Some(actor.kind) != except)
        .flat_map(|actor| {
            let (from, to) = actor.walking_segment().map_or((None, None), |(from, to)| {
                (Cell::from_point(from), Cell::from_point(to))
            });
            [Cell::from_point(actor.position), from, to]
                .into_iter()
                .flatten()
        })
}

fn household_occupies(world: &LivingWorld, cell: Cell, except: Option<ActorKind>) -> bool {
    household_cells(world, except).any(|occupied| occupied == cell)
}

fn clear_handoff(world: &LivingWorld, visitor: Cell, partner: Cell) -> bool {
    visitor.adjacent(partner)
        || Cell((visitor.0 + partner.0) / 2, (visitor.1 + partner.1) / 2).walkable(world)
}

fn urgent(world: &LivingWorld, partner: ActorKind) -> bool {
    let actor = &world.actors()[partner as usize];
    actor.hunger() >= 68 || actor.thirst() >= 68 || actor.stamina() < 36
}

fn near_site(world: &LivingWorld, partner: ActorKind, site: Cell) -> bool {
    let actor = &world.actors()[partner as usize];
    let point = site.point();
    (actor.position.x - point.x).abs() < 2.0 && (actor.position.y - point.y).abs() < 2.0
}

fn partner_at_site(world: &LivingWorld, visit: &Visit) -> bool {
    visit
        .partner
        .zip(visit.rendezvous)
        .is_some_and(|(partner, site)| {
            world.activity_arrived(partner) && near_site(world, partner, site)
        })
}

fn release_partner(world: &mut LivingWorld, visit: &mut Visit) {
    if let Some(partner) = visit.partner.take() {
        world.recover_activity(&[partner]);
    }
    visit.rendezvous = None;
}

fn observe(world: &LivingWorld, kind: ReportKind) -> u16 {
    match kind {
        ReportKind::DeliverySite => (28..=31).filter(|&x| world.can_walk(x, 15)).count() as u16,
        ReportKind::PlantCover => world
            .plots()
            .iter()
            .filter(|plot| {
                matches!(
                    plot.stage,
                    PlotStage::Seed | PlotStage::Growing | PlotStage::Mature
                )
            })
            .count() as u16,
        ReportKind::PondStudy => (9..14)
            .flat_map(|x| (18..22).map(move |y| (x, y)))
            .filter(|&(x, y)| world.ground(x, y) == Ground::Water)
            .count() as u16,
        ReportKind::RoadSurvey => (7..=45)
            .filter(|&x| world.ground(x, 14) == Ground::Path && world.can_walk(x, 14))
            .count() as u16,
    }
}

fn window_id(now: u64) -> u64 {
    (now.saturating_add(HOUR)) / WINDOW
}

fn window_end(window: u64) -> u64 {
    window
        .saturating_add(1)
        .saturating_mul(WINDOW)
        .saturating_sub(HOUR)
}

fn human_admission_hours(now: u64) -> bool {
    let hour = (now % DAY) / HOUR;
    (5..20).contains(&hour)
}

fn desired_count(seed: u64, window: u64, now: u64) -> u8 {
    let hour = (now % DAY) / HOUR;
    let distribution: &[u64] = match hour {
        5..=7 => &QUIET,
        8..=19 => &STANDARD,
        _ => &NIGHT,
    };
    let sample = draw(seed, window, 0) >> 32;
    distribution.partition_point(|&cumulative| cumulative <= sample) as u8
}

fn draw(seed: u64, key: u64, stream: u64) -> u64 {
    let mixed =
        seed ^ key.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ stream.wrapping_mul(0x6a09_e667_f3bc_c909);
    let mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(world: &mut LivingWorld, visitors: &mut VisitorState, end: u64, slice: u64) {
        visitors.advance(world);
        while world.world_ms() < end {
            let now = world.world_ms();
            let next = world
                .next_update_ms()
                .min(visitors.next_update_ms(world).unwrap_or(end))
                .min(end)
                .min(now.saturating_add(slice))
                .max(now + 1);
            world.advance_world_ms(next - now);
            visitors.advance(world);
        }
    }

    #[test]
    fn normal_targets_match_the_documented_cdf() {
        let mut bins = [0usize; STANDARD.len()];
        for i in 0..100_000 {
            let sample = desired_count(VisitorState::default().seed, i, 11 * HOUR);
            bins[sample as usize] += 1;
        }
        let mut previous = 0;
        for (index, &weight) in STANDARD.iter().enumerate() {
            let expected = (weight - previous) as f64 * 100_000.0 / (1u64 << 32) as f64;
            assert!((bins[index] as f64 - expected).abs() < 500.0, "{bins:?}");
            previous = weight;
        }
        assert_eq!(window_end(window_id(11 * HOUR)), 14 * HOUR);
        assert_eq!(window_end(window_id(23 * HOUR)), 26 * HOUR);
    }

    #[test]
    fn a_clear_handoff_uses_two_tiles_and_avoids_household_positions() {
        let mut world = LivingWorld::new_wall_ms(0);
        let visitors = VisitorState::default();
        let profession = Profession::SeedMerchant;
        let plan = profession.plan();
        let mut visit = Visit {
            profession,
            stage: Stage::Waiting,
            cell: plan.site,
            edge: None,
            admitted: world.world_ms(),
            due: world.world_ms(),
            cargo: 3,
            facing_left: false,
            partner: None,
            rendezvous: None,
        };
        assert!(visitors.start_meeting(&mut world, &mut visit, &plan));
        let site = visit.rendezvous.unwrap();
        assert!(!visit.cell.adjacent(site));
        assert!(visit.cell.trading_distance(site));
        assert!(!household_occupies(&world, site, visit.partner));
        assert!(world.activity_deadline(visit.partner.unwrap()).is_some());
    }

    #[test]
    fn ten_professions_finish_visits_with_legal_distinct_positions() {
        let mut world = LivingWorld::new_wall_ms(0);
        let mut visitors = VisitorState::default();
        let mut observed = [false; PROFESSIONS.len()];
        let mut peak = 0;
        let mut last_receipt = None;
        let mut reports = 0;
        let mut trades = 0;
        let mut saw_cargo = false;
        while world.world_ms() < 24 * DAY {
            let now = world.world_ms();
            let next = world
                .next_update_ms()
                .min(visitors.next_update_ms(&world).unwrap())
                .max(now + 1);
            world.advance_world_ms(next - now);
            let stock = world.stock().clone();
            visitors.advance(&mut world);
            assert!(visitors.validate_for_world(&world));
            peak = peak.max(visitors.present());
            for visual in visitors.visuals(&world) {
                observed[visual.profession as usize] = true;
                saw_cargo |= visual.pose == VisitorPose::Carrying;
                assert!(visual.animation_frame < 3);
            }
            if let Some(receipt) = visitors.last_receipt()
                && Some(receipt) != last_receipt
            {
                match receipt.result {
                    VisitOutcome::Trade {
                        paid,
                        paid_units,
                        received,
                        received_units,
                    } if visitors.trades() == trades + 1
                        && receipt.world_ms == world.world_ms() =>
                    {
                        assert_eq!(stock.count(paid), world.stock().count(paid) + paid_units);
                        assert_eq!(
                            stock.count(received) + received_units,
                            world.stock().count(received)
                        );
                    }
                    VisitOutcome::Report { .. } => reports += 1,
                    _ => {}
                }
                last_receipt = Some(receipt);
            }
            trades = visitors.trades();
            if observed.iter().all(|value| *value) && reports >= 4 && trades >= 6 {
                break;
            }
        }
        assert!(saw_cargo);
        assert!(observed.iter().all(|value| *value), "seen {observed:?}");
        assert!(peak >= 2 && peak <= MAX_HUMAN_VISITORS, "peak {peak}");
        assert!(
            trades >= 6 && reports >= 4,
            "{trades} trades, {reports} reports"
        );
    }

    #[test]
    fn checkpoint_and_sliced_visits_match_and_never_replay_a_trade() {
        let mut world = LivingWorld::new_wall_ms(0);
        let mut visitors = VisitorState::default();
        while world.world_ms() < 4 * DAY
            && !visitors
                .slots
                .iter()
                .flatten()
                .any(|visit| visit.stage == Stage::Meeting)
        {
            let next = world
                .next_update_ms()
                .min(visitors.next_update_ms(&world).unwrap())
                .max(world.world_ms() + 1);
            tick(&mut world, &mut visitors, next, u64::MAX);
        }
        assert!(
            visitors
                .slots
                .iter()
                .flatten()
                .any(|visit| visit.stage == Stage::Meeting)
        );
        let mut restored_world: LivingWorld =
            serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        let mut restored_visitors: VisitorState =
            serde_json::from_slice(&serde_json::to_vec(&visitors).unwrap()).unwrap();
        assert!(restored_visitors.validate_for_world(&restored_world));
        tick(&mut world, &mut visitors, 5 * DAY, u64::MAX);
        tick(&mut restored_world, &mut restored_visitors, 5 * DAY, 20_000);
        assert_eq!(visitors, restored_visitors);
        assert_eq!(world, restored_world);
    }

    #[test]
    fn arrivals_respect_the_rhythm_and_reports_do_not_spawn_stock() {
        let mut world = LivingWorld::new_wall_ms(0);
        let mut visitors = VisitorState::default();
        let mut last_arrival = None;
        let mut last_visits = 0;
        let mut last_reports = 0;
        while world.world_ms() < 3 * DAY {
            let next = world
                .next_update_ms()
                .min(visitors.next_update_ms(&world).unwrap())
                .max(world.world_ms() + 1);
            world.advance_world_ms(next - world.world_ms());
            let stock = world.stock().clone();
            let trades = visitors.trades();
            visitors.advance(&mut world);
            if visitors.visits() != last_visits {
                assert!(human_admission_hours(world.world_ms()));
                assert!(
                    last_arrival.is_none_or(|arrival| world.world_ms() - arrival >= 15 * MINUTE)
                );
                last_arrival = Some(world.world_ms());
                last_visits = visitors.visits();
            }
            if visitors.reports() != last_reports && trades == visitors.trades() {
                for item in [
                    Item::Seeds,
                    Item::Water,
                    Item::Crops,
                    Item::Food,
                    Item::Wood,
                    Item::Stone,
                    Item::Rope,
                    Item::Fish,
                ] {
                    assert_eq!(world.stock().count(item), stock.count(item));
                }
                last_reports = visitors.reports();
            }
        }
        assert!(last_visits > 5 && last_reports >= 2);
    }
}
