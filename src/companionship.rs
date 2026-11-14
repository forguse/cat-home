//! Fixed courtyard keepsakes awarded from the host's accumulated active time.
use crate::map_data::TilePos;

pub const REWARD_COUNT: usize = 9;
pub const ALL_REWARDS: u16 = (1 << REWARD_COUNT) - 1;
const MINUTE: u64 = 60_000;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RewardStyle {
    PawTag,
    LanternRibbon,
    Keepsake,
    Card,
    BenchCushion,
    MemoryRibbon,
    Portrait,
    AnniversaryPlaque,
    CelebrationPennant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reward {
    pub bit: u16,
    pub threshold_ms: u64,
    pub title: &'static str,
    pub description: &'static str,
    pub tile: TilePos,
    pub style: RewardStyle,
}

pub const REWARDS: [Reward; REWARD_COUNT] = [
    Reward {
        bit: 1 << 0,
        threshold_ms: 10 * MINUTE,
        title: "橘子的爪印牌",
        description: "橘子常坐的院篱挂上压了爪印的小木牌。",
        tile: TilePos::new(17, 16),
        style: RewardStyle::PawTag,
    },
    Reward {
        bit: 1 << 1,
        threshold_ms: HOUR,
        title: "第一次亮灯",
        description: "饭桌旁的庭院灯系上第一条暖黄丝带。",
        tile: TilePos::new(20, 17),
        style: RewardStyle::LanternRibbon,
    },
    Reward {
        bit: 1 << 2,
        threshold_ms: DAY,
        title: "拾来的小物",
        description: "橘子拾来的彩石留在饭桌边的旧木箱上。",
        tile: TilePos::new(14, 22),
        style: RewardStyle::Keepsake,
    },
    Reward {
        bit: 1 << 3,
        threshold_ms: 7 * DAY,
        title: "米信的满周贺卡",
        description: "米信寄来的手写贺卡夹在东院栅栏上。",
        tile: TilePos::new(28, 19),
        style: RewardStyle::Card,
    },
    Reward {
        bit: 1 << 4,
        threshold_ms: 30 * DAY,
        title: "院椅坐垫",
        description: "家门东侧的旧长椅添上一只拼布坐垫。",
        tile: TilePos::new(27, 21),
        style: RewardStyle::BenchCushion,
    },
    Reward {
        bit: 1 << 5,
        threshold_ms: 90 * DAY,
        title: "果园纪念树",
        description: "果园步道旁的老树系上绿丝带，成为这段陪伴的纪念树。",
        tile: TilePos::new(32, 69),
        style: RewardStyle::MemoryRibbon,
    },
    Reward {
        bit: 1 << 6,
        threshold_ms: 180 * DAY,
        title: "共餐庭院合影",
        description: "小禾、阿榆与橘子的合影装进木框，挂在共餐庭院。",
        tile: TilePos::new(18, 16),
        style: RewardStyle::Portrait,
    },
    Reward {
        bit: 1 << 7,
        threshold_ms: 365 * DAY,
        title: "柳溪村一周年纪念牌",
        description: "村集市的入口围栏留下一面小小的周年牌。",
        tile: TilePos::new(76, 52),
        style: RewardStyle::AnniversaryPlaque,
    },
    Reward {
        bit: 1 << 8,
        threshold_ms: 730 * DAY,
        title: "灯下两周年彩旗",
        description: "市场树边的街灯挂上两周年彩旗，来年仍然在。",
        tile: TilePos::new(106, 51),
        style: RewardStyle::CelebrationPennant,
    },
];

/// The host alone supplies active time. A world clock or wall timestamp
/// must never feed this function.
pub fn earned(active_ms: u64) -> u16 {
    REWARDS
        .iter()
        .take_while(|reward| reward.threshold_ms <= active_ms)
        .fold(0, |mask, reward| mask | reward.bit)
}

pub fn claimable(received: u16, active_ms: u64) -> u16 {
    earned(active_ms) & !received
}

pub fn latest(received: u16) -> Option<&'static Reward> {
    REWARDS.iter().rfind(|reward| received & reward.bit != 0)
}

pub fn next(active_ms: u64) -> Option<&'static Reward> {
    REWARDS
        .iter()
        .find(|reward| reward.threshold_ms > active_ms)
}

pub fn valid_received(received: u16, active_ms: u64) -> bool {
    received & !earned(active_ms) == 0 && received & received.wrapping_add(1) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_data::{PhaseMask, WILLOW_VILLAGE};

    #[test]
    fn each_real_time_threshold_awards_once_in_order() {
        let mut received = 0;
        for reward in REWARDS {
            assert_eq!(earned(reward.threshold_ms - 1), received);
            assert_eq!(next(reward.threshold_ms - 1), Some(&reward));
            assert_eq!(claimable(received, reward.threshold_ms), reward.bit);
            received |= reward.bit;
            assert_eq!(claimable(received, reward.threshold_ms), 0);
            assert_eq!(latest(received), Some(&reward));
            assert!(valid_received(received, reward.threshold_ms));
            assert!(!valid_received(received, reward.threshold_ms - 1));
        }
        assert_eq!(earned(u64::MAX), ALL_REWARDS);
        assert_eq!(claimable(0, u64::MAX), ALL_REWARDS);
        assert_eq!(next(u64::MAX), None);
        assert!(!valid_received(0b101, u64::MAX));
    }

    #[test]
    fn every_display_is_anchored_to_an_existing_unique_outdoor_prop() {
        for (index, reward) in REWARDS.iter().enumerate() {
            assert!(
                WILLOW_VILLAGE.scenic.iter().any(|object| {
                    object.tile == reward.tile
                        && object.solid
                        && PhaseMask::DAY_ONE.contains(object.phase)
                }),
                "invalid display anchor {reward:?}"
            );
            assert!(
                REWARDS[..index]
                    .iter()
                    .all(|other| other.tile != reward.tile)
            );
        }
    }
}
