//! One occupied house can be under renovation at a time.
use super::*;

pub(super) const PATH: [TilePos; 8] = [
    TilePos::new(13, 18),
    TilePos::new(14, 18),
    TilePos::new(15, 18),
    TilePos::new(16, 18),
    TilePos::new(17, 18),
    TilePos::new(18, 18),
    TilePos::new(19, 18),
    TilePos::new(20, 18),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Work {
    Inspect(u8),
    MoveOut(u8),
    Repair(u8),
    MoveBack(u8),
    TryCorner(u8),
    SurveyPath,
    LayPath,
    TestPath,
    BuildCorner,
    RestAtCorner,
    PrepareGuests,
    CloseGuestTrial,
}

impl Work {
    pub(super) fn owner(self) -> usize {
        usize::from(match self {
            Self::Inspect(i)
            | Self::MoveOut(i)
            | Self::Repair(i)
            | Self::MoveBack(i)
            | Self::TryCorner(i) => i,
            _ => 0,
        })
    }
    pub(super) fn transports(self) -> bool {
        matches!(self, Self::MoveOut(_) | Self::MoveBack(_))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    inspected: u8,
    pub displaced: Option<u8>,
    #[serde(skip_serializing_if = "super::village::is_false")]
    pub displaced_room_empty: bool,
    pub progress: u8,
    pub next: u64,
    returned: u8,
    pub(crate) done: u8,
    pub had_sleep: bool,
    pub checked: bool,
    pub path: u8,
    pub path_next: u64,
    pub corner: bool,
    pub corner_used: [u8; 2],
    pub corner_day: [Option<u64>; 2],
    pub guest_day: Option<u64>,
    pub guest_table: u8,
    pub guests_ready: bool,
}

impl State {
    pub(super) fn progress_started(&self) -> bool {
        self.inspected != 0
    }

    pub(super) fn completed(&self, chapter: &str) -> bool {
        match chapter {
            "Q30" => self.checked,
            "Q31" => self.done & 1 != 0,
            "Q32" => self.done == 0b11,
            "Q33" => self.path == 4,
            "Q34" => self.corner_used == [2, 2],
            "Q35" => self.guests_ready,
            _ => false,
        }
    }
    pub(super) fn task(&self, worker: usize, now: u64) -> Option<Work> {
        if now / DAY_MS < 14 || self.done == 0b11 {
            return None;
        }
        let owner = u8::from(self.done == 1);
        if self.inspected & (1 << owner) == 0 {
            return (worker == usize::from(owner)).then_some(Work::Inspect(owner));
        }
        if let Some(occupant) = self.displaced {
            return if now < self.next {
                None
            } else if self.progress < 3
                && (self.had_sleep || self.displaced_room_empty)
                && worker == 1
            {
                Some(Work::Repair(occupant))
            } else if self.progress == 3 && worker == usize::from(occupant) {
                Some(Work::MoveBack(occupant))
            } else {
                None
            };
        }
        if worker != usize::from(owner) {
            return None;
        }
        if self.returned & (1 << owner) != 0 {
            Some(Work::TryCorner(owner))
        } else {
            Some(Work::MoveOut(owner))
        }
    }
    pub(super) fn repair_cost(&self) -> (Item, u16) {
        (
            [Item::Wood, Item::Stone, Item::Rope][usize::from(self.progress.min(2))],
            1,
        )
    }
    pub(super) fn commit(&mut self, work: Work, now: u64) {
        let owner = work.owner();
        match work {
            Work::Inspect(_) => self.inspected |= 1 << owner,
            Work::MoveOut(i) => {
                self.displaced = Some(i);
                self.had_sleep = false;
                self.next = now + DAY_MS;
            }
            Work::Repair(_) => {
                self.checked = true;
                self.progress += 1;
                self.next = now + 2 * DAY_MS;
            }
            Work::MoveBack(_) => {
                self.displaced = None;
                self.displaced_room_empty = false;
                self.progress = 0;
                self.had_sleep = false;
                self.next = 0;
                self.returned |= 1 << owner;
            }
            Work::TryCorner(_) => self.done |= 1 << owner,
            Work::SurveyPath => {
                self.path = 1;
                self.path_next = now + DAY_MS;
            }
            Work::LayPath => {
                self.path += 1;
                self.path_next = now + DAY_MS;
            }
            Work::TestPath => {
                self.path = 4;
                self.path_next = 0;
            }
            Work::BuildCorner => self.corner = true,
            Work::RestAtCorner => {}
            Work::PrepareGuests => {
                self.guest_day = Some(now / DAY_MS);
                self.guest_table = 2;
            }
            Work::CloseGuestTrial => {
                self.guest_table = 0;
                self.guests_ready = true;
            }
        }
    }

    pub(super) fn park_rest(&mut self, i: usize, day: u64) {
        if self.corner && i < 2 && self.corner_day[i] != Some(day) {
            self.corner_day[i] = Some(day);
            self.corner_used[i] = self.corner_used[i].saturating_add(1).min(2);
        }
    }

    pub(super) fn walk_surface(&self, pos: TilePos) -> bool {
        let width = match self.path {
            0 | 1 => 0,
            2 => 4,
            _ => PATH.len(),
        };
        PATH[..width].contains(&pos)
    }

    pub(super) fn common_task(
        &self,
        i: usize,
        now: u64,
        outlet: bool,
        rest: bool,
        supplies: bool,
    ) -> Option<Work> {
        if self.done != 0b11 || i >= 2 {
            return None;
        }
        let day = now / DAY_MS;
        Some(match self.path {
            0 if outlet && i == 0 => Work::SurveyPath,
            1 | 2 if i == 1 && now >= self.path_next => Work::LayPath,
            3 if i == 0 && now >= self.path_next => Work::TestPath,
            4 if !self.corner && rest && i == 1 => Work::BuildCorner,
            4 if self.corner && self.corner_used[i] < 2 && self.corner_day[i] != Some(day) => {
                Work::RestAtCorner
            }
            4 if i == 0
                && self.corner_used == [2, 2]
                && supplies
                && day >= 56
                && self.guest_day.is_none() =>
            {
                Work::PrepareGuests
            }
            4 if i == 1
                && self.guest_table > 0
                && self.guest_day.is_some_and(|started| day > started) =>
            {
                Work::CloseGuestTrial
            }
            _ => return None,
        })
    }
    pub(super) fn validate(&self, now: u64) -> bool {
        self.inspected <= 3
            && self.returned <= 3
            && matches!(self.done, 0 | 1 | 3)
            && self.done & !self.returned == 0
            && self.returned & !self.inspected == 0
            && self.displaced.is_none_or(|i| {
                i < 2
                    && self.inspected & (1 << i) != 0
                    && self.returned & (1 << i) == 0
                    && self.next <= now.saturating_add(2 * DAY_MS)
            })
            && (self.displaced.is_some() || self.progress == 0 && self.next == 0)
            && (!self.displaced_room_empty || self.displaced.is_some() && !self.had_sleep)
            && self.progress <= 3
            && (!self.had_sleep || self.displaced.is_some())
            && (self.progress == 0 || (self.had_sleep || self.displaced_room_empty) && self.checked)
            && self.path <= 4
            && self.path_next <= now.saturating_add(DAY_MS)
            && (self.path == 0 || self.done == 0b11)
            && (!self.corner || self.path == 4)
            && self.corner_used.into_iter().all(|uses| uses <= 2)
            && self
                .corner_day
                .into_iter()
                .flatten()
                .all(|day| day <= now / DAY_MS)
            && self
                .guest_day
                .is_none_or(|day| (56..=now / DAY_MS).contains(&day))
            && matches!(self.guest_table, 0 | 2)
            && (!self.guests_ready || self.guest_table == 0 && self.guest_day.is_some())
    }
}
