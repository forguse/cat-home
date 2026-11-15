//! A fixed rabbit's outbound and return paths to an actual building eave.
use super::super::{
    ANIMALS, DAY_MS, Edge, MINUTE, Resume, WildlifeVisual, foothold, land, point, route_tile, tile,
    weather_wet,
};
use super::Proof;
use crate::{
    living_world::{BuildingKind, LivingWorld, YardSeason, YardWeather},
    map_data::TilePos,
    map_nav::{MapNav, SearchResult},
};
use serde::{Deserialize, Serialize};

pub(super) const RABBIT: usize = 4;
const PACE: u64 = 39_000;
const DIRECTIONS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trail {
    start: TilePos,
    dirs: [u64; 2],
    len: u8,
}

impl Trail {
    fn find(world: &LivingWorld, from: TilePos, goal: TilePos) -> Option<Self> {
        let view = world.navigation_view()?;
        let mut nav = MapNav::new();
        if nav.begin(&view, from, goal) != SearchResult::Pending {
            return None;
        }
        let SearchResult::Found(path) = nav.advance(&view, 4_096) else {
            return None;
        };
        let steps = path.steps();
        if !path.is_complete() || steps.len() > 64 {
            return None;
        }
        let mut dirs = [0u64; 2];
        let mut at = from;
        for (i, &next) in steps.iter().enumerate() {
            let dir = DIRECTIONS
                .iter()
                .position(|&(dx, dy)| at.offset(dx, dy) == Some(next))?;
            dirs[i / 32] |= (dir as u64) << (2 * (i % 32));
            at = next;
        }
        Some(Self {
            start: from,
            dirs,
            len: steps.len() as u8,
        })
    }

    fn step(self, i: u8, cell: TilePos, forward: bool) -> Option<TilePos> {
        let i = usize::from(if forward { i } else { i.checked_sub(1)? });
        if i >= usize::from(self.len).min(64) {
            return None;
        }
        let dir = ((self.dirs[i / 32] >> (2 * (i % 32))) & 3) as usize;
        let (dx, dy) = DIRECTIONS[dir ^ if forward { 0 } else { 2 }];
        cell.offset(dx, dy)
    }

