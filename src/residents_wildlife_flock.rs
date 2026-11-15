//! Two existing birds share an actual rail without double occupancy.
use super::{
    ANIMALS, DAY_MS, Edge, MINUTE, Resume, SPARROW, SWALLOW, WildlifeVisual, animal_visible, point,
    route_tile, scenery_here,
};
use crate::{
    living_world::{LivingWorld, Point, YardWeather},
    map_data::{TilePos, WILLOW_VILLAGE},
};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub(super) const RAIL: TilePos = TilePos::new(18, 16);
pub(super) const ALTERNATE: TilePos = TilePos::new(17, 16);
const BIRDS: [usize; 2] = [SPARROW, SWALLOW];
const FLIGHT_STEP: u64 = 28_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Leg {
    Approach,
    Descent,
    Circle,
    Alternate,
    Perched,
    Return,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pair {
    leg: Leg,
    edges: [Edge; 2],
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    birds: Option<Pair>,
    observed_day: Option<u64>,
    resume: Option<Resume>,
}

fn perch(tile: TilePos) -> Point {
    let p = point(tile);
    Point {
        x: p.x,
        y: p.y - 5.0,
    }
}

fn overhead(dx: i8, dy: i8) -> Point {
    let p = perch(RAIL);
    Point {
        x: p.x + f32::from(dx),
        y: p.y + f32::from(dy),
    }
}

fn waypoint(leg: Leg) -> [Point; 2] {
    match leg {
        Leg::Approach => [overhead(-2, -8), overhead(6, -20)],
        Leg::Descent => [perch(RAIL), overhead(-4, -19)],
        Leg::Circle => [perch(RAIL), overhead(-20, -12)],
        Leg::Alternate | Leg::Perched => [perch(RAIL), perch(ALTERNATE)],
        Leg::Return => BIRDS.map(|id| point(route_tile(id, 0))),
    }
}

fn rails_exist(world: &LivingWorld) -> bool {
    static RAILS: OnceLock<[usize; 2]> = OnceLock::new();
    let rails = RAILS.get_or_init(|| {
        [
            "scenic.home_table_court.fence.18_16",
            "scenic.home_table_court.fence.17_16",
        ]
        .map(|id| {
            WILLOW_VILLAGE
                .scenic
                .iter()
                .position(|item| item.id == id)
                .expect("village bird rail")
        })
    });
    rails.iter().all(|&id| scenery_here(world, id))
}

fn open(world: &LivingWorld) -> bool {
    rails_exist(world) && matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds)
}

pub(super) fn layered_depth(world: &LivingWorld, v: &WildlifeVisual) -> Option<i32> {
    let rail = match v.sprite {
        "wild_sparrow_perch_0" if v.position == perch(RAIL) => RAIL,
        "wild_swallow_perch_0" if v.position == perch(ALTERNATE) => ALTERNATE,
        _ => return None,
    };
    rails_exist(world).then(|| point(rail).y as i32 + 1)
}

impl Pair {
    fn transition(leg: Leg, from: [Point; 2], now: u64) -> Self {
        let positions = waypoint(leg);
        let mut edges = [0, 1].map(|i| Edge::travel(from[i], positions[i], now, FLIGHT_STEP));
        match leg {
            // Their approach signals converge at one rail, but the lower bird
            // has right of way and both retain physical positions while staged.
            Leg::Approach => {
                let end = edges[0].end.max(edges[1].end);
                edges[0].end = end;
                edges[1].end = end;
            }
            Leg::Perched => {
                edges = positions.map(|at| Edge::hold(at, now, 4 * MINUTE));
            }
            _ => {}
        }
        Self { leg, edges }
    }

    fn deadline(self) -> u64 {
        self.edges[0].end.max(self.edges[1].end)
    }
}

impl State {
    pub(super) fn empty(&self) -> bool {
        self.birds.is_none() && self.observed_day.is_none() && self.resume.is_none()
    }

    pub(super) fn active(&self) -> bool {
        self.birds.is_some()
    }

    pub(super) fn deadline(&self) -> u64 {
        self.birds.map_or(u64::MAX, Pair::deadline)
    }

    pub(super) fn resume(&self, id: usize) -> Option<Resume> {
        BIRDS.contains(&id).then_some(self.resume).flatten()
    }

    pub(super) fn valid(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        self.observed_day.is_none_or(|day| day <= now / DAY_MS)
            && self.resume.is_none_or(|r| r.at <= now && r.waypoint == 0)
            && self.birds.is_none_or(|p| {
                self.observed_day.is_some()
                    && p.edges
                        .into_iter()
                        .zip(waypoint(p.leg))
                        .all(|(edge, to)| edge.valid(now, 15..=70, 11..=42) && edge.to == to)
            })
    }

