//! Small, time-derived garden. The only saved values are one reveal age per site.

use crate::simulation::{Season, Weather};

const HOUR: u64 = 3_600;
const DAY: u64 = 24 * HOUR;
const STORM_CYCLE: u64 = 24 * DAY;
const GROWING_SITES: usize = 12;
pub const SITE_COUNT: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Kind {
    Daisy,
    Tulip,
    Lavender,
    Sunflower,
    Oak,
    Maple,
    Pine,
    Apple,
    Carrot,
    Radish,
    Lettuce,
    Pumpkin,
    Planter,
    ToolShed,
    Pond,
    Greenhouse,
}

const KINDS: [Kind; SITE_COUNT] = [
    Kind::Daisy,
    Kind::Tulip,
    Kind::Lavender,
    Kind::Sunflower,
    Kind::Oak,
    Kind::Maple,
    Kind::Pine,
    Kind::Apple,
    Kind::Carrot,
    Kind::Radish,
    Kind::Lettuce,
    Kind::Pumpkin,
    Kind::Planter,
    Kind::ToolShed,
    Kind::Pond,
    Kind::Greenhouse,
];

impl Kind {
    pub fn species_index(self) -> Option<usize> {
        let index = self as usize;
        (index < GROWING_SITES).then_some(index)
    }

    pub fn label(self) -> &'static str {
        [
            "菊花",
            "郁金香",
            "薰衣草",
            "向日葵",
            "橡树",
            "枫树",
            "松树",
            "苹果树",
            "胡萝卜",
            "萝卜",
            "生菜",
            "南瓜",
            "木花槽",
            "工具棚",
            "小池塘",
            "温室",
        ][self as usize]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Stage {
    Seed,
    Sprout,
    Growing,
    Budding,
    Mature,
    Harvest,
    Groundwork,
    Framing,
    Complete,
    Damaged,
    Repairing,
    Recovering,
}

const GROWTH_STAGES: [Stage; 6] = [
    Stage::Seed,
    Stage::Sprout,
    Stage::Growing,
    Stage::Budding,
    Stage::Mature,
    Stage::Harvest,
];

impl Stage {
    pub fn growth_index(self) -> Option<usize> {
        let index = self as usize;
        (index < GROWTH_STAGES.len()).then_some(index)
    }

    pub fn response_index(self) -> Option<usize> {
        (self as usize).checked_sub(Stage::Damaged as usize)
    }

    pub fn label(self) -> &'static str {
        [
            "播种", "发芽", "生长", "现蕾", "成熟", "收获", "整地", "搭建", "完工", "受损", "修复",
            "恢复",
        ][self as usize]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Event {
    None,
    Storm,
    Frost,
    Drought,
    Flood,
}

impl Event {
    pub fn disaster_index(self) -> Option<usize> {
        (self as usize).checked_sub(1)
    }

    pub fn label(self) -> &'static str {
        ["", "风暴", "霜冻", "干旱", "积涝"][self as usize]
    }

    fn response_hours(self) -> [u64; 3] {
        match self {
            Self::None => [0, 0, 0],
            Self::Storm => [10, 14, 10],
            Self::Frost => [14, 12, 12],
            Self::Drought => [18, 12, 10],
            Self::Flood => [16, 20, 12],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Plot {
    pub id: u8,
    pub x: usize,
    pub y: usize,
    pub kind: Kind,
    pub stage: Stage,
    pub progress: u8,
    pub event: Event,
    pub working: bool,
}

impl Plot {
    pub fn site_label(self) -> &'static str {
        [
            "左侧前院",
            "石径前沿",
            "软垫前沿",
            "东侧内畦",
            "东侧外畦",
            "东侧向阳畦",
            "东侧斜径畦",
            "东侧尽头",
            "南侧左畦",
            "南侧中畦",
            "南侧里畦",
            "南侧外畦",
            "东侧花槽工地",
            "南侧工具棚工地",
            "东侧池塘工地",
            "南侧温室工地",
        ][self.id as usize]
    }

    pub fn job_label(self) -> &'static str {
        match self.stage {
            Stage::Seed => "翻土播种",
            Stage::Sprout | Stage::Growing => "查看幼苗",
            Stage::Budding | Stage::Mature => "照看花果",
            Stage::Harvest => "收拾花果和种子",
            Stage::Groundwork => "打地基",
            Stage::Framing => "搭建设施",
            Stage::Repairing => match self.event {
                Event::Storm => "清理断枝和碎木",
                Event::Frost => "剪掉冻坏的枝叶",
                Event::Drought => "补水和换土",
                Event::Flood => "排水和清淤",
                Event::None => "修理设施",
            },
            Stage::Recovering => "补种或加固",
            _ => "巡视庭院",
        }
    }
}

const EMPTY_PLOT: Plot = Plot {
    id: 0,
    x: 0,
    y: 0,
    kind: Kind::Daisy,
    stage: Stage::Seed,
    progress: 0,
    event: Event::None,
    working: false,
};

pub struct Garden {
    plots: [Plot; SITE_COUNT],
    count: usize,
}

impl Garden {
    pub fn plots(&self) -> &[Plot] {
        &self.plots[..self.count]
    }

