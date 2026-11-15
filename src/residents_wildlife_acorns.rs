//! Fixed acorn places. Each place owns one real nut from fall to consumption or germination.
use super::{
    ANIMALS, DAY_MS, Edge, MINUTE, Resume, WildlifeVisual, animal_visible, exit_itinerary,
    foothold, ground, point, route_tile, scenery_here, tile,
};
use crate::{
    living_world::{Ground, LivingWorld, Point, YardSeason, YardWeather},
    map_data::{TilePos, WILLOW_VILLAGE},
};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const SQUIRREL: usize = 2;
const CROW: usize = 11;
pub(super) const ANIMALS_WITH_CACHES: [usize; 2] = [SQUIRREL, CROW];
const CACHES: [TilePos; 3] = [
    TilePos::new(30, 71),
    TilePos::new(30, 70),
    TilePos::new(181, 16),
];
const FALL: [TilePos; 2] = [TilePos::new(32, 70), TilePos::new(184, 16)];
const LEAF: TilePos = TilePos::new(182, 16);
const LEAF_DRAIN: TilePos = TilePos::new(180, 16);
const STONE: TilePos = TilePos::new(181, 15);
const CROW_INSPECT: TilePos = TilePos::new(182, 15);
const YEAR_DAYS: u64 = 112;
const FALL_TIME: u64 = 38_000;
const SQUIRREL_CORNERS: [TilePos; 5] = [
    TilePos::new(30, 70),
    TilePos::new(30, 72),
    TilePos::new(31, 72),
    TilePos::new(32, 70),
    TilePos::new(32, 72),
];

fn source(i: usize) -> usize {
    static TREES: OnceLock<[usize; 2]> = OnceLock::new();
    TREES.get_or_init(|| {
        [
            "scenic.orchard_lane_cover.tree.32_69",
            "density.quarry_north_face.tree.183_15",
        ]
        .map(|id| {
            WILLOW_VILLAGE
                .scenic
                .iter()
                .position(|s| s.id == id)
                .expect("authored wildlife oak")
        })
    })[i]
}

