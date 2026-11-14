//! The household's first named grain field and its finite batch.
use super::*;

pub(super) const FIELD: u8 = 8;
const DRY_HOURS: u8 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Work {
    Sow,
    Cut,
    CarrySheaf,
    Thresh,
    Spread,
    Uncover,
    Turn,
    Cover,
    Store,
    Cook,
    SaveSeed,
    SowRoot,
    LiftRoot,
    StoreRoot,
    CookRoot,
    RecoverRootSeed,
    MountMill,
    DemonstrateMill,
    ObserveMill,
    MillWheat,
    MillIndependently,
    CookFlour,
    CookTraineeFlour,
}

impl Work {
    pub(super) fn transports(self) -> bool {
        matches!(
            self,
            Self::CarrySheaf
                | Self::Spread
                | Self::Uncover
                | Self::Cover
                | Self::Store
                | Self::Cook
                | Self::StoreRoot
                | Self::CookRoot
                | Self::MillWheat
                | Self::MillIndependently
                | Self::CookFlour
                | Self::CookTraineeFlour
        )
    }

    pub(super) fn activity(self) -> Activity {
        match self {
            Self::Sow | Self::SowRoot | Self::RecoverRootSeed => Activity::Sow,
            Self::Cut | Self::LiftRoot => Activity::Harvest,
            Self::CarrySheaf | Self::Store | Self::SaveSeed | Self::Cover => Activity::Store,
            Self::Thresh => Activity::SortHarvest,
            Self::Spread | Self::Uncover | Self::Turn => Activity::DryHarvest,
            Self::Cook | Self::CookRoot | Self::CookFlour | Self::CookTraineeFlour => {
                Activity::Cook
            }
            Self::MountMill => Activity::Build,
            Self::DemonstrateMill
            | Self::ObserveMill
            | Self::MillWheat
            | Self::MillIndependently => Activity::SortHarvest,
            Self::StoreRoot => Activity::Store,
        }
    }

