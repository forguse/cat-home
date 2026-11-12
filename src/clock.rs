use std::time::Instant;

const DAY: u64 = 24 * 60 * 60;
const DAWN: u64 = 5 * 60 * 60;
const DUSK: u64 = 20 * 60 * 60;
const DAYLIGHT: u64 = DUSK - DAWN;
const NIGHT_FACTOR: u8 = 2;
const SECOND_NANOS: u128 = 1_000_000_000;

// A daytime world second takes two weighted seconds, while a night second takes one.
// Measure the cycle from dawn so both rates each occupy one contiguous range.
const DAYLIGHT_WEIGHT: u128 = DAYLIGHT as u128 * NIGHT_FACTOR as u128 * SECOND_NANOS;
const CYCLE_WEIGHT: u128 = DAYLIGHT_WEIGHT + (DAY - DAYLIGHT) as u128 * SECOND_NANOS;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Speed {
    One = 1,
    Two = 2,
    #[default]
    Four = 4,
    Eight = 8,
}

impl Speed {
    fn parse(multiplier: u64) -> Option<Self> {
        match multiplier {
            1 => Some(Self::One),
            2 => Some(Self::Two),
            4 => Some(Self::Four),
            8 => Some(Self::Eight),
            _ => None,
        }
    }

    fn elapsed_weight(self, elapsed_nanos: u128) -> u128 {
        elapsed_nanos * self as u128 * NIGHT_FACTOR as u128
    }

