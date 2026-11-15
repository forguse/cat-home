//! Fixed subsoils and one measured first-preparation instruction per field.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoilProfile {
    Loam,
    Sand,
    Clay,
}

// The last ordinary expansion lies in the original farm's loam. The two
// reclaimed clearings are distinct pieces of ground beside that expansion.
const PROFILES: [(u8, SoilProfile); 3] = [
    (29, SoilProfile::Loam),
    (30, SoilProfile::Sand),
    (31, SoilProfile::Clay),
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Surveyed {
    observed: u8,
    pending: u8,
    primed: u8,
}

fn profile(plot: u8) -> Option<(u8, SoilProfile)> {
    PROFILES
        .iter()
        .position(|&(id, _)| id == plot)
        .map(|idx| (1 << idx, PROFILES[idx].1))
}

impl Surveyed {
    pub(super) fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub(super) fn valid(&self) -> bool {
        self.observed & !0b111 == 0
            && self.pending & !self.observed == 0
            && self.primed & !self.pending == 0
            && self.primed & !0b010 == 0
    }

    fn observed(&self, plot: u8) -> Option<SoilProfile> {
        let (bit, kind) = profile(plot)?;
        (self.observed & bit != 0).then_some(kind)
    }

    fn awaiting(&self, plot: u8) -> Option<SoilProfile> {
        let (bit, kind) = profile(plot)?;
        (self.pending & bit != 0).then_some(kind)
    }
}

impl LivingWorld {
    pub(crate) fn village_mud_at(&self, tile: TilePos) -> bool {
        self.soft_ground(Cell::from(tile))
            .is_some_and(|(snow, _)| !snow)
    }

    pub(crate) fn village_subsoil(&self, plot: u8) -> Option<(TilePos, SoilProfile)> {
        let (_, kind) = profile(plot)?;
        let rect = self.available_plot_rect(usize::from(plot))?;
        Some((TilePos::new(rect.x, rect.y), kind))
    }

    pub fn village_soil_profile(&self, plot: u8) -> Option<SoilProfile> {
        let state = self.village.as_ref()?;
        self.available_plot_rect(usize::from(plot))?;
        state.family_routines.soil_profile.observed(plot)
    }

    fn soil_profile_work_foot(&self, plot: u8) -> Option<TilePos> {
        let (bit, _) = profile(plot)?;
        let state = self.village.as_ref()?;
        let index = usize::from(plot);
        self.available_plot_rect(index)?;
        if self.plots.get(index)?.stage != PlotStage::Cleared
            || state.family_routines.soil_profile.observed & bit != 0
            || state.flooded_plots & (1 << index) != 0
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            || self
                .actors
                .iter()
                .any(|actor| actor.task.is_some_and(|task| task.uses_plot(plot)))
        {
            return None;
        }
        let foot = Self::plot_work_cell(index);
        (self.valid_world_cell(foot)
            && !self.actors.iter().any(|actor| {
                !actor.inside
                    && (actor.cell == foot || actor.route.get(actor.route_next) == Some(&foot))
            }))
        .then_some(foot.tile())
    }

    pub(crate) fn village_unexamined_soil(&self) -> Option<(u8, TilePos, SoilProfile)> {
        PROFILES.into_iter().find_map(|(plot, kind)| {
            self.soil_profile_work_foot(plot)
                .map(|foot| (plot, foot, kind))
        })
    }

    pub(crate) fn survey_work_still_possible(&self, plot: u8, foot: TilePos) -> bool {
        self.soil_profile_work_foot(plot) == Some(foot)
    }

    pub(crate) fn record_soil_profile(&mut self, plot: u8, foot: TilePos) -> bool {
        if !self.survey_work_still_possible(plot, foot) {
            return false;
        }
        let (bit, _) = profile(plot).unwrap();
        let survey = &mut self.village.as_mut().unwrap().family_routines.soil_profile;
        survey.observed |= bit;
        survey.pending |= bit;
        true
    }

    /// `Some(None)` means the prescribed real ingredient is missing. The
    /// field may wait for water/seed while unrelated work continues.
    pub(super) fn surveyed_soil_first_task(&self, plot: u8) -> Option<Option<Task>> {
        let state = self.village.as_ref()?;
        let kind = state.family_routines.soil_profile.awaiting(plot)?;
        Some(match kind {
            SoilProfile::Sand if self.before_sow_moisture_required(plot) => {
                (self.stock.count(Item::Water) > 0).then_some(Task::Water(plot))
            }
            SoilProfile::Loam if self.surveyed_loam_ready_to_sow(plot) => {
                (self.stock.count(Item::Seeds) > 0).then_some(Task::Sow(plot))
            }
            _ => Some(Task::Till(plot)),
        })
    }

    pub(super) fn queued_soil_preparation(&self, actor: usize) -> Option<Task> {
        if actor != ActorKind::Xiaohe.index()
            || !(7..17).contains(&(self.minute_of_day() / 60))
            || !matches!(self.weather(), YardWeather::Clear | YardWeather::Clouds)
            || self.stock.count(Item::Food) < 5
            || self.stock.count(Item::Water) < 2
            || self.actors[actor].hunger >= 53
            || self.actors[actor].thirst >= 56
            || self.actors[actor].stamina < 46
        {
            return None;
        }
        let village = self.village.as_ref()?;
        PROFILES.into_iter().find_map(|(plot, _)| {
            let index = usize::from(plot);
            if self.soil_survey_plot == Some(plot)
                || self.available_plot_rect(index).is_none()
                || self.plots.get(index)?.stage != PlotStage::Cleared
                || village.flooded_plots & (1 << plot) != 0
                || self
                    .actors
                    .iter()
                    .any(|worker| worker.task.is_some_and(|task| task.uses_plot(plot)))
            {
                return None;
            }
            let task = self.surveyed_soil_first_task(plot).flatten()?;
            task.tool()
                .is_none_or(|tool| self.tool_available(actor, tool))
                .then_some(task)
        })
    }

    pub(super) fn before_sow_moisture_required(&self, plot: u8) -> bool {
        let Some((bit, SoilProfile::Sand)) = profile(plot) else {
            return false;
        };
        self.village.as_ref().is_some_and(|state| {
            let survey = &state.family_routines.soil_profile;
            survey.awaiting(plot) == Some(SoilProfile::Sand) && survey.primed & bit == 0
        })
    }

    pub(super) fn surveyed_loam_ready_to_sow(&self, plot: u8) -> bool {
        self.village.as_ref().is_some_and(|state| {
            state.family_routines.soil_profile.awaiting(plot) == Some(SoilProfile::Loam)
                && state.family_routines.rotation_due & (1 << plot) == 0
        })
    }

    pub(super) fn note_measured_soil_water(&mut self, plot: u8) {
        if self.before_sow_moisture_required(plot)
            && let Some((bit, _)) = profile(plot)
        {
            self.village
                .as_mut()
                .unwrap()
                .family_routines
                .soil_profile
                .primed |= bit;
        }
    }

    pub(super) fn finish_measured_soil_preparation(&mut self, plot: u8) {
        if let Some((bit, _)) = profile(plot)
            && let Some(state) = &mut self.village
        {
            state.family_routines.soil_profile.pending &= !bit;
            state.family_routines.soil_profile.primed &= !bit;
        }
    }
}
