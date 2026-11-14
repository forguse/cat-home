//! Windrow compost and plot-scale water records for the village household.
use super::*;

const MATERIAL_CAP: u8 = 8;
const MIN_CURE: u64 = 4 * DAY_MS;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
enum Compost {
    #[default]
    Empty,
    Mixed {
        begun: u64,
        turned: bool,
    },
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Work {
    Inspect(u8),
    InspectDrain(u8),
    ClearDrain(u8),
    TestDrain(u8),
    Mix,
    Turn,
    Apply(u8),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    inspection: Option<u8>,
    observed_wet: bool,
    water_proven: bool,
    drainage: Option<u8>,
    drain_clear: bool,
    drain_tested: bool,
    leaves: u8,
    kitchen_scraps: u8,
    supplemented: u32,
    compost: Compost,
    fertilized: bool,
    fish_success: bool,
    fish_empty: bool,
    cooked_fish: bool,
    fish_pending: bool,
}

impl State {
    pub(super) fn completed(&self, chapter: &str) -> bool {
        match chapter {
            "Q17" => self.water_proven,
            "Q18" => self.water_proven && self.drain_tested,
            "Q19" => self.fertilized,
            "Q21" => self.water_proven && self.fish_success && self.fish_empty && self.cooked_fish,
            _ => false,
        }
    }

    pub(super) fn task(
        &self,
        now: u64,
        plots: &[Plot],
        flooded: u32,
        available: u32,
        q02: bool,
        q03: bool,
    ) -> Option<Work> {
        if q02 && self.inspection.is_none() && !self.water_proven {
            if let Some((id, _)) = plots.iter().enumerate().find(|(id, plot)| {
                *id != usize::from(grain::FIELD)
                    && available & (1 << id) != 0
                    && flooded & (1 << id) == 0
                    && matches!(plot.stage, PlotStage::Seed | PlotStage::Growing)
                    && !plot.watered
            }) {
                return Some(Work::Inspect(id as u8));
            }
        }
        if let Some(plot) = self.drainage
            && !self.drain_tested
            && flooded & (1 << plot) == 0
        {
            return Some(if self.drain_clear {
                Work::TestDrain(plot)
            } else {
                Work::ClearDrain(plot)
            });
        }
        if self.water_proven && self.drainage.is_none() {
            return (6..30)
                .chain(0..6)
                .filter(|&plot| plot != grain::FIELD)
                .find(|&plot| available & (1 << plot) != 0 && flooded & (1 << plot) == 0)
                .map(Work::InspectDrain);
        }
        if q03 && !self.fertilized {
            return match self.compost {
                Compost::Empty if self.leaves >= 2 && self.kitchen_scraps >= 1 => Some(Work::Mix),
                Compost::Mixed {
                    begun,
                    turned: false,
                } if now >= begun + DAY_MS => Some(Work::Turn),
                Compost::Ready => plots.iter().enumerate().find_map(|(id, plot)| {
                    (available & (1 << id) != 0
                        && flooded & (1 << id) == 0
                        && plot.stage == PlotStage::Tilled)
                        .then_some(Work::Apply(id as u8))
                }),
                _ => None,
            };
        }
        None
    }

    pub(super) fn inspected_plot(&self) -> Option<u8> {
        (!self.water_proven).then_some(self.inspection).flatten()
    }

    pub(super) fn water(&mut self, plot: u8, other_watered: bool) {
        if self.inspection == Some(plot) && !self.water_proven {
            self.observed_wet |= other_watered;
            self.water_proven |= self.observed_wet;
            if !self.water_proven {
                self.inspection = None;
            }
        }
    }

    pub(super) fn drained(&mut self, plot: u8) {
        if self.water_proven && !self.drain_tested && self.drainage.is_none() {
            self.drainage = Some(plot);
        }
        if self.drainage == Some(plot) {
            self.drain_clear = true;
        }
    }

    pub(super) fn has_clear_outlet(&self, plot: usize) -> bool {
        self.drain_tested && self.drainage == Some(plot as u8)
    }

