//! Wall-calendar authorizations projected onto the queued world clock.
use super::{DAY_MS, advance_clock};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum TownEra {
    #[default]
    Humble,
    Workshop,
    Market,
    MarketStreet,
    SecondBlock,
    PublicDistrict,
    Mature,
    Heritage,
}

impl TownEra {
    pub const ALL: [Self; 8] = [
        Self::Humble,
        Self::Workshop,
        Self::Market,
        Self::MarketStreet,
        Self::SecondBlock,
        Self::PublicDistrict,
        Self::Mature,
        Self::Heritage,
    ];

    pub const fn real_day(self) -> u64 {
        [0, 7, 30, 60, 90, 180, 365, 730][self as usize]
    }

    pub const fn label(self) -> &'static str {
        [
            "小院起步",
            "工坊与扩田",
            "常态小集",
            "市场街",
            "东面住区",
            "公共街区",
            "成熟村镇",
            "原地更新",
        ][self as usize]
    }

    pub const fn for_chapter(chapter: u8) -> Self {
        match chapter {
            25..=35 | 42..=46 => Self::Workshop,
            47..=52 | 54..=56 => Self::Market,
            53 | 57..=59 => Self::MarketStreet,
            60..=62 => Self::SecondBlock,
            63 | 64 | 67 | 68 => Self::PublicDistrict,
            65 | 69 | 71 => Self::Mature,
            _ => Self::Humble,
        }
    }
}

/// The seven broad boundaries and 24 construction permits share their
/// chronological cursor. Permit dates leave visible periods between paid jobs.
pub(super) const PROJECT_DAYS: [u64; 24] = [
    7, 8, 11, 15, 21, 26, 30, 36, 44, 60, 67, 75, 90, 102, 120, 180, 203, 225, 245, 275, 300, 365,
    405, 455,
];
pub(super) const PUBLIC_WORK_DAYS: [u64; 12] = [30, 38, 46, 50, 54, 56, 64, 66, 78, 80, 84, 87];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TownCalendar {
    origin: u64,
    observed: u64,
    unlock_at: [Option<u64>; 8],
    work_at: [Option<u64>; 24],
    public_work_at: [Option<u64>; 12],
    /// v2/v4 fast-era evidence. Only individual already paid jobs are exempt.
    pub inherited_bits: u16,
    pub inherited_market_finish: u8,
    /// A later repair receipt for the original courtyard and street.
    #[serde(default)]
    pub renewed: bool,
}

impl TownCalendar {
    pub fn new(
        origin: u64,
        observed: u64,
        at_world: u64,
        inherited_bits: u16,
        inherited_market_finish: u8,
    ) -> Self {
        let mut unlock_at = [None; 8];
        unlock_at[0] = Some(at_world);
        for era in TownEra::ALL.into_iter().skip(1) {
            if origin
                .checked_add(era.real_day() * DAY_MS)
                .is_some_and(|due| due <= observed)
            {
                unlock_at[era as usize] = Some(at_world);
            }
        }
        let work_at = PROJECT_DAYS.map(|id| {
            origin
                .checked_add(id * DAY_MS)
                .filter(|&due| due <= observed)
                .map(|_| at_world)
        });
        let public_work_at = PUBLIC_WORK_DAYS.map(|day| {
            origin
                .checked_add(day * DAY_MS)
                .filter(|&due| due <= observed)
                .map(|_| at_world)
        });
        Self {
            origin,
            observed,
            unlock_at,
            work_at,
            public_work_at,
            inherited_bits,
            inherited_market_finish,
            renewed: false,
        }
    }

