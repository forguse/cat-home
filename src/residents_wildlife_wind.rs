//! The creek has a small bounded air corridor with a wind vector.
use super::routes::Proof;
use super::{
    ANIMALS, DAY_MS, Edge, HOUR, MINUTE, Resume, SWALLOW, WildlifeVisual, animal_visible, foothold,
    ground, point, route_tile,
};
use crate::{
    living_world::{Ground, LivingWorld, Point, YardSeason, YardWeather},
    map_data::TilePos,
};
use serde::{Deserialize, Serialize};

const BANKS: [TilePos; 2] = [TilePos::new(59, 40), TilePos::new(68, 40)];
const OPEN_WATER: TilePos = TilePos::new(63, 40);
const BASE: u64 = 28_000;
const SPELL: u64 = 3 * HOUR;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Wind {
    east: bool,
    strength: u8,
}

impl Wind {
    fn pair(self) -> (TilePos, TilePos) {
        if self.east {
            (BANKS[0], BANKS[1])
        } else {
            (BANKS[1], BANKS[0])
        }
    }

    fn with_tailwind(self) -> u64 {
        BASE * (100 - u64::from(self.strength) * 7) / 100
    }
}

fn wind(world: &LivingWorld) -> Option<Wind> {
    if !matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds)
        || world.season() == YardSeason::Winter
        || ground(world, OPEN_WATER) != Ground::Water
        || BANKS
            .into_iter()
            .any(|tile| ground(world, tile) == Ground::Water)
    {
        return None;
    }
    // Alternating creek flow directions are stable inside a three-hour
    // spell. Cooler late hours only leave a gentle breeze over this reach.
    let spell = world.world_ms() / SPELL;
    let thermal = ((spell + spell / 8 + 2 * (spell / 224)) % 5) as u8;
    (thermal > 0).then_some(Wind {
        east: (spell + spell / 8) % 2 == 0,
        strength: thermal.min(3),
    })
}

fn flight_point(at: TilePos) -> Point {
    let p = point(at);
    Point {
        x: p.x,
        y: p.y - 13.0,
    }
}