    pub(super) fn leaves(&mut self) {
        self.leaves = self.leaves.saturating_add(1).min(MATERIAL_CAP);
    }
    pub(super) fn scraps(&mut self) {
        self.kitchen_scraps = self.kitchen_scraps.saturating_add(1).min(MATERIAL_CAP);
    }

    pub(super) fn fishing(&mut self, caught: bool) {
        self.fish_success |= caught;
        self.fish_empty |= !caught;
        self.fish_pending |= caught;
    }

    pub(super) fn fish_cooked(&mut self) {
        if self.fish_pending {
            self.cooked_fish = true;
            self.fish_pending = false;
        }
    }

    pub(super) fn aims_to_fish(&self) -> bool {
        self.water_proven && (!self.fish_success || !self.fish_empty)
    }

    pub(super) fn aims_to_cook_fish(&self) -> bool {
        self.water_proven && self.fish_pending && !self.cooked_fish
    }

    pub(super) fn tick(&mut self, now: u64) {
        if matches!(self.compost, Compost::Mixed { begun, turned: true } if now >= begun + MIN_CURE)
        {
            self.compost = Compost::Ready;
        }
    }

    pub(super) fn fertilizer(&mut self, plot: usize) -> bool {
        let mask = 1 << plot;
        let had_it = self.supplemented & mask != 0;
        self.supplemented &= !mask;
        had_it
    }

    pub(super) fn commit(
        &mut self,
        work: Work,
        now: u64,
        field: &[Plot],
        flood: u32,
        stock: &mut Stock,
    ) {
        match work {
            Work::Inspect(id) => {
                if !self.water_proven
                    && matches!(
                        field[usize::from(id)].stage,
                        PlotStage::Seed | PlotStage::Growing
                    )
                    && !field[usize::from(id)].watered
                    && flood & (1 << id) == 0
                {
                    self.inspection = Some(id);
                    self.observed_wet = field.iter().any(|plot| plot.watered);
                }
            }
            Work::TestDrain(id)
                if self.drain_clear && self.drainage == Some(id) && flood & (1 << id) == 0 =>
            {
                self.drain_tested |= stock.take(Item::Water, 1);
            }
            Work::InspectDrain(id) if self.water_proven && self.drainage.is_none() => {
                self.drainage = Some(id);
            }
            Work::ClearDrain(id) if self.drainage == Some(id) => {
                self.drain_clear = true;
            }
            Work::Mix
                if self.compost == Compost::Empty
                    && self.leaves >= 2
                    && self.kitchen_scraps >= 1 =>
            {
                self.leaves -= 2;
                self.kitchen_scraps -= 1;
                self.compost = Compost::Mixed {
                    begun: now,
                    turned: false,
                };
            }
            Work::Turn if matches!(self.compost, Compost::Mixed { begun, turned: false } if now >= begun + DAY_MS) =>
            {
                let Compost::Mixed { begun, .. } = self.compost else {
                    return;
                };
                self.compost = Compost::Mixed {
                    begun,
                    turned: true,
                };
            }
            Work::Apply(id)
                if self.compost == Compost::Ready
                    && field[usize::from(id)].stage == PlotStage::Tilled
                    && flood & (1 << id) == 0 =>
            {
                self.compost = Compost::Empty;
                self.supplemented |= 1 << id;
                self.fertilized = true;
            }
            _ => {}
        }
    }

    pub(super) fn validate(&self, now: u64) -> bool {
        self.inspection.is_none_or(|id| id < 30)
            && self.drainage.is_none_or(|id| id < 30)
            && (!self.drain_clear || self.drainage.is_some())
            && (!self.drain_tested || self.drain_clear && self.water_proven)
            && self.leaves <= MATERIAL_CAP
            && self.kitchen_scraps <= MATERIAL_CAP
            && self.supplemented >> 30 == 0
            && !matches!(self.compost, Compost::Mixed { begun, .. } if begun > now)
    }
}