    pub fn multiplier_at(self, time: WorldTime) -> u8 {
        self as u8 * if time.is_night() { NIGHT_FACTOR } else { 1 }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldTime {
    pub seconds: u64,
    pub seconds_of_day: u64,
}

impl WorldTime {
    fn advance(self, elapsed_weight: u128) -> Self {
        let initial_offset = (self.seconds_of_day + DAY - DAWN) % DAY;
        let initial_weight = initial_offset.min(DAYLIGHT) * u64::from(NIGHT_FACTOR)
            + initial_offset.saturating_sub(DAYLIGHT);
        let total = u128::from(initial_weight) * SECOND_NANOS + elapsed_weight;
        let cycles = total / CYCLE_WEIGHT;
        let within_cycle = total % CYCLE_WEIGHT;
        let offset = (within_cycle.min(DAYLIGHT_WEIGHT) / u128::from(NIGHT_FACTOR)
            + within_cycle.saturating_sub(DAYLIGHT_WEIGHT))
            / SECOND_NANOS;
        let advanced = cycles * u128::from(DAY) + offset - u128::from(initial_offset);
        Self {
            seconds: self
                .seconds
                .saturating_add(u64::try_from(advanced).unwrap_or(u64::MAX)),
            seconds_of_day: (offset as u64 + DAWN) % DAY,
        }
    }

    pub fn minutes(self) -> u16 {
        (self.seconds_of_day / 60) as u16
    }

    pub fn is_night(self) -> bool {
        !(DAWN..DUSK).contains(&self.seconds_of_day)
    }
}

#[derive(Clone, Copy)]
pub struct Anchor {
    pub real: u64,
    pub world: WorldTime,
    pub speed: Speed,
}

impl Anchor {
    pub fn decode(real: u64, world: u64, seconds_of_day: u64, speed: u64) -> Option<Self> {
        if real == 0 || world == 0 || seconds_of_day >= DAY {
            return None;
        }
        Some(Self {
            real,
            world: WorldTime {
                seconds: world,
                seconds_of_day,
            },
            speed: Speed::parse(speed)?,
        })
    }

    fn elapsed_weight(self, real: u64) -> u128 {
        self.speed
            .elapsed_weight(u128::from(real.saturating_sub(self.real)) * SECOND_NANOS)
    }
}

pub struct Clock {
    world: WorldTime,
    elapsed_weight: u128,
    tick: Instant,
    speed: Speed,
}

impl Clock {
    pub fn load(real_now: u64, local_seconds: u64, saved: Option<Anchor>) -> Self {
        let world = saved.map_or(
            WorldTime {
                seconds: real_now,
                seconds_of_day: local_seconds % DAY,
            },
            |anchor| anchor.world,
        );
        Self {
            world,
            elapsed_weight: saved.map_or(0, |anchor| anchor.elapsed_weight(real_now)),
            tick: Instant::now(),
            speed: saved.map_or_else(Speed::default, |anchor| anchor.speed),
        }
    }

    pub fn speed(&self) -> Speed {
        self.speed
    }

    pub fn time(&self) -> WorldTime {
        self.at(Instant::now())
    }

    fn at(&self, tick: Instant) -> WorldTime {
        self.world.advance(self.elapsed_weight_at(tick))
    }

    fn elapsed_weight_at(&self, tick: Instant) -> u128 {
        let elapsed = tick.saturating_duration_since(self.tick).as_nanos();
        self.elapsed_weight + self.speed.elapsed_weight(elapsed)
    }

    pub fn set_speed(&mut self, speed: Speed) -> bool {
        self.set_speed_at(speed, Instant::now())
    }

    fn set_speed_at(&mut self, speed: Speed, tick: Instant) -> bool {
        if self.speed == speed {
            return false;
        }
        self.elapsed_weight = self.elapsed_weight_at(tick);
        self.tick = tick;
        self.speed = speed;
        true
    }

    pub fn anchor(&self, real: u64) -> Anchor {
        Anchor {
            real,
            world: self.time(),
            speed: self.speed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn legacy_save_keeps_its_age_and_seeds_from_local_time() {
        let started = 1_000;
        let clock = Clock::load(12_000, 45_107, None);
        let time = clock.at(clock.tick);
        assert_eq!(time.seconds - started, 11_000);
        assert_eq!(time.seconds_of_day, 45_107);
        assert_eq!(clock.speed(), Speed::Four);
    }

    #[test]
    fn speed_changes_only_apply_to_future_elapsed_time() {
        let mut clock = Clock::load(500, 43_234, None);
        let changed = clock.tick + Duration::from_secs(18);
        let previous = clock.at(changed);
        assert!(clock.set_speed_at(Speed::Two, changed));
        assert_eq!(clock.at(changed), previous);
        assert_eq!(
            clock.at(changed + Duration::from_secs(5)).seconds,
            previous.seconds + 10
        );
        assert!(!clock.set_speed_at(Speed::Two, changed));
    }

    #[test]
    fn an_offline_interval_advances_at_saved_speed_and_wraps_the_local_day() {
        let anchor = Anchor::decode(1_000, 10_000, DAY - 14, 8).unwrap();
        let clock = Clock::load(1_005, 40_000, Some(anchor));
        let time = clock.at(clock.tick);
        assert_eq!(time.seconds, 10_080);
        assert_eq!(time.seconds_of_day, 66);
        assert_eq!(time.minutes(), 1);
        assert_eq!(clock.speed(), Speed::Eight);
        let rolled_back = Clock::load(990, 2, Some(anchor));
        assert_eq!(rolled_back.at(rolled_back.tick), anchor.world);
    }

    #[test]
    fn crosses_dusk_and_dawn_at_the_exact_clock_boundaries() {
        let clock_from = |second| Clock::load(1_000, 0, Anchor::decode(1_000, 10_000, second, 1));
        let dusk = clock_from(DUSK - 1);
        assert_eq!(
            dusk.at(dusk.tick + Duration::from_millis(500))
                .seconds_of_day,
            DUSK - 1
        );
        let start_of_night = dusk.at(dusk.tick + Duration::from_secs(1));
        assert_eq!(start_of_night.seconds_of_day, DUSK);
        assert_eq!(dusk.speed().multiplier_at(start_of_night), 2);
        assert_eq!(
            dusk.at(dusk.tick + Duration::from_millis(1500))
                .seconds_of_day,
            DUSK + 1
        );

        let dawn = clock_from(DAWN - 1);
        let start_of_day = dawn.at(dawn.tick + Duration::from_millis(500));
        assert_eq!(start_of_day.seconds_of_day, DAWN);
        assert_eq!(dawn.speed().multiplier_at(start_of_day), 1);
        assert_eq!(
            dawn.at(dawn.tick + Duration::from_millis(1500))
                .seconds_of_day,
            DAWN + 1
        );
    }

    #[test]
    fn rate_switch_keeps_subsecond_progress_into_dawn() {
        let mut clock = Clock::load(1_000, 0, Anchor::decode(1_000, 10_000, DAWN - 1, 1));
        let change_at = clock.tick + Duration::from_millis(250);
        assert!(clock.set_speed_at(Speed::Two, change_at));
        assert_eq!(
            clock
                .at(change_at + Duration::from_millis(124))
                .seconds_of_day,
            DAWN - 1
        );
        assert_eq!(
            clock
                .at(change_at + Duration::from_millis(125))
                .seconds_of_day,
            DAWN
        );
    }

    #[test]
    fn offline_projection_accounts_for_many_day_and_night_cycles() {
        let cycles = 100_000;
        let anchor = Anchor::decode(100_000, 1_000_000, 14 * 3600 + 57, 4).unwrap();
        let reopened = Clock::load(anchor.real + cycles * 17_550 + 3, 0, Some(anchor));
        let time = reopened.at(reopened.tick);
        assert_eq!(time.seconds, anchor.world.seconds + cycles * DAY + 12);
        assert_eq!(time.seconds_of_day, anchor.world.seconds_of_day + 12);
    }
}
