//! Persistent appointments for named villagers. Stock stays in custody
//! until the appropriate participant reaches the delivery site.
use super::*;
use crate::living_world::{PlotStage, YardSeason};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SocialProgress {
    pub market_after_review: bool,
    pub market_cleanup: bool,
    pub fixed_market: bool,
    pub matured_village: bool,
    pub harvest_gathering: bool,
    pub winter_workshop: bool,
    pub familiar_visitor: bool,
    pub last_market_cleanup_day: Option<u64>,
}

const CHAPTERS: [u8; 23] = [
    36, 37, 38, 39, 40, 41, 47, 48, 49, 50, 51, 52, 53, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69,
];
const CHAPTER_MASK: u32 = (1 << CHAPTERS.len()) - 1;
pub(super) const HELPER: usize = 7;
const CARPENTER: usize = 2;
const FARMER: usize = 10;
const TAILOR: usize = 11;
const WEAVER: usize = 5;
const COOK: usize = 3;
const POST: usize = 0;
const SEEDS: usize = 1;
const IRRIGATOR: usize = 16;
const APPOINTMENT_LENGTH: u64 = 3 * HOUR;
const MAX_APPOINTMENT_LENGTH: u64 = 6 * HOUR;
const WHEAT_PLOT: usize = 8;
const TRIAL_SHELTER: &str = "stall_seed";

fn trial_shelter(world: &LivingWorld) -> Option<&'static crate::map_data::BuildingDef> {
    WILLOW_VILLAGE.building(TRIAL_SHELTER).filter(|shelter| {
        world.village_building_active(shelter.id)
            && world.can_walk(shelter.door.x.into(), shelter.door.y.into())
    })
}

