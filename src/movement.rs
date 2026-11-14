//! Ground travel follows real elapsed time even across a world-clock rate change.
use super::{DAY_MS, HOUR, advance_clock};

const DAY_RATE: u64 = 48;
const NIGHT_RATE: u64 = 96;
const DAWN: u64 = 5 * HOUR;
const DUSK: u64 = 20 * HOUR;
const OLD_MAX_EDGE: u64 = 14_000;
const PREVIOUS_PERSON_MS: u64 = 1_000;
const ANIMAL_MS: u64 = 500;
const CAT_MS: u64 = 650;
const PERSON_MS: u64 = 750;
const HEAVY_LOAD_MS: u64 = 180;
const TIRED_MS: u64 = 140;
const ROUGH_WEATHER_MS: u64 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pace {
    Human,
    Cat,
    Animal,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WalkConditions {
    pub loaded: bool,
    pub tired: bool,
    pub rough_weather: bool,
}

/// World time of one tile at the requested real walking pace.
pub fn edge_duration(start: u64, pace: Pace, conditions: WalkConditions) -> u64 {
    let real_ms = match pace {
        Pace::Human => PERSON_MS,
        Pace::Cat => CAT_MS,
        Pace::Animal => ANIMAL_MS,
    } + HEAVY_LOAD_MS * u64::from(conditions.loaded)
        + TIRED_MS * u64::from(conditions.tired)
        + ROUGH_WEATHER_MS * u64::from(conditions.rough_weather);
    advance_clock(start, u128::from(real_ms * NIGHT_RATE)).0 - start
}

/// Saved edges keep their deadlines, including the original 8–14s world-time
/// edges and the later one-second walking pace.
pub fn valid_travel_edge(start: u64, end: u64) -> bool {
    let duration = end.saturating_sub(start);
    let fastest = ANIMAL_MS * DAY_RATE;
    let slowest = (PREVIOUS_PERSON_MS + HEAVY_LOAD_MS + TIRED_MS + ROUGH_WEATHER_MS) * NIGHT_RATE;
    (8_000..=OLD_MAX_EDGE).contains(&duration) || (fastest..=slowest).contains(&duration)
}

/// Older edges retain linear world-clock interpolation. New edges interpolate
/// in wall-time units so crossing sunrise or sunset does not change stride speed.
pub fn edge_fraction(now: u64, start: u64, end: u64) -> f32 {
    if now >= end {
        return 1.0;
    }
    if now <= start {
        return 0.0;
    }
    let span = end - start;
    if span <= OLD_MAX_EDGE {
        return (now - start) as f32 / span as f32;
    }
    let initial = night_weight(start);
    (night_weight(now) - initial) as f32 / (night_weight(end) - initial) as f32
}

/// Express a game timestamp in units of one game millisecond at the fast rate.
fn night_weight(world_ms: u64) -> u128 {
    let base = u128::from(world_ms / DAY_MS) * u128::from(DAY_MS + DUSK - DAWN);
    let offset = world_ms % DAY_MS;
    base + u128::from(offset + offset.min(DUSK).saturating_sub(DAWN))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_speed_stays_steady_at_dawn_and_dusk() {
        for origin in [4 * HOUR, 12 * HOUR, DUSK - 12_000, DAWN - 24_000, DAY_MS - 24_000] {
            let end = origin + edge_duration(origin, Pace::Human, WalkConditions::default());
            assert!(valid_travel_edge(origin, end));
            assert_eq!(advance_clock(origin, 72_000).0, end);
            for quarters in 0..=4 {
                let world = advance_clock(origin, 18_000 * quarters).0;
                assert_eq!(edge_fraction(world, origin, end), quarters as f32 / 4.0);
            }
        }
    }

    #[test]
    fn edge_effort_varies_by_species_and_actual_load_and_accepts_existing_saves() {
        let effort = WalkConditions { loaded: true, tired: true, rough_weather: true };
        for (start, increment) in [(6 * HOUR, DAY_RATE), (23 * HOUR, NIGHT_RATE)] {
            assert_eq!(edge_duration(start, Pace::Human, effort), 1_170 * increment);
            assert_eq!(edge_duration(start, Pace::Cat, WalkConditions::default()), 650 * increment);
            assert_eq!(edge_duration(start, Pace::Animal, WalkConditions::default()), 500 * increment);
            assert!(valid_travel_edge(start, start + 1_420 * increment));
        }
        let start = DUSK - 4_000;
        let end = start + 10_000;
        assert_eq!(edge_fraction(start + 5_000, start, end), 0.5);
        assert!(valid_travel_edge(start, end));
        assert!(!valid_travel_edge(start, start + 20_000));
    }
}
