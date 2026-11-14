//! The fixed development sequence and bounded pantry ledger.

use serde::{Deserialize, Serialize};

#[path = "building_improvements.rs"]
pub mod improvements;
pub use improvements::{
    BuildingUse, Fixture, FixtureStatus, IMPROVEMENT_SITES, ImprovementOrder, ImprovementState,
};

#[path = "housing.rs"]
pub mod housing;
pub use housing::{FormerHomeUse, HousePlan, HousingPhase, HousingState};

pub const CHAPTER_COUNT: usize = 24;
pub const SITE_COUNT: usize = 8;
const DAY: u64 = 86_400_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevelopmentKind {
    Road,
    Bridge,
    Market,
    Stall,
    Guesthouse,
    Depot,
    Well,
    Garden,
}

#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub kind: DevelopmentKind,
    pub x: u8,
    pub y: u8,
}

pub const SITES: [Site; SITE_COUNT] = [
    Site {
        kind: DevelopmentKind::Road,
        x: 7,
        y: 24,
    },
    Site {
        kind: DevelopmentKind::Bridge,
        x: 12,
        y: 28,
    },
    Site {
        kind: DevelopmentKind::Market,
        x: 19,
        y: 34,
    },
    Site {
        kind: DevelopmentKind::Stall,
        x: 22,
        y: 34,
    },
    Site {
        kind: DevelopmentKind::Guesthouse,
        x: 27,
        y: 35,
    },
    Site {
        kind: DevelopmentKind::Depot,
        x: 32,
        y: 35,
    },
    Site {
        kind: DevelopmentKind::Well,
        x: 35,
        y: 35,
    },
    Site {
        kind: DevelopmentKind::Garden,
        x: 40,
        y: 35,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Material {
    Wood,
    Stone,
    Rope,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    Measure,
    Prepare,
    OpenRoad,
    Abutment,
    Deck,
    OpenBridge,
    Square,
    Stands,
    OpenMarket,
    Foundation,
    Frame,
    OpenStalls,
    OpenGuesthouse,
    OpenDepot,
    OpenWell,
    OpenGarden,
}

#[derive(Clone, Copy, Debug)]
pub struct Chapter {
    pub title: &'static str,
    pub site: u8,
    pub phase: u8,
    pub earliest_day: u16,
    pub material: Material,
    pub amount: u16,
    pub work_shifts: u8,
    pub cure_days: u8,
    pub effect: Effect,
}

use Effect::*;
use Material::*;

const fn chapter(
    title: &'static str,
    site: u8,
    phase: u8,
    earliest_day: u16,
    material: Material,
    amount: u16,
    work_shifts: u8,
    cure_days: u8,
    effect: Effect,
) -> Chapter {
    Chapter {
        title,
        site,
        phase,
        earliest_day,
        material,
        amount,
        work_shifts,
        cure_days,
        effect,
    }
}

pub const CHAPTERS: [Chapter; CHAPTER_COUNT] = [
    chapter("丈量村口路", 0, 1, 2, Stone, 1, 2, 1, Measure),
    chapter("铺村口路基", 0, 2, 4, Wood, 1, 2, 1, Prepare),
    chapter("开放村口交接点", 0, 3, 6, Stone, 1, 2, 1, OpenRoad),
    chapter("砌近岸桥台", 1, 1, 10, Stone, 2, 3, 2, Abutment),
    chapter("架设桥板", 1, 2, 14, Wood, 3, 3, 3, Deck),
    chapter("安装护栏并验桥", 1, 3, 18, Rope, 1, 5, 20, OpenBridge),
    chapter("清整市集广场", 2, 1, 44, Stone, 1, 2, 2, Square),
    chapter("安装公共货架", 2, 2, 49, Wood, 2, 3, 2, Stands),
    chapter("开放南岸换货", 2, 3, 55, Rope, 1, 2, 2, OpenMarket),
    chapter("放置摊棚基座", 3, 1, 63, Stone, 1, 2, 2, Foundation),
    chapter("架起摊棚木架", 3, 2, 69, Wood, 2, 3, 2, Frame),
    chapter("完成遮雨摊棚", 3, 3, 75, Rope, 1, 2, 2, OpenStalls),
    chapter("砌筑客舍地基", 4, 1, 85, Stone, 2, 3, 3, Foundation),
    chapter("安装客舍梁柱", 4, 2, 94, Wood, 3, 3, 3, Frame),
    chapter("开放客舍歇脚处", 4, 3, 104, Wood, 2, 3, 3, OpenGuesthouse),
    chapter("修建仓储站地台", 5, 1, 115, Stone, 2, 3, 3, Foundation),
    chapter("组装通风储架", 5, 2, 125, Wood, 2, 3, 3, Frame),
    chapter("开放保鲜仓储站", 5, 3, 137, Rope, 1, 3, 3, OpenDepot),
    chapter("勘定公共井水线", 6, 1, 150, Stone, 2, 3, 3, Foundation),
    chapter("砌起公共井台", 6, 2, 162, Stone, 2, 3, 3, Frame),
    chapter("启用公共井", 6, 3, 175, Rope, 1, 3, 3, OpenWell),
    chapter("整平公共花园", 7, 1, 189, Stone, 1, 2, 3, Foundation),
    chapter("架设花园矮栏", 7, 2, 200, Wood, 2, 3, 3, Frame),
    chapter("开放花园歇脚路", 7, 3, 212, Wood, 1, 3, 3, OpenGarden),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveProject {
    pub id: u8,
    pub phase: u8,
    pub progress: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Project {
    pub id: u8,
    pub begun: u64,
    pub shifts: u8,
    pub next_shift: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct StoryState {
    pub(crate) finished: u8,
    pub(crate) running: Option<Project>,
    pub(crate) trades: u64,
    pub(crate) spoiled: u64,
    pub(crate) next_trade: u64,
    pub(crate) milestones: u8,
    pub(crate) last_assessed_day: Option<u64>,
    pub(crate) tended_garden: u64,
    pub(crate) next_garden: u64,
    pub(crate) improvements: ImprovementState,
    pub(crate) housing: HousingState,
}

impl StoryState {
    pub fn completed(&self) -> usize {
        usize::from(self.finished)
    }
    pub fn milestones(&self) -> u8 {
        self.milestones
    }
    pub fn market_trades(&self) -> u64 {
        self.trades
    }
    pub fn waste(&self) -> u64 {
        self.spoiled
    }
    pub fn garden_visits(&self) -> u64 {
        self.tended_garden
    }

    pub fn improvements(&self) -> &ImprovementState {
        &self.improvements
    }

    pub fn housing(&self) -> &HousingState {
        &self.housing
    }

    pub fn active(&self) -> Option<ActiveProject> {
        let project = self.running?;
        let chapter = CHAPTERS.get(usize::from(project.id))?;
        Some(ActiveProject {
            id: project.id,
            phase: chapter.phase,
            progress: (12 + u16::from(project.shifts) * 78 / u16::from(chapter.work_shifts)) as u8,
        })
    }

    pub(crate) fn finish_due(&self) -> Option<u64> {
        let project = self.running?;
        let chapter = &CHAPTERS[usize::from(project.id)];
        (project.shifts >= chapter.work_shifts)
            .then_some(project.begun + u64::from(chapter.cure_days) * DAY)
    }

    pub fn is_open(&self, site: DevelopmentKind) -> bool {
        site_open(self.finished, site)
    }

    pub(crate) fn validate_for(&self, now: u64, village: bool) -> bool {
        usize::from(self.finished) <= CHAPTER_COUNT
            && self.milestones & !0b1111 == 0
            && self.last_assessed_day.is_none_or(|day| day <= now / DAY)
            && self.improvements.validate(now)
            && self.housing.validate_for(now, village)
            && self.running.is_none_or(|project| {
                CHAPTERS
                    .get(usize::from(project.id))
                    .is_some_and(|chapter| {
                        project.id == self.finished
                            && project.begun <= now
                            && project.begun > 0
                            && project.shifts <= chapter.work_shifts
                            && project.next_shift >= project.begun
                    })
            })
    }
}

pub(crate) fn site_open(finished: u8, site: DevelopmentKind) -> bool {
    let effect = match site {
        DevelopmentKind::Road => OpenRoad,
        DevelopmentKind::Bridge => OpenBridge,
        DevelopmentKind::Market => OpenMarket,
        DevelopmentKind::Stall => OpenStalls,
        DevelopmentKind::Guesthouse => OpenGuesthouse,
        DevelopmentKind::Depot => OpenDepot,
        DevelopmentKind::Well => OpenWell,
        DevelopmentKind::Garden => OpenGarden,
    };
    let completion = (site as usize + 1) * 3;
    debug_assert_eq!(CHAPTERS[completion - 1].effect, effect);
    usize::from(finished) >= completion
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_shift_bridge_progress_remains_monotonic() {
        let mut state = StoryState {
            finished: 5,
            ..StoryState::default()
        };
        for (shifts, expected) in [12, 27, 43, 58, 74, 90].into_iter().enumerate() {
            state.running = Some(Project {
                id: 5,
                begun: DAY,
                shifts: shifts as u8,
                next_shift: DAY,
            });
            assert_eq!(state.active().unwrap().progress, expected);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct Batch {
    pub expires: u64,
    pub units: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct CropBatches([Batch; 4]);

impl CropBatches {
    pub fn units(&self) -> u16 {
        self.0
            .iter()
            .fold(0_u16, |sum, b| sum.saturating_add(b.units))
    }

    pub fn add(&mut self, units: u16, expires: u64) {
        if units == 0 {
            return;
        }
        if let Some(batch) = self
            .0
            .iter_mut()
            .find(|b| b.units > 0 && b.expires == expires)
        {
            batch.units += units;
        } else if let Some(batch) = self.0.iter_mut().find(|b| b.units == 0) {
            *batch = Batch { expires, units };
        } else {
            // Conservatively merge into the last earlier date. Shelf life never extends on merge.
            let index = self
                .0
                .iter()
                .rposition(|b| b.expires <= expires)
                .unwrap_or(0);
            let batch = &mut self.0[index];
            batch.expires = batch.expires.min(expires);
            batch.units += units;
        }
        self.0.sort_by_key(|b| (b.units == 0, b.expires));
    }

    pub fn take(&mut self, mut units: u16) {
        for batch in &mut self.0 {
            let amount = units.min(batch.units);
            units -= amount;
            batch.units -= amount;
            if batch.units == 0 {
                *batch = Batch::default();
            }
            if units == 0 {
                break;
            }
        }
        self.0.sort_by_key(|b| b.units == 0);
    }

    pub fn expire(&mut self, now: u64) -> u16 {
        let units = self
            .0
            .iter()
            .filter(|b| b.expires <= now)
            .map(|b| b.units)
            .sum();
        self.take(units);
        units
    }

    pub fn next_expiry(&self) -> Option<u64> {
        self.0.iter().find(|b| b.units > 0).map(|b| b.expires)
    }

    pub fn validate(&self, stock: u16, now: u64) -> bool {
        let used = self.0.iter().take_while(|b| b.units > 0).count();
        self.units() <= stock
            && self.0[..used].iter().all(|b| b.expires >= now)
            && self.0[..used]
                .windows(2)
                .all(|pair| pair[0].expires <= pair[1].expires)
            && self.0[used..].iter().all(|b| *b == Batch::default())
    }
}
