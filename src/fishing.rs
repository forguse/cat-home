//! Short, clock-derived visits to the pond, with a fixed result per visit.

use crate::simulation::{CareAction, CatAction, DayPart, Snapshot, Weather};

const SLOT: u64 = 45 * 60;
const FIRST_CAST: u64 = 96;
const CAST: u8 = 20;
const WAIT: u8 = 48;
const BITE: u8 = 16;
const LAND: u8 = 22;
const EAT: u8 = 40;
const MISS: u8 = 35;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Casting,
    Waiting,
    Bite,
    Caught,
    Eating,
    Disappointed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Moment {
    pub phase: Phase,
    /// World seconds since this phase began.
    pub elapsed: u8,
}

fn session(age: u64, seed: u64) -> Option<(u64, bool)> {
    let slot = age / SLOT;
    let within = age % SLOT;
    let roll =
        (seed ^ slot.wrapping_mul(0x9e37_79b9_7f4a_7c15)).wrapping_add(0x27d4_eb2f_1656_67c5);
    let roll = (roll ^ (roll >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let roll = (roll ^ (roll >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    let roll = roll ^ (roll >> 31);

    let start = if slot == 0 {
        FIRST_CAST
    } else if roll % 3 != 0 {
        240 + (roll >> 8) % 1_800
    } else {
        return None;
    };
    Some((within.checked_sub(start)?, (roll >> 48) % 100 < 58))
}

pub fn moment(state: &Snapshot) -> Option<Moment> {
    if state.day_part != DayPart::Day
        || state.weather == Weather::Snow
        || state.caretaker == CareAction::Building
        || matches!(
            state.cat,
            CatAction::Treat | CatAction::Eat | CatAction::Sleep
        )
    {
        return None;
    }

    let (mut elapsed, caught) = session(state.phase_seconds, state.world_start)?;
    let (terminal, duration) = if caught {
        (Phase::Eating, EAT)
    } else {
        (Phase::Disappointed, MISS)
    };
    let phases = [
        (Phase::Casting, CAST),
        (Phase::Waiting, WAIT),
        (Phase::Bite, if caught { BITE } else { 0 }),
        (Phase::Caught, if caught { LAND } else { 0 }),
        (terminal, duration),
    ];
    for (phase, duration) in phases {
        if elapsed < u64::from(duration) {
            return Some(Moment {
                phase,
                elapsed: elapsed as u8,
            });
        }
        elapsed -= u64::from(duration);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::{self, Season, WeatherMode};

    fn at(age: u64, seed: u64) -> Snapshot {
        simulation::snapshot(
            seed + age,
            seed,
            12 * 60,
            Season::Spring,
            WeatherMode::Clear,
            None,
        )
    }

    #[test]
    fn a_bite_leads_to_catch_and_a_finite_meal() {
        let seed = (1..100)
            .find(|&seed| session(FIRST_CAST, seed) == Some((0, true)))
            .unwrap();
        let mut age = FIRST_CAST;
        for (phase, duration) in [
            (Phase::Casting, CAST),
            (Phase::Waiting, WAIT),
            (Phase::Bite, BITE),
            (Phase::Caught, LAND),
            (Phase::Eating, EAT),
        ] {
            assert_eq!(moment(&at(age, seed)), Some(Moment { phase, elapsed: 0 }));
            let elapsed = duration - 1;
            assert_eq!(
                moment(&at(age + u64::from(elapsed), seed)),
                Some(Moment { phase, elapsed })
            );
            age += u64::from(duration);
        }
        assert_eq!(moment(&at(age, seed)), None);
    }

    #[test]
    fn an_empty_hook_leads_to_a_finite_disappointed_pose() {
        let seed = (1..100)
            .find(|&seed| session(FIRST_CAST, seed) == Some((0, false)))
            .unwrap();
        let missed = FIRST_CAST + u64::from(CAST + WAIT);
        assert_eq!(moment(&at(missed - 1, seed)).unwrap().phase, Phase::Waiting);
        assert_eq!(
            moment(&at(missed, seed)),
            Some(Moment {
                phase: Phase::Disappointed,
                elapsed: 0
            })
        );
        assert_eq!(moment(&at(missed + u64::from(MISS), seed)), None);
    }

    #[test]
    fn weather_and_a_manual_treat_do_not_reroll_the_outcome() {
        let mut state = at(FIRST_CAST + u64::from(CAST + WAIT), 42);
        let before = moment(&state).unwrap();
        state.day_part = DayPart::Night;
        assert_eq!(moment(&state), None);
        state.day_part = DayPart::Day;
        state.weather = Weather::Snow;
        assert_eq!(moment(&state), None);
        state.weather = Weather::Rain;
        state.cat = CatAction::Treat;
        assert_eq!(moment(&state), None);
        state.cat = CatAction::Idle;
        assert_eq!(moment(&state), Some(before));
    }

    #[test]
    fn a_slot_has_at_most_one_short_visit_and_can_be_skipped() {
        let mut empty = 0;
        let mut visits = 0;
        for slot in 1..32 {
            let mut previous = false;
            let mut starts = 0;
            let mut active = 0;
            for age in slot * SLOT..(slot + 1) * SLOT {
                let present = moment(&at(age, 42)).is_some();
                starts += usize::from(present && !previous);
                active += usize::from(present);
                previous = present;
            }
            assert!(starts <= 1);
            assert!(active <= usize::from(CAST + WAIT + BITE + LAND + EAT));
            visits += starts;
            empty += usize::from(starts == 0);
        }
        assert!(empty > 0 && visits > 0);
    }
}
