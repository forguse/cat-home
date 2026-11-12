use std::time::{SystemTime, UNIX_EPOCH};

use crate::activities::{self, Activity, Motion, Prop};
use crate::cast::{self, EffectKind, Role};
use crate::ecology::{self, Event, Kind, Plot, Stage};
use crate::effects::Effects;
use crate::fishing::{self, Moment, Phase};
use crate::projects::{self, LOTS, Project};
use crate::renderer::{DETAIL_BYTES, Damage, DetailSprite};
use crate::simulation::{CareAction, CatAction, DayPart, Season, Snapshot, Visitor, Weather};

const WIDTH: i32 = 160;
const HEIGHT: i32 = 100;
const FRAME_BYTES: usize = (WIDTH * HEIGHT * 4) as usize;
const WIDE_AT: i32 = 85;
const TALL_AT: i32 = 90;
const TILE_BYTES: usize = 16 * 16 * 4;
const TERRAIN: &[u8] = include_bytes!("../assets/world/terrain.bgra");
const SPRITE_W: usize = 48;
const SPRITE_H: usize = 32;
const SPRITE_BYTES: usize = SPRITE_W * SPRITE_H * 4;
const OUTFIT_FRAMES: usize = 3;
const SCENES: &[u8] = include_bytes!("../assets/world/scenes.bgra");
const BUILDS: &[u8] = include_bytes!("../assets/world/builds.bgra");
const CONSTRUCTION: &[u8] = include_bytes!("../assets/world/construction.bgra");
const CAT_IDLE: &[u8] = include_bytes!("../assets/world/cat_idle.bgra");
const CAT_SLEEP: &[u8] = include_bytes!("../assets/world/cat_sleep.bgra");
const CAT_WALK: &[u8] = include_bytes!("../assets/world/cat_walk.bgra");
const CAT_EAT: &[u8] = include_bytes!("../assets/world/cat_eat.bgra");
const CAT_TREAT: &[u8] = include_bytes!("../assets/world/cat_treat.bgra");
const KEEPER: &[u8] = include_bytes!("../assets/world/keeper.bgra");
const SPARROW: &[u8] = include_bytes!("../assets/world/sparrow.bgra");
const GREY_CAT: &[u8] = include_bytes!("../assets/world/grey_cat.bgra");
const BLACK_CAT: &[u8] = include_bytes!("../assets/world/black_cat.bgra");
const SQUIRREL: &[u8] = include_bytes!("../assets/world/squirrel.bgra");
const HEDGEHOG: &[u8] = include_bytes!("../assets/world/hedgehog.bgra");
const RABBIT: &[u8] = include_bytes!("../assets/world/rabbit.bgra");
const FROG: &[u8] = include_bytes!("../assets/world/frog.bgra");
const EXTRA_CAST: &[u8] = include_bytes!("../assets/world/extra-cast.bgra");
const ACTION_ANIMATIONS: &[u8] = include_bytes!("../assets/world/action-animation.bgra");
const BUILDING_STAGES: &[u8] = include_bytes!("../assets/world/construction-progress.bgra");
const STAGE_BYTES: usize = 48 * 40 * 4;

#[repr(C)]
#[derive(Default)]
struct LocalTime {
    year: u16,
    month: u16,
    weekday: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLocalTime(result: *mut LocalTime);
}

pub fn unix_millis() -> u64 {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(value.as_millis()).unwrap_or(u64::MAX)
}

pub fn local_seconds() -> u64 {
    let mut time = LocalTime::default();
    unsafe { GetLocalTime(&mut time) };
    u64::from(time.hour) * 3_600 + u64::from(time.minute) * 60 + u64::from(time.second)
}

#[derive(Clone, Copy, Debug)]
pub struct ManualActions {
    pub water_bowl: bool,
    pub lantern: Option<bool>,
    pub keeper_called_at: Option<u64>,
    pub visitor_snack_at: Option<u64>,
    pub invited_visitor: Visitor,
    pub invited_role: Option<Role>,
    pub visitor_invited_at: Option<u64>,
    pub window_tapped_at: Option<u64>,
}

impl Default for ManualActions {
    fn default() -> Self {
        Self {
            water_bowl: true,
            lantern: None,
            keeper_called_at: None,
            visitor_snack_at: None,
            invited_visitor: Visitor::None,
            invited_role: None,
            visitor_invited_at: None,
            window_tapped_at: None,
        }
    }
}

fn recent(time: Option<u64>, now: u64, seconds: u64) -> bool {
    time.and_then(|time| now.checked_sub(time))
        .is_some_and(|age| age < seconds)
}

fn cat_heard_window(state: &Snapshot, now: u64, actions: &ManualActions) -> bool {
    matches!(state.cat, CatAction::Idle | CatAction::Walk)
        && recent(actions.window_tapped_at, now, 5)
}

pub fn observe(mut state: Snapshot, now: u64, actions: &ManualActions) -> Snapshot {
    if recent(actions.keeper_called_at, now, 50)
        && matches!(state.caretaker, CareAction::Away | CareAction::Resting)
    {
        state.caretaker = CareAction::Tending;
    }
    if actions.invited_visitor != Visitor::None && recent(actions.visitor_invited_at, now, 90) {
        state.visitor = actions.invited_visitor;
    }
    state
}

pub fn check_assets() -> bool {
    fn valid(bytes: &[u8], frame_bytes: usize, min_frames: usize) -> bool {
        bytes.len() >= frame_bytes * min_frames
            && bytes.len().is_multiple_of(frame_bytes)
            && bytes
                .chunks_exact(4)
                .all(|p| p[..3].iter().all(|channel| *channel <= p[3]))
    }
    SCENES.len() == FRAME_BYTES * 4
        && BUILDS.len() == FRAME_BYTES * 4
        && valid(SCENES, FRAME_BYTES, 4)
        && valid(BUILDS, FRAME_BYTES, 4)
        && CONSTRUCTION.len() == FRAME_BYTES * 9
        && valid(CONSTRUCTION, FRAME_BYTES, 9)
        && TERRAIN.len() == TILE_BYTES * 16
        && valid(TERRAIN, TILE_BYTES, 16)
        && KEEPER.len() == SPRITE_BYTES * OUTFIT_FRAMES * 5
        && EXTRA_CAST.len() == SPRITE_BYTES * 3 * 21
        && valid(EXTRA_CAST, SPRITE_BYTES, 3 * 21)
        && ACTION_ANIMATIONS.len() == DETAIL_BYTES * 30 * 6 * 6
        && valid(ACTION_ANIMATIONS, DETAIL_BYTES, 30 * 6 * 6)
        && BUILDING_STAGES.len() == STAGE_BYTES * 12 * 12
        && valid(BUILDING_STAGES, STAGE_BYTES, 12 * 12)
        && [
            CAT_IDLE, CAT_SLEEP, CAT_WALK, CAT_EAT, CAT_TREAT, KEEPER, SPARROW, GREY_CAT,
            BLACK_CAT, SQUIRREL, HEDGEHOG, RABBIT, FROG,
        ]
        .iter()
        .all(|bytes| valid(bytes, SPRITE_BYTES, 2))
}

pub fn redraw_ms(state: &Snapshot) -> u32 {
    if matches!(
        state.cat,
        CatAction::Walk | CatAction::Treat | CatAction::Eat
    ) {
        return 170;
    }
    if matches!(state.weather, Weather::Rain | Weather::Snow) {
        return 260;
    }
    if state.visitor != Visitor::None
        || matches!(state.caretaker, CareAction::Building | CareAction::Feeding)
    {
        return 470;
    }
    900
}

pub fn status_line(state: &Snapshot) -> String {
    let time = match state.day_part {
        DayPart::Dawn => "清晨",
        DayPart::Day => "白天",
        DayPart::Dusk => "黄昏",
        DayPart::Night => "夜晚",
    };
    let weather = weather_label(state.weather);
    let activity = match state.caretaker {
        CareAction::Away => "管理员外出",
        CareAction::Feeding => "管理员准备猫粮",
        CareAction::Tending => "管理员打理院子",
        CareAction::Building => "管理员正在施工",
        CareAction::Resting => "管理员休息",
    };
    let cat = match state.cat {
        CatAction::Idle => "猫咪在院里",
        CatAction::Sleep if state.day_part == DayPart::Night => "猫咪在屋里睡觉",
        CatAction::Sleep if (780..815).contains(&state.minutes_of_day) => "猫咪在院里午睡",
        CatAction::Sleep => "猫咪睡着了",
        CatAction::Walk => "猫咪散步",
        CatAction::Eat => "猫咪吃饭",
        CatAction::Treat => "猫咪吃零食",
    };
    let guest = match state.visitor {
        Visitor::None => "",
        Visitor::Sparrow => " · 小麻雀来串门",
        Visitor::GreyCat => " · 灰猫来串门",
        Visitor::BlackCat => " · 黑猫来串门",
        Visitor::Squirrel => " · 松鼠来串门",
        Visitor::Hedgehog => " · 刺猬来串门",
        Visitor::Rabbit => " · 兔子来串门",
        Visitor::Frog => " · 青蛙来串门",
    };
    let progress = match state.build_stage {
        0 => format!("花圃 {}%", state.build_percent),
        1 => format!("工作台 {}%", state.build_percent),
        2 => format!("凉亭 {}%", state.build_percent),
        _ => "庭院已建好".to_owned(),
    };
    format!("{time} · {weather} · {cat} · {activity} · {progress}{guest}")
}

pub fn weather_label(weather: Weather) -> &'static str {
    match weather {
        Weather::Clear => "晴",
        Weather::Clouds => "多云",
        Weather::Rain => "小雨",
        Weather::Snow => "小雪",
    }
}

pub fn notebook(
    state: &Snapshot,
    size: (i32, i32),
    unlocks: &[u64; 16],
    projects: &[Project; LOTS],
    effects: &Effects,
) -> String {
    let active = activities::current(state);
    let garden = garden_at(state, size, unlocks, effects);
    let job = plot_job(state, &garden);
    let fishing = fishing::moment(state);
    let mut status = *state;
    if job.is_some() || fishing.is_some() {
        status.caretaker = CareAction::Tending;
    }
    let mut text = status_line(&status).replace(" · ", "\n");
    for (label, activity) in [
        ("猫咪", active.cat),
        ("管理员", active.keeper),
        ("来客", active.visitor),
        ("庭院", active.yard),
    ] {
        if let Some(activity) = activity {
            text.push_str(&format!("\n\n{label}正在{}", activity.name));
        }
    }
    if state.build_stage < 3 {
        text.push_str("\n\n施工时间  08:10–08:18  ·  17:05–17:15");
    }
    text.push_str(&format!("\n\n当前可见地块 {}", garden.plots().len()));
    for plot in garden.plots() {
        text.push_str(&format!(
            "\n  {}  {}  {}",
            plot.site_label(),
            plot.kind.label(),
            plot.stage.label()
        ));
        if plot.event != Event::None {
            text.push_str(&format!(" · {}", plot.event.label()));
        }
        if plot.id >= 12 && matches!(plot.stage, Stage::Groundwork | Stage::Framing) {
            text.push_str(&format!(" {}%", plot.progress));
        }
        if let Some(entry) = activities::ecology_entry(plot) {
            text.push_str(&format!(" · {entry}"));
        }
    }
    if let Some(plot) = job {
        text.push_str(&format!(
            "\n管理员在第 {} 块地{} {}",
            plot.id + 1,
            plot.job_label(),
            plot.kind.label()
        ));
    }
    let outfit = if fishing.is_some() {
        Some(if state.weather == Weather::Rain {
            Outfit::Raincoat
        } else {
            Outfit::Garden
        })
    } else {
        keeper_outfit(state, job)
    };
    if let Some(outfit) = outfit {
        text.push_str(&format!("\n园丁穿着{}", outfit.label()));
    }
    if let Some(moment) = fishing {
        text.push_str(match moment.phase {
            Phase::Casting | Phase::Waiting | Phase::Bite => "\n园丁在池边钓鱼",
            Phase::Caught => "\n园丁钓到一条小鱼",
            Phase::Eating => "\n橘猫在吃刚钓到的鱼",
            Phase::Disappointed => "\n这次没钓到鱼，园丁有点失落",
        });
    }
    let context = cast_context(
        state,
        &garden,
        &ManualActions::default(),
        unix_millis() / 1000,
        fishing.is_some(),
        effects,
    );
    for role in [context.guest, context.companion].into_iter().flatten() {
        text.push_str(&format!("\n访客 {}", role.label()));
        if let Some(action) = cast::action(role, &context) {
            text.push_str(&format!(" · {} ({})", action.label, action.id));
        }
    }
    for sample in projects::visible_with_help(
        state.phase_seconds,
        seconds_of_day(state),
        size,
        projects,
        &effects.work,
    )
    .filter(|sample| sample.kind != projects::Kind::Empty)
    {
        text.push_str(&format!(
            "\n{} · {} {}%",
            sample.kind.label(),
            sample.stage.label(),
            sample.percent
        ));
        if sample.stage != projects::Stage::Complete {
            let (start, end) = sample.kind.shift();
            text.push_str(&format!(
                " · {:02}:{:02}–{:02}:{:02}",
                start / 60,
                start % 60,
                end / 60,
                end % 60
            ));
        }
    }
    effects.append_notebook(&mut text);
    text
}

