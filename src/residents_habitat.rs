//! Short, conserved visits by three already present village animals.
use super::{
    ANIMALS, BEE, DAY_MS, DUCK, DUCK_BANK, DUCK_WAIT, Edge, HOUR, MINUTE, Receipt, Resume, SPARROW,
    WildlifeVisual, animal_visible, foothold, ground, point, route_tile, tile, weather_wet,
};
use crate::{
    living_world::{BuildingKind, Ground, LivingWorld, OrchardPhase, Point, YardWeather},
    map_data::TilePos,
};
use serde::{Deserialize, Serialize};

const TRACK_LIFETIME: u64 = 2 * HOUR;
const BANK: [TilePos; 2] = [DUCK_BANK, TilePos::new(17, 30)];
const ORCHARD: [usize; 4] = [8, 9, 10, 11];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum BeeLeg {
    ToFirst,
    First,
    ToSecond,
    Second,
    Home,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BeeVisit {
    source: u8,
    target: u8,
    leg: BeeLeg,
    edge: Edge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum SparrowLeg {
    Ridge,
    Eave,
    Call,
    Home,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SparrowVisit {
    leg: SparrowLeg,
    edge: Edge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum DuckLeg {
    Approach,
    Land,
    Along,
    ShoreBack,
    WaterBack,
    Home,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DuckVisit {
    leg: DuckLeg,
    edge: Edge,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Habitat {
    bee: Option<BeeVisit>,
    sparrow: Option<SparrowVisit>,
    duck: Option<DuckVisit>,
    bee_day: Option<u64>,
    sparrow_day: Option<u64>,
    duck_day: Option<u64>,
    resumes: [Option<Resume>; 2],
    rainy: bool,
    damp_until: u64,
    track_until: [u64; 2],
}

pub(super) fn blossom(world: &LivingWorld, tree: usize) -> Option<Point> {
    let tree = world.trees().get(tree)?;
    Some(Point {
        x: tree.position.x + 3.0,
        y: tree.position.y - 11.0,
    })
}

fn east_house(world: &LivingWorld) -> Option<&crate::living_world::Building> {
    world
        .buildings()
        .iter()
        .find(|b| b.kind == BuildingKind::EastHouse && b.level > 0)
}

fn roof(world: &LivingWorld, ridge: bool) -> Option<Point> {
    let door = east_house(world)?.position;
    Some(if ridge {
        Point {
            x: door.x + 3.0,
            y: door.y - 45.0,
        }
    } else {
        Point {
            x: door.x - 11.0,
            y: door.y - 31.0,
        }
    })
}

pub(super) fn layered_depth(world: &LivingWorld, visual: &WildlifeVisual) -> Option<i32> {
    if visual.sprite.starts_with("wild_sparrow_") {
        let p = east_house(world)?.position;
        if (visual.position.x - p.x).abs() <= 22.0 && (visual.position.y - p.y + 39.0).abs() <= 15.0
        {
            return Some(p.y as i32 + 1);
        }
    }
    if visual.sprite == "wild_honeybee_idle_0" {
        return ORCHARD.into_iter().find_map(|i| {
            (blossom(world, i) == Some(visual.position))
                .then_some(world.trees().get(i)?.position.y as i32 + 1)
        });
    }
    None
}

impl Habitat {
    pub(super) fn duck_active(&self) -> bool {
        self.duck.is_some()
    }

    pub(super) fn deadline(&self) -> u64 {
        [
            self.bee.map(|v| v.edge.end),
            self.sparrow.map(|v| v.edge.end),
            self.duck.map(|v| v.edge.end),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(u64::MAX)
    }

    pub(super) fn resume(&self, id: usize) -> Option<Resume> {
        let slot = match id {
            BEE => 0,
            SPARROW => 1,
            _ => return None,
        };
        self.resumes[slot]
    }

    pub(super) fn valid(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        [self.bee_day, self.sparrow_day, self.duck_day]
            .into_iter()
            .flatten()
            .all(|day| day <= now / DAY_MS)
            && self.damp_until <= now.saturating_add(TRACK_LIFETIME)
            && self
                .track_until
                .into_iter()
                .all(|until| until <= now.saturating_add(TRACK_LIFETIME))
            && self
                .resumes
                .iter()
                .flatten()
                .all(|r| r.at <= now && r.waypoint == 0)
            && self.bee.is_none_or(|v| {
                v.source != v.target
                    && ORCHARD.contains(&usize::from(v.source))
                    && ORCHARD.contains(&usize::from(v.target))
                    && v.edge.valid(now, 15..=135, 60..=123)
                    && match v.leg {
                        BeeLeg::ToFirst | BeeLeg::First => {
                            blossom(world, usize::from(v.source)) == Some(v.edge.to)
                        }
                        BeeLeg::ToSecond | BeeLeg::Second => {
                            blossom(world, usize::from(v.target)) == Some(v.edge.to)
                        }
                        BeeLeg::Home => v.edge.to == point(route_tile(BEE, 0)),
                    }
            })
            && self.sparrow.is_none_or(|v| {
                v.edge.valid(now, 17..=27, 9..=17)
                    && match v.leg {
                        SparrowLeg::Ridge => roof(world, true) == Some(v.edge.to),
                        SparrowLeg::Eave | SparrowLeg::Call => {
                            roof(world, false) == Some(v.edge.to)
                        }
                        SparrowLeg::Home => v.edge.to == point(route_tile(SPARROW, 0)),
                    }
            })
            && self.duck.is_none_or(|v| {
                v.edge.valid(now, 12..=18, 29..=34) && v.edge.to == point(duck_destination(v.leg))
            })
    }

    pub(super) fn advance(
        &mut self,
        world: &LivingWorld,
        occupied: &[TilePos],
        probe: bool,
        pond: bool,
        [bee, sparrow, duck]: [Point; 3],
    ) -> [Option<Receipt>; 3] {
        let now = world.world_ms();
        let day = now / DAY_MS;
        if probe {
            let rain = weather_wet(world);
            if self.rainy && !rain && world.weather() != YardWeather::Snow {
                self.damp_until = now + TRACK_LIFETIME;
            }
            if rain || world.weather() == YardWeather::Snow {
                self.damp_until = 0;
                self.track_until = [0; 2];
            }
            self.rainy = rain;
        }
        let results = [
            self.advance_bee(world),
            self.advance_sparrow(world),
            self.advance_duck(world, occupied),
        ];
        if !probe {
            return results;
        }
        if self.bee.is_none()
            && self.bee_day != Some(day)
            && world.weather() == YardWeather::Clear
            && animal_visible(BEE, ANIMALS[BEE], world)
        {
            for target in ORCHARD.into_iter().filter(|&i| {
                world.orchard_phase(i) == Some(OrchardPhase::Flower)
                    && !world.orchard_pollination_pending(i)
            }) {
                let Some(source) = ORCHARD
                    .into_iter()
                    .find(|&i| i != target && world.orchard_phase(i) == Some(OrchardPhase::Flower))
                else {
                    continue;
                };
                self.bee = Some(BeeVisit {
                    source: source as u8,
                    target: target as u8,
                    leg: BeeLeg::ToFirst,
                    edge: Edge::travel(bee, blossom(world, source).unwrap(), now, 12_000),
                });
                self.bee_day = Some(day);
                break;
            }
        }
        if self.sparrow.is_none()
            && self.sparrow_day != Some(day)
            && (360..450).contains(&world.minute_of_day())
            && animal_visible(SPARROW, ANIMALS[SPARROW], world)
            && matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds)
            && let Some(ridge) = roof(world, true)
        {
            self.sparrow_day = Some(day);
            self.sparrow = Some(SparrowVisit {
                leg: SparrowLeg::Ridge,
                edge: Edge::travel(sparrow, ridge, now, 22_000),
            });
        }
        if !pond
            && self.duck.is_none()
            && self.duck_day != Some(day)
            && now < self.damp_until
            && animal_visible(DUCK, ANIMALS[DUCK], world)
            && bank_walkable(world, occupied)
        {
            self.duck_day = Some(day);
            self.duck = Some(DuckVisit {
                leg: DuckLeg::Approach,
                edge: Edge::walk(duck, DUCK_WAIT, now),
            });
        }
        results
    }

    fn advance_bee(&mut self, world: &LivingWorld) -> Option<Receipt> {
        let v = self.bee?;
        let now = world.world_ms();
        let trees_ready = world.weather() == YardWeather::Clear
            && [v.source, v.target]
                .into_iter()
                .all(|i| world.orchard_phase(usize::from(i)) == Some(OrchardPhase::Flower))
            && !world.orchard_pollination_pending(usize::from(v.target));
        if v.leg != BeeLeg::Home && !trees_ready {
            self.bee = Some(BeeVisit {
                leg: BeeLeg::Home,
                edge: Edge::travel(v.edge.position(now), point(route_tile(BEE, 0)), now, 12_000),
                ..v
            });
            return None;
        }
        if now < v.edge.end {
            return None;
        }
        let at = v.edge.to;
        let (leg, edge) = match v.leg {
            BeeLeg::ToFirst => (BeeLeg::First, Edge::hold(at, now, 50_000)),
            BeeLeg::First => (
                BeeLeg::ToSecond,
                Edge::travel(at, blossom(world, usize::from(v.target))?, now, 18_000),
            ),
            BeeLeg::ToSecond => (BeeLeg::Second, Edge::hold(at, now, 50_000)),
            BeeLeg::Second => (
                BeeLeg::Home,
                Edge::travel(at, point(route_tile(BEE, 0)), now, 12_000),
            ),
            BeeLeg::Home => {
                self.resumes[0] = Some(Resume {
                    at: now,
                    waypoint: 0,
                });
                self.bee = None;
                return None;
            }
        };
        self.bee = Some(BeeVisit { leg, edge, ..v });
        (v.leg == BeeLeg::Second).then_some(Receipt::Bee {
            source: v.source,
            target: v.target,
        })
    }

    fn advance_sparrow(&mut self, world: &LivingWorld) -> Option<Receipt> {
        let v = self.sparrow?;
        let now = world.world_ms();
        if now < v.edge.end {
            return None;
        }
        let p = v.edge.to;
        let home = || {
            (
                SparrowLeg::Home,
                Edge::travel(p, point(route_tile(SPARROW, 0)), now, 22_000),
            )
        };
        let (leg, edge, called) = match v.leg {
            SparrowLeg::Ridge => {
                let (leg, edge) = match roof(world, false) {
                    Some(eave) => (SparrowLeg::Eave, Edge::travel(p, eave, now, 22_000)),
                    None => home(),
                };
                (leg, edge, false)
            }
            SparrowLeg::Eave => (SparrowLeg::Call, Edge::hold(p, now, MINUTE), false),
            SparrowLeg::Call => {
                let (leg, edge) = home();
                (
                    leg,
                    edge,
                    (360..450).contains(&world.minute_of_day()) && roof(world, false) == Some(p),
                )
            }
            SparrowLeg::Home => {
                self.resumes[1] = Some(Resume {
                    at: now,
                    waypoint: 0,
                });
                self.sparrow = None;
                return None;
            }
        };
        self.sparrow = Some(SparrowVisit { leg, edge });
        called.then_some(Receipt::Sparrow { at: tile(p) })
    }

    fn advance_duck(&mut self, world: &LivingWorld, occupied: &[TilePos]) -> Option<Receipt> {
        let v = self.duck?;
        let now = world.world_ms();
        if now < v.edge.end {
            return None;
        }
        let p = v.edge.to;
        let old = v.leg;
        let still_wet = now < self.damp_until && bank_walkable(world, occupied);
        let leg = match old {
            DuckLeg::Approach if still_wet => DuckLeg::Land,
            DuckLeg::Land if still_wet => {
                self.track_until[0] = now + TRACK_LIFETIME;
                DuckLeg::Along
            }
            DuckLeg::Along if still_wet => {
                self.track_until[1] = now + TRACK_LIFETIME;
                DuckLeg::ShoreBack
            }
            DuckLeg::ShoreBack | DuckLeg::Land | DuckLeg::Along => DuckLeg::WaterBack,
            DuckLeg::WaterBack | DuckLeg::Approach => DuckLeg::Home,
            DuckLeg::Home => {
                self.duck = None;
                return Some(Receipt::DuckRejoined);
            }
        };
        self.duck = Some(DuckVisit {
            leg,
            edge: Edge::walk(p, duck_destination(leg), now),
        });
        (leg == DuckLeg::ShoreBack).then_some(Receipt::DuckPrint {
            first: BANK[0],
            last: BANK[1],
        })
    }

    pub(super) fn marks(&self, world: &LivingWorld) -> impl Iterator<Item = WildlifeVisual> + '_ {
        let now = world.world_ms();
        BANK.into_iter()
            .zip(self.track_until)
            .enumerate()
            .filter_map(move |(i, (at, until))| {
                (until > now).then_some(WildlifeVisual {
                    position: point(at),
                    sprite: "wild_duck_track",
                    facing_left: i == 0,
                })
            })
            .chain(
                self.sparrow
                    .filter(|v| v.leg == SparrowLeg::Call)
                    .map(|v| WildlifeVisual {
                        position: v.edge.position(now),
                        sprite: "wild_sparrow_chirp",
                        facing_left: false,
                    }),
            )
    }

    pub(super) fn visual(&self, id: usize, world: &LivingWorld) -> Option<WildlifeVisual> {
        let now = world.world_ms();
        let (edge, sprite) = match id {
            BEE => {
                let v = self.bee?;
                let sprite = if matches!(v.leg, BeeLeg::First | BeeLeg::Second) {
                    "wild_honeybee_idle_0"
                } else {
                    ANIMALS[BEE].frames[1 + (now / 24_000 % 3) as usize]
                };
                (v.edge, sprite)
            }
            SPARROW => {
                let v = self.sparrow?;
                let sprite = if v.leg == SparrowLeg::Call {
                    "wild_sparrow_perch_0"
                } else {
                    ANIMALS[SPARROW].frames[1 + (now / 24_000 % 2) as usize]
                };
                (v.edge, sprite)
            }
            DUCK => {
                let v = self.duck?;
                let sprite = if ground(world, tile(v.edge.position(now))) == Ground::Water {
                    "wild_duckling_swim_0"
                } else {
                    ["wild_duckling_walk_0", "wild_duckling_walk_1"][(now / 24_000 % 2) as usize]
                };
                (v.edge, sprite)
            }
            _ => return None,
        };
        Some(WildlifeVisual {
            sprite,
            position: edge.position(now),
            facing_left: edge.to.x < edge.from.x,
        })
    }
}

fn bank_walkable(world: &LivingWorld, occupied: &[TilePos]) -> bool {
    BANK.into_iter().all(|at| {
        matches!(ground(world, at), Ground::Grass | Ground::Soil) && foothold(world, occupied, at)
    })
}

fn duck_destination(leg: DuckLeg) -> TilePos {
    match leg {
        DuckLeg::Approach | DuckLeg::WaterBack => DUCK_WAIT,
        DuckLeg::Land | DuckLeg::ShoreBack => BANK[0],
        DuckLeg::Along => BANK[1],
        DuckLeg::Home => route_tile(DUCK, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_blossom_sends_the_same_bee_home_from_the_reopened_position() {
        let world = LivingWorld::new_village_wall_ms(0);
        assert_ne!(world.orchard_phase(8), Some(OrchardPhase::Flower));
        let started = world.world_ms();
        let from = blossom(&world, 8).unwrap();
        let mut visit = Habitat {
            bee: Some(BeeVisit {
                source: 8,
                target: 9,
                leg: BeeLeg::First,
                edge: Edge::hold(from, started, MINUTE),
            }),
            ..Habitat::default()
        };
        assert!(visit.valid(&world));
        let bytes = serde_json::to_vec(&visit).unwrap();
        let mut reopened: Habitat = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(visit.advance_bee(&world), None);
        assert_eq!(reopened.advance_bee(&world), None);
        assert_eq!(reopened.bee.unwrap().leg, BeeLeg::Home);
        assert_eq!(reopened.bee.unwrap().edge.from, from);
        assert_eq!(
            serde_json::to_vec(&visit).unwrap(),
            serde_json::to_vec(&reopened).unwrap()
        );
        reopened.bee.as_mut().unwrap().edge.to = from;
        assert!(!reopened.valid(&world));
    }

    #[test]
    fn a_person_at_the_muddy_shore_cancels_the_duck_trace() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        let edge = Edge::walk(point(DUCK_WAIT), BANK[0], now);
        let mut habitat = Habitat {
            damp_until: now + TRACK_LIFETIME / 2,
            duck: Some(DuckVisit {
                leg: DuckLeg::Land,
                edge,
            }),
            ..Habitat::default()
        };
        world.advance_world_ms(edge.end - now);
        assert_eq!(habitat.advance_duck(&world, &[BANK[0]]), None);
        assert_eq!(habitat.duck.unwrap().leg, DuckLeg::WaterBack);
        assert_eq!(habitat.marks(&world).count(), 0);
    }
}
