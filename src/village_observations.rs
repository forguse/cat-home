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
        }
    }

    fn retention(self, mode: u8) -> u64 {
        let minutes = match self {
            Self::FallingTree => [5, 15, 30],
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
    ) -> Option<(usize, Note)> {
        let note = Note {
            kind,
            actor,
            target,
            at: now,
            mode,
        };
        if let Some((id, previous)) = self.0.iter().enumerate().find_map(|(id, entry)| {
            let entry = (*entry)?;
            (entry.kind == kind
                && entry.actor == actor
                && entry.target == target
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
        })
    }
}