    pub fn job(&self) -> Option<&Plot> {
        self.plots()
            .iter()
            .filter(|plot| plot.working)
            .max_by_key(|plot| match plot.stage {
                Stage::Repairing => 4,
                Stage::Harvest => 3,
                Stage::Groundwork | Stage::Framing => 2,
                Stage::Seed | Stage::Recovering => 1,
                _ => 0,
            })
    }
}

#[derive(Clone, Copy)]
enum Region {
    Original,
    East(usize),
    South(usize),
}

use Region::{East, Original, South};

// Each layout slot keeps its identity when the viewport changes.
const SITES: [(Region, usize, usize); SITE_COUNT] = [
    (Original, 24, 87),
    (Original, 69, 87),
    (Original, 112, 88),
    (East(188), 99, 74),
    (East(208), 119, 84),
    (East(232), 143, 72),
    (East(252), 163, 85),
    (East(276), 187, 75),
    (South(123), 28, 103),
    (South(123), 70, 103),
    (South(143), 106, 123),
    (South(143), 134, 123),
    (East(208), 109, 59),
    (South(170), 48, 151),
    (East(270), 180, 58),
    (South(196), 118, 173),
];

fn position(id: usize, width: usize, height: usize) -> Option<(usize, usize)> {
    let (region, mut x, mut y) = SITES[id];
    match region {
        East(minimum) if width < minimum => return None,
        South(minimum) if height < minimum => return None,
        _ => {}
    }
    if !matches!(region, East(_)) && x >= 85 {
        x = x.saturating_add(width.saturating_sub(160));
    }
    if !matches!(region, South(_)) {
        y = y.checked_sub(100usize.saturating_sub(height))?;
    }
    (x >= 13 && x.saturating_add(13) < width && y >= 11 && y.saturating_add(7) < height)
        .then_some((x, y))
}

pub fn visible_sites(width: usize, height: usize) -> u16 {
    (0..SITE_COUNT).fold(0, |mask, id| {
        mask | ((position(id, width, height).is_some() as u16) << id)
    })
}

// Durations per real phase, in hours. Every species visits each plot over rotations.
const FLOWER: [u64; 6] = [1, 8, 24, 16, 36, 11];
const TREE: [u64; 6] = [8, 24, 48, 48, 72, 16];
const CROP: [u64; 6] = [2, 12, 30, 24, 36, 16];
const ROTATION_HOURS: u64 = 72 * 24;
const FIRST_SPECIES: [usize; GROWING_SITES] = [0, 8, 5, 1, 6, 9, 2, 10, 3, 7, 11, 4];
const FIRST_SITE_AGE: [u64; 3] = [0, 6 * HOUR, 39 * HOUR];

fn percent(age: u64, total: u64) -> u8 {
    (age.min(total) * 100 / total) as u8
}

fn growth(mut age: u64, first: usize) -> (Kind, Stage, u8) {
    age %= ROTATION_HOURS * HOUR;
    for number in 0..GROWING_SITES {
        let species = (first + number) % GROWING_SITES;
        let spans = match species / 4 {
            0 => FLOWER,
            1 => TREE,
            _ => CROP,
        };
        for (stage, hours) in GROWTH_STAGES.into_iter().zip(spans) {
            let span = hours * HOUR;
            if age < span {
                return (KINDS[species], stage, percent(age, span));
            }
            age -= span;
        }
    }
    unreachable!("the rotation covers every species and stage")
}

#[derive(Clone, Copy)]
struct Disaster {
    at: u64,
    window: u64,
    event: Event,
}

impl Disaster {
    fn ends(self) -> u64 {
        self.at
            .saturating_add(self.event.response_hours().into_iter().sum::<u64>() * HOUR)
    }

    fn response(self, age: u64) -> Option<(Stage, u8)> {
        let mut elapsed = age.checked_sub(self.at)?;
        let stages = [Stage::Damaged, Stage::Repairing, Stage::Recovering];
        for (stage, hours) in stages.into_iter().zip(self.event.response_hours()) {
            let span = hours * HOUR;
            if elapsed < span {
                return Some((stage, percent(elapsed, span)));
            }
            elapsed -= span;
        }
        None
    }

