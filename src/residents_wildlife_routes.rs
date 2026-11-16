//! Rain-border and overnight routes for the same animals already in the village.
use super::{
    ANIMALS, DAY_MS, Edge, HOUR, MINUTE, Resume, WildlifeVisual, animal_visible, exit_itinerary,
    foothold, ground, land, point, scenery_here, tile, weather_wet,
};
use crate::{
    legacy_content::{LegacyActor, LegacyPlace},
    living_world::{Ground, LivingWorld, Point, YardSeason, YardWeather},
    map_data::{TilePos, WILLOW_VILLAGE},
};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[path = "residents_wildlife_rabbit.rs"]
mod rabbit;

const SNAIL: usize = 14;
const FIREFLY: usize = 16;
const TRACE: [TilePos; 3] = [
    TilePos::new(21, 23),
    TilePos::new(21, 22),
    TilePos::new(21, 21),
];
const SHADE: TilePos = TilePos::new(20, 22);
const TRACE_TIME: u64 = 75 * MINUTE;
const DRY_TIME: u64 = 3 * HOUR;
const SNAIL_CRAWL: u64 = 100_000;
const FIREFLY_EDGE: u64 = 36_000;
const DAWN: u64 = 5 * HOUR;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in super::super) enum Proof {
    SnailTrail,
    SnailDryRest,
    FireflyResume(TilePos, TilePos),
    FireflyDawn(TilePos),
    SharedLanding,
    Tailwind { from: TilePos, to: TilePos },
    Acorn(super::acorns::Proof),
    SwallowFoundation { source: TilePos, eave: TilePos },
    SwallowOldNest(TilePos),
    RabbitResumed { from: TilePos, cover: TilePos },
}

