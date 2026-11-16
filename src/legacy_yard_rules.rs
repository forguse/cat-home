use super::{Actor, DAY_MS, Fixture, HOUR, Item, MINUTE, StepMark, Tool, scenic_resources};
use crate::map_data::{TilePos, WILLOW_VILLAGE};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum CastResult {
    Empty,
    FishInHand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmptyMealSource {
    pub tile: TilePos,
    pub until: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<StepMark>,
}

pub(super) const EMPTY_MEAL_WINDOW: u64 = 4 * HOUR;
pub(super) const NEW_FIXTURE_WINDOW: u64 = 3 * DAY_MS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FixtureVersion {
    pub fixture: Fixture,
    pub installed_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum QuietVisit {
    Pause(TilePos),
    Inspect { version: FixtureVersion, stand: TilePos },
}

impl QuietVisit {
    pub fn tile(self) -> TilePos {
        match self {
            Self::Pause(tile) | Self::Inspect { stand: tile, .. } => tile,
        }
    }

    pub fn duration(self) -> u64 {
        match self {
            Self::Pause(_) => 4 * MINUTE,
            Self::Inspect { .. } => 2 * MINUTE,
        }
    }
}

/// Unfinished physical work, with one entry per household actor or fixture.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HouseholdRules {
    pub muddy_hoe: [bool; 2],
    pub cast: [Option<CastResult>; 2],
    pub gathered_scenery: [Option<u16>; 2],
    #[serde(default)]
    pub gathered_from: [Option<TilePos>; 2],
    pub empty_meal_source: [Option<EmptyMealSource>; 2],
    pub well_needs_trial: bool,
    #[serde(default)]
    pub last_stop: [Option<TilePos>; 2],
    #[serde(default)]
    pub quiet_visit_day: [Option<u64>; 2],
    #[serde(default)]
    pub new_fixture: Option<FixtureVersion>,
    #[serde(default)]
    pub seen_fixture: [Option<FixtureVersion>; 2],
}

impl HouseholdRules {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub fn validate(&self, now: u64, actors: &[Actor; 3], held_fibre: [u8; 2]) -> bool {
        self.new_fixture.is_none_or(|version| version.installed_at <= now) && (0..2).all(|i| {
            let actor = &actors[i];
            (!self.muddy_hoe[i] || actor.tool == Some(Tool::Hoe))
                && match self.cast[i] {
                    Some(CastResult::Empty) => actor.tool == Some(Tool::FishingRod),
                    Some(CastResult::FishInHand) => actor.carried == Some(Item::Fish) && actor.amount == 1,
                    None => true,
                }
                && self.gathered_scenery[i].is_none_or(|id| {
                    let ordinal = usize::from(id);
                    scenic_resources::recipe(ordinal).is_some_and(|recipe| {
                        !WILLOW_VILLAGE.scenic[ordinal].temporary_ground
                            && match recipe.product {
                                scenic_resources::Product::Stock(item) => {
                                    actor.carried == Some(item) && actor.amount == u16::from(recipe.amount)
                                }
                                scenic_resources::Product::Fibre => held_fibre[i] == recipe.amount,
                            }
                    })
                })
                && self.empty_meal_source[i].is_none_or(|source| {
                    source.tile.index().is_some() && source.until <= now.saturating_add(EMPTY_MEAL_WINDOW)
                        && source.track.is_none_or(|step| step.valid()
                            && step.from.x.abs_diff(source.tile.x) + step.from.y.abs_diff(source.tile.y) <= 3)
                })
                && self.gathered_from[i].is_none_or(|foot| {
                    foot.index().is_some() && self.gathered_scenery[i].is_some_and(|id| {
                        WILLOW_VILLAGE.scenic.get(usize::from(id)).is_some_and(|source| scenic_resources::distance(foot, source.tile) == 1)
                    })
                })
                && self.last_stop[i].is_none_or(|tile| tile.index().is_some())
                && self.quiet_visit_day[i].is_none_or(|day| day <= now / DAY_MS)
                && self.seen_fixture[i].is_none_or(|version| version.installed_at <= now)
        })
    }
}