    fn replacement(self, id: usize, seed: u64) -> usize {
        (mix(self.window ^ seed ^ (id as u64 + 1).wrapping_mul(137)) % GROWING_SITES as u64)
            as usize
    }
}

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn disaster_at(window: u64, seed: u64) -> Disaster {
    let roll = mix(seed ^ window.wrapping_mul(0x9e3779b97f4a7c15));
    let offset = 14 * DAY + roll % (4 * DAY + 1);
    Disaster {
        at: (window * STORM_CYCLE).saturating_add(offset),
        window,
        event: [Event::Storm, Event::Frost, Event::Drought, Event::Flood]
            [(roll >> 32) as usize % 4],
    }
}

fn last_disaster(id: usize, age: u64, revealed: u64, seed: u64) -> Option<Disaster> {
    // Each calamity affects one starter plot, one extension plot and one worksite.
    let (offset, count) = match id {
        0..=2 => (0, 3),
        3..=11 => (3, 9),
        _ => (12, 4),
    };
    let current = age / STORM_CYCLE;
    let residue = ((id - offset) as u64 + seed % count) % count;
    let difference = (current % count + count - residue) % count;
    let mut window = current.checked_sub(difference)?;
    let mut disaster = disaster_at(window, seed);
    if disaster.at > age {
        window = window.checked_sub(count)?;
        disaster = disaster_at(window, seed);
    }
    (disaster.at > revealed).then_some(disaster)
}

fn normal_plant(id: usize, age: u64, revealed: u64, seed: u64) -> (Kind, Stage, u8) {
    if let Some(event) = last_disaster(id, age, revealed, seed) {
        return growth(
            age.saturating_sub(event.ends()),
            event.replacement(id, seed),
        );
    }
    let initial = FIRST_SITE_AGE.get(id).copied().unwrap_or(0);
    growth(
        age.saturating_sub(revealed).saturating_add(initial),
        FIRST_SPECIES[id],
    )
}

const SHIFT: u64 = 12 * HOUR;
const WORK_PER_SHIFT: u64 = 24 * 60;
const WORK_BUDGET: [u64; 4] = [50 * 60, 100 * 60, 150 * 60, 180 * 60];
const WORK_DELAY: [u64; 4] = [45 * 60, 3 * HOUR, 5 * HOUR, 8 * HOUR];

fn construction_clock(age: u64) -> (u64, bool) {
    let into_shift = age % SHIFT;
    let total = age / SHIFT * WORK_PER_SHIFT + into_shift.min(WORK_PER_SHIFT);
    (total, into_shift < WORK_PER_SHIFT)
}

fn building(id: usize, age: u64, revealed: u64, disaster: Option<Disaster>) -> (Stage, u8, bool) {
    let index = id - GROWING_SITES;
    let budget = WORK_BUDGET[index];
    let starts = revealed.saturating_add(WORK_DELAY[index]);
    let (time, working) = if let Some(disaster) = disaster {
        let done = construction_clock(disaster.at.saturating_sub(starts))
            .0
            .min(budget);
        let lost = done / 4;
        let resume = disaster.ends().max(starts);
        let (repair, working) = construction_clock(age.saturating_sub(resume));
        (done - lost + repair, age >= resume && working)
    } else {
        let (done, working) = construction_clock(age.saturating_sub(starts));
        (done, age >= starts && working)
    };
    let progress = percent(time, budget);
    let stage = match progress {
        0..=33 => Stage::Groundwork,
        34..=99 => Stage::Framing,
        _ => Stage::Complete,
    };
    (stage, progress, working && stage != Stage::Complete)
}

fn update_plot(plot: &mut Plot, age: u64, revealed: u64, seed: u64, open_air: bool) {
    let id = plot.id as usize;
    let planting = id < GROWING_SITES;
    let disaster = last_disaster(id, age, revealed, seed);
    let maintenance =
        (age.saturating_sub(revealed) % (2 * HOUR) + id as u64 * 97) % (2 * HOUR) < 34;
    if let Some(disaster) = disaster {
        if let Some((response, progress)) = disaster.response(age) {
            if planting {
                plot.kind = if response == Stage::Recovering {
                    KINDS[disaster.replacement(id, seed)]
                } else {
                    normal_plant(id, disaster.at.saturating_sub(1), revealed, seed).0
                };
            } else {
                plot.kind = KINDS[id];
            }
            plot.event = disaster.event;
            plot.stage = response;
            plot.progress = progress;
            plot.working = response != Stage::Damaged && maintenance && open_air;
            return;
        }
    }
    if planting {
        (plot.kind, plot.stage, plot.progress) = normal_plant(id, age, revealed, seed);
        plot.working = maintenance && open_air;
    } else {
        plot.kind = KINDS[id];
        (plot.stage, plot.progress, plot.working) = building(id, age, revealed, disaster);
        plot.working &= open_air;
    }
}

pub fn plots(
    now_secs: u64,
    started: u64,
    width: usize,
    height: usize,
    season: Season,
    weather: Weather,
) -> Garden {
    plots_with_unlocks(
        now_secs,
        started,
        width,
        height,
        season,
        weather,
        &[1; SITE_COUNT],
    )
}

pub fn plots_with_unlocks(
    now_secs: u64,
    started: u64,
    width: usize,
    height: usize,
    season: Season,
    weather: Weather,
    unlocked_at: &[u64; SITE_COUNT],
) -> Garden {
    let age = now_secs.saturating_sub(started);
    let seed = mix(started ^ 0xd6e8feb86659fd93);
    let open_air =
        weather != Weather::Snow && !(weather == Weather::Rain && season == Season::Winter);
    let mut garden = Garden {
        plots: [EMPTY_PLOT; SITE_COUNT],
        count: 0,
    };
    for (id, saved) in unlocked_at.iter().enumerate() {
        let Some((x, y)) = position(id, width, height) else {
            continue;
        };
        let revealed = saved.checked_sub(1).unwrap_or(age).min(age);
        let mut plot = Plot {
            id: id as u8,
            x,
            y,
            ..EMPTY_PLOT
        };
        update_plot(&mut plot, age, revealed, seed, open_air);
        garden.plots[garden.count] = plot;
        garden.count += 1;
    }
    garden
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_axes_reveal_different_sites_without_moving_original_plot_identity() {
        assert_eq!(visible_sites(160, 100), 0b111);
        assert_eq!(visible_sites(300, 100), 0b0101_0000_1111_1111);
        assert_eq!(visible_sites(160, 200), 0b1010_1111_0000_0111);
        assert_eq!(visible_sites(300, 200), u16::MAX);
        let day = 2 * DAY;
        let first = plots(day, 0, 160, 100, Season::Spring, Weather::Clear);
        let wide = plots(day, 0, 300, 200, Season::Spring, Weather::Clear);
        for (old, expanded) in first.plots().iter().zip(wide.plots()) {
            assert_eq!(
                (old.id, old.kind, old.stage),
                (expanded.id, expanded.kind, expanded.stage)
            );
        }
    }

    #[test]
    fn new_site_starts_at_reveal_and_keeps_aging_if_hidden() {
        let now = 30 * DAY;
        let mut reveal = [1; SITE_COUNT];
        reveal[3] = now + 1;
        let wide =
            |age| plots_with_unlocks(age, 0, 200, 100, Season::Spring, Weather::Clear, &reveal);
        assert_eq!(wide(now).plots()[3].stage, Stage::Seed);
        assert_eq!(wide(now + 2 * HOUR).plots()[3].stage, Stage::Sprout);
        assert_eq!(visible_sites(160, 100) & (1 << 3), 0);
        assert_eq!(wide(now + DAY).plots()[3].stage, Stage::Growing);
    }

    #[test]
    fn calamities_show_repair_and_replant_without_changing_other_sites() {
        let seed = mix(0xd6e8feb86659fd93);
        let event = disaster_at(0, seed);
        let affected = (3 - seed % 3) as usize % 3;
        let before = plots(event.at - 1, 0, 160, 100, Season::Spring, Weather::Clear);
        let at = plots(event.at, 0, 160, 100, Season::Spring, Weather::Clear);
        assert_eq!(at.plots()[affected].event, event.event);
        assert_eq!(at.plots()[affected].stage, Stage::Damaged);
        assert_eq!(at.plots()[affected].kind, before.plots()[affected].kind);
        assert_eq!(
            at.plots()
                .iter()
                .filter(|plot| plot.event != Event::None)
                .count(),
            1
        );
        let hours = event.event.response_hours();
        let repair = plots(
            event.at + hours[0] * HOUR,
            0,
            160,
            100,
            Season::Spring,
            Weather::Clear,
        );
        assert_eq!(repair.plots()[affected].stage, Stage::Repairing);
        let reset = plots(event.ends(), 0, 160, 100, Season::Spring, Weather::Clear);
        assert_eq!(reset.plots()[affected].stage, Stage::Seed);
    }

    #[test]
    fn construction_advances_only_in_short_site_shifts() {
        let starts = WORK_DELAY[0];
        let in_shift = building(12, starts + 600, 0, None);
        let after_shift = building(12, starts + WORK_PER_SHIFT, 0, None);
        let overnight = building(12, starts + 10 * HOUR, 0, None);
        assert!(in_shift.2);
        assert!(in_shift.1 < after_shift.1);
        assert_eq!(after_shift, overnight);
        assert!(building(12, starts + SHIFT + 600, 0, None).1 > overnight.1);
    }
}
