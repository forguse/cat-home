//! An eight-day real surface project, built beside the open bridge route.
use super::*;

pub(super) const SHOULDER: [TilePos; 4] = [
    TilePos::new(56, 41),
    TilePos::new(57, 41),
    TilePos::new(58, 41),
    TilePos::new(59, 41),
];
pub(super) const MARKET_LANE: [TilePos; 6] = [
    TilePos::new(97, 50),
    TilePos::new(98, 50),
    TilePos::new(99, 50),
    TilePos::new(100, 50),
    TilePos::new(101, 50),
    TilePos::new(102, 50),
];
pub(super) const MARKET_LAMPS: [TilePos; 6] = [
    TilePos::new(80, 52),
    TilePos::new(107, 44),
    TilePos::new(106, 51),
    TilePos::new(79, 46),
    TilePos::new(93, 47),
    TilePos::new(95, 50),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Work {
    SurveyWest,
    SurveyEast,
    Approve,
    Build,
    InspectEast,
    CarryTest,
    ReturnTest,
    PackOrder,
    DeliverOrder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum MarketWork {
    Survey,
    Base,
    Pave,
    Walk,
    Drain,
    TestWater,
    MoveLamp,
    NightWalk,
    CarryMill,
    SetMill,
    InspectMill,
    Sweep,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Market {
    pub stage: u8,
    pub next: u64,
    pub lamps: u8,
    pub clearings: u8,
    pub last_cleanup: Option<u64>,
}

impl Market {
    pub(super) fn completed(&self, chapter: &str) -> bool {
        match chapter {
            "Q54" => self.stage >= 1,
            "Q55" => self.stage >= 4,
            "Q56" => self.stage >= 6,
            "Q57" => self.stage >= 8,
            "Q58" => self.stage >= 11,
            "Q59" => self.clearings >= 3,
            _ => false,
        }
    }
    pub(super) fn task(
        &self,
        i: usize,
        now: u64,
        reviewed: bool,
        fixed_market: bool,
        cleanup: Option<u64>,
        mill_built: bool,
    ) -> Option<MarketWork> {
        let day = now / DAY_MS;
        if now < self.next || day < 224 {
            return None;
        }
        Some(match (self.stage, i) {
            (0, 0) if reviewed => MarketWork::Survey,
            (1, 1) => MarketWork::Base,
            (2, 1) => MarketWork::Pave,
            (3, 0) => MarketWork::Walk,
            (4, 1) => MarketWork::Drain,
            (5, 0) => MarketWork::TestWater,
            (6, 1) if day >= 336 && fixed_market => MarketWork::MoveLamp,
            (7, 1) if self.lamps < 6 => MarketWork::MoveLamp,
            (7, 0) if self.lamps == 6 && (now % DAY_MS / HOUR) >= 18 => MarketWork::NightWalk,
            (8, 1) if fixed_market && mill_built => MarketWork::CarryMill,
            (9, 1) => MarketWork::SetMill,
            (10, 0) => MarketWork::InspectMill,
            (11, _) if self.cleanup_due(day, cleanup).is_some() => MarketWork::Sweep,
            _ => return None,
        })
    }
    pub(super) fn cost(work: MarketWork) -> Option<(Item, u16)> {
        match work {
            MarketWork::Base | MarketWork::Pave => Some((Item::Stone, 1)),
            MarketWork::Drain | MarketWork::SetMill => Some((Item::Wood, 1)),
            MarketWork::TestWater => Some((Item::Water, 1)),
            _ => None,
        }
    }
    pub(super) fn commit(&mut self, work: MarketWork, now: u64, cleanup: Option<u64>) {
        if work == MarketWork::MoveLamp {
            self.lamps = self.lamps.saturating_add(1).min(6);
        } else if work == MarketWork::Sweep {
            self.clearings += 1;
            self.last_cleanup = cleanup;
            self.next = now + 3 * DAY_MS;
            return;
        }
        if work == MarketWork::MoveLamp && self.stage == 7 {
            self.next = now + DAY_MS / 2;
            return;
        }
        self.stage += 1;
        self.next = now
            + match work {
                MarketWork::Survey => 3 * DAY_MS,
                MarketWork::Base | MarketWork::Pave => 5 * DAY_MS,
                MarketWork::Drain => 4 * DAY_MS,
                MarketWork::SetMill => 8 * DAY_MS,
                _ => 0,
            };
    }
    pub(super) fn covered(&self, at: TilePos) -> bool {
        let width = match self.stage {
            0..=1 => 0,
            2 => 3,
            _ => MARKET_LANE.len(),
        };
        MARKET_LANE[..width].contains(&at)
    }

    pub(super) fn cleanup_due(&self, day: u64, cleanup: Option<u64>) -> Option<u64> {
        cleanup.filter(|&close| {
            (close..=close.saturating_add(1)).contains(&day)
                && self.clearings < 3
                && self.last_cleanup.is_none_or(|last| close > last)
        })
    }
    pub(super) fn validate(&self, now: u64) -> bool {
        self.stage <= 11
            && self.next <= now.saturating_add(8 * DAY_MS)
            && self.lamps <= 6
            && (self.stage >= 7 || self.lamps == 0)
            && (self.stage < 8 || self.lamps == 6)
            && self.clearings <= 3
            && self.last_cleanup.is_none_or(|day| day <= now / DAY_MS)
            && (self.clearings == 0 || self.stage == 11 && self.last_cleanup.is_some())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    pub stage: u8,
    pub due: u64,
    pub loaded: Option<(Item, u16)>,
}

impl State {
    pub(super) fn completed(&self, chapter: &str) -> bool {
        match chapter {
            "Q42" => self.stage >= 2,
            "Q43" => self.stage >= 3,
            "Q44" => self.stage >= 6,
            "Q45" => self.stage >= 9,
            "Q46" => self.stage >= 12,
            _ => false,
        }
    }
    pub(super) fn task(&self, i: usize, now: u64) -> Option<Work> {
        if now < self.due || now / DAY_MS < 56 {
            return None;
        }
        Some(match (i, self.stage) {
            (0, 0) => Work::SurveyWest,
            (0, 1) => Work::SurveyEast,
            (1, 2) => Work::Approve,
            (1, 3..=5) => Work::Build,
            (0, 6) => Work::InspectEast,
            (0, 7) => Work::CarryTest,
            (0, 8) => Work::ReturnTest,
            (1, 9) => Work::PackOrder,
            (1, 10) => Work::DeliverOrder,
            _ => return None,
        })
    }
    pub(super) fn cost(&self) -> Option<(Item, u16)> {
        match self.stage {
            3 => Some((Item::Stone, 1)),
            4 => Some((Item::Wood, 1)),
            5 => Some((Item::Rope, 1)),
            7 => Some((Item::Crops, 2)),
            9 => Some((Item::Food, 2)),
            _ => None,
        }
    }
    pub(super) fn covered(&self, at: TilePos) -> bool {
        let width = match self.stage {
            0..=3 => 0,
            4 => 1,
            5 => 2,
            _ => SHOULDER.len(),
        };
        SHOULDER[..width].contains(&at)
    }
    pub(super) fn commit(&mut self, now: u64) {
        self.stage += 1;
        self.due = now
            + match self.stage {
                3 | 4 => 3 * DAY_MS,
                5 => 2 * DAY_MS,
                _ => 0,
            };
    }
    pub(super) fn validate(&self, now: u64) -> bool {
        self.stage <= 12
            && self.due <= now.saturating_add(3 * DAY_MS)
            && self.loaded
                == match self.stage {
                    8 => Some((Item::Crops, 2)),
                    10 | 11 => Some((Item::Food, 2)),
                    _ => None,
                }
    }
}