fn seconds_of_day(state: &Snapshot) -> u64 {
    u64::from(state.minutes_of_day) * 60
        + (state.world_start.saturating_add(state.phase_seconds) % 60)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct View {
    width: i32,
    height: i32,
}

impl View {
    fn new((width, height): (i32, i32)) -> Self {
        Self { width, height }
    }

    fn x_growth(self) -> i32 {
        (self.width - WIDTH).max(0)
    }
    fn y_growth(self) -> i32 {
        (self.height - HEIGHT).max(0)
    }
    fn top(self) -> i32 {
        (HEIGHT - self.height).max(0)
    }

    fn fixed(self, x: i32, y: i32) -> (i32, i32) {
        (
            x + i32::from(x >= WIDE_AT) * self.x_growth(),
            y - self.top(),
        )
    }

    fn roaming(self, x: i32, y: i32) -> (i32, i32) {
        let extra = (x - 61).clamp(0, 72) * self.x_growth() / 72;
        let x = (x + extra).clamp(9, self.width - 10);
        let front = (y - 76).clamp(0, 13);
        (x, y - self.top() + self.y_growth() * front / 13)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Backdrop {
    view: View,
    daylight: usize,
    season: Season,
    stage: usize,
    scaffolding: Option<usize>,
    lantern: Option<bool>,
    water: bool,
    plots: [Option<PlotVisual>; 16],
    buildings: [Option<Structure>; LOTS],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlotVisual {
    x: i32,
    y: i32,
    kind: Kind,
    stage: Stage,
    event: Event,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Structure {
    x: i32,
    y: i32,
    kind: projects::Kind,
    stage: projects::Stage,
    late: bool,
}

fn garden_at(
    state: &Snapshot,
    size: (i32, i32),
    unlocks: &[u64; 16],
    effects: &Effects,
) -> ecology::Garden {
    ecology::plots_with_care(
        state.world_start.saturating_add(state.phase_seconds),
        state.world_start,
        size.0 as usize,
        size.1 as usize,
        state.season,
        state.weather,
        unlocks,
        &effects.plots,
    )
}

fn selected_guest(state: &Snapshot, actions: &ManualActions, now: u64) -> Option<Role> {
    actions
        .invited_role
        .filter(|role| role.is_extra() && recent(actions.visitor_invited_at, now, 90))
        .or_else(|| cast::guest(state))
}

fn cast_context<'a>(
    state: &'a Snapshot,
    garden: &'a ecology::Garden,
    actions: &ManualActions,
    now: u64,
    fish: bool,
    effects: &Effects,
) -> cast::Context<'a> {
    let mut context = cast::Context::new(state, garden);
    context.water = actions.water_bowl && effects.has_water();
    context.lamp = actions.lantern.unwrap_or(
        matches!(state.day_part, DayPart::Dusk | DayPart::Night)
            || effects
                .active(EffectKind::Lamp, state.phase_seconds)
                .is_some(),
    );
    context.food = snack_available(effects, actions, now);
    context.knock = recent(actions.window_tapped_at, now, 5);
    context.fish = fish;
    let legacy = Role::from_visitor(state.visitor);
    let extra = selected_guest(state, actions, now);
    context.guest = legacy.or(extra);
    context.companion = if legacy.is_some() && extra.is_some() {
        extra
    } else {
        context
            .guest
            .and_then(|primary| cast::paired_guest(primary, state))
    }
    .filter(|role| Some(*role) != context.guest && cast::can_visit(*role));
    context
}

fn snack_available(effects: &Effects, actions: &ManualActions, now: u64) -> bool {
    effects.has_food()
        || (recent(actions.visitor_snack_at, now, 180)
            && effects.mark(EffectKind::Forage).is_none())
}

pub fn apply_results(
    state: &Snapshot,
    size: (i32, i32),
    unlocks: &[u64; 16],
    projects: &[Project; LOTS],
    effects: &mut Effects,
    results: [Option<cast::Action>; 4],
) -> bool {
    if results.iter().all(Option::is_none) {
        return false;
    }
    let mut changed = false;
    for action in results.into_iter().flatten() {
        if !effects.accepts(action) {
            continue;
        }
        let garden = garden_at(state, size, unlocks, effects);
        let project = if matches!(
            action.effect_kind,
            EffectKind::Build | EffectKind::Repair | EffectKind::Survey
        ) {
            projects::visible_with_help(
                state.phase_seconds,
                seconds_of_day(state),
                size,
                projects,
                &effects.work,
            )
            .filter(|sample| {
                sample.kind != projects::Kind::Empty && sample.stage != projects::Stage::Complete
            })
            .min_by_key(|sample| sample.x.abs_diff(action.x) + sample.y.abs_diff(action.y))
            .map(|sample| sample.lot)
        } else {
            None
        };
        changed |= effects.apply(action, state.phase_seconds, &garden, project);
    }
    changed
}

fn plot_job<'a>(state: &Snapshot, garden: &'a ecology::Garden) -> Option<&'a Plot> {
    if state.day_part == DayPart::Night || state.caretaker != CareAction::Away {
        return None;
    }
    garden.job()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outfit {
    Garden,
    Apron,
    Helmet,
    Raincoat,
    Sweater,
}

impl Outfit {
    fn frame(self, step: usize) -> usize {
        self as usize * OUTFIT_FRAMES + step % OUTFIT_FRAMES
    }

    fn label(self) -> &'static str {
        match self {
            Self::Garden => "草帽和背带裤",
            Self::Apron => "厨房围裙",
            Self::Helmet => "安全帽和工作服",
            Self::Raincoat => "雨衣和雨靴",
            Self::Sweater => "针织外套",
        }
    }
}

fn keeper_outfit(state: &Snapshot, job: Option<&Plot>) -> Option<Outfit> {
    let outdoors = || {
        if matches!(state.weather, Weather::Rain | Weather::Snow) {
            Outfit::Raincoat
        } else {
            Outfit::Garden
        }
    };
    match state.caretaker {
        CareAction::Feeding => Some(Outfit::Apron),
        CareAction::Building => Some(Outfit::Helmet),
        CareAction::Tending => Some(outdoors()),
        CareAction::Resting => Some(Outfit::Sweater),
        CareAction::Away => job.map(|plot| {
            if matches!(plot.stage, Stage::Groundwork | Stage::Framing)
                || (plot.id >= 12 && plot.stage == Stage::Repairing)
            {
                Outfit::Helmet
            } else {
                outdoors()
            }
        }),
    }
}

#[derive(Clone, Copy)]
struct KeeperPose {
    x: i32,
    y: i32,
    outfit: Outfit,
    motion: Motion,
    flip: bool,
    prop: Prop,
}

#[derive(Default)]
struct Wardrobe {
    target: Option<Outfit>,
    started: u64,
    from: Option<KeeperPose>,
    last: Option<KeeperPose>,
    changing: bool,
}

impl Wardrobe {
    fn pose(
        &mut self,
        desired: Option<KeeperPose>,
        door: (i32, i32),
        now: u64,
    ) -> Option<KeeperPose> {
        let target = desired.map(|pose| pose.outfit);
        if self.target != target {
            self.target = target;
            self.started = now;
            self.from = self.last;
        }
        let age = now.saturating_sub(self.started);
        let leave_at = if self.from.is_some() { 2_050 } else { 450 };
        self.changing = if desired.is_some() {
            age < leave_at + 1_250
        } else {
            self.from.is_some() && age < 1_250
        };
        let pose = if age < 1_250 && self.from.is_some() {
            self.from
                .map(|pose| approach(pose, (pose.x, pose.y), door, age, 1_250))
        } else if age < leave_at {
            None
        } else if age < leave_at + 1_250 {
            desired.map(|pose| approach(pose, door, (pose.x, pose.y), age - leave_at, 1_250))
        } else {
            desired
        };
        self.last = pose;
        pose
    }
}

fn approach(
    mut pose: KeeperPose,
    start: (i32, i32),
    end: (i32, i32),
    at: u64,
    span: u64,
) -> KeeperPose {
    (pose.x, pose.y) = lerp(start, end, at, span);
    pose.flip = end.0 < start.0;
    pose.motion = Motion::Patrol;
    pose.prop = Prop::None;
    pose
}

fn lerp(start: (i32, i32), end: (i32, i32), at: u64, span: u64) -> (i32, i32) {
    let step = at.min(span) as i32;
    let span = span as i32;
    (
        start.0 + (end.0 - start.0) * step / span,
        start.1 + (end.1 - start.1) * step / span,
    )
}

pub struct Scene {
    pixels: Vec<u8>,
    background: Vec<u8>,
    backdrop: Option<Backdrop>,
    view: View,
    painted: Option<Damage>,
    wardrobe: Wardrobe,
    cat_last: Option<(i32, i32)>,
    cat_entry: Option<u64>,
    results: [Option<cast::Action>; 4],
    details: [Option<DetailSprite>; 4],
    detailed: bool,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            pixels: Vec::new(),
            background: Vec::new(),
            backdrop: None,
            view: View::new((WIDTH, HEIGHT)),
            painted: None,
            wardrobe: Wardrobe::default(),
            cat_last: None,
            cat_entry: None,
            results: [None; 4],
            details: [None; 4],
            detailed: false,
        }
    }

    pub fn in_transition(&self) -> bool {
        self.wardrobe.changing || (self.cat_entry.is_some() && self.cat_last.is_some())
    }

    pub fn take_results(&mut self) -> [Option<cast::Action>; 4] {
        std::mem::take(&mut self.results)
    }

    pub fn render_detailed(
        &mut self,
        state: &Snapshot,
        now_ms: u64,
        actions: &ManualActions,
        size: (i32, i32),
        unlocks: &[u64; 16],
        projects: &[Project; LOTS],
        effects: &Effects,
    ) -> (&[u8], Option<Damage>, &[Option<DetailSprite>; 4]) {
        self.detailed = true;
        let (_, changed) =
            self.render_view(state, now_ms, actions, size, unlocks, projects, effects);
        self.detailed = false;
        (&self.pixels, changed, &self.details)
    }

    pub fn render_view(
        &mut self,
        state: &Snapshot,
        now_ms: u64,
        actions: &ManualActions,
        size: (i32, i32),
        unlocks: &[u64; 16],
        projects: &[Project; LOTS],
        effects: &Effects,
    ) -> (&[u8], Option<Damage>) {
        let view = View::new(size);
        let previous = self.painted.take();
        self.results.fill(None);
        self.details.fill(None);
        let garden = garden_at(state, size, unlocks, effects);
        let mut plots = [None; 16];
        for plot in garden.plots() {
            plots[usize::from(plot.id)] = Some(PlotVisual {
                x: plot.x as i32,
                y: plot.y as i32,
                kind: plot.kind,
                stage: plot.stage,
                event: plot.event,
            });
        }
        let mut buildings = [None; LOTS];
        let mut working = None;
        for site in projects::visible_with_help(
            state.phase_seconds,
            seconds_of_day(state),
            size,
            projects,
            &effects.work,
        )
        .filter(|site| site.kind != projects::Kind::Empty)
        {
            buildings[site.lot] = Some(Structure {
                x: site.x,
                y: site.y,
                kind: site.kind,
                stage: site.stage,
                late: site.stage_progress >= 50,
            });
            if working.is_none() && site.working {
                working = Some((site.x + 11, site.y - 2));
            }
        }
        let backdrop = Backdrop {
            view,
            daylight: match state.day_part {
                DayPart::Dawn => 0,
                DayPart::Day => 1,
                DayPart::Dusk => 2,
                DayPart::Night => 3,
            },
            season: state.season,
            stage: state.build_stage.min(3) as usize,
            scaffolding: (state.build_in_progress && state.build_stage < 3).then(|| {
                usize::from(state.build_stage) * 3
                    + usize::from(state.build_percent.min(99)) * 3 / 100
            }),
            lantern: actions.lantern.or_else(|| {
                effects
                    .active(EffectKind::Lamp, state.phase_seconds)
                    .map(|_| true)
            }),
            water: actions.water_bowl && effects.has_water(),
            plots,
            buildings,
        };
        let dirty = if self.backdrop != Some(backdrop) {
            self.view = view;
            self.rebuild(backdrop);
            self.backdrop = Some(backdrop);
            Some(Damage::full(size))
        } else {
            if let Some(region) = previous {
                let width = view.width as usize;
                for y in region.top as usize..region.bottom as usize {
                    let start = (y * width + region.left as usize) * 4;
                    let end = (y * width + region.right as usize) * 4;
                    self.pixels[start..end].copy_from_slice(&self.background[start..end]);
                }
            }
            previous
        };
        self.weather(state.weather, state.day_part, now_ms);
        self.pond_glint(state.day_part, now_ms);
        self.world_marks(effects, &garden, state, now_ms);
        let now = now_ms / 1000;
        let active = activities::current(state);
        if let Some(activity) = active.yard {
            let (x, y, _) = pose(activity, now_ms);
            let (x, y) = view.roaming(x, y);
            self.prop(activity.prop, x, y, now_ms);
        }
        if snack_available(effects, actions, now) {
            let (x, y) = view.roaming(140, 80);
            self.prop(Prop::Food, x, y, now_ms);
        }
        let pond = garden
            .plots()
            .iter()
            .find(|plot| plot.kind == Kind::Pond && plot.stage == Stage::Complete)
            .map_or_else(
                || view.fixed(38, 85),
                |plot| (plot.x as i32, plot.y as i32 - 2),
            );
        let fishing = working.is_none().then(|| fishing::moment(state)).flatten();
        let context = cast_context(state, &garden, actions, now, fishing.is_some(), effects);
        let keeper_act = cast::action(Role(0), &context);
        let cat_act = cast::action(Role(10), &context);
        let guests = [context.guest, context.companion];
        let guest_actions = guests.map(|guest| guest.and_then(|role| cast::action(role, &context)));
        let joint = guest_actions
            .iter()
            .flatten()
            .chain(
                [keeper_act.as_ref(), cat_act.as_ref()]
                    .into_iter()
                    .flatten(),
            )
            .find(|action| action.partner.is_some());
        let own_scene = |action: Option<cast::Action>| {
            action.filter(|act| joint.is_none_or(|leader| Some(act.role) != leader.partner))
        };
        let reacting = |role| joint.filter(|leader| leader.partner == Some(role));
        self.people(
            state,
            active.keeper,
            plot_job(state, &garden),
            fishing.map(|moment| (moment, pond)),
            working,
            own_scene(keeper_act).as_ref(),
            reacting(Role(0)),
            now_ms,
        );
        for (role, action) in guests
            .into_iter()
            .zip(guest_actions.iter())
            .filter_map(|(role, action)| Some((role?, action.as_ref())))
        {
            let baseline = active
                .visitor
                .filter(|_| Some(role) == Role::from_visitor(state.visitor));
            self.draw_guest(
                role,
                baseline,
                own_scene(action.copied()).as_ref(),
                reacting(role),
                now_ms,
            );
        }
        self.cat(
            state,
            active.cat,
            now_ms,
            actions,
            fishing.map(|moment| (moment, pond)),
            own_scene(cat_act).as_ref(),
            reacting(Role(10)),
        );
        (
            &self.pixels,
            match (dirty, self.painted) {
                (Some(before), Some(after)) => Some(before.union(after)),
                (before, after) => before.or(after),
            },
        )
    }

    fn rebuild(&mut self, backdrop: Backdrop) {
        let view = self.view;
        let size = (view.width * view.height * 4) as usize;
        self.pixels.resize(size, 0);
        let base = frame(SCENES, FRAME_BYTES, backdrop.daylight);
        let builds = frame(BUILDS, FRAME_BYTES, backdrop.stage);
        let construction = backdrop
            .scaffolding
            .map(|index| frame(CONSTRUCTION, FRAME_BYTES, index));
        let (wide, tall) = (view.x_growth(), view.y_growth());
        let building_light = match backdrop.daylight {
            0 => [95, 92, 100],
            2 => [83, 84, 97],
            3 => [78, 66, 62],
            _ => [100; 3],
        };
        for y in 0..view.height {
            let line = y + view.top();
            let extra_y = (TALL_AT..TALL_AT + tall).contains(&line);
            let source_y = if line >= TALL_AT + tall {
                line - tall
            } else {
                line
            };
            for x in 0..view.width {
                let extra_x = (WIDE_AT..WIDE_AT + wide).contains(&x);
                let source_x = if x >= WIDE_AT + wide {
                    x - wide
                } else {
                    x.min(WIDE_AT)
                };
                let index = ((y * view.width + x) * 4) as usize;
                let target = &mut self.pixels[index..index + 4];
                let open_ground = extra_x && (53..90).contains(&source_y)
                    || extra_y && (13..view.width - 13).contains(&x);
                if open_ground {
                    target.copy_from_slice(terrain(backdrop.daylight, x, y, extra_y));
                } else if extra_y {
                    target.fill(0);
                } else {
                    let source = ((source_y * WIDTH + source_x) * 4) as usize;
                    target.copy_from_slice(&base[source..source + 4]);
                    if !extra_x {
                        over_building(target, &builds[source..source + 4], building_light);
                        if let Some(layer) = construction {
                            over_building(target, &layer[source..source + 4], building_light);
                        }
                    }
                }
            }
        }
        self.apply_season(backdrop.season);
        self.yard_paths(backdrop.daylight == 3);
        let (pond_x, pond_y) = view.fixed(38, 85);
        self.pond(pond_x, pond_y, backdrop.daylight == 3);
        if let Some(lit) = backdrop.lantern {
            self.porch_light(lit);
        }
        if backdrop.water {
            let (x, y) = view.fixed(86, 84);
            self.prop(Prop::Water, x.min(view.width - 10), y, 0);
        }
        for plot in backdrop.plots.into_iter().flatten() {
            self.plot(plot, backdrop.daylight == 3);
        }
        for building in backdrop.buildings.into_iter().flatten() {
            self.structure(building, building_light);
        }
        // This solid handle is visible and receives mouse input even over empty sky.
        for y in view.height - 14..view.height {
            for x in view.width - 14..view.width {
                if view.width - x + view.height - y <= 15 {
                    self.point(x, y, [73, 64, 80], 218);
                }
            }
        }
        for (dx, dy) in [(8, 2), (5, 2), (2, 2), (5, 5), (2, 5), (2, 8)] {
            self.point(
                view.width - dx - 1,
                view.height - dy - 1,
                [237, 217, 170],
                242,
            );
        }
        self.background.resize(size, 0);
        self.background.copy_from_slice(&self.pixels);
        if self.pixels.capacity() > size * 4 {
            self.pixels.shrink_to(size);
            self.background.shrink_to(size);
        }
        self.painted = None;
    }

    fn yard_paths(&mut self, night: bool) {
        let view = self.view;
        let y = 80 - view.top();
        let end = view.width - 18;
        let added_land = view.x_growth();
        let mut bends = [
            (48, y + 1),
            (65, y - 2),
            (85, y - 1),
            (107 + added_land, y + 1),
            (125 + added_land, y),
            (end, y - 3),
        ];
        for (x, _) in &mut bends {
            *x = (*x).min(end);
        }
        self.path(&bends, false, 2, night);
        self.path(&[(y - 10, 50), (y - 6, 51), (y + 1, 48)], true, 2, night);
        if view.width >= 210 {
            self.path(&[(y - 7, 112), (y - 3, 107), (y + 1, 105)], true, 1, night);
            self.path(
                &[
                    (112, y - 7),
                    (135, y - 12),
                    (end - 24, y - 11),
                    (end, y - 16),
                ],
                false,
                1,
                night,
            );
        }
        if view.height >= 143 {
            self.path(
                &[(y + 2, 49), (100, 54), (114, 47), (133, 51)],
                true,
                2,
                night,
            );
            let mut southern_bends = [(30, 134), (51, 133), (69, 129), (104, 131), (end, 128)];
            for (x, _) in &mut southern_bends[..4] {
                *x = (*x).min(end);
            }
            self.path(&southern_bends, false, 2, night);
        }
    }

    fn path(&mut self, bends: &[(i32, i32)], vertical: bool, width: i32, night: bool) {
        let [edge, dirt, shade, grit] = if night {
            [[59, 71, 75], [102, 98, 91], [86, 85, 81], [131, 119, 102]]
        } else {
            [
                [134, 144, 77],
                [203, 174, 113],
                [172, 145, 95],
                [228, 202, 142],
            ]
        };
        for pair in bends.windows(2) {
            let (from, start) = pair[0];
            let (to, finish) = pair[1];
            for along in from..to {
                let center = start + (finish - start) * (along - from) / (to - from);
                let near = width + i32::from(along.rem_euclid(11) < 3);
                let far = width + i32::from(along.rem_euclid(17) >= 12);
                for across in -near..=far {
                    let (x, y) = if vertical {
                        (center + across, along)
                    } else {
                        (along, center + across)
                    };
                    let grain = (x * 29 + y * 13 + x * y).rem_euclid(31);
                    let (color, alpha) = match (across, grain) {
                        (at, 0..=11) if at == -near => (edge, 138),
                        (at, _) if at == -near => (shade, 172),
                        (at, 0..=9) if at == far => (edge, 104),
                        (at, _) if at == far => (shade, 194),
                        (_, 0..=2) => (grit, 255),
                        (_, 3..=5) => (shade, 235),
                        _ => (dirt, 255),
                    };
                    self.point(x, y, color, alpha);
                }
            }
        }
    }

    fn plant_bed(&mut self, x: i32, y: i32, tree: bool, crop: bool, night: bool) {
        let [base, ridge, shadow] = match (tree, crop, night) {
            (true, _, false) => [[116, 139, 73], [149, 158, 83], [85, 117, 69]],
            (true, _, true) => [[58, 76, 60], [73, 92, 65], [49, 65, 59]],
            (_, true, false) => [[141, 89, 62], [184, 128, 76], [108, 73, 57]],
            (_, true, true) => [[92, 70, 65], [123, 92, 71], [71, 58, 57]],
            (_, _, false) => [[159, 112, 71], [194, 145, 89], [118, 94, 62]],
            (_, _, true) => [[106, 83, 68], [133, 104, 77], [79, 68, 61]],
        };
        for dy in -3_i32..=2 {
            let half = if dy.abs() == 3 || dy == 2 { 4 } else { 6 };
            for dx in -half..=half {
                let grain = (x + y + dx * 3 + dy * 7).rem_euclid(11);
                let color = match (dy, grain) {
                    (2, _) | (_, 0) => shadow,
                    (-3, _) | (0, 4..=8) if crop => ridge,
                    (_, 1..=3) => ridge,
                    _ => base,
                };
                let alpha = if dx.abs() == half || dy == 2 {
                    216
                } else {
                    255
                };
                self.point(x + dx, y + dy, color, alpha);
            }
        }
    }

    fn apply_season(&mut self, season: Season) {
        let tint = match season {
            Season::Spring => return,
            Season::Autumn => [88u16, 131, 205],
            Season::Winter => [200u16, 193, 177],
        };
        for pixel in self.pixels.chunks_exact_mut(4) {
            if pixel[3] < 225
                || pixel[1] <= pixel[2].saturating_add(7)
                || pixel[1] <= pixel[0].saturating_add(5)
            {
                continue;
            }
            let alpha = u16::from(pixel[3]);
            for (channel, seasonal) in pixel[..3].iter_mut().zip(tint) {
                *channel = ((u16::from(*channel) * 6 + seasonal * 4) / 10).min(alpha) as u8;
            }
        }
    }

    fn plot(&mut self, plot: PlotVisual, night: bool) {
        let PlotVisual {
            x,
            y,
            kind,
            stage,
            event,
        } = plot;
        let rim = [165, 124, 88];
        let dark = [97, 73, 68];
        if matches!(
            kind,
            Kind::Planter | Kind::ToolShed | Kind::Pond | Kind::Greenhouse
        ) {
            self.small_building(x, y, kind, stage, night);
        } else {
            let (color, fruit, tree, root) = match kind {
                Kind::Daisy => ([101, 150, 83], [255, 238, 193], false, false),
                Kind::Tulip => ([95, 149, 87], [236, 110, 139], false, false),
                Kind::Lavender => ([114, 149, 99], [185, 144, 213], false, false),
                Kind::Sunflower => ([122, 146, 76], [255, 194, 74], false, false),
                Kind::Oak => ([104, 153, 76], [206, 176, 105], true, false),
                Kind::Maple => ([202, 133, 69], [247, 175, 105], true, false),
                Kind::Pine => ([72, 130, 106], [142, 180, 122], true, false),
                Kind::Apple => ([116, 157, 71], [227, 95, 82], true, false),
                Kind::Carrot => ([108, 155, 82], [245, 153, 65], false, true),
                Kind::Radish => ([119, 160, 87], [230, 124, 150], false, true),
                Kind::Lettuce => ([115, 162, 84], [192, 204, 109], false, true),
                Kind::Pumpkin => ([116, 145, 72], [246, 154, 59], false, true),
                _ => unreachable!(),
            };
            self.plant_bed(x, y, tree, root, night);
            match stage {
                Stage::Seed => {
                    self.point(x - 2, y - 1, fruit, 255);
                    self.point(x + 2, y, fruit, 255);
                    self.point(x, y - if tree { 2 } else { 1 }, color, 255);
                    if root {
                        self.point(x + 1, y, fruit, 255);
                    }
                }
                Stage::Harvest => {
                    self.prop(Prop::Food, x - 1, y, 0);
                    self.point(x - 2, y - 3, fruit, 255);
                    self.point(x, y - 3, fruit, 255);
                    if tree {
                        self.point(x + 4, y - 3, rim, 255);
                        self.point(x + 4, y - 2, rim, 255);
                    }
                }
                Stage::Damaged | Stage::Repairing => {
                    for dx in -4_i32..=4 {
                        self.point(x + dx, y - dx.rem_euclid(2) - 1, dark, 255);
                    }
                    if stage == Stage::Repairing {
                        self.prop(Prop::Tool, x + 3, y - 1, 0);
                    }
                }
                _ => {
                    let size = match stage {
                        Stage::Sprout | Stage::Recovering => 2,
                        Stage::Growing => {
                            if tree {
                                6
                            } else {
                                4
                            }
                        }
                        Stage::Budding => {
                            if tree {
                                9
                            } else {
                                6
                            }
                        }
                        _ => {
                            if tree {
                                12
                            } else {
                                7
                            }
                        }
                    };
                    let color = if night {
                        color.map(|channel: u8| channel - channel / 3)
                    } else {
                        color
                    };
                    let fruit = if night {
                        fruit.map(|channel: u8| channel - channel / 4)
                    } else {
                        fruit
                    };
                    if tree && size >= 6 {
                        self.orchard_tree((x, y), kind, size, color, fruit, night);
                    } else {
                        let leaf = color.map(|channel| channel.saturating_add(26));
                        for dy in 1..=size {
                            self.point(x, y - dy, color, 255);
                        }
                        for (dx, dy) in [(-2, 3), (-1, 2), (1, 2), (2, 3)] {
                            if size >= dy {
                                self.point(x + dx, y - dy, if dx < 0 { leaf } else { color }, 255);
                            }
                        }
                        if matches!(stage, Stage::Budding | Stage::Mature) {
                            let fruit_y = if root { y - 1 } else { y - size };
                            let offsets: &[(i32, i32)] = match (kind, stage) {
                                (_, Stage::Budding) => &[(0, 0)],
                                (Kind::Lavender, _) => {
                                    &[(0, -2), (-1, -1), (0, -1), (0, 0), (1, 0)]
                                }
                                (Kind::Tulip, _) => {
                                    &[(-1, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (0, 1)]
                                }
                                (Kind::Pumpkin | Kind::Lettuce, _) => &[
                                    (-1, -1),
                                    (0, -1),
                                    (1, -1),
                                    (-2, 0),
                                    (-1, 0),
                                    (0, 0),
                                    (1, 0),
                                    (2, 0),
                                    (-1, 1),
                                    (0, 1),
                                    (1, 1),
                                ],
                                _ => &[(0, -1), (-1, 0), (0, 0), (1, 0), (0, 1)],
                            };
                            let glint = fruit.map(|channel| channel.saturating_add(23));
                            for &(dx, dy) in offsets {
                                self.point(
                                    x + dx,
                                    fruit_y + dy,
                                    if dy < 0 { glint } else { fruit },
                                    255,
                                );
                            }
                            if stage == Stage::Mature {
                                let spacing = if root { 4 } else { 3 };
                                for (side, height) in [(-spacing, size - 2), (spacing, size - 3)] {
                                    let top = y - height;
                                    for sy in top..y {
                                        self.point(x + side, sy, color, 255);
                                    }
                                    self.point(x + side - 1, y - 2, leaf, 255);
                                    self.point(x + side + 1, y - 3, color, 255);
                                    let bloom_y = if root { y - 1 } else { top };
                                    self.point(x + side, bloom_y, fruit, 255);
                                    self.point(x + side + 1, bloom_y, glint, 255);
                                }
                            }
                            if stage == Stage::Mature
                                && matches!(kind, Kind::Daisy | Kind::Sunflower)
                            {
                                self.point(
                                    x,
                                    fruit_y,
                                    if kind == Kind::Daisy {
                                        [220, 158, 66]
                                    } else {
                                        dark
                                    },
                                    255,
                                );
                            }
                        }
                    }
                }
            }
        }
        if event != Event::None {
            let prop = match event {
                Event::Storm => Prop::Leaf,
                Event::Frost => Prop::Snow,
                Event::Drought => Prop::Stone,
                Event::Flood => Prop::Puddle,
                Event::None => unreachable!(),
            };
            self.prop(prop, x + 5, y + 1, 0);
        }
    }

    fn orchard_tree(
        &mut self,
        (x, y): (i32, i32),
        kind: Kind,
        height: i32,
        leaf: [u8; 3],
        fruit: [u8; 3],
        night: bool,
    ) {
        let trunk = if night { [111, 83, 68] } else { [155, 106, 67] };
        let shade = leaf.map(|channel| channel.saturating_sub(25));
        let light = leaf.map(|channel| channel.saturating_add(25));
        for dy in 1..height {
            self.point(x, y - dy, trunk, 255);
            if height >= 9 {
                self.point(x + 1, y - dy, [98, 74, 62], 255);
            }
        }
        let crown = height / 3;
        for row in 0..height * 2 / 3 {
            let half = if kind == Kind::Pine {
                (row % 4 + row / 3).min(crown)
            } else {
                (1 + row.min(height * 2 / 3 - row)).min(crown)
            };
            for dx in -half..=half {
                let color = if dx == half || row >= height * 2 / 3 - 2 {
                    shade
                } else if dx < 0 && (row + dx).rem_euclid(5) < 2 {
                    light
                } else {
                    leaf
                };
                self.point(x + dx, y - height + row, color, 255);
            }
        }
        if height >= 9 {
            self.point(x - 2, y - height + 3, fruit, 255);
        }
        if height >= 12 {
            self.point(x + 2, y - height + 4, fruit, 255);
            self.point(x - 1, y - height + 5, fruit, 255);
        }
    }

    fn small_building(&mut self, x: i32, y: i32, kind: Kind, stage: Stage, night: bool) {
        let wood = if night { [91, 72, 66] } else { [121, 82, 59] };
        let trim = if night { [127, 99, 73] } else { [188, 132, 82] };
        for dx in -7..=7 {
            self.point(x + dx, y + 1, wood, 110);
            self.point(x + dx, y, trim, 255);
        }
        match stage {
            Stage::Groundwork => self.prop(Prop::Wood, x + 2, y - 2, 0),
            Stage::Framing | Stage::Repairing => {
                for dy in 2..=8 {
                    self.point(x - 6, y - dy, wood, 255);
                    self.point(x + 6, y - dy, trim, 255);
                    self.point(x + dy - 5, y - dy, trim, 255);
                }
            }
            Stage::Damaged => self.prop(Prop::Stone, x, y - 2, 0),
            _ => {
                // A shared palette keeps the permanent garden fixtures legible at 1×.
                let palette: [[u8; 3]; 9] = [
                    [104, 73, 62],
                    [155, 99, 64],
                    [211, 152, 93],
                    [80, 118, 76],
                    [135, 164, 81],
                    [79, 151, 145],
                    [156, 199, 170],
                    [235, 203, 125],
                    [223, 130, 122],
                ];
                let art: &[&str] = match kind {
                    Kind::Planter => &[
                        "..7....8....7..",
                        ".747..848..747.",
                        "..34...34...3..",
                        ".223444443422.",
                        "00111111111100",
                        ".012222222210.",
                        "..0111111110..",
                    ],
                    Kind::ToolShed => &[
                        "....2222222....",
                        "...211111112...",
                        "..21112211112..",
                        ".2111111111112.",
                        "000000000000000",
                        ".121117771121.",
                        ".121113331121.",
                        ".122213431121.",
                        ".121113331121.",
                        ".121113331121.",
                        ".000000000000.",
                    ],
                    Kind::Pond => &[
                        "...34..........",
                        "..433......3...",
                        "..32666662334..",
                        ".265555555562.",
                        ".065555654552.",
                        "..02655555620.",
                        "....0222220...",
                    ],
                    Kind::Greenhouse => &[
                        "....6666666....",
                        "...656565656...",
                        "..66565656566..",
                        ".6666666666666.",
                        ".456564656564.",
                        ".456563656564.",
                        ".454363634354.",
                        ".454363634354.",
                        ".333333333333.",
                        ".000000000000.",
                    ],
                    _ => unreachable!(),
                };
                for (row, line) in art.iter().rev().enumerate() {
                    for (column, ink) in line.bytes().enumerate() {
                        if ink != b'.' {
                            let mut color = palette[usize::from(ink - b'0')];
                            if night {
                                color = color.map(|channel| channel - channel / 3);
                            }
                            self.point(x - 7 + column as i32, y - row as i32, color, 255);
                        }
                    }
                }
            }
        }
    }

    fn structure(&mut self, building: Structure, light: [u16; 3]) {
        let Structure {
            x,
            y,
            kind,
            stage,
            late,
        } = building;
        let index = (usize::from(kind.style()) - 1) * 12 + stage as usize * 2 + usize::from(late);
        let tile = frame(BUILDING_STAGES, STAGE_BYTES, index);
        for (row, line) in tile.chunks_exact(48 * 4).enumerate() {
            for (column, pixel) in line.chunks_exact(4).enumerate() {
                if pixel[3] != 0 {
                    self.pixel(
                        x - 24 + column as i32,
                        y - 35 + row as i32,
                        &lit_building_pixel(pixel, light),
                    );
                }
            }
        }
    }

    fn role_animation(
        &mut self,
        role: Role,
        motion: Motion,
        position: (i32, i32, bool),
        ms: u64,
        fallback: (&[u8], usize),
    ) {
        if !self.detailed {
            self.sprite(fallback.0, fallback.1, position, motion);
            return;
        }
        let timing = match motion {
            Motion::Hop | Motion::Patrol => 130,
            Motion::Bob | Motion::Tilt => 170,
            Motion::Nap => 440,
            Motion::Still => 250,
        };
        let index = (usize::from(role.0) * 6 + motion as usize) * 6 + (ms / timing) as usize % 6;
        let detail = DetailSprite::new(frame(ACTION_ANIMATIONS, DETAIL_BYTES, index), position);
        if let Some(area) = detail.damage((self.view.width, self.view.height))
            && let Some(slot) = self.details.iter_mut().find(|slot| slot.is_none())
        {
            *slot = Some(detail);
            self.painted = Some(self.painted.map_or(area, |old| old.union(area)));
        }
    }

    fn sprite(
        &mut self,
        sheet: &[u8],
        index: usize,
        (x, y, flip): (i32, i32, bool),
        motion: Motion,
    ) {
        let sprite = frame(sheet, SPRITE_BYTES, index);
        let left = x - 24;
        let top = y - 29;
        for sy in 0..SPRITE_H {
            let lean = i32::from(motion == Motion::Tilt) * ((29 - sy as i32) / 6);
            let crouch = if motion == Motion::Nap {
                (29 - sy as i32) / 3
            } else {
                0
            };
            let row = top + sy as i32 + crouch;
            if !(0..self.view.height).contains(&row) {
                continue;
            }
            let left = left + lean;
            let first = (-left).clamp(0, SPRITE_W as i32) as usize;
            let last = (self.view.width - left).clamp(0, SPRITE_W as i32) as usize;
            for sx in first..last {
                let px = if flip { SPRITE_W - sx - 1 } else { sx };
                let offset = (sy * SPRITE_W + px) * 4;
                let pixel = &sprite[offset..offset + 4];
                if pixel[3] != 0 {
                    self.pixel(left + sx as i32, row, pixel);
                }
            }
        }
    }

    fn pixel(&mut self, x: i32, y: i32, color: &[u8]) {
        if (0..self.view.width).contains(&x) && (0..self.view.height).contains(&y) {
            let offset = ((y * self.view.width + x) * 4) as usize;
            over(&mut self.pixels[offset..offset + 4], color);
            let damage = Damage {
                left: x,
                top: y,
                right: x + 1,
                bottom: y + 1,
            };
            self.painted = Some(self.painted.map_or(damage, |prior| prior.union(damage)));
        }
    }

    fn point(&mut self, x: i32, y: i32, rgb: [u8; 3], alpha: u8) {
        let premul = |channel| (u16::from(channel) * u16::from(alpha) / 255) as u8;
        self.pixel(
            x,
            y,
            &[premul(rgb[2]), premul(rgb[1]), premul(rgb[0]), alpha],
        );
    }

    fn weather(&mut self, weather: Weather, day: DayPart, ms: u64) {
        match weather {
            Weather::Clear => {}
            Weather::Clouds => {
                let dim = if day == DayPart::Night { 65 } else { 125 };
                let offset = ((ms / 9_000) % 12) as i32 + self.view.width * 2 / 3 - 107;
                for (line, (left, right)) in [(2, 10), (0, 17), (0, 20), (4, 17)].iter().enumerate()
                {
                    for col in *left..*right {
                        self.point(107 + col + offset, 18 + line as i32, [212, 216, 226], dim);
                    }
                }
            }
            Weather::Rain => {
                let tick = ms / 240;
                let spread = (self.view.width - 18) as u64;
                let depth = (self.view.height - 24) as u64;
                for n in 0..16 {
                    let x = 10 + ((n * 31 + n * n * 3 + tick / 4) % spread) as i32;
                    let y = 12 + ((n * 19 + tick * 5) % depth) as i32;
                    self.point(x, y, [177, 213, 240], 107);
                    self.point(x - 1, y + 1, [177, 213, 240], 73);
                    self.point(x - 1, y + 2, [177, 213, 240], 44);
                }
            }
            Weather::Snow => {
                let tick = ms / 300;
                let spread = (self.view.width - 18) as u64;
                let depth = (self.view.height - 24) as u64;
                for n in 0..17 {
                    let x = 10 + ((n * 31 + n * n + tick / 3) % spread) as i32;
                    let y = 12 + ((n * 17 + tick * (n % 2 + 1)) % depth) as i32;
                    self.point(x, y, [233, 240, 255], 175);
                    if n % 4 == 0 {
                        self.point(x + 1, y, [233, 240, 255], 81);
                    }
                }
            }
        }
    }

    fn porch_light(&mut self, lit: bool) {
        let glass = if lit { [255, 209, 122] } else { [68, 77, 99] };
        let frame = if lit { [182, 120, 86] } else { [93, 75, 78] };
        for y in 58..64 {
            for x in 33..38 {
                self.point(
                    x,
                    y - self.view.top(),
                    if y == 60 || x == 35 { frame } else { glass },
                    255,
                );
            }
        }
        self.point(35, 64 - self.view.top(), frame, 255);
    }

    fn pond(&mut self, x: i32, y: i32, dark: bool) {
        let [bank, deep, water, shallow, ripple, stone, sunlit] = if dark {
            [
                [54, 73, 73],
                [36, 64, 89],
                [40, 83, 109],
                [53, 105, 122],
                [79, 124, 141],
                [80, 88, 87],
                [106, 112, 107],
            ]
        } else {
            [
                [114, 126, 76],
                [39, 107, 129],
                [54, 145, 159],
                [88, 174, 172],
                [144, 209, 193],
                [138, 137, 109],
                [177, 168, 131],
            ]
        };
        let banks: [(i32, i32); 12] = [
            (-6, 2),
            (-9, 5),
            (-11, 9),
            (-12, 10),
            (-12, 12),
            (-11, 13),
            (-11, 13),
            (-10, 12),
            (-8, 11),
            (-6, 9),
            (-3, 7),
            (1, 5),
        ];
        for (row, &(left, right)) in banks.iter().enumerate() {
            let dy = row as i32 - 6;
            for dx in left..=right {
                let edge = (dx - left).min(right - dx);
                let color = match (row, edge) {
                    (0 | 11, _) | (_, 0) => bank,
                    (1, _) | (2..=4, 1) => deep,
                    (10, _) | (_, 1) => shallow,
                    (3..=6, _) if dx < -4 => deep,
                    _ => water,
                };
                self.point(x + dx, y + dy, color, 255);
            }
        }
        for (offset, dy, length) in [(-7, -3, 3), (1, -2, 4), (7, -1, 3), (-2, 1, 4), (4, 3, 5)] {
            for dx in offset..offset + length {
                self.point(x + dx, y + dy, ripple, if dx == offset { 108 } else { 166 });
            }
        }
        for (dx, dy) in [(-11, 0), (-8, 3), (0, 5), (8, 3), (11, 1), (5, -5)] {
            for (sx, sy) in [(0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                self.point(
                    x + dx + sx,
                    y + dy + sy,
                    if sy == 0 { sunlit } else { stone },
                    255,
                );
            }
        }
        let reed = if dark { [74, 105, 75] } else { [102, 153, 69] };
        let tip = if dark { [121, 121, 81] } else { [166, 173, 77] };
        for (dx, dy, height) in [
            (-11, -2, 4),
            (-9, -3, 6),
            (-7, -4, 3),
            (10, -2, 4),
            (12, -1, 3),
        ] {
            for stem in 0..height {
                self.point(
                    x + dx,
                    y + dy - stem,
                    if stem == height - 1 { tip } else { reed },
                    255,
                );
            }
            self.point(x + dx - 1, y + dy - 2, tip, 255);
        }
        for (dx, dy) in [(-5, 0), (-4, -1), (-4, 0), (-3, 0), (-4, 1)] {
            self.point(x + dx, y + dy, if dy < 0 { tip } else { reed }, 255);
        }
    }

    fn fish(&mut self, x: i32, y: i32) {
        for (dx, dy) in [
            (-3, -1),
            (-2, 0),
            (-3, 1),
            (-1, 0),
            (0, -1),
            (0, 0),
            (0, 1),
            (1, -1),
            (1, 0),
            (2, 0),
        ] {
            self.point(x + dx, y + dy, [155, 210, 219], 255);
        }
        self.point(x + 1, y - 1, [62, 79, 110], 255);
    }

    fn pond_glint(&mut self, time: DayPart, ms: u64) {
        let beat = (ms / 320) % 12;
        if beat >= 5 {
            return;
        }
        let (x, y) = self.view.fixed(38, 85);
        let color = match time {
            DayPart::Night => [86, 129, 149],
            DayPart::Dusk => [193, 192, 148],
            _ => [157, 217, 196],
        };
        let span = 1 + beat as i32 / 2;
        for dx in -span..=span {
            self.point(x + 5 + dx, y, color, 145 - beat as u8 * 24);
        }
    }

    fn pond_event(&mut self, moment: Moment, (x, y): (i32, i32), ms: u64) {
        match moment.phase {
            Phase::Casting | Phase::Waiting | Phase::Bite => {
                let swing = if moment.phase == Phase::Casting {
                    i32::from(moment.elapsed / 5)
                } else {
                    3
                };
                for n in 0..8 {
                    self.point(x + 14 - n, y - 8 - n / 2 + 3 - swing, [159, 112, 83], 255);
                }
                for n in 0..12 {
                    self.point(x + 7 - n / 2, y - 12 + n, [231, 229, 201], 178);
                }
                let dip = i32::from(moment.phase == Phase::Bite && (ms / 220).is_multiple_of(2));
                self.point(x + 1, y - 1 + dip, [246, 137, 123], 255);
                if moment.phase == Phase::Bite {
                    self.prop(Prop::Puddle, x, y + 1, ms);
                }
            }
            Phase::Caught => self.fish(x + 12, y - 11 - i32::from(moment.elapsed % 3)),
            Phase::Eating => {
                self.prop(Prop::Food, x + 8, y + 5, ms);
                if moment.elapsed < 28 {
                    self.fish(x + 8, y + 2);
                }
            }
            Phase::Disappointed => {
                let drop = ((ms / 520) % 3) as i32;
                for (dx, dy) in [(0, 0), (-1, 1), (0, 1), (1, 1), (0, 2)] {
                    self.point(x + 17 + dx, y - 12 + dy + drop, [150, 194, 226], 220);
                }
            }
        }
    }

    fn people(
        &mut self,
        state: &Snapshot,
        keeper: Option<&Activity>,
        job: Option<&Plot>,
        fishing: Option<(Moment, (i32, i32))>,
        project: Option<(i32, i32)>,
        role_action: Option<&cast::Action>,
        reaction: Option<&cast::Action>,
        ms: u64,
    ) {
        let outfit = keeper_outfit(state, job).unwrap_or(Outfit::Garden);
        let role_action =
            role_action.filter(|_| fishing.is_none() && project.is_none() && job.is_none());
        let desired = if let Some((moment, (x, y))) = fishing {
            Some(KeeperPose {
                x: x + 16,
                y: y - 2,
                flip: true,
                motion: if moment.phase == Phase::Disappointed {
                    Motion::Tilt
                } else {
                    Motion::Still
                },
                outfit: if state.weather == Weather::Rain {
                    Outfit::Raincoat
                } else {
                    Outfit::Garden
                },
                prop: Prop::None,
            })
        } else if let Some((x, y)) = project.filter(|_| {
            matches!(
                state.caretaker,
                CareAction::Away | CareAction::Resting | CareAction::Tending
            )
        }) {
            Some(KeeperPose {
                x,
                y,
                outfit: Outfit::Helmet,
                motion: Motion::Bob,
                flip: true,
                prop: Prop::Tool,
            })
        } else if let Some(plot) = job {
            let x = plot.x as i32;
            let y = plot.y as i32;
            let prop = match plot.stage {
                Stage::Seed => Prop::Seed,
                Stage::Sprout | Stage::Growing | Stage::Recovering => Prop::Water,
                Stage::Budding => Prop::Flower,
                Stage::Mature | Stage::Harvest => Prop::Food,
                Stage::Groundwork | Stage::Framing | Stage::Complete => Prop::Wood,
                Stage::Damaged | Stage::Repairing => Prop::Tool,
            };
            Some(KeeperPose {
                x: x - 9,
                y: y + 3,
                flip: false,
                motion: Motion::Tilt,
                outfit,
                prop,
            })
        } else if let Some(leader) = reaction {
            let (x, y, flip) = self.partner_position(leader, ms);
            Some(KeeperPose {
                x,
                y,
                flip,
                outfit,
                motion: Motion::Tilt,
                prop: Prop::None,
            })
        } else if let Some(action) = role_action {
            let (x, y, flip) = self.cast_position(action, ms);
            let prop = if action.phase == cast::Phase::Approach {
                Prop::None
            } else {
                action.prop
            };
            Some(KeeperPose {
                x,
                y,
                flip,
                outfit,
                motion: action.motion,
                prop,
            })
        } else if let Some(activity) = keeper {
            let (mut x, mut y, flip) = pose(activity, ms);
            if state.caretaker == CareAction::Building {
                let (dx, dy) = match state.build_stage {
                    0 => (5, 13),
                    1 => (-18, 0),
                    _ => (0, 0),
                };
                x += dx;
                y += dy;
                (x, y) = self.view.fixed(x, y);
            } else {
                (x, y) = self.view.roaming(x, y);
            }
            (x < self.view.width).then_some(KeeperPose {
                x,
                y,
                flip,
                motion: activity.motion,
                outfit,
                prop: activity.prop,
            })
        } else {
            None
        };
        let door = self.view.fixed(50, 69);
        if let Some(KeeperPose {
            x,
            y,
            outfit,
            motion,
            flip,
            prop,
        }) = self.wardrobe.pose(desired, door, ms)
        {
            let frame = outfit.frame((ms / 500) as usize);
            if !self.wardrobe.changing
                && (outfit == Outfit::Garden || outfit == Outfit::Sweater && motion == Motion::Nap)
            {
                self.role_animation(Role(0), motion, (x, y, flip), ms, (KEEPER, frame));
            } else {
                self.sprite(KEEPER, frame, (x, y, flip), motion);
            }
            self.actor_prop(prop, x, y, flip, ms);
            if !self.wardrobe.changing {
                if let Some((moment, pond)) = fishing {
                    self.pond_event(moment, pond, ms);
                } else if outfit == Outfit::Helmet && prop == Prop::Tool {
                    self.prop(Prop::Spark, x + 9, y - 13, ms);
                } else if let Some(action) = role_action {
                    self.cast_result(action, x, y, ms);
                }
            }
        }
    }

    fn draw_guest(
        &mut self,
        role: Role,
        baseline: Option<&Activity>,
        action: Option<&cast::Action>,
        reaction: Option<&cast::Action>,
        ms: u64,
    ) {
        let step = (ms / 650) as usize % 3;
        let fallback: (&[u8], usize) = match role.0 {
            1..=9 => (EXTRA_CAST, usize::from(role.0 - 1) * 3 + step),
            11 => (GREY_CAT, step),
            12 => (BLACK_CAT, step),
            13 => (SPARROW, step),
            14 => (SQUIRREL, step),
            15 => (HEDGEHOG, step),
            16 => (RABBIT, step),
            17 => (FROG, step),
            18..=29 => (EXTRA_CAST, usize::from(role.0 - 9) * 3 + step),
            _ => return,
        };
        if let Some(leader) = reaction {
            let position = self.partner_position(leader, ms);
            self.role_animation(role, Motion::Tilt, position, ms, fallback);
        } else if let Some(action) = action {
            let (x, y, flip) = self.cast_position(action, ms);
            self.role_animation(role, action.motion, (x, y, flip), ms, fallback);
            if action.phase != cast::Phase::Approach {
                self.actor_prop(action.prop, x, y, flip, ms);
            }
            self.cast_result(action, x, y, ms);
        } else if let Some(activity) = baseline {
            let (x, y, flip) = pose(activity, ms);
            let (x, y) = self.view.roaming(x, y);
            self.role_animation(role, activity.motion, (x, y, !flip), ms, fallback);
            self.actor_prop(activity.prop, x, y, !flip, ms);
        } else {
            let x = self.view.width - 24;
            let y = (self.view.height - 24).max(16);
            self.role_animation(role, Motion::Still, (x, y, true), ms, fallback);
        }
    }

    fn partner_position(&self, action: &cast::Action, ms: u64) -> (i32, i32, bool) {
        let (x, y, _) = self.cast_position(action, ms);
        let on_right = x < self.view.width - 24;
        let neighbor = if on_right { x + 12 } else { x - 12 };
        (neighbor.clamp(9, self.view.width - 10), y, on_right)
    }

    fn cast_position(&self, action: &cast::Action, ms: u64) -> (i32, i32, bool) {
        let pond_pair = matches!(
            (action.role.0, action.partner.map(|role| role.0)),
            (17, Some(19)) | (7, Some(17 | 19 | 20))
        );
        let (x, y) = if pond_pair {
            self.view.fixed(32, 86)
        } else if action.plot.is_some() {
            (action.x, action.y)
        } else if action.target == cast::Target::Pond && action.phase != cast::Phase::Approach {
            self.view.fixed(42, 85)
        } else {
            self.view.roaming(action.x, action.y)
        };
        let (x, y, approaching_left) = if action.phase == cast::Phase::Approach {
            let entry = match action.role.0 {
                0 => self.view.fixed(50, 69),
                10 => self.view.fixed(110, 78),
                _ => (self.view.width - 10, (self.view.height - 19).min(y + 7)),
            };
            let progress = i32::from(action.phase_progress);
            (
                entry.0 + (x - entry.0) * progress / 100,
                entry.1 + (y - entry.1) * progress / 100,
                x < entry.0,
            )
        } else {
            (x, y, action.role.0 >= 10)
        };
        let (dx, mut dy, patrol_flip) = motion_offset(action.motion, ms);
        if self.detailed && matches!(action.motion, Motion::Hop | Motion::Bob) {
            dy = 0;
        }
        (
            (x + dx).clamp(9, self.view.width - 10),
            (y + dy).clamp(12, self.view.height - 10),
            approaching_left ^ (action.phase != cast::Phase::Approach && patrol_flip),
        )
    }

    fn cast_result(&mut self, action: &cast::Action, x: i32, y: i32, ms: u64) {
        if action.phase != cast::Phase::Result && !action.completes_now {
            return;
        }
        if let Some(slot) = self
            .results
            .iter_mut()
            .find(|slot| slot.is_none_or(|earlier| earlier.role == action.role))
        {
            *slot = Some(*action);
        }
        if action.prop == Prop::None {
            return;
        }
        let (dx, dy) = match action.target {
            cast::Target::Sky => (8, -14),
            cast::Target::Water | cast::Target::Pond => (9, 2),
            cast::Target::Flower
            | cast::Target::Tree
            | cast::Target::Crop
            | cast::Target::Garden => (-6, 3),
            _ => (7, 1),
        };
        self.prop(action.prop, x + dx, y + dy, ms);
    }

    fn world_marks(
        &mut self,
        effects: &Effects,
        garden: &ecology::Garden,
        state: &Snapshot,
        ms: u64,
    ) {
        use EffectKind::*;
        for (kind, prop, anchor) in [
            (Trail, Prop::Paw, (74, 86)),
            (Signal, Prop::Note, (127, 63)),
            (Rest, Prop::Star, (116, 73)),
            (Bond, Prop::Heart, (103, 74)),
            (MoveObject, Prop::Leaf, (58, 89)),
            (Forage, Prop::Crumb, (67, 87)),
            (Tidy, Prop::Spark, (47, 81)),
            (Water, Prop::Water, (40, 90)),
            (Drain, Prop::Puddle, (31, 88)),
            (Sow, Prop::Seed, (85, 87)),
            (TendPlant, Prop::Leaf, (86, 85)),
            (Pollinate, Prop::Flower, (119, 86)),
            (Harvest, Prop::Food, (94, 88)),
            (Build, Prop::Wood, (119, 63)),
            (Repair, Prop::Stone, (125, 66)),
            (Shelter, Prop::Wood, (16, 74)),
            (Nest, Prop::Feather, (136, 66)),
            (Deliver, Prop::Note, (34, 73)),
            (Forecast, Prop::Cloud, (71, 66)),
            (Art, Prop::Book, (56, 74)),
            (SeedTrade, Prop::Seed, (65, 76)),
            (Food, Prop::Food, (140, 80)),
            (Lamp, Prop::Lamp, (44, 65)),
            (Survey, Prop::Stone, (111, 66)),
        ] {
            let Some(mark) = effects.active(kind, state.phase_seconds) else {
                continue;
            };
            if kind == Food && !effects.has_food()
                || kind == Trail && state.weather == Weather::Rain
            {
                continue;
            }
            let (x, y) = if mark.site < 16
                && matches!(
                    kind,
                    Water | Drain | Sow | TendPlant | Pollinate | Harvest | Build | Repair | Art
                ) {
                let Some(plot) = garden.plots().iter().find(|plot| plot.id == mark.site) else {
                    continue;
                };
                (plot.x as i32 + 4, plot.y as i32 - 3)
            } else {
                self.view.roaming(anchor.0, anchor.1)
            };
            if kind == Art {
                for yy in -6..0 {
                    for xx in -3..=3 {
                        let color = if xx == -3 || xx == 3 || yy == -6 {
                            [154, 116, 81]
                        } else if yy < -3 {
                            [222, 192, 177]
                        } else {
                            [122, 161, 113]
                        };
                        self.point(x + xx, y + yy, color, 255);
                    }
                }
            } else {
                self.prop(prop, x, y, ms);
            }
        }
    }

    fn cat(
        &mut self,
        state: &Snapshot,
        activity: Option<&Activity>,
        ms: u64,
        actions: &ManualActions,
        fishing: Option<(Moment, (i32, i32))>,
        role_action: Option<&cast::Action>,
        reaction: Option<&cast::Action>,
    ) {
        let Some(activity) = activity else { return };
        if state.day_part == DayPart::Night && state.cat == CatAction::Sleep {
            let door = self.view.fixed(50, 69);
            if let Some(start) = self.cat_last {
                let elapsed = ms.saturating_sub(*self.cat_entry.get_or_insert(ms));
                if elapsed < 1_450 {
                    let (x, y) = lerp(start, door, elapsed, 1_450);
                    self.role_animation(
                        Role(10),
                        Motion::Patrol,
                        (x, y, door.0 < start.0),
                        ms,
                        (CAT_WALK, (ms / 200) as usize),
                    );
                    return;
                }
                self.cat_last = None;
            }
            let (x, y) = self.view.fixed(43, 52 - ((ms / 950) % 3) as i32);
            self.sleep_mark(x, y);
            if let Some(action) = role_action {
                self.cast_result(action, x, y, ms);
            }
            return;
        }
        self.cat_entry = None;
        let (mut x, mut y, mut flip) = pose(activity, ms);
        (x, y) = self.view.roaming(x, y);
        if let Some((moment, (pond_x, pond_y))) = fishing {
            if moment.phase == Phase::Eating {
                self.cat_last = Some((pond_x + 3, pond_y + 6));
                self.role_animation(
                    Role(10),
                    Motion::Bob,
                    (pond_x + 3, pond_y + 6, false),
                    ms,
                    (CAT_EAT, (ms / 320) as usize),
                );
                return;
            }
        }
        let (mut sheet, pace) = match state.cat {
            CatAction::Idle => (CAT_IDLE, 850),
            CatAction::Sleep => (CAT_SLEEP, 1_700),
            CatAction::Walk => (CAT_WALK, 170),
            CatAction::Eat => (CAT_EAT, 260),
            CatAction::Treat => (CAT_TREAT, 260),
        };
        let mut motion = activity.motion;
        let mut prop = activity.prop;
        if let Some(leader) =
            reaction.filter(|_| matches!(state.cat, CatAction::Idle | CatAction::Walk))
        {
            (x, y, flip) = self.partner_position(leader, ms);
            motion = Motion::Tilt;
            prop = Prop::None;
        } else if let Some(action) = role_action.filter(|_| state.cat != CatAction::Treat) {
            (x, y, flip) = self.cast_position(action, ms);
            motion = action.motion;
            prop = if action.phase == cast::Phase::Approach {
                Prop::None
            } else {
                action.prop
            };
            self.cast_result(action, x, y, ms);
        }
        if let Some(action) =
            role_action.filter(|action| state.cat == CatAction::Treat && action.completes_now)
        {
            self.cast_result(action, x, y, ms);
        }
        if state.cat == CatAction::Sleep && (780..815).contains(&state.minutes_of_day) {
            let sheltered = matches!(state.weather, Weather::Rain | Weather::Snow);
            (x, y) = self.view.fixed(
                if sheltered { 57 } else { 110 },
                if sheltered { 73 } else { 78 },
            );
        }
        if cat_heard_window(state, ms / 1000, actions) {
            (x, y) = self.view.fixed(51, 75);
            flip = true;
            sheet = CAT_IDLE;
            motion = Motion::Tilt;
            let (bell_x, bell_y) = self.view.fixed(40, 57);
            self.prop(Prop::Bell, bell_x, bell_y, ms);
        } else {
            self.actor_prop(prop, x, y, flip, ms);
            if state.cat == CatAction::Eat && !matches!(prop, Prop::Food | Prop::Water) {
                self.prop(Prop::Food, x + 11, y + 1, ms);
            }
        }
        if state.cat == CatAction::Sleep {
            motion = Motion::Still;
            let lift = ((ms / 1_400) % 3) as i32;
            let (side, above) = if self.detailed { (5, 8) } else { (9, 15) };
            self.sleep_mark(x + side + lift, y - above - lift);
        }
        self.cat_last = Some((x, y));
        let motion = match (self.detailed, state.cat) {
            (true, CatAction::Sleep) => Motion::Nap,
            (true, CatAction::Eat | CatAction::Treat) => Motion::Bob,
            _ => motion,
        };
        self.role_animation(
            Role(10),
            motion,
            (x, y, flip),
            ms,
            (sheet, (ms / pace) as usize),
        );
    }

    fn actor_prop(&mut self, prop: Prop, x: i32, y: i32, flip: bool, ms: u64) {
        let side = if flip { -1 } else { 1 };
        let x = (x + side * 10).clamp(5, self.view.width - 5);
        let y = if matches!(
            prop,
            Prop::Bell | Prop::Heart | Prop::Note | Prop::Spark | Prop::Star
        ) {
            y - 14 - ((ms / 600) % 3) as i32
        } else {
            y + 1
        };
        self.prop(prop, x, y, ms);
    }

    fn prop(&mut self, prop: Prop, x: i32, y: i32, ms: u64) {
        let gold = [250, 208, 125];
        let cream = [255, 235, 179];
        let rose = [232, 151, 175];
        let green = [109, 157, 112];
        let blue = [139, 198, 224];
        let pale = [206, 232, 242];
        let wood = [175, 121, 82];
        let stone = [146, 151, 164];
        let dark = [86, 75, 96];
        let flash = (ms / 550).is_multiple_of(2);
        let (pixels, first, second): (&[&str], [u8; 3], [u8; 3]) = match prop {
            Prop::None => return,
            Prop::Bell => (&["..+..", ".###.", ".###+", "#####", "..+.."], gold, cream),
            Prop::Book => (&["++.++", "##+##", "+#+#+", "##+##", ".###."], dark, cream),
            Prop::Butterfly if flash => (&["#...#", "##+##", ".#+#.", "..+.."], rose, gold),
            Prop::Butterfly => (&[".....", ".#+#.", "##+##", ".#.#."], rose, gold),
            Prop::Cloud => (
                &["...##...", ".######.", "++++++++", ".++++++."],
                pale,
                [182, 194, 213],
            ),
            Prop::Crumb => (&[".+...", "....+", "#..#."], wood, gold),
            Prop::Feather => (&["....+", "...++", ".+++#", ".+##.", "#...."], stone, pale),
            Prop::Flower => (&["..#..", ".#+#.", "..#..", "..+..", ".++.."], rose, green),
            Prop::Food => (&[".+.+.", ".++..", "#####", ".###."], wood, gold),
            Prop::Heart => (&[".#.#.", "#####", ".###.", "..#.."], rose, cream),
            Prop::Lamp => (&["..+..", ".###.", ".#+#.", ".+++.", "..#.."], wood, gold),
            Prop::Leaf => (&["..###", ".##+.", "##+..", "+...."], green, gold),
            Prop::Mist => (&[".++....", "++++..+", "..++++."], pale, [186, 202, 216]),
            Prop::Note => (&["..+##", "..+..", "..+..", "#++..", ".#..."], pale, cream),
            Prop::Paw => (&[".#.#.", "#...#", "..#..", ".###."], [133, 112, 99], pale),
            Prop::Puddle if flash => (&["..+++..", ".+###+.", "++#+#++", ".+++++."], blue, pale),
            Prop::Puddle => (&["..+++..", ".+#+#+.", "++##+++", ".+++++."], blue, pale),
            Prop::Rain => (&["...+.", ".+...", "....+", "..+..", ".#..."], blue, pale),
            Prop::Seed => (&["...+.", ".#..+", "#..#."], wood, gold),
            Prop::Snow => (
                &["..+..", ".+#+.", "+###+", ".+#+.", "..+.."],
                pale,
                [219, 232, 255],
            ),
            Prop::Spark if flash => (&["..+..", ".....", "+.#.+", ".....", "..+.."], cream, gold),
            Prop::Spark => (&[".....", "..#..", ".#+#.", "..#.."], gold, cream),
            Prop::Star => (&["..+..", "..#..", "+###+", "..#..", "..+.."], gold, cream),
            Prop::Stone => (&[".++..", "+++#.", "#####", ".###."], stone, pale),
            Prop::Tool => (&["..+##", "...++", "..#..", ".#...", "#...."], wood, stone),
            Prop::Water if flash => (&["..+..", "#+++#", "#####", ".###."], blue, pale),
            Prop::Water => (&["...+.", "#+++#", "#####", ".###."], blue, pale),
            Prop::Wood => (&["..++.", ".+++.", "+++#.", "+##..", "##..."], wood, gold),
        };
        let alpha = if matches!(prop, Prop::Mist | Prop::Cloud) {
            123
        } else {
            239
        };
        let left = x - pixels[0].len() as i32 / 2;
        let top = y - pixels.len() as i32 + 1;
        for (dy, row) in pixels.iter().enumerate() {
            for (dx, part) in row.bytes().enumerate() {
                let color = match part {
                    b'#' => first,
                    b'+' => second,
                    _ => continue,
                };
                self.point(left + dx as i32, top + dy as i32, color, alpha);
            }
        }
    }

    fn sleep_mark(&mut self, x: i32, y: i32) {
        for (dx, dy) in [
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 0),
            (2, 1),
            (1, 2),
            (0, 3),
            (1, 3),
            (2, 3),
            (3, 3),
        ] {
            self.point(x + dx, y + dy, [237, 229, 244], 213);
        }
    }
}

