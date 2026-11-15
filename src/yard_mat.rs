//! One physical yard mat. The furniture is at its bed, on the rail, or in one person's hands.
use super::*;

const RAIL: TilePos = TilePos::new(17, 16);
const RAIL_FOOT: TilePos = TilePos::new(17, 17);
const DRY_TIME: u64 = 30 * MINUTE;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum MatWork { LiftWet, Hang, LiftDry, Return, Flip }

impl MatWork {
    pub(super) fn stand(self) -> TilePos {
        match self {
            Self::Hang | Self::LiftDry => RAIL_FOOT,
            _ => village::poi("cat_rest").tile(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
enum Place {
    #[default]
    Bed,
    CarryToRail(u8),
    Rail(u64),
    CarryToBed(u8),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Mat {
    place: Place,
    wet: bool,
    reverse: bool,
    used: bool,
    crease_until: Option<u64>,
    last_care: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatPlacement { Bed(TilePos), Rail(TilePos), Held(ActorKind) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct YardMatVisual {
    pub placement: MatPlacement,
    pub damp: bool,
    pub reverse: bool,
    pub creased: bool,
}

impl Mat {
    pub(super) fn is_empty(&self) -> bool { self == &Self::default() }

    pub(super) fn valid(&self, now: u64, actors: &[Actor; 3]) -> bool {
        self.last_care.is_none_or(|day| day <= now / DAY_MS)
            && self.crease_until.is_none_or(|until| until <= now.saturating_add(HOUR))
            && match self.place {
                Place::Bed => true,
                Place::Rail(due) => self.wet && due <= now.saturating_add(3 * HOUR + DRY_TIME),
                Place::CarryToRail(i) | Place::CarryToBed(i) => i < 2 && !actors[usize::from(i)].outside
                    && actors[usize::from(i)].tool.is_none() && actors[usize::from(i)].carried.is_none()
                    && self.wet == matches!(self.place, Place::CarryToRail(_)),
            }
    }
}

impl LivingWorld {
    pub fn yard_mat_visual(&self) -> Option<YardMatVisual> {
        let mat = &self.village.as_ref()?.family_routines.mat;
        let placement = match mat.place {
            Place::Bed => MatPlacement::Bed(village::poi("cat_rest").tile()),
            Place::Rail(_) => MatPlacement::Rail(RAIL),
            Place::CarryToRail(i) | Place::CarryToBed(i) => MatPlacement::Held(self.actors[usize::from(i)].kind),
        };
        Some(YardMatVisual { placement, damp: mat.wet, reverse: mat.reverse,
            creased: mat.crease_until.is_some_and(|until| until > self.now_ms) })
    }

    pub(super) fn cat_mat_ready(&self) -> bool {
        self.village.as_ref().is_some_and(|state| {
            let mat = state.family_routines.mat;
            mat.place == Place::Bed && !mat.wet
        })
    }

    pub(super) fn mat_in_hand(&self, i: usize) -> bool { self.mat_carry_task(i).is_some() }

    pub(super) fn mat_carry_task(&self, i: usize) -> Option<Task> {
        match self.village.as_ref()?.family_routines.mat.place {
            Place::CarryToRail(owner) if usize::from(owner) == i => Some(Task::Mat(MatWork::Hang)),
            Place::CarryToBed(owner) if usize::from(owner) == i => Some(Task::Mat(MatWork::Return)),
            _ => None,
        }
    }

    pub(super) fn observe_mat_rain(&mut self) {
        if self.weather() != YardWeather::Rain { return; }
        let Some(state) = &mut self.village else { return; };
        let mat = &mut state.family_routines.mat;
        if let Place::Rail(_) = mat.place { mat.place = Place::Rail((self.now_ms / (3 * HOUR) + 1) * (3 * HOUR) + DRY_TIME); }
        if matches!(mat.place, Place::Bed | Place::Rail(_)) { mat.wet = true; }
    }

    pub(super) fn mat_task(&self, i: usize) -> Option<Task> {
        if i >= 2 || !(7..19).contains(&(self.minute_of_day() / 60))
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            || !self.village.as_ref()?.opening.completed("Q07")|| self.stock.count(Item::Food) < 5
            || self.actors[..2].iter().any(|actor| matches!(actor.task, Some(Task::Mat(_))))
        { return None; }
        let mat = self.village.as_ref()?.family_routines.mat;
        let day = self.now_ms / DAY_MS;
        let cat = &self.actors[ActorKind::Orange.index()];
        let bed = village::poi("cat_rest").tile();
        let bed_free = cat.cell.tile() != bed && cat.route_goal.map(Cell::tile) != Some(bed);
        let work = match mat.place {
            Place::Rail(due) if self.now_ms >= due => MatWork::LiftDry,
            Place::Bed if bed_free && mat.wet && mat.last_care.is_none_or(|previous| previous + 7 <= day) => MatWork::LiftWet,
            Place::Bed if bed_free && !mat.wet && mat.used && mat.last_care != Some(day) => MatWork::Flip,
            _ => return None,
        };
        let actor = &self.actors[i];
        (actor.tool.is_none() && actor.carried.is_none() && self.landscape_carried_fibre(actor.kind) == 0
            && actor.hunger < 58 && actor.thirst < 58 && actor.stamina >= 40
            && actor.cell.distance(Cell::from(work.stand())) <= 12
            && self.quiet_visit_site_available(i, work.stand())).then_some(Task::Mat(work))
    }

    pub(super) fn commit_mat(&mut self, i: usize, work: MatWork) {
        if i >= 2 || self.actors[i].tool.is_some() || self.actors[i].carried.is_some()
            || self.landscape_carried_fibre(self.actors[i].kind) > 0 { return; }
        let rain = matches!(self.weather(), YardWeather::Rain | YardWeather::Snow);
        let day = self.now_ms / DAY_MS;
        let bed = village::poi("cat_rest").tile();
        let cat = &self.actors[ActorKind::Orange.index()];
        let bed_free = cat.cell.tile() != bed && cat.route_goal.map(Cell::tile) != Some(bed);
        let Some(state) = &mut self.village else { return; };
        let mat = &mut state.family_routines.mat;
        match (work, mat.place) {
            (MatWork::LiftWet, Place::Bed) if bed_free && mat.wet && !rain => {
                mat.place = Place::CarryToRail(i as u8);
                mat.crease_until = None;
            }
            (MatWork::Hang, Place::CarryToRail(owner)) if usize::from(owner) == i => {
                mat.place = Place::Rail(self.now_ms + DRY_TIME);
            }
            (MatWork::LiftDry, Place::Rail(due)) if !rain && self.now_ms >= due => {
                mat.wet = false;
                mat.place = Place::CarryToBed(i as u8);
            }
            (MatWork::Return, Place::CarryToBed(owner)) if usize::from(owner) == i => {
                mat.wet = rain;
                mat.used = false;
                mat.place = Place::Bed;
                mat.last_care = Some(day);
                if !rain { self.record_household_completion("H01-39", i, RAIL, LegacyPlace::Front); }
            }
            (MatWork::Flip, Place::Bed) if bed_free && mat.used && !mat.wet && !rain => {
                mat.reverse = !mat.reverse;
                mat.used = false;
                mat.crease_until = None;
                mat.last_care = Some(day);
                self.record_household_completion("H01-40", i, bed, LegacyPlace::Front);
            }
            _ => (),
        }
    }

    pub(super) fn knead_mat(&mut self, i: usize) -> bool {
        if i != ActorKind::Orange.index() || self.actors[i].cell != village::poi("cat_rest")
            || !self.cat_mat_ready() { return false; }
        let mat = &mut self.village.as_mut().unwrap().family_routines.mat;
        mat.used = true;
        mat.crease_until = Some(self.now_ms + HOUR);
        self.record_household_completion("A01-07", i, village::poi("cat_rest").tile(), LegacyPlace::Front);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(world: &mut LivingWorld, actor: usize, tile: TilePos) {
        let cell = Cell::from(tile);
        world.actors[actor].cell = cell;
        world.actors[actor].position = cell.point();
    }

    #[test]
    fn one_mat_survives_double_pickup_rain_and_readback() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let bed = village::poi("cat_rest").tile();
        let worker = ActorKind::Ayu.index();
        let cat = ActorKind::Orange.index();
        let door = world.home_door(0).tile();
        let rain = (32..32 + 56 * 8).map(|spell| spell * 3 * HOUR).find(|now| {
            LivingWorld::weather_at(*now) == YardWeather::Rain
                && !matches!(LivingWorld::weather_at(*now + 3 * HOUR), YardWeather::Rain | YardWeather::Snow)
        }).unwrap();
        world.now_ms = rain;
        world.observe_mat_rain();
        assert!(!world.cat_mat_ready());
        at(&mut world, cat, door);
        world.now_ms += 3 * HOUR;
        at(&mut world, worker, bed);
        world.commit(worker, Task::Mat(MatWork::LiftWet));
        world.commit(0, Task::Mat(MatWork::LiftWet));
        assert_eq!(world.yard_mat_visual().unwrap().placement, MatPlacement::Held(ActorKind::Ayu));
        assert!(!world.activity_available(ActorKind::Ayu));

        let mut world: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert_eq!(world.mat_carry_task(worker), Some(Task::Mat(MatWork::Hang)));
        at(&mut world, worker, RAIL_FOOT);
        world.commit(worker, Task::Mat(MatWork::Hang));
        assert_eq!(world.yard_mat_visual().unwrap().placement, MatPlacement::Rail(RAIL));
        world.commit(worker, Task::Mat(MatWork::LiftDry));
        assert_eq!(world.yard_mat_visual().unwrap().placement, MatPlacement::Rail(RAIL));
        world.now_ms += DRY_TIME;
        world.commit(worker, Task::Mat(MatWork::LiftDry));
        at(&mut world, worker, bed);
        world.commit(worker, Task::Mat(MatWork::Return));
        assert!(world.cat_mat_ready());
        assert_eq!(world.yard_mat_visual().unwrap().placement, MatPlacement::Bed(bed));

        at(&mut world, cat, bed);
        assert!(world.knead_mat(cat));
        world.commit(worker, Task::Mat(MatWork::Flip));
        assert!(world.yard_mat_visual().unwrap().creased);
        at(&mut world, cat, door);
        world.commit(worker, Task::Mat(MatWork::Flip));
        let mat = world.yard_mat_visual().unwrap();
        assert!(mat.reverse);
        assert!(!mat.creased);
        world.commit(worker, Task::Mat(MatWork::Flip));
        assert_eq!(world.yard_mat_visual().unwrap(), mat);
    }
}
