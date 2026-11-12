const DAY: u64 = 24 * 60 * 60;
pub const LOTS: usize = 8;
pub const MAX_HELP_MINUTES: u16 = 120;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    #[default]
    Empty,
    Cabin,
    Stone,
    Brick,
    Treehouse,
    Glasshouse,
    Waterside,
    RainShelter,
    Barn,
    Lookout,
    Bakery,
    Greenhouse,
    Workshop,
}

pub const BLUEPRINTS: [Kind; 12] = [
    Kind::Cabin,
    Kind::Stone,
    Kind::Brick,
    Kind::Treehouse,
    Kind::Glasshouse,
    Kind::Waterside,
    Kind::RainShelter,
    Kind::Barn,
    Kind::Lookout,
    Kind::Bakery,
    Kind::Greenhouse,
    Kind::Workshop,
];

struct Blueprint {
    label: &'static str,
    days: u8,
    shift_start: u16,
    shift_minutes: u16,
    milestones: [u8; 3],
}

impl Kind {
    pub fn style(self) -> u8 {
        self as u8
    }

    pub fn from_style(style: u8) -> Option<Self> {
        if style == 0 {
            return Some(Self::Empty);
        }
        BLUEPRINTS.get(usize::from(style - 1)).copied()
    }

    pub fn label(self) -> &'static str {
        self.blueprint().label
    }

    pub fn days(self) -> u8 {
        self.blueprint().days
    }

    pub fn shift(self) -> (u16, u16) {
        let plan = self.blueprint();
        (plan.shift_start, plan.shift_start + plan.shift_minutes)
    }

    fn blueprint(self) -> Blueprint {
        let (label, days, hour, minute, shift_minutes, milestones) = match self {
            Self::Empty => ("空地", 0, 0, 0, 0, [0, 0, 0]),
            Self::Cabin => ("小木屋", 20, 9, 10, 26, [15, 50, 73]),
            Self::Stone => ("石屋", 26, 10, 20, 32, [27, 59, 81]),
            Self::Brick => ("红砖屋", 24, 13, 35, 24, [19, 56, 80]),
            Self::Treehouse => ("树屋", 17, 14, 10, 24, [14, 51, 73]),
            Self::Glasshouse => ("玻璃花房", 13, 11, 5, 22, [13, 43, 68]),
            Self::Waterside => ("水边小屋", 16, 8, 50, 25, [24, 53, 76]),
            Self::RainShelter => ("雨亭", 7, 15, 10, 15, [12, 46, 69]),
            Self::Barn => ("谷仓", 19, 9, 45, 23, [20, 48, 74]),
            Self::Lookout => ("瞭望塔", 25, 10, 57, 29, [22, 62, 84]),
            Self::Bakery => ("面包房", 21, 13, 0, 23, [18, 50, 72]),
            Self::Greenhouse => ("温室", 15, 15, 45, 20, [15, 41, 72]),
            Self::Workshop => ("工坊", 11, 16, 20, 22, [16, 47, 66]),
        };
        Blueprint {
            label,
            days,
            shift_start: hour * 60 + minute,
            shift_minutes,
            milestones,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Project {
    pub kind: Kind,
    pub start_age: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Planned,
    Foundation,
    Frame,
    Roof,
    Finish,
    Complete,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Planned => "备料",
            Self::Foundation => "地基",
            Self::Frame => "墙体与框架",
            Self::Roof => "屋顶",
            Self::Finish => "门窗与收尾",
            Self::Complete => "竣工",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub lot: usize,
    pub x: i32,
    pub y: i32,
    pub kind: Kind,
    pub stage: Stage,
    pub percent: u8,
    pub working: bool,
}

// Fixed ground locations. The minimum viewport dimensions also reserve room
// for the full building sprite and keep the expansion sites on the lawn.
const SITES: [(i32, i32, i32, i32); LOTS] = [
    (150, 82, 88, 68),
    (224, 82, 127, 62),
    (130, 143, 35, 126),
    (140, 166, 81, 149),
    (314, 82, 215, 66),
    (351, 82, 252, 68),
    (148, 180, 131, 161),
    (390, 82, 292, 65),
];

fn position(lot: usize, (width, height): (i32, i32)) -> Option<(i32, i32)> {
    let (min_width, min_height, x, y) = SITES[lot];
    if width < min_width || height < min_height {
        return None;
    }
    Some((x, y - (100 - height).max(0)))
}

pub fn first_available(size: (i32, i32), projects: &[Project; LOTS]) -> Option<usize> {
    projects.iter().enumerate().find_map(|(lot, project)| {
        (project.kind == Kind::Empty && position(lot, size).is_some()).then_some(lot)
    })
}

#[cfg(test)]
pub fn visible(
    age: u64,
    local_seconds: u64,
    size: (i32, i32),
    projects: &[Project; LOTS],
) -> impl Iterator<Item = Sample> + '_ {
    visible_with_help(age, local_seconds, size, projects, &[0; LOTS])
}

pub fn visible_with_help<'a>(
    age: u64,
    local_seconds: u64,
    size: (i32, i32),
    projects: &'a [Project; LOTS],
    work_minutes: &'a [u16; LOTS],
) -> impl Iterator<Item = Sample> + 'a {
    projects
        .iter()
        .copied()
        .enumerate()
        .filter_map(move |(lot, project)| {
            let (x, y) = position(lot, size)?;
            let (stage, percent, working) =
                progress(project, age, local_seconds % DAY, work_minutes[lot]);
            Some(Sample {
                lot,
                x,
                y,
                kind: project.kind,
                stage,
                percent,
                working,
            })
        })
}

