use std::time::{SystemTime, UNIX_EPOCH};

use crate::activities::{self, Activity, Motion, Prop};
use crate::cast::{self, Role};
use crate::ecology::{self, Event, Kind, Plot, Stage};
use crate::fishing::{self, Moment, Phase};
use crate::projects::{self, LOTS, Project};
use crate::renderer::Damage;
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
    let weather = match state.weather {
        Weather::Clear => "晴",
        Weather::Clouds => "多云",
        Weather::Rain => "小雨",
        Weather::Snow => "小雪",
    };
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

pub fn notebook(
    state: &Snapshot,
    size: (i32, i32),
    unlocks: &[u64; 16],
    projects: &[Project; LOTS],
) -> String {
    let active = activities::current(state);
    let garden = garden_at(state, size, unlocks);
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
    );
    if let Some(role) = context.guest {
        text.push_str(&format!("\n访客 {}", role.label()));
        if let Some(action) = cast::action(role, &context) {
            text.push_str(&format!(" · {} ({})", action.label, action.id));
        }
    }
    for sample in projects::visible(state.phase_seconds, seconds_of_day(state), size, projects)
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
}

fn garden_at(state: &Snapshot, size: (i32, i32), unlocks: &[u64; 16]) -> ecology::Garden {
    ecology::plots_with_unlocks(
        state.world_start.saturating_add(state.phase_seconds),
        state.world_start,
        size.0 as usize,
        size.1 as usize,
        state.season,
        state.weather,
        unlocks,
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
) -> cast::Context<'a> {
    let mut context = cast::Context::new(state, garden);
    context.water = actions.water_bowl;
    context.lamp = actions
        .lantern
        .unwrap_or(matches!(state.day_part, DayPart::Dusk | DayPart::Night));
    context.food = recent(actions.visitor_snack_at, now, 180);
    context.knock = recent(actions.window_tapped_at, now, 5);
    context.fish = fish;
    context.guest =
        selected_guest(state, actions, now).or_else(|| Role::from_visitor(state.visitor));
    context
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
    traces: [Option<Trace>; 8],
}

