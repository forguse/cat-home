//! One swallow revisits the same constructed mud nest.
use super::routes::Proof;
use super::{
    ANIMALS, DAY_MS, Edge, HOUR, MINUTE, Resume, Roof, SWALLOW, WildlifeVisual, animal_visible,
    foothold, ground, point, route_tile, tile, weather_wet,
};
use crate::{
    living_world::{Ground, LivingWorld, Point, YardSeason, YardWeather},
    map_data::TilePos,
};
use serde::{Deserialize, Serialize};

const BANK: TilePos = TilePos::new(66, 40);
const CREEK: TilePos = TilePos::new(65, 40);
const TRAVEL: u64 = 27_000;
const MIX_TIME: u64 = 95_000;
const YEAR: u64 = 112 * DAY_MS;
const ROOF: Roof = Roof::WeaverHome;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Leg {
    ToMud,
    Collect,
    ToRoof,
    Fit,
    Recognize,
    Rest,
    PutBack,
    Return,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Visit {
    leg: Leg,
    edge: Edge,
    /// Same supported unit of wet sediment, collected only after real contact.
    carries: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    visit: Option<Visit>,
    wet: Option<bool>,
    moisture: Option<u64>,
    source_spent: Option<u64>,
    first_built_year: Option<u64>,
    last_reused: Option<u64>,
    courses: u8,
    resume: Option<Resume>,
}

fn dry(world: &LivingWorld) -> bool {
    matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds)
}

fn bank(world: &LivingWorld, occupied: &[TilePos]) -> bool {
    ground(world, CREEK) == Ground::Water
        && ground(world, BANK) == Ground::Grass
        && foothold(world, occupied, BANK)
}

fn root() -> Point {
    point(route_tile(SWALLOW, 0))
}

impl State {
    pub(super) fn empty(&self) -> bool {
        self.wet.is_none()
    }
    pub(super) fn active(&self) -> bool {
        self.visit.is_some()
    }
    pub(super) fn deadline(&self) -> u64 {
        self.visit.map_or(u64::MAX, |v| v.edge.end)
    }
    pub(super) fn resume(&self, id: usize) -> Option<Resume> {
        if id == SWALLOW { self.resume } else { None }
    }

    pub(super) fn valid(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        self.courses <= 2
            && (self.courses > 0) == self.first_built_year.is_some()
            && self.first_built_year.is_none_or(|y| y <= now / YEAR)
            && self.last_reused.is_none_or(|y| {
                self.courses == 2 && Some(y) > self.first_built_year && y <= now / YEAR
            })
            && self.moisture.is_none_or(|t| t <= now)
            && self.source_spent.is_none_or(|t| t <= now)
            && self.resume.is_none_or(|r| r.at <= now && r.waypoint == 0)
            && self.visit.is_none_or(|v| {
                v.edge.valid(now, 62..=84, 20..=43)
                    && match v.leg {
                        Leg::ToMud | Leg::Collect => !v.carries && v.edge.to == point(BANK),
                        Leg::ToRoof | Leg::Fit => {
                            v.carries && self.courses < 2 && v.edge.to == ROOF.eave()
                        }
                        Leg::Recognize | Leg::Rest => {
                            !v.carries && self.courses == 2 && v.edge.to == ROOF.eave()
                        }
                        Leg::PutBack => v.carries && v.edge.to == point(BANK),
                        Leg::Return => !v.carries && v.edge.to == root(),
                    }
            })
    }

