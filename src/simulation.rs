//! Small, clock-driven simulation. Sampling the same instant always produces
//! the same world, even when the window was hidden or the program was closed.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DayPart {
    Dawn,
    Day,
    Dusk,
    Night,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Weather {
    Clear,
    Clouds,
    Rain,
    Snow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Season {
    Spring,
    Autumn,
    Winter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeatherMode {
    Auto,
    Clear,
    Rain,
    Snow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatAction {
    Idle,
    Sleep,
    Walk,
    Eat,
    Treat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CareAction {
    Away,
    Feeding,
    Tending,
    Building,
    Resting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visitor {
    None,
    Sparrow,
    GreyCat,
    BlackCat,
    Squirrel,
    Hedgehog,
    Rabbit,
    Frog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub day_part: DayPart,
    /// Time on the world's clock, from midnight through 23:59.
    pub minutes_of_day: u16,
    pub weather: Weather,
    pub season: Season,
    pub cat: CatAction,
    pub caretaker: CareAction,
    pub visitor: Visitor,
    /// Number of completed improvements, from zero to three.
    pub build_stage: u8,
    /// Percentage of the next improvement, or 100 when everything is complete.
    pub build_percent: u8,
    /// Unfinished construction remains visible while the caretaker is off duty.
    pub build_in_progress: bool,
    /// Stable start of this world's virtual clock and seed for its routines.
    pub world_start: u64,
    /// Simulated seconds elapsed since the resident's first day.
    pub phase_seconds: u64,
}

const DAY: u64 = 24 * 60 * 60;
const PREPARATION: u64 = 60 * 60;
const BUILD_MINUTES: [u64; 3] = [6, 24, 65];
const SHIFTS: [(u64, u64); 2] = [
    (8 * 3_600 + 10 * 60, 8 * 3_600 + 18 * 60),
    (17 * 3_600 + 5 * 60, 17 * 3_600 + 15 * 60),
];
const MEALS: [u64; 3] = [8 * 3_600, 12 * 3_600 + 30 * 60, 18 * 3_600 + 35 * 60];
const VISIT_ONBOARDING: std::ops::Range<u64> = 46..118;
const VISIT_BASE: u64 = 1_140;
const VISIT_PERIOD: u64 = 1_680;
const VISIT_JITTER: u64 = 720;
const WEATHER_PERIOD: u64 = 45 * 60;
const CARE_PERIOD: u64 = 32 * 60;

pub fn snapshot(
    now: u64,
    started: u64,
    local_minutes: u16,
    season: Season,
    mode: WeatherMode,
    treat_at: Option<u64>,
) -> Snapshot {
    let age = now.saturating_sub(started);
    let minutes_of_day = local_minutes % 1_440;
    let day_part = day_part(minutes_of_day);
    let weather = weather_at(now, started, season, mode);
    let local_seconds = u64::from(minutes_of_day) * 60 + now % 60;
    let worked = work_elapsed(age.saturating_sub(PREPARATION), local_seconds);
    let build_stage = BUILD_MINUTES
        .iter()
        .take_while(|&&minutes| worked >= minutes * 60)
        .count() as u8;
    let completed_work = match build_stage {
        0 => 0,
        count => BUILD_MINUTES[usize::from(count) - 1] * 60,
    };
    let next_work = BUILD_MINUTES
        .get(usize::from(build_stage))
        .map(|&minutes| minutes * 60);
    let project_worked = worked - completed_work;
    let build_percent = next_work.map_or(100, |target| {
        (100 * project_worked / (target - completed_work)) as u8
    });
    let building = next_work.is_some() && age >= PREPARATION && during_shift(local_seconds);
    let build_in_progress = next_work.is_some() && (building || project_worked > 0);
    let caretaker = caretaker_at(age, local_seconds, day_part, building);
    let visitor = visitor_at(now, age, started, local_seconds, season, mode);
    let cat = cat_at(
        now,
        age,
        minutes_of_day,
        day_part,
        weather,
        caretaker,
        treat_at,
    );

    Snapshot {
        day_part,
        minutes_of_day,
        weather,
        season,
        cat,
        caretaker,
        visitor,
        build_stage,
        build_percent,
        build_in_progress,
        world_start: started,
        phase_seconds: age,
    }
}

fn during_shift(seconds: u64) -> bool {
    SHIFTS
        .iter()
        .any(|&(start, end)| (start..end).contains(&seconds))
}

fn work_before(seconds: u64) -> u64 {
    SHIFTS
        .iter()
        .map(|&(start, end)| seconds.min(end).saturating_sub(start))
        .sum()
}

/// Count only the portions of the work shifts elapsed since construction
/// became eligible. Complete days are counted together, without a day loop.
fn work_elapsed(eligible_age: u64, local_seconds: u64) -> u64 {
    let work_per_day = work_before(DAY);
    let complete_days = eligible_age / DAY;
    let remainder = eligible_age % DAY;
    let partial = if remainder <= local_seconds {
        work_before(local_seconds) - work_before(local_seconds - remainder)
    } else {
        work_before(local_seconds) + work_per_day - work_before(DAY - (remainder - local_seconds))
    };
    complete_days * work_per_day + partial
}

fn day_part(minutes: u16) -> DayPart {
    match minutes % 1_440 {
        300..450 => DayPart::Dawn,
        450..1_080 => DayPart::Day,
        1_080..1_200 => DayPart::Dusk,
        _ => DayPart::Night,
    }
}

fn weather_at(now: u64, seed: u64, season: Season, mode: WeatherMode) -> Weather {
    match mode {
        WeatherMode::Clear => Weather::Clear,
        WeatherMode::Rain => Weather::Rain,
        WeatherMode::Snow => Weather::Snow,
        WeatherMode::Auto => {
            let sample = mix(seed ^ (now / WEATHER_PERIOD)) % 100;
            match (season, sample) {
                (Season::Winter, 0..=27) => Weather::Snow,
                (Season::Autumn, 0..=29) | (Season::Spring, 0..=17) => Weather::Rain,
                (_, 0..=49) => Weather::Clouds,
                _ => Weather::Clear,
            }
        }
    }
}

fn visitor_at(
    now: u64,
    age: u64,
    seed: u64,
    local_seconds: u64,
    season: Season,
    mode: WeatherMode,
) -> Visitor {
    if VISIT_ONBOARDING.contains(&age) {
        return Visitor::Sparrow;
    }
    let Some(regular_age) = age.checked_sub(VISIT_BASE) else {
        return Visitor::None;
    };
    let slot = regular_age / VISIT_PERIOD;
    let in_slot = regular_age % VISIT_PERIOD;
    let event = mix(seed ^ 0x4D87_CDA1_A8CE_D293 ^ slot);
    let arrival = event % VISIT_JITTER;
    let duration = 180 + (event >> 18) % 121;

    if !(arrival..arrival + duration).contains(&in_slot) {
        return Visitor::None;
    }
    // Sample at arrival so a passing shower or dusk leaves a guest's visit intact.
    let elapsed = in_slot - arrival;
    let arrival_local = (local_seconds + DAY - elapsed) % DAY;
    let day = day_part((arrival_local / 60) as u16);
    let arrival_now = now.saturating_sub(elapsed);
    let weather = weather_at(arrival_now, seed, season, mode);
    let recent_rain = mode == WeatherMode::Auto
        && arrival_now % WEATHER_PERIOD < 15 * 60
        && arrival_now
            .checked_sub(WEATHER_PERIOD)
            .is_some_and(|earlier| weather_at(earlier, seed, season, mode) == Weather::Rain);
    choose_visitor(event >> 32, day, weather, recent_rain)
}

fn choose_visitor(roll: u64, day: DayPart, weather: Weather, recent_rain: bool) -> Visitor {
    let dry = matches!(weather, Weather::Clear | Weather::Clouds);
    let candidates = [
        Some(Visitor::Sparrow),
        Some(Visitor::GreyCat),
        Some(Visitor::BlackCat),
        (day == DayPart::Day && dry).then_some(Visitor::Squirrel),
        (matches!(day, DayPart::Dusk | DayPart::Night) && weather != Weather::Snow)
            .then_some(Visitor::Hedgehog),
        (matches!(day, DayPart::Dawn | DayPart::Day) && dry).then_some(Visitor::Rabbit),
        (weather == Weather::Rain || (recent_rain && dry)).then_some(Visitor::Frog),
    ];
    let mut eligible = candidates.into_iter().flatten();
    let index = (roll % eligible.clone().count() as u64) as usize;
    eligible
        .nth(index)
        .expect("three guests are always eligible")
}

fn caretaker_at(age: u64, local_seconds: u64, day: DayPart, building: bool) -> CareAction {
    let meal_now = (15..55).contains(&age)
        || MEALS
            .iter()
            .any(|&meal| (meal..meal + 70).contains(&local_seconds));
    if meal_now {
        return CareAction::Feeding;
    }
    if building {
        return CareAction::Building;
    }
    if during_shift(local_seconds) || (55..115).contains(&age) {
        return CareAction::Tending;
    }
    match (day, age % CARE_PERIOD) {
        (DayPart::Night, 175..250) => CareAction::Resting,
        (DayPart::Night, _) => CareAction::Away,
        (_, 90..175) => CareAction::Tending,
        (_, 175..250) => CareAction::Resting,
        _ => CareAction::Away,
    }
}

fn cat_at(
    now: u64,
    age: u64,
    minutes_of_day: u16,
    day: DayPart,
    weather: Weather,
    caretaker: CareAction,
    treat_at: Option<u64>,
) -> CatAction {
    if treat_at
        .and_then(|time| now.checked_sub(time))
        .is_some_and(|seconds| seconds < 4)
    {
        return CatAction::Treat;
    }
    if caretaker == CareAction::Feeding {
        return CatAction::Eat;
    }
    if day == DayPart::Night
        || (13 * 60..13 * 60 + 35).contains(&minutes_of_day)
        || age % (28 * 60) >= 24 * 60
    {
        return CatAction::Sleep;
    }
    let sheltered = matches!(weather, Weather::Rain | Weather::Snow);
    if !sheltered && (50..67).contains(&(age % (3 * 60))) {
        CatAction::Walk
    } else {
        CatAction::Idle
    }
}

/// Stateless integer mixer for reproducible variations without a random
/// generator, allocations, background work, or a saved schedule.
fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(age: u64, local_minutes: u16) -> Snapshot {
        let started = 1_800_000_000;
        snapshot(
            started + age,
            started,
            local_minutes,
            Season::Spring,
            WeatherMode::Clear,
            None,
        )
    }

    fn at_from(start_minute: u16, minutes: u64) -> Snapshot {
        let local_minute = ((u64::from(start_minute) + minutes) % 1_440) as u16;
        at(minutes * 60, local_minute)
    }

    #[test]
    fn follows_the_local_clock_at_boundaries() {
        for (minute, expected) in [
            (299, DayPart::Night),
            (300, DayPart::Dawn),
            (449, DayPart::Dawn),
            (450, DayPart::Day),
            (1_079, DayPart::Day),
            (1_080, DayPart::Dusk),
            (1_199, DayPart::Dusk),
            (1_200, DayPart::Night),
            (1_440, DayPart::Night),
        ] {
            assert_eq!(at(0, minute).day_part, expected);
        }
    }

    #[test]
    fn construction_accumulates_only_during_short_shifts() {
        let start = 6 * 60 + 30;
        let before = at_from(start, 99);
        let during = at_from(start, 103);
        let first_done = at_from(start, 106);
        let break_time = at_from(start, 110);
        let still_on_break = at_from(start, 400);

        assert!(!before.build_in_progress);
        assert_eq!(during.caretaker, CareAction::Building);
        assert_eq!(during.build_percent, 50);
        assert_eq!(first_done.build_stage, 1);
        assert!(break_time.build_in_progress);
        assert_ne!(break_time.caretaker, CareAction::Building);
        assert_eq!(break_time.build_percent, still_on_break.build_percent);
    }

    #[test]
    fn construction_catches_up_after_the_app_has_been_closed() {
        let start = 6 * 60 + 30;
        assert_eq!(at_from(start, 1_440 + 106).build_stage, 2);
        assert_eq!(at_from(start, 3 * 1_440 + 637).build_stage, 2);
        let completed = at_from(start, 3 * 1_440 + 638);
        assert_eq!(completed.build_stage, 3);
        assert_eq!(completed.build_percent, 100);
        assert!(!completed.build_in_progress);
        assert_ne!(completed.caretaker, CareAction::Building);
    }

    #[test]
    fn initial_and_daily_meals_are_distinct_from_treats() {
        assert_eq!(at(20, 600).caretaker, CareAction::Feeding);
        assert_eq!(at(20, 600).cat, CatAction::Eat);
        assert_eq!(at(56, 600).caretaker, CareAction::Tending);
        for minute in [8 * 60, 12 * 60 + 30, 18 * 60 + 35] {
            let mealtime = at(50_400, minute);
            assert_eq!(mealtime.caretaker, CareAction::Feeding);
            assert_eq!(mealtime.cat, CatAction::Eat);
        }
        assert_eq!(at(1_000, 1_380).cat, CatAction::Sleep);
    }

    #[test]
    fn visitors_leave_and_return_without_consecutive_events() {
        assert_eq!(at(45, 720).visitor, Visitor::None);
        assert_eq!(at(46, 720).visitor, Visitor::Sparrow);
        assert_eq!(at(117, 720).visitor, Visitor::Sparrow);
        assert_eq!(at(118, 720).visitor, Visitor::None);
        let mut previous = Visitor::None;
        let mut arrivals = Vec::new();
        for age in VISIT_BASE..VISIT_BASE + 4 * VISIT_PERIOD {
            let next = at(age, 720).visitor;
            if previous == Visitor::None && next != Visitor::None {
                arrivals.push(age);
            } else if previous != Visitor::None && next != Visitor::None {
                assert_eq!(previous, next);
            }
            previous = next;
        }
        assert_eq!(arrivals.len(), 4);
        assert!(
            arrivals
                .windows(2)
                .all(|pair| (960..=2_400).contains(&(pair[1] - pair[0])))
        );
    }

    #[test]
    fn new_guests_have_distinct_arrival_conditions() {
        let arrivals = |day, weather, recent_rain| {
            (0..42)
                .map(|roll| choose_visitor(roll, day, weather, recent_rain))
                .collect::<Vec<_>>()
        };
        let daylight = arrivals(DayPart::Day, Weather::Clear, false);
        assert!(daylight.contains(&Visitor::Squirrel));
        assert!(daylight.contains(&Visitor::Rabbit));
        assert!(!daylight.contains(&Visitor::Hedgehog));
        assert!(!daylight.contains(&Visitor::Frog));

        let dawn = arrivals(DayPart::Dawn, Weather::Clouds, false);
        assert!(dawn.contains(&Visitor::Rabbit));
        assert!(!dawn.contains(&Visitor::Squirrel));

        let dusk = arrivals(DayPart::Dusk, Weather::Clear, false);
        assert!(dusk.contains(&Visitor::Hedgehog));
        let snowy_night = arrivals(DayPart::Night, Weather::Snow, true);
        assert!(!snowy_night.contains(&Visitor::Hedgehog));
        assert!(!snowy_night.contains(&Visitor::Frog));

        let raining = arrivals(DayPart::Day, Weather::Rain, false);
        assert!(raining.contains(&Visitor::Frog));
        assert!(!raining.contains(&Visitor::Squirrel));
        assert!(!raining.contains(&Visitor::Rabbit));
        assert!(arrivals(DayPart::Day, Weather::Clear, true).contains(&Visitor::Frog));
    }

    #[test]
    fn virtual_epoch_is_preserved_in_the_snapshot() {
        let state = snapshot(
            50_100,
            50_000,
            1_980,
            Season::Spring,
            WeatherMode::Clear,
            None,
        );
        assert_eq!(state.world_start, 50_000);
        assert_eq!(state.phase_seconds, 100);
        assert_eq!(state.minutes_of_day, 540);
    }

    #[test]
    fn cat_sleeps_at_night_and_naps_after_lunch() {
        assert_eq!(at(200, 779).cat, CatAction::Idle);
        assert_eq!(at(200, 780).cat, CatAction::Sleep);
        assert_eq!(at(200, 814).cat, CatAction::Sleep);
        assert_eq!(at(200, 815).cat, CatAction::Idle);
        assert_eq!(at(0, 1_200).cat, CatAction::Sleep);
        assert_eq!(at(0, 299).cat, CatAction::Sleep);
        assert_eq!(at(0, 300).cat, CatAction::Idle);
        assert_eq!(at(20, 1_200).cat, CatAction::Eat);

        let treat = snapshot(200, 0, 1_200, Season::Spring, WeatherMode::Clear, Some(199));
        assert_eq!(treat.cat, CatAction::Treat);
    }

    #[test]
    fn all_guests_arrive_on_the_natural_schedule() {
        let started = 1_800_000_000;
        let mut guests = Vec::new();
        for age in (0..7 * DAY).step_by(30) {
            let local_minutes = ((age / 60 + 360) % 1_440) as u16;
            let state = snapshot(
                started + age,
                started,
                local_minutes,
                Season::Spring,
                WeatherMode::Auto,
                None,
            );
            if state.visitor != Visitor::None && !guests.contains(&state.visitor) {
                guests.push(state.visitor);
            }
        }
        assert_eq!(guests.len(), 7, "observed guests: {guests:?}");
    }

    #[test]
    fn weather_override_and_treat_are_independent_of_season() {
        let snapshot_at = |mode, treat| snapshot(1_500, 0, 720, Season::Spring, mode, treat);
        let before = snapshot_at(WeatherMode::Rain, None);
        let after = snapshot_at(WeatherMode::Snow, Some(1_499));
        assert_eq!(before.weather, Weather::Rain);
        assert_eq!(before.cat, CatAction::Sleep);
        assert_eq!(after.weather, Weather::Snow);
        assert_eq!(after.cat, CatAction::Treat);
        assert_ne!(
            snapshot_at(WeatherMode::Clear, Some(1_501)).cat,
            CatAction::Treat
        );
        assert_ne!(
            snapshot_at(WeatherMode::Clear, Some(1_496)).cat,
            CatAction::Treat
        );
        assert_eq!(
            snapshot(95, 100, 720, Season::Spring, WeatherMode::Auto, None).phase_seconds,
            0
        );
    }
}
