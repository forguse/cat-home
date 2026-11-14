//! Authority for bounded, data-driven outside activities in the household.

use serde::{Deserialize, Serialize};

use crate::{
    living_world::{
        Activity as HouseholdActivity, ActorKind, DAY_MS, Ground, Item, LivingWorld, MAP_HEIGHT,
        MAP_WIDTH, PlotStage, Point, Pose, TILE_PX, YardSeason, YardWeather,
    },
    map_data::WILLOW_VILLAGE,
    plugins::{
        Activity, ActorSelector, ContentId, Facility, Fact, FactSource, FactValue, LocalId,
        MarketSource, Material, PackId, PluginError, PluginStore, SceneOp, SceneStep, Season, Slot,
        ValidatedPack, Weather, Window,
    },
};

const PROBE_MS: u64 = 15_000;
const MAX_WORLD_REFERENCES: usize = 32;
const MAX_COOLDOWNS: usize = 32;
const ORIGIN: Point = Point::at_tile(30, 14);
const BASKETBALL_PACK: &str = "org.cat-home.community.basketball";
const HUMANS: [ActorKind; 2] = [ActorKind::Xiaohe, ActorKind::Ayu];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Cell {
    x: u8,
    y: u8,
}

impl Cell {
    fn at(point: Point) -> Option<Self> {
        if !point.x.is_finite() || !point.y.is_finite() || point.x < 0.0 || point.y < 0.0 {
            return None;
        }
        let x = (point.x / TILE_PX as f32) as u32;
        let y = (point.y / TILE_PX as f32) as u32;
        (x < u32::from(WILLOW_VILLAGE.width) && y < u32::from(WILLOW_VILLAGE.height)).then_some(
            Self {
                x: x as u8,
                y: y as u8,
            },
        )
    }