const DAYS_PER_SEASON: u64 = 28;
const DAYS_PER_CYCLE: u64 = 4 * DAYS_PER_SEASON;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Stage {
    Request,
    Pack,
    Meet,
    Work,
    Deliver,
    Return,
    Trial,
    Refund,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Appointment {
    chapter: u8,
    resident: u8,
    stage: Stage,
    host: Option<ActorKind>,
    guest_site: TilePos,
    host_site: TilePos,
    appointment: u64,
    deadline: u64,
    due: u64,
    input: Option<Cargo>,
    supplies: Option<Cargo>,
    seed_return: bool,
    accepted: bool,
    plot: Option<u8>,
    #[serde(default)]
    phase_work: u8,
    #[serde(default)]
    bag: Option<Cargo>,
    #[serde(default)]
    guest_loaded: Option<Cargo>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
enum ToolPlace {
    #[default]
    Workshop,
    Borrowed(ActorKind),
    Returned,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
enum WinterItem {
    #[default]
    InUse,
    Patched,
    TestFit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
struct MarketReview {
    next_batch: u8,
    leftover: u8,
    guide: bool,
    water_tested: bool,
    session: Option<MarketSession>,
    fixed_use: bool,
    autumn_trade_year: Option<u64>,
    trading_day: Option<u64>,
    staffed_day: Option<u64>,
    cleanup_day: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MarketSession {
    chapter: u8,
    batch: u8,
    sold: u8,
    opened: bool,
    day: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
struct NeighbourPlan {
    provisions: u8,
    prepared: bool,
    water: u8,
    fuel: u8,
    first_day: Option<u64>,
    second_day: Option<u64>,
    trial_days: u8,
    trial_day: Option<u64>,
    first_stable_days: u8,
    reserve_water_day: Option<u64>,
    farmer_work_day: Option<u64>,
    tailor_work_day: Option<u64>,
    checked_day: Option<u64>,
    good_days: u8,
    duty_days: u8,
    closing_after_second: bool,
    season: Option<u8>,
    decoration: Option<Cargo>,
    retired_decoration: Option<Cargo>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    completed: u32,
    next_offer: u64,
    active: Option<Appointment>,
    known: Option<u8>,
    familiar_return: bool,
    tool: ToolPlace,
    winter: WinterItem,
    deferred: Option<(u8, u64)>,
    quiet_hours: bool,
    returned_harvest_at: Option<u64>,
    market: MarketReview,
    neighbours: NeighbourPlan,
}

impl State {
    pub(super) fn completed_id(&self, id: &str) -> bool {
        id.strip_prefix('Q')
            .and_then(|n| n.parse().ok())
            .is_some_and(|n| self.completed(n))
    }

    fn completed(&self, id: u8) -> bool {
        CHAPTERS
            .iter()
            .position(|&q| q == id)
            .is_some_and(|bit| self.completed & (1 << bit) != 0)
    }

    fn complete(&mut self, id: u8) {
        let bit = CHAPTERS
            .iter()
            .position(|&q| q == id)
            .expect("owned village chapter");
        self.completed |= 1 << bit;
    }

    fn close_booking(&mut self, now: u64) {
        self.next_offer = now + 25 * MINUTE;
    }

    pub(super) fn progress(&self) -> SocialProgress {
        SocialProgress {
            market_after_review: self.completed(49),
            market_cleanup: self.completed(52),
            fixed_market: self.completed(53),
            matured_village: self.completed(65),
            harvest_gathering: self.completed(67),
            winter_workshop: self.completed(68),
            familiar_visitor: self.familiar_return,
            last_market_cleanup_day: self.market.cleanup_day,
        }
    }

    pub(super) fn host(&self) -> Option<&ActorKind> {
        self.active.as_ref()?.host.as_ref()
    }

    pub(super) fn market_planning_guest(&self) -> Option<usize> {
        self.active
            .filter(|a| matches!(a.chapter, 47 | 50 | 51 | 67) && a.stage == Stage::Request)
            .map(|a| usize::from(a.resident))
    }

    pub(super) fn reserved_harvest_plot(&self) -> Option<u8> {
        self.active
            .filter(|a| a.chapter == 38 && matches!(a.stage, Stage::Request | Stage::Meet))?
            .plot
    }

    pub(super) fn following(&self, id: usize) -> bool {
        self.active
            .is_some_and(|a| usize::from(a.resident) == id && a.stage != Stage::Request)
    }
    pub(super) fn visit_token(&self) -> bool {
        self.active
            .is_some_and(|a| !matches!(a.stage, Stage::Request | Stage::Trial))
    }

    pub(super) fn public_corner(&self) -> Option<(TilePos, u8)> {
        self.neighbours.decoration?;
        let tile = WILLOW_VILLAGE.building("commons_hall")?.door;
        Some((tile, self.neighbours.season?))
    }

    pub(super) fn hold_arrival(&self, id: usize) -> bool {
        (id == FARMER && !self.completed(62)) || (id == TAILOR && !self.completed(63))
    }

    pub(super) fn is_destination_reserved(&self, p: TilePos) -> bool {
        self.active.is_some_and(|a| {
            !matches!(a.stage, Stage::Request | Stage::Return | Stage::Trial)
                && (p == a.guest_site || p == a.host_site)
        })
    }

    pub(super) fn carried_item(&self, id: usize) -> Option<Item> {
        let appointment = self.active.filter(|a| usize::from(a.resident) == id)?;
        appointment
            .supplies
            .or(appointment.guest_loaded)
            .map(|cargo| cargo.item)
    }

    pub(super) fn host_held(&self, owner: ActorKind) -> Option<VisitorHeld> {
        if self.tool == ToolPlace::Borrowed(owner) {
            return Some(VisitorHeld::Tool(VisitorTool::Hammer));
        }
        let appointment = self.active.filter(|a| a.host == Some(owner))?;
        appointment.bag.map(|bag| VisitorHeld::Goods(bag.item))
    }

    pub(super) fn resident_activity(&self, id: usize) -> Option<&'static str> {
        let a = self
            .active
            .filter(|a| usize::from(a.resident) == id && a.stage != Stage::Request)?;
        Some(match (a.chapter, a.stage) {
            (_, Stage::Return) => "沿约定路线回住处",
            (36, _) => "带木样去家中作坊",
            (37, _) => "等候借物使用归还",
            (38, Stage::Deliver) => "帮忙把收获带回粮仓",
            (38, _) => "与家人分工收菜",
            (40, _) => "赴改约后的会面",
            (46, _) => "领取存放的村路包裹",
            (61, _) => "去客舍短住",
            (62 | 63, _) => "搬家中原来的物件",
            (66, _) => "带种盒下田",
            (67, _) => "秋日市场同桌",
            (68, _) => "在织坊修补农衣",
            (69, _) => "布置公共会堂一角",
            _ => "按约见面",
        })
    }
    pub(super) fn prepare_visitor_water(&self, id: usize, day: u64) -> bool {
        id == IRRIGATOR
            && self.neighbours.prepared
            && !self.completed(62)
            && self.neighbours.water < 7
            && self.neighbours.reserve_water_day != Some(day)
    }
    pub(super) fn fill_visitor_water(&mut self, world: &mut LivingWorld, at: TilePos) {
        let day = world.world_ms() / DAY_MS;
        if self.prepare_visitor_water(IRRIGATOR, day) && world.village_draw_visitor_water(at) {
            self.neighbours.water += 1;
            self.neighbours.reserve_water_day = Some(day);
        }
    }
    pub(super) fn helper_route(&self, world: &LivingWorld) -> bool {
        self.completed(36)
            && !self.completed(38)
            && world.world_ms() / DAY_MS >= 56
            && world.village_chapter_completed("Q10")
            && world.minute_of_day() < 780
    }

    fn harvest_candidate(&self, world: &LivingWorld, worker: TilePos) -> Option<u8> {
        (0..WILLOW_VILLAGE.crops.len())
            .map(|index| index as u8)
            .filter(|&plot| world.social_eligible_harvest(plot))
            .min_by_key(|&plot| distance(worker, WILLOW_VILLAGE.crops[usize::from(plot)].work))
    }

    pub(super) fn temporary_bed(&self, id: usize, tile: TilePos) -> bool {
        id == FARMER
            && !self.completed(62)
            && WILLOW_VILLAGE
                .building("craft_inn")
                .is_some_and(|inn| inn.door == tile)
    }

    pub(super) fn returning_tailor_at_boundary(&self, id: usize, walk: &super::Walker) -> bool {
        id == TAILOR
            && walk.state == super::State::Away
            && walk.at == WILLOW_VILLAGE.boundary.spawn
            && self
                .active
                .is_some_and(|a| a.chapter == 63 && a.stage == Stage::Return && !a.accepted)
    }

    pub(super) fn protect_trial_candidate(&self, id: usize) -> bool {
        id == FARMER && self.completed(60) && !self.completed(62)
            || id == TAILOR && self.completed(62) && !self.completed(63)
    }

    pub(super) fn uses_guest_bed(&self) -> bool {
        self.completed(60) && !self.completed(62)
    }

    pub(super) fn note_neighbour_work(&mut self, resident: usize, day: u64, farm: bool) {
        match resident {
            FARMER if farm => self.neighbours.farmer_work_day = Some(day),
            TAILOR if !farm => self.neighbours.tailor_work_day = Some(day),
            _ => {}
        }
    }

    pub(super) fn tailoring_cover_needed(&self, day: u64) -> bool {
        self.completed(63)
            && !self.completed(65)
            && self.neighbours.tailor_work_day.is_none_or(|d| d + 2 < day)
    }

    pub(super) fn market_active(&self, day: u64) -> bool {
        self.market
            .session
            .is_some_and(|s| s.day == day && s.sold < s.batch)
    }

    pub(super) fn should_stage_seed_merchant(&self, world: &LivingWorld, day: u64) -> bool {
        day >= 224
            && world.town_allows(TownEra::for_chapter(48))
            && (day % 7 == 2 || day % 7 == 4)
            && self.awaiting_seed_session()
    }

    pub(super) fn neighbour_food_needed(&self) -> bool {
        self.completed(64) && !self.completed(65)
    }

    fn awaiting_seed_session(&self) -> bool {
        self.completed(47) && !self.completed(48)
            || self.completed(49) && self.completed(50) && self.completed(51) && !self.completed(52)
    }

    pub(super) fn crop_reserve_for_market(&self, world: &LivingWorld) -> Option<u16> {
        let day = world.world_ms() / DAY_MS;
        let first_plan = !self.completed(47)
            && day >= 112
            && self.completed(41)
            && world.village_chapter_completed("Q46")
            && world.village_chapter_completed("Q16");
        let trial = day >= 224 && self.awaiting_seed_session();
        ((first_plan || trial) && world.town_allows(TownEra::Market)
            || self.fixed_market_needs_sale(world)
            || self.completed(53)
                && !self.completed(67)
                && world.town_allows(TownEra::for_chapter(67))
                && world.season() == YardSeason::Autumn
                && self.market.autumn_trade_year != Some(day / DAYS_PER_CYCLE))
        .then_some(10)
    }

    pub(super) fn guide_target(&self, mask: PhaseMask, index: usize) -> Option<TilePos> {
        self.market.guide.then_some(())?;
        slot(
            mask,
            if index % 2 == 0 {
                "stall_seed.door"
            } else {
                "stall_meal.door"
            },
            "queue1",
        )
    }
    pub(super) fn market_shuttered(&self, day: u64) -> bool {
        self.market.session.is_some_and(|s| s.day == day)
    }

    pub(super) fn fixed_market_needs_sale(&self, world: &LivingWorld) -> bool {
        self.completed(52)
            && !self.completed(53)
            && world.town_allows(TownEra::for_chapter(53))
            && world.world_ms() / DAY_MS >= 336
            && world.village_chapter_completed("Q56")
    }

    pub(super) fn autumn_market_needs_sale(&self, world: &LivingWorld) -> bool {
        self.completed(53)
            && !self.completed(67)
            && world.town_allows(TownEra::for_chapter(67))
            && self.market.autumn_trade_year != Some(world.world_ms() / (DAYS_PER_CYCLE * DAY_MS))
            && world.season() == YardSeason::Autumn
            && world.stock().count(Item::Crops) >= 8
    }

    pub(super) fn autumn_cook_preparing(&self, world: &LivingWorld, id: usize) -> bool {
        id == COOK
            && self.completed(53)
            && !self.completed(67)
            && world.town_allows(TownEra::for_chapter(67))
            && world.season() == YardSeason::Autumn
            && world.world_ms() / DAY_MS >= 336
    }

    pub(super) fn maturity_needs_sale(&self, world: &LivingWorld) -> bool {
        self.completed(63)
            && !self.completed(65)
            && !self.neighbours.closing_after_second
            && world.stock().count(Item::Crops) >= 8
    }

    pub(super) fn regular_market_week(&self, world: &LivingWorld, now: u64) -> bool {
        world.town_allows(TownEra::Mature)
            && matches!(world.town_era(), TownEra::Mature | TownEra::Heritage)
            && self.completed(53)
            && self.completed(65)
            && self.completed(67)
            && self
                .market
                .trading_day
                .is_none_or(|day| day / 7 < now / (7 * DAY_MS))
    }

    pub(super) fn witness_sale(&mut self, index: usize, world: &LivingWorld, now: u64) {
        if index > MOBILE {
            return;
        }
        if index < MOBILE {
            self.market.trading_day = Some(now / DAY_MS);
            if self.completed(52)
                && world.town_allows(TownEra::for_chapter(53))
                && world.village_chapter_completed("Q56")
                && phase_active(world.phase_mask(), "permanent_market")
            {
                self.market.fixed_use = true;
            }
            if self.completed(53)
                && world.season() == YardSeason::Autumn
                && shop(index).cost.item == Item::Crops
                && world.stock().count(Item::Crops) >= 6
            {
                self.market.autumn_trade_year = Some(now / (DAYS_PER_CYCLE * DAY_MS));
            }
            return;
        }
        if let Some(session) = &mut self.market.session
            && session.day == now / DAY_MS
            && session.opened
            && session.sold < session.batch
        {
            session.sold += 1;
        }
    }

    pub(super) fn valid(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        let day = now / DAY_MS;
        self.completed & !CHAPTER_MASK == 0
            && self.conserved_loan()
            && self.next_offer <= now.saturating_add(DAY_MS)
            && self.known.is_none_or(|id| usize::from(id) < RESIDENT_COUNT)
            && self.deferred.is_none_or(|(id, due)| {
                usize::from(id) == CARPENTER && due <= now.saturating_add(DAY_MS)
            })
            && self.market.leftover <= 3
            && self.market.next_batch <= 2
            && self.market.trading_day.is_none_or(|d| d <= day)
            && self.market.staffed_day.is_none_or(|d| d <= day)
            && self.market.cleanup_day.is_none_or(|d| d <= day)
            && self
                .market
                .autumn_trade_year
                .is_none_or(|y| y <= day / DAYS_PER_CYCLE)
            && self.returned_harvest_at.is_none_or(|at| at <= now)
            && self.market.session.is_none_or(|s| {
                matches!(s.chapter, 48 | 52)
                    && (1..=2).contains(&s.batch)
                    && s.sold <= s.batch
                    && s.day <= day
            })
            && self.neighbours.provisions <= 7
            && self.neighbours.water <= 7
            && self.neighbours.fuel <= 2
            && self.neighbours.trial_days <= 3
            && self.neighbours.first_stable_days <= 14
            && self.neighbours.good_days <= 28
            && self.neighbours.duty_days <= 14
            && self.neighbours.trial_day.is_none_or(|d| d <= day)
            && self.neighbours.reserve_water_day.is_none_or(|d| d <= day)
            && self.neighbours.farmer_work_day.is_none_or(|d| d <= day)
            && self.neighbours.tailor_work_day.is_none_or(|d| d <= day)
            && self.neighbours.checked_day.is_none_or(|d| d <= day)
            && self.neighbours.first_day.is_none_or(|d| d <= day)
            && self.neighbours.second_day.is_none_or(|d| d <= day)
            && (!self.completed(62) || self.completed(61) && self.neighbours.first_day.is_some())
            && (!self.completed(63) || self.completed(62) && self.neighbours.second_day.is_some())
            && (!self.completed(65) || self.completed(64))
            && (!self.completed(69) || self.neighbours.decoration.is_some())
            && self.neighbours.season.is_none_or(|s| s <= 3)
            && self.neighbours.decoration.is_none_or(|c| c.quantity == 1)
            && self
                .neighbours
                .retired_decoration
                .is_none_or(|c| c == cargo(Item::Wood, 1))
            && self.active.is_none_or(|a| {
                (CHAPTERS.contains(&a.chapter) || matches!(a.chapter, 46 | 71))
                    && usize::from(a.resident) < RESIDENT_COUNT
                    && a.guest_site.index().is_some()
                    && a.host_site.index().is_some()
                    && a.appointment <= now
                    && a.deadline >= a.appointment
                    && a.deadline <= now.saturating_add(MAX_APPOINTMENT_LENGTH)
                    && a.due <= now.saturating_add(APPOINTMENT_LENGTH)
                    && (!matches!(a.stage, Stage::Pack | Stage::Work | Stage::Deliver)
                        || a.host.is_some())
                    && (a.stage != Stage::Meet || a.chapter == 46 || a.host.is_some())
                    && a.host.is_none_or(|kind| {
                        kind != ActorKind::Orange && world.activity_deadline(kind).is_some()
                    })
                    && a.input.is_none_or(|c| (1..=7).contains(&c.quantity))
                    && a.supplies.is_none_or(|c| (1..=7).contains(&c.quantity))
                    && a.bag.is_none_or(|c| Some(c) == a.input)
                    && a.guest_loaded
                        .is_none_or(|c| c.quantity == 1 && matches!(a.chapter, 36 | 62 | 63))
                    && a.resident == resident_for(a.chapter).map_or(u8::MAX, |n| n as u8)
                    && a.phase_work <= 1
                    && a.plot
                        .is_none_or(|plot| usize::from(plot) < WILLOW_VILLAGE.crops.len())
            })
    }
}

impl State {
    #[cfg(test)]
    pub(super) fn market_probe(&self) -> Option<(u8, &'static str)> {
        self.active.map(|a| {
            let stage = match a.stage {
                Stage::Request => "request",
                Stage::Pack => "pack",
                Stage::Meet => "meet",
                Stage::Work => "work",
                Stage::Deliver => "deliver",
                Stage::Return => "return",
                Stage::Trial => "trial",
                Stage::Refund => "refund",
            };
            (a.chapter, stage)
        })
    }

    pub(super) fn update(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        mask: PhaseMask,
        now: u64,
    ) {
        self.update_market(life, world, now);
        self.update_neighbours(life, world, now);
        if let Some(mut active) = self.active.take() {
            self.step(life, world, mask, now, &mut active);
            return;
        }
        let minute = world.minute_of_day();
        let closes = if self.quiet_hours { 870 } else { 900 };
        if self.next_offer > now || !(465..closes).contains(&minute) || life.trip.is_some() {
            return;
        }
        self.next_offer = now + 8 * MINUTE;
        if life.present() >= MAX_HUMAN_VISITORS {
            return;
        }
        let day = now / DAY_MS;
        let q = |n| self.completed(n);
        let w = |id| world.village_chapter_completed(id);
        let dry = !matches!(world.weather(), YardWeather::Rain | YardWeather::Snow);
        let helper = &life.people[HELPER].walk;
        let candidate_harvest = (day >= 56
            && dry
            && q(36)
            && !q(38)
            && w("Q10")
            && !helper.hidden()
            && life.people[HELPER].can_accept(cargo(Item::Crops, 3))
            && life.people[HELPER].can_accept(cargo(Item::Seeds, 1)))
        .then(|| self.harvest_candidate(world, helper.at))
        .flatten()
        .filter(|&plot| distance(helper.at, WILLOW_VILLAGE.crops[usize::from(plot)].work) <= 40);
        let retry_harvest = q(36) && !q(38) && day >= 56;
        let chapter = [
            (46, world.public_order_ready()),
            (
                36,
                day >= 14 && w("Q05") && w("Q24") && life.people[CARPENTER].count(Item::Wood) > 0,
            ),
            (37, q(36) && w("Q02")),
            (38, candidate_harvest.is_some()),
            (
                39,
                q(38) && w("Q35") && world.stock().count(Item::Food) >= 8,
            ),
            (40, q(37) && q(39)),
            (41, q(40) && w("Q29")),
            (
                47,
                day >= 112
                    && q(41)
                    && w("Q46")
                    && w("Q16")
                    && life.people[SEEDS].count(Item::Seeds)
                        + life.people[SEEDS].counter_goods(MOBILE)
                        >= shop(MOBILE).goods.quantity
                    && world.stock().count(Item::Crops) >= 8,
            ),
            (
                50,
                q(49)
                    && w("Q18")
                    && trial_shelter(world).is_some()
                    && world.stock().count(Item::Water) >= 3,
            ),
            (51, q(48) && w("Q35")),
            (
                60,
                day >= 336
                    && !self.neighbours.prepared
                    && q(53)
                    && w("Q59")
                    && w("Q15")
                    && world.stock().count(Item::Food) >= 8,
            ),
            (
                61,
                day >= 448
                    && q(60)
                    && q(41)
                    && self.neighbours.provisions == 7
                    && world.stock().count(Item::Food) >= 7,
            ),
            (
                62,
                day >= 672
                    && q(61)
                    && q(60)
                    && self.neighbours.trial_days == 3
                    && world.stock().count(Item::Food) >= 7,
            ),
            (
                63,
                day >= 686
                    && q(62)
                    && self.neighbours.first_day.is_some_and(|d| day >= d + 14)
                    && self.neighbours.first_stable_days >= 14
                    && world.stock().count(Item::Food) >= 7,
            ),
            (
                66,
                day >= 56
                    && world.season() == YardSeason::Spring
                    && q(41)
                    && w("Q23")
                    && w("Q13")
                    && world.plots().iter().any(|p| p.stage == PlotStage::Tilled)
                    && world.stock().count(Item::Seeds) >= 2,
            ),
            (
                67,
                day >= 336
                    && world.season() == YardSeason::Autumn
                    && q(53)
                    && self.market.autumn_trade_year == Some(day / DAYS_PER_CYCLE)
                    && w("Q16")
                    && world.stock().count(Item::Food) >= 7
                    && life.people[COOK].count(Item::Food) > 0,
            ),
            (
                68,
                day >= 336
                    && world.season() == YardSeason::Winter
                    && w("Q58")
                    && w("Q15")
                    && (self.winter != WinterItem::InUse
                        || life.services.damaged(14) && world.stock().count(Item::Rope) > 0),
            ),
            (
                69,
                day >= 700 && q(65) && q(67) && q(68) && world.stock().count(Item::Wood) > 0,
            ),
            (
                71,
                q(65) && !self.familiar_return && self.known == Some(CARPENTER as u8) && w("Q70"),
            ),
        ]
        .into_iter()
        .find_map(|(id, ready)| {
            (ready
                && world.town_allows(TownEra::for_chapter(id))
                && (!q(id) || id == 69 && self.neighbours.season != Some(world.season() as u8))
                && resident_for(id).is_some_and(|resident| {
                    life.people[resident].walk.state != super::State::Unsettled
                        || matches!(id, 61 | 63)
                }))
            .then_some(id)
        });
        let Some(chapter) = chapter else {
            if retry_harvest {
                self.next_offer = now + 2 * MINUTE;
            }
            return;
        };
        if chapter == 40 {
            // The carpenter only offers the first visit while both owners
            // are actually occupied. The same request survives the change.
            if self.deferred.is_none() {
                if HOMEBOUND[..2]
                    .iter()
                    .all(|&actor| !world.activity_available(actor))
                {
                    self.deferred = Some((CARPENTER as u8, now + 3 * HOUR));
                }
                return;
            }
            if self.deferred.is_some_and(|(_, then)| then > now) {
                return;
            }
        }
        let (site, input) = match chapter {
            36 | 37 | 40 | 41 => ("home_workshop.door", None),
            38 => ("crop_04", None),
            39 => ("home_table", Some(cargo(Item::Food, 1))),
            46 => ("market_gate", None),
            47 => ("home_trade", None),
            50 => (
                trial_shelter(world).unwrap().door_poi,
                Some(cargo(Item::Water, 1)),
            ),
            51 => ("market_well", Some(cargo(Item::Water, 1))),
            60 => ("home_pantry.door", None),
            61 => ("craft_inn.door", Some(cargo(Item::Food, 1))),
            62 => (house(FARMER).door_poi, Some(cargo(Item::Food, 1))),
            63 => (house(TAILOR).door_poi, Some(cargo(Item::Food, 1))),
            66 => ("home_trade", Some(cargo(Item::Seeds, 1))),
            67 => ("stall_meal.door", Some(cargo(Item::Food, 1))),
            68 => (
                "craft_textile.door",
                (self.winter == WinterItem::InUse).then_some(cargo(Item::Rope, 1)),
            ),
            69 => ("commons_hall.door", Some(cargo(Item::Wood, 1))),
            71 => ("home_workshop.door", None),
            _ => return,
        };
        let resident = resident_for(chapter).expect("accepted appointment has named visitor");
        let plot = if chapter == 38 {
            candidate_harvest
        } else if chapter == 66 {
            (0..WILLOW_VILLAGE.crops.len())
                .filter(|&tile| {
                    tile != WHEAT_PLOT && mask.contains(WILLOW_VILLAGE.crops[tile].phase)
                })
                .find(|&index| world.plot_stage(index) == Some(PlotStage::Tilled))
                .map(|index| index as u8)
        } else {
            None
        };
        if matches!(chapter, 38 | 66) && plot.is_none() {
            return;
        }
        let destination = if chapter == 38 {
            plot.map(|id| WILLOW_VILLAGE.crops[usize::from(id)].work)
        } else {
            life.interaction_position(mask, site)
        };
        let Some(destination) = destination else {
            return;
        };
        let guest = if matches!(chapter, 38 | 46 | 62 | 63) {
            Some(destination)
        } else {
            event_stands(world, destination)
                .find(|p| !life.occupied(world, *p, Some(resident), None))
        };
        let Some(guest_site) = guest else {
            return;
        };
        let Some(host_site) = event_stands(world, guest_site)
            .find(|&p| p != destination && !life.occupied(world, p, None, None))
        else {
            return;
        };
        let duration = if chapter == 71 {
            MAX_APPOINTMENT_LENGTH
        } else if matches!(chapter, 38 | 46 | 50 | 51 | 61 | 62 | 63 | 67 | 68 | 69) {
            5 * HOUR
        } else {
            APPOINTMENT_LENGTH
        };
        self.active = Some(Appointment {
            chapter,
            resident: resident as u8,
            stage: Stage::Request,
            host: None,
            guest_site,
            host_site,
            appointment: now,
            deadline: now + duration,
            due: now,
            input,
            supplies: None,
            seed_return: false,
            accepted: false,
            plot,
            phase_work: 0,
            bag: None,
            guest_loaded: None,
        });
    }

    fn step(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        mask: PhaseMask,
        now: u64,
        a: &mut Appointment,
    ) {
        let id = usize::from(a.resident);
        let wet = matches!(world.weather(), YardWeather::Rain | YardWeather::Snow);
        if a.stage == Stage::Request {
            if now >= a.deadline || wet && matches!(a.chapter, 38 | 66 | 67) {
                self.next_offer = now + 20 * MINUTE;
                return;
            }
            let person = &life.people[id].walk;
            if !world.can_walk(a.guest_site.x.into(), a.guest_site.y.into())
                || !world.can_walk(a.host_site.x.into(), a.host_site.y.into())
            {
                return;
            }
            let origin = if matches!(a.chapter, 61 | 63) && person.state == super::State::Unsettled
            {
                WILLOW_VILLAGE.boundary.spawn
            } else {
                person.at
            };
            let posted_order = a.chapter == 46 && world.public_order_ready();
            if person.edge.is_some()
                || main_bridge_deck(person.at)
                || life.people[id].counter.is_some()
                || life.people[id].returning_cargo.is_some()
                || life.people[id].produce_in_transit.is_some()
                || life.people[id].service_material.is_some()
                || life.people[id].neighbour_meal.is_some()
                || life.people[id].escrow.is_some()
                || life.services.carried_asset(id as u8).is_some()
                || person.state == super::State::Away
                || person.hidden() && life.occupied(world, origin, Some(id), None)
                || outer_corridor(origin) && life.corridor_reserved(Some(id), None)
                || life.trip.is_some()
                || life.present() >= MAX_HUMAN_VISITORS
                || a.chapter == 61
                    && life
                        .guests
                        .iter()
                        .flatten()
                        .any(|guest| guest.bed.is_some_and(|bed| bed < 2))
            {
                self.active = Some(*a);
                return;
            }
            if posted_order {
                life.clear_order(id);
                life.people[id].job = None;
                life.people[id].job_stop = 0;
                life.people[id].walk.at = origin;
                life.people[id]
                    .walk
                    .redirect(a.guest_site, super::State::Outbound, now);
                a.stage = Stage::Meet;
                self.active = Some(*a);
                return;
            }
            let host = [ActorKind::Xiaohe, ActorKind::Ayu]
                .into_iter()
                .find(|&kind| {
                    let p = world.actor_tile(kind);
                    let actor = &world.actors()[kind as usize];
                    distance(p, a.host_site) <= 160
                        && actor.hunger() < 60
                        && actor.thirst() < 66
                        && actor.stamina() >= 42
                        && world.reserve_market_trip(kind)
                });
            let Some(host) = host else {
                self.active = Some(*a);
                return;
            };
            let pack_site = slot(mask, "pantry_loading", "load").expect("pantry dock");
            if !world.move_for_activity(
                host,
                point(if a.input.is_some() {
                    pack_site
                } else {
                    a.host_site
                }),
            ) {
                world.end_activity(&[host], 0);
                self.active = Some(*a);
                return;
            }
            life.clear_order(id);
            life.people[id].job = None;
            life.people[id].job_stop = 0;
            if matches!(a.chapter, 62 | 63) {
                let item = if a.chapter == 62 {
                    Item::Seeds
                } else {
                    Item::Rope
                };
                let stock = cargo(item, 1);
                a.guest_loaded = life.people[id].outgoing(stock).then_some(stock);
            }
            let person = &mut life.people[id].walk;
            person.at = origin;
            let collect_at = if a.chapter == 36 {
                house(id).door
            } else {
                a.guest_site
            };
            person.redirect(collect_at, super::State::Outbound, now);
            a.host = Some(host);
            a.stage = if a.input.is_some() {
                Stage::Pack
            } else {
                Stage::Meet
            };
            a.due = now + 10 * MINUTE;
        }
        if a.stage == Stage::Pack {
            let host = a.host.expect("a loaded appointment owns its host");
            let dock = slot(mask, "pantry_loading", "load").expect("pantry dock");
            if now >= a.deadline || world.activity_deadline(host).is_none() {
                self.finish_host(world, a);
                self.return_resident(life, world, mask, now, a);
            } else if world.activity_arrived(host) && world.actor_tile(host) == dock {
                let price = a.input.expect("packing an offered resource");
                let reserve = match price.item {
                    Item::Food => 6,
                    Item::Water => 2,
                    Item::Seeds => 1,
                    _ => 0,
                };
                if world.stock().count(price.item) >= price.quantity + reserve
                    && world.spend_stock(price.item, price.quantity)
                {
                    a.bag = Some(price);
                    if world.move_for_activity_with_load(host, point(a.host_site), true) {
                        a.stage = Stage::Meet;
                    } else {
                        self.cancel(life, world, mask, now, a);
                    }
                }
            }
        }
        if a.stage == Stage::Meet {
            if a.chapter == 36
                && a.guest_loaded.is_none()
                && life.people[id].walk.at == house(id).door
                && life.people[id].walk.edge.is_none()
            {
                let sample = cargo(Item::Wood, 1);
                if life.people[id].outgoing(sample) {
                    a.guest_loaded = Some(sample);
                    life.people[id]
                        .walk
                        .redirect(a.guest_site, super::State::Outbound, now);
                }
            }
            if a.chapter == 46 {
                if life.people[id].walk.state == super::State::AtWork
                    && life.people[id].walk.at == a.guest_site
                    && life.people[id].can_accept(cargo(Item::Food, 2))
                {
                    if let Some((item, quantity)) = world.take_public_order() {
                        a.supplies = Some(cargo(item, quantity));
                        a.accepted = true;
                    }
                    self.return_resident(life, world, mask, now, a);
                } else if now >= a.deadline {
                    self.return_resident(life, world, mask, now, a);
                }
                self.active = Some(*a);
                return;
            }
            let Some(host) = a.host else {
                return;
            };
            if now >= a.deadline
                || world.activity_deadline(host).is_none()
                || wet && matches!(a.chapter, 38 | 66 | 67)
            {
                self.cancel(life, world, mask, now, a);
            } else if now >= a.due
                && world.activity_arrived(host)
                && world.actor_tile(host) == a.host_site
                && life.people[id].walk.state == super::State::AtWork
                && life.people[id].walk.at == a.guest_site
            {
                self.meet(life, world, mask, now, a);
            }
        }
        if a.stage == Stage::Work {
            let Some(host) = a.host else {
                return;
            };
            if now >= a.deadline || world.activity_deadline(host).is_none() {
                self.cancel(life, world, mask, now, a);
            } else if world.activity_arrived(host)
                && world.actor_tile(host) == a.host_site
                && life.people[id].walk.state == super::State::AtWork
                && now >= a.due
            {
                if self.tool == ToolPlace::Borrowed(host) && a.chapter == 37 {
                    let workshop = resolve(mask, "home_workshop.door").expect("day one workshop");
                    if a.phase_work == 0 {
                        a.phase_work = 1;
                        a.due = now + WORK;
                        world.external_pose(host as usize, Pose::Work);
                    } else if a.host_site != workshop
                        && world.move_for_activity(host, point(workshop))
                    {
                        a.host_site = workshop;
                        a.due = now + 10 * MINUTE;
                        world.external_pose(host as usize, Pose::Work);
                    } else if a.host_site == workshop && a.phase_work == 1 {
                        self.tool = ToolPlace::Returned;
                        a.accepted = true;
                        self.finish_host(world, a);
                        self.return_resident(life, world, mask, now, a);
                    }
                } else if a.chapter == 66 {
                    if let Some(plot) = a.plot
                        && world.village_local_work("sow", Some(plot))
                    {
                        a.accepted = true;
                    }
                    self.finish_host(world, a);
                    self.return_resident(life, world, mask, now, a);
                } else if a.chapter == 50 {
                    self.market.water_tested = true;
                    a.accepted = true;
                    self.finish_host(world, a);
                    self.return_resident(life, world, mask, now, a);
                } else if a.chapter == 68 && self.winter == WinterItem::Patched {
                    self.winter = WinterItem::TestFit;
                    a.accepted = true;
                    self.finish_host(world, a);
                    self.return_resident(life, world, mask, now, a);
                } else if a.chapter == 68 && self.winter == WinterItem::TestFit {
                    a.accepted = true;
                    self.cancel(life, world, mask, now, a);
                } else if a.chapter == 71 {
                    a.accepted = self.known == Some(a.resident);
                    self.finish_host(world, a);
                    self.return_resident(life, world, mask, now, a);
                }
            }
        }
        if a.stage == Stage::Deliver {
            let Some(host) = a.host else {
                return;
            };
            if now >= a.deadline || world.activity_deadline(host).is_none() {
                self.finish_host(world, a);
                self.return_resident(life, world, mask, now, a);
            } else if world.activity_arrived(host)
                && world.actor_tile(host) == a.host_site
                && life.people[id].walk.state == super::State::AtWork
                && life.people[id].walk.at == a.guest_site
            {
                let goods = a.supplies.map(|c| (c.item, c.quantity));
                let seed = a.seed_return.then_some((Item::Seeds, 1));
                let mut received = [(Item::Food, 0); 2];
                for (slot, cargo) in received.iter_mut().zip([goods, seed].into_iter().flatten()) {
                    *slot = cargo;
                }
                let count = usize::from(goods.is_some()) + usize::from(seed.is_some());
                if world.exchange_stock_atomic(&[], &received[..count]) {
                    self.returned_harvest_at = Some(now);
                    a.supplies = None;
                    a.seed_return = false;
                    a.accepted = true;
                    self.finish_host(world, a);
                    self.return_resident(life, world, mask, now, a);
                }
            }
        }
        if a.stage == Stage::Return {
            let state = life.people[id].walk.state;
            if !a.accepted
                && now >= a.deadline.saturating_add(3 * HOUR)
                && !matches!(a.chapter, 61 | 62 | 63)
                && a.bag.is_none()
                && a.supplies.is_none()
                && !a.seed_return
                && a.guest_loaded.is_none()
            {
                self.next_offer = now + 2 * HOUR;
                return;
            }
            let declined_tailor = a.chapter == 63 && !a.accepted;
            let arrived = if declined_tailor {
                state == super::State::Away
                    && life.people[id].walk.at == WILLOW_VILLAGE.boundary.spawn
            } else {
                state == super::State::Inside
            };
            if arrived {
                if let Some(goods) = a.supplies {
                    if !life.people[id].incoming(goods) {
                        self.active = Some(*a);
                        return;
                    }
                    a.supplies = None;
                }
                if a.seed_return {
                    if !life.people[id].incoming(cargo(Item::Seeds, 1)) {
                        self.active = Some(*a);
                        return;
                    }
                    a.seed_return = false;
                }
                if let Some(item) = a.guest_loaded {
                    if !life.people[id].incoming(item) {
                        self.active = Some(*a);
                        return;
                    }
                    a.guest_loaded = None;
                }
                if declined_tailor {
                    life.people[id].walk.at = house(id).door;
                    life.people[id].walk.target = house(id).door;
                    life.people[id].walk.state = super::State::Unsettled;
                    life.people[id].walk.due = 0;
                }
                if a.chapter == 69 {
                    self.neighbours.retired_decoration = self
                        .neighbours
                        .retired_decoration
                        .take()
                        .filter(|&old| !life.people[id].incoming(old));
                }
                if a.accepted && a.chapter == 61 {
                    a.stage = Stage::Trial;
                    a.due = now;
                } else if a.accepted && a.chapter == 71 {
                    self.familiar_return = true;
                } else if a.accepted && a.chapter == 60 {
                    self.neighbours.prepared = true;
                } else if a.accepted && a.chapter != 46 {
                    if a.chapter == 62 {
                        self.neighbours.first_day = Some(now / DAY_MS);
                    }
                    if a.chapter == 63 {
                        self.neighbours.second_day = Some(now / DAY_MS);
                    }
                    self.complete(a.chapter);
                }
                if a.stage != Stage::Trial {
                    self.close_booking(now);
                    return;
                }
            }
            if a.stage == Stage::Return
                && life.people[id].walk.edge.is_none()
                && !matches!(state, super::State::Leaving | super::State::LeavingVillage)
            {
                self.return_resident(life, world, mask, now, a);
            }
        }
        if a.stage == Stage::Refund {
            self.cancel(life, world, mask, now, a);
        }
        if a.stage == Stage::Trial && self.neighbours.trial_days == 3 {
            self.complete(61);
            self.close_booking(now);
            return;
        }
        self.active = Some(*a);
    }

    fn finish_host(&mut self, world: &mut LivingWorld, a: &mut Appointment) {
        if let Some(host) = a.host.take() {
            world.end_activity(&[host], 3);
        }
    }

    fn cancel(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        mask: PhaseMask,
        now: u64,
        a: &mut Appointment,
    ) {
        if a.chapter == 37
            && let ToolPlace::Borrowed(owner) = self.tool
        {
            a.stage = Stage::Refund;
            let workshop = resolve(mask, "home_workshop.door").expect("day one workshop");
            a.host_site = workshop;
            if world.activity_deadline(owner).is_none() {
                a.host = None;
                if !world.reserve_market_trip(owner) {
                    return;
                }
                a.host = Some(owner);
            }
            if !world.activity_arrived(owner) {
                return;
            }
            if world.actor_tile(owner) != workshop {
                world.move_for_activity(owner, point(workshop));
                return;
            }
            self.tool = ToolPlace::Workshop;
        }
        if let Some(bag) = a.bag {
            a.stage = Stage::Refund;
            let dock = slot(mask, "pantry_loading", "load").expect("pantry dock");
            if a.host
                .is_some_and(|host| world.activity_deadline(host).is_none())
            {
                a.host = None;
            }
            if a.host.is_none() {
                a.host = [ActorKind::Xiaohe, ActorKind::Ayu]
                    .into_iter()
                    .find(|&host| world.reserve_market_trip(host));
            }
            if let Some(host) = a.host {
                if world.activity_arrived(host) {
                    if world.actor_tile(host) == dock {
                        if world.exchange_stock_atomic(&[], &[(bag.item, bag.quantity)]) {
                            a.bag = None;
                        }
                    } else {
                        world.move_for_activity_with_load(host, point(dock), true);
                    }
                }
                if a.bag.is_some() && now >= a.deadline + HOUR {
                    self.finish_host(world, a);
                    a.deadline = now + HOUR;
                }
            }
            if a.bag.is_some() {
                return;
            }
        }
        self.finish_host(world, a);
        self.return_resident(life, world, mask, now, a);
    }

    fn return_resident(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        mask: PhaseMask,
        now: u64,
        a: &mut Appointment,
    ) {
        let id = usize::from(a.resident);
        a.stage = Stage::Return;
        if life.people[id].walk.edge.is_some() {
            return;
        }
        let declined_tailor = a.chapter == 63 && !a.accepted;
        let destination = if declined_tailor {
            WILLOW_VILLAGE.boundary.spawn
        } else if a.chapter == 61 || a.chapter == 62 && !a.accepted {
            resolve(mask, "craft_inn.door").unwrap_or(house(id).door)
        } else {
            house(id).door
        };
        let walker = &life.people[id].walk;
        if walker.hidden()
            && walker.at != destination
            && life.occupied(world, walker.at, Some(id), None)
        {
            return;
        }
        let return_state = if declined_tailor {
            super::State::LeavingVillage
        } else {
            super::State::Leaving
        };
        life.people[id]
            .walk
            .redirect(destination, return_state, now);
        a.due = now;
    }

    fn meet(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        mask: PhaseMask,
        now: u64,
        a: &mut Appointment,
    ) {
        let id = usize::from(a.resident);
        let host = a.host.expect("meeting uses an arriving host");
        let accepted = match a.chapter {
            36 if a.guest_loaded == Some(cargo(Item::Wood, 1)) => {
                if !world.exchange_stock_atomic(&[], &[(Item::Wood, 1)]) {
                    return;
                }
                a.guest_loaded = None;
                self.known = Some(a.resident);
                life.services.build_rapport(id);
                true
            }
            37 if self.known == Some(a.resident) && self.tool == ToolPlace::Workshop => {
                self.tool = ToolPlace::Borrowed(host);
                a.stage = Stage::Work;
                a.due = now + WORK;
                world.external_pose(host as usize, Pose::Work);
                return;
            }
            38 => {
                if let Some(plot) = a.plot
                    && life.people[id].can_accept(cargo(Item::Crops, 3))
                    && life.people[id].can_accept(cargo(Item::Seeds, 1))
                    && let Some([crop, seeds]) = world.harvest_for_resident(plot)
                {
                    a.supplies = Some(cargo(crop.0, crop.1));
                    a.seed_return = seeds == (Item::Seeds, 1);
                    let dock = slot(mask, "pantry_loading", "load").expect("pantry dock");
                    let stand = [(37, 24), (36, 25), (33, 25)]
                        .into_iter()
                        .map(|(x, y)| TilePos::new(x, y))
                        .find(|&p| {
                            !pantry_service_zone(p)
                                && !life.occupied(world, p, Some(id), None)
                                && world.can_walk(p.x.into(), p.y.into())
                        });
                    if let Some(host_site) = stand
                        && world.move_for_activity(host, point(host_site))
                    {
                        a.host_site = host_site;
                        a.guest_site = dock;
                        a.stage = Stage::Deliver;
                        life.people[id]
                            .walk
                            .redirect(dock, super::State::Outbound, now);
                        return;
                    }
                }
                false
            }
            39 | 61 | 62 | 63 => {
                if matches!(a.chapter, 62 | 63) && a.guest_loaded.is_none() {
                    self.cancel(life, world, mask, now, a);
                    return;
                }
                let accepts_food = a.bag.take() == Some(cargo(Item::Food, 1));
                if accepts_food {
                    life.services.build_rapport(id);
                    life.people[id].supper_day = Some(now / DAY_MS);
                }
                accepts_food
            }
            67 => {
                if life.people[id].count(Item::Food) > 0
                    && a.bag == Some(cargo(Item::Food, 1))
                    && world.apply_community(CommunityAction::ShareFood(host), a.guest_site)
                {
                    assert!(life.people[id].outgoing(cargo(Item::Food, 1)));
                    life.people[id].supper_day = Some(now / DAY_MS);
                    a.bag = None;
                    life.services.build_rapport(id);
                    true
                } else {
                    false
                }
            }
            40 if self
                .deferred
                .is_some_and(|(resident, due)| resident == a.resident && now >= due) =>
            {
                self.deferred = None;
                life.services.build_rapport(id);
                true
            }
            41 if self.known == Some(a.resident) && self.tool == ToolPlace::Returned => {
                self.quiet_hours = true;
                true
            }
            47 => life.people[id].count(Item::Seeds) >= shop(MOBILE).goods.quantity,
            50 => {
                let sheltered = trial_shelter(world)
                    .is_some_and(|b| distance(b.door, life.people[id].walk.at) <= 2);
                if sheltered && a.bag == Some(cargo(Item::Water, 1)) {
                    a.bag = None;
                    a.stage = Stage::Work;
                    a.due = now + WORK;
                    world.external_pose(host as usize, Pose::Work);
                    return;
                }
                false
            }
            51 => {
                let seating = WILLOW_VILLAGE.scenic.iter().any(|s| {
                    matches!(s.kind, crate::map_data::ScenicKind::Bench)
                        && distance(s.tile, a.guest_site) <= 8
                });
                self.market.guide = seating && a.bag.take() == Some(cargo(Item::Water, 1));
                self.market.guide
            }
            60 => {
                let beds =
                    resolve(mask, "craft_inn.door").is_some() && mask.contains(house(FARMER).phase);
                beds && world.stock().count(Item::Food) >= 8 && world.stock().count(Item::Wood) >= 2
            }
            66 if life.people[id].can_accept(cargo(Item::Seeds, 1)) => {
                if let Some(plot) = a.plot
                    && a.bag.take() == Some(cargo(Item::Seeds, 1))
                    && life.people[id].incoming(cargo(Item::Seeds, 1))
                {
                    let site = WILLOW_VILLAGE.crops[usize::from(plot)].work;
                    let stand = event_stands(world, site)
                        .find(|&p| !life.occupied(world, p, Some(id), None));
                    if let Some(host_site) = stand
                        && world.move_for_activity(host, point(host_site))
                    {
                        a.host_site = host_site;
                        a.guest_site = site;
                        a.stage = Stage::Work;
                        a.due = now + WORK;
                        life.people[id]
                            .walk
                            .redirect(site, super::State::Outbound, now);
                        return;
                    }
                }
                false
            }
            68 if self.winter == WinterItem::InUse => {
                if a.bag == Some(cargo(Item::Rope, 1)) && life.services.repair(14, 1) {
                    a.bag = None;
                    self.winter = WinterItem::Patched;
                    a.stage = Stage::Work;
                    a.due = now + WORK;
                    world.external_pose(host as usize, Pose::Work);
                    return;
                }
                false
            }
            68 => {
                a.stage = Stage::Work;
                a.due = now + WORK;
                return;
            }
            69 => {
                if self.neighbours.retired_decoration.is_some()
                    || !life.people[id].can_accept(cargo(Item::Wood, 1))
                {
                    return;
                }
                if let Some(wood) = a.bag.take() {
                    self.neighbours.retired_decoration = self.neighbours.decoration.replace(wood);
                    self.neighbours.season = Some(world.season() as u8);
                }
                self.neighbours.season == Some(world.season() as u8)
            }
            71 if self.known == Some(a.resident) => {
                life.services.build_rapport(id);
                let Some(corner) = world.market_lamp_tiles().first().copied() else {
                    return;
                };
                let stand =
                    event_stands(world, corner).find(|&p| !life.occupied(world, p, Some(id), None));
                if let Some(guest_site) = stand {
                    let seat = event_stands(world, guest_site)
                        .find(|&p| !life.occupied(world, p, None, None));
                    if let Some(host_site) = seat
                        && world.move_for_activity(host, point(host_site))
                    {
                        a.host_site = host_site;
                        a.guest_site = guest_site;
                        a.stage = Stage::Work;
                        a.due = now + WORK;
                        life.people[id]
                            .walk
                            .redirect(guest_site, super::State::Outbound, now);
                    }
                }
                return;
            }
            _ => false,
        };
        a.accepted = accepted;
        if !accepted && a.bag.is_some() {
            self.cancel(life, world, mask, now, a);
        } else {
            self.finish_host(world, a);
            self.return_resident(life, world, mask, now, a);
        }
    }

    fn update_market(&mut self, life: &VillageLife, world: &LivingWorld, now: u64) {
        let day = now / DAY_MS;
        let minute = world.minute_of_day();
        if (480..990).contains(&minute)
            && phase_active(world.phase_mask(), "permanent_market")
            && SHOPS.iter().enumerate().any(|(index, shop)| {
                let seller = &life.people[merchant_at(index, now)];
                seller.walk.at == booth_slot(index)
                    && seller.walk.state == super::State::AtWork
                    && seller.counter_goods(index) >= shop.goods.quantity
                    && business_open(index, world.phase_mask(), now)
            })
        {
            self.market.staffed_day = Some(day);
        }
        if (self.market.trading_day == Some(day) || self.market.staffed_day == Some(day))
            && minute >= 1000
            && SHOPS.iter().enumerate().all(|(index, _)| {
                let seller = &life.people[merchant_at(index, now)].walk;
                seller.at != booth_slot(index)
            })
        {
            self.market.cleanup_day = Some(day);
        }
        if let Some(session) = self.market.session {
            let seller = &life.people[SEEDS].walk;
            if session.day != day
                || minute >= 760 && seller.at == house(SEEDS).door && seller.hidden()
            {
                self.market.session = None;
                if session.sold > 0 && session.opened {
                    self.market.leftover = session.batch - session.sold;
                    self.complete(session.chapter);
                }
            } else if !session.opened
                && seller.at == booth_slot(MOBILE)
                && seller.state == super::State::AtWork
            {
                self.market.session.as_mut().unwrap().opened = true;
            }
        }
        if self.completed(48)
            && !self.completed(49)
            && now >= self.next_offer
            && life.people[SEEDS].walk.hidden()
        {
            self.market.next_batch = if self.market.leftover > 0 { 1 } else { 2 };
            self.complete(49);
        }
        if day >= 224
            && world.town_allows(TownEra::for_chapter(48))
            && (day % 7 == 2 || day % 7 == 4)
            && (515..650).contains(&minute)
            && self.market.session.is_none()
            && world.stock().count(Item::Crops) >= 8
            && life.people[SEEDS].count(Item::Seeds) >= shop(MOBILE).goods.quantity
            && !matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            let chapter = if self.completed(47) && !self.completed(48) {
                Some(48)
            } else if self.completed(49)
                && self.completed(50)
                && self.completed(51)
                && !self.completed(52)
            {
                Some(52)
            } else {
                None
            };
            if let Some(chapter) = chapter {
                self.market.session = Some(MarketSession {
                    chapter,
                    batch: if chapter == 52 {
                        self.market.next_batch.clamp(1, 2)
                    } else {
                        2
                    },
                    sold: 0,
                    opened: false,
                    day,
                });
            }
        }
        if !self.completed(53)
            && self.completed(52)
            && world.town_allows(TownEra::for_chapter(53))
            && day >= 336
            && world.village_chapter_completed("Q56")
            && phase_active(world.phase_mask(), "permanent_market")
            && self.market.fixed_use
            && self
                .market
                .cleanup_day
                .zip(self.market.trading_day)
                .is_some_and(|(closed, sold)| closed >= sold)
        {
            self.complete(53);
        }
    }

    fn update_neighbours(&mut self, life: &mut VillageLife, world: &mut LivingWorld, now: u64) {
        let day = now / DAY_MS;
        let minute = world.minute_of_day();
        if self.neighbours.prepared && !self.completed(62) {
            for (item, cap, reserve, stock) in [
                (Item::Food, 7, 8, &mut self.neighbours.provisions),
                (Item::Wood, 2, 4, &mut self.neighbours.fuel),
            ] {
                if *stock < cap && world.stock().count(item) > reserve && world.spend_stock(item, 1)
                {
                    *stock += 1;
                }
            }
            if !self.completed(60)
                && self.neighbours.provisions == 7
                && self.neighbours.water == 7
                && self.neighbours.fuel == 2
            {
                self.complete(60);
            }
        } else if self.completed(62) {
            let returned = [
                (Item::Food, u16::from(self.neighbours.provisions)),
                (Item::Water, u16::from(self.neighbours.water)),
                (Item::Wood, u16::from(self.neighbours.fuel)),
            ];
            for (item, quantity) in returned {
                if quantity != 0 && world.exchange_stock_atomic(&[], &[(item, quantity)]) {
                    match item {
                        Item::Food => self.neighbours.provisions = 0,
                        Item::Water => self.neighbours.water = 0,
                        Item::Wood => self.neighbours.fuel = 0,
                        _ => unreachable!(),
                    }
                }
            }
        }
        if self.active.is_some_and(|a| a.stage == Stage::Trial)
            && !self.completed(62)
            && self.neighbours.trial_day != Some(day)
            && minute >= 1080
            && self.neighbours.provisions > 0
            && self.neighbours.water > 0
            && self.temporary_bed(FARMER, life.people[FARMER].walk.at)
            && life.people[FARMER].walk.state == super::State::Inside
        {
            self.neighbours.provisions -= 1;
            self.neighbours.water -= 1;
            if world.season() == YardSeason::Winter {
                self.neighbours.fuel = self.neighbours.fuel.saturating_sub(1);
            }
            self.neighbours.trial_days = self.neighbours.trial_days.saturating_add(1).min(3);
            self.neighbours.trial_day = Some(day);
        }
        if self.completed(62)
            && !self.completed(63)
            && self.neighbours.checked_day != Some(day)
            && minute >= 1260
        {
            if life.people[FARMER].supper_day == Some(day)
                && life.people[FARMER].walk.state == super::State::Inside
            {
                self.neighbours.checked_day = Some(day);
                self.neighbours.first_stable_days =
                    self.neighbours.first_stable_days.saturating_add(1).min(14);
            }
        }
        if self.completed(63) && self.neighbours.checked_day != Some(day) && minute >= 1260 {
            if self.neighbours.checked_day.is_some_and(|d| d + 1 < day) {
                self.neighbours.good_days = 0;
            }
            let settled_meal = [FARMER, TAILOR]
                .into_iter()
                .all(|id| life.people[id].supper_day == Some(day));
            if settled_meal
                && world.stock().count(Item::Food) >= 6
                && [FARMER, TAILOR]
                    .into_iter()
                    .all(|id| life.people[id].walk.state == super::State::Inside)
            {
                self.neighbours.checked_day = Some(day);
                if self
                    .neighbours
                    .farmer_work_day
                    .is_some_and(|d| d + 3 >= day)
                    && self
                        .neighbours
                        .tailor_work_day
                        .is_some_and(|d| d + 3 >= day)
                {
                    self.neighbours.duty_days = self.neighbours.duty_days.saturating_add(1).min(14);
                }
                if self.neighbours.duty_days == 14 && world.town_allows(TownEra::for_chapter(64)) {
                    self.complete(64);
                }
                self.neighbours.closing_after_second |= self
                    .market
                    .cleanup_day
                    .zip(self.neighbours.second_day)
                    .is_some_and(|(close, moved)| close >= moved);
                if self.completed(64)
                    && world.village_chapter_completed("Q57")
                    && self.neighbours.closing_after_second
                {
                    self.neighbours.good_days = self.neighbours.good_days.saturating_add(1).min(28);
                    if self.neighbours.good_days == 28
                        && day >= 700
                        && world.town_allows(TownEra::for_chapter(65))
                    {
                        self.complete(65);
                    }
                } else {
                    self.neighbours.good_days = 0;
                }
            } else if minute >= 1435 {
                self.neighbours.checked_day = Some(day);
                self.neighbours.good_days = 0;
            }
        }
    }
    fn conserved_loan(&self) -> bool {
        match self.tool {
            ToolPlace::Borrowed(host) => self.active.is_some_and(|a| {
                host != ActorKind::Orange
                    && a.chapter == 37
                    && (a.stage == Stage::Work && a.host == Some(host)
                        || a.stage == Stage::Refund
                            && !a.accepted
                            && a.bag.is_none()
                            && a.host.is_none_or(|current| current == host))
            }),
            ToolPlace::Returned => {
                self.completed(37)
                    || self
                        .active
                        .is_some_and(|a| a.chapter == 37 && a.accepted && a.stage == Stage::Return)
            }
            ToolPlace::Workshop => !self.completed(37),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn appointment(chapter: u8, who: usize, stage: Stage, now: u64) -> Appointment {
        Appointment {
            chapter,
            resident: who as u8,
            stage,
            host: None,
            guest_site: TilePos::new(43, 13),
            host_site: TilePos::new(44, 13),
            appointment: now,
            deadline: now + HOUR,
            due: now,
            input: None,
            supplies: None,
            seed_return: false,
            accepted: false,
            plot: None,
            phase_work: 0,
            bag: None,
            guest_loaded: None,
        }
    }

    #[test]
    fn an_interrupted_meeting_preserves_the_current_edge_then_physically_returns() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let mask = world.phase_mask();
        let now = world.world_ms();
        let p = house(CARPENTER).door;
        let person = &mut life.people[CARPENTER].walk;
        let nearby = event_stands(&world, p)
            .find(|&t| distance(t, p) == 1)
            .unwrap();
        person.redirect(nearby, super::super::State::Outbound, now);
        person.segment.push(nearby);
        person.edge = Some(Edge {
            to: nearby,
            start: now - STEP,
            end: now,
        });
        let mut social = State::default();
        let mut a = appointment(36, CARPENTER, Stage::Meet, now);
        social.return_resident(&mut life, &mut world, mask, now, &mut a);
        assert_eq!(a.stage, Stage::Return);
        assert_eq!(life.people[CARPENTER].walk.edge.unwrap().to, nearby);
        let person = &mut life.people[CARPENTER].walk;
        person.edge = None;
        person.at = nearby;
        social.step(&mut life, &mut world, mask, now, &mut a);
        assert_eq!(
            life.people[CARPENTER].walk.state,
            super::super::State::Leaving
        );
        assert_eq!(life.people[CARPENTER].walk.target, p);
        assert!(!social.completed(36));
    }

    #[test]
    fn same_tool_must_be_owned_by_the_booking_and_must_return() {
        let world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        let mut s = State::default();
        assert!(s.conserved_loan());
        s.tool = ToolPlace::Borrowed(ActorKind::Ayu);
        assert!(!s.conserved_loan());
        let mut a = appointment(37, CARPENTER, Stage::Work, now);
        a.host = Some(ActorKind::Xiaohe);
        s.active = Some(a);
        assert!(!s.conserved_loan());
        s.active.as_mut().unwrap().host = Some(ActorKind::Ayu);
        assert!(s.conserved_loan());
        s.tool = ToolPlace::Returned;
        assert!(!s.conserved_loan());
        let a = s.active.as_mut().unwrap();
        a.stage = Stage::Return;
        a.host = None;
        a.accepted = true;
        assert!(s.valid(&world));
    }

    #[test]
    fn cancelled_loan_keeps_the_original_hammer_until_workshop_arrival() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let now = world.world_ms();
        let mask = world.phase_mask();
        let owner = ActorKind::Ayu;
        let workshop = resolve(mask, "home_workshop.door").unwrap();
        let mut social = State::default();
        let mut visit = appointment(37, CARPENTER, Stage::Work, now);
        visit.host = Some(owner);
        social.tool = ToolPlace::Borrowed(owner);
        assert_ne!(world.actor_tile(owner), workshop);
        social.cancel(&mut life, &mut world, mask, now, &mut visit);
        assert_eq!(visit.stage, Stage::Refund);
        assert_eq!(visit.host, Some(owner));
        social.active = Some(visit);
        let json = serde_json::to_string(&social).unwrap();
        let mut social: State = serde_json::from_str(&json).unwrap();
        assert!(social.valid(&world));
        assert_eq!(
            social.host_held(owner),
            Some(VisitorHeld::Tool(VisitorTool::Hammer))
        );
        assert!(social.host_held(ActorKind::Xiaohe).is_none());
        for _ in 0..45 {
            world.advance_world_ms(MINUTE);
            if world.activity_arrived(owner) {
                break;
            }
        }
        assert_eq!(world.actor_tile(owner), workshop);
        let now = world.world_ms();
        social.cancel(&mut life, &mut world, mask, now, &mut visit);
        assert_eq!(social.tool, ToolPlace::Workshop);
        assert_eq!(visit.stage, Stage::Return);
        assert!(social.host_held(owner).is_none());
        assert!(!social.completed(37));
    }

    #[test]
    fn rescheduled_winter_fitting_never_repairs_without_packed_fibre() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let mask = world.phase_mask();
        let now = world.world_ms();
        let mut story = State::default();
        let mut first = appointment(68, WEAVER, Stage::Meet, now);
        first.host = Some(ActorKind::Ayu);
        let prior_wear = life.services.wearing_status("work_coat").unwrap();
        assert!(prior_wear >= 2);

        assert!(world.reserve_market_trip(ActorKind::Ayu));
        story.meet(&mut life, &mut world, mask, now, &mut first);
        assert_eq!(life.services.wearing_status("work_coat"), Some(prior_wear));
        assert_eq!(story.winter, WinterItem::InUse);

        let material = cargo(Item::Rope, 1);
        let mut first = appointment(68, WEAVER, Stage::Meet, now);
        assert!(world.reserve_market_trip(ActorKind::Xiaohe));
        assert!(world.spend_stock(material.item, material.quantity));
        let supply_after_packing = world.stock().count(material.item);
        first.host = Some(ActorKind::Xiaohe);
        first.input = Some(material);
        first.bag = Some(material);
        story.meet(&mut life, &mut world, mask, now, &mut first);
        assert_eq!(
            life.services.wearing_status("work_coat"),
            Some(prior_wear - 1)
        );
        assert!(first.bag.is_none());
        assert_eq!(story.winter, WinterItem::Patched);
        story.cancel(&mut life, &mut world, mask, now, &mut first);

        let mut replacement = appointment(68, WEAVER, Stage::Meet, now);
        assert!(world.reserve_market_trip(ActorKind::Xiaohe));
        replacement.host = Some(ActorKind::Xiaohe);
        story.meet(&mut life, &mut world, mask, now, &mut replacement);
        assert_eq!(replacement.stage, Stage::Work);
        assert_eq!(
            life.services.wearing_status("work_coat"),
            Some(prior_wear - 1)
        );
        assert_eq!(world.stock().count(material.item), supply_after_packing);
    }

    #[test]
    fn cancelled_gift_is_serialized_and_returned_only_at_the_pantry() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let now = world.world_ms();
        let mask = world.phase_mask();
        let mut social = State::default();
        let packed = cargo(Item::Food, 1);
        let stock = world.stock().count(Item::Food);
        let mut visit = appointment(39, HELPER, Stage::Refund, now);
        visit.input = Some(packed);
        visit.bag = Some(packed);
        assert!(world.spend_stock(packed.item, packed.quantity));
        social.active = Some(visit);
        let bytes = serde_json::to_vec(&social).unwrap();
        let restored: State = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.active.unwrap().bag, Some(packed));
        assert!(restored.valid(&world));
        assert_eq!(world.stock().count(Item::Food), stock - 1);
        let dock = slot(mask, "pantry_loading", "load").unwrap();
        assert!(world.reserve_market_trip(ActorKind::Ayu));
        visit.host = Some(ActorKind::Ayu);
        assert!(world.move_for_activity(ActorKind::Ayu, point(dock)));
        for _ in 0..100 {
            world.advance_world_ms(STEP);
            if world.activity_arrived(ActorKind::Ayu) {
                break;
            }
        }
        assert_eq!(world.actor_tile(ActorKind::Ayu), dock);
        let now = world.world_ms();
        social.cancel(&mut life, &mut world, mask, now, &mut visit);
        assert!(visit.bag.is_none());
        assert_eq!(world.stock().count(Item::Food), stock);
        assert!(!social.completed(39));
    }

    #[test]
    fn batch_limits_and_fixed_stall_gate_require_the_correct_trade() {
        let world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        let day = now / DAY_MS;
        let mut s = State::default();
        s.market.session = Some(MarketSession {
            chapter: 48,
            batch: 1,
            sold: 0,
            opened: false,
            day,
        });
        s.witness_sale(MOBILE, &world, now);
        assert_eq!(s.market.session.unwrap().sold, 0);
        s.market.session.as_mut().unwrap().opened = true;
        s.witness_sale(MOBILE + 1, &world, now);
        assert_eq!(s.market.session.unwrap().sold, 0);
        s.witness_sale(MOBILE, &world, now);
        s.witness_sale(MOBILE, &world, now);
        assert_eq!(s.market.session.unwrap().sold, 1);
        assert!(!s.market_active(day));
        assert!(s.market_shuttered(day));
        assert!(!s.market.fixed_use);
    }

    #[test]
    fn rain_trial_uses_the_weekly_canopy_before_the_permanent_depot_exists() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        assert!(trial_shelter(&world).is_none());
        assert!(world.activate_weekly_market());
        let shelter = trial_shelter(&world).expect("weekly seed stall with an awning");
        assert!(!world.village_building_active("market_depot"));
        let mask = world.phase_mask();
        let now = world.world_ms();
        let mut life = VillageLife::default();
        let mut story = State::default();
        let mut visit = appointment(50, POST, Stage::Meet, now);
        assert!(world.reserve_market_trip(ActorKind::Ayu));
        visit.host = Some(ActorKind::Ayu);
        let water = cargo(Item::Water, 1);
        visit.input = Some(water);
        assert!(world.spend_stock(water.item, water.quantity));
        visit.bag = Some(water);
        life.people[POST].walk.at = shelter.door;
        story.meet(&mut life, &mut world, mask, now, &mut visit);
        assert_eq!(visit.stage, Stage::Work);
        assert_eq!(visit.bag, None);
    }

    #[test]
    fn large_neighbour_reserve_fills_only_from_actual_well_and_one_drawing_per_day() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut story = State::default();
        story.neighbours.prepared = true;
        let house_stock = world.stock().count(Item::Water);
        story.fill_visitor_water(&mut world, TilePos::new(0, 0));
        assert_eq!(story.neighbours.water, 0);
        let well = resolve(world.phase_mask(), "home_well").unwrap();
        story.fill_visitor_water(&mut world, well);
        story.fill_visitor_water(&mut world, well);
        assert_eq!(story.neighbours.water, 1);
        assert_eq!(world.stock().count(Item::Water), house_stock);
    }

    #[test]
    fn new_households_keep_the_compiled_person_bed_and_initial_move_distinct() {
        let mut life = VillageLife::default();
        assert!(life.social.hold_arrival(FARMER));
        assert!(life.social.hold_arrival(TAILOR));
        assert_eq!(house(FARMER).id, house(TAILOR).id);
        let pair = life.residents().skip(FARMER).take(2).collect::<Vec<_>>();
        assert_ne!(pair[0].id, pair[1].id);
        assert_ne!(pair[0].bed, pair[1].bed);
        life.social.complete(62);
        assert!(!life.social.hold_arrival(FARMER));
        assert!(life.social.hold_arrival(TAILOR));
        life.social.complete(63);
        assert!(!life.social.hold_arrival(TAILOR));
    }
}

fn event_stands(world: &LivingWorld, anchor: TilePos) -> impl Iterator<Item = TilePos> + '_ {
    apron_tiles(world, anchor).chain(
        [(0, 1), (-1, 0), (1, 0), (0, -1)]
            .into_iter()
            .filter_map(move |(x, y)| anchor.offset(x, y))
            .filter(|p| {
                !outer_corridor(*p)
                    && world.can_walk(p.x.into(), p.y.into())
                    && !WILLOW_VILLAGE.buildings.iter().any(|b| b.door == *p)
            }),
    )
}

fn resident_for(chapter: u8) -> Option<usize> {
    Some(match chapter {
        36 | 37 | 40 | 41 | 71 => CARPENTER,
        38 | 39 => HELPER,
        46 | 50 | 51 | 69 => POST,
        47 | 66 => SEEDS,
        60 | 67 => COOK,
        61 | 62 => FARMER,
        63 => TAILOR,
        68 => WEAVER,
        _ => return None,
    })
}