    pub(super) fn advance(
        &mut self,
        world: &LivingWorld,
        may_start: bool,
        sources: [Point; 2],
    ) -> bool {
        let now = world.world_ms();
        let day = now / DAY_MS;
        let can_fly = open(world)
            && BIRDS
                .into_iter()
                .all(|id| animal_visible(id, ANIMALS[id], world));
        let Some(pair) = self.birds else {
            if may_start
                && self.observed_day != Some(day)
                && (480..500).contains(&(now % DAY_MS / MINUTE))
                && can_fly
            {
                self.observed_day = Some(day);
                self.birds = Some(Pair::transition(Leg::Approach, sources, now));
            }
            return false;
        };
        if !can_fly && pair.leg != Leg::Return {
            self.birds = Some(Pair::transition(
                Leg::Return,
                pair.edges.map(|edge| edge.position(now)),
                now,
            ));
            return false;
        }
        if now < pair.deadline() {
            return false;
        }
        let next = match pair.leg {
            Leg::Approach => Leg::Descent,
            Leg::Descent => Leg::Circle,
            Leg::Circle => Leg::Alternate,
            Leg::Alternate => Leg::Perched,
            Leg::Perched => Leg::Return,
            Leg::Return => {
                self.birds = None;
                self.resume = Some(Resume {
                    at: now,
                    waypoint: 0,
                });
                return false;
            }
        };
        self.birds = Some(Pair::transition(next, pair.edges.map(|edge| edge.to), now));
        next == Leg::Perched
    }

    pub(super) fn visual(&self, id: usize, world: &LivingWorld) -> Option<WildlifeVisual> {
        let i = BIRDS.into_iter().position(|bird| bird == id)?;
        let pair = self.birds?;
        let now = world.world_ms();
        let edge = pair.edges[i];
        let perched = pair.leg == Leg::Perched
            || i == 0
                && matches!(pair.leg, Leg::Descent | Leg::Circle | Leg::Alternate)
                && now >= edge.end;
        let sprite = if perched {
            ["wild_sparrow_perch_0", "wild_swallow_perch_0"][i]
        } else if now + 18_000 >= edge.end
            && matches!(pair.leg, Leg::Descent | Leg::Alternate)
            && (i == 0) == (pair.leg == Leg::Descent)
        {
            ["wild_sparrow_land_0", "wild_swallow_land_0"][i]
        } else {
            [
                ["wild_sparrow_fly_0", "wild_sparrow_fly_1"],
                ["wild_swallow_fly_0", "wild_swallow_fly_1"],
            ][i][usize::from(now / 34_000 % 2 != 0)]
        };
        Some(WildlifeVisual {
            sprite,
            position: edge.position(now),
            facing_left: edge.to.x < edge.from.x,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_birds_can_land_only_after_the_high_one_clears_the_shared_rail() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut state = State::default();
        let start = 4 * DAY_MS + 8 * 60 * MINUTE;
        let seek = |world: &mut LivingWorld, until| {
            world.queue_world_time(until - world.world_ms());
            while !world.advance_pending_to(until) {}
        };
        seek(&mut world, start);
        for i in 0..10 {
            if open(&world)
                && BIRDS
                    .into_iter()
                    .all(|id| animal_visible(id, ANIMALS[id], &world))
            {
                break;
            }
            seek(&mut world, start + (i + 1) * DAY_MS);
        }
        let sources = BIRDS.map(|id| point(route_tile(id, 0)));
        assert!(!state.advance(&world, true, sources));
        assert_eq!(state.birds.unwrap().leg, Leg::Approach);
        for leg in [Leg::Descent, Leg::Circle, Leg::Alternate, Leg::Perched] {
            seek(&mut world, state.deadline());
            let before = BIRDS.map(|id| state.visual(id, &world).unwrap().position);
            let mut reopened: State =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(
                reopened.advance(&world, false, sources),
                leg == Leg::Perched
            );
            let after = BIRDS.map(|id| reopened.visual(id, &world).unwrap().position);
            assert_eq!(before, after);
            assert_ne!(after[0], after[1]);
            assert!(reopened.valid(&world));
            state = reopened;
        }
        assert_eq!(state.visual(SPARROW, &world).unwrap().position, perch(RAIL));
        assert_eq!(
            state.visual(SWALLOW, &world).unwrap().position,
            perch(ALTERNATE)
        );
    }
}