impl Proof {
    pub(in super::super) fn record(self, world: &mut LivingWorld) {
        let (base, id, at, target, place) = match self {
            Self::SnailTrail => ("A11-01", SNAIL, TRACE[0], TRACE[2], LegacyPlace::Front),
            Self::SnailDryRest => ("A11-05", SNAIL, SHADE, SHADE, LegacyPlace::Nearby),
            Self::FireflyResume(at, target) => ("A13-05", FIREFLY, at, target, LegacyPlace::Nearby),
            Self::FireflyDawn(at) => ("A13-10", FIREFLY, at, at, LegacyPlace::Nearby),
            Self::RabbitResumed { from, cover } => {
                ("A07-09", rabbit::RABBIT, from, cover, LegacyPlace::Front)
            }
            Self::SharedLanding => {
                world.record_legacy_completion(
                    "S05",
                    LegacyActor::Wildlife(super::SWALLOW as u8),
                    super::flock::ALTERNATE,
                    super::flock::RAIL,
                    LegacyPlace::Front,
                    1,
                    Some(LegacyActor::Wildlife(super::SPARROW as u8)),
                );
                return;
            }
            Self::SwallowFoundation { source, eave } => (
                "A09-01",
                super::SWALLOW,
                eave,
                source,
                LegacyPlace::Front,
            ),
            Self::SwallowOldNest(eave) => {
                world.record_village_observation(
                    crate::living_world::Observation::OldNest,
                    LegacyActor::Wildlife(super::SWALLOW as u8), eave, eave, LegacyPlace::Front,
                );
                return;
            }
            Self::Tailwind { from, to } => ("S15", super::SWALLOW, to, from, LegacyPlace::Nearby),
            Self::Acorn(proof) => {
                let (base, animal, at, target) = proof.facts();
                (base, animal, at, target, LegacyPlace::Nearby)
            }
        };
        world.record_legacy_completion(
            base,
            LegacyActor::Wildlife(id as u8),
            at,
            target,
            place,
            1,
            None,
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum SnailLeg {
    ToTrail,
    Trace(u8),
    ToShade,
    Drying,
    Closed,
    Retreat,
    Rejoin,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snail {
    leg: SnailLeg,
    edge: Edge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Reed {
    Pond,
    Wetland,
}

impl Reed {
    const fn tile(self) -> TilePos {
        match self {
            Self::Pond => TilePos::new(20, 38),
            Self::Wetland => TilePos::new(74, 111),
        }
    }

    fn leaf(self) -> Point {
        let p = point(self.tile());
        Point {
            x: p.x + 1.0,
            y: p.y - 6.0,
        }
    }

    fn source(self, world: &LivingWorld) -> bool {
        scenic(world, self as usize + 1)
    }

    fn selected(world: &LivingWorld) -> Self {
        if crate::residents::phase_active(world.phase_mask(), "late") {
            Self::Wetland
        } else {
            Self::Pond
        }
    }

    fn flight_start(self) -> Point {
        let base = super::route_tile(FIREFLY, 0);
        point(match self {
            Self::Pond => base,
            Self::Wetland => base.offset(57, 77).expect("wetland patrol"),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum FireflyLeg {
    RainLanding,
    RainWait,
    RainTakeoff,
    DawnLanding,
    DayRest,
    AtReed,
    EveningTakeoff,
    Rejoin,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Firefly {
    reed: Reed,
    leg: FireflyLeg,
    edge: Edge,
    establish: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    snail: Option<Snail>,
    snail_resume: Option<Resume>,
    snail_day: Option<u64>,
    wet_since: Option<u64>,
    dry_since: Option<u64>,
    trace_until: u64,
    trace_pieces: u8,
    fly: Option<Firefly>,
    fly_resume: Option<Resume>,
    fly_last_seen: Option<u64>,
    fly_dawn: Option<u64>,
    #[serde(skip_serializing_if = "rabbit::State::empty")]
    rabbit: rabbit::State,
}

fn scenic(world: &LivingWorld, slot: usize) -> bool {
    static SCENERY: OnceLock<[usize; 3]> = OnceLock::new();
    let ids = SCENERY.get_or_init(|| {
        [
            "scenic.home_table_court.grass.20_22",
            "scenic.pond_south_reeds.reeds.20_38",
            "scenic.wetland_reed_islands.reeds.74_111",
        ]
        .map(|id| {
            WILLOW_VILLAGE
                .scenic
                .iter()
                .position(|item| item.id == id)
                .expect("wildlife cover")
        })
    });
    scenery_here(world, ids[slot])
}

fn shade_exists(world: &LivingWorld) -> bool {
    ground(world, SHADE) == Ground::Grass && scenic(world, 0)
}

pub(super) fn layered_depth(world: &LivingWorld, v: &WildlifeVisual) -> Option<i32> {
    if v.sprite != "wild_firefly_idle_0" {
        return None;
    }
    [Reed::Pond, Reed::Wetland].into_iter().find_map(|reed| {
        (v.position == reed.leaf() && reed.source(world)).then(|| point(reed.tile()).y as i32 + 1)
    })
}

fn dry_air(world: &LivingWorld) -> bool {
    matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds)
        && matches!(
            world.season(),
            YardSeason::Spring | YardSeason::Summer | YardSeason::Autumn
        )
}

fn next_dawn(now: u64) -> u64 {
    let day = now / DAY_MS;
    if now % DAY_MS < DAWN {
        day * DAY_MS + DAWN
    } else {
        (day + 1) * DAY_MS + DAWN
    }
}

fn same_step(
    world: &LivingWorld,
    occupied: &[TilePos],
    from: Point,
    to: TilePos,
    now: u64,
) -> Option<Edge> {
    let edge = Edge::travel(from, point(to), now, SNAIL_CRAWL);
    let origin = tile(from);
    let samples = ((edge.from.x - edge.to.x)
        .abs()
        .max((edge.from.y - edge.to.y).abs())
        / 2.0)
        .ceil()
        .max(1.0) as u64;
    (exit_itinerary(world, edge)
        && (1..=samples).all(|n| {
            let at = tile(edge.position(now + (edge.end - now) * n / samples));
            (at == origin && origin != to) || foothold(world, occupied, at)
        }))
    .then_some(edge)
}

impl State {
    pub(super) fn empty(&self) -> bool {
        self.snail.is_none()
            && self.snail_resume.is_none()
            && self.snail_day.is_none()
            && self.wet_since.is_none()
            && self.dry_since.is_none()
            && self.fly.is_none()
            && self.fly_resume.is_none()
            && self.fly_last_seen.is_none()
            && self.fly_dawn.is_none()
            && self.rabbit.empty()
    }

    pub(super) fn deadline(&self) -> u64 {
        [self.snail.map(|s| s.edge.end), self.fly.map(|f| f.edge.end)]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(u64::MAX)
            .min(self.rabbit.deadline())
    }

    pub(super) fn phase(&self, id: usize, now: u64) -> Option<u64> {
        if id == rabbit::RABBIT {
            return self.rabbit.phase(now);
        }
        let resume = match id {
            SNAIL => self.snail_resume,
            FIREFLY => self.fly_resume,
            _ => None,
        }?;
        Some(now.saturating_sub(resume.at))
    }

    pub(super) fn valid(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        self.snail_day.is_none_or(|day| day <= now / DAY_MS)
            && self.fly_dawn.is_none_or(|day| day <= now / DAY_MS)
            && [self.wet_since, self.dry_since, self.fly_last_seen]
                .into_iter()
                .flatten()
                .all(|time| time <= now)
            && [self.snail_resume, self.fly_resume]
                .into_iter()
                .flatten()
                .all(|r| r.at <= now && r.waypoint == 0)
            && self.trace_pieces <= 2
            && self.trace_until <= now.saturating_add(TRACE_TIME)
            && (self.trace_until > 0 || self.trace_pieces == 0)
            && self.rabbit.valid(world)
            && self.snail.is_none_or(|s| {
                s.edge.valid(now, 18..=22, 20..=24)
                    && match s.leg {
                        SnailLeg::ToTrail => s.edge.to == point(TRACE[0]),
                        SnailLeg::Trace(i @ 1..=2) => s.edge.to == point(TRACE[usize::from(i)]),
                        SnailLeg::Trace(_) => false,
                        SnailLeg::ToShade | SnailLeg::Drying | SnailLeg::Closed => {
                            s.edge.to == point(SHADE)
                        }
                        SnailLeg::Retreat => s.edge.from == s.edge.to,
                        SnailLeg::Rejoin => s.edge.to == point(super::route_tile(SNAIL, 0)),
                    }
            })
            && self.fly.is_none_or(|f| {
                let bounds = match f.reed {
                    Reed::Pond => (17..=21, 33..=39),
                    Reed::Wetland => (73..=77, 109..=113),
                };
                f.edge.valid(now, bounds.0, bounds.1)
                    && f.establish
                        .is_none_or(|at| at <= now.saturating_add(DAY_MS))
                    && match f.leg {
                        FireflyLeg::RainLanding
                        | FireflyLeg::RainWait
                        | FireflyLeg::DawnLanding
                        | FireflyLeg::DayRest
                        | FireflyLeg::AtReed => f.edge.to == f.reed.leaf(),
                        _ => f.edge.to == f.reed.flight_start(),
                    }
            })
    }

    pub(super) fn advance(
        &mut self,
        world: &LivingWorld,
        occupied: &[TilePos],
        probe: bool,
    ) -> [Option<Proof>; 3] {
        let now = world.world_ms();
        let is_rain = weather_wet(world);
        let mut rain_ended = false;
        if probe {
            if is_rain {
                self.wet_since.get_or_insert(now);
                self.dry_since = None;
                self.trace_pieces = 0;
                self.trace_until = 0;
            } else {
                rain_ended = self.wet_since.take().is_some();
                if dry_air(world) {
                    self.dry_since.get_or_insert(now);
                } else {
                    self.dry_since = None;
                }
            }
        }
        [
            self.snail_step(world, occupied, rain_ended, now),
            self.firefly_step(world, probe, now),
            self.rabbit.advance(world, occupied, probe),
        ]
    }

    fn snail_step(
        &mut self,
        world: &LivingWorld,
        occupied: &[TilePos],
        rain_ended: bool,
        now: u64,
    ) -> Option<Proof> {
        let active = u64::from(ANIMALS[SNAIL].active_from)..u64::from(ANIMALS[SNAIL].active_to);
        let minute = now % DAY_MS / MINUTE;
        let Some(mut snail) = self.snail else {
            if rain_ended
                && active.contains(&minute)
                && dry_air(world)
                && self.snail_day != Some(now / DAY_MS)
                && TRACE
                    .into_iter()
                    .all(|at| ground(world, at) == Ground::Path)
            {
                let phase = self
                    .phase(SNAIL, now)
                    .unwrap_or(now.saturating_add(1_951 * SNAIL as u64));
                let from = super::super::routine_animal(SNAIL, world, phase).position;
                if let Some(edge) = same_step(world, occupied, from, TRACE[0], now) {
                    self.snail = Some(Snail {
                        leg: SnailLeg::ToTrail,
                        edge,
                    });
                    self.snail_day = Some(now / DAY_MS);
                }
            }
            return None;
        };
        if now < snail.edge.end {
            if matches!(
                snail.leg,
                SnailLeg::ToShade | SnailLeg::Drying | SnailLeg::Closed
            ) && (!shade_exists(world)
                || snail.leg == SnailLeg::ToShade && !foothold(world, occupied, SHADE))
            {
                snail.leg = SnailLeg::Retreat;
                snail.edge = Edge::hold(snail.edge.position(now), now, 1);
                self.snail = Some(snail);
            }
            return None;
        }
        let at = snail.edge.to;
        let is_rain = weather_wet(world);
        let mut proof = None;
        if matches!(snail.leg, SnailLeg::ToTrail | SnailLeg::Trace(_))
            && ground(world, tile(at)) == Ground::Path
            && foothold(world, occupied, tile(at))
            && !is_rain
            && world.weather() != YardWeather::Snow
        {
            let step = match snail.leg {
                SnailLeg::ToTrail => 0,
                SnailLeg::Trace(i) => i,
                _ => unreachable!(),
            };
            if step > 0 {
                self.trace_pieces = step;
                self.trace_until = now + TRACE_TIME;
            }
            if let Some(&next) = TRACE.get(usize::from(step) + 1) {
                if let Some(edge) = same_step(world, occupied, at, next, now) {
                    self.snail = Some(Snail {
                        leg: SnailLeg::Trace(step + 1),
                        edge,
                    });
                    return None;
                }
            } else {
                proof = Some(Proof::SnailTrail);
            }
        }
        if matches!(
            snail.leg,
            SnailLeg::ToShade | SnailLeg::Drying | SnailLeg::Closed
        ) && shade_exists(world)
            && (snail.leg != SnailLeg::ToShade || foothold(world, occupied, SHADE))
        {
            if !is_rain && world.weather() != YardWeather::Snow {
                if snail.leg != SnailLeg::Closed
                    && self.dry_since.is_some_and(|dry| now >= dry + DRY_TIME)
                {
                    snail.leg = SnailLeg::Closed;
                    proof = Some(Proof::SnailDryRest);
                } else if snail.leg == SnailLeg::ToShade {
                    snail.leg = SnailLeg::Drying;
                }
                snail.edge = Edge::hold(at, now, 12 * MINUTE);
                self.snail = Some(snail);
                return proof;
            }
        } else if matches!(snail.leg, SnailLeg::ToTrail | SnailLeg::Trace(_))
            && shade_exists(world)
            && !is_rain
            && world.weather() != YardWeather::Snow
            && let Some(edge) = same_step(world, occupied, at, SHADE, now)
        {
            self.snail = Some(Snail {
                leg: SnailLeg::ToShade,
                edge,
            });
            return proof;
        }
        if snail.leg == SnailLeg::Rejoin {
            self.snail_resume = Some(Resume {
                at: now,
                waypoint: 0,
            });
            self.snail = None;
            return proof;
        }
        if let Some(edge) = same_step(world, occupied, at, super::route_tile(SNAIL, 0), now) {
            self.snail = Some(Snail {
                leg: SnailLeg::Rejoin,
                edge,
            });
        } else {
            // Keep the current legal stop while the human has right of way.
            snail.leg = SnailLeg::Retreat;
            snail.edge = Edge::hold(at, now, MINUTE);
            self.snail = Some(snail);
        }
        proof
    }

    fn firefly_step(&mut self, world: &LivingWorld, probe: bool, now: u64) -> Option<Proof> {
        let minute = now % DAY_MS / MINUTE;
        let patrol = animal_visible(FIREFLY, ANIMALS[FIREFLY], world);
        let Some(mut fly) = self.fly else {
            if !probe {
                return None;
            }
            if patrol {
                self.fly_last_seen = Some(now);
            } else if self
                .fly_last_seen
                .is_some_and(|seen| now <= seen + MINUTE * 2)
                && world.season() == YardSeason::Summer
            {
                let reed = Reed::selected(world);
                let rain = weather_wet(world);
                if reed.source(world) && (rain || (70..=72).contains(&minute)) {
                    let phase = self
                        .phase(FIREFLY, now)
                        .unwrap_or(now.saturating_add(1_951 * FIREFLY as u64));
                    let from = super::super::routine_animal(FIREFLY, world, phase).position;
                    self.fly = Some(Firefly {
                        leg: if rain {
                            FireflyLeg::RainLanding
                        } else {
                            FireflyLeg::DawnLanding
                        },
                        edge: Edge::travel(from, reed.leaf(), now, FIREFLY_EDGE),
                        reed,
                        establish: None,
                    });
                }
                self.fly_last_seen = None;
            }
            return None;
        };
        if now < fly.edge.end {
            if probe
                && !matches!(
                    fly.leg,
                    FireflyLeg::RainTakeoff | FireflyLeg::EveningTakeoff | FireflyLeg::Rejoin
                )
                && (!fly.reed.source(world) || world.season() != YardSeason::Summer)
            {
                fly.leg = FireflyLeg::Rejoin;
                fly.edge = Edge::travel(
                    fly.edge.position(now),
                    fly.reed.flight_start(),
                    now,
                    FIREFLY_EDGE,
                );
                self.fly = Some(fly);
            }
            return None;
        }
        let at = fly.edge.to;
        let reed = fly.reed;
        let fly_home = |from: Point| Edge::travel(from, reed.flight_start(), now, FIREFLY_EDGE);
        let hold = |duration| Edge::hold(at, now, duration);
        let mut proof = None;
        if matches!(
            fly.leg,
            FireflyLeg::RainTakeoff | FireflyLeg::EveningTakeoff | FireflyLeg::Rejoin
        ) {
            if fly.leg == FireflyLeg::RainTakeoff && patrol && reed.source(world) {
                proof = Some(Proof::FireflyResume(reed.tile(), tile(fly.edge.to)));
            }
            self.fly = None;
            self.fly_resume = Some(Resume {
                at: now,
                waypoint: 0,
            });
            self.fly_last_seen = patrol.then_some(now);
            return proof;
        }
        if !reed.source(world) || world.season() != YardSeason::Summer {
            fly.leg = FireflyLeg::Rejoin;
            fly.edge = fly_home(at);
        } else {
            match fly.leg {
                FireflyLeg::RainLanding if weather_wet(world) => {
                    fly.leg = FireflyLeg::RainWait;
                    fly.edge = hold(MINUTE);
                }
                FireflyLeg::RainLanding if patrol => {
                    fly.leg = FireflyLeg::Rejoin;
                    fly.edge = fly_home(at);
                }
                FireflyLeg::RainWait if patrol => {
                    fly.leg = FireflyLeg::RainTakeoff;
                    fly.edge = fly_home(at);
                }
                FireflyLeg::RainWait if minute <= 70 || minute >= 1150 => {
                    fly.edge = hold(MINUTE);
                }
                FireflyLeg::RainWait | FireflyLeg::RainLanding | FireflyLeg::DawnLanding => {
                    let dawn = next_dawn(now);
                    fly.leg = FireflyLeg::DayRest;
                    fly.edge = hold(dawn - now);
                    fly.establish = Some(dawn);
                }
                FireflyLeg::DayRest => {
                    if fly.establish.is_some_and(|dawn| now < dawn + 2 * MINUTE)
                        && self.fly_dawn != Some(now / DAY_MS)
                        && matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds)
                    {
                        self.fly_dawn = Some(now / DAY_MS);
                        proof = Some(Proof::FireflyDawn(reed.tile()));
                    }
                    fly.leg = FireflyLeg::AtReed;
                    fly.edge = hold(10 * MINUTE);
                }
                FireflyLeg::AtReed if patrol => {
                    fly.leg = FireflyLeg::EveningTakeoff;
                    fly.edge = fly_home(at);
                }
                FireflyLeg::AtReed => fly.edge = hold(10 * MINUTE),
                _ => unreachable!(),
            }
        }
        self.fly = Some(fly);
        proof
    }

    pub(super) fn visual(&self, id: usize, world: &LivingWorld) -> Option<WildlifeVisual> {
        if id == rabbit::RABBIT {
            return self.rabbit.visual(world);
        }
        let now = world.world_ms();
        let (edge, sprite) = match id {
            SNAIL => {
                let snail = self.snail?;
                let sprite = match snail.leg {
                    SnailLeg::Closed => "wild_snail_retract_0",
                    SnailLeg::Drying => "wild_snail_idle_0",
                    _ if now / 42_000 % 2 == 0 => "wild_snail_crawl_0",
                    _ => "wild_snail_feel_0",
                };
                (snail.edge, sprite)
            }
            FIREFLY => {
                let fly = self.fly?;
                let sprite = match fly.leg {
                    FireflyLeg::RainWait | FireflyLeg::DayRest | FireflyLeg::AtReed => {
                        "wild_firefly_idle_0"
                    }
                    _ if now / 34_000 % 2 == 0 => "wild_firefly_fly_0",
                    _ => "wild_firefly_fly_1",
                };
                (fly.edge, sprite)
            }
            _ => return None,
        };
        Some(WildlifeVisual {
            sprite,
            position: edge.position(now),
            facing_left: edge.to.x < edge.from.x,
        })
    }

    pub(super) fn marks(
        &self,
        world: &LivingWorld,
    ) -> impl Iterator<Item = WildlifeVisual> + use<> {
        let now = world.world_ms();
        [0, 1]
            .map(|i| {
                let [from, to] = [TRACE[i], TRACE[i + 1]];
                (i < usize::from(self.trace_pieces)
                    && self.trace_until > now
                    && land(world, from)
                    && land(world, to))
                .then(|| {
                    let p = point(from);
                    let q = point(to);
                    WildlifeVisual {
                        position: Point {
                            x: (p.x + q.x) / 2.0,
                            y: (p.y + q.y) / 2.0,
                        },
                        sprite: if self.trace_until - now < TRACE_TIME / 2 {
                            "wild_snail_trace_faint"
                        } else {
                            "wild_snail_trace"
                        },
                        facing_left: false,
                    }
                })
            })
            .into_iter()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(world: &mut LivingWorld, amount: u64) {
        let end = world.world_ms() + amount;
        world.queue_world_time(amount);
        while !world.advance_pending_to(end) {}
        assert_eq!(world.world_ms(), end);
    }

    #[test]
    fn snail_leaves_only_completed_trace_on_a_real_open_hard_path_after_reload() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        advance(&mut world, 3 * HOUR);
        while weather_wet(&world) || !TRACE.into_iter().all(|at| foothold(&world, &[], at)) {
            advance(&mut world, MINUTE);
        }
        let now = world.world_ms();
        assert!(
            TRACE
                .into_iter()
                .all(|at| ground(&world, at) == Ground::Path)
        );
        let mut state = State {
            snail: Some(Snail {
                leg: SnailLeg::ToTrail,
                edge: Edge::hold(point(TRACE[0]), now, 1),
            }),
            ..Default::default()
        };
        advance(&mut world, 1);
        let snapshot = serde_json::to_vec(&state).unwrap();
        let mut blocked: State = serde_json::from_slice(&snapshot).unwrap();
        assert_eq!(
            blocked.snail_step(&world, &[TRACE[1]], true, world.world_ms()),
            None
        );
        assert_eq!(blocked.trace_pieces, 0);
        assert_eq!(blocked.marks(&world).count(), 0);
        assert_eq!(state.snail_step(&world, &[], true, world.world_ms()), None);
        assert!(state.valid(&world));
        let edge = state.snail.unwrap().edge;
        let delta = edge.end - world.world_ms();
        advance(&mut world, delta / 2);
        let mid = state.visual(SNAIL, &world).unwrap().position;
        let mut state: State =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(state.visual(SNAIL, &world).unwrap().position, mid);
        assert_eq!(state.marks(&world).count(), 0);
        let delta = edge.end - world.world_ms();
        advance(&mut world, delta);
        assert_eq!(state.snail_step(&world, &[], false, world.world_ms()), None);
        assert_eq!(state.marks(&world).count(), 1);
        let delta = state.snail.unwrap().edge.end - world.world_ms();
        advance(&mut world, delta);
        assert_eq!(
            state.snail_step(&world, &[], false, world.world_ms()),
            Some(Proof::SnailTrail)
        );
        assert_eq!(state.marks(&world).count(), 2);
        assert_eq!(state.snail_step(&world, &[], false, world.world_ms()), None);
        assert_eq!(state.marks(&world).count(), 2);
    }

    #[test]
    fn firefly_can_only_resume_after_waiting_at_an_actual_reed() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        advance(&mut world, 24 * DAY_MS + 15 * HOUR);
        while !animal_visible(FIREFLY, ANIMALS[FIREFLY], &world) {
            advance(&mut world, MINUTE);
            assert!(world.world_ms() < 32 * DAY_MS);
        }
        let reed = Reed::Pond;
        assert!(reed.source(&world));
        let now = world.world_ms();
        let mut state = State {
            fly: Some(Firefly {
                reed,
                leg: FireflyLeg::RainWait,
                edge: Edge::hold(reed.leaf(), now, 1),
                establish: None,
            }),
            ..Default::default()
        };
        assert!(state.valid(&world));
        advance(&mut world, 1);
        assert_eq!(state.firefly_step(&world, true, world.world_ms()), None);
        assert_eq!(state.fly.unwrap().leg, FireflyLeg::RainTakeoff);
        let edge = state.fly.unwrap().edge;
        let delta = edge.end - world.world_ms();
        advance(&mut world, delta / 2);
        let mut reopened: State =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            state.visual(FIREFLY, &world).unwrap().position,
            reopened.visual(FIREFLY, &world).unwrap().position
        );
        let delta = edge.end - world.world_ms();
        advance(&mut world, delta);
        assert_eq!(
            reopened.firefly_step(&world, true, world.world_ms()),
            Some(Proof::FireflyResume(reed.tile(), tile(reed.flight_start())))
        );
        assert!(reopened.fly.is_none());
        let just_rejoined = reopened.phase(FIREFLY, world.world_ms()).unwrap();
        assert_eq!(
            super::super::super::routine_animal(FIREFLY, &world, just_rejoined).position,
            reed.flight_start()
        );
    }

    #[test]
    fn drying_snail_seals_in_the_same_shade_and_leaves_it_when_rain_returns() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        advance(&mut world, 3 * HOUR);
        while !dry_air(&world) || !foothold(&world, &[], SHADE) {
            advance(&mut world, MINUTE);
        }
        let now = world.world_ms();
        let rest = Edge::hold(point(SHADE), now, 1);
        let mut state = State {
            snail: Some(Snail {
                leg: SnailLeg::Drying,
                edge: rest,
            }),
            dry_since: Some(now - DRY_TIME),
            ..Default::default()
        };
        assert!(shade_exists(&world));
        advance(&mut world, 1);
        assert_eq!(
            state.snail_step(&world, &[], false, world.world_ms()),
            Some(Proof::SnailDryRest)
        );
        let mut restored: State =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            restored.visual(SNAIL, &world).unwrap().sprite,
            "wild_snail_retract_0"
        );
        assert_eq!(
            restored.visual(SNAIL, &world).unwrap().position,
            point(SHADE)
        );
        while !weather_wet(&world) {
            advance(&mut world, 3 * HOUR);
        }
        assert_eq!(
            restored.snail_step(&world, &[], false, world.world_ms()),
            None
        );
        assert_ne!(restored.snail.unwrap().leg, SnailLeg::Closed);
        assert_eq!(
            restored.visual(SNAIL, &world).unwrap().position,
            point(SHADE)
        );
    }
}
