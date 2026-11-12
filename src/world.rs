use std::time::{SystemTime, UNIX_EPOCH};

use crate::activities::{self, Activity, Motion, Prop};
use crate::simulation::{CareAction, CatAction, DayPart, Season, Snapshot, Visitor, Weather};

const WIDTH: usize = 160;
const HEIGHT: usize = 100;
const FRAME_BYTES: usize = WIDTH * HEIGHT * 4;
const SPRITE_W: usize = 48;
const SPRITE_H: usize = 32;
const SPRITE_BYTES: usize = SPRITE_W * SPRITE_H * 4;
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

pub fn local_minutes() -> u16 {
    let mut time = LocalTime::default();
    unsafe { GetLocalTime(&mut time) };
    time.hour * 60 + time.minute
}

#[derive(Clone, Copy, Debug)]
pub struct ManualActions {
    pub water_bowl: bool,
    pub lantern: Option<bool>,
    pub keeper_called_at: Option<u64>,
    pub visitor_snack_at: Option<u64>,
    pub invited_visitor: Visitor,
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
        && [
            CAT_IDLE, CAT_SLEEP, CAT_WALK, CAT_EAT, CAT_TREAT, KEEPER, SPARROW, GREY_CAT, BLACK_CAT,
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
    };
    let progress = match state.build_stage {
        0 => format!("花圃 {}%", state.build_percent),
        1 => format!("工作台 {}%", state.build_percent),
        2 => format!("凉亭 {}%", state.build_percent),
        _ => "庭院已建好".to_owned(),
    };
    format!("{time} · {weather} · {cat} · {activity} · {progress}{guest}")
}

pub fn notebook(state: &Snapshot) -> String {
    let active = activities::current(state);
    let mut text = status_line(state).replace(" · ", "\n");
    for (label, activity) in [
        ("猫咪", active.cat),
        ("管理员", active.keeper),
        ("来客", active.visitor),
        ("庭院", active.yard),
    ] {
        if let Some(activity) = activity {
            text.push_str(&format!("\n\n{label}正在：{}", activity.name));
        }
    }
    if state.build_stage < 3 {
        text.push_str("\n\n施工时间  08:10–08:18  ·  17:05–17:15");
    }
    text
}

