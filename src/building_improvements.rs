//! Finite functional fittings in existing household and village buildings.

use serde::{Deserialize, Serialize};

use super::{DAY, DevelopmentKind, Material};

pub const IMPROVEMENT_SITES: usize = 10;
pub const MAX_STRUCTURE_LEVEL: u8 = 4;
const REST_DAYS: u64 = 7;

#[derive(Clone, Copy)]
pub(crate) struct StructuralWork {
    pub material: Material,
    pub units: u16,
    pub minutes: u16,
}

const fn structural(material: Material, units: u16, minutes: u16) -> StructuralWork {
    StructuralWork {
        material,
        units,
        minutes,
    }
}

pub(crate) fn structure_work(site: usize, level: u8, step: u8) -> StructuralWork {
    use Material::{Rope, Stone, Wood};
    let phases = match (level, site) {
        (2, 0) => [
            structural(Stone, 2, 26),
            structural(Wood, 3, 30),
            structural(Rope, 1, 24),
        ],
        (2, 1) => [
            structural(Stone, 2, 26),
            structural(Wood, 3, 32),
            structural(Wood, 1, 24),
        ],
        (2, 2) => [
            structural(Stone, 3, 30),
            structural(Wood, 2, 30),
            structural(Rope, 1, 22),
        ],
        (2, 3) => [
            structural(Stone, 2, 26),
            structural(Wood, 3, 28),
            structural(Rope, 1, 22),
        ],
        (2, _) => [
            structural(Stone, 3, 28),
            structural(Wood, 3, 32),
            structural(Rope, 1, 22),
        ],
        (3, 0) => [
            structural(Stone, 3, 34),
            structural(Wood, 4, 36),
            structural(Rope, 2, 26),
        ],
        (3, 1) => [
            structural(Stone, 3, 34),
            structural(Wood, 5, 38),
            structural(Rope, 1, 24),
        ],
        (3, 2) => [
            structural(Stone, 4, 36),
            structural(Wood, 3, 34),
            structural(Rope, 2, 26),
        ],
        (3, 3) => [
            structural(Stone, 3, 30),
            structural(Wood, 5, 36),
            structural(Rope, 2, 26),
        ],
        (3, _) => [
            structural(Stone, 4, 36),
            structural(Wood, 5, 38),
            structural(Rope, 1, 26),
        ],
        _ => [
            structural(Stone, 2, 24),
            structural(Wood, 3, 24),
            structural(Wood, 1, 24),
        ],
    };
    phases[usize::from(step)]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildingUse {
    Residence,
    Cooking,
    Storage,
    Workshop,
    Trade,
    Rest,
    Water,
    Garden,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fixture {
    WestWeatherstrip,
    WestSeedDrawer,
    EastWeatherstrip,
    EastToolRack,
    KitchenBatchStove,
    KitchenQuickStove,
    PantryDryRack,
    PantryCoolShelf,
    WorkshopJig,
    WorkshopToolBoard,
    StallStockBins,
    GuesthouseBench,
    DepotVentRack,
    WellRope,
    GardenPlanter,
}

#[derive(Clone, Copy)]
struct Spec {
    site: u8,
    label: &'static str,
    asset: &'static str,
    life: u16,
    phases: [(Material, u16, u16); 2],
}

const fn spec(
    site: u8,
    label: &'static str,
    asset: &'static str,
    life: u16,
    first: (Material, u16, u16),
    second: (Material, u16, u16),
) -> Spec {
    Spec {
        site,
        label,
        asset,
        life,
        phases: [first, second],
    }
}

impl Fixture {
    fn spec(self) -> Spec {
        use Fixture::*;
        use Material::{Rope, Stone, Wood};
        match self {
            WestWeatherstrip => spec(
                0,
                "西屋挡风门帘",
                "fixture_west_weatherstrip",
                180,
                (Wood, 1, 22),
                (Rope, 1, 16),
            ),
            WestSeedDrawer => spec(
                0,
                "西屋分格种匣",
                "fixture_west_seed_drawer",
                80,
                (Wood, 1, 20),
                (Stone, 1, 14),
            ),
            EastWeatherstrip => spec(
                1,
                "东屋挡风门帘",
                "fixture_east_weatherstrip",
                180,
                (Wood, 1, 22),
                (Rope, 1, 16),
            ),
            EastToolRack => spec(
                1,
                "东屋工具架",
                "fixture_east_tool_rack",
                90,
                (Wood, 1, 18),
                (Rope, 1, 15),
            ),
            KitchenBatchStove => spec(
                2,
                "厨房双锅灶",
                "fixture_kitchen_batch_stove",
                48,
                (Stone, 2, 26),
                (Wood, 1, 20),
            ),
            KitchenQuickStove => spec(
                2,
                "厨房快火灶",
                "fixture_kitchen_quick_stove",
                60,
                (Stone, 1, 20),
                (Wood, 1, 18),
            ),
            PantryDryRack => spec(
                3,
                "储物棚晒粮架",
                "fixture_pantry_dry_rack",
                100,
                (Wood, 1, 20),
                (Rope, 1, 18),
            ),
            PantryCoolShelf => spec(
                3,
                "储物棚凉藏格",
                "fixture_pantry_cool_shelf",
                70,
                (Stone, 2, 24),
                (Wood, 1, 20),
            ),
            WorkshopJig => spec(
                4,
                "工坊施工夹具",
                "fixture_workshop_jig",
                72,
                (Wood, 2, 22),
                (Stone, 1, 18),
            ),
            WorkshopToolBoard => spec(
                4,
                "工坊工具板",
                "fixture_workshop_tool_board",
                100,
                (Wood, 1, 20),
                (Rope, 1, 18),
            ),
            StallStockBins => spec(
                5,
                "摊棚分类货箱",
                "fixture_stall_stock_bins",
                32,
                (Wood, 1, 22),
                (Rope, 1, 16),
            ),
            GuesthouseBench => spec(
                6,
                "客舍歇脚长凳",
                "fixture_guesthouse_bench",
                48,
                (Wood, 2, 22),
                (Stone, 1, 16),
            ),
            DepotVentRack => spec(
                7,
                "南仓通风储架",
                "fixture_depot_vent_rack",
                40,
                (Wood, 2, 24),
                (Rope, 1, 18),
            ),
            WellRope => spec(
                8,
                "公井提水滑轮",
                "fixture_well_rope",
                75,
                (Wood, 1, 22),
                (Rope, 2, 20),
            ),
            GardenPlanter => spec(
                9,
                "公共花园养护槽",
                "fixture_garden_planter",
                24,
                (Stone, 1, 24),
                (Wood, 1, 18),
            ),
        }
    }

    pub fn site(self) -> u8 {
        self.spec().site
    }

    pub fn label(self) -> &'static str {
        self.spec().label
    }

    pub fn asset_name(self) -> &'static str {
        self.spec().asset
    }

    pub fn uses(self) -> u16 {
        self.spec().life
    }

    pub fn building_use(self) -> BuildingUse {
        use BuildingUse::*;
        [
            Residence, Residence, Cooking, Storage, Workshop, Trade, Rest, Storage, Water, Garden,
        ][usize::from(self.site())]
    }

    pub(crate) fn requires(self) -> Option<DevelopmentKind> {
        match self.site() {
            5 => Some(DevelopmentKind::Stall),
            6 => Some(DevelopmentKind::Guesthouse),
            7 => Some(DevelopmentKind::Depot),
            8 => Some(DevelopmentKind::Well),
            9 => Some(DevelopmentKind::Garden),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImprovementOrder {
    pub fixture: Fixture,
    pub phase: u8,
}

impl ImprovementOrder {
    pub(crate) fn material(self) -> (Material, u16) {
        let (material, units, _) = self.fixture.spec().phases[usize::from(self.phase)];
        (material, units)
    }

    pub(crate) fn minutes(self) -> u16 {
        self.fixture.spec().phases[usize::from(self.phase)].2
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureStatus {
    pub fixture: Fixture,
    pub uses_left: u16,
    pub installed_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    fixture: Fixture,
    started: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImprovementState {
    slots: [Option<FixtureStatus>; IMPROVEMENT_SITES],
    active: Option<Project>,
    cursor: u8,
    last_finish: u64,
    completed: u64,
    repairs: u64,
    conversions: u64,
    #[serde(default)]
    structure_due: [u64; 5],
}

impl ImprovementState {
    pub fn installed(&self) -> impl Iterator<Item = FixtureStatus> + '_ {
        self.slots.iter().filter_map(|&status| status)
    }

    pub fn active(&self) -> Option<ImprovementOrder> {
        self.active.map(|project| ImprovementOrder {
            fixture: project.fixture,
            phase: 1,
        })
    }

    pub fn completed(&self) -> u64 {
        self.completed
    }

    pub fn repairs(&self) -> u64 {
        self.repairs
    }

    pub fn conversions(&self) -> u64 {
        self.conversions
    }

    pub(crate) fn structure_due(&self, site: usize) -> u64 {
        self.structure_due[site]
    }

    pub(crate) fn structure_completed_stage(&mut self, site: usize, next: u64) {
        self.structure_due[site] = next;
    }

    pub(crate) fn working(&self, fixture: Fixture) -> bool {
        self.slots[usize::from(fixture.site())]
            .is_some_and(|status| status.fixture == fixture && status.uses_left > 0)
    }

    pub(crate) fn used(&mut self, fixture: Fixture) {
        if let Some(status) = &mut self.slots[usize::from(fixture.site())]
            && status.fixture == fixture
        {
            status.uses_left = status.uses_left.saturating_sub(1);
        }
    }

    pub(crate) fn restore_use(&mut self, fixture: Fixture) -> bool {
        let Some(status)=self.slots[usize::from(fixture.site())].as_mut() else {return false;};
        if status.fixture!=fixture || status.uses_left>=fixture.uses()
            || self.active.is_some_and(|order|order.fixture.site()==fixture.site()) {
            return false;
        }
        status.uses_left=(status.uses_left+3).min(fixture.uses());
        self.repairs=self.repairs.saturating_add(1);
        self.completed=self.completed.saturating_add(1);
        true
    }

    pub(crate) fn orders(
        &self,
        now: u64,
        season: usize,
    ) -> impl Iterator<Item = ImprovementOrder> + '_ {
        let phase_one = self
            .active
            .filter(|project| now.saturating_sub(project.started) >= DAY)
            .map(|project| ImprovementOrder {
                fixture: project.fixture,
                phase: 1,
            });
        let alternatives = preferred_fixtures(season);
        let new = self.active.is_none() && now.saturating_sub(self.last_finish) >= REST_DAYS * DAY;
        phase_one
            .into_iter()
            .chain((0..IMPROVEMENT_SITES).filter_map(move |offset| {
                if !new {
                    return None;
                }
                let index = (usize::from(self.cursor) + offset) % IMPROVEMENT_SITES;
                let fixture = alternatives[index];
                let warranted = self.slots[index].is_none_or(|status| {
                    let age = now.saturating_sub(status.installed_at);
                    (status.fixture != fixture && age >= 28 * DAY)
                        || (status.uses_left <= status.fixture.uses() / 6 && age >= 14 * DAY)
                });
                warranted.then_some(ImprovementOrder { fixture, phase: 0 })
            }))
    }

    pub(crate) fn accepts(&self, order: ImprovementOrder, now: u64) -> bool {
        match (order.phase, self.active) {
            (0, None) => now.saturating_sub(self.last_finish) >= REST_DAYS * DAY,
            (1, Some(project)) => {
                project.fixture == order.fixture && now.saturating_sub(project.started) >= DAY
            }
            _ => false,
        }
    }

    pub(crate) fn commit(&mut self, order: ImprovementOrder, now: u64) {
        debug_assert!(self.accepts(order, now));
        if order.phase == 0 {
            self.active = Some(Project {
                fixture: order.fixture,
                started: now,
            });
            return;
        }
        let index = usize::from(order.fixture.site());
        if let Some(previous) = self.slots[index] {
            if previous.fixture == order.fixture {
                self.repairs = self.repairs.saturating_add(1);
            } else {
                self.conversions = self.conversions.saturating_add(1);
            }
        }
        self.slots[index] = Some(FixtureStatus {
            fixture: order.fixture,
            uses_left: order.fixture.uses(),
            installed_at: now,
        });
        self.active = None;
        self.completed = self.completed.saturating_add(1);
        self.last_finish = now;
        self.cursor = ((index + 1) % IMPROVEMENT_SITES) as u8;
    }

    pub(crate) fn validate(&self, now: u64) -> bool {
        usize::from(self.cursor) < IMPROVEMENT_SITES
            && self.last_finish <= now
            && self.repairs.saturating_add(self.conversions) <= self.completed
            && self
                .structure_due
                .iter()
                .all(|&due| due <= now.saturating_add(28 * DAY))
            && self
                .active
                .is_none_or(|project| project.started >= self.last_finish && project.started <= now)
            && self.slots.iter().enumerate().all(|(index, status)| {
                status.is_none_or(|status| {
                    usize::from(status.fixture.site()) == index
                        && status.uses_left <= status.fixture.uses()
                        && status.installed_at <= self.last_finish
                })
            })
    }
}

fn preferred_fixtures(season: usize) -> [Fixture; IMPROVEMENT_SITES] {
    use Fixture::*;
    let warm = matches!(season, 0 | 1);
    [
        if warm {
            WestSeedDrawer
        } else {
            WestWeatherstrip
        },
        if warm { EastToolRack } else { EastWeatherstrip },
        if warm {
            KitchenBatchStove
        } else {
            KitchenQuickStove
        },
        if warm { PantryCoolShelf } else { PantryDryRack },
        if warm { WorkshopJig } else { WorkshopToolBoard },
        StallStockBins,
        GuesthouseBench,
        DepotVentRack,
        WellRope,
        GardenPlanter,
    ]
}
