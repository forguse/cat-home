//! Fixed-size evidence pages and independent calendar/running milestones.
use super::*;

const MAX_MARKS: u8 = 0b1111;
const MIN_CALENDAR_ACTIVE: u64 = 12 * HOUR;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Work {
    Review(u8),
    ReadTogether(u8),
    NoteReturn,
    CareKeepsake,
    Celebrate(u8),
    Gather,
    RenewCorner,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    pub created: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_seen_v4: Option<u64>,
    anniversary: Option<[u64; 2]>,
    pub marks: u8,
    pub celebrated: u8,
    pub notebook_pages: [u16; 12],
    pub pages_reviewed: u8,
    pub pages_read: u8,
    pub familiar_return: bool,
    pub polished: bool,
    pub gathering: Option<u8>,
}

impl State {
    pub(super) fn new(created: Option<u64>) -> Self {
        Self {
            created,
            ..Self::default()
        }
    }
    pub(super) fn completed(&self, chapter: &str) -> bool {
        match chapter {
            "Q70" => self.pages_read == 0b11,
            "Q71" => self.familiar_return,
            "Q72" => self.celebrated != 0,
            _ => false,
        }
    }
    pub(super) fn observe_clock(&mut self, now: u64, active: u64) {
        if let Some(origin) = self.created.or(self.first_seen_v4)
            && active >= MIN_CALENDAR_ACTIVE
            && self.marks & 3 != 3
        {
            if self.anniversary.is_none() {
                self.anniversary = Some([
                    anniversary_ms(origin, 1).unwrap_or(u64::MAX),
                    anniversary_ms(origin, 2).unwrap_or(u64::MAX),
                ]);
            }
            for (index, due) in self.anniversary.unwrap().into_iter().enumerate() {
                if now >= due {
                    self.marks |= 1 << index;
                }
            }
        }
        if active >= 365 * DAY_MS {
            self.marks |= 1 << 2;
        }
        if active >= 730 * DAY_MS {
            self.marks |= 1 << 3;
        }
    }

    pub(super) fn note_first_v4(&mut self, now: u64) {
        if self.created.is_none() && self.first_seen_v4.is_none() {
            self.first_seen_v4 = Some(now);
        }
    }

    pub(super) fn calendar_origin(&self) -> Option<u64> {
        self.created.or(self.first_seen_v4)
    }
    pub(super) fn task(&self, i: usize, now: u64, revisit: bool) -> Option<Work> {
        if i >= 2 || now / DAY_MS < 56 {
            return None;
        }
        let bit = 1 << i;
        if self.pages_reviewed & bit == 0 {
            return Some(Work::Review(i as u8));
        }
        if self.pages_reviewed == 0b11 && self.pages_read & bit == 0 {
            return Some(Work::ReadTogether(i as u8));
        }
        if i == 0 && revisit && self.completed("Q70") && !self.familiar_return {
            return Some(Work::NoteReturn);
        }
        let pending = self.marks & !self.celebrated;
        if pending != 0 && self.completed("Q70") {
            if i == 1 && !self.polished {
                return Some(Work::CareKeepsake);
            }
            if i == 0 && self.polished && self.gathering.is_none() {
                return Some(Work::Celebrate(pending.trailing_zeros() as u8));
            }
            if self.gathering.is_some() {
                return Some(Work::Gather);
            }
        }
        None
    }

    pub(super) fn validate(&self) -> bool {
        self.marks <= MAX_MARKS
            && (self.created.is_none() || self.first_seen_v4.is_none())
            && self.celebrated & !self.marks == 0
            && self.pages_reviewed <= 0b11
            && self.pages_read & !self.pages_reviewed == 0
            && self.notebook_pages.into_iter().all(|chapter| chapter <= 72)
            && (!self.familiar_return || self.completed("Q70"))
            && (!self.polished || self.completed("Q70"))
            && self.gathering.is_none_or(|mark| {
                mark < 4
                    && self.marks & (1 << mark) != 0
                    && self.polished
                    && self.celebrated & (1 << mark) == 0
            })
            && self.anniversary.is_none_or(|due| {
                self.created.or(self.first_seen_v4).is_some_and(|origin| {
                    due[0] == anniversary_ms(origin, 1).unwrap_or(u64::MAX)
                        && due[1] == anniversary_ms(origin, 2).unwrap_or(u64::MAX)
                })
            })
    }
}

// Convert epoch days through Gregorian years. The supported window is fixed
// and tiny; saturating inputs cannot incur unbounded calendar work.
fn anniversary_ms(ms: u64, years: u16) -> Option<u64> {
    let mut days = ms / DAY_MS;
    let mut year = 1970u16;
    while year <= 2500 {
        let span = 365 + u64::from(leap(year));
        if days < span {
            break;
        }
        days -= span;
        year += 1;
    }
    if year > 2500 {
        return None;
    }
    let mut month = 1;
    while days >= month_days(year, month) {
        days -= month_days(year, month);
        month += 1;
    }
    let end = year + years;
    let prev_days: u64 = (1970..end).map(|y| 365 + u64::from(leap(y))).sum();
    let months: u64 = (1..month).map(|m| month_days(end, m)).sum();
    let day = days.min(month_days(end, month) - 1);
    Some((prev_days + months + day) * DAY_MS + ms % DAY_MS)
}
fn leap(y: u16) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}
fn month_days(y: u16, m: u8) -> u64 {
    match m {
        2 => 28 + u64::from(leap(y)),
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn february_anniversary_and_active_time_do_not_unlock_each_other() {
        // 2024-02-29 UTC; in non-leap years, use the last day of February.
        let origin = 1_709_164_800_000;
        let next = 1_740_700_800_000;
        assert_eq!(anniversary_ms(origin, 1), Some(next));
        let mut state = State::new(Some(origin));
        state.observe_clock(next, 11 * HOUR);
        assert_eq!(state.marks, 0);
        state.observe_clock(next, 12 * HOUR);
        assert_eq!(state.marks, 1);
        let mut active_only = State::new(None);
        active_only.observe_clock(next, 730 * DAY_MS);
        assert_eq!(active_only.marks, 0b1100);
        assert!(state.validate() && active_only.validate());
        assert!(!active_only.completed("Q72"));
    }

    #[test]
    fn first_migrated_observation_preserves_unknown_creation_date() {
        let mut state = State::new(None);
        let observed = 1_709_164_800_000;
        let due = 1_740_700_800_000;
        state.note_first_v4(observed);
        state.note_first_v4(due);
        state.observe_clock(due - 1, 12 * HOUR);
        assert_eq!(state.marks, 0);
        state.observe_clock(due, 12 * HOUR);
        assert_eq!(state.created, None);
        assert_eq!(state.first_seen_v4, Some(observed));
        assert_eq!(state.marks, 1);
        assert!(state.validate());
    }
}