fn routine() -> Point {
    point(route_tile(SWALLOW, 0))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Leg {
    Climb,
    Glide,
    Approach,
    Rejoin,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Flight {
    east: bool,
    strength: u8,
    spell: u64,
    leg: Leg,
    edge: Edge,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    flight: Option<Flight>,
    attempted_day: Option<u64>,
    resume: Option<Resume>,
}

impl State {
    pub(super) fn empty(&self) -> bool {
        self.attempted_day.is_none() && self.flight.is_none() && self.resume.is_none()
    }
    pub(super) fn active(&self) -> bool {
        self.flight.is_some()
    }
    pub(super) fn deadline(&self) -> u64 {
        self.flight.map_or(u64::MAX, |f| f.edge.end)
    }
    pub(super) fn resume(&self, id: usize) -> Option<Resume> {
        if id == SWALLOW { self.resume } else { None }
    }

    pub(super) fn valid(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        self.attempted_day.is_none_or(|day| day <= now / DAY_MS)
            && self.resume.is_none_or(|r| r.at <= now && r.waypoint == 0)
            && self.flight.is_none_or(|f| {
                let vector = Wind {
                    east: f.east,
                    strength: f.strength.min(3),
                };
                let (from, to) = vector.pair();
                self.attempted_day.is_some()
                    && (1..=3).contains(&f.strength)
                    && f.spell <= now / SPELL
                    && f.edge.valid(now, 58..=69, 37..=42)
                    && match f.leg {
                        Leg::Climb => f.edge.to == flight_point(from),
                        Leg::Glide => {
                            f.edge.from == flight_point(from)
                                && f.edge.to == flight_point(to)
                                && f.edge.end - f.edge.start
                                    == Edge::travel(
                                        f.edge.from,
                                        f.edge.to,
                                        0,
                                        vector.with_tailwind(),
                                    )
                                    .end
                        }
                        Leg::Approach => f.edge.to == point(to),
                        Leg::Rejoin => f.edge.to == routine(),
                    }
            })
    }

    pub(super) fn advance(
        &mut self,
        world: &LivingWorld,
        occupied: &[TilePos],
        may_start: bool,
        source: Point,
    ) -> Option<Proof> {
        let now = world.world_ms();
        let day = now / DAY_MS;
        let Some(f) = self.flight else {
            let minute = now % DAY_MS / MINUTE;
            if !may_start
                || self.attempted_day == Some(day)
                || !(660..695).contains(&minute)
                || !animal_visible(SWALLOW, ANIMALS[SWALLOW], world)
            {
                return None;
            }
            let vector = wind(world)?;
            let (from, to) = vector.pair();
            if !foothold(world, occupied, to) {
                return None;
            }
            self.attempted_day = Some(day);
            self.flight = Some(Flight {
                east: vector.east,
                strength: vector.strength,
                spell: now / SPELL,
                leg: Leg::Climb,
                edge: Edge::travel(source, flight_point(from), now, BASE),
            });
            return None;
        };
        let vector = Wind {
            east: f.east,
            strength: f.strength,
        };
        let (from, to) = vector.pair();
        let uninterrupted =
            f.spell == now / SPELL && wind(world) == Some(vector) && foothold(world, occupied, to);
        if f.leg != Leg::Rejoin && !uninterrupted {
            self.flight = Some(Flight {
                leg: Leg::Rejoin,
                edge: Edge::travel(f.edge.position(now), routine(), now, BASE),
                ..f
            });
            return None;
        }
        if now < f.edge.end {
            return None;
        }
        let at = f.edge.to;
        let (leg, edge, proof) = match f.leg {
            Leg::Climb => (
                Leg::Glide,
                Edge::travel(at, flight_point(to), now, vector.with_tailwind()),
                None,
            ),
            Leg::Glide => (Leg::Approach, Edge::travel(at, point(to), now, BASE), None),
            Leg::Approach => (
                Leg::Rejoin,
                Edge::travel(at, routine(), now, BASE),
                Some(Proof::Tailwind { from, to }),
            ),
            Leg::Rejoin => {
                self.resume = Some(Resume {
                    at: now,
                    waypoint: 0,
                });
                self.flight = None;
                return None;
            }
        };
        self.flight = Some(Flight { leg, edge, ..f });
        proof
    }

    pub(super) fn visual(&self, world: &LivingWorld) -> Option<WildlifeVisual> {
        let f = self.flight?;
        let now = world.world_ms();
        let sprite = if f.leg == Leg::Glide && f.edge.fraction(now) > 0.25 {
            "wild_swallow_fly_1"
        } else if f.leg == Leg::Approach && now + 20_000 >= f.edge.end {
            "wild_swallow_land_0"
        } else {
            ANIMALS[SWALLOW].frames[(now / 19_000 % 3) as usize]
        };
        Some(WildlifeVisual {
            sprite,
            position: f.edge.position(now),
            facing_left: f.edge.to.x < f.edge.from.x,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_tailwind_changes_the_crossing_duration_and_reopens_in_flight() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let settle = |world: &mut LivingWorld, due: u64| {
            world.queue_world_time(due - world.world_ms());
            while !world.advance_pending_to(due) {}
        };
        for day in 4..14 {
            settle(&mut world, day * DAY_MS + 11 * HOUR);
            if wind(&world).is_some() {
                break;
            }
        }
        let mut state = State::default();
        assert_eq!(state.advance(&world, &[], true, routine()), None);
        settle(&mut world, state.deadline());
        assert_eq!(state.advance(&world, &[], false, routine()), None);
        let f = state.flight.unwrap();
        assert_eq!(f.leg, Leg::Glide);
        assert!(f.edge.end - f.edge.start < Edge::travel(f.edge.from, f.edge.to, 0, BASE).end);
        settle(&mut world, f.edge.start + (f.edge.end - f.edge.start) / 2);
        let before = state.visual(&world).unwrap().position;
        let mut restored: State =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert!(restored.valid(&world));
        assert_eq!(before, restored.visual(&world).unwrap().position);
        let (from, to) = wind(&world).unwrap().pair();
        let mut diverted = restored.clone();
        assert_eq!(diverted.advance(&world, &[to], false, routine()), None);
        assert_eq!(diverted.flight.unwrap().leg, Leg::Rejoin);
        assert_eq!(before, diverted.visual(&world).unwrap().position);
        settle(&mut world, f.edge.end);
        assert_eq!(restored.advance(&world, &[], false, routine()), None);
        settle(&mut world, restored.deadline());
        assert_eq!(
            restored.advance(&world, &[], false, routine()),
            Some(Proof::Tailwind { from, to })
        );
    }
}