fn progress(
    project: Project,
    age: u64,
    local_seconds: u64,
    help_minutes: u16,
) -> (Stage, u8, bool) {
    if project.kind == Kind::Empty || age < project.start_age {
        return (Stage::Planned, 0, false);
    }
    let plan = project.kind.blueprint();
    let elapsed = age - project.start_age;
    if elapsed >= u64::from(plan.days) * DAY {
        return (Stage::Complete, 100, false);
    }
    let shift_start = u64::from(plan.shift_start) * 60;
    let shift_length = u64::from(plan.shift_minutes) * 60;
    let local_start = (local_seconds + DAY - elapsed % DAY) % DAY;
    let scheduled = work_until(local_start + elapsed, shift_start, shift_length)
        - work_until(local_start, shift_start, shift_length);
    let worked = scheduled + u64::from(help_minutes.min(MAX_HELP_MINUTES)) * 60;
    let goal = u64::from(plan.days) * shift_length;
    let percent = (worked * 100 / goal).min(99) as u8;
    let [foundation, frame, roof] = plan.milestones;
    let stage = if worked == 0 {
        Stage::Planned
    } else if percent < foundation {
        Stage::Foundation
    } else if percent < frame {
        Stage::Frame
    } else if percent < roof {
        Stage::Roof
    } else {
        Stage::Finish
    };
    let working =
        worked < goal && (shift_start..shift_start + shift_length).contains(&local_seconds);
    (stage, percent, working)
}

fn work_until(local: u64, start: u64, length: u64) -> u64 {
    local / DAY * length + (local % DAY).saturating_sub(start).min(length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{Anchor, Clock};

    #[test]
    fn shifts_pause_at_night_and_preserve_progress() {
        let cabin = Project {
            kind: Kind::Cabin,
            start_age: 0,
        };
        let morning = (9 * 60 + 10) * 60;
        assert_eq!(progress(cabin, 0, morning, 0), (Stage::Planned, 0, true));
        let at_noon = progress(cabin, 12 * 3600 - morning, 12 * 3600, 0);
        let at_night = progress(cabin, 23 * 3600 - morning, 23 * 3600, 0);
        assert_eq!(at_noon, (Stage::Foundation, 5, false));
        assert_eq!(at_night, at_noon);
        assert_eq!(
            progress(cabin, DAY, morning, 0),
            (Stage::Foundation, 5, true)
        );
    }

    #[test]
    fn twenty_day_cabin_is_done_after_five_real_days_offline_at_default_speed() {
        let anchor = Anchor::decode(1_000_000, 5_000_000, 15 * 3600, 4).unwrap();
        let cabin = Project {
            kind: Kind::Cabin,
            start_age: 0,
        };
        assert_ne!(
            progress(cabin, 20 * DAY - 1, 15 * 3600 - 1, 0).0,
            Stage::Complete
        );
        let reopened = Clock::load(anchor.real + 5 * DAY, 0, Some(anchor)).time();
        let elapsed = reopened.seconds - anchor.world.seconds;
        assert!(elapsed >= 20 * DAY);
        assert_eq!(
            progress(cabin, elapsed, reopened.seconds_of_day, 0),
            (Stage::Complete, 100, false)
        );
    }

    #[test]
    fn blueprint_ids_roundtrip_and_sizes_reveal_stable_lots() {
        let mut projects = [Project::default(); LOTS];
        let compact = (160, 100);
        let large = (400, 200);
        for (index, kind) in BLUEPRINTS.iter().copied().enumerate() {
            assert_eq!(usize::from(kind.style()), index + 1);
            assert_eq!(Kind::from_style(kind.style()), Some(kind));
            assert!(kind.days() > 0);
        }
        assert_eq!(first_available(compact, &projects), Some(0));
        projects[0] = Project {
            kind: Kind::Cabin,
            start_age: 12,
        };
        assert_eq!(first_available(compact, &projects), None);
        assert_eq!(first_available(large, &projects), Some(1));
        assert_eq!(visible(13, 37_000, compact, &projects).count(), 1);
        assert_eq!(visible(13, 37_000, large, &projects).count(), LOTS);
        assert_eq!(
            visible(13, 37_000, compact, &projects).next(),
            visible(13, 37_000, large, &projects).next()
        );
    }

    #[test]
    fn visitors_advance_construction_without_skipping_its_calendar_deadline() {
        let mut projects = [Project::default(); LOTS];
        projects[0] = Project {
            kind: Kind::RainShelter,
            start_age: 0,
        };
        let work = [u16::MAX; LOTS];
        let sample = |age| {
            visible_with_help(age, age % DAY, (160, 100), &projects, &work)
                .next()
                .unwrap()
        };
        let unhelped = visible(DAY, 0, (160, 100), &projects).next().unwrap();
        assert!(sample(DAY).percent > unhelped.percent);
        assert_eq!(sample(7 * DAY - 1).percent, 99);
        assert_ne!(sample(7 * DAY - 1).stage, Stage::Complete);
        assert_eq!(sample(7 * DAY).stage, Stage::Complete);
    }
}