    pub(super) fn advance(
        &mut self,
        world: &LivingWorld,
        occupied: &[TilePos],
        probe: bool,
        allowed: bool,
        source: Point,
    ) -> Option<Proof> {
        let now = world.world_ms();
        let year = now / YEAR;
        let roof = world.village_building_active(ROOF.id());
        if probe {
            let wet = weather_wet(world);
            if self.wet.replace(wet) == Some(true) && !wet {
                self.moisture = Some(now);
            }
            if !roof {
                self.courses = 0;
                self.first_built_year = None;
                self.last_reused = None;
            }
        }
        let spring = world.season() == YardSeason::Spring;
        let active = animal_visible(SWALLOW, ANIMALS[SWALLOW], world);
        let soil_available = self
            .moisture
            .is_some_and(|time| now < time + HOUR && self.source_spent != Some(time));
        let Some(mut visit) = self.visit else {
            if !probe
                || !allowed
                || !roof
                || !dry(world)
                || !active
                || !spring
                || now / DAY_MS % 28 >= 11
                || !(580..985).contains(&(now % DAY_MS / MINUTE))
            {
                return None;
            }
            if self.courses < 2 && soil_available && bank(world, occupied) {
                self.visit = Some(Visit {
                    leg: Leg::ToMud,
                    edge: Edge::travel(source, point(BANK), now, TRAVEL),
                    carries: false,
                });
            } else if self.courses == 2
                && self.first_built_year.is_some_and(|built| year > built)
                && self.last_reused != Some(year)
            {
                self.visit = Some(Visit {
                    leg: Leg::Recognize,
                    edge: Edge::travel(source, ROOF.eave(), now, TRAVEL),
                    carries: false,
                });
            }
            return None;
        };
        if visit.leg == Leg::Return && now >= visit.edge.end {
            self.resume = Some(Resume {
                at: now,
                waypoint: 0,
            });
            self.visit = None;
            return None;
        }
        if !matches!(visit.leg, Leg::Return | Leg::PutBack)
            && (!dry(world) || !roof || !spring || !active)
        {
            let (leg, destination) = if visit.carries {
                (Leg::PutBack, point(BANK))
            } else {
                (Leg::Return, root())
            };
            visit.leg = leg;
            visit.edge = Edge::travel(visit.edge.position(now), destination, now, TRAVEL);
            self.visit = Some(visit);
            return None;
        }
        if now < visit.edge.end {
            return None;
        }
        let at = visit.edge.to;
        let mut proof = None;
        let (leg, edge) = match visit.leg {
            Leg::ToMud if soil_available && bank(world, occupied) => {
                (Leg::Collect, Edge::hold(at, now, 60_000))
            }
            Leg::Collect if soil_available && bank(world, occupied) => {
                self.source_spent = self.moisture;
                visit.carries = true;
                (Leg::ToRoof, Edge::travel(at, ROOF.eave(), now, TRAVEL))
            }
            Leg::ToRoof => (Leg::Fit, Edge::hold(at, now, MIX_TIME)),
            Leg::Fit if visit.carries && self.courses < 2 => {
                visit.carries = false;
                self.courses += 1;
                if self.courses == 1 {
                    self.first_built_year = Some(year);
                    proof = Some(Proof::SwallowFoundation {
                        source: BANK,
                        eave: tile(ROOF.eave()),
                    });
                }
                (Leg::Return, Edge::travel(at, root(), now, TRAVEL))
            }
            Leg::Recognize if self.courses == 2 => (Leg::Rest, Edge::hold(at, now, MIX_TIME)),
            Leg::Rest => {
                self.last_reused = Some(year);
                proof = Some(Proof::SwallowOldNest(tile(ROOF.eave())));
                (Leg::Return, Edge::travel(at, root(), now, TRAVEL))
            }
            Leg::PutBack => {
                visit.carries = false;
                (Leg::Return, Edge::travel(at, root(), now, TRAVEL))
            }
            _ => (Leg::Return, Edge::travel(at, root(), now, TRAVEL)),
        };
        self.visit = Some(Visit { leg, edge, ..visit });
        proof
    }

    pub(super) fn visual(&self, world: &LivingWorld) -> Option<WildlifeVisual> {
        let v = self.visit?;
        let now = world.world_ms();
        let sprite = if matches!(v.leg, Leg::Collect | Leg::Fit | Leg::Rest) {
            "wild_swallow_perch_0"
        } else if now + 20_000 >= v.edge.end
            && matches!(v.leg, Leg::ToRoof | Leg::ToMud | Leg::Recognize)
        {
            "wild_swallow_land_0"
        } else {
            ANIMALS[SWALLOW].frames[(now / 22_000 % 3) as usize]
        };
        Some(WildlifeVisual {
            sprite,
            position: v.edge.position(now),
            facing_left: v.edge.to.x < v.edge.from.x,
        })
    }

