//! Finite habitat appointments for the existing village animals.
use super::{ANIMALS, DAY_MS, HOUR, MINUTE, WildlifeVisual, animal_visible, point};
use crate::{
    living_world::{ActorKind, Ground, LivingWorld, Point, ScenicStage, TILE_PX, YardWeather},
    map_data::{TilePos, WILLOW_VILLAGE},
};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[path = "residents_habitat.rs"]
mod habitat;
pub(super) const SPARROW: usize = 8;
pub(super) const BEE: usize = 17;

pub(super) const FROG: usize = 5;
pub(super) const MOUSE: usize = 7;
pub(super) const SWALLOW: usize = 9;
pub(super) const DUCK: usize = 10;
const MOUSE_TURN: TilePos = TilePos::new(15, 22);
const MOUSE_SHOULDER: TilePos = TilePos::new(15, 23);
const MOUSE_DEN: TilePos = TilePos::new(16, 23);
const MOUSE_EXIT: TilePos = TilePos::new(16, 22);
const MOUSE_RESUME: usize = 2;
const CALL_ROCK: TilePos = TilePos::new(18, 31);
const SPLASH: TilePos = TilePos::new(17, 32);
const OTHER_BANK: TilePos = TilePos::new(12, 29);
const DUCK_WAIT: TilePos = TilePos::new(16, 31);
const DUCK_BANK: TilePos = TilePos::new(17, 31);
const SHIFTED: [usize; 4] = [MOUSE, SWALLOW, FROG, DUCK];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Receipt {
    Mouse {
        heard_at: TilePos,
        alternate: TilePos,
        cat: bool,
    },
    Swallow,
    Pond {
        calling: TilePos,
        bank: TilePos,
    },
    Bee {
        source: u8,
        target: u8,
    },
    Sparrow {
        at: TilePos,
    },
    DuckPrint {
        first: TilePos,
        last: TilePos,
    },
    DuckRejoined,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Edge {
    from: Point,
    to: Point,
    start: u64,
    end: u64,
}

impl Edge {
    fn walk(from: Point, to: TilePos, now: u64) -> Self {
        Self::travel(from, point(to), now, 34_000)
    }

    fn travel(from: Point, to: Point, now: u64, per_tile: u64) -> Self {
        let distance = (from.x - to.x).hypot(from.y - to.y) / TILE_PX as f32;
        Self::new(from, to, now, (distance * per_tile as f32).ceil() as u64)
    }

    fn hold(at: Point, now: u64, duration: u64) -> Self {
        Self::new(at, at, now, duration)
    }

    fn new(from: Point, to: Point, start: u64, duration: u64) -> Self {
        Self {
            from,
            to,
            start,
            end: start.saturating_add(duration.max(1)),
        }
    }

    fn position(self, now: u64) -> Point {
        let t = self.fraction(now);
        Point {
            x: self.from.x + (self.to.x - self.from.x) * t,
            y: self.from.y + (self.to.y - self.from.y) * t,
        }
    }

    fn fraction(self, now: u64) -> f32 {
        (now.saturating_sub(self.start) as f32 / (self.end - self.start).max(1) as f32)
            .clamp(0.0, 1.0)
    }

    fn valid(
        self,
        now: u64,
        x: std::ops::RangeInclusive<u16>,
        y: std::ops::RangeInclusive<u16>,
    ) -> bool {
        self.start <= now
            && self.end > self.start
            && self.end - self.start <= DAY_MS
            && [self.from, self.to].into_iter().all(|p| {
                p.x.is_finite()
                    && p.y.is_finite()
                    && x.contains(&tile(p).x)
                    && y.contains(&tile(p).y)
            })
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Resume {
    at: u64,
    waypoint: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum MousePhase {
    Listen,
    Turn,
    Shoulder,
    Enter,
    Den,
    Exit,
    Alternate,
    Abort,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mouse {
    phase: MousePhase,
    edge: Edge,
    heard_at: TilePos,
    cat: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Roof {
    WeaverHome,
}

impl Roof {
    fn id(self) -> &'static str {
        match self {
            Self::WeaverHome => "residence_weaver",
        }
    }

    fn eave(self) -> Point {
        let door = point(
            WILLOW_VILLAGE
                .building(self.id())
                .expect("wildlife shelter")
                .door,
        );
        // The textile house's roof fascia is 34 texels above its threshold.
        Point {
            x: door.x - 8.0,
            y: door.y - 17.0,
        }
    }

    fn select(world: &LivingWorld) -> Option<Self> {
        // The temporary market canopies fold up when a seller leaves.
        world
            .village_building_active(Self::WeaverHome.id())
            .then_some(Self::WeaverHome)
    }

    fn takeoff(self) -> Point {
        let eave = self.eave();
        Point {
            x: eave.x - 20.0,
            y: eave.y - 2.0,
        }
    }
}

pub(crate) fn layered_depth(world: &LivingWorld, entry: &WildlifeVisual) -> Option<i32> {
    if let Some(depth) = habitat::layered_depth(world, entry) {
        return Some(depth);
    }
    if entry.sprite.starts_with("wild_frog_") && entry.position.x >= point(CALL_ROCK).x - 2.0 {
        return Some(point(CALL_ROCK).y as i32 + 1);
    }
    let roof = Roof::WeaverHome;
    let eave = roof.eave();
    (entry.sprite.starts_with("wild_swallow_")
        && world.village_building_active(roof.id())
        && (entry.position.x - eave.x).abs() <= 12.0
        && (entry.position.y - eave.y).abs() <= 5.0)
        .then(|| point(WILLOW_VILLAGE.building(roof.id()).unwrap().door).y as i32 + 1)
}

pub(crate) fn is_mouse_den(v: &WildlifeVisual) -> bool {
    v.sprite == "wild_field_mouse_idle_0" && v.position == point(MOUSE_DEN)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum BirdPhase {
    Inbound,
    Wait,
    LiftOff,
    Return,
    Abort,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bird {
    phase: BirdPhase,
    roof: Roof,
    edge: Edge,
    landed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum PondPhase {
    Approach,
    Calling,
    Slip,
    Cross,
    Land,
    Rest,
    Return,
    Abort,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pond {
    phase: PondPhase,
    frog: Edge,
    duck: Edge,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    mouse: Option<Mouse>,
    bird: Option<Bird>,
    pond: Option<Pond>,
    resume: [Option<Resume>; 4],
    mouse_day: Option<u64>,
    bird_day: Option<u64>,
    pond_day: Option<u64>,
    rain_seen: Option<bool>,
    next_probe: u64,
    habitat: habitat::Habitat,
}

fn tile(p: Point) -> TilePos {
    TilePos::new(
        (p.x / TILE_PX as f32).floor() as u16,
        (p.y / TILE_PX as f32).floor() as u16,
    )
}

fn route_tile(id: usize, waypoint: usize) -> TilePos {
    let (x, y) = ANIMALS[id].path[waypoint];
    TilePos::new(x, y)
}

fn weather_wet(world: &LivingWorld) -> bool {
    world.weather() == YardWeather::Rain
}

fn land(world: &LivingWorld, at: TilePos) -> bool {
    world.can_walk(i32::from(at.x), i32::from(at.y))
}

fn ground(world: &LivingWorld, at: TilePos) -> Ground {
    world.ground(i32::from(at.x), i32::from(at.y))
}

fn scenery_here(world: &LivingWorld, id: usize) -> bool {
    let item = &WILLOW_VILLAGE.scenic[id];
    world.phase_mask().contains(item.phase)
        && world.map_density_scenic_visible(item.tile)
        && world
            .scenic_resource(id)
            .is_none_or(|stage| stage.stage == ScenicStage::Standing)
}

fn cover(world: &LivingWorld) -> bool {
    static GRASS: OnceLock<usize> = OnceLock::new();
    let id = *GRASS.get_or_init(|| {
        WILLOW_VILLAGE
            .scenic
            .iter()
            .position(|s| s.id == "scenic.home_table_court.grass.16_23")
            .expect("mouse cover")
    });
    ground(world, MOUSE_DEN) == Ground::Grass && scenery_here(world, id)
}

fn rock(world: &LivingWorld) -> bool {
    static ROCK: OnceLock<usize> = OnceLock::new();
    let id = *ROCK.get_or_init(|| {
        WILLOW_VILLAGE
            .scenic
            .iter()
            .position(|s| s.id == "scenic.home_outer_ring.rock.18_31")
            .expect("calling rock")
    });
    scenery_here(world, id)
}

fn land_itinerary(world: &LivingWorld, edge: Edge) -> bool {
    let n = ((edge.to.x - edge.from.x)
        .abs()
        .max((edge.to.y - edge.from.y).abs())
        * 2.0)
        .ceil() as u64;
    (0..=n.max(1)).all(|i| {
        let at = edge.start + (edge.end - edge.start) * i / n.max(1);
        land(world, tile(edge.position(at)))
    })
}

fn exit_itinerary(world: &LivingWorld, edge: Edge) -> bool {
    let departure = tile(edge.from);
    if land(world, departure) {
        return land_itinerary(world, edge);
    }
    // A newly closed source tile can be left, including when an animal was
    // hidden at its edge. It must still exit straight into legal land.
    let distance = ((edge.to.x - edge.from.x)
        .abs()
        .max((edge.to.y - edge.from.y).abs())
        * 2.0)
        .ceil() as u64;
    (1..=distance.max(1)).all(|i| {
        let at = edge.start + (edge.end - edge.start) * i / distance.max(1);
        let cell = tile(edge.position(at));
        cell == departure || land(world, cell)
    })
}

fn foothold(world: &LivingWorld, used: &[TilePos], at: TilePos) -> bool {
    land(world, at)
        && !used.contains(&at)
        && world.actors().iter().all(|actor| {
            actor.inside()
                || (world.actor_tile(actor.kind) != at
                    && world.actor_next_tile(actor.kind) != Some(at))
        })
}

impl State {
    pub(super) fn empty(&self) -> bool {
        self.next_probe == 0 && self.mouse.is_none() && self.bird.is_none() && self.pond.is_none()
    }

    pub(super) fn deadline(&self) -> u64 {
        let mut deadline = self.next_probe;
        if let Some(mouse) = self.mouse {
            deadline = deadline.min(mouse.edge.end);
        }
        if let Some(bird) = self.bird {
            deadline = deadline.min(bird.edge.end);
        }
        if let Some(pond) = self.pond {
            deadline = deadline.min(pond.frog.end.max(pond.duck.end));
        }
        deadline.min(self.habitat.deadline())
    }

    pub(super) fn needs_update(&self, world: &LivingWorld) -> bool {
        world.world_ms() >= self.deadline() || self.mouse_witness(world).is_some()
    }

    pub(super) fn valid(&self, world: &LivingWorld) -> bool {
        let now = world.world_ms();
        self.next_probe <= now.saturating_add(MINUTE)
            && self.habitat.valid(world)
            && !(self.pond.is_some() && self.habitat.duck_active())
            && [self.mouse_day, self.bird_day, self.pond_day]
                .into_iter()
                .flatten()
                .all(|day| day <= now / DAY_MS)
            && self.resume.iter().zip(SHIFTED).all(|(resume, id)| {
                resume
                    .is_none_or(|r| r.at <= now && usize::from(r.waypoint) < ANIMALS[id].path.len())
            })
            && self.mouse.is_none_or(|m| {
                m.heard_at.x >= 14
                    && m.heard_at.x <= 16
                    && (21..=22).contains(&m.heard_at.y)
                    && m.edge.valid(now, 13..=16, 20..=23)
                    && match m.phase {
                        MousePhase::Listen => m.edge.from == m.edge.to,
                        MousePhase::Turn => m.edge.to == point(MOUSE_TURN),
                        MousePhase::Shoulder => m.edge.to == point(MOUSE_SHOULDER),
                        MousePhase::Enter | MousePhase::Den => m.edge.to == point(MOUSE_DEN),
                        MousePhase::Exit => m.edge.to == point(MOUSE_EXIT),
                        MousePhase::Alternate => {
                            m.edge.to == point(route_tile(MOUSE, MOUSE_RESUME))
                        }
                        MousePhase::Abort => ANIMALS[MOUSE]
                            .path
                            .iter()
                            .any(|&(x, y)| m.edge.to == point(TilePos::new(x, y))),
                    }
            })
            && self.bird.is_none_or(|b| {
                b.edge.valid(now, 63..=84, 20..=44)
                    && match b.phase {
                        BirdPhase::Inbound | BirdPhase::Wait => b.edge.to == b.roof.eave(),
                        BirdPhase::LiftOff => b.edge.to == b.roof.takeoff() && b.landed,
                        BirdPhase::Return | BirdPhase::Abort => {
                            b.edge.to == point(route_tile(SWALLOW, 0))
                        }
                    }
            })
            && self.pond.is_none_or(|p| {
                p.frog.valid(now, 12..=18, 29..=32)
                    && p.duck.valid(now, 13..=17, 30..=33)
                    && match p.phase {
                        PondPhase::Approach | PondPhase::Calling => {
                            p.frog.to == point(CALL_ROCK) && p.duck.to == point(DUCK_WAIT)
                        }
                        PondPhase::Slip => {
                            p.frog.to == point(SPLASH) && p.duck.to == point(DUCK_WAIT)
                        }
                        PondPhase::Cross => {
                            p.frog.to == point(OTHER_BANK) && p.duck.to == point(DUCK_WAIT)
                        }
                        PondPhase::Land | PondPhase::Rest => {
                            p.frog.to == point(OTHER_BANK) && p.duck.to == point(DUCK_BANK)
                        }
                        PondPhase::Return | PondPhase::Abort => {
                            p.frog.to == point(route_tile(FROG, 0))
                                && p.duck.to == point(route_tile(DUCK, 0))
                        }
                    }
            })
    }

    fn rejoin(&mut self, id: usize, waypoint: usize, now: u64) {
        let slot = SHIFTED
            .iter()
            .position(|&animal| animal == id)
            .expect("resumable animal");
        self.resume[slot] = Some(Resume {
            at: now,
            waypoint: waypoint as u8,
        });
    }

    pub(super) fn routine_phase(&self, id: usize, now: u64) -> u64 {
        if let Some(phase) = self.habitat.phase(id, now) {
            return phase;
        }
        SHIFTED
            .iter()
            .position(|&animal| animal == id)
            .and_then(|slot| self.resume[slot])
            .map_or_else(
                || now.saturating_add(1_951 * id as u64),
                |r| {
                    now.saturating_sub(r.at)
                        .saturating_add(u64::from(r.waypoint) * ANIMALS[id].period)
                },
            )
    }

    pub(super) fn advance(
        &mut self,
        world: &LivingWorld,
        occupied: &[TilePos],
    ) -> [Option<Receipt>; 6] {
        let now = world.world_ms();
        let probe = now >= self.next_probe;
        if probe {
            self.next_probe = now.saturating_add(MINUTE);
        }
        let [mouse, swallow, pond] = [
            self.advance_mouse(world, occupied),
            self.advance_bird(world),
            self.advance_pond(world, occupied),
        ];
        let samples = [BEE, SPARROW, DUCK]
            .map(|id| super::routine_animal(id, world, self.routine_phase(id, now)).position);
        let [bee, sparrow, duck] =
            self.habitat
                .advance(world, occupied, probe, self.pond.is_some(), samples);
        let duck = if duck == Some(Receipt::DuckRejoined) {
            self.rejoin(DUCK, 0, now);
            None
        } else {
            duck
        };
        self.start_mouse(world, occupied);
        if probe {
            self.start_bird(world);
            self.start_pond(world, occupied);
        }
        [mouse, swallow, pond, bee, sparrow, duck]
    }

    fn mouse_witness(&self, world: &LivingWorld) -> Option<(ActorKind, Point)> {
        let now = world.world_ms();
        if self.mouse.is_some()
            || self.mouse_day == Some(now / DAY_MS)
            || !animal_visible(MOUSE, ANIMALS[MOUSE], world)
        {
            return None;
        }
        let from = super::routine_animal(MOUSE, world, self.routine_phase(MOUSE, now)).position;
        [ActorKind::Orange, ActorKind::Xiaohe, ActorKind::Ayu]
            .into_iter()
            .find(|&kind| {
                let actor = &world.actors()[kind as usize];
                !actor.inside()
                    && actor.walking_progress(now).is_some()
                    && (actor.position.x - from.x).hypot(actor.position.y - from.y)
                        <= 2.0 * TILE_PX as f32
            })
            .map(|mover| (mover, from))
    }

    fn start_mouse(&mut self, world: &LivingWorld, occupied: &[TilePos]) {
        let now = world.world_ms();
        if let Some((mover, from)) = self.mouse_witness(world)
            && cover(world)
            && [MOUSE_TURN, MOUSE_SHOULDER, MOUSE_DEN, MOUSE_EXIT]
                .into_iter()
                .all(|at| foothold(world, occupied, at))
        {
            self.mouse_day = Some(now / DAY_MS);
            self.mouse = Some(Mouse {
                phase: MousePhase::Listen,
                edge: Edge::hold(from, now, 22_000),
                heard_at: tile(from),
                cat: mover == ActorKind::Orange,
            });
        }
    }

    fn advance_mouse(&mut self, world: &LivingWorld, occupied: &[TilePos]) -> Option<Receipt> {
        let now = world.world_ms();
        let mouse = self.mouse?;
        if now < mouse.edge.end {
            return None;
        }
        let current = mouse.edge.to;
        let active = animal_visible(MOUSE, ANIMALS[MOUSE], world);
        let (phase, edge) = match mouse.phase {
            MousePhase::Listen => (MousePhase::Turn, Edge::walk(current, MOUSE_TURN, now)),
            MousePhase::Turn => (
                MousePhase::Shoulder,
                Edge::walk(current, MOUSE_SHOULDER, now),
            ),
            MousePhase::Shoulder => (MousePhase::Enter, Edge::walk(current, MOUSE_DEN, now)),
            MousePhase::Enter => (MousePhase::Den, Edge::hold(current, now, MINUTE)),
            MousePhase::Den if !active && cover(world) => {
                (MousePhase::Den, Edge::hold(current, now, MINUTE))
            }
            MousePhase::Den => (MousePhase::Exit, Edge::walk(current, MOUSE_EXIT, now)),
            MousePhase::Exit => (
                MousePhase::Alternate,
                Edge::walk(current, route_tile(MOUSE, MOUSE_RESUME), now),
            ),
            MousePhase::Alternate if foothold(world, occupied, tile(current)) => {
                self.mouse = None;
                self.rejoin(MOUSE, MOUSE_RESUME, now);
                return Some(Receipt::Mouse {
                    heard_at: mouse.heard_at,
                    alternate: route_tile(MOUSE, MOUSE_RESUME),
                    cat: mouse.cat,
                });
            }
            MousePhase::Abort if foothold(world, occupied, tile(current)) => {
                let waypoint = ANIMALS[MOUSE]
                    .path
                    .iter()
                    .position(|&(x, y)| current == point(TilePos::new(x, y)))
                    .expect("mouse return endpoint");
                self.rejoin(MOUSE, waypoint, now);
                self.mouse = None;
                return None;
            }
            MousePhase::Alternate | MousePhase::Abort => {
                (mouse.phase, Edge::hold(current, now, MINUTE))
            }
        };
        let available = (cover(world) || matches!(phase, MousePhase::Exit | MousePhase::Alternate))
            && (!matches!(phase, MousePhase::Exit) || cover(world))
            && (phase == MousePhase::Den
                || foothold(world, occupied, tile(edge.to)) && land_itinerary(world, edge));
        if available {
            self.mouse = Some(Mouse {
                phase,
                edge,
                ..mouse
            });
        } else {
            let retreat = ANIMALS[MOUSE]
                .path
                .iter()
                .map(|&(x, y)| TilePos::new(x, y))
                .filter(|&at| foothold(world, occupied, at))
                .map(|at| Edge::walk(current, at, now))
                .filter(|&edge| exit_itinerary(world, edge))
                .min_by_key(|edge| edge.end);
            self.mouse = Some(if let Some(edge) = retreat {
                Mouse {
                    phase: MousePhase::Abort,
                    edge,
                    ..mouse
                }
            } else {
                Mouse {
                    edge: Edge::hold(current, now, MINUTE),
                    ..mouse
                }
            });
        }
        None
    }

    fn start_bird(&mut self, world: &LivingWorld) {
        let now = world.world_ms();
        let minute = now % DAY_MS / MINUTE;
        let rain = weather_wet(world);
        let entering_rain = self.rain_seen.replace(rain) == Some(false) && rain;
        let animal = ANIMALS[SWALLOW];
        if !entering_rain
            || self.bird.is_some()
            || self.bird_day == Some(now / DAY_MS)
            || !(u64::from(animal.active_from)..u64::from(animal.active_to)).contains(&minute)
            || world.season() == crate::living_world::YardSeason::Winter
        {
            return;
        }
        if let Some(roof) = Roof::select(world) {
            let from =
                super::routine_animal(SWALLOW, world, self.routine_phase(SWALLOW, now)).position;
            self.bird_day = Some(now / DAY_MS);
            self.bird = Some(Bird {
                phase: BirdPhase::Inbound,
                roof,
                edge: Edge::travel(from, roof.eave(), now, 23_000),
                landed: false,
            });
        }
    }

    fn advance_bird(&mut self, world: &LivingWorld) -> Option<Receipt> {
        let now = world.world_ms();
        let bird = self.bird?;
        if now < bird.edge.end {
            return None;
        }
        if matches!(bird.phase, BirdPhase::Return | BirdPhase::Abort) {
            self.rejoin(SWALLOW, 0, now);
            self.bird = None;
            return (bird.phase == BirdPhase::Return && bird.landed).then_some(Receipt::Swallow);
        }
        let at = bird.edge.to;
        let open = world.village_building_active(bird.roof.id());
        self.bird = Some(if bird.phase == BirdPhase::LiftOff {
            Bird {
                phase: if open {
                    BirdPhase::Return
                } else {
                    BirdPhase::Abort
                },
                edge: Edge::travel(at, point(route_tile(SWALLOW, 0)), now, 23_000),
                ..bird
            }
        } else if open && weather_wet(world) {
            Bird {
                phase: BirdPhase::Wait,
                edge: Edge::hold(at, now, MINUTE),
                landed: true,
                ..bird
            }
        } else if open && bird.landed {
            Bird {
                phase: BirdPhase::LiftOff,
                edge: Edge::travel(at, bird.roof.takeoff(), now, 23_000),
                ..bird
            }
        } else {
            Bird {
                phase: BirdPhase::Abort,
                edge: Edge::travel(at, point(route_tile(SWALLOW, 0)), now, 23_000),
                ..bird
            }
        });
        None
    }

    fn bank_safe(world: &LivingWorld, occupied: &[TilePos]) -> bool {
        rock(world)
            && foothold(world, occupied, DUCK_BANK)
            && foothold(world, occupied, OTHER_BANK)
            && ground(world, DUCK_WAIT) == Ground::Water
            && ground(world, SPLASH) == Ground::Water
    }

    fn start_pond(&mut self, world: &LivingWorld, occupied: &[TilePos]) {
        let now = world.world_ms();
        if self.pond.is_some()
            || self.habitat.duck_active()
            || self.pond_day == Some(now / DAY_MS)
            || !animal_visible(FROG, ANIMALS[FROG], world)
            || !animal_visible(DUCK, ANIMALS[DUCK], world)
            || !Self::bank_safe(world, occupied)
        {
            return;
        }
        let frog = super::routine_animal(FROG, world, self.routine_phase(FROG, now)).position;
        let duck = super::routine_animal(DUCK, world, self.routine_phase(DUCK, now)).position;
        self.pond_day = Some(now / DAY_MS);
        self.pond = Some(Pond {
            phase: PondPhase::Approach,
            frog: Edge::walk(frog, CALL_ROCK, now),
            duck: Edge::walk(duck, DUCK_WAIT, now),
        });
    }

    fn pond_return(pond: Pond, now: u64, abort: bool) -> Pond {
        Pond {
            phase: if abort {
                PondPhase::Abort
            } else {
                PondPhase::Return
            },
            frog: Edge::walk(pond.frog.position(now), route_tile(FROG, 0), now),
            duck: Edge::walk(pond.duck.position(now), route_tile(DUCK, 0), now),
        }
    }

    fn advance_pond(&mut self, world: &LivingWorld, occupied: &[TilePos]) -> Option<Receipt> {
        let now = world.world_ms();
        let pond = self.pond?;
        if now < pond.frog.end.max(pond.duck.end) {
            return None;
        }
        if matches!(pond.phase, PondPhase::Return | PondPhase::Abort) {
            self.pond = None;
            self.rejoin(FROG, 0, now);
            self.rejoin(DUCK, 0, now);
            return None;
        }
        if !Self::bank_safe(world, occupied) || !weather_wet(world) {
            self.pond = Some(Self::pond_return(pond, now, true));
            return None;
        }
        let frog_at = pond.frog.to;
        let duck_at = pond.duck.to;
        let (phase, frog, duck) = match pond.phase {
            PondPhase::Approach => (
                PondPhase::Calling,
                Edge::hold(frog_at, now, 22_000),
                Edge::hold(duck_at, now, 22_000),
            ),
            PondPhase::Calling => (
                PondPhase::Slip,
                Edge::walk(frog_at, SPLASH, now),
                Edge::hold(duck_at, now, 1),
            ),
            PondPhase::Slip => (
                PondPhase::Cross,
                Edge::walk(frog_at, OTHER_BANK, now),
                Edge::hold(duck_at, now, 1),
            ),
            PondPhase::Cross => (
                PondPhase::Land,
                Edge::hold(frog_at, now, 1),
                Edge::walk(duck_at, DUCK_BANK, now),
            ),
            PondPhase::Land => (
                PondPhase::Rest,
                Edge::hold(frog_at, now, 35_000),
                Edge::hold(duck_at, now, 35_000),
            ),
            PondPhase::Rest => {
                self.pond = Some(Self::pond_return(pond, now, false));
                return None;
            }
            PondPhase::Return | PondPhase::Abort => unreachable!("handled above"),
        };
        self.pond = Some(Pond { phase, frog, duck });
        (phase == PondPhase::Rest
            && ground(world, DUCK_BANK) != Ground::Water
            && frog_at == point(OTHER_BANK))
        .then_some(Receipt::Pond {
            calling: CALL_ROCK,
            bank: OTHER_BANK,
        })
    }

    pub(super) fn visual(&self, id: usize, world: &LivingWorld) -> Option<WildlifeVisual> {
        if let Some(visual) = self.habitat.visual(id, world) {
            return Some(visual);
        }
        let now = world.world_ms();
        let frame = (now / 24_000 % 2) as usize;
        let (edge, sprite) = match id {
            MOUSE => {
                let m = self.mouse?;
                (
                    m.edge,
                    if matches!(m.phase, MousePhase::Listen | MousePhase::Den) {
                        "wild_field_mouse_idle_0"
                    } else {
                        ["wild_field_mouse_run_0", "wild_field_mouse_run_1"][frame]
                    },
                )
            }
            SWALLOW => {
                let b = self.bird?;
                let sprite = match b.phase {
                    BirdPhase::Wait => "wild_swallow_perch_0",
                    BirdPhase::Inbound if now + 20_000 >= b.edge.end => "wild_swallow_land_0",
                    _ => ANIMALS[SWALLOW].frames[(now / 19_000 % 3) as usize],
                };
                (b.edge, sprite)
            }
            FROG => {
                let p = self.pond?;
                let sprite = match p.phase {
                    PondPhase::Calling | PondPhase::Land | PondPhase::Rest => "wild_frog_call_0",
                    PondPhase::Slip if now + 15_000 >= p.frog.end => "wild_frog_land_0",
                    _ if now >= p.frog.end => "wild_frog_idle_0",
                    _ => ["wild_frog_hop_0", "wild_frog_hop_1"][frame],
                };
                (p.frog, sprite)
            }
            DUCK => {
                let p = self.pond?;
                let sprite = if ground(world, tile(p.duck.position(now))) == Ground::Water {
                    "wild_duckling_swim_0"
                } else if now >= p.duck.end || p.duck.from == p.duck.to {
                    "wild_duckling_idle_0"
                } else {
                    "wild_duckling_walk_0"
                };
                (p.duck, sprite)
            }
            _ => return None,
        };
        let mut position = edge.position(now);
        if id == FROG {
            let from_rock = f32::from(u8::from(edge.from == point(CALL_ROCK)));
            let to_rock = f32::from(u8::from(edge.to == point(CALL_ROCK)));
            position.y -= 7.0 * (from_rock + (to_rock - from_rock) * edge.fraction(now));
        }
        Some(WildlifeVisual {
            sprite,
            position,
            facing_left: edge.to.x < edge.from.x,
        })
    }

    pub(super) fn marks(&self, world: &LivingWorld) -> impl Iterator<Item = WildlifeVisual> + '_ {
        self.habitat.marks(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{map_data::WILLOW_VILLAGE, residents::VillageLife};
    use std::path::Path;

    fn capture(directory: Option<&Path>, name: &str, world: &LivingWorld, life: &VillageLife) {
        let Some(directory) = directory else {
            return;
        };
        let name = directory.join(name);
        if name.exists() {
            return;
        }
        std::fs::create_dir_all(&name).unwrap();
        let frozen = serde_json::json!({ "map_revision": WILLOW_VILLAGE.map_revision, "world": world, "life": life });
        std::fs::write(
            name.join("world-v4.json"),
            serde_json::to_vec(&frozen).unwrap(),
        )
        .unwrap();
    }

    fn active_frames(world: &LivingWorld, life: &VillageLife) -> [Option<WildlifeVisual>; 4] {
        SHIFTED.map(|id| {
            life.wildlife.visual(id, world).or_else(|| {
                animal_visible(id, ANIMALS[id], world).then(|| {
                    super::super::routine_animal(
                        id,
                        world,
                        life.wildlife.routine_phase(id, world.world_ms()),
                    )
                })
            })
        })
    }

    #[test]
    fn occupied_approach_cancels_mouse_before_it_can_claim_the_den() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        let from = point(route_tile(MOUSE, 0));
        let mut state = State {
            mouse: Some(Mouse {
                phase: MousePhase::Listen,
                edge: Edge::hold(from, now, 1),
                heard_at: tile(from),
                cat: true,
            }),
            ..State::default()
        };
        assert!(state.valid(&world));
        world.advance_world_ms(1);
        assert_eq!(state.advance_mouse(&world, &[MOUSE_TURN]), None);
        assert_eq!(state.mouse.unwrap().phase, MousePhase::Abort);
        world.advance_world_ms(state.mouse.unwrap().edge.end - world.world_ms());
        assert_eq!(state.advance_mouse(&world, &[MOUSE_TURN]), None);
        assert!(state.mouse.is_none());
        assert!(state.valid(&world));
    }

    #[test]
    fn until_the_frog_reaches_the_far_bank_the_duck_is_still_in_water() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        while !(animal_visible(FROG, ANIMALS[FROG], &world)
            && animal_visible(DUCK, ANIMALS[DUCK], &world))
        {
            world.advance_world_ms(10 * MINUTE);
            assert!(world.world_ms() < 40 * DAY_MS);
        }
        let now = world.world_ms();
        assert!(land(&world, DUCK_BANK) && land(&world, OTHER_BANK));
        assert_eq!(ground(&world, DUCK_WAIT), Ground::Water);
        let mut state = State {
            pond: Some(Pond {
                phase: PondPhase::Cross,
                frog: Edge::walk(point(SPLASH), OTHER_BANK, now),
                duck: Edge::hold(point(DUCK_WAIT), now, 1),
            }),
            ..Default::default()
        };
        let snapshot = serde_json::to_vec(&state).unwrap();
        let mut restored: State = serde_json::from_slice(&snapshot).unwrap();
        let edge = state.pond.unwrap().frog;
        world.advance_world_ms(edge.end - now - 1);
        assert_eq!(state.advance_pond(&world, &[]), None);
        assert_eq!(
            tile(state.visual(DUCK, &world).unwrap().position),
            DUCK_WAIT
        );
        world.advance_world_ms(1);
        assert_eq!(state.advance_pond(&world, &[]), None);
        assert_eq!(restored.advance_pond(&world, &[]), None);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        assert_eq!(
            tile(state.visual(FROG, &world).unwrap().position),
            OTHER_BANK
        );
        assert_eq!(
            tile(state.visual(DUCK, &world).unwrap().position),
            DUCK_WAIT
        );
        let end = state.pond.unwrap().duck.end;
        world.advance_world_ms(end - world.world_ms());
        assert_eq!(
            state.advance_pond(&world, &[]),
            Some(Receipt::Pond {
                calling: CALL_ROCK,
                bank: OTHER_BANK
            })
        );
        assert_eq!(state.advance_pond(&world, &[]), None);
        assert_eq!(
            tile(state.visual(DUCK, &world).unwrap().position),
            DUCK_BANK
        );

        assert_eq!(restored.advance_pond(&world, &[DUCK_BANK]), None);
        assert_eq!(restored.pond.unwrap().phase, PondPhase::Abort);

        let rock_id = WILLOW_VILLAGE
            .scenic
            .iter()
            .position(|s| s.tile == CALL_ROCK)
            .unwrap();
        assert!(world.test_scenic_visual_fixture(rock_id, ScenicStage::Cleared, false));
        let mut missing_rock: State = serde_json::from_slice(&snapshot).unwrap();
        assert_eq!(missing_rock.advance_pond(&world, &[]), None);
        assert_eq!(missing_rock.pond.unwrap().phase, PondPhase::Abort);
    }

    #[test]
    #[ignore = "standalone natural ecology replay and screenshot capture"]
    fn natural_habitat_replay_measures_continuity_at_every_state_boundary() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let directory = std::env::var_os("CAT_HOME_WILDLIFE_PROOF").map(std::path::PathBuf::from);
        let end = world.world_ms() + 24 * DAY_MS;
        world.queue_world_time(end - world.world_ms());
        let mut captures = [false; 9];
        let mut receipts = [false; 2];
        let mut max_boundary = 0.0_f32;
        let mut transitions = 0;
        for _ in 0..900_000 {
            let now = world.world_ms();
            world.set_external_occupied(&life.occupied_tiles());
            if !world.advance_pending_to(now) || world.navigation_pending() {
                continue;
            }
            let before = active_frames(&world, &life);
            life.advance(&mut world);
            if life.navigation_pending() {
                continue;
            }
            world.record_village_social(life.social_progress());
            for (from, to) in before.into_iter().zip(active_frames(&world, &life)) {
                if let (Some(from), Some(to)) = (from, to) {
                    let delta =
                        (from.position.x - to.position.x).hypot(from.position.y - to.position.y);
                    max_boundary = max_boundary.max(delta);
                    transitions += usize::from(from.sprite != to.sprite);
                }
            }
            let mouse = life.wildlife.mouse.map(|m| m.phase);
            let bird = life.wildlife.bird.map(|b| b.phase);
            let pond = life.wildlife.pond.map(|p| p.phase);
            for receipt in world.legacy_behaviors().recent() {
                use crate::legacy_content::LegacyActor;
                if receipt.actor == LegacyActor::Wildlife(MOUSE as u8)
                    && receipt.target == route_tile(MOUSE, MOUSE_RESUME)
                {
                    receipts[0] = true;
                }
                if receipt.actor == LegacyActor::Wildlife(FROG as u8)
                    && receipt.at == CALL_ROCK
                    && receipt.target == OTHER_BANK
                {
                    receipts[1] = true;
                }
            }
            for (index, name, ready) in [
                (0, "mouse-listen", mouse == Some(MousePhase::Listen)),
                (1, "mouse-den", mouse == Some(MousePhase::Den)),
                (2, "mouse-alternate", mouse == Some(MousePhase::Alternate)),
                (3, "swallow-to-roof", bird == Some(BirdPhase::Inbound)),
                (4, "swallow-waits", bird == Some(BirdPhase::Wait)),
                (
                    5,
                    "swallow-returns",
                    life.wildlife.bird.is_some_and(|b| {
                        b.phase == BirdPhase::LiftOff && b.edge.fraction(now) >= 0.2
                    }),
                ),
                (6, "frog-calls-duck-waits", pond == Some(PondPhase::Calling)),
                (7, "frog-crosses-duck-waits", pond == Some(PondPhase::Cross)),
                (8, "duck-on-land", pond == Some(PondPhase::Rest)),
            ] {
                if ready && !captures[index] {
                    assert!(life.wildlife.valid(&world));
                    captures[index] = true;
                    capture(directory.as_deref(), name, &world, &life);
                }
            }
            if now >= end || captures.iter().all(|&seen| seen) && receipts.iter().all(|&seen| seen)
            {
                break;
            }
            world.set_external_occupied(&life.occupied_tiles());
            let picture_deadline = if bird == Some(BirdPhase::LiftOff) {
                now + 8_000
            } else {
                end
            };
            let next = world
                .next_update_ms()
                .min(life.next_update_ms(&world).unwrap_or(end))
                .min(end)
                .min(picture_deadline)
                .max(now + 1);
            let _ = world.advance_pending_to(next);
        }
        println!(
            "wildlife natural stages {:?}; {transitions} frame transitions; largest same-time boundary displacement {max_boundary:.3} world px",
            captures
        );
        assert!(captures.into_iter().all(|seen| seen));
        assert_eq!(receipts, [true, true]);
        assert!(max_boundary < 0.01);
    }
}