    pub fn observe(&mut self, wall_ms: u64, previous_world_target: u64, fraction: u8) {
        if wall_ms <= self.observed {
            return;
        }
        let markers = TownEra::ALL
            .into_iter()
            .map(TownEra::real_day)
            .zip(&mut self.unlock_at)
            .chain(PROJECT_DAYS.into_iter().zip(&mut self.work_at))
            .chain(PUBLIC_WORK_DAYS.into_iter().zip(&mut self.public_work_at));
        for (day, marker) in markers {
            if marker.is_none()
                && let Some(due) = self
                    .origin
                    .checked_add(day * DAY_MS)
                    .filter(|&due| due <= wall_ms)
            {
                let elapsed = due.saturating_sub(self.observed);
                *marker = Some(
                    advance_clock(
                        previous_world_target,
                        u128::from(elapsed) * 96 + u128::from(fraction),
                    )
                    .0,
                );
            }
        }
        self.observed = wall_ms;
    }

    pub fn days(&self) -> u64 {
        self.observed.saturating_sub(self.origin) / DAY_MS
    }

    pub fn allows(&self, era: TownEra, now: u64) -> bool {
        self.unlock_at[era as usize].is_some_and(|at| at <= now)
    }

    pub fn next_boundary(&self, now: u64) -> Option<u64> {
        [
            &self.unlock_at[..],
            &self.work_at[..],
            &self.public_work_at[..],
        ]
        .into_iter()
        .filter_map(|dates| {
            let index = dates.partition_point(|date| date.is_some_and(|at| at <= now));
            dates.get(index).copied().flatten()
        })
        .min()
    }

    pub fn permits_project(&self, id: u8, now: u64) -> bool {
        self.work_at
            .get(usize::from(id))
            .copied()
            .flatten()
            .is_some_and(|at| at <= now)
    }

    pub fn permits_public_work(&self, stage: u8, now: u64) -> bool {
        stage < self.inherited_market_finish
            || self
                .public_work_at
                .get(usize::from(stage))
                .copied()
                .flatten()
                .is_some_and(|at| at <= now)
    }

    pub fn validate(&self, target: u64, last_wall: u64) -> bool {
        self.observed == last_wall
            && self.unlock_at[0].is_some_and(|at| at <= target)
            && self.inherited_bits & !0b11_1111 == 0
            && matches!(self.inherited_market_finish, 0 | 4 | 6 | 8 | 11 | 12)
            && self.unlock_at.into_iter().flatten().is_sorted()
            && self.work_at.into_iter().flatten().is_sorted()
            && self.public_work_at.into_iter().flatten().is_sorted()
            && TownEra::ALL
                .into_iter()
                .map(TownEra::real_day)
                .zip(self.unlock_at)
                .skip(1)
                .chain(PROJECT_DAYS.into_iter().zip(self.work_at))
                .chain(PUBLIC_WORK_DAYS.into_iter().zip(self.public_work_at))
                .all(|(day, marker)| {
                    let due = self.origin.checked_add(day * DAY_MS);
                    marker.is_some() == due.is_some_and(|due| due <= self.observed)
                        && marker.is_none_or(|at| at <= target)
                })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_real_calendar_boundaries_is_independent_of_queue_size() {
        let origin = 100_000;
        let start = 4 * DAY_MS + 6 * super::super::HOUR;
        let mut once = TownCalendar::new(origin, origin, start, 0, 0);
        let mut split = once.clone();
        let last = origin + 731 * DAY_MS;
        once.observe(last, start, 0);
        let mut target = start;
        let mut fraction = 0;
        let mut previous = origin;
        for era in TownEra::ALL.into_iter().skip(1) {
            let due = origin + era.real_day() * DAY_MS;
            for wall in [due - 1, due] {
                split.observe(wall, target, fraction);
                (target, fraction) = advance_clock(
                    target,
                    u128::from(wall - previous) * 96 + u128::from(fraction),
                );
                assert_eq!(split.allows(era, target), wall == due);
                assert_eq!(once.allows(era, target), wall == due);
                previous = wall;
            }
        }
        split.observe(last, target, fraction);
        assert_eq!(split, once);
        split.observe(origin, target, fraction);
        assert_eq!(split, once);
        assert!(once.validate(advance_clock(start, 731 * u128::from(DAY_MS) * 96).0, last));
    }
}