    pub(super) fn duration(self) -> u64 {
        match self {
            Self::Sow
            | Self::SowRoot
            | Self::CarrySheaf
            | Self::Cover
            | Self::Store
            | Self::StoreRoot => 5 * MINUTE,
            Self::Cut | Self::Thresh | Self::LiftRoot => 25 * MINUTE,
            Self::Cook | Self::CookRoot | Self::CookFlour | Self::CookTraineeFlour => 30 * MINUTE,
            Self::MountMill | Self::DemonstrateMill | Self::MillWheat | Self::MillIndependently => {
                30 * MINUTE
            }
            Self::ObserveMill | Self::RecoverRootSeed => 5 * MINUTE,
            Self::Spread | Self::Uncover | Self::Turn | Self::SaveSeed => 10 * MINUTE,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
enum Crop {
    #[default]
    Fallow,
    Growing,
    Mature,
    Cut,
    ThreshingFloor,
    Wet,
    Outside,
    Covered,
    Stored,
    RootGrowing,
    RootMature,
    RootsLifted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(super) struct State {
    crop: Crop,
    seed: u8,
    seed_saved: bool,
    sow_day: u64,
    growing_days: u8,
    watered_days: u8,
    dry_ticks: u8,
    turned: bool,
    wet_wheat: u8,
    dry_wheat: u8,
    straw: u8,
    harvests: u8,
    stored: bool,
    replanted: bool,
    root_seed: bool,
    rotated: bool,
    root_cellar: u8,
    root_expires: u64,
    root_cooked: bool,
    pub(super) used_leaf_first: bool,
    pub(super) serving: bool,
    pub(super) ready_until: u64,
    pub(super) eaten: u8,
    mill: bool,
    flour: u8,
    bran: u8,
    flour_cooked: bool,
    taught: bool,
    milled_independently: bool,
    prepared_independently: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            crop: Crop::Fallow,
            seed: 1,
            seed_saved: false,
            sow_day: 0,
            growing_days: 0,
            watered_days: 0,
            dry_ticks: 0,
            turned: false,
            wet_wheat: 0,
            dry_wheat: 0,
            straw: 0,
            harvests: 0,
            stored: false,
            replanted: false,
            root_seed: true,
            rotated: false,
            root_cellar: 0,
            root_expires: 0,
            root_cooked: false,
            used_leaf_first: false,
            serving: false,
            ready_until: 0,
            eaten: 0,
            mill: false,
            flour: 0,
            bran: 0,
            flour_cooked: false,
            taught: false,
            milled_independently: false,
            prepared_independently: false,
        }
    }
}

impl State {
    pub(super) fn completed(&self, chapter: &str) -> bool {
        match chapter {
            "Q09" => self.growing_days >= 8 || self.harvests != 0,
            "Q10" => self.harvests != 0,
            "Q11" => self.stored,
            "Q12" => self.eaten == 0b11,
            "Q13" => self.replanted,
            "Q14" => self.eaten == 0b11 && self.root_cooked && self.used_leaf_first,
            "Q22" => self.rotated,
            "Q25" => self.flour_cooked,
            "Q28" => self.prepared_independently,
            _ => false,
        }
    }

    pub(super) fn crop_stage(&self) -> Option<PlotStage> {
        match self.crop {
            Crop::Growing | Crop::RootGrowing if self.growing_days < 3 => Some(PlotStage::Seed),
            Crop::Growing | Crop::RootGrowing => Some(PlotStage::Growing),
            Crop::Mature | Crop::RootMature => Some(PlotStage::Mature),
            Crop::Cut | Crop::ThreshingFloor | Crop::RootsLifted => Some(PlotStage::Cleared),
            _ => None,
        }
    }

    pub(super) fn can_water(&self, day: u64) -> bool {
        matches!(self.crop, Crop::Growing | Crop::RootGrowing)
            && day > self.sow_day
            && self.watered_days < self.growing_days + 1
    }

    pub(super) fn water(&mut self) {
        self.watered_days += 1;
    }

    pub(super) fn day(&mut self, day: u64, season: YardSeason) {
        if matches!(self.crop, Crop::Growing | Crop::RootGrowing)
            && day > self.sow_day
            && self.watered_days > self.growing_days
            && season != YardSeason::Winter
        {
            self.growing_days += 1;
            if self.crop == Crop::RootGrowing && self.growing_days == 4 {
                self.crop = Crop::RootMature;
                self.rotated = true;
            } else if self.growing_days == 8 {
                self.crop = Crop::Mature;
            }
        }
    }

    pub(super) fn elapsed_drying(&mut self, now: u64, weather: YardWeather) {
        if self.crop != Crop::Outside || now % HOUR >= 10 * MINUTE {
            return;
        }
        let hour = (now % DAY_MS) / HOUR;
        if matches!(weather, YardWeather::Rain | YardWeather::Snow) {
            return;
        }
        if (8..18).contains(&hour) && (self.dry_ticks < 4 || self.turned) {
            self.dry_ticks = self.dry_ticks.saturating_add(1).min(DRY_HOURS);
        }
    }

    pub(super) fn task(
        &self,
        hour: u16,
        day: u64,
        weather: YardWeather,
        field: PlotStage,
        q03: bool,
        q19: bool,
    ) -> Option<Task> {
        let rain = matches!(weather, YardWeather::Rain | YardWeather::Snow);
        Some(match self.crop {
            Crop::Fallow if self.seed > 0 => match field {
                PlotStage::Grass => Task::Clear(FIELD),
                PlotStage::Cleared => Task::Till(FIELD),
                PlotStage::Tilled => Task::Grain(Work::Sow),
                _ => return None,
            },
            Crop::Growing | Crop::RootGrowing if self.can_water(day) => Task::Water(FIELD),
            Crop::Mature if !rain => Task::Grain(Work::Cut),
            Crop::Cut => Task::Grain(Work::CarrySheaf),
            Crop::ThreshingFloor => Task::Grain(Work::Thresh),
            Crop::Wet if !rain && (7..16).contains(&hour) => Task::Grain(Work::Spread),
            Crop::Covered if !rain && (7..16).contains(&hour) => Task::Grain(Work::Uncover),
            Crop::Outside if self.dry_ticks >= DRY_HOURS => Task::Grain(Work::Store),
            Crop::Outside if rain => Task::Grain(Work::Cover),
            Crop::Outside if self.dry_ticks >= 4 && !self.turned => Task::Grain(Work::Turn),
            Crop::Stored
                if self.dry_wheat >= 2 && q03 && self.ready_until == 0 && self.eaten != 0b11 =>
            {
                Task::Grain(Work::Cook)
            }
            Crop::Stored if self.dry_wheat > 0 && !self.seed_saved => Task::Grain(Work::SaveSeed),
            Crop::Stored | Crop::Fallow
                if self.replanted && self.root_seed && self.eaten == 0b11 && q19 && day >= 14 =>
            {
                Task::Grain(Work::SowRoot)
            }
            Crop::RootMature if !rain => Task::Grain(Work::LiftRoot),
            Crop::RootsLifted => Task::Grain(Work::StoreRoot),
            Crop::Fallow if self.rotated && !self.root_seed && !self.completed("Q14") => {
                Task::Grain(Work::RecoverRootSeed)
            }
            _ => return None,
        })
    }

    pub(super) fn commit(
        &mut self,
        work: Work,
        now: u64,
        weather: YardWeather,
        field: &mut Plot,
    ) -> u16 {
        let clear = !matches!(weather, YardWeather::Rain | YardWeather::Snow);
        match (self.crop, work) {
            (Crop::Fallow, Work::Sow) if self.seed > 0 && field.stage == PlotStage::Tilled => {
                self.seed -= 1;
                self.sow_day = now / DAY_MS;
                self.growing_days = 0;
                self.watered_days = 0;
                self.crop = Crop::Growing;
                self.replanted |= self.seed_saved;
            }
            (Crop::Mature, Work::Cut) if clear => {
                self.crop = Crop::Cut;
            }
            (Crop::Cut, Work::CarrySheaf) => {
                self.crop = Crop::ThreshingFloor;
            }
            (Crop::ThreshingFloor, Work::Thresh) if self.wet_wheat == 0 && self.straw < 8 => {
                self.crop = Crop::Wet;
                self.wet_wheat = 5;
                self.straw += 1;
                self.harvests = self.harvests.saturating_add(1);
                field.stage = PlotStage::Tilled;
                self.dry_ticks = 0;
                self.turned = false;
            }
            (Crop::Wet, Work::Spread) | (Crop::Covered, Work::Uncover) if clear => {
                self.crop = Crop::Outside;
            }
            (Crop::Outside, Work::Turn) if clear && self.dry_ticks >= 4 => {
                self.turned = true;
            }
            (Crop::Outside, Work::Cover) => {
                self.crop = Crop::Covered;
            }
            (Crop::Outside, Work::Store) if self.dry_ticks >= DRY_HOURS && self.dry_wheat <= 11 => {
                self.crop = Crop::Stored;
                self.dry_wheat += self.wet_wheat;
                self.wet_wheat = 0;
                self.stored = true;
            }
            (Crop::Stored, Work::Cook)
                if self.dry_wheat >= 2 && self.ready_until == 0 && self.eaten != 0b11 =>
            {
                self.dry_wheat -= 2;
                self.ready_until = now + DAY_MS;
            }
            (Crop::Stored, Work::SaveSeed) if self.dry_wheat > 0 && !self.seed_saved => {
                self.seed_saved = true;
                self.seed += 1;
                self.dry_wheat -= 1;
                self.crop = Crop::Fallow;
            }
            (Crop::Stored | Crop::Fallow, Work::SowRoot)
                if self.replanted && self.root_seed && self.eaten == 0b11 =>
            {
                self.root_seed = false;
                self.sow_day = now / DAY_MS;
                self.growing_days = 0;
                self.watered_days = 0;
                self.crop = Crop::RootGrowing;
            }
            (Crop::RootMature, Work::LiftRoot) if clear => {
                self.crop = Crop::RootsLifted;
            }
            (Crop::RootsLifted, Work::StoreRoot) => {
                self.root_cellar = 3;
                self.root_expires = now + 10 * DAY_MS;
                self.crop = Crop::Fallow;
            }
            (_, Work::CookRoot) if self.root_cellar > 0 && self.root_expires > now => {
                self.root_cellar -= 1;
                if self.root_cellar == 0 {
                    self.root_expires = 0;
                }
                self.root_cooked = true;
                return 3;
            }
            (Crop::Fallow, Work::RecoverRootSeed)
                if self.rotated && !self.root_seed && !self.completed("Q14") =>
            {
                self.root_seed = true;
            }
            _ => {}
        }
        0
    }

    pub(super) fn root_due(&self) -> Option<u64> {
        (self.root_expires > 0).then_some(self.root_expires)
    }

    pub(super) fn mill_work(&self, i: usize, lesson_possible: bool) -> Option<Work> {
        if !self.completed("Q12") || self.prepared_independently {
            return None;
        }
        Some(
            match (
                i,
                self.mill,
                self.flour_cooked,
                self.taught,
                self.milled_independently,
                self.flour,
            ) {
                (1, false, _, _, _, 0) if self.dry_wheat >= 2 => Work::MountMill,
                (1, true, false, _, _, 0) if self.dry_wheat >= 2 => Work::MillWheat,
                (1, true, false, _, _, 1) => Work::CookFlour,
                (1, true, true, false, _, 0) => Work::DemonstrateMill,
                (0, true, true, false, _, 0) if lesson_possible => Work::ObserveMill,
                (0, true, true, true, false, 0) if self.dry_wheat >= 2 => Work::MillIndependently,
                (0, true, true, true, true, 1) => Work::CookTraineeFlour,
                _ => return None,
            },
        )
    }

    pub(super) fn mill_installed(&self) -> bool {
        self.mill
    }

    #[cfg(test)]
    pub(super) fn prepare_mill_render_fixture(&mut self) {
        self.harvests = 1;
        self.straw = 1;
        self.stored = true;
        self.eaten = 0b11;
        self.mill = true;
    }

    pub(super) fn can_share_mill(&self) -> bool {
        self.mill && self.prepared_independently && self.dry_wheat >= 2 && self.flour == 0
    }

    pub(super) fn produce_community_flour(&mut self) -> bool {
        if !self.can_share_mill() {
            return false;
        }
        self.dry_wheat -= 2;
        self.flour = 1;
        self.bran += 1;
        true
    }

    pub(super) fn mill_commit(&mut self, work: Work) -> u16 {
        match work {
            Work::MountMill => self.mill = true,
            Work::ObserveMill => self.taught = true,
            Work::MillWheat | Work::MillIndependently => {
                self.dry_wheat -= 2;
                self.flour = 1;
                self.bran += 1;
                self.milled_independently |= work == Work::MillIndependently;
            }
            Work::CookFlour | Work::CookTraineeFlour => {
                self.flour = 0;
                self.flour_cooked = true;
                self.prepared_independently |= work == Work::CookTraineeFlour;
                return 3;
            }
            _ => {}
        }
        0
    }

    pub(super) fn drying_at(&self, tile: TilePos) -> bool {
        matches!(self.crop, Crop::Outside | Crop::Covered) && tile == drying_site().tile()
    }

    pub(super) fn expire_roots(&mut self, now: u64) -> u8 {
        if self.root_expires != 0 && self.root_expires <= now {
            self.root_expires = 0;
            std::mem::take(&mut self.root_cellar)
        } else {
            0
        }
    }

    pub(super) fn should_cook_root(&self, fresh_produce: u16, now: u64) -> bool {
        self.root_cellar > 0
            && (fresh_produce < 2
                || self.root_expires.saturating_sub(now) <= 4 * DAY_MS
                || self.used_leaf_first && !self.root_cooked)
    }

    pub(super) fn note_leaf_before_root(&mut self, now: u64) {
        if self.needs_leaf_first(now) {
            self.used_leaf_first = true;
        }
    }

    pub(super) fn needs_leaf_first(&self, now: u64) -> bool {
        self.root_cellar > 0 && !self.used_leaf_first
            && self.root_expires.saturating_sub(now) > DAY_MS
    }

    pub(super) fn validate(&self, now: u64) -> bool {
        let actively_wet = matches!(self.crop, Crop::Wet | Crop::Outside | Crop::Covered);
        self.seed <= 1
            && self.sow_day <= now / DAY_MS
            && self.growing_days <= 8
            && self.watered_days <= 8
            && self.dry_ticks <= DRY_HOURS
            && self.watered_days >= self.growing_days
            && self.watered_days <= self.growing_days + 1
            && self.wet_wheat == if actively_wet { 5 } else { 0 }
            && self.dry_wheat <= 16
            && self.straw == self.harvests
            && self.harvests <= 2
            && self.eaten <= 0b11
            && self.ready_until <= now.saturating_add(DAY_MS)
            && (self.ready_until == 0 || self.stored && (self.serving || self.ready_until >= now))
            && (!self.serving || self.ready_until > 0 && self.eaten != 0b11)
            && (!self.seed_saved || self.stored)
            && (!self.replanted || self.seed_saved)
            && (!self.stored || self.harvests > 0)
            && self.root_cellar <= 3
            && (self.root_expires == 0) == (self.root_cellar == 0)
            && (self.root_expires == 0
                || (now..=now.saturating_add(10 * DAY_MS)).contains(&self.root_expires))
            && (!self.rotated || self.replanted && self.eaten == 0b11)
            && self.flour <= 1
            && self.bran <= 3
            && (!self.mill || self.eaten == 0b11)
            && (self.bran == 0 || self.mill)
            && (!self.flour_cooked || self.bran > 0)
            && (!self.taught || self.flour_cooked)
            && (!self.milled_independently || self.taught && self.bran >= 2)
            && (!self.prepared_independently || self.milled_independently)
            && (self.eaten == 0 || self.serving || self.eaten == 0b11 && self.stored)
    }
}

pub(super) fn drying_site() -> Cell {
    let loading = village::poi("pantry_loading");
    Cell(loading.0, loading.1 + 1)
}