    pub(super) fn marks(
        &self,
        world: &LivingWorld,
    ) -> impl Iterator<Item = WildlifeVisual> + use<> {
        [
            (self.courses > 0 && world.village_building_active(ROOF.id())).then(|| {
                WildlifeVisual {
                    position: ROOF.eave(),
                    facing_left: false,
                    sprite: if self.courses == 2 {
                        "wild_swallow_nest"
                    } else {
                        "wild_swallow_nest_base"
                    },
                }
            }),
            self.visit.filter(|v| v.carries).map(|v| WildlifeVisual {
                position: v.edge.position(world.world_ms()),
                facing_left: v.edge.to.x < v.edge.from.x,
                sprite: "wild_swallow_mud_in_beak",
            }),
        ]
        .into_iter()
        .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrive(world: &mut LivingWorld, deadline: u64) {
        world.queue_world_time(deadline - world.world_ms());
        while !world.advance_pending_to(deadline) {}
    }

    #[test]
    fn only_physically_fitting_the_carried_mud_creates_the_first_course() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        arrive(&mut world, 4 * DAY_MS + 10 * HOUR);
        while !dry(&world) || !bank(&world, &[]) {
            let tomorrow = (world.world_ms() / DAY_MS + 1) * DAY_MS + 10 * HOUR;
            arrive(&mut world, tomorrow);
            assert!(tomorrow < 11 * DAY_MS);
        }
        let now = world.world_ms();
        let mut state = State {
            wet: Some(false),
            moisture: Some(now - 1),
            ..Default::default()
        };
        assert_eq!(state.advance(&world, &[], true, true, root()), None);
        assert_eq!(state.visit.unwrap().leg, Leg::ToMud);
        arrive(&mut world, state.deadline());
        let save = serde_json::to_vec(&state).unwrap();
        let mut blocked: State = serde_json::from_slice(&save).unwrap();
        assert_eq!(blocked.advance(&world, &[BANK], true, false, root()), None);
        assert_eq!(blocked.courses, 0);
        assert!(blocked.source_spent.is_none());
        assert_eq!(state.advance(&world, &[], true, false, root()), None);
        assert_eq!(state.visit.unwrap().leg, Leg::Collect);
        arrive(&mut world, state.deadline());
        assert_eq!(state.advance(&world, &[], true, false, root()), None);
        assert!(state.visit.unwrap().carries);
        let index = state.source_spent;
        arrive(&mut world, state.deadline());
        assert_eq!(state.advance(&world, &[], true, false, root()), None);
        assert_eq!(state.visit.unwrap().leg, Leg::Fit);
        assert_eq!(state.courses, 0);
        let mut reopened: State =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        arrive(&mut world, state.deadline());
        assert_eq!(
            reopened.advance(&world, &[], true, false, root()),
            Some(Proof::SwallowFoundation {
                source: BANK,
                eave: tile(ROOF.eave()),
            })
        );
        assert_eq!(reopened.courses, 1);
        assert_eq!(reopened.source_spent, index);
        assert!(reopened.valid(&world));
        assert_eq!(reopened.marks(&world).count(), 1);
        assert_eq!(reopened.advance(&world, &[], true, false, root()), None);
        assert_eq!(reopened.courses, 1);
    }

    #[test]
    fn spring_revisit_records_the_same_occupied_nest_in_both_weather_windows() {
        use crate::living_world::Observation;
        let initial = LivingWorld::new_village_wall_ms(0);
        let sample = |now: u64| {
            let mut data = serde_json::to_value(&initial).unwrap();
            data["now_ms"] = now.into();
            data["target_ms"] = now.into();
            serde_json::from_value::<LivingWorld>(data).unwrap()
        };
        for (id, weather) in [("A09-08~10042", YardWeather::Clear), ("A09-08~11042", YardWeather::Clouds)] {
            let mut world = (112..123).map(|day| sample(day * DAY_MS + 10 * HOUR))
                .find(|world| world.weather() == weather && animal_visible(SWALLOW, ANIMALS[SWALLOW], world))
                .expect("regular spring visit window");
            let mut nest = State {
                wet: Some(false),
                first_built_year: Some(0),
                courses: 2,
                ..State::default()
            };
            assert!(nest.valid(&world));
            assert!(nest.advance(&world, &[], true, true, root()).is_none());
            assert_eq!(nest.visit.unwrap().leg, Leg::Recognize);
            world = sample(nest.deadline());
            assert!(nest.advance(&world, &[], false, true, root()).is_none());
            assert_eq!(nest.visit.unwrap().leg, Leg::Rest);
            world = sample(nest.deadline());
            nest.advance(&world, &[], false, true, root()).unwrap().record(&mut world);
            assert!(world.legacy_behaviors().observed_ids().any(|seen| seen == id));
            assert_eq!(nest.last_reused, Some(1));
            assert_eq!(nest.courses, 2);
            let note = world.active_village_observations().find(|note| note.kind == Observation::OldNest).unwrap();
            assert_eq!(note.target, tile(ROOF.eave()));
            assert_eq!(note.expires(), world.world_ms() + 4 * HOUR);
        }
    }
}