#[derive(Clone, Copy)]
struct Trace {
    prop: Prop,
    x: i32,
    y: i32,
    until_ms: u64,
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
            traces: [None; 8],
        }
    }

    pub fn in_transition(&self) -> bool {
        self.wardrobe.changing || (self.cat_entry.is_some() && self.cat_last.is_some())
    }

    pub fn render_view(
        &mut self,
        state: &Snapshot,
        now_ms: u64,
        actions: &ManualActions,
        size: (i32, i32),
        unlocks: &[u64; 16],
        projects: &[Project; LOTS],
    ) -> (&[u8], Option<Damage>) {
        let view = View::new(size);
        let previous = self.painted.take();
        let garden = garden_at(state, size, unlocks);
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
        for site in projects::visible(state.phase_seconds, seconds_of_day(state), size, projects)
            .filter(|site| site.kind != projects::Kind::Empty)
        {
            buildings[site.lot] = Some(Structure {
                x: site.x,
                y: site.y,
                kind: site.kind,
                stage: site.stage,
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
            lantern: actions.lantern,
            water: actions.water_bowl,
            plots,
            buildings,
        };
        let dirty = if self.backdrop != Some(backdrop) {
            if self.view != view {
                self.traces.fill(None);
            }
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
        for trace in self
            .traces
            .into_iter()
            .flatten()
            .filter(|trace| trace.until_ms > now_ms)
        {
            self.prop(trace.prop, trace.x, trace.y, now_ms);
        }
        let now = now_ms / 1000;
        let active = activities::current(state);
        if let Some(activity) = active.yard {
            let (x, y, _) = pose(activity, now_ms);
            let (x, y) = view.roaming(x, y);
            self.prop(activity.prop, x, y, now_ms);
        }
        if recent(actions.visitor_snack_at, now, 180) {
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
        let context = cast_context(state, &garden, actions, now, fishing.is_some());
        let keeper_act = cast::action(Role(0), &context);
        let cat_act = cast::action(Role(10), &context);
        let visitor_act =
            Role::from_visitor(state.visitor).and_then(|role| cast::action(role, &context));
        self.people(
            state,
            active.keeper,
            active.visitor,
            plot_job(state, &garden),
            fishing.map(|moment| (moment, pond)),
            working,
            keeper_act.as_ref(),
            visitor_act.as_ref(),
            now_ms,
        );
        if let Some(role) = context.guest.filter(|role| role.is_extra()) {
            self.extra_guest(role, cast::action(role, &context).as_ref(), now_ms);
        }
        self.cat(
            state,
            active.cat,
            now_ms,
            actions,
            fishing.map(|moment| (moment, pond)),
            cat_act.as_ref(),
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
                        over(target, &builds[source..source + 4]);
                        if let Some(layer) = construction {
                            over(target, &layer[source..source + 4]);
                        }
                    }
                }
            }
        }
        self.apply_season(backdrop.season);
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
            self.plot(plot);
        }
        for building in backdrop.buildings.into_iter().flatten() {
            self.structure(building);
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

    fn plot(&mut self, plot: PlotVisual) {
        let PlotVisual {
            x,
            y,
            kind,
            stage,
            event,
        } = plot;
        let soil = [128, 90, 69];
        let rim = [165, 124, 88];
        let dark = [97, 73, 68];
        if matches!(
            kind,
            Kind::Planter | Kind::ToolShed | Kind::Pond | Kind::Greenhouse
        ) {
            self.small_building(x, y, kind, stage);
        } else {
            for dy in -2..=1 {
                let edge = if dy == -2 || dy == 1 { 3 } else { 5 };
                for dx in -edge..=edge {
                    let color = if (dx + dy) % 3 == 0 { rim } else { soil };
                    self.point(x + dx, y + dy, color, 236);
                }
            }
            let (color, fruit, tree, root) = match kind {
                Kind::Daisy => ([106, 151, 97], [254, 232, 187], false, false),
                Kind::Tulip => ([95, 154, 102], [217, 100, 131], false, false),
                Kind::Lavender => ([116, 157, 123], [175, 131, 212], false, false),
                Kind::Sunflower => ([114, 143, 80], [243, 191, 82], false, false),
                Kind::Oak => ([95, 137, 87], [204, 172, 107], true, false),
                Kind::Maple => ([171, 122, 82], [237, 162, 113], true, false),
                Kind::Pine => ([78, 131, 113], [154, 183, 138], true, false),
                Kind::Apple => ([105, 155, 98], [224, 112, 105], true, false),
                Kind::Carrot => ([124, 161, 107], [236, 148, 85], false, true),
                Kind::Radish => ([133, 161, 99], [226, 128, 145], false, true),
                Kind::Lettuce => ([113, 161, 91], [174, 200, 108], false, true),
                Kind::Pumpkin => ([110, 145, 99], [236, 149, 69], false, true),
                _ => unreachable!(),
            };
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
                    let stem = if tree && size > 2 { rim } else { color };
                    for dy in 1..=size {
                        self.point(x, y - dy, stem, 255);
                    }
                    let crown = if tree { size / 3 } else { (size / 2).min(2) };
                    for dx in -crown..=crown {
                        self.point(x + dx, y - size + 1 + dx.abs(), color, 255);
                        if tree && size >= 6 {
                            self.point(x + dx, y - size + 2 + dx.abs() / 2, color, 255);
                        }
                    }
                    if matches!(stage, Stage::Budding | Stage::Mature) {
                        let fruit_y = if root { y - 2 } else { y - size };
                        self.point(x, fruit_y, fruit, 255);
                        if stage == Stage::Mature {
                            self.point(x - 1, fruit_y + 1, fruit, 255);
                            self.point(x + 1, fruit_y + 1, fruit, 255);
                            self.point(x, fruit_y - 1, fruit, 255);
                            if tree {
                                self.point(x + 2, fruit_y + 3, fruit, 255);
                                self.point(x - 2, fruit_y + 4, fruit, 255);
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

    fn small_building(&mut self, x: i32, y: i32, kind: Kind, stage: Stage) {
        let wood = [142, 97, 76];
        let trim = [195, 145, 105];
        let fill = match kind {
            Kind::Planter => [134, 103, 76],
            Kind::ToolShed => [165, 114, 84],
            Kind::Pond => [126, 182, 204],
            Kind::Greenhouse => [141, 182, 163],
            _ => unreachable!(),
        };
        for dx in -7..=7 {
            self.point(x + dx, y, wood, 255);
            self.point(x + dx, y - 1, trim, 255);
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
                let height = match kind {
                    Kind::Planter => 4,
                    Kind::Pond => 5,
                    _ => 10,
                };
                for dy in 2..=height {
                    let edge = if dy == height { 4 } else { 6 };
                    for dx in -edge..=edge {
                        let color = if dx == -edge || dx == edge || dy == height {
                            trim
                        } else {
                            fill
                        };
                        self.point(x + dx, y - dy, color, 255);
                    }
                }
                let prop = match kind {
                    Kind::Planter => Prop::Flower,
                    Kind::Pond => Prop::Puddle,
                    Kind::ToolShed => Prop::Tool,
                    Kind::Greenhouse => Prop::Leaf,
                    _ => unreachable!(),
                };
                self.prop(prop, x, y - 3, 0);
            }
        }
    }

    fn structure(&mut self, building: Structure) {
        use projects::Stage as Work;
        let Structure { x, y, kind, stage } = building;
        let style = kind.style();
        let (width, height, base, roof) = match style {
            1 => (11, 12, [177, 123, 78], [121, 89, 95]),
            2 => (12, 14, [153, 157, 171], [107, 110, 131]),
            3 => (11, 13, [173, 99, 90], [119, 76, 88]),
            4 => (10, 18, [182, 139, 93], [103, 145, 112]),
            5 => (12, 11, [151, 192, 187], [218, 224, 203]),
            6 => (11, 13, [130, 171, 178], [84, 119, 152]),
            7 => (12, 12, [170, 128, 101], [119, 129, 156]),
            8 => (13, 16, [164, 96, 85], [120, 78, 94]),
            9 => (7, 24, [155, 122, 107], [101, 108, 138]),
            10 => (12, 12, [212, 170, 123], [154, 94, 111]),
            11 => (13, 11, [143, 185, 148], [97, 138, 116]),
            _ => (12, 14, [142, 142, 135], [82, 132, 137]),
        };
        let dark = [83, 68, 82];
        let timber = [180, 135, 93];
        let trim = [225, 187, 129];
        let glass = [164, 208, 206];
        if stage == Work::Planned {
            for dx in (-width..=width).step_by(3) {
                self.point(x + dx, y, [211, 173, 111], 230);
            }
            for dx in [-width, width] {
                for dy in 1..=4 {
                    self.point(x + dx, y - dy, timber, 255);
                }
            }
            return;
        }
        for dx in -width..=width {
            self.point(x + dx, y, dark, 255);
            self.point(
                x + dx,
                y - 1,
                if style == 6 { [106, 160, 177] } else { trim },
                255,
            );
        }
        if stage == Work::Foundation {
            self.prop(Prop::Wood, x - 4, y - 2, 0);
            self.prop(Prop::Stone, x + 4, y - 2, 0);
            return;
        }
        let lifted = if matches!(style, 4 | 6) {
            if style == 4 { 6 } else { 3 }
        } else {
            2
        };
        for dy in 2..=height {
            for dx in [-width + 1, width - 1] {
                self.point(x + dx, y - dy, timber, 255);
            }
            if dy >= lifted && matches!(stage, Work::Finish | Work::Complete) && style != 7 {
                for dx in -width + 2..width - 1 {
                    let row_edge = matches!(style, 1..=3 | 6 | 8 | 12) && (dy + 1) % 4 == 0;
                    let framing = matches!(style, 5 | 11) && (dx % 4 == 0 || dy % 5 == 0);
                    let color = if framing {
                        roof
                    } else if row_edge {
                        timber
                    } else {
                        base
                    };
                    self.point(x + dx, y - dy, color, 255);
                }
            }
        }
        if stage == Work::Frame {
            for dx in -width..=width {
                let dy = (dx + width) * (height - 3) / (width * 2);
                self.point(x + dx, y - 2 - dy, trim, 255);
            }
            return;
        }
        for rise in 0..=6 {
            let span = match style {
                2 => width + 1 - i32::from(rise >= 3) * 2,
                5 | 11 => width + 1 - (rise * rise / 4),
                8 => width + 1 - rise / 2 - i32::from(rise > 3) * 2,
                7 | 10 => width + 2 - rise / 3,
                _ => width + 2 - rise * 2,
            };
            for dx in -span..=span {
                let color = if rise == 0 {
                    dark
                } else if style == 10 && (dx + width + 2) / 2 % 2 == 0 {
                    [244, 204, 154]
                } else {
                    roof
                };
                self.point(x + dx, y - height - rise, color, 255);
            }
        }
        if stage == Work::Roof {
            return;
        }
        let door = if style == 8 { 3 } else { 2 };
        if !matches!(style, 5 | 7 | 11) {
            for dy in lifted..(lifted + 6).min(height) {
                for dx in -door..=door {
                    self.point(
                        x + dx,
                        y - dy,
                        if dx == 0 && style == 8 { trim } else { dark },
                        255,
                    );
                }
            }
        }
        if stage != Work::Complete {
            return;
        }
        let window_height = height / 2 + 2;
        for offset in [-(width / 2), width / 2] {
            for dy in 0..3 {
                for dx in -1..=1 {
                    self.point(
                        x + offset + dx,
                        y - window_height - dy,
                        if dy == 0 { trim } else { glass },
                        255,
                    );
                }
            }
        }
        match style {
            4 | 9 => {
                for dy in 1..height - 2 {
                    self.point(x + width - 3, y - dy, timber, 255);
                    if dy % 2 == 0 {
                        self.point(x + width - 2, y - dy, trim, 255);
                    }
                }
            }
            5 | 11 => self.prop(Prop::Flower, x - width + 2, y - 2, 0),
            6 => self.prop(Prop::Puddle, x + 4, y + 2, 0),
            7 => self.prop(Prop::Lamp, x, y - height + 1, 0),
            8 => self.prop(Prop::Seed, x - width, y - 3, 0),
            10 => self.prop(Prop::Food, x + width - 2, y - 3, 0),
            12 => self.prop(Prop::Tool, x + width - 2, y - 3, 0),
            _ => {}
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
        let shore = if dark { [61, 72, 88] } else { [103, 117, 98] };
        let water = if dark {
            [64, 101, 130]
        } else {
            [107, 169, 184]
        };
        for dy in -3_i32..=3 {
            let edge = 9 - dy.abs() * 2;
            for dx in -edge..=edge {
                let color = if dx.abs() == edge || dy == 3 {
                    shore
                } else {
                    water
                };
                self.point(x + dx, y + dy, color, 255);
            }
        }
        for dx in -4..=-1 {
            self.point(
                x + dx,
                y - 1,
                if dark {
                    [91, 126, 155]
                } else {
                    [171, 211, 211]
                },
                230,
            );
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
        visitor: Option<&Activity>,
        job: Option<&Plot>,
        fishing: Option<(Moment, (i32, i32))>,
        project: Option<(i32, i32)>,
        role_action: Option<&cast::Action>,
        guest_action: Option<&cast::Action>,
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
            self.sprite(
                KEEPER,
                outfit.frame((ms / 500) as usize),
                (x, y, flip),
                motion,
            );
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
        self.draw_visitor(state, visitor, guest_action, ms);
    }

    fn draw_visitor(
        &mut self,
        state: &Snapshot,
        visitor: Option<&Activity>,
        action: Option<&cast::Action>,
        ms: u64,
    ) {
        if let Some(activity) = visitor {
            let guest = match state.visitor {
                Visitor::None => return,
                Visitor::Sparrow => SPARROW,
                Visitor::GreyCat => GREY_CAT,
                Visitor::BlackCat => BLACK_CAT,
                Visitor::Squirrel => SQUIRREL,
                Visitor::Hedgehog => HEDGEHOG,
                Visitor::Rabbit => RABBIT,
                Visitor::Frog => FROG,
            };
            if let Some(action) = action {
                self.cast_sprite(guest, (ms / 680) as usize, action, ms);
            } else {
                let (x, y, flip) = pose(activity, ms);
                let (x, y) = self.view.roaming(x, y);
                self.sprite(guest, (ms / 680) as usize, (x, y, !flip), activity.motion);
                self.actor_prop(activity.prop, x, y, !flip, ms);
            }
        }
    }

    fn extra_guest(&mut self, role: Role, action: Option<&cast::Action>, ms: u64) {
        let group = match role.0 {
            1..=9 => role.0 - 1,
            18..=29 => role.0 - 9,
            _ => return,
        };
        let step = usize::from(group) * 3 + (ms / 650) as usize % 3;
        if let Some(action) = action {
            self.cast_sprite(EXTRA_CAST, step, action, ms);
        } else {
            let x = self.view.width - 24;
            let y = (self.view.height - 24).max(16);
            self.sprite(EXTRA_CAST, step, (x, y, true), Motion::Still);
        }
    }

    fn cast_sprite(&mut self, atlas: &[u8], step: usize, action: &cast::Action, ms: u64) {
        let (x, y, flip) = self.cast_position(action, ms);
        self.sprite(atlas, step, (x, y, flip), action.motion);
        if action.phase != cast::Phase::Approach {
            self.actor_prop(action.prop, x, y, flip, ms);
        }
        self.cast_result(action, x, y, ms);
    }

    fn cast_position(&self, action: &cast::Action, ms: u64) -> (i32, i32, bool) {
        let (x, y) = if action.plot.is_some() {
            (action.x, action.y)
        } else if action.target == cast::Target::Pond && action.phase != cast::Phase::Approach {
            self.view.fixed(42, 85)
        } else {
            self.view.roaming(action.x, action.y)
        };
        let (dx, dy, patrol_flip) = motion_offset(action.motion, ms);
        (
            (x + dx).clamp(9, self.view.width - 10),
            (y + dy).clamp(12, self.view.height - 10),
            (action.role.0 >= 10) ^ patrol_flip,
        )
    }

    fn cast_result(&mut self, action: &cast::Action, x: i32, y: i32, ms: u64) {
        if action.phase != cast::Phase::Result || action.prop == Prop::None {
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
        self.traces[usize::from(action.role.0) % self.traces.len()] = Some(Trace {
            prop: action.prop,
            x: x + dx,
            y: y + dy,
            until_ms: ms.saturating_add(12_000),
        });
        self.prop(action.prop, x + dx, y + dy, ms);
    }

    fn cat(
        &mut self,
        state: &Snapshot,
        activity: Option<&Activity>,
        ms: u64,
        actions: &ManualActions,
        fishing: Option<(Moment, (i32, i32))>,
        role_action: Option<&cast::Action>,
    ) {
        let Some(activity) = activity else { return };
        if state.day_part == DayPart::Night && state.cat == CatAction::Sleep {
            let door = self.view.fixed(50, 69);
            if let Some(start) = self.cat_last {
                let elapsed = ms.saturating_sub(*self.cat_entry.get_or_insert(ms));
                if elapsed < 1_450 {
                    let (x, y) = lerp(start, door, elapsed, 1_450);
                    self.sprite(
                        CAT_WALK,
                        (ms / 200) as usize,
                        (x, y, door.0 < start.0),
                        Motion::Patrol,
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
                self.sprite(
                    CAT_EAT,
                    (ms / 320) as usize,
                    (pond_x + 3, pond_y + 6, false),
                    Motion::Bob,
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
        if let Some(action) = role_action.filter(|_| state.cat != CatAction::Treat) {
            (x, y, flip) = self.cast_position(action, ms);
            motion = action.motion;
            prop = if action.phase == cast::Phase::Approach {
                Prop::None
            } else {
                action.prop
            };
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
            self.sleep_mark(x + 9 + lift, y - 15 - lift);
        }
        self.cat_last = Some((x, y));
        self.sprite(sheet, (ms / pace) as usize, (x, y, flip), motion);
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
        );
        assert!(scene.in_transition());
        scene.render_view(
            &state,
            1_001_700,
            &manual,
            (160, 100),
            &UNLOCKS,
            &NO_PROJECTS,
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
                        &NO_PROJECTS
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
        );
        assert_ne!(damage, Some(Damage::full((240, 145))));
        let (small, _) =
            scene.render_view(&state, 1_001_000, &manual, (80, 60), &UNLOCKS, &NO_PROJECTS);
        assert_eq!(small.len(), 80 * 60 * 4);
    }
}