pub struct Scene {
    pixels: Box<[u8]>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            pixels: vec![0; FRAME_BYTES].into_boxed_slice(),
        }
    }

    pub fn render_with_actions(
        &mut self,
        state: &Snapshot,
        now_ms: u64,
        actions: &ManualActions,
    ) -> &[u8] {
        let daylight = match state.day_part {
            DayPart::Dawn => 0,
            DayPart::Day => 1,
            DayPart::Dusk => 2,
            DayPart::Night => 3,
        };
        self.pixels
            .copy_from_slice(frame(SCENES, FRAME_BYTES, daylight));
        self.overlay(frame(
            BUILDS,
            FRAME_BYTES,
            state.build_stage.min(3) as usize,
        ));
        if state.build_in_progress && state.build_stage < 3 {
            let progress = usize::from(state.build_stage) * 3
                + usize::from(state.build_percent.min(99)) * 3 / 100;
            self.overlay(frame(CONSTRUCTION, FRAME_BYTES, progress));
        }
        self.apply_season(state.season);
        if let Some(lit) = actions.lantern {
            self.porch_light(lit);
        }
        self.weather(state.weather, state.day_part, now_ms);
        let now = now_ms / 1000;
        let active = activities::current(state);
        if let Some(activity) = active.yard {
            let (x, y, _) = pose(activity, now_ms);
            self.prop(activity.prop, x, y, now_ms);
        }
        if actions.water_bowl {
            self.prop(Prop::Water, 86, 84, now_ms);
        }
        if recent(actions.visitor_snack_at, now, 180) {
            self.prop(Prop::Food, 140, 80, now_ms);
        }
        self.people(state, active.keeper, active.visitor, now_ms);
        self.cat(state, active.cat, now_ms, actions);
        for (x, y) in [
            (143, 92),
            (146, 89),
            (146, 92),
            (149, 86),
            (149, 89),
            (149, 92),
        ] {
            self.point(x, y + 1, [59, 62, 67], 210);
            self.point(x, y, [229, 213, 177], 235);
        }
        &self.pixels
    }

    fn overlay(&mut self, layer: &[u8]) {
        for (target, pixel) in self.pixels.chunks_exact_mut(4).zip(layer.chunks_exact(4)) {
            over(target, pixel);
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
            for sx in 0..SPRITE_W {
                let px = if flip { SPRITE_W - sx - 1 } else { sx };
                let offset = (sy * SPRITE_W + px) * 4;
                let pixel = &sprite[offset..offset + 4];
                if pixel[3] != 0 {
                    let lean = i32::from(motion == Motion::Tilt) * ((29 - sy as i32) / 6);
                    let crouch = if motion == Motion::Nap {
                        (29 - sy as i32) / 3
                    } else {
                        0
                    };
                    self.pixel(left + sx as i32 + lean, top + sy as i32 + crouch, pixel);
                }
            }
        }
    }

    fn pixel(&mut self, x: i32, y: i32, color: &[u8]) {
        if (0..WIDTH as i32).contains(&x) && (0..HEIGHT as i32).contains(&y) {
            let offset = (y as usize * WIDTH + x as usize) * 4;
            over(&mut self.pixels[offset..offset + 4], color);
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
                let offset = ((ms / 9_000) % 12) as i32;
                for (line, (left, right)) in [(2, 10), (0, 17), (0, 20), (4, 17)].iter().enumerate()
                {
                    for col in *left..*right {
                        self.point(107 + col + offset, 18 + line as i32, [212, 216, 226], dim);
                    }
                }
            }
            Weather::Rain => {
                let tick = ms / 240;
                for n in 0..16 {
                    let x = 12 + ((n * 31 + n * n * 3 + tick / 4) % 142) as i32;
                    let y = 27 + ((n * 19 + tick * 5) % 61) as i32;
                    self.point(x, y, [177, 213, 240], 107);
                    self.point(x - 1, y + 1, [177, 213, 240], 73);
                    self.point(x - 1, y + 2, [177, 213, 240], 44);
                }
            }
            Weather::Snow => {
                let tick = ms / 300;
                for n in 0..17 {
                    let x = 14 + ((n * 31 + n * n + tick / 3) % 138) as i32;
                    let y = 22 + ((n * 17 + tick * (n % 2 + 1)) % 65) as i32;
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
                self.point(x, y, if y == 60 || x == 35 { frame } else { glass }, 255);
            }
        }
        self.point(35, 64, frame, 255);
    }

    fn people(
        &mut self,
        state: &Snapshot,
        keeper: Option<&Activity>,
        visitor: Option<&Activity>,
        ms: u64,
    ) {
        if let Some(activity) = keeper {
            let (mut x, mut y, flip) = pose(activity, ms);
            if state.caretaker == CareAction::Building {
                let (dx, dy) = match state.build_stage {
                    0 => (5, 13),
                    1 => (-18, 0),
                    _ => (0, 0),
                };
                x += dx;
                y += dy;
                if activity.prop == Prop::Tool {
                    self.prop(Prop::Spark, x + 9, y - 13, ms);
                }
            }
            self.sprite(KEEPER, (ms / 560) as usize, (x, y, flip), activity.motion);
            self.actor_prop(activity.prop, x, y, flip, ms);
        }
        if let Some(activity) = visitor {
            let guest = match state.visitor {
                Visitor::None => return,
                Visitor::Sparrow => SPARROW,
                Visitor::GreyCat => GREY_CAT,
                Visitor::BlackCat => BLACK_CAT,
            };
            let (x, y, flip) = pose(activity, ms);
            self.sprite(guest, (ms / 680) as usize, (x, y, !flip), activity.motion);
            self.actor_prop(activity.prop, x, y, !flip, ms);
        }
    }

    fn cat(
        &mut self,
        state: &Snapshot,
        activity: Option<&Activity>,
        ms: u64,
        actions: &ManualActions,
    ) {
        let Some(activity) = activity else { return };
        let (mut x, mut y, mut flip) = pose(activity, ms);
        let (mut sheet, pace) = match state.cat {
            CatAction::Idle => (CAT_IDLE, 850),
            CatAction::Sleep => (CAT_SLEEP, 1_700),
            CatAction::Walk => (CAT_WALK, 170),
            CatAction::Eat => (CAT_EAT, 260),
            CatAction::Treat => (CAT_TREAT, 260),
        };
        let mut motion = activity.motion;
        if cat_heard_window(state, ms / 1000, actions) {
            (x, y, flip) = (51, 75, true);
            sheet = CAT_IDLE;
            motion = Motion::Tilt;
            self.prop(Prop::Bell, 40, 57, ms);
        } else {
            self.actor_prop(activity.prop, x, y, flip, ms);
            if state.cat == CatAction::Eat && !matches!(activity.prop, Prop::Food | Prop::Water) {
                self.prop(Prop::Food, x + 11, y + 1, ms);
            }
        }
        if state.cat == CatAction::Sleep {
            motion = Motion::Still;
            let lift = ((ms / 1_400) % 3) as i32;
            self.sleep_mark(x + 9 + lift, y - 15 - lift);
        }
        self.sprite(sheet, (ms / pace) as usize, (x, y, flip), motion);
    }

    fn actor_prop(&mut self, prop: Prop, x: i32, y: i32, flip: bool, ms: u64) {
        let side = if flip { -1 } else { 1 };
        let x = (x + side * 10).clamp(5, 154);
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
    let phase = |duration, steps| ((ms / duration) % steps) as i32;
    let (x, y, flip) = match activity.motion {
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
    };
    (activity.x + x, activity.y + y, flip)
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
            .render_with_actions(&state, ms, &ManualActions::default())
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
            assert_ne!(&baseline, scene.render_with_actions(&state, ms, &actions));
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
                let image = scene.render_with_actions(&state, 1_000_000, &ManualActions::default());
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
}
