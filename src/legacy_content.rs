//! Identity bridge for completed, physically executed village actions.
//! Producers own work, custody and recovery; this ledger grants no materials,
//! time, progress or animation. Unregistered old rules never run here.
use crate::map_data::TilePos;
use serde::{Deserialize, Serialize};

const DAY: u64 = 86_400_000;
const RECENT_LIMIT: usize = 12;
const ID_COUNT: usize = 5_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LegacyActor {
    Household(u8),
    Resident(u8),
    Wildlife(u8),
}

impl LegacyActor {
    fn role(self) -> Option<u8> {
        match self {
            Self::Household(0 | 1) => Some(0),
            Self::Household(2) => Some(10),
            Self::Resident(0) => Some(6),
            Self::Resident(1) => Some(9),
            Self::Resident(2) => Some(2),
            Self::Resident(3) => Some(5),
            Self::Resident(8) => Some(8),
            Self::Resident(16) => Some(7),
            Self::Wildlife(id) => [
                11, 12, 14, 15, 16, 17, 27, 28, 13, 18, 19, 24, 25, 29, 20, 21, 22, 23, 26,
            ]
            .get(usize::from(id))
            .copied(),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Household(0) => "小禾",
            Self::Household(1) => "阿榆",
            Self::Household(2) => "橘子",
            Self::Resident(index) if index < 20 => {
                crate::visitors::Profession::resident(usize::from(index)).label()
            }
            Self::Wildlife(index) => [
                "灰猫",
                "黑猫",
                "松鼠",
                "刺猬",
                "兔子",
                "青蛙",
                "蜥蜴",
                "田鼠",
                "麻雀",
                "燕子",
                "小鸭",
                "乌鸦",
                "山雀",
                "白鸽",
                "蜗牛",
                "瓢虫",
                "萤火虫",
                "蜜蜂",
                "蝴蝶",
            ]
            .get(usize::from(index))
            .copied()
            .unwrap_or("动物"),
            _ => "来客",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LegacyPlace {
    Nearby,
    Front,
    Expanded,
}

#[derive(Clone, Copy, Debug)]
pub struct CompletedBehavior {
    pub base: &'static str,
    pub actor: LegacyActor,
    pub at: TilePos,
    pub target: TilePos,
    pub parcel: LegacyPlace,
    pub mode: u8,
    pub partner: Option<LegacyActor>,
    pub now_ms: u64,
    /// Clear 0, clouds 1, rain 2, snow 3.
    pub weather: u8,
    /// Spring 0, summer 1, autumn 2, winter 3.
    pub season: u8,
}

struct Group {
    base: &'static str,
    start: usize,
    len: usize,
    roles: u32,
    display: &'static str,
}
struct Rule {
    id: &'static str,
    ordinal: u16,
    period: u8,
    weather: u8,
    season: u8,
    place: u8,
    mode: u8,
    partner: u8,
    label: &'static str,
}
include!("legacy_bindings.rs");

fn period(ms: u64) -> u8 {
    match ms % DAY / 60_000 {
        300..450 => 0,
        450..1080 => 1,
        1080..1200 => 2,
        _ => 3,
    }
}

impl Rule {
    fn matches(&self, event: CompletedBehavior) -> bool {
        if self.partner != 255 && event.partner.and_then(LegacyActor::role) != Some(self.partner) {
            return false;
        }
        if self.mode == 0 {
            return true;
        }
        let season = match event.season {
            0 => 0,
            2 => 1,
            3 => 2,
            _ => 255,
        };
        let place = match self.place {
            0 => true,
            1 => event.parcel == LegacyPlace::Front,
            2 => event.parcel == LegacyPlace::Expanded,
            3 => event.at.x < event.target.x,
            4 => event.at.x == event.target.x && event.at.y >= event.target.y,
            5 => event.at.x > event.target.x,
            _ => false,
        };
        self.mode == event.mode
            && self.period == period(event.now_ms)
            && self.weather == event.weather
            && self.season == season
            && place
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservedBehavior {
    id: u16,
    pub actor: LegacyActor,
    pub at: TilePos,
    pub target: TilePos,
    pub now_ms: u64,
}
impl ObservedBehavior {
    pub fn id(&self) -> &'static str {
        RULES
            .iter()
            .find(|r| r.ordinal == self.id)
            .map_or("", |r| r.id)
    }
    pub fn label(&self) -> &'static str {
        RULES
            .iter()
            .find(|r| r.ordinal == self.id)
            .map_or("", |r| r.label)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BehaviorLedger {
    // Stable IDs leave room for all 5000; never append one item per day.
    seen: [[u64; 32]; 3],
    #[serde(deserialize_with = "deserialize_recent")]
    recent: Vec<ObservedBehavior>,
    completed: u64,
}

// Reject oversize input while reading it, before allocating a history-sized
// vector. The outer save cap is a second, independent boundary.
fn deserialize_recent<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ObservedBehavior>, D::Error> {
    struct RecentVisitor;
    impl<'de> serde::de::Visitor<'de> for RecentVisitor {
        type Value = Vec<ObservedBehavior>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "at most {RECENT_LIMIT} recent completed behaviors")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut recent = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(RECENT_LIMIT));
            while let Some(row) = seq.next_element::<ObservedBehavior>()? {
                if recent.len() == RECENT_LIMIT {
                    return Err(serde::de::Error::custom(
                        "too many recent completed behaviors",
                    ));
                }
                recent.push(row);
            }
            Ok(recent)
        }
    }
    deserializer.deserialize_seq(RecentVisitor)
}

fn group_of(ordinal: u16) -> Option<&'static Group> {
    GROUPS.iter().find(|g| {
        RULES[g.start..g.start + g.len]
            .iter()
            .any(|r| r.ordinal == ordinal)
    })
}

impl BehaviorLedger {
    pub fn is_empty(&self) -> bool {
        self.completed == 0
    }
    fn seen(&self, n: u16) -> bool {
        let n = usize::from(n);
        self.seen[n / 2048][n / 64 % 32] & (1 << (n % 64)) != 0
    }
    pub fn record(&mut self, event: CompletedBehavior) -> bool {
        let Some(role) = event.actor.role() else {
            return false;
        };
        let Ok(index) = GROUPS.binary_search_by_key(&event.base, |g| g.base) else {
            return false;
        };
        let group = &GROUPS[index];
        if group.roles & (1 << role) == 0
            || event.at.index().is_none()
            || event.target.index().is_none()
            || event.weather > 3
            || event.season > 3
            || !(1..=3).contains(&event.mode)
        {
            return false;
        }
        let rows = &RULES[group.start..group.start + group.len];
        if !rows[0].matches(event) {
            return false;
        }
        // A single real completion selects one identity, never all variants.
        // Its narrow context must have actually occurred; weather/time/position
        // are not invented to increase coverage.
        let selected = rows[1..]
            .iter()
            .filter(|rule| rule.matches(event))
            .min_by_key(|rule| (self.seen(rule.ordinal), rule.ordinal))
            .unwrap_or(&rows[0]);
        if self.recent.iter().any(|last| {
            last.actor == event.actor
                && last.now_ms == event.now_ms
                && last.at == event.at
                && rows.iter().any(|r| r.ordinal == last.id)
        }) {
            return false;
        }
        let n = usize::from(selected.ordinal);
        self.seen[n / 2048][n / 64 % 32] |= 1 << (n % 64);
        self.completed = self.completed.saturating_add(1);
        // Repeated footsteps cannot evict all other kinds of work from the
        // notebook. Keep the latest occurrence per actor and behavior family.
        if let Some(old) = self.recent.iter().position(|last| {
            last.actor == event.actor && rows.iter().any(|rule| rule.ordinal == last.id)
        }) {
            self.recent.remove(old);
        }
        if self.recent.len() == RECENT_LIMIT {
            self.recent.remove(0);
        }
        self.recent.push(ObservedBehavior {
            id: selected.ordinal,
            actor: event.actor,
            at: event.at,
            target: event.target,
            now_ms: event.now_ms,
        });
        true
    }
    pub fn validate(&self, now_ms: u64) -> bool {
        static ALLOWED: std::sync::OnceLock<[[u64; 32]; 3]> = std::sync::OnceLock::new();
        let allowed = ALLOWED.get_or_init(|| {
            let mut bits = [[0u64; 32]; 3];
            for rule in RULES {
                let n = usize::from(rule.ordinal);
                assert!(n < ID_COUNT);
                bits[n / 2048][n / 64 % 32] |= 1 << (n % 64);
            }
            bits
        });
        self.recent.len() <= RECENT_LIMIT
            && self.completed >= self.observed_count() as u64
            && (self.completed == 0) == (self.observed_count() == 0)
            && (self.completed == 0) == self.recent.is_empty()
            && self
                .recent
                .windows(2)
                .all(|pair| pair[0].now_ms <= pair[1].now_ms)
            && self.recent.iter().enumerate().all(|(i, r)| {
                r.now_ms <= now_ms
                    && r.at.index().is_some()
                    && r.target.index().is_some()
                    && group_of(r.id).is_some_and(|group| {
                        r.actor
                            .role()
                            .is_some_and(|role| group.roles & (1 << role) != 0)
                            && !self.recent[..i].iter().any(|old| {
                                old.actor == r.actor
                                    && group_of(old.id)
                                        .is_some_and(|old_group| old_group.base == group.base)
                            })
                    })
                    && self.seen(r.id)
            })
            && self
                .seen
                .iter()
                .flatten()
                .zip(allowed.iter().flatten())
                .all(|(seen, allowed)| seen & !allowed == 0)
    }
    pub fn observed_count(&self) -> usize {
        self.seen
            .iter()
            .flatten()
            .map(|n| n.count_ones() as usize)
            .sum()
    }
    pub fn recent(&self) -> &[ObservedBehavior] {
        &self.recent
    }
    pub fn observed_ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        RULES
            .iter()
            .filter(|rule| self.seen(rule.ordinal))
            .map(|rule| rule.id)
    }
    pub fn registered_counts() -> (usize, usize) {
        (GROUPS.len(), RULES.len() - GROUPS.len())
    }
    /// A narrow scene counts once toward its parent activity, even if the base
    /// identity itself has never been selected. Do not call variants new acts.
    pub fn observed_kinds(&self) -> (usize, usize) {
        let families = GROUPS
            .iter()
            .filter(|g| {
                RULES[g.start..g.start + g.len]
                    .iter()
                    .any(|r| self.seen(r.ordinal))
            })
            .count();
        let variants = RULES
            .iter()
            .filter(|r| r.mode != 0 && self.seen(r.ordinal))
            .count();
        (families, variants)
    }
    pub fn summary(&self) -> String {
        let (bases, variants) = Self::registered_counts();
        let (seen, scenes) = self.observed_kinds();
        let mut s = format!(
            "已接入 {bases} 类行为、{variants} 种特定场景。\r\n本村已见 {seen} 类行为，其中有 {scenes} 种特定场景；共 {} 个不同记录。\r\n",
            self.observed_count()
        );
        for event in self.recent.iter().rev().take(5) {
            let minute = event.now_ms % DAY / 60_000;
            let action = group_of(event.id).map_or(event.label(), |g| g.display);
            s.push_str(&format!(
                "  第 {} 天 {:02}:{:02} · {}：{}\r\n",
                event.now_ms / DAY + 1,
                minute / 60,
                minute % 60,
                event.actor.name(),
                action
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event() -> CompletedBehavior {
        CompletedBehavior {
            base: "S08",
            actor: LegacyActor::Household(0),
            at: TilePos::new(20, 20),
            target: TilePos::new(21, 20),
            parcel: LegacyPlace::Front,
            mode: 1,
            partner: None,
            now_ms: 10 * 3_600_000,
            weather: 2,
            season: 0,
        }
    }
    #[test]
    fn only_actual_context_selects_one_id_and_never_creates_resources() {
        let mut ledger = BehaviorLedger::default();
        assert!(ledger.record(event()));
        assert_eq!(ledger.observed_count(), 1);
        assert!(!ledger.record(event()));
        assert_eq!(ledger.completed, 1);
        let e = CompletedBehavior {
            actor: LegacyActor::Wildlife(9),
            ..event()
        };
        assert!(!ledger.record(e));
        let mut summer = BehaviorLedger::default();
        assert!(summer.record(CompletedBehavior {
            season: 1,
            ..event()
        }));
        assert_eq!(summer.recent()[0].id(), "S08");
        assert!(ledger.validate(event().now_ms));
    }
    #[test]
    fn unknown_bits_future_events_and_missing_real_partner_are_rejected() {
        let mut ledger = BehaviorLedger::default();
        assert!(!ledger.record(CompletedBehavior {
            base: "A08-10",
            actor: LegacyActor::Wildlife(5),
            ..event()
        }));
        assert!(ledger.record(CompletedBehavior {
            base: "A08-10",
            actor: LegacyActor::Wildlife(5),
            partner: Some(LegacyActor::Wildlife(10)),
            ..event()
        }));
        assert!(!ledger.validate(0));
        ledger.seen[2][31] = u64::MAX;
        assert!(!ledger.validate(event().now_ms));
    }
    #[test]
    fn history_is_bounded_and_saved_ordinals_survive_reopening() {
        let mut ledger = BehaviorLedger::default();
        for n in 0..1000 {
            assert!(ledger.record(CompletedBehavior {
                now_ms: event().now_ms + n,
                ..event()
            }));
        }
        assert_eq!(ledger.recent().len(), 1);
        let bytes = serde_json::to_vec(&ledger).unwrap();
        assert!(bytes.len() < 4096);
        let reloaded: BehaviorLedger = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(ledger, reloaded);
        assert!(reloaded.validate(event().now_ms + 1000));
    }
    #[test]
    fn saved_history_rejects_role_forgery_duplicate_families_and_backwards_time() {
        let mut ledger = BehaviorLedger::default();
        assert!(ledger.record(event()));
        let valid = ledger.clone();
        ledger.recent[0].actor = LegacyActor::Wildlife(9);
        assert!(!ledger.validate(event().now_ms));
        ledger = valid.clone();
        ledger.recent.push(ledger.recent[0].clone());
        assert!(!ledger.validate(event().now_ms));
        ledger = valid;
        assert!(ledger.record(CompletedBehavior {
            actor: LegacyActor::Household(1),
            now_ms: event().now_ms + 1,
            ..event()
        }));
        assert!(ledger.validate(event().now_ms + 1));
        ledger.recent.swap(0, 1);
        assert!(!ledger.validate(event().now_ms + 1));
    }
    #[test]
    fn oversized_history_fails_deserialization_before_world_validation() {
        let mut ledger = BehaviorLedger::default();
        assert!(ledger.record(event()));
        let mut value = serde_json::to_value(ledger).unwrap();
        let row = value["recent"][0].clone();
        value["recent"] = serde_json::Value::Array(vec![row; RECENT_LIMIT + 1]);
        let error = serde_json::from_value::<BehaviorLedger>(value)
            .unwrap_err()
            .to_string();
        assert!(error.contains("too many recent"), "{error}");
    }
    #[test]
    fn several_seen_scenes_remain_one_activity_family() {
        let mut ledger = BehaviorLedger::default();
        for mode in 1..=3 {
            assert!(ledger.record(CompletedBehavior {
                mode,
                now_ms: event().now_ms + u64::from(mode),
                ..event()
            }));
        }
        assert_eq!(ledger.observed_kinds().0, 1);
        assert!(ledger.observed_kinds().1 <= ledger.observed_count());
        assert!(ledger.summary().contains("本村已见 1 类行为"));
        assert!(ledger.summary().contains("小禾"));
        assert!(!ledger.summary().contains("S08~"));
    }
}