fn pose(activity: &Activity, ms: u64) -> (i32, i32, bool) {
    let (x, y, flip) = motion_offset(activity.motion, ms);
    (activity.x + x, activity.y + y, flip)
}

fn motion_offset(motion: Motion, ms: u64) -> (i32, i32, bool) {
    let phase = |duration, steps| ((ms / duration) % steps) as i32;
    match motion {
        Motion::Still | Motion::Tilt | Motion::Nap => (0, 0, false),
        Motion::Bob => (0, -phase(480_u64, 2_u64), false),
        Motion::Hop => (
            0,
            -[0, 0, 2, 5, 3, 0][phase(240_u64, 6_u64) as usize],
            false,
        ),
        Motion::Patrol => {
            let step = phase(230_u64, 28_u64);
            (step.min(28 - step) - 7, 0, step >= 14)
        }
    }
}

fn terrain(daylight: usize, x: i32, y: i32, front: bool) -> &'static [u8] {
    let grid_x = x / 16;
    let grid_y = y / 16;
    let kind = match ((grid_x * 7 + grid_y * 11) % 13, front) {
        (0, _) => 3,
        (1..=2, true) => 2,
        _ => (grid_x + grid_y) as usize % 2,
    };
    let tile = frame(TERRAIN, TILE_BYTES, daylight * 4 + kind);
    let offset = (((y % 16) * 16 + x % 16) * 4) as usize;
    &tile[offset..offset + 4]
}