    fn point(self) -> Point {
        Point::at_tile(self.x.into(), self.y.into())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuiltFacility {
    id: ContentId,
    cell: Cell,
    width: u8,
    height: u8,
}

impl BuiltFacility {
    fn contains(&self, cell: Cell) -> bool {
        (self.cell.x..self.cell.x + self.width).contains(&cell.x)
            && (self.cell.y..self.cell.y + self.height).contains(&cell.y)
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.cell.x < other.cell.x + other.width
            && other.cell.x < self.cell.x + self.width
            && self.cell.y < other.cell.y + other.height
            && other.cell.y < self.cell.y + self.height
    }

    fn slot(&self, slot: Slot) -> Option<Point> {
        let x = i32::from(self.cell.x);
        let y = i32::from(self.cell.y);
        let width = i32::from(self.width);
        let height = i32::from(self.height);
        Some(match slot {
            Slot::CourtA => Point::at_tile(x + width / 4, y + height / 2),
            Slot::CourtB => Point::at_tile(x + 3 * width / 4, y + height / 2),
            Slot::Target => {
                // The target is above the floor in the middle of the facility.
                Point {
                    x: ((x * TILE_PX) + (width * TILE_PX) / 2) as f32,
                    y: ((y * TILE_PX) - 8) as f32,
                }
            }
            _ => return None,
        })
    }

    fn is_clear(&self, world: &LivingWorld) -> bool {
        (self.cell.y..self.cell.y + self.height).all(|y| {
            (self.cell.x..self.cell.x + self.width).all(|x| {
                world.ground(x.into(), y.into()) == Ground::Grass
                    && world.can_walk(x.into(), y.into())
            })
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedProp {
    id: ContentId,
    ground: Cell,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    activity: ContentId,
    accepted_at: u64,
    offered_day: u64,
    last_succeeded: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneState {
    seed: u64,
    built: Vec<BuiltFacility>,
    owned: Vec<OwnedProp>,
    decisions: Vec<Decision>,
    recover: Vec<ActorKind>,
    completed: u64,
    made_contacts: u64,
    #[serde(default)]
    basketball_completed: u64,
    #[serde(default)]
    village: bool,
}

impl Default for SceneState {
    fn default() -> Self {
        Self {
            seed: 0xca7_010e,
            built: Vec::new(),
            owned: Vec::new(),
            decisions: Vec::new(),
            recover: Vec::new(),
            completed: 0,
            made_contacts: 0,
            basketball_completed: 0,
            village: false,
        }
    }
}

impl SceneState {
    pub fn completed(&self) -> u64 {
        self.completed
    }

    pub fn made_contacts(&self) -> u64 {
        self.made_contacts
    }

    pub fn basketball_completed(&self) -> u64 {
        self.basketball_completed
    }

    /// Actors whose external reservation was acquired by this plugin runtime.
    /// Restoration clears this exact set, leaving other scheduler owners alone.
    pub fn reserved_actors(&self) -> &[ActorKind] {
        &self.recover
    }

    fn valid(&self) -> bool {
        if self.built.len() + self.owned.len() > MAX_WORLD_REFERENCES
            || self.decisions.len() > MAX_COOLDOWNS
            || self.recover.len() > HUMANS.len()
            || self.basketball_completed > self.completed
        {
            return false;
        }
        let (width, height) = if self.village {
            (
                usize::from(WILLOW_VILLAGE.width),
                usize::from(WILLOW_VILLAGE.height),
            )
        } else {
            (MAP_WIDTH, MAP_HEIGHT)
        };
        self.built.iter().enumerate().all(|(i, facility)| {
            (1..=12).contains(&facility.width)
                && (1..=8).contains(&facility.height)
                && usize::from(facility.cell.x) + usize::from(facility.width) <= width
                && usize::from(facility.cell.y) + usize::from(facility.height) <= height
                && self.built[..i]
                    .iter()
                    .all(|other| other.id != facility.id && !other.overlaps(facility))
        }) && self.owned.iter().enumerate().all(|(i, prop)| {
            usize::from(prop.ground.x) < width
                && usize::from(prop.ground.y) < height
                && self.owned[..i].iter().all(|other| other.id != prop.id)
        }) && self.decisions.iter().enumerate().all(|(i, decision)| {
            self.decisions[..i]
                .iter()
                .all(|other| other.activity != decision.activity)
        }) && self
            .recover
            .iter()
            .enumerate()
            .all(|(i, actor)| HUMANS.contains(actor) && !self.recover[..i].contains(actor))
    }

    fn decision(&self, id: &ContentId) -> Option<&Decision> {
        self.decisions
            .iter()
            .find(|decision| &decision.activity == id)
    }

    fn facility(&self, id: &ContentId) -> Option<&BuiltFacility> {
        self.built.iter().find(|entry| &entry.id == id)
    }

    fn prop(&self, id: &ContentId) -> Option<&OwnedProp> {
        self.owned.iter().find(|entry| &entry.id == id)
    }

    fn can_add_world_reference(&self) -> bool {
        self.built.len() + self.owned.len() < MAX_WORLD_REFERENCES
    }
}

#[derive(Clone, Debug)]
pub struct FrameCommand {
    pub pack_id: PackId,
    pub frame_id: LocalId,
    pub point: Point,
    pub depth: i32,
    pub actor_override: Option<ActorKind>,
    pub flip: bool,
}

#[derive(Debug)]
pub struct SceneRuntime {
    state: SceneState,
    active: Option<Active>,
    suggestion: Option<ContentId>,
    next_probe: u64,
    weather: Option<Weather>,
    status: &'static str,
}

impl Default for SceneRuntime {
    fn default() -> Self {
        Self {
            state: SceneState::default(),
            active: None,
            suggestion: None,
            next_probe: 0,
            weather: None,
            status: "等待活动条件",
        }
    }
}

#[derive(Debug)]
struct Active {
    pack: PackId,
    activity: ContentId,
    step: LocalId,
    team: Vec<ActorKind>,
    focus: ActorKind,
    facility: Option<ContentId>,
    prop: Option<ContentId>,
    possession: Possession,
    deadline: u64,
    step_started: u64,
    phase: Phase,
    repeats: u8,
    stopping: bool,
    declined: bool,
    contacts: u16,
    effort: u8,
}

#[derive(Clone, Copy, Debug)]
enum Possession {
    Stored,
    Held(ActorKind),
    Bounce(Bounce),
    Loose(Cell),
    Flight(Flight),
}

#[derive(Clone, Copy, Debug)]
struct Flight {
    start: u64,
    contact_at: u64,
    end: u64,
    from: Point,
    ground_y: f32,
    to: Point,
    landing: Cell,
    target: Option<(Point, f32)>,
}

impl Flight {
    fn depth_at(self, now: u64) -> i32 {
        let t = elapsed_fraction(now, self.start, self.end);
        (self.ground_y + (self.landing.point().y - self.ground_y) * t) as i32 + 1
    }

    fn at(self, now: u64) -> Point {
        if now > self.contact_at {
            let t = elapsed_fraction(now, self.contact_at, self.end);
            let landing = self.landing.point();
            return Point {
                x: self.to.x + (landing.x - self.to.x) * t,
                y: self.to.y + (landing.y - self.to.y) * t * t,
            };
        }
        let t = elapsed_fraction(now, self.start, self.contact_at);
        Point {
            x: self.from.x + (self.to.x - self.from.x) * t,
            y: self.from.y + (self.to.y - self.from.y) * t - 20.0 * 4.0 * t * (1.0 - t),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Bounce {
    holder: ActorKind,
    start: u64,
    end: u64,
    depth: u8,
}

fn elapsed_fraction(now: u64, start: u64, end: u64) -> f32 {
    now.saturating_sub(start).min(end - start) as f32 / (end - start).max(1) as f32
}

#[derive(Debug)]
enum Phase {
    Enter,
    Moving {
        kinds: Vec<ActorKind>,
        target: Point,
        preparation: Option<BuiltFacility>,
    },
    Timed {
        until: u64,
        preparation: Option<BuiltFacility>,
    },
    Cleanup {
        until: u64,
        actor: ActorKind,
        target: Point,
        returning: bool,
    },
}

enum StepResult {
    Waiting,
    Success,
    Failure,
    End,
}

impl SceneRuntime {
    pub fn from_state(state: SceneState) -> Result<Self, PluginError> {
        if !state.valid() {
            return Err(PluginError::Invalid("活动存档包含过多或无效的对象".into()));
        }
        Ok(Self {
            state,
            ..Self::default()
        })
    }

    pub fn state(&self) -> &SceneState {
        &self.state
    }

    /// Rebind each supported facility before migrating the household map.
    /// Physical objects and activity totals retain their original identities.
    pub fn migrate_to_village(&mut self, world: &mut LivingWorld) -> Result<(), String> {
        if self.state.village {
            return Ok(());
        }
        let lot = WILLOW_VILLAGE
            .lot("plugin_basketball")
            .ok_or("缺少球场地籍")?;
        if self.state.built.len() > 1
            || self
                .state
                .built
                .iter()
                .any(|facility| !Self::basketball_fits(facility, lot))
        {
            return Err("存在无法匹配新地籍的活动设施".into());
        }
        self.cancel(world);
        for facility in &mut self.state.built {
            facility.cell = Cell {
                x: lot.rect.x as u8,
                y: lot.rect.y as u8,
            };
        }
        let custody = Cell::at(Self::village_origin()).expect("village custody tile");
        for prop in &mut self.state.owned {
            prop.ground = custody;
        }
        self.state.village = true;
        Ok(())
    }

    pub fn validate_for_world(&self, world: &LivingWorld) -> bool {
        self.state.valid()
            && (self.state.village == world.is_village()
                || !self.state.village
                    && self.state.built.is_empty()
                    && self.state.owned.is_empty())
            && self
                .state
                .recover
                .iter()
                .all(|&actor| world.activity_deadline(actor).is_some())
            && self.state.owned.iter().all(|prop| {
                !world.is_village()
                    || world.can_walk(i32::from(prop.ground.x), i32::from(prop.ground.y))
            })
            && (!world.is_village()
                || self.state.built.iter().all(|facility| {
                    WILLOW_VILLAGE.lot("plugin_basketball").is_some_and(|lot| {
                        Self::basketball_fits(facility, lot)
                            && u16::from(facility.cell.x) == lot.rect.x
                            && u16::from(facility.cell.y) == lot.rect.y
                    })
                }))
    }

    fn basketball_fits(facility: &BuiltFacility, lot: &crate::map_data::LotDef) -> bool {
        facility.id.namespace() == BASKETBALL_PACK
            && facility.id.local() == "court"
            && u16::from(facility.width) <= lot.rect.w
            && u16::from(facility.height) <= lot.rect.h
    }

    fn origin(world: &LivingWorld) -> Point {
        if world.is_village() {
            Self::village_origin()
        } else {
            ORIGIN
        }
    }

    fn village_origin() -> Point {
        WILLOW_VILLAGE
            .slot("pantry_loading", "pantry_loading.load")
            .and_then(|slot| match slot.position {
                crate::map_data::SlotPos::Outdoor(tile) => {
                    Some(Point::at_tile(tile.x.into(), tile.y.into()))
                }
                _ => None,
            })
            .expect("compiled household loading station")
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.state.seed = seed;
    }

    pub fn set_weather(&mut self, weather: Weather) {
        self.weather = Some(weather);
    }

    pub fn status(&self) -> &str {
        self.status
    }

    pub fn active_pack(&self) -> Option<&PackId> {
        self.active.as_ref().map(|active| &active.pack)
    }

    pub fn basketball_active(&self) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active.pack.as_str() == BASKETBALL_PACK
                && active.team.len() == 2
                && active.facility.is_some()
                && active.prop.is_some()
                && !active.stopping
        })
    }

    pub fn is_in_use(&self, id: &PackId) -> bool {
        self.active_pack() == Some(id)
            || self
                .state
                .built
                .iter()
                .any(|facility| facility.id.namespace() == id.as_str())
            || self
                .state
                .owned
                .iter()
                .any(|prop| prop.id.namespace() == id.as_str())
    }

    /// Permit an idle pack replacement only when all saved objects still resolve.
    pub fn validate_update(&self, candidate: &ValidatedPack) -> Result<(), PluginError> {
        let id = &candidate.manifest.id;
        if self.active_pack() == Some(id) {
            return Err(PluginError::InUse(id.to_string()));
        }
        for built in &self.state.built {
            if built.id.namespace() == id.as_str()
                && candidate.facility(&built.id).is_none_or(|facility| {
                    facility.width != u16::from(built.width)
                        || facility.height != u16::from(built.height)
                })
            {
                return Err(PluginError::InUse(format!(
                    "{} 的在用设施需要迁移",
                    built.id
                )));
            }
        }
        for owned in &self.state.owned {
            if owned.id.namespace() == id.as_str() && candidate.item(&owned.id).is_none() {
                return Err(PluginError::InUse(format!(
                    "{} 的在用物品需要迁移",
                    owned.id
                )));
            }
        }
        Ok(())
    }

    pub fn suggest(&mut self, id: &ContentId) {
        self.suggestion = Some(id.clone());
        self.next_probe = 0;
    }

    pub fn next_update_ms(&self, world: &LivingWorld) -> Option<u64> {
        let now = world.world_ms();
        if !self.state.recover.is_empty() && self.active.is_none() {
            return Some(now);
        }
        let Some(active) = &self.active else {
            return (self.next_probe != u64::MAX).then_some(self.next_probe.max(now));
        };
        let due = match &active.phase {
            Phase::Enter => now,
            Phase::Moving { kinds, .. } => kinds
                .iter()
                .filter_map(|&kind| actor_deadline(world, kind))
                .min()
                .unwrap_or(now),
            Phase::Timed { until, .. } => *until,
            Phase::Cleanup { until, actor, .. } => {
                actor_deadline(world, *actor).unwrap_or(now).min(*until)
            }
        };
        Some(
            if active.stopping {
                due
            } else {
                due.min(active.deadline)
            }
            .max(now),
        )
    }

    pub fn cancel(&mut self, world: &mut LivingWorld) {
        if let Some(active) = self.active.take() {
            self.persist_prop(&active, world);
        }
        self.release_saved_reservations(world);
        self.next_probe = world.world_ms().saturating_add(PROBE_MS);
        self.status = "活动已结束，居民恢复日常";
    }

    pub fn advance(&mut self, world: &mut LivingWorld, store: &PluginStore) {
        if world.is_village() && self.state.built.is_empty() && self.state.owned.is_empty() {
            self.state.village = true;
        }
        if self.active.is_none() {
            self.release_saved_reservations(world);
        }
        let now = world.world_ms();
        if self.active.is_none() {
            if store.enabled_packs().next().is_none() {
                self.next_probe = u64::MAX;
                self.suggestion = None;
                self.status = "没有已启用的活动插件";
            } else if now >= self.next_probe || self.next_probe == u64::MAX {
                self.next_probe = next_probe(now, store);
                self.propose(world, store);
            }
        }
        let Some(mut active) = self.active.take() else {
            return;
        };
        let Some(record) = store.package(&active.pack) else {
            self.active = Some(active);
            self.cancel(world);
            return;
        };
        let Some(activity) = record
            .pack
            .activities
            .iter()
            .find(|activity| activity.id == active.activity)
        else {
            self.active = Some(active);
            self.cancel(world);
            return;
        };
        let urgent = active.team.iter().any(|&kind| needs_attention(world, kind));
        if !record.enabled || urgent || now >= active.deadline || !dry(self.weather(world)) {
            active.stopping = true;
        }
        for _ in 0..32 {
            if active.stopping {
                if self.cleanup(world, &mut active) {
                    self.finish(world, active, false);
                } else {
                    self.persist_prop(&active, world);
                    self.active = Some(active);
                }
                return;
            }
            let Some(step) = activity.scene.step(&active.step) else {
                self.finish(world, active, false);
                return;
            };
            let result = self.step(world, &record.pack, &mut active, step);
            let next = match result {
                StepResult::Waiting => {
                    self.persist_prop(&active, world);
                    self.active = Some(active);
                    return;
                }
                StepResult::End => {
                    if !matches!(active.possession, Possession::Stored) {
                        active.stopping = true;
                        continue;
                    }
                    let success = !active.declined;
                    self.finish(world, active, success);
                    return;
                }
                StepResult::Failure => {
                    active.declined |= step.op == SceneOp::Invite;
                    step.on_failure.as_ref()
                }
                StepResult::Success => {
                    if let Some(repeat) = &step.repeat
                        && active.repeats < repeat.count
                    {
                        active.repeats += 1;
                        Some(&repeat.target)
                    } else {
                        step.next.as_ref()
                    }
                }
            };
            if let Some(next) = next {
                active.step = next.clone();
                active.step_started = now;
                active.phase = Phase::Enter;
            } else {
                active.stopping = true;
            }
        }
        self.persist_prop(&active, world);
        self.active = Some(active);
    }

    fn release_saved_reservations(&mut self, world: &mut LivingWorld) {
        if !self.state.recover.is_empty() {
            world.recover_activity(&self.state.recover);
            self.state.recover.clear();
        }
    }

    fn propose(&mut self, world: &mut LivingWorld, store: &PluginStore) {
        let now = world.world_ms();
        let day = now / DAY_MS;
        let requested = self.suggestion.take();
        self.status = "居民正忙，或活动条件尚未满足";
        for pack in store.enabled_packs() {
            for activity in &pack.activities {
                let manual = requested.as_ref() == Some(&activity.id);
                if requested.is_some() && (!manual || !activity.allow_user_suggestion) {
                    continue;
                }
                if !window_open(activity.chance.window, world.minute_of_day()) {
                    continue;
                }
                let ready = self.state.decision(&activity.id).is_none_or(|decision| {
                    decision.offered_day != day
                        && now.saturating_sub(decision.accepted_at) >= activity.cooldown_world_ms
                });
                let mut available = HUMANS.into_iter().filter(|&kind| {
                    world.activity_available(kind) && !needs_attention(world, kind)
                });
                let team: Vec<_> = available
                    .by_ref()
                    .take(usize::from(activity.participants.count))
                    .collect();
                let facts = SceneFacts {
                    state: &self.state,
                    world,
                    activity,
                    weather: self.weather(world),
                    team: &team,
                    ready,
                };
                if !ready
                    || team.len() != usize::from(activity.participants.count)
                    || work_due(world)
                    || !activity.when.evaluate(&facts)
                    || !manual && !activity.chance.allows(self.state.seed, &activity.id, day)
                    || !self.can_prepare(world, pack, activity, team[0])
                {
                    continue;
                }
                if self.state.decisions.len() == MAX_COOLDOWNS
                    && self.state.decision(&activity.id).is_none()
                {
                    self.state.decisions.retain(|decision| {
                        now.saturating_sub(decision.accepted_at) <= 32 * DAY_MS
                            || store.enabled_packs().any(|pack| {
                                pack.activities
                                    .iter()
                                    .any(|activity| activity.id == decision.activity)
                            })
                    });
                    if self.state.decisions.len() == MAX_COOLDOWNS {
                        self.status = "活动冷却记录已达到容量";
                        return;
                    }
                }
                if !world.reserve_activity(&team) {
                    continue;
                }
                if let Some(decision) = self
                    .state
                    .decisions
                    .iter_mut()
                    .find(|decision| decision.activity == activity.id)
                {
                    decision.offered_day = day;
                    decision.accepted_at = now;
                } else {
                    self.state.decisions.push(Decision {
                        activity: activity.id.clone(),
                        accepted_at: now,
                        offered_day: day,
                        last_succeeded: None,
                    });
                }
                self.state.recover = team.clone();
                self.active = Some(Active {
                    pack: pack.manifest.id.clone(),
                    activity: activity.id.clone(),
                    step: activity.scene.start.clone(),
                    focus: team[0],
                    team,
                    facility: None,
                    prop: None,
                    possession: Possession::Stored,
                    deadline: now.saturating_add(activity.max_duration_world_ms),
                    step_started: now,
                    phase: Phase::Enter,
                    repeats: 0,
                    stopping: false,
                    declined: false,
                    contacts: 0,
                    effort: 0,
                });
                self.status = "居民正在进行活动";
                return;
            }
        }
    }

    fn can_prepare(
        &self,
        world: &LivingWorld,
        pack: &ValidatedPack,
        activity: &Activity,
        kind: ActorKind,
    ) -> bool {
        activity
            .scene
            .step(&activity.scene.start)
            .is_some_and(|step| match step.op {
                SceneOp::PrepareFacility => step
                    .facility
                    .as_ref()
                    .and_then(|id| {
                        pack.facility(id).map(|facility| {
                            self.state.facility(id).is_some()
                                || (self.state.can_add_world_reference()
                                    && can_pay(world, &facility.materials)
                                    && self.find_site(world, kind, id, facility).is_some())
                        })
                    })
                    .unwrap_or(false),
                SceneOp::OrderItem => {
                    step.prop
                        .as_ref()
                        .and_then(|id| pack.item(id))
                        .is_some_and(|item| {
                            let MarketSource::MarketOrder { materials } = &item.source;
                            self.state.can_add_world_reference() && can_pay(world, materials)
                        })
                }
                _ => true,
            })
    }

    fn step(
        &mut self,
        world: &mut LivingWorld,
        pack: &ValidatedPack,
        active: &mut Active,
        step: &SceneStep,
    ) -> StepResult {
        let phase = std::mem::replace(&mut active.phase, Phase::Enter);
        match phase {
            Phase::Enter => self.enter(world, pack, active, step),
            Phase::Moving {
                kinds,
                target,
                preparation,
            } => {
                if !kinds.iter().all(|&kind| world.activity_arrived(kind)) {
                    active.phase = Phase::Moving {
                        kinds,
                        target,
                        preparation,
                    };
                    return StepResult::Waiting;
                }
                if !kinds
                    .iter()
                    .all(|&kind| near(actor_position(world, kind), target, 3.0))
                {
                    return StepResult::Failure;
                }
                if step.op == SceneOp::MoveTo {
                    return StepResult::Success;
                }
                self.begin_timed(world, active, step, preparation)
            }
            Phase::Timed { until, preparation } => {
                if world.world_ms() < until {
                    active.phase = Phase::Timed { until, preparation };
                    return StepResult::Waiting;
                }
                self.commit_step(world, pack, active, step, preparation)
            }
            Phase::Cleanup { .. } => StepResult::Failure,
        }
    }

    fn enter(
        &mut self,
        world: &mut LivingWorld,
        pack: &ValidatedPack,
        active: &mut Active,
        step: &SceneStep,
    ) -> StepResult {
        use SceneOp::*;
        let Some(kind) = active.actor(step.actor) else {
            return StepResult::Failure;
        };
        match step.op {
            End => return StepResult::End,
            ReserveSlots => {
                let Some(facility) = step
                    .facility
                    .as_ref()
                    .and_then(|id| self.state.facility(id))
                else {
                    return StepResult::Failure;
                };
                if !facility.is_clear(world)
                    || !active
                        .team
                        .iter()
                        .zip([Slot::CourtA, Slot::CourtB])
                        .all(|(&kind, slot)| {
                            facility
                                .slot(slot)
                                .is_some_and(|point| world.can_reach_for_activity(kind, point))
                        })
                {
                    return StepResult::Failure;
                }
                active.facility = Some(facility.id.clone());
                return StepResult::Success;
            }
            PrepareFacility => {
                let Some((id, facility)) = step
                    .facility
                    .as_ref()
                    .and_then(|id| pack.facility(id).map(|facility| (id, facility)))
                else {
                    return StepResult::Failure;
                };
                if self.state.facility(id).is_some() {
                    return StepResult::Success;
                }
                let Some(site) = self.find_site(world, kind, id, facility) else {
                    return StepResult::Failure;
                };
                let target = site.slot(Slot::CourtA).expect("facility slot");
                return self.begin_move(world, active, step, target, Some(site));
            }
            OrderItem => {
                if step
                    .prop
                    .as_ref()
                    .is_some_and(|id| self.state.prop(id).is_some())
                {
                    return StepResult::Success;
                }
                return self.begin_move(world, active, step, Self::origin(world), None);
            }
            MoveTo => {
                let Some(target) = step.slot.and_then(|slot| self.slot(world, active, slot)) else {
                    return StepResult::Failure;
                };
                return self.begin_move(world, active, step, target, None);
            }
            TakeProp => {
                if !matches!(active.possession, Possession::Stored) {
                    return StepResult::Failure;
                }
                let Some(prop) = step.prop.as_ref().and_then(|id| self.state.prop(id)) else {
                    return StepResult::Failure;
                };
                active.prop = Some(prop.id.clone());
                active.possession =
                    if prop.ground == Cell::at(Self::origin(world)).expect("valid origin") {
                        Possession::Stored
                    } else {
                        Possession::Loose(prop.ground)
                    };
                return self.begin_move(world, active, step, prop.ground.point(), None);
            }
            RetrieveProp => {
                if step.prop != active.prop {
                    return StepResult::Failure;
                }
                let Possession::Loose(cell) = active.possession else {
                    return StepResult::Failure;
                };
                return self.begin_move(world, active, step, cell.point(), None);
            }
            PassArc | ThrowArc => {
                if !self.launch(world, pack, active, step, kind) {
                    return StepResult::Failure;
                }
            }
            CatchOrGround => {
                if step.prop != active.prop {
                    return StepResult::Failure;
                }
                let Possession::Flight(flight) = active.possession else {
                    return StepResult::Failure;
                };
                if near(actor_position(world, kind), flight.landing.point(), 12.0) {
                    active.possession = Possession::Held(kind);
                } else {
                    active.possession = Possession::Loose(flight.landing);
                }
            }
            Animate if step.bounce_height > 0 => {
                if !matches!(active.possession, Possession::Held(holder) if holder == kind)
                    || active.prop != step.prop
                {
                    return StepResult::Failure;
                }
                let start = world.world_ms();
                active.possession = Possession::Bounce(Bounce {
                    holder: kind,
                    start,
                    end: start.saturating_add(step.duration_world_ms),
                    depth: step.bounce_height,
                });
            }
            RotateRoles => {
                // Transfer takes place at the catch or retrieval. Retain that same holder.
                return if matches!(active.possession, Possession::Held(_)) {
                    StepResult::Success
                } else {
                    StepResult::Failure
                };
            }
            ReturnProp
                if !matches!(active.possession, Possession::Held(holder) if holder == kind)
                    || step.prop != active.prop
                    || !near(actor_position(world, kind), Self::origin(world), 4.0) =>
            {
                return StepResult::Failure;
            }
            _ => {}
        }
        self.begin_timed(world, active, step, None)
    }

    fn begin_move(
        &mut self,
        world: &mut LivingWorld,
        active: &mut Active,
        step: &SceneStep,
        target: Point,
        preparation: Option<BuiltFacility>,
    ) -> StepResult {
        let kinds = active.actors(step.actor);
        if kinds.is_empty()
            || !kinds.iter().all(|&kind| {
                world.activity_arrived(kind) && world.can_reach_for_activity(kind, target)
            })
        {
            return StepResult::Failure;
        }
        if !kinds.iter().all(|&kind| {
            near(actor_position(world, kind), target, 3.0) || world.move_for_activity(kind, target)
        }) {
            return StepResult::Failure;
        }
        active.phase = Phase::Moving {
            kinds,
            target,
            preparation,
        };
        StepResult::Waiting
    }

    fn begin_timed(
        &mut self,
        world: &mut LivingWorld,
        active: &mut Active,
        step: &SceneStep,
        preparation: Option<BuiltFacility>,
    ) -> StepResult {
        let pose = match step.op {
            SceneOp::PrepareFacility => Pose::Work,
            SceneOp::Rest => Pose::Rest,
            _ => Pose::Idle,
        };
        for kind in active.actors(step.actor) {
            world.external_pose(kind as usize, pose);
        }
        active.step_started = world.world_ms();
        let until = world
            .world_ms()
            .saturating_add(step.duration_world_ms.max(1));
        active.phase = Phase::Timed { until, preparation };
        StepResult::Waiting
    }

    fn commit_step(
        &mut self,
        world: &mut LivingWorld,
        pack: &ValidatedPack,
        active: &mut Active,
        step: &SceneStep,
        preparation: Option<BuiltFacility>,
    ) -> StepResult {
        use SceneOp::*;
        let Some(kind) = active.actor(step.actor) else {
            return StepResult::Failure;
        };
        let success = match step.op {
            Invite => {
                active.team.iter().all(|&kind| {
                    !needs_attention(world, kind) && actor(world, kind).stamina() >= 42
                }) && !work_due(world)
            }
            PrepareFacility => {
                let Some(site) = preparation else {
                    return StepResult::Failure;
                };
                let Some(definition) = pack.facility(&site.id) else {
                    return StepResult::Failure;
                };
                if !self.state.can_add_world_reference()
                    || !site.is_clear(world)
                    || self.state.built.iter().any(|other| other.overlaps(&site))
                    || !pay(world, &definition.materials)
                {
                    return StepResult::Failure;
                }
                self.state.built.push(site);
                active.effort = active.effort.saturating_add(6);
                true
            }
            OrderItem => {
                let Some((id, item)) = step
                    .prop
                    .as_ref()
                    .and_then(|id| pack.item(id).map(|item| (id, item)))
                else {
                    return StepResult::Failure;
                };
                let MarketSource::MarketOrder { materials } = &item.source;
                if !self.state.can_add_world_reference() || !pay(world, materials) {
                    return StepResult::Failure;
                }
                self.state.owned.push(OwnedProp {
                    id: id.clone(),
                    ground: Cell::at(Self::origin(world)).expect("valid origin"),
                });
                true
            }
            TakeProp | RetrieveProp => {
                if let Possession::Held(_) | Possession::Flight(_) | Possession::Bounce(_) =
                    active.possession
                {
                    return StepResult::Failure;
                }
                active.possession = Possession::Held(kind);
                active.focus = kind;
                true
            }
            PassArc => true,
            CatchOrGround => {
                if !matches!(active.possession, Possession::Held(holder) if holder == kind) {
                    return StepResult::Failure;
                }
                active.focus = kind;
                true
            }
            ThrowArc => {
                let Possession::Flight(flight) = active.possession else {
                    return StepResult::Failure;
                };
                let contact = flight.target.is_some_and(|(target, radius)| {
                    near(flight.at(flight.contact_at), target, radius)
                });
                active.possession = Possession::Loose(flight.landing);
                active.effort = active.effort.saturating_add(3);
                if contact {
                    active.contacts += 1;
                    self.state.made_contacts = self.state.made_contacts.saturating_add(1);
                }
                contact
            }
            ReturnProp => {
                active.possession = Possession::Stored;
                true
            }
            Animate if step.bounce_height > 0 => {
                let Possession::Bounce(bounce) = active.possession else {
                    return StepResult::Failure;
                };
                active.possession = Possession::Held(bounce.holder);
                true
            }
            _ => true,
        };
        if success {
            StepResult::Success
        } else {
            StepResult::Failure
        }
    }

    fn launch(
        &self,
        world: &LivingWorld,
        pack: &ValidatedPack,
        active: &mut Active,
        step: &SceneStep,
        kind: ActorKind,
    ) -> bool {
        if !matches!(active.possession, Possession::Held(holder) if holder == kind)
            || active.prop.as_ref() != step.prop.as_ref()
        {
            return false;
        }
        let Some(facility) = active
            .facility
            .as_ref()
            .and_then(|id| self.state.facility(id))
        else {
            return false;
        };
        let from = actor_position(world, kind);
        if Cell::at(from).is_none_or(|cell| !facility.contains(cell)) {
            return false;
        }
        let (to, landing, target) = if step.op == SceneOp::PassArc {
            let Some(other) = active.other(kind) else {
                return false;
            };
            let Some(target) = step.slot.and_then(|slot| facility.slot(slot)) else {
                return false;
            };
            let Some(cell) = Cell::at(target) else {
                return false;
            };
            if !facility.contains(cell) || !world.can_reach_for_activity(other, target) {
                return false;
            }
            (
                Point {
                    x: target.x,
                    y: target.y - 10.0,
                },
                cell,
                None,
            )
        } else {
            let Some(fallback) = facility.slot(Slot::Target) else {
                return false;
            };
            let region = pack.facility(&facility.id).and_then(|definition| {
                let [x, y] = definition.target_offset?;
                Some((
                    Point {
                        x: f32::from(facility.cell.x) * TILE_PX as f32 + f32::from(x),
                        y: f32::from(facility.cell.y) * TILE_PX as f32 + f32::from(y),
                    },
                    f32::from(definition.target_radius?),
                ))
            });
            let (target, radius) = region.unwrap_or((fallback, 3.0));
            // Stable aim variation is expressed in world distance. The resulting arc is
            // tested against the actual target region at its end.
            let aim = ((world.world_ms() / DAY_MS + u64::from(active.repeats) + kind as u64) % 5)
                as i32
                - 2;
            let fatigue = if actor(world, kind).stamina() < 55 {
                2.0
            } else {
                0.0
            };
            let landing = Cell {
                x: facility.cell.x
                    + (i32::from(facility.width) / 2 + aim).clamp(0, i32::from(facility.width) - 1)
                        as u8,
                y: facility.cell.y + facility.height / 4,
            };
            (
                Point {
                    x: target.x + aim as f32 * 2.5 + fatigue,
                    y: target.y,
                },
                landing,
                Some((target, radius)),
            )
        };
        let start = world.world_ms();
        let duration = step.duration_world_ms.max(1);
        let travel = if target.is_some() {
            duration * 3 / 4
        } else {
            duration
        };
        active.possession = Possession::Flight(Flight {
            start,
            contact_at: start.saturating_add(travel.max(1)),
            end: start.saturating_add(duration),
            from: Point {
                x: from.x,
                y: from.y - 10.0,
            },
            ground_y: from.y,
            to,
            landing,
            target,
        });
        active.focus = kind;
        true
    }

    fn find_site(
        &self,
        world: &LivingWorld,
        actor: ActorKind,
        id: &ContentId,
        definition: &Facility,
    ) -> Option<BuiltFacility> {
        let width = definition.width as u8;
        let height = definition.height as u8;
        if width < 2 || height < 2 {
            return None;
        }
        if world.is_village() {
            let lot = WILLOW_VILLAGE.lot("plugin_basketball")?;
            if id.namespace() != BASKETBALL_PACK
                || id.local() != "court"
                || !world.phase_mask().contains(lot.phase)
                || u16::from(width) > lot.rect.w
                || u16::from(height) > lot.rect.h
            {
                return None;
            }
            let site = BuiltFacility {
                id: id.clone(),
                cell: Cell {
                    x: lot.rect.x as u8,
                    y: lot.rect.y as u8,
                },
                width,
                height,
            };
            return (site.is_clear(world)
                && !self.state.built.iter().any(|other| other.overlaps(&site))
                && site
                    .slot(Slot::CourtA)
                    .is_some_and(|p| world.can_reach_for_activity(actor, p)))
            .then_some(site);
        }
        let mut nearest = None;
        for y in 16u8..=32u8.saturating_sub(height) {
            for x in 15u8..=48u8.saturating_sub(width) {
                let distance = u16::from(x.abs_diff(28)) + u16::from(y.abs_diff(18));
                if nearest.as_ref().is_some_and(|(best, _)| distance >= *best) {
                    continue;
                }
                let site = BuiltFacility {
                    id: id.clone(),
                    cell: Cell { x, y },
                    width,
                    height,
                };
                if site.is_clear(world)
                    && !self.state.built.iter().any(|other| other.overlaps(&site))
                    && site
                        .slot(Slot::CourtA)
                        .is_some_and(|slot| world.can_reach_for_activity(actor, slot))
                {
                    nearest = Some((distance, site));
                }
            }
        }
        nearest.map(|(_, site)| site)
    }

    fn slot(&self, world: &LivingWorld, active: &Active, slot: Slot) -> Option<Point> {
        match slot {
            Slot::Origin => Some(Self::origin(world)),
            Slot::PropLocation => match active.possession {
                Possession::Stored => Some(Self::origin(world)),
                Possession::Loose(cell) => Some(cell.point()),
                _ => None,
            },
            _ => active
                .facility
                .as_ref()
                .and_then(|id| self.state.facility(id))
                .and_then(|f| f.slot(slot)),
        }
    }

    fn cleanup(&self, world: &mut LivingWorld, active: &mut Active) -> bool {
        let now = world.world_ms();
        if let Phase::Cleanup {
            until,
            actor,
            target,
            returning,
        } = active.phase
        {
            if now >= until {
                return true;
            }
            if !world.activity_arrived(actor) {
                return false;
            }
            if !near(actor_position(world, actor), target, 3.0) {
                return true;
            }
            if returning {
                active.possession = Possession::Stored;
                return true;
            }
            active.possession = Possession::Held(actor);
            active.phase = Phase::Enter;
        }
        if let Possession::Flight(flight) = active.possession {
            if now < flight.end {
                active.phase = Phase::Timed {
                    until: flight.end,
                    preparation: None,
                };
                return false;
            }
            active.possession = Possession::Loose(flight.landing);
        }
        if let Possession::Bounce(bounce) = active.possession {
            active.possession = Possession::Held(bounce.holder);
        }
        let (kind, target, returning) = match active.possession {
            Possession::Stored => return true,
            Possession::Loose(cell) => (active.focus, cell.point(), false),
            Possession::Held(kind) => (kind, Self::origin(world), true),
            Possession::Flight(_) | Possession::Bounce(_) => unreachable!("settled before cleanup"),
        };
        if needs_attention(world, kind) {
            return true;
        }
        if !world.activity_arrived(kind) {
            if let Some(until) = world.activity_deadline(kind) {
                active.phase = Phase::Timed {
                    until: until.max(now.saturating_add(1)),
                    preparation: None,
                };
                return false;
            }
            return true;
        }
        if !near(actor_position(world, kind), target, 3.0) && !world.move_for_activity(kind, target)
        {
            return true;
        }
        active.phase = Phase::Cleanup {
            until: now.saturating_add(5 * 60_000),
            actor: kind,
            target,
            returning,
        };
        false
    }

    fn persist_prop(&mut self, active: &Active, world: &LivingWorld) {
        let Some(id) = &active.prop else {
            return;
        };
        let Some(prop) = self.state.owned.iter_mut().find(|prop| prop.id == *id) else {
            return;
        };
        let cell = match active.possession {
            Possession::Stored => Cell::at(Self::origin(world)),
            Possession::Held(kind) => Cell::at(actor_position(world, kind)),
            Possession::Bounce(bounce) => Cell::at(actor_position(world, bounce.holder)),
            Possession::Loose(cell) => Some(cell),
            Possession::Flight(flight) => Some(flight.landing),
        };
        if let Some(cell) = cell.filter(|cell| world.can_walk(cell.x.into(), cell.y.into())) {
            prop.ground = cell;
        }
    }

    fn finish(&mut self, world: &mut LivingWorld, active: Active, success: bool) {
        self.persist_prop(&active, world);
        world.end_activity(&active.team, active.effort);
        self.state.recover.clear();
        if let Some(decision) = self
            .state
            .decisions
            .iter_mut()
            .find(|decision| decision.activity == active.activity)
        {
            decision.last_succeeded = Some(success);
        }
        if success {
            self.state.completed = self.state.completed.saturating_add(1);
            if active.pack.as_str() == BASKETBALL_PACK
                && active.team.len() == 2
                && active.contacts > 0
                && active.facility.is_some()
                && active.prop.is_some()
            {
                self.state.basketball_completed = self.state.basketball_completed.saturating_add(1);
            }
            self.status = "活动已完成";
        } else if active.declined {
            self.status = "居民婉拒邀请";
        } else {
            self.status = "活动已安全收尾";
        }
    }

    pub fn visuals(&self, store: &PluginStore, world: &LivingWorld) -> Vec<FrameCommand> {
        let mut commands = Vec::new();
        for facility in &self.state.built {
            let Some(pack_id) = PackId::parse(facility.id.namespace()).ok() else {
                continue;
            };
            let Some(record) = store.package(&pack_id) else {
                continue;
            };
            let Some(definition) = record.pack.facility(&facility.id) else {
                continue;
            };
            let point = Point {
                x: f32::from(facility.cell.x) * TILE_PX as f32,
                y: f32::from(facility.cell.y) * TILE_PX as f32,
            };
            commands.push(command(
                &pack_id,
                &definition.frame,
                point,
                point.y as i32 - 1,
            ));
            for layer in &definition.decorations {
                let point = Point {
                    x: point.x + f32::from(layer.offset[0]),
                    y: point.y + f32::from(layer.offset[1]),
                };
                commands.push(command(
                    &pack_id,
                    &layer.frame,
                    point,
                    point.y as i32 + i32::from(layer.depth_bias),
                ));
            }
        }
        let mut anchors = [None; 2];
        if let Some(active) = &self.active
            && let Some(record) = store.package(&active.pack)
            && let Some(step) = record
                .pack
                .activities
                .iter()
                .find(|activity| activity.id == active.activity)
                .and_then(|activity| activity.scene.step(&active.step))
            && !active.stopping
        {
            for kind in active.actors(step.actor) {
                let clip = if kind == ActorKind::Ayu && !step.alternate_frames.is_empty() {
                    &step.alternate_frames
                } else {
                    &step.frames
                };
                if clip.is_empty() {
                    continue;
                }
                let elapsed = world.world_ms().saturating_sub(active.step_started);
                let duration = step.duration_world_ms.max(30_000);
                let index = (elapsed.saturating_mul(clip.len() as u64) / duration) as usize;
                let frame = &clip[index.min(clip.len() - 1)];
                let position = actor_position(world, kind);
                let flip = actor(world, kind).facing_left;
                let mut overlay = command(&active.pack, frame, position, position.y as i32);
                overlay.actor_override = Some(kind);
                overlay.flip = flip;
                if let Some(frame) = record.pack.frame(frame)
                    && let Some(hand) = frame.hand_anchor
                {
                    let x = f32::from(hand[0]) - f32::from(frame.anchor[0]);
                    anchors[kind as usize] = Some(Point {
                        x: position.x + if flip { -x } else { x },
                        y: position.y + f32::from(hand[1]) - f32::from(frame.anchor[1]),
                    });
                }
                commands.push(overlay);
            }
        }
        for prop in &self.state.owned {
            let Some(pack_id) = PackId::parse(prop.id.namespace()).ok() else {
                continue;
            };
            let Some(record) = store.package(&pack_id) else {
                continue;
            };
            let Some(definition) = record.pack.item(&prop.id) else {
                continue;
            };
            let ground = |point: Point| (point, point.y as i32 + 1);
            let (point, depth) = if let Some(active) = self
                .active
                .as_ref()
                .filter(|active| active.prop.as_ref() == Some(&prop.id))
            {
                match active.possession {
                    Possession::Stored => ground(Self::origin(world)),
                    Possession::Loose(cell) => ground(cell.point()),
                    Possession::Held(kind) => (
                        hand_point(world, &anchors, kind),
                        actor_position(world, kind).y as i32 + 1,
                    ),
                    Possession::Bounce(bounce) => {
                        let mut hand = hand_point(world, &anchors, bounce.holder);
                        let progress = elapsed_fraction(world.world_ms(), bounce.start, bounce.end);
                        let cycle = (progress * 2.0).fract();
                        hand.y += f32::from(bounce.depth) * (1.0 - (1.0 - 2.0 * cycle).abs());
                        (hand, actor_position(world, bounce.holder).y as i32 + 1)
                    }
                    Possession::Flight(flight) => (
                        flight.at(world.world_ms()),
                        flight.depth_at(world.world_ms()),
                    ),
                }
            } else {
                ground(prop.ground.point())
            };
            commands.push(command(&pack_id, &definition.frame, point, depth));
        }
        commands
    }

    fn weather(&self, world: &LivingWorld) -> Weather {
        self.weather.unwrap_or_else(|| match world.weather() {
            YardWeather::Clear => Weather::Clear,
            YardWeather::Clouds => Weather::Clouds,
            YardWeather::Rain => Weather::Rain,
            YardWeather::Snow => Weather::Snow,
        })
    }
}

impl Active {
    fn other(&self, kind: ActorKind) -> Option<ActorKind> {
        self.team
            .iter()
            .copied()
            .find(|&candidate| candidate != kind)
    }

    fn actor(&self, selector: Option<ActorSelector>) -> Option<ActorKind> {
        match selector.unwrap_or(ActorSelector::Initiator) {
            ActorSelector::Initiator | ActorSelector::Both => self.team.first().copied(),
            ActorSelector::Partner => self.team.get(1).copied(),
            ActorSelector::Holder => Some(match self.possession {
                Possession::Held(kind) => kind,
                Possession::Bounce(bounce) => bounce.holder,
                _ => self.focus,
            }),
            ActorSelector::Other => self.other(self.focus),
        }
    }

    fn actors(&self, selector: Option<ActorSelector>) -> Vec<ActorKind> {
        if selector == Some(ActorSelector::Both) {
            self.team.clone()
        } else {
            self.actor(selector).into_iter().collect()
        }
    }
}

struct SceneFacts<'a> {
    state: &'a SceneState,
    world: &'a LivingWorld,
    activity: &'a Activity,
    weather: Weather,
    team: &'a [ActorKind],
    ready: bool,
}

impl FactSource for SceneFacts<'_> {
    fn read(&self, fact: &Fact) -> Option<FactValue> {
        let result = match fact {
            Fact::WorldHour => return Some(FactValue::Number(self.world.minute_of_day() / 60)),
            Fact::WorldMinute => return Some(FactValue::Number(self.world.minute_of_day())),
            Fact::Season => {
                return Some(FactValue::Season(match self.world.season() {
                    YardSeason::Spring => Season::Spring,
                    YardSeason::Summer => Season::Summer,
                    YardSeason::Autumn => Season::Autumn,
                    YardSeason::Winter => Season::Winter,
                }));
            }
            Fact::Weather => return Some(FactValue::Weather(self.weather)),
            Fact::SurfaceDry => dry(self.weather),
            Fact::AllAvailable => self.team.len() == usize::from(self.activity.participants.count),
            Fact::UrgentNeeds => self
                .team
                .iter()
                .any(|&kind| needs_attention(self.world, kind)),
            Fact::Willing => self
                .team
                .iter()
                .all(|&kind| actor(self.world, kind).stamina() >= 42),
            Fact::EssentialWorkDue => work_due(self.world),
            Fact::FacilityUsable(id) => self.state.facility(id).is_some_and(|facility| {
                facility.is_clear(self.world)
                    && self.team.iter().all(|&kind| {
                        facility
                            .slot(Slot::CourtA)
                            .is_some_and(|point| self.world.can_reach_for_activity(kind, point))
                    })
            }),
            Fact::ItemObtainable(id) => {
                self.state.prop(id).is_some_and(|prop| {
                    self.team.first().is_some_and(|&kind| {
                        self.world.can_reach_for_activity(kind, prop.ground.point())
                    })
                }) || material_kind(id).is_some_and(|item| self.world.stock().count(item) > 0)
            }
            Fact::CooldownReady => self.ready,
            Fact::LastSucceeded => {
                return self
                    .state
                    .decision(&self.activity.id)?
                    .last_succeeded
                    .map(FactValue::Bool);
            }
            Fact::HasWish => self
                .team
                .iter()
                .all(|&kind| actor(self.world, kind).stamina() >= 42),
        };
        Some(FactValue::Bool(result))
    }
}

fn dry(weather: Weather) -> bool {
    !matches!(weather, Weather::Rain | Weather::Snow)
}

fn window_open(window: Window, minute: u16) -> bool {
    match window {
        Window::WorldDay => true,
        Window::WorldDayAfternoon => (12 * 60..18 * 60).contains(&minute),
    }
}

fn next_probe(now: u64, store: &PluginStore) -> u64 {
    if store
        .enabled_packs()
        .flat_map(|pack| &pack.activities)
        .any(|activity| activity.chance.window == Window::WorldDay)
    {
        return now.saturating_add(PROBE_MS);
    }
    let day = now / DAY_MS;
    let in_day = now % DAY_MS;
    let afternoon = 12 * 60 * 60 * 1000;
    let evening = 18 * 60 * 60 * 1000;
    if in_day < afternoon {
        day.saturating_mul(DAY_MS).saturating_add(afternoon)
    } else if in_day >= evening {
        day.saturating_add(1)
            .saturating_mul(DAY_MS)
            .saturating_add(afternoon)
    } else {
        now.saturating_add(PROBE_MS)
    }
}

fn actor(world: &LivingWorld, kind: ActorKind) -> &crate::living_world::Actor {
    &world.actors()[kind as usize]
}

fn actor_position(world: &LivingWorld, kind: ActorKind) -> Point {
    actor(world, kind).position
}

fn hand_point(world: &LivingWorld, anchors: &[Option<Point>; 2], kind: ActorKind) -> Point {
    anchors[kind as usize].unwrap_or_else(|| {
        let actor = actor(world, kind);
        Point {
            x: actor.position.x + if actor.facing_left { -4.0 } else { 4.0 },
            y: actor.position.y - 8.0,
        }
    })
}

fn actor_deadline(world: &LivingWorld, kind: ActorKind) -> Option<u64> {
    if world.activity_arrived(kind) {
        None
    } else {
        world.activity_deadline(kind)
    }
}

fn near(a: Point, b: Point, distance: f32) -> bool {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy <= distance * distance
}

fn needs_attention(world: &LivingWorld, kind: ActorKind) -> bool {
    let actor = actor(world, kind);
    actor.hunger() >= 70 || actor.thirst() >= 72 || actor.stamina() < 35
}

fn work_due(world: &LivingWorld) -> bool {
    world.actors().iter().any(|actor| {
        matches!(
            actor.activity,
            HouseholdActivity::Harvest | HouseholdActivity::FellTree | HouseholdActivity::Build
        )
    }) || (world
        .plots()
        .iter()
        .any(|plot| plot.stage == PlotStage::Mature)
        && world.stock().count(Item::Food) < 3)
}

fn material_kind(id: &ContentId) -> Option<Item> {
    if id.namespace() != "core" {
        return None;
    }
    Some(match id.local() {
        "grain" => Item::Crops,
        "wood" => Item::Wood,
        "stone" => Item::Stone,
        "rope" => Item::Rope,
        _ => return None,
    })
}

fn can_pay(world: &LivingWorld, materials: &[Material]) -> bool {
    materials.iter().all(|material| {
        material_kind(&material.item)
            .is_some_and(|item| world.stock().count(item) >= material.quantity)
    })
}

fn pay(world: &mut LivingWorld, materials: &[Material]) -> bool {
    materials
        .iter()
        .map(|material| Some((material_kind(&material.item)?, material.quantity)))
        .collect::<Option<Vec<_>>>()
        .is_some_and(|costs| world.spend_stock_bundle(&costs))
}

fn command(pack: &PackId, frame: &LocalId, point: Point, depth: i32) -> FrameCommand {
    FrameCommand {
        pack_id: pack.clone(),
        frame_id: frame.clone(),
        point,
        depth,
        actor_override: None,
        flip: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::HostConfig;
    use crate::visitors::VisitorState;
    use std::{
        path::Path,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn test_store() -> (PluginStore, PackId) {
        let host = HostConfig {
            capabilities: [
                SceneOp::PrepareFacility,
                SceneOp::OrderItem,
                SceneOp::ReserveSlots,
                SceneOp::Invite,
                SceneOp::MoveTo,
                SceneOp::TakeProp,
                SceneOp::Animate,
                SceneOp::PassArc,
                SceneOp::ThrowArc,
                SceneOp::CatchOrGround,
                SceneOp::RotateRoles,
                SceneOp::Rest,
            ]
            .iter()
            .filter_map(|op| op.capability())
            .map(str::to_owned)
            .collect(),
            core_items: ["core:grain", "core:wood", "core:stone", "core:rope"]
                .iter()
                .map(|id| id.parse().unwrap())
                .collect(),
            ..HostConfig::default()
        };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let unique = format!(
            "target/cat-home-scene-test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        );
        let mut store = PluginStore::open(root.join(unique), host).unwrap();
        let id = store
            .install(&root.join("content-packs/basketball/庭院篮球.catpack"))
            .unwrap();
        (store, id)
    }

    fn advance_next(world: &mut LivingWorld, scenes: &mut SceneRuntime, store: &PluginStore) {
        let now = world.world_ms();
        let target = world
            .next_update_ms()
            .min(scenes.next_update_ms(world).unwrap_or(u64::MAX))
            .max(now + 1);
        world.advance_world_ms(target - now);
        scenes.advance(world, store);
        assert!(
            world.world_ms() > now,
            "world failed to advance: {now} -> {target}, scene {:?}",
            scenes.active
        );
    }

    #[test]
    fn village_migration_preserves_the_ball_and_court_without_purchasing_again() {
        let mut world = LivingWorld::new_wall_ms(0);
        assert!(world.reserve_activity(&[ActorKind::Xiaohe]));
        let court = ContentId::parse("org.cat-home.community.basketball:court").unwrap();
        let ball = ContentId::parse("org.cat-home.community.basketball:ball").unwrap();
        let mut scenes = SceneRuntime::default();
        scenes.state.built.push(BuiltFacility {
            id: court.clone(),
            cell: Cell { x: 26, y: 17 },
            width: 12,
            height: 8,
        });
        scenes.state.owned.push(OwnedProp {
            id: ball.clone(),
            ground: Cell { x: 29, y: 23 },
        });
        scenes.state.recover.push(ActorKind::Xiaohe);
        scenes.state.completed = 17;
        let stock = world.stock().clone();
        scenes.migrate_to_village(&mut world).unwrap();
        scenes.migrate_to_village(&mut world).unwrap();
        let new_world = world.migrate_to_village().unwrap();
        assert!(scenes.validate_for_world(&new_world));
        assert_eq!(*new_world.stock(), stock);
        assert_eq!(scenes.state.completed, 17);
        assert_eq!(scenes.state.owned.len(), 1);
        assert_eq!(
            scenes.state.prop(&ball).unwrap().ground.point(),
            SceneRuntime::village_origin()
        );
        assert_eq!(
            scenes.state.facility(&court).unwrap().cell,
            Cell { x: 80, y: 70 }
        );
        assert!(scenes.state.recover.is_empty());
        assert_eq!(new_world.activity_deadline(ActorKind::Xiaohe), None);
    }

    #[test]
    fn bundled_activity_prepares_and_plays_autonomously_with_real_stock() {
        let (store, id) = test_store();
        let mut world = LivingWorld::new_wall_ms(0);
        let mut scenes = SceneRuntime::default();
        let mut saw_held = false;
        let mut saw_flight = false;
        let mut saw_return = false;
        let mut holders = [false; 2];
        let mut shooters = [false; 2];
        let mut saw_bounce = false;
        let mut visit_counts = std::collections::BTreeMap::new();
        let mut previous_active = false;
        let mut previous_step = None;
        let mut previously_built = false;
        let mut previously_owned = false;
        while world.world_ms() < 75 * DAY_MS {
            let now = world.world_ms();
            let target = world
                .next_update_ms()
                .min(scenes.next_update_ms(&world).unwrap_or(u64::MAX))
                .max(now + 1);
            world.advance_world_ms(target - now);
            let stock_before_scene = world.stock().clone();
            scenes.advance(&mut world, &store);
            if !previously_built && !scenes.state.built.is_empty() {
                assert_eq!(
                    world.stock().count(Item::Wood) + 6,
                    stock_before_scene.count(Item::Wood)
                );
                assert_eq!(
                    world.stock().count(Item::Stone) + 2,
                    stock_before_scene.count(Item::Stone)
                );
                assert_eq!(
                    world.stock().count(Item::Rope) + 1,
                    stock_before_scene.count(Item::Rope)
                );
                previously_built = true;
            }
            if !previously_owned && !scenes.state.owned.is_empty() {
                assert_eq!(
                    world.stock().count(Item::Wood) + 2,
                    stock_before_scene.count(Item::Wood)
                );
                assert_eq!(
                    world.stock().count(Item::Crops) + 6,
                    stock_before_scene.count(Item::Crops)
                );
                previously_owned = true;
            }
            if let Some(active) = &scenes.active {
                if previous_step.as_ref() != Some(&active.step) {
                    *visit_counts.entry(active.step.to_string()).or_insert(0) += 1;
                    previous_step = Some(active.step.clone());
                }
                previous_active = active.prop.is_some();
                match active.possession {
                    Possession::Held(kind) => {
                        saw_held = true;
                        holders[kind as usize] = true;
                    }
                    Possession::Flight(flight) => {
                        saw_flight = true;
                        if flight.target.is_some() {
                            shooters[active.focus as usize] = true;
                        }
                    }
                    Possession::Bounce(_) => saw_bounce = true,
                    _ => {}
                }
            } else if previous_active
                && scenes
                    .state
                    .owned
                    .iter()
                    .any(|prop| prop.ground.point() == ORIGIN)
            {
                saw_return = true;
                previous_step = None;
                if scenes.state.basketball_completed() > 0 && shooters == [true; 2] && saw_bounce {
                    break;
                }
            }
        }
        let test_pack = store.package(&id).unwrap();
        let site = scenes.find_site(
            &world,
            HUMANS[0],
            &ContentId::new(&id, &test_pack.pack.facilities[0].id),
            &test_pack.pack.facilities[0],
        );
        assert_eq!(
            scenes.state.built.len(),
            1,
            "site {site:?}, stock {:?}, state {:?}, status {}",
            world.stock(),
            scenes.state,
            scenes.status()
        );
        assert_eq!(
            scenes.state.owned.len(),
            1,
            "world {:?}, state {:?}, status {}",
            world.stock(),
            scenes.state,
            scenes.status()
        );
        assert!(
            saw_held && saw_flight && saw_return && holders == [true; 2],
            "elapsed {} days; held {saw_held}, flight {saw_flight}, returned {saw_return}, holders {holders:?}, state {:?}",
            world.world_ms() / DAY_MS,
            scenes.state
        );
        assert!(
            scenes.state.made_contacts() > 0,
            "{visit_counts:?}, shooters {shooters:?}, bounced {saw_bounce}, {:?}",
            scenes.state
        );
        assert_eq!(shooters, [true; 2], "{visit_counts:?}");
        assert!(saw_bounce);
        assert!(scenes.state.basketball_completed() > 0);
        assert!(previously_built && previously_owned);
        assert!(scenes.is_in_use(&id));
        assert!(world.validate());
    }

    #[test]
    fn disabling_in_flight_preserves_one_prop_and_releases_reservations() {
        let (mut store, id) = test_store();
        let mut world = LivingWorld::new_wall_ms(0);
        let mut scenes = SceneRuntime::default();
        while world.world_ms() < 75 * DAY_MS {
            advance_next(&mut world, &mut scenes, &store);
            if scenes
                .active
                .as_ref()
                .is_some_and(|active| matches!(active.possession, Possession::Flight(_)))
            {
                break;
            }
        }
        assert!(
            scenes
                .active
                .as_ref()
                .is_some_and(|active| matches!(active.possession, Possession::Flight(_)))
        );
        let persisted = serde_json::to_vec(scenes.state()).unwrap();
        let mut recovered =
            SceneRuntime::from_state(serde_json::from_slice(&persisted).unwrap()).unwrap();
        let mut recovered_world = world.clone();
        recovered.cancel(&mut recovered_world);
        assert_eq!(recovered.state.owned.len(), 1);
        assert!(recovered.state.recover.is_empty());
        assert!(recovered_world.validate());

        store.set_enabled(&id, false).unwrap();
        while scenes.active.is_some() && world.world_ms() < 76 * DAY_MS {
            advance_next(&mut world, &mut scenes, &store);
        }
        assert!(scenes.active.is_none());
        assert_eq!(scenes.state.owned.len(), 1);
        assert!(scenes.state.recover.is_empty());
        assert!(matches!(
            store.uninstall(&id, scenes.is_in_use(&id)),
            Err(PluginError::InUse(_))
        ));
        assert!(world.validate());
    }

    #[test]
    fn restoring_preparation_releases_only_its_own_actor_and_visitors_can_finish() {
        let (store, _) = test_store();
        let mut world = LivingWorld::new_wall_ms(0);
        let mut scenes = SceneRuntime::default();
        let mut visitors = VisitorState::default();
        let (plugin_actor, visitor_actor) = loop {
            world.advance_pending_to(world.world_ms());
            visitors.advance(&mut world);
            // Establish the ownership collision under test explicitly. The
            // separate autonomous-play test covers the preparation scheduler;
            // its one-off setup need not coincidentally overlap this visitor.
            if let Some(visitor) = visitors.meeting_partner() {
                let plugin = if visitor == ActorKind::Xiaohe { ActorKind::Ayu } else { ActorKind::Xiaohe };
                if world.reserve_activity(&[plugin]) {
                    scenes.state.recover.push(plugin);
                    assert!(scenes.validate_for_world(&world));
                    assert!(visitors.validate_for_world(&world));
                    assert_ne!(plugin, visitor);
                    break (plugin, visitor);
                }
            }
            let now = world.world_ms();
            assert!(
                now < 40 * DAY_MS,
                "the household never ran both appointments at once"
            );
            let next = world
                .next_update_ms()
                .min(visitors.next_update_ms(&world).unwrap_or(u64::MAX))
                .max(now + 1);
            world.advance_world_ms(next - now);
        };

        let data = serde_json::to_vec(&(world, scenes.state(), visitors)).unwrap();
        let (mut world, state, mut visitors): (LivingWorld, SceneState, VisitorState) =
            serde_json::from_slice(&data).unwrap();
        let mut scenes = SceneRuntime::from_state(state).unwrap();
        assert!(visitors.validate_for_world(&world));
        let visitor_deadline = world
            .activity_deadline(visitor_actor)
            .expect("saved visitor owns actor");
        scenes.advance(&mut world, &store);
        assert_eq!(
            world.activity_deadline(visitor_actor),
            Some(visitor_deadline)
        );
        assert_eq!(world.activity_deadline(plugin_actor), None);
        assert!(scenes.state().reserved_actors().is_empty());
        assert!(visitors.validate_for_world(&world));

        let first_trades = visitors.trades();
        let end = world.world_ms() + DAY_MS;
        while visitors.trades() == first_trades && world.world_ms() < end {
            let now = world.world_ms();
            let next = world
                .next_update_ms()
                .min(visitors.next_update_ms(&world).unwrap_or(u64::MAX))
                .min(scenes.next_update_ms(&world).unwrap_or(u64::MAX))
                .max(now + 1);
            world.advance_world_ms(next - now);
            visitors.advance(&mut world);
            scenes.advance(&mut world, &store);
        }
        assert!(visitors.trades() > first_trades);
        assert!(visitors.validate_for_world(&world));
    }
}
