//! Short-lived notes from completed work. A live note keeps its slot until
//! expiry. Full storage falls back to the ordinary behavior.
use crate::{legacy_content::LegacyActor, map_data::TilePos};
use serde::{Deserialize, Serialize};

const MINUTE: u64 = 60_000;
const SLOTS: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Observation {
    RestStop,
    CatNap,
    CatNight,
    HouseholdNight,
    GroomPause,
    Fixture,
    Well,
    Soil,
    RootCrop,
    CatWatchesHarvest,
    HarvestTally,
    WorkNotice,
    Menu,
    OldNest,
    HouseholdDrain,
    IrrigatorDrain,
    FallingTree,
    PassingTrail,
    KitchenDetour,
    ReclaimedGround,
    CloudBreak,
    RainShelter,
    FishingWater,
    SparrowCall,
}

impl Observation {
    pub(crate) fn base(self) -> &'static str {
        match self {
            Self::RestStop => "S02",
            Self::CatNap => "A01-08",
            Self::CatNight => "A01-10",
            Self::HouseholdNight => "H01-60",
            Self::GroomPause => "A01-04",
            Self::Fixture => "S13",
            Self::Well => "S18",
            Self::Soil => "H02-01",
            Self::RootCrop => "H01-16",
            Self::CatWatchesHarvest => "A01-31",
            Self::HarvestTally => "H01-57",
            Self::WorkNotice => "H01-56",
            Self::Menu => "H06-09",
            Self::OldNest => "A09-08",
            Self::HouseholdDrain => "H01-34",
            Self::IrrigatorDrain => "H08-04",
            Self::FallingTree => "S22",
            Self::PassingTrail => "S01",
            Self::KitchenDetour => "S14",
            Self::ReclaimedGround => "S09",
            Self::CloudBreak => "S12",
            Self::RainShelter => "S04",
            Self::FishingWater => "H01-29",
            Self::SparrowCall => "A04-10",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::RestStop => "旧歇脚处",
            Self::CatNap => "吃饭后的午睡处",
            Self::CatNight | Self::HouseholdNight => "夜间回屋休息",
            Self::GroomPause => "饮水后停下来理毛",
            Self::Fixture => "新设施已巡看",
            Self::Well => "新修的井已试提四份水",
            Self::Soil => "新畦已开窗验土并回填",
            Self::RootCrop => "已查看根菜长势",
            Self::CatWatchesHarvest => "在畦外看完收菜",
            Self::HarvestTally => "这一批收成已入仓",
            Self::WorkNotice => "施工续班时间已确认",
            Self::Menu => "厨师清点当日可约餐量",
            Self::OldNest => "春归时踏访旧燕巢",
            Self::HouseholdDrain | Self::IrrigatorDrain => "该畦已排涝",
            Self::FallingTree => "附近刚有树木倒下",
            Self::PassingTrail => "让路后沿原计划前进，留下脚印",
            Self::KitchenDetour => "空餐柜后改道开火，留下脚印",
            Self::ReclaimedGround => "清出地块并收回物料",
            Self::CloudBreak => "停步望见云散开",
            Self::RainShelter => "在屋檐下避过这阵雨",
            Self::FishingWater => "从岸上查看水面并下竿",
            Self::SparrowCall => "黎明鸣叫后的一小段余音",
        }
    }

    fn retention(self, mode: u8) -> u64 {
        let minutes = match self {
            Self::FallingTree | Self::CloudBreak | Self::RainShelter | Self::SparrowCall | Self::FishingWater => [5, 15, 30],
            Self::PassingTrail | Self::KitchenDetour => [30, 60, 90],
            Self::RestStop
            | Self::CatNap
            | Self::CatNight
            | Self::HouseholdNight
            | Self::GroomPause => [30, 120, 360],
            _ => [60, 240, 720],
        };
        minutes[usize::from(mode - 1)] * MINUTE
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub kind: Observation,
    pub actor: LegacyActor,
    pub target: TilePos,
    pub at: u64,
    mode: u8,
    #[serde(default, skip_serializing_if = "Evidence::plain")]
    pub evidence: Evidence,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    #[default]
    Place,
    Step(StepMark),
    Removed(u8),
}

impl Evidence {
    fn plain(&self) -> bool { *self == Self::Place }

    pub(super) fn modes(self) -> u8 {
        match self {
            Self::Removed(0) => 0,
            Self::Removed(1) => 0b1010,
            _ => 0b1110,
        }
    }
}

/// An actual contiguous edge. It is stored at fixed size until the corresponding
/// physical task succeeds, at which point it is eligible for a rendered trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepMark {
    pub from: TilePos,
    pub to: TilePos,
}

impl StepMark {
    pub(super) fn valid(self) -> bool {
        self.from.index().is_some() && self.to.index().is_some()
            && self.from.x.abs_diff(self.to.x) + self.from.y.abs_diff(self.to.y) == 1
    }
}

impl Note {
    pub fn expires(self) -> u64 {
        self.at.saturating_add(self.kind.retention(self.mode))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Notes([Option<Note>; SLOTS]);

impl Notes {
    pub(super) fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    pub(super) fn active(&self, now: u64) -> impl Iterator<Item = &Note> + '_ {
        self.0
            .iter()
            .flatten()
            .filter(move |note| now < note.expires())
    }

    pub(super) fn candidate(
        &self,
        kind: Observation,
        actor: LegacyActor,
        target: TilePos,
        now: u64,
        mode: u8,
        evidence: Evidence,
    ) -> Option<(usize, Note)> {
        let note = Note {
            kind,
            actor,
            target,
            at: now,
            mode,
            evidence,
        };
        if let Some((id, previous)) = self.0.iter().enumerate().find_map(|(id, entry)| {
            let entry = (*entry)?;
            (entry.kind == kind
                && entry.actor == actor
                && entry.target == target
                && entry.evidence == evidence
                && entry.expires() > now)
                .then_some((id, entry))
        }) {
            // A fresh, shorter note cannot erase the still-promised longer trace.
            return Some((
                id,
                if note.expires() >= previous.expires() {
                    note
                } else {
                    previous
                },
            ));
        }
        let id = self
            .0
            .iter()
            .position(|entry| entry.is_none_or(|old| old.expires() <= now))?;
        Some((id, note))
    }

    pub(super) fn commit(&mut self, (id, note): (usize, Note)) {
        self.0[id] = Some(note);
    }

    pub(super) fn validate(&self, now: u64) -> bool {
        self.0.iter().flatten().all(|note| {
            (1..=3).contains(&note.mode)
                && note.at <= now
                && note.target.index().is_some()
                && crate::legacy_content::actor_can_record(note.kind.base(), note.actor)
                && match (note.kind, note.evidence) {
                    (Observation::PassingTrail | Observation::KitchenDetour, Evidence::Step(step)) => step.valid(),
                    (Observation::ReclaimedGround, Evidence::Removed(amount)) => (1..=3).contains(&amount),
                    (Observation::PassingTrail | Observation::KitchenDetour | Observation::ReclaimedGround, _) => false,
                    (_, Evidence::Place) => true,
                    _ => false,
                }
        })
    }
}