fn frame(bytes: &[u8], size: usize, index: usize) -> &[u8] {
    let start = (index % (bytes.len() / size)) * size;
    &bytes[start..start + size]
}

fn over(target: &mut [u8], pixel: &[u8]) {
    let alpha = pixel[3];
    if alpha == 255 {
        target.copy_from_slice(pixel);
    } else if alpha != 0 {
        let inverse = u16::from(255 - alpha);
        for (dest, source) in target.iter_mut().zip(pixel) {
            *dest = (u16::from(*source) + u16::from(*dest) * inverse / 255) as u8;
        }
    }
}

fn over_building(target: &mut [u8], pixel: &[u8], light: [u16; 3]) {
    if pixel[3] != 0 {
        over(target, &lit_building_pixel(pixel, light));
    }
}

fn lit_building_pixel(pixel: &[u8], light: [u16; 3]) -> [u8; 4] {
    // The atlas uses premultiplied BGRA. Warm window glass stays luminous at night.
    if pixel[3] == 255 && pixel[2] >= 233 && pixel[1] >= 182 && pixel[0] <= 146 {
        [pixel[0], pixel[1], pixel[2], pixel[3]]
    } else {
        let shade = |channel: usize| (u16::from(pixel[channel]) * light[channel] / 100) as u8;
        [shade(0), shade(1), shade(2), pixel[3]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::{WeatherMode, snapshot};
    const UNLOCKS: [u64; 16] = [1; 16];
    const NO_PROJECTS: [Project; LOTS] = [Project {
        kind: projects::Kind::Empty,
        start_age: 0,
    }; LOTS];

    fn sample() -> Snapshot {
        snapshot(
            1_800_000_000,
            1_799_996_000,
            720,
            Season::Spring,
            WeatherMode::Clear,
            None,
        )
    }

    #[test]
    fn clothes_change_only_after_the_keeper_passes_through_the_house() {
        let door = (50, 69);
        let garden = KeeperPose {
            x: 92,
            y: 79,
            outfit: Outfit::Garden,
            motion: Motion::Still,
            flip: false,
            prop: Prop::Water,
        };
        let apron = KeeperPose {
            x: 75,
            y: 80,
            outfit: Outfit::Apron,
            prop: Prop::Food,
            ..garden
        };
        let mut wardrobe = Wardrobe::default();
        assert!(wardrobe.pose(Some(garden), door, 10_000).is_none());
        assert_eq!(
            wardrobe.pose(Some(garden), door, 12_000).unwrap().outfit,
            Outfit::Garden
        );
        assert_eq!(
            wardrobe.pose(Some(apron), door, 13_000).unwrap().outfit,
            Outfit::Garden
        );
        let midway = wardrobe.pose(Some(apron), door, 13_600).unwrap();
        assert!(midway.x < garden.x && midway.x > door.0);
        assert!(wardrobe.pose(Some(apron), door, 14_500).is_none());
        let emerging = wardrobe.pose(Some(apron), door, 15_100).unwrap();
        assert_eq!(emerging.outfit, Outfit::Apron);
        assert!(emerging.x < apron.x);
        assert_eq!(wardrobe.pose(Some(apron), door, 16_500).unwrap().x, apron.x);
        assert!(!wardrobe.changing);
    }

    #[test]
    fn cat_stays_outdoors_for_a_noon_nap_and_walks_inside_at_night() {
        let mut state = sample();
        state.cat = CatAction::Sleep;
        state.minutes_of_day = 800;
        state.day_part = DayPart::Day;
        let mut scene = Scene::new();
        let manual = ManualActions::default();
        scene.render_view(
            &state,
            1_000_000,
            &manual,
            (160, 100),
            &UNLOCKS,
            &NO_PROJECTS,
            &Effects::default(),
        );
        assert_eq!(scene.cat_last, Some((110, 78)));
        state.day_part = DayPart::Night;
        scene.render_view(
            &state,
            1_000_100,
            &manual,
            (160, 100),
            &UNLOCKS,
            &NO_PROJECTS,
            &Effects::default(),
        );
        assert!(scene.in_transition());
        scene.render_view(
            &state,
            1_001_700,
            &manual,
            (160, 100),
            &UNLOCKS,
            &NO_PROJECTS,
            &Effects::default(),
        );
        assert_eq!(scene.cat_last, None);
        assert!(!scene.in_transition());
    }

    #[test]
    fn manual_visits_and_caretaker_call_expire_without_changing_construction() {
        let mut base = sample();
        base.caretaker = CareAction::Away;
        base.visitor = Visitor::None;
        let actions = ManualActions {
            keeper_called_at: Some(1_000),
            visitor_invited_at: Some(1_000),
            invited_visitor: Visitor::BlackCat,
            ..Default::default()
        };
        let active = observe(base, 1_049, &actions);
        assert_eq!(active.caretaker, CareAction::Tending);
        assert_eq!(active.visitor, Visitor::BlackCat);
        assert_eq!(active.build_percent, base.build_percent);
        assert_eq!(active.build_stage, base.build_stage);
        assert_eq!(observe(base, 1_050, &actions).caretaker, CareAction::Away);
        assert_eq!(observe(base, 1_090, &actions), base);
        assert_eq!(observe(base, 999, &actions), base);
        base.caretaker = CareAction::Building;
        assert_eq!(
            observe(base, 1_010, &actions).caretaker,
            CareAction::Building
        );
    }

    #[test]
    fn rendered_water_lantern_snack_and_window_actions_change_pixels() {
        let mut state = sample();
        state.cat = CatAction::Idle;
        let mut scene = Scene::new();
        let ms = 1_000_000;
        let baseline = scene
            .render_view(
                &state,
                ms,
                &ManualActions::default(),
                (WIDTH, HEIGHT),
                &UNLOCKS,
                &NO_PROJECTS,
                &Effects::default(),
            )
            .0
            .to_vec();
        for actions in [
            ManualActions {
                water_bowl: false,
                ..Default::default()
            },
            ManualActions {
                lantern: Some(true),
                ..Default::default()
            },
            ManualActions {
                visitor_snack_at: Some(1_000),
                ..Default::default()
            },
            ManualActions {
                window_tapped_at: Some(1_000),
                ..Default::default()
            },
        ] {
            assert_ne!(
                &baseline,
                scene
                    .render_view(
                        &state,
                        ms,
                        &actions,
                        (WIDTH, HEIGHT),
                        &UNLOCKS,
                        &NO_PROJECTS,
                        &Effects::default(),
                    )
                    .0
            );
        }
    }

    #[test]
    fn construction_phases_have_distinct_valid_layers() {
        assert!(check_assets());
        let mut state = sample();
        state.build_in_progress = true;
        let mut scene = Scene::new();
        let mut frames = Vec::new();
        for stage in 0..3 {
            state.build_stage = stage;
            for percent in [0, 34, 67] {
                state.build_percent = percent;
                let (image, _) = scene.render_view(
                    &state,
                    1_000_000,
                    &ManualActions::default(),
                    (WIDTH, HEIGHT),
                    &UNLOCKS,
                    &NO_PROJECTS,
                    &Effects::default(),
                );
                assert!(
                    image
                        .chunks_exact(4)
                        .all(|p| p[..3].iter().all(|channel| *channel <= p[3]))
                );
                assert!(!frames.iter().any(|existing: &Vec<u8>| existing == image));
                frames.push(image.to_vec());
            }
        }
    }

    #[test]
    fn each_new_character_and_blueprint_has_distinct_visible_art() {
        let mut state = sample();
        state.phase_seconds = 90 * 86_400;
        state.caretaker = CareAction::Away;
        let mut frames: Vec<Vec<u8>> = Vec::new();
        for role in (1..=9).chain(18..=29) {
            let mut scene = Scene::new();
            let actions = ManualActions {
                invited_role: Some(Role(role)),
                visitor_invited_at: Some(1_000),
                ..Default::default()
            };
            let image = scene
                .render_view(
                    &state,
                    1_000_000,
                    &actions,
                    (160, 100),
                    &UNLOCKS,
                    &NO_PROJECTS,
                    &Effects::default(),
                )
                .0;
            assert!(
                !frames.iter().any(|previous| previous == image),
                "role {role}"
            );
            frames.push(image.to_vec());
        }
        frames.clear();
        for kind in projects::BLUEPRINTS {
            let mut scene = Scene::new();
            let mut plan = NO_PROJECTS;
            plan[0].kind = kind;
            let image = scene
                .render_view(
                    &state,
                    1_000_000,
                    &ManualActions::default(),
                    (160, 100),
                    &UNLOCKS,
                    &plan,
                    &Effects::default(),
                )
                .0;
            assert!(
                !frames.iter().any(|previous| previous == image),
                "blueprint {}",
                kind.label()
            );
            frames.push(image.to_vec());
        }
    }

    #[test]
    fn completed_sparrow_scene_replants_a_bed_and_survives_reload() {
        let mut state = sample();
        state.phase_seconds = 120;
        let size = (160, 100);
        let mut effects = Effects::default();
        let original = garden_at(&state, size, &UNLOCKS, &effects).plots()[0];
        assert_eq!(original.kind, Kind::Daisy);
        let mut scene = Scene::new();
        scene.render_view(
            &state,
            1000,
            &ManualActions::default(),
            size,
            &UNLOCKS,
            &NO_PROJECTS,
            &effects,
        );
        scene.take_results();
        let mut action = cast::Action {
            role: Role(13),
            id: "A04-01",
            label: "播种",
            detail: "",
            effect: "",
            effect_kind: EffectKind::Sow,
            motion: Motion::Bob,
            prop: Prop::Seed,
            target: cast::Target::Flower,
            x: 24,
            y: 87,
            phase: cast::Phase::Work,
            phase_progress: 50,
            completes_now: false,
            variant: 0,
            partner: None,
            window: 1,
            plot: Some(original.id),
        };
        scene.cast_result(&action, 24, 87, 1000);
        assert_eq!(scene.take_results(), [None; 4]);
        action.phase = cast::Phase::Result;
        scene.cast_result(&action, 24, 87, 1100);
        assert!(apply_results(
            &state,
            size,
            &UNLOCKS,
            &NO_PROJECTS,
            &mut effects,
            scene.take_results()
        ));
        let planted = garden_at(&state, size, &UNLOCKS, &effects).plots()[0];
        assert_eq!((planted.kind, planted.stage), (Kind::Tulip, Stage::Seed));

        let mut encoded = String::new();
        effects.encode(&mut encoded);
        let mut loaded = Effects::default();
        for line in encoded.lines() {
            let (key, value) = line.split_once('=').unwrap();
            loaded.decode_field(key, value);
        }
        assert_eq!(
            garden_at(&state, size, &UNLOCKS, &loaded).plots()[0].kind,
            planted.kind
        );
        assert!(!apply_results(
            &state,
            size,
            &UNLOCKS,
            &NO_PROJECTS,
            &mut loaded,
            [Some(action), None, None, None]
        ));
        action.role = Role(10);
        action.effect_kind = EffectKind::Build;
        assert!(!loaded.accepts(action));
    }

    #[test]
    fn large_view_adds_land_without_stretching_original_pixels() {
        let mut state = sample();
        state.weather = Weather::Clear;
        let mut scene = Scene::new();
        let manual = ManualActions::default();
        let baseline = scene
            .render_view(
                &state,
                1_000_000,
                &manual,
                (160, 100),
                &UNLOCKS,
                &NO_PROJECTS,
                &Effects::default(),
            )
            .0
            .to_vec();
        let (image, first_damage) = scene.render_view(
            &state,
            1_000_000,
            &manual,
            (240, 145),
            &UNLOCKS,
            &NO_PROJECTS,
            &Effects::default(),
        );
        assert_eq!(image.len(), 240 * 145 * 4);
        assert_eq!(first_damage, Some(Damage::full((240, 145))));
        let pixel = |x, y, stride| ((y * stride + x) * 4) as usize;
        for y in 20..50 {
            for x in 27..73 {
                let from = pixel(x, y, 160);
                let to = pixel(x, y, 240);
                assert_eq!(&baseline[from..from + 4], &image[to..to + 4]);
            }
        }
        assert_eq!(image[pixel(101, 99, 240) + 3], 255);
        let (_, damage) = scene.render_view(
            &state,
            1_001_000,
            &manual,
            (240, 145),
            &UNLOCKS,
            &NO_PROJECTS,
            &Effects::default(),
        );
        assert_ne!(damage, Some(Damage::full((240, 145))));
        let (small, _) = scene.render_view(
            &state,
            1_001_000,
            &manual,
            (80, 60),
            &UNLOCKS,
            &NO_PROJECTS,
            &Effects::default(),
        );
        assert_eq!(small.len(), 80 * 60 * 4);
    }
}
