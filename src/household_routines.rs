//! Bounded continuity for the household's physical follow-up work.
use super::{Actor, ActorKind, Cell, DAY_MS, Ground, HOUR, Item, LivingWorld, MINUTE, Observation, Outfit, Pose, Stage, Task, Tool, YardSeason, YardWeather, village};
use crate::map_data::TilePos;
use crate::legacy_content::{LegacyActor, LegacyPlace};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DryWater {
    pub plot: u8,
    pub pours: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PassingWitness {
    pub yielder: u8,
    pub entry: TilePos,
    pub opposite: TilePos,
    pub shoulder: TilePos,
    pub goals: [TilePos; 2],
    pub tasks: [Task; 2],
    pub expires: u64,
    pub stepped_aside: bool,
    pub crossed: bool,
    pub reached: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    pub rotation_due: u32,
    pub split_seed_pouch: [bool; 2],
    pub dry_water: [Option<DryWater>; 2],
    pub raincoat_spell: [Option<u64>; 2],
    pub night_entries: [Option<u64>; 3],
    pub cat_meal_sniffed: bool,
    pub cat_last_meal: Option<u64>,
    pub cat_napped_after: Option<u64>,
    pub cat_first_snow: Option<u64>,
    pub cat_paw_marked: bool,
    pub passing: Option<PassingWitness>,
    pub porch_considered: [Option<u64>; 2],
    pub porch_ready: [Option<u64>; 2],
    #[serde(skip_serializing_if = "super::household_detail::State::is_empty")]
    pub detail: super::household_detail::State,
    #[serde(skip_serializing_if = "super::yard_mat::Mat::is_empty")]
    pub mat: super::yard_mat::Mat,
    #[serde(skip_serializing_if = "super::orchard_ground::Cover::is_empty")]
    pub orchard_cover: super::orchard_ground::Cover,
    #[serde(skip_serializing_if = "super::yard_survey::State::is_empty")]
    pub survey: super::yard_survey::State,
    #[serde(skip_serializing_if = "super::soil_profile::Surveyed::is_empty")]
    pub soil_profile: super::soil_profile::Surveyed,
}

impl State {
    pub fn is_empty(&self) -> bool { self == &Self::default() }

    pub fn valid(&self, now: u64, actors: &[Actor; 3], cat_food: bool) -> bool {
        self.detail.valid(now, cat_food, actors) && self.mat.valid(now, actors)
            && self.orchard_cover.valid(now) && self.survey.valid(now) && self.soil_profile.valid()
            && self.rotation_due & 0x3fff_ffff == 0
            && (!self.cat_meal_sniffed || cat_food)
            && self.cat_last_meal.is_none_or(|time| time <= now)
            && self.cat_napped_after.is_none_or(|time| self.cat_last_meal.is_some_and(|last| time <= last))
            && self.cat_first_snow.is_none_or(|year| year <= now / (112 * DAY_MS))
            && self.night_entries.iter().all(|night| night.is_none_or(|day| day <= now / DAY_MS))
            && (0..2).all(|i| {
                let actor = &actors[i];
                (!self.split_seed_pouch[i] || actor.carried == Some(Item::Crops) && actor.seeds_in_basket)
                    && self.raincoat_spell[i].is_none_or(|spell| spell <= now / (3 * HOUR))
                    && self.porch_considered[i].is_none_or(|day| day <= now / DAY_MS)
                    && self.porch_ready[i].is_none_or(|day| self.porch_considered[i] == Some(day))
                    && self.dry_water[i].is_none_or(|state| {
                        state.plot < 32 && state.pours <= 2 && actor.task == Some(Task::Water(state.plot))
                            && actor.stage == Stage::Work && actor.tool == Some(Tool::WateringCan)
                            && if state.pours == 0 { actor.carried == Some(Item::Water) && actor.amount == 1 }
                               else { actor.carried.is_none() }
                    })
            }) && self.passing.is_none_or(|mark| {
                mark.yielder <= ActorKind::Ayu as u8 && mark.reached <= 0b11
                    && mark.expires <= now.saturating_add(HOUR)
                    && (!mark.crossed || mark.stepped_aside)
                    && [mark.entry, mark.opposite, mark.shoulder].into_iter().chain(mark.goals)
                        .all(|tile| tile.index().is_some())
                    && mark.tasks.iter().all(|task| task.valid_for(true))
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldWaterKind { Soak, Drain }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldWaterWork {
    pub plot: u8,
    pub tile: TilePos,
    pub kind: FieldWaterKind,
    /// Completed pours or drainage thirds, from zero through two during work.
    pub phase: u8,
}

impl LivingWorld {
    pub fn cat_food_served(&self) -> bool {
        self.village.as_ref().is_some_and(|state| state.opening.cat_food)
    }

    pub fn harvest_seed_pouch(&self, kind: ActorKind) -> bool {
        let i = kind.index();
        i < 2 && self.village.as_ref().is_some_and(|v| v.family_routines.split_seed_pouch[i])
    }

    pub fn field_water_work(&self, kind: ActorKind) -> Option<FieldWaterWork> {
        let i = kind.index();
        let actor = &self.actors[i];
        let village = self.village.as_ref()?;
        if i >= 2 || actor.stage != Stage::Work { return None; }
        let (plot, kind, phase) = match actor.task? {
            Task::Water(id) => {
                let work = village.family_routines.dry_water[i].filter(|work| work.plot == id)?;
                (id, FieldWaterKind::Soak, work.pours)
            }
            Task::Drain(id) if village.flooded_plots & (1 << id) != 0 => {
                let duration = actor.until.saturating_sub(actor.since).max(1);
                let thirds = ((3 * self.now_ms.saturating_sub(actor.since)) / duration).min(2) as u8;
                (id, FieldWaterKind::Drain, thirds)
            }
            _ => return None,
        };
        let area = self.available_plot_rect(usize::from(plot))?;
        Some(FieldWaterWork { plot, tile: TilePos::new(area.x, area.y), kind, phase })
    }

    pub fn cat_snow_work(&self) -> Option<(TilePos, u64, u64)> {
        let cat = &self.actors[ActorKind::Orange.index()];
        let Some(Task::CatProbeSnow { snow, .. }) = cat.task else { return None; };
        (cat.stage == Stage::Work).then(|| (snow, self.now_ms.saturating_sub(cat.since), cat.until.saturating_sub(cat.since)))
    }

    pub(super) fn start_dry_watering(&mut self, i: usize, plot: u8) -> bool {
        if i >= 2 || self.season() != YardSeason::Summer || !self.is_village()
            || plot == super::grain::FIELD || !self.village_plot_needs_water(plot)
            || !self.has_carry(i, Item::Water, 1) || self.actors[i].tool != Some(Tool::WateringCan)
        {
            return false;
        }
        let spell = self.now_ms / (3 * HOUR);
        if spell < 2 || Self::wet_spell(spell - 1) || Self::wet_spell(spell - 2)
            || self.weather() != YardWeather::Clear || Self::weather_at(self.now_ms + 6 * MINUTE) != YardWeather::Clear
        {
            return false;
        }
        let field = &mut self.village.as_mut().expect("irrigated village plot").family_routines;
        field.dry_water[i] = Some(DryWater { plot, pours: 0 });
        true
    }

    pub(super) fn advance_dry_watering(&mut self, i: usize, plot: u8) -> bool {
        let Some(work) = self.village.as_ref().and_then(|v| v.family_routines.dry_water.get(i).copied().flatten()) else { return false; };
        if work.plot != plot || work.pours >= 2 { return false; }
        if work.pours == 0 {
            if !self.has_carry(i, Item::Water, 1) { return false; }
            // The bucket was already debited from the pantry. The first pour
            // transfers it into this plot's three-dose work batch. The
            // remaining two thirds stay here across a save, never in stock.
            self.drop_carry(i);
        }
        self.village.as_mut().unwrap().family_routines.dry_water[i] = Some(DryWater { plot, pours: work.pours + 1 });
        self.stage(i, Stage::Work, Pose::Work, 2 * MINUTE);
        true
    }

    pub(super) fn last_dry_dose(&self, i: usize, plot: u8) -> bool {
        i < 2 && self.village.as_ref().is_some_and(|v| {
            v.family_routines.dry_water[i] == Some(DryWater { plot, pours: 2 })
        })
    }

    pub(super) fn finish_watering_receipt(&mut self, i: usize, plot: u8, at: TilePos) {
        if i < 2 && let Some(state) = &mut self.village
            && state.family_routines.dry_water[i].take().is_some_and(|work| work.plot == plot && work.pours == 2)
        {
            self.record_household_completion("H01-33", i, at, LegacyPlace::Front);
        }
    }

    pub(super) fn finish_rain_shift(&mut self, i: usize, at: TilePos) {
        if i < 2 && self.actors[i].outfit == Outfit::Raincoat && self.weather() == YardWeather::Rain
            && let Some(state) = &mut self.village {
            state.family_routines.detail.rain_shift_done[i] = true;
            if state.family_routines.raincoat_spell[i] == Some(self.now_ms / (3 * HOUR)) {
                state.family_routines.raincoat_spell[i] = None;
                self.record_household_completion("H01-52", i, at, LegacyPlace::Front);
            }
        }
    }

    pub(super) fn porch_break_site(&self, i: usize) -> Option<TilePos> {
        let state = self.village.as_ref()?;
        if i >= 2 || !state.opening.completed("Q07") || !(16..19).contains(&(self.minute_of_day() / 60))
            || !matches!(self.season(), YardSeason::Autumn | YardSeason::Winter)
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            || self.stock.count(Item::Food) < 5 || self.stock.count(Item::Water) < 3
        {
            return None;
        }
        let who = &self.actors[i];
        let porch = self.home_side(i);
        (who.stamina >= 60 && who.hunger < 43 && (24..60).contains(&who.thirst)
            && who.tool.is_none() && who.carried.is_none() && state.landscape.fibre_held[i] == 0
            && who.cell.distance(porch) <= 12 && self.quiet_visit_site_available(i, porch.tile()))
            .then_some(porch.tile())
    }

    pub(super) fn maybe_change_for_porch(&mut self, i: usize) -> bool {
        let Some(_) = self.porch_break_site(i) else { return false; };
        let day = self.now_ms / DAY_MS;
        if self.actors[i].outfit == Outfit::Home || self.village.as_ref().unwrap().family_routines.porch_considered[i] == Some(day)
            || !self.begin(i, Task::ChangeClothes(Outfit::Home))
        {
            return false;
        }
        let state = &mut self.village.as_mut().unwrap().family_routines;
        state.porch_considered[i] = Some(day);
        state.porch_ready[i] = None;
        true
    }

    pub(super) fn ready_porch_break(&self, i: usize) -> Option<Task> {
        let at = self.porch_break_site(i)?;
        let ready = self.village.as_ref()?.family_routines.porch_ready[i];
        (ready == Some(self.now_ms / DAY_MS) && self.actors[i].outfit == Outfit::Home).then_some(Task::PorchBreak(at))
    }

    pub(super) fn finish_porch_break(&mut self, i: usize, at: TilePos) {
        if i >= 2 || at != self.home_side(i).tile() || self.actors[i].outfit != Outfit::Home
            || !self.has_carry(i, Item::Water, 1) || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            return;
        }
        self.drop_carry(i);
        self.actors[i].stamina = self.actors[i].stamina.saturating_add(16).min(100);
        self.actors[i].thirst = self.actors[i].thirst.saturating_sub(40);
        self.record_household_completion("H01-53", i, self.home_door(i).tile(), LegacyPlace::Front);
    }

    pub(super) fn see_indoor_sleep(&mut self, i: usize, task: Task) {
        let hour = self.minute_of_day() / 60;
        if (6..21).contains(&hour) || !self.actors[i].inside || !self.is_village() {
            return;
        }
        let (observation, home) = if i < 2 && task == Task::Sleep {
            (Observation::HouseholdNight, self.home_door(i))
        } else if i == ActorKind::Orange.index() && matches!(task, Task::CatSleepInside(_)) {
            (Observation::CatNight, self.home_door(0))
        } else {
            return;
        };
        if self.actors[i].cell != home { return; }
        let night = (self.now_ms / DAY_MS).saturating_sub(u64::from(hour < 6));
        let state = &mut self.village.as_mut().expect("village sleep").family_routines;
        if state.night_entries[i] != Some(night) {
            state.night_entries[i] = Some(night);
            self.record_household_observation(observation, i, home.tile(), LegacyPlace::Front);
        }
    }

    pub(super) fn cat_post_meal_nap(&self) -> Option<Task> {
        let state = &self.village.as_ref()?.family_routines;
        let meal = state.cat_last_meal?;
        let cat = &self.actors[ActorKind::Orange.index()];
        if state.cat_napped_after == Some(meal) || self.now_ms.saturating_sub(meal) > 2 * HOUR
            || !(11..14).contains(&(self.minute_of_day() / 60)) || cat.hunger > 55 || cat.thirst > 55
        {
            return None;
        }
        let covered = matches!(self.weather(), YardWeather::Rain | YardWeather::Snow) || !self.cat_mat_ready();
        let place = if covered { self.home_door(0) } else { village::poi("cat_rest") };
        self.quiet_visit_site_available(ActorKind::Orange.index(), place.tile()).then_some(Task::CatNap(place.tile()))
    }

    pub(super) fn cat_finish_nap(&mut self, i: usize, at: TilePos) {
        if i != ActorKind::Orange.index() || !(11..15).contains(&(self.minute_of_day() / 60)) {
            return;
        }
        let covered = matches!(self.weather(), YardWeather::Rain | YardWeather::Snow);
        if at != self.home_door(0).tile() && (covered || at != village::poi("cat_rest").tile()) { return; }
        let state = &mut self.village.as_mut().expect("cat nap").family_routines;
        if let Some(meal) = state.cat_last_meal
            && state.cat_napped_after != Some(meal) && self.now_ms.saturating_sub(meal) <= 3 * HOUR
        {
            state.cat_napped_after = Some(meal);
            self.actors[i].stamina = self.actors[i].stamina.saturating_add(28).min(100);
            self.record_household_observation(Observation::CatNap, i, at, LegacyPlace::Front);
        }
    }

    pub(super) fn redirect_wet_nap(&mut self) -> bool {
        let i = ActorKind::Orange.index();
        let actor = &self.actors[i];
        let Some(Task::CatNap(before)) = actor.task else { return false; };
        let shelter = self.home_door(0);
        if before == shelter.tile() || !matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            || !matches!(actor.stage, Stage::Prepare | Stage::Work)
        {
            return false;
        }
        // A committed walking edge is never interrupted. This runs at the mat
        // after arrival, or while the cat is already stationary and resting.
        if !self.quiet_visit_site_available(i, shelter.tile()) || !self.begin(i, Task::CatNap(shelter.tile())) {
            self.idle(i, MINUTE);
        }
        true
    }

    pub(super) fn first_snow_probe(&self) -> Option<Task> {
        let state = &self.village.as_ref()?.family_routines;
        let i = ActorKind::Orange.index();
        let actor = &self.actors[i];
        if self.weather() != YardWeather::Snow || state.cat_first_snow == Some(self.now_ms / (112 * DAY_MS))
            || actor.hunger > 66 || actor.thirst > 58 || actor.stamina < 38
        {
            return None;
        }
        let mat = village::poi("cat_rest").tile();
        (-3..=3).flat_map(|y| (-3..=3).filter_map(move |x| mat.offset(x, y)))
            .filter(|&stand| self.ground(stand.x.into(), stand.y.into()) == Ground::Path
                && self.quiet_visit_site_available(i, stand) && actor.cell.distance(Cell::from(stand)) <= 16)
            .filter_map(|stand| {
                let snow = [(1, 0), (0, 1), (-1, 0), (0, -1)].into_iter()
                    .filter_map(|(dx, dy)| stand.offset(dx, dy))
                    .find(|&snow| self.valid_world_cell(Cell::from(snow))
                        && self.soft_ground(Cell::from(snow)).is_some_and(|(snow, _)| snow))?;
                Some((actor.cell.distance(Cell::from(stand)), stand, snow))
            }).min().map(|(_, stand, snow)| Task::CatProbeSnow { stand, snow })
    }

    pub(super) fn cat_finish_snow_probe(&mut self, i: usize, stand: TilePos, snow: TilePos) {
        if i != ActorKind::Orange.index() || self.weather() != YardWeather::Snow
            || self.ground(stand.x.into(), stand.y.into()) != Ground::Path || !self.valid_world_cell(Cell::from(snow))
            || Cell::from(stand).distance(Cell::from(snow)) != 1
        {
            return;
        }
        let Some((true, impression)) = self.soft_ground(Cell::from(snow)) else { return; };
        let state = &mut self.village.as_mut().expect("village snow").family_routines;
        let year = self.now_ms / (112 * DAY_MS);
        if state.cat_first_snow != Some(year) {
            state.cat_first_snow = Some(year);
            self.footprints.add(snow, stand, ActorKind::Orange, true, impression, self.now_ms);
            self.footprints.interrupt(ActorKind::Orange);
            self.record_household_completion("A01-49", i, snow, LegacyPlace::Front);
        }
    }

    pub(super) fn remember_pass(&mut self, i: usize, partner: usize, shoulder: Cell) {
        if i >= 2 || partner >= 2 || self.actors[..2].iter().any(|actor| actor.outside) { return; }
        let Some(state) = &mut self.village else { return; };
        if state.family_routines.passing.is_some_and(|mark| mark.expires > self.now_ms) { return; }
        let [a, b, _] = &self.actors;
        let [Some(goal_a), Some(goal_b)] = [a.route_goal, b.route_goal] else { return; };
        let [Some(task_a), Some(task_b)] = [a.task, b.task] else { return; };
        let actor = &self.actors[i];
        let other = &self.actors[partner];
        state.family_routines.passing = Some(PassingWitness {
            yielder: i as u8, entry: actor.cell.tile(), opposite: other.cell.tile(), shoulder: shoulder.tile(),
            goals: [goal_a.tile(), goal_b.tile()], tasks: [task_a, task_b], expires: self.now_ms + HOUR,
            stepped_aside: false, crossed: false, reached: 0,
        });
    }

    pub(super) fn observe_pass_step(&mut self, i: usize, from: TilePos, to: TilePos) {
        if i >= 2 { return; }
        let Some(state) = &mut self.village else { return; };
        let Some(mark) = &mut state.family_routines.passing else { return; };
        if mark.expires < self.now_ms || (0..2).any(|actor| {
            mark.reached & (1 << actor) == 0 && self.actors[actor].task != Some(mark.tasks[actor])
        }) {
            state.family_routines.passing = None;
            return;
        }
        let yielder = usize::from(mark.yielder);
        mark.stepped_aside |= i == yielder && from == mark.entry && to == mark.shoulder;
        // A legal detour is also a pass: the other person must actually
        // leave the tile that blocked the yielder, then both original goals
        // must be reached. Simply plotting a new route records nothing.
        mark.crossed |= mark.stepped_aside && i != yielder && from == mark.opposite && to != mark.shoulder;
        if mark.crossed && to == mark.goals[i] {
            mark.reached |= 1 << i;
        }
        if mark.reached == 0b11 {
            let completion = *mark;
            state.family_routines.passing = None;
            self.record_legacy_completion("S01", LegacyActor::Household(completion.yielder), completion.goals[yielder],
                completion.entry, LegacyPlace::Nearby, 1, Some(LegacyActor::Household((1 - yielder) as u8)));
        }
    }
}