    fn tile_at(self, endpoint: u8) -> Option<TilePos> {
        if self.len == 0 || self.len > 64 || endpoint > self.len {
            return None;
        }
        let mut cell = self.start;
        if !(6..=44).contains(&cell.x) || !(14..=66).contains(&cell.y) {
            return None;
        }
        for i in 0..endpoint {
            cell = self.step(i, cell, true)?;
            if !(6..=44).contains(&cell.x) || !(14..=66).contains(&cell.y) {
                return None;
            }
        }
        Some(cell)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Leg {
    Align,
    Out,
    Wait,
    Peek,
    Back,
    Bypass,
    Forage,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rabbit {
    leg: Leg,
    trail: Trail,
    origin: u8,
    index: u8,
    cover: TilePos,
    edge: Edge,
    wet_at_cover: bool,
    resumed: bool,
    last_retry: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    visit: Option<Rabbit>,
    last_day: Option<u64>,
    last_seen: Option<u64>,
    resume: Option<Resume>,
}

fn sheds(world: &LivingWorld) -> [Option<TilePos>; 2] {
    let cottage = world.buildings().iter().any(|b| b.kind == BuildingKind::WestHouse && b.level > 0);
    [
        world.farm_store_door(),
        world.resolve_poi("home_xiaohe.door").filter(|_| cottage).map(tile),
    ]
}

fn current_roof(world: &LivingWorld, under: TilePos) -> bool {
    land(world, under) && sheds(world).into_iter().flatten().any(|door| {
        door.offset(-1, 0) == Some(under) || door.offset(1, 0) == Some(under)
    })
}

fn forage_waypoint(origin: u8) -> usize {
    if origin == 0 {
        1
    } else {
        usize::from(origin) - 1
    }
}

impl State {
    pub(super) fn empty(&self) -> bool {
        self.visit.is_none()
            && self.last_day.is_none()
            && self.last_seen.is_none()
            && self.resume.is_none()
    }

    pub(super) fn valid(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        self.last_day.is_none_or(|day| day <= now / DAY_MS)
            && self.last_seen.is_none_or(|seen| seen <= now)
            && self
                .resume
                .is_none_or(|r| r.at <= now && usize::from(r.waypoint) < ANIMALS[RABBIT].path.len())
            && self.visit.is_none_or(|r| {
                r.edge.valid(now, 6..=44, 14..=66)
                    && usize::from(r.origin) < ANIMALS[RABBIT].path.len()
                    && r.trail.tile_at(r.trail.len).is_some_and(|destination| {
                        if matches!(r.leg, Leg::Bypass | Leg::Forage)
                            && destination == route_tile(RABBIT, usize::from(r.origin))
                        {
                            true
                        } else {
                            r.leg != Leg::Bypass && destination == r.cover
                        }
                    })
                    && match r.leg {
                        Leg::Forage => {
                            r.resumed
                                && r.edge.to == point(route_tile(RABBIT, forage_waypoint(r.origin)))
                        }
                        _ => r.trail.tile_at(r.index).map(point) == Some(r.edge.to),
                    }
            })
    }

    pub(super) fn deadline(&self) -> u64 {
        self.visit.map_or(u64::MAX, |r| r.edge.end)
    }

    pub(super) fn phase(&self, now: u64) -> Option<u64> {
        self.resume
            .map(|r| now.saturating_sub(r.at) + u64::from(r.waypoint) * ANIMALS[RABBIT].period)
    }

    pub(super) fn advance(
        &mut self,
        world: &LivingWorld,
        occupied: &[TilePos],
        probe: bool,
    ) -> Option<Proof> {
        let now = world.world_ms();
        let day = now / DAY_MS;
        let minute = now % DAY_MS / MINUTE;
        let wet = weather_wet(world);
        let Some(mut r) = self.visit else {
            if !probe {
                return None;
            }
            if super::super::animal_visible(RABBIT, ANIMALS[RABBIT], world) {
                self.last_seen = Some(now);
            } else if wet
                && self
                    .last_seen
                    .take()
                    .is_some_and(|seen| now <= seen + 2 * MINUTE)
                && self.last_day != Some(day)
                && (320..=490).contains(&minute)
                && world.season() != YardSeason::Winter
            {
                self.last_day = Some(day);
                let phase = self
                    .phase(now)
                    .unwrap_or(now.saturating_add(1_951 * RABBIT as u64));
                let from = super::super::super::routine_animal(RABBIT, world, phase).position;
                let origin = (0..ANIMALS[RABBIT].path.len())
                    .find(|&i| route_tile(RABBIT, i) == tile(from))?;
                let (cover, trail) = sheds(world).into_iter().flatten()
                    .flat_map(|door| [door.offset(-1, 0), door.offset(1, 0)].into_iter().flatten())
                    .filter(|&under| foothold(world, occupied, under))
                    .find_map(|under| {
                        let trail = Trail::find(world, tile(from), under)?;
                        (trail.tile_at(trail.len) == Some(under)).then_some((under, trail))
                    })?;
                self.visit = Some(Rabbit {
                    leg: Leg::Align,
                    trail,
                    origin: origin as u8,
                    index: 0,
                    cover,
                    edge: Edge::travel(from, point(tile(from)), now, PACE),
                    wet_at_cover: false,
                    resumed: false,
                    last_retry: None,
                });
            }
            return None;
        };
        if now < r.edge.end {
            return None;
        }
        let at = tile(r.edge.to);
        if r.leg == Leg::Forage {
            self.resume = Some(Resume {
                at: now,
                waypoint: forage_waypoint(r.origin) as u8,
            });
            self.visit = None;
            return (matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds)
                && foothold(world, occupied, at))
            .then_some(Proof::RabbitResumed {
                from: at,
                cover: r.cover,
            });
        }
        if matches!(r.leg, Leg::Align | Leg::Out) {
            r.leg = if wet && current_roof(world, r.cover) {
                Leg::Out
            } else {
                Leg::Back
            };
        }
        if r.leg == Leg::Out && r.index == r.trail.len {
            r.leg = Leg::Wait;
            r.wet_at_cover = current_roof(world, r.cover) && foothold(world, occupied, at);
        }
        if r.leg == Leg::Wait {
            if wet
                && r.wet_at_cover
                && current_roof(world, r.cover)
                && foothold(world, occupied, at)
            {
                r.edge = Edge::hold(r.edge.to, now, MINUTE);
                self.visit = Some(r);
                return None;
            }
            r.resumed = r.wet_at_cover
                && current_roof(world, r.cover)
                && matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds);
            if r.resumed {
                r.leg = Leg::Peek;
                r.edge = Edge::hold(r.edge.to, now, 57_000);
                self.visit = Some(r);
                return None;
            }
            r.leg = Leg::Back;
        }
        if r.leg == Leg::Peek {
            r.leg = if wet && current_roof(world, r.cover) {
                Leg::Wait
            } else {
                Leg::Back
            };
        }
        if r.leg == Leg::Back && r.index == 0 || r.leg == Leg::Bypass && r.index == r.trail.len {
            let origin = route_tile(RABBIT, usize::from(r.origin));
            self.resume = Some(Resume {
                at: now,
                waypoint: r.origin,
            });
            self.visit = None;
            if r.resumed && !wet && (320..690).contains(&minute) {
                // The route concludes by beginning the actual next forage leg.
                let next_tile = route_tile(RABBIT, forage_waypoint(r.origin));
                if at == origin && foothold(world, occupied, next_tile) {
                    r.leg = Leg::Forage;
                    r.edge = Edge::travel(r.edge.to, point(next_tile), now, PACE);
                    self.visit = Some(r);
                }
            }
            return None;
        }
        let forward = matches!(r.leg, Leg::Out | Leg::Bypass);
        if let Some(next) = r.trail.step(r.index, at, forward) {
            if foothold(world, occupied, next) {
                r.index = if forward { r.index + 1 } else { r.index - 1 };
                r.edge = Edge::travel(r.edge.to, point(next), now, PACE);
            } else {
                r.edge = Edge::hold(r.edge.to, now, 3 * MINUTE);
                let rev = world.terrain_revision();
                if !land(world, next) && r.last_retry != Some(rev) {
                    r.last_retry = Some(rev);
                    if let Some(path) =
                        Trail::find(world, at, route_tile(RABBIT, usize::from(r.origin)))
                        && path.tile_at(path.len).is_some()
                    {
                        r.trail = path;
                        r.index = 0;
                        r.leg = Leg::Bypass;
                    }
                }
            }
        }
        self.visit = Some(r);
        None
    }

    pub(super) fn visual(&self, world: &LivingWorld) -> Option<WildlifeVisual> {
        let rabbit = self.visit?;
        let now = world.world_ms();
        let edge = rabbit.edge;
        let sprite = if rabbit.leg == Leg::Peek {
            "wild_rabbit_idle_0"
        } else if rabbit.leg == Leg::Wait || edge.from == edge.to {
            "wild_rabbit_crouch_0"
        } else if edge.fraction(now) < 0.72 {
            "wild_rabbit_hop_0"
        } else {
            "wild_rabbit_land_0"
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
    fn packed_trail_to_the_real_eave_roundtrips_and_reverses_without_losing_positions() {
        let world = LivingWorld::new_village_wall_ms(0);
        let origin = route_tile(RABBIT, 0);
        let eave = tile(world.resolve_poi("home_xiaohe.door").unwrap()).offset(-1, 0).unwrap();
        assert!(current_roof(&world, eave));
        let route = Trail::find(&world, origin, eave).unwrap();
        assert_eq!(route.tile_at(route.len), Some(eave));
        assert!(route.len > 40);
        let saved = serde_json::to_vec(&route).unwrap();
        let route: Trail = serde_json::from_slice(&saved).unwrap();
        let mut reverse = eave;
        for i in (1..=route.len).rev() {
            reverse = route.step(i, reverse, false).unwrap();
            assert!(land(&world, reverse));
            assert_eq!(route.tile_at(i - 1), Some(reverse));
        }
        assert_eq!(reverse, origin);
    }
}