fn rock(world: &LivingWorld) -> bool {
    static ROCK: OnceLock<usize> = OnceLock::new();
    scenery_here(
        world,
        *ROCK.get_or_init(|| {
            WILLOW_VILLAGE
                .scenic
                .iter()
                .position(|s| s.id == "density.quarry_north_face.rock.181_15")
                .expect("authored acorn crevice")
        }),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in super::super) enum Proof {
    SquirrelStore(TilePos),
    SquirrelRetrieve(TilePos),
    Sprout(TilePos),
    CrowStore,
    CrowRetrieve,
}

impl Proof {
    pub(super) fn facts(self) -> (&'static str, usize, TilePos, TilePos) {
        match self {
            Self::SquirrelStore(cache) => ("A05-01", SQUIRREL, cache, FALL[0]),
            Self::SquirrelRetrieve(cache) => ("A05-02", SQUIRREL, route_tile(SQUIRREL, 0), cache),
            Self::Sprout(cache) => ("A05-03", SQUIRREL, route_tile(SQUIRREL, 0), cache),
            Self::CrowStore => ("A15-01", CROW, CACHES[2], STONE),
            Self::CrowRetrieve => ("A15-02", CROW, route_tile(CROW, 0), CACHES[2]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squirrel_carry_detours_the_real_orchard_shrub_and_keeps_its_nut() {
        let world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        assert!(scenery_here(&world, source(0)));
        assert!(rock(&world));
        assert!(maybe_path(0, point(FALL[0]), CACHES[0], now, &world, &[]).is_none());
        let (edge, via) = path_leg(0, point(FALL[0]), CACHES[0], now, &world, &[]).unwrap();
        assert_eq!(via, Some(CACHES[1]));
        let held = Nut {
            place: Place::Held,
            since: now,
            year: 0,
        };
        let mut state = State {
            nuts: [Some(held), None, None],
            ..State::default()
        };
        assert!(!state.valid(&world));
        state.jobs[0] = Some(Job {
            slot: 0,
            leg: Leg::Hide,
            edge,
            retry: false,
            working: false,
            via,
        });
        let bytes = serde_json::to_vec(&state).unwrap();
        let reopened: State = serde_json::from_slice(&bytes).unwrap();
        assert!(reopened.valid(&world));
        assert_eq!(bytes, serde_json::to_vec(&reopened).unwrap());
        assert_eq!(
            reopened
                .marks(&world)
                .into_iter()
                .flatten()
                .filter(|v| v.sprite == "wild_acorn_held")
                .count(),
            1
        );
        assert!(path_leg(0, point(FALL[0]), CACHES[0], now, &world, &[CACHES[0]]).is_none());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Place {
    Falling,
    Fallen,
    Held,
    Buried { wet: bool },
    OpenCrevice,
    Covered,
    Washed,
    Sprout { seen: bool },
    YoungTree { seen: bool },
    Spent,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Nut {
    year: u64,
    since: u64,
    place: Place,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Leg {
    Pick,
    Hide,
    PickLeaf,
    Cover,
    CheckStone,
    Recover,
    Look,
    LeaveSprout,
    CarryBack,
    DropNut,
    Return,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    slot: u8,
    leg: Leg,
    edge: Edge,
    retry: bool,
    working: bool,
    #[serde(default)]
    via: Option<TilePos>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    nuts: [Option<Nut>; 3],
    jobs: [Option<Job>; 2],
    resumes: [Option<Resume>; 2],
    leaf_drift: Option<Edge>,
    leaf_scraps_until: Option<u64>,
}

fn owner(slot: usize) -> usize {
    usize::from(slot == 2)
}

fn destination(leg: Leg, slot: usize) -> TilePos {
    match leg {
        Leg::Pick | Leg::DropNut => FALL[owner(slot)],
        Leg::Hide | Leg::Cover | Leg::Recover | Leg::Look => CACHES[slot],
        Leg::PickLeaf => LEAF,
        Leg::CheckStone => CROW_INSPECT,
        Leg::LeaveSprout | Leg::CarryBack | Leg::Return => {
            route_tile(ANIMALS_WITH_CACHES[owner(slot)], 0)
        }
    }
}

fn work_time(leg: Leg) -> Option<u64> {
    match leg {
        Leg::Pick | Leg::Cover | Leg::CheckStone => Some(30_000),
        Leg::Hide | Leg::Recover => Some(70_000),
        Leg::Look | Leg::CarryBack | Leg::DropNut => Some(35_000),
        _ => None,
    }
}

fn maybe_path(
    i: usize,
    from: Point,
    to: TilePos,
    now: u64,
    world: &LivingWorld,
    occupied: &[TilePos],
) -> Option<Edge> {
    let edge = Edge::travel(from, point(to), now, if i == 0 { 38_000 } else { 31_000 });
    (foothold(world, occupied, to) && (i == 1 || exit_itinerary(world, edge))).then_some(edge)
}

fn path_leg(
    i: usize,
    from: Point,
    to: TilePos,
    now: u64,
    world: &LivingWorld,
    occupied: &[TilePos],
) -> Option<(Edge, Option<TilePos>)> {
    if let Some(edge) = maybe_path(i, from, to, now, world, occupied) {
        return Some((edge, None));
    }
    if i != 0 || !foothold(world, occupied, to) {
        return None;
    }
    SQUIRREL_CORNERS.into_iter().find_map(|corner| {
        if corner == to || corner == tile(from) {
            return None;
        }
        let first = maybe_path(i, from, corner, now, world, occupied)?;
        maybe_path(i, first.to, to, first.end, world, occupied)?;
        Some((first, Some(corner)))
    })
}

fn dry(world: &LivingWorld) -> bool {
    matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds)
}

fn digs(world: &LivingWorld, slot: usize) -> bool {
    ground(world, CACHES[slot]) == Ground::Grass
}

fn set(nut: &mut Nut, now: u64, place: Place) {
    nut.place = place;
    nut.since = now;
}

impl State {
    pub(super) fn empty(&self) -> bool {
        self.nuts.iter().all(Option::is_none)
            && self.jobs.iter().all(Option::is_none)
            && self.resumes.iter().all(Option::is_none)
            && self.leaf_drift.is_none()
            && self.leaf_scraps_until.is_none()
    }

    pub(super) fn deadline(&self) -> u64 {
        self.jobs
            .iter()
            .flatten()
            .map(|j| j.edge.end)
            .chain(self.leaf_drift.map(|e| e.end))
            .chain(
                self.nuts
                    .iter()
                    .flatten()
                    .filter(|n| n.place == Place::Falling)
                    .map(|n| n.since.saturating_add(FALL_TIME)),
            )
            .min()
            .unwrap_or(u64::MAX)
    }

    pub(super) fn crow_checks_flock(&self, world: &LivingWorld, probe: bool) -> bool {
        if let Some(j) = self.jobs[1] {
            return j.edge.end <= world.world_ms() && matches!(j.leg, Leg::Pick | Leg::Hide);
        }
        probe
            && dry(world)
            && (400..530).contains(&(world.world_ms() % DAY_MS / MINUTE))
            && (world.season() == YardSeason::Autumn && self.nuts[2].is_none()
                || self.nuts[2]
                    .is_some_and(|n| matches!(n.place, Place::Fallen | Place::OpenCrevice)))
    }

    pub(super) fn resume(&self, id: usize) -> Option<Resume> {
        self.resumes[ANIMALS_WITH_CACHES
            .iter()
            .position(|&animal| id == animal)?]
    }

    pub(super) fn valid(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        self.nuts.iter().enumerate().all(|(slot, nut)| {
            nut.is_none_or(|n| {
                n.since <= now
                    && n.year <= n.since / DAY_MS / YEAR_DAYS
                    && match n.place {
                        Place::Buried { .. } => slot != 2,
                        Place::Sprout { .. } | Place::YoungTree { .. } => slot == 1,
                        Place::OpenCrevice | Place::Covered | Place::Washed => slot == 2,
                        Place::Held => self.jobs[owner(slot)].is_some_and(|j| {
                            usize::from(j.slot) == slot
                                && matches!(j.leg, Leg::Hide | Leg::CarryBack | Leg::DropNut)
                        }),
                        _ => true,
                    }
            })
        }) && self.jobs.iter().enumerate().all(|(i, job)| {
            job.is_none_or(|j| {
                let slot = usize::from(j.slot);
                let Some(n) = self.nuts.get(slot).copied().flatten() else {
                    return false;
                };
                owner(slot) == i
                    && (if i == 0 {
                        j.edge.valid(now, 29..=34, 69..=74)
                    } else {
                        j.edge.valid(now, 179..=184, 14..=21)
                    })
                    && (if j.retry {
                        j.edge.from == j.edge.to
                    } else {
                        j.edge.to == point(j.via.unwrap_or_else(|| destination(j.leg, slot)))
                    })
                    && j.via.is_none_or(|via| {
                        i == 0 && !j.retry && !j.working && SQUIRREL_CORNERS.contains(&via)
                    })
                    && (!j.working
                        || (!j.retry && j.edge.from == j.edge.to && work_time(j.leg).is_some()))
                    && match j.leg {
                        Leg::Pick => n.place == Place::Fallen,
                        Leg::Hide | Leg::CarryBack | Leg::DropNut => n.place == Place::Held,
                        Leg::PickLeaf | Leg::Cover => n.place == Place::OpenCrevice,
                        Leg::CheckStone => n.place == Place::Washed,
                        Leg::Recover => matches!(n.place, Place::Buried { .. } | Place::Washed),
                        Leg::Look | Leg::LeaveSprout => matches!(
                            n.place,
                            Place::Sprout { seen: false } | Place::YoungTree { seen: false }
                        ),
                        Leg::Return => !matches!(n.place, Place::Held),
                    }
            })
        }) && self
            .resumes
            .iter()
            .flatten()
            .all(|r| r.at <= now && r.waypoint == 0)
            && self.leaf_drift.is_none_or(|e| {
                e.valid(now, 179..=183, 16..=16)
                    && e.from == point(CACHES[2])
                    && e.to == point(LEAF_DRAIN)
                    && self.nuts[2].is_some_and(|n| n.place == Place::Washed)
            })
            && self
                .leaf_scraps_until
                .is_none_or(|end| end <= now.saturating_add(DAY_MS))
    }

    pub(super) fn advance(
        &mut self,
        world: &LivingWorld,
        occupied: &[TilePos],
        probe: bool,
        positions: [Point; 2],
        birds_near_crevice: bool,
    ) -> [Option<Proof>; 2] {
        let now = world.world_ms();
        let day = now / DAY_MS;
        let year = day / YEAR_DAYS;
        let rain = world.weather() == YardWeather::Rain;
        for n in self
            .nuts
            .iter_mut()
            .flatten()
            .filter(|n| n.place == Place::Falling)
        {
            if n.since.saturating_add(FALL_TIME) <= now {
                set(n, now, Place::Fallen);
            }
        }
        if self.leaf_drift.is_some_and(|e| e.end <= now) {
            self.leaf_drift = None;
        }
        if probe {
            if self.leaf_scraps_until.is_some_and(|end| end <= now) {
                self.leaf_scraps_until = None;
            }
            for slot in 0..3 {
                let i = owner(slot);
                if world.season() == YardSeason::Autumn
                    && dry(world)
                    && self.nuts[slot].is_none_or(|n| n.place == Place::Spent && n.year < year)
                    && scenery_here(world, source(i))
                    && (slot != 1
                        || (day % YEAR_DAYS >= 60 && self.nuts[0].is_some_and(|n| n.year == year)))
                    && (slot == 2
                        || self.nuts[1 - slot]
                            .is_none_or(|n| !matches!(n.place, Place::Fallen | Place::Falling)))
                {
                    self.nuts[slot] = Some(Nut {
                        year,
                        since: now,
                        place: Place::Falling,
                    });
                }
                let Some(n) = &mut self.nuts[slot] else {
                    continue;
                };
                if let Place::Buried { wet } = &mut n.place {
                    *wet |= rain;
                    if slot == 1
                        && *wet
                        && world.season() == YardSeason::Spring
                        && year > n.year
                        && digs(world, slot)
                    {
                        set(n, now, Place::Sprout { seen: false });
                    }
                }
                if let Place::Sprout { seen } = n.place
                    && now.saturating_sub(n.since) >= 12 * DAY_MS
                    && digs(world, slot)
                {
                    set(n, now, Place::YoungTree { seen });
                }
                if slot == 2 && n.place == Place::Covered && rain && rock(world) {
                    set(n, now, Place::Washed);
                    self.leaf_drift = Some(Edge::travel(
                        point(CACHES[2]),
                        point(LEAF_DRAIN),
                        now,
                        65_000,
                    ));
                }
            }
        }
        [0, 1].map(|i| {
            if self.jobs[i].is_some() {
                return self.step(i, world, occupied, dry(world), birds_near_crevice);
            }
            if !probe
                || !dry(world)
                || !animal_visible(
                    ANIMALS_WITH_CACHES[i],
                    ANIMALS[ANIMALS_WITH_CACHES[i]],
                    world,
                )
            {
                return None;
            }
            let slots = if i == 0 { 0..2 } else { 2..3 };
            let minute = now % DAY_MS / MINUTE;
            for slot in slots {
                let Some(n) = self.nuts[slot] else {
                    continue;
                };
                let cache_free = foothold(world, occupied, CACHES[slot]);
                let leg = match n.place {
                    Place::Fallen
                        if cache_free
                            && (i == 0 && digs(world, slot)
                                || i == 1 && rock(world) && !birds_near_crevice) =>
                    {
                        Leg::Pick
                    }
                    Place::Buried { .. }
                        if slot == 0
                            && (350..440).contains(&minute)
                            && day > n.since / DAY_MS
                            && cache_free
                            && digs(world, slot) =>
                    {
                        Leg::Recover
                    }
                    Place::Washed if self.leaf_drift.is_none() && rock(world) && cache_free => {
                        Leg::CheckStone
                    }
                    Place::OpenCrevice if rock(world) && cache_free && !birds_near_crevice => {
                        Leg::PickLeaf
                    }
                    Place::Sprout { seen: false } | Place::YoungTree { seen: false }
                        if cache_free && digs(world, slot) =>
                    {
                        Leg::Look
                    }
                    _ => continue,
                };
                if let Some((edge, via)) = path_leg(
                    i,
                    positions[i],
                    destination(leg, slot),
                    now,
                    world,
                    occupied,
                ) {
                    self.jobs[i] = Some(Job {
                        slot: slot as u8,
                        leg,
                        edge,
                        retry: false,
                        working: false,
                        via,
                    });
                    break;
                }
            }
            None
        })
    }

    fn step(
        &mut self,
        i: usize,
        world: &LivingWorld,
        occupied: &[TilePos],
        dry: bool,
        other_bird: bool,
    ) -> Option<Proof> {
        let job = self.jobs[i]?;
        let now = world.world_ms();
        if now < job.edge.end {
            return None;
        }
        let slot = usize::from(job.slot);
        let at = job.edge.to;
        if job.via.is_some() {
            self.route(i, job, job.leg, at, now, world, occupied);
            return None;
        }
        if !job.retry
            && !job.working
            && let Some(duration) = work_time(job.leg)
            && foothold(world, occupied, tile(at))
        {
            self.jobs[i] = Some(Job {
                edge: Edge::hold(at, now, duration),
                working: true,
                ..job
            });
            return None;
        }
        let n = self.nuts[slot].as_mut().expect("active acorn slot");
        let ground_safe = if i == 0 {
            digs(world, slot)
        } else {
            rock(world)
        };
        let (leg, proof) = if job.retry {
            (job.leg, None)
        } else if !foothold(world, occupied, tile(at)) {
            // At a blocked landing do not apply a pickup or interaction.
            self.route(i, job, job.leg, at, now, world, occupied);
            return None;
        } else {
            match job.leg {
                Leg::Pick if dry && ground_safe && (i == 0 || !other_bird) => {
                    set(n, now, Place::Held);
                    (Leg::Hide, None)
                }
                Leg::Pick => (Leg::Return, None),
                Leg::Hide if !ground_safe => (Leg::DropNut, None),
                Leg::Hide if i == 0 => {
                    set(n, now, Place::Buried { wet: false });
                    (Leg::Return, Some(Proof::SquirrelStore(CACHES[slot])))
                }
                Leg::Hide if !other_bird => {
                    set(n, now, Place::OpenCrevice);
                    (Leg::PickLeaf, None)
                }
                Leg::Hide => (Leg::DropNut, None),
                Leg::PickLeaf if rock(world) => (Leg::Cover, None),
                Leg::Cover if rock(world) => {
                    set(n, now, Place::Covered);
                    (Leg::Return, Some(Proof::CrowStore))
                }
                Leg::PickLeaf | Leg::Cover => {
                    // A failed cover leaves the same nut physically in the open crevice.
                    (Leg::Return, None)
                }
                Leg::CheckStone if dry && ground_safe => (Leg::Recover, None),
                Leg::CheckStone => (Leg::Return, None),
                Leg::Recover if !dry || !ground_safe => (Leg::Return, None),
                Leg::Recover => {
                    set(n, now, Place::Held);
                    if i == 1 {
                        self.leaf_scraps_until = Some(now + 90 * MINUTE);
                    }
                    (Leg::CarryBack, None)
                }
                Leg::Look if ground_safe => (Leg::LeaveSprout, None),
                Leg::Look => (Leg::Return, None),
                Leg::LeaveSprout => {
                    if ground_safe {
                        n.place = match n.place {
                            Place::YoungTree { .. } => Place::YoungTree { seen: true },
                            _ => Place::Sprout { seen: true },
                        };
                    }
                    self.resumes[i] = Some(Resume {
                        at: now,
                        waypoint: 0,
                    });
                    self.jobs[i] = None;
                    return ground_safe.then_some(Proof::Sprout(CACHES[slot]));
                }
                Leg::CarryBack => {
                    set(n, now, Place::Spent);
                    self.resumes[i] = Some(Resume {
                        at: now,
                        waypoint: 0,
                    });
                    self.jobs[i] = None;
                    return Some(if i == 0 {
                        Proof::SquirrelRetrieve(CACHES[slot])
                    } else {
                        Proof::CrowRetrieve
                    });
                }
                Leg::DropNut => {
                    set(n, now, Place::Fallen);
                    (Leg::Return, None)
                }
                Leg::Return => {
                    self.resumes[i] = Some(Resume {
                        at: now,
                        waypoint: 0,
                    });
                    self.jobs[i] = None;
                    return None;
                }
            }
        };
        self.route(i, job, leg, at, now, world, occupied);
        proof
    }

    fn route(
        &mut self,
        i: usize,
        job: Job,
        leg: Leg,
        at: Point,
        now: u64,
        world: &LivingWorld,
        occupied: &[TilePos],
    ) {
        let route = path_leg(
            i,
            at,
            destination(leg, usize::from(job.slot)),
            now,
            world,
            occupied,
        );
        self.jobs[i] = Some(Job {
            slot: job.slot,
            leg,
            edge: route.map_or_else(|| Edge::hold(at, now, 2 * MINUTE), |(edge, _)| edge),
            retry: route.is_none(),
            working: false,
            via: route.and_then(|(_, via)| via),
        });
    }

    pub(super) fn visual(&self, id: usize, world: &LivingWorld) -> Option<WildlifeVisual> {
        let i = ANIMALS_WITH_CACHES.iter().position(|&a| id == a)?;
        let job = self.jobs[i]?;
        let now = world.world_ms();
        let frame = usize::from(job.edge.fraction(now) > 0.55);
        let sprite = if i == 0 {
            if job.edge.from == job.edge.to || now >= job.edge.end {
                "wild_squirrel_forage_0"
            } else {
                ["wild_squirrel_leap_0", "wild_squirrel_land_0"][frame]
            }
        } else if job.edge.from == job.edge.to || now >= job.edge.end {
            "wild_crow_idle_0"
        } else {
            ["wild_crow_fly_0", "wild_crow_fly_1"][frame]
        };
        Some(WildlifeVisual {
            sprite,
            position: job.edge.position(now),
            facing_left: job.edge.to.x < job.edge.from.x,
        })
    }

    pub(super) fn marks(&self, world: &LivingWorld) -> [Option<WildlifeVisual>; 6] {
        let marker = |sprite, position, facing_left| {
            Some(WildlifeVisual {
                sprite,
                position,
                facing_left,
            })
        };
        let nuts = [0, 1, 2].map(|slot| {
            let nut = self.nuts[slot]?;
            let i = owner(slot);
            let (sprite, at) = match nut.place {
                Place::Falling => {
                    let mut from = point(WILLOW_VILLAGE.scenic[source(i)].tile);
                    from.y -= 12.0;
                    return marker(
                        "wild_acorn",
                        Edge::new(from, point(FALL[i]), nut.since, FALL_TIME)
                            .position(world.world_ms()),
                        false,
                    );
                }
                Place::Fallen => ("wild_acorn", FALL[i]),
                Place::Buried { .. } if digs(world, slot) => {
                    ("wild_acorn_covered_soil", CACHES[slot])
                }
                Place::Sprout { .. } if digs(world, slot) => ("wild_acorn_sprout", CACHES[slot]),
                Place::YoungTree { .. } if digs(world, slot) => {
                    ("wild_acorn_young_oak", CACHES[slot])
                }
                Place::Covered if rock(world) => ("wild_acorn_cover_leaf", CACHES[slot]),
                Place::Washed | Place::OpenCrevice if rock(world) => ("wild_acorn", CACHES[slot]),
                Place::Held => {
                    let actor = self.visual(ANIMALS_WITH_CACHES[i], world)?;
                    let sign = if actor.facing_left { -1.0 } else { 1.0 };
                    return marker(
                        "wild_acorn_held",
                        Point {
                            x: actor.position.x + sign * 2.5,
                            y: actor.position.y - 4.0,
                        },
                        actor.facing_left,
                    );
                }
                _ => return None,
            };
            marker(sprite, point(at), false)
        });
        let drift = self.nuts[2]
            .filter(|n| n.place == Place::Washed)
            .and_then(|_| {
                let at = self
                    .leaf_drift
                    .map_or(point(LEAF_DRAIN), |e| e.position(world.world_ms()));
                marker("wild_acorn_cover_leaf", at, false)
            });
        let scraps = self
            .leaf_scraps_until
            .filter(|&end| end > world.world_ms())
            .and_then(|_| marker("wild_acorn_leaf_scraps", point(CACHES[2]), false));
        let cover = self.nuts[2].and_then(|n| {
            if n.place == Place::Fallen
                || n.place == Place::Held && self.jobs[1].is_some_and(|j| j.leg != Leg::CarryBack)
                || n.place == Place::OpenCrevice && self.jobs[1].is_none_or(|j| j.leg != Leg::Cover)
            {
                marker("wild_acorn_cover_leaf", point(LEAF), false)
            } else if self.jobs[1].is_some_and(|j| j.leg == Leg::Cover) {
                let actor = self.visual(CROW, world)?;
                marker(
                    "wild_acorn_leaf_held",
                    Point {
                        x: actor.position.x + if actor.facing_left { -3.0 } else { 3.0 },
                        y: actor.position.y - 4.0,
                    },
                    actor.facing_left,
                )
            } else {
                None
            }
        });
        [nuts[0], nuts[1], nuts[2], drift, scraps, cover]
    }
}
