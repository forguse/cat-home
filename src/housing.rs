//! Finite construction lots and the household's authoritative home assignment.

use serde::{Deserialize, Serialize};

use super::super::land_reuse::RECLAIMED_HOME_ORIGIN;
use super::{DAY, Material};

const LEGACY_LOTS: [[u8; 2]; 3] = [[16, 5], [38, 6], [4, 6]];
pub(crate) const LOTS: [[u8; 2]; 3] = [[1, 5], [41, 4], [25, 2]];
use super::super::village::COMPATIBLE_HOME_ORIGINS;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HousePlan {
    Single,
    Shared,
}

impl HousePlan {
    pub fn footprint(self) -> [u8; 2] {
        match self {
            Self::Single => [4, 3],
            Self::Shared => [6, 4],
        }
    }

    pub fn beds(self) -> u8 {
        match self {
            Self::Single => 1,
            Self::Shared => 2,
        }
    }

    pub fn asset_name(self) -> &'static str {
        match self {
            Self::Single => "housing_single",
            Self::Shared => "housing_shared",
        }
    }

    /// Maximum rendered offsets from the exterior threshold in world pixels.
    /// Includes all construction sprites and the low contact shadow.
    pub(crate) fn sight_extents(self) -> [i32; 4] {
        match self {
            Self::Single => [17, 34, 20, 4],
            Self::Shared => [26, 43, 27, 4],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HousingPhase {
    Clear,
    Foundation,
    Frame,
    Roof,
    FitOut,
    MoveIn,
}

impl HousingPhase {
    pub fn asset_name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Foundation => "foundation",
            Self::Frame => "frame",
            Self::Roof => "roof",
            Self::FitOut => "fit_out",
            Self::MoveIn => "move_in",
        }
    }

    pub(crate) fn next(self) -> Option<Self> {
        Some(match self {
            Self::Clear => Self::Foundation,
            Self::Foundation => Self::Frame,
            Self::Frame => Self::Roof,
            Self::Roof => Self::FitOut,
            Self::FitOut => Self::MoveIn,
            Self::MoveIn => return None,
        })
    }

    pub(crate) fn minutes(self, plan: HousePlan) -> u64 {
        let single = match self {
            Self::Clear => 22,
            Self::Foundation => 36,
            Self::Frame => 45,
            Self::Roof => 38,
            Self::FitOut => 30,
            Self::MoveIn => 18,
        };
        single + if plan == HousePlan::Shared { 8 } else { 0 }
    }

    pub(crate) fn material(self, plan: HousePlan) -> Option<(Material, u16)> {
        let shared = u16::from(plan == HousePlan::Shared);
        Some(match self {
            Self::Clear | Self::MoveIn => return None,
            Self::Foundation => (Material::Stone, 3 + 2 * shared),
            Self::Frame => (Material::Wood, 4 + 2 * shared),
            Self::Roof => (Material::Wood, 2 + 2 * shared),
            Self::FitOut => (Material::Rope, 2 + shared),
        })
    }

    fn pause_days(self, plan: HousePlan) -> u64 {
        let shared = u64::from(plan == HousePlan::Shared);
        match self {
            Self::Clear => 2 + shared,
            Self::Foundation => 3 + shared,
            Self::Frame => 2 + shared,
            Self::Roof => 2 + shared,
            Self::FitOut => 1 + shared,
            Self::MoveIn => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormerHomeUse {
    GrainStorage,
    ToolRoom,
}

impl FormerHomeUse {
    pub fn asset_name(self) -> &'static str {
        match self {
            Self::GrainStorage => "housing_old_home_grain_storage",
            Self::ToolRoom => "housing_old_home_tool_room",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Order {
    pub(crate) plan: HousePlan,
    pub(crate) lot: u8,
    pub(crate) phase: HousingPhase,
    /// Older saves and in-flight tasks stored a lot slot without its coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<[u8; 2]>,
    /// Commission the reclaimed building as agricultural storage.
    #[serde(default, skip_serializing_if = "is_false")]
    farm_store: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl Order {
    pub(crate) fn new(plan: HousePlan, lot: u8) -> Self {
        Self {
            plan,
            lot,
            phase: HousingPhase::Clear,
            origin: Some(LOTS[usize::from(lot)]),
            farm_store: false,
        }
    }

    pub(crate) fn lot_origin(self) -> [u8; 2] {
        self.origin
            .unwrap_or_else(|| LEGACY_LOTS[usize::from(self.lot)])
    }

    pub(crate) fn door(self) -> [u8; 2] {
        let [x, y] = self.lot_origin();
        let [width, height] = self.plan.footprint();
        [x + width / 2, y + height]
    }

    pub(crate) fn site(self) -> [u8; 2] {
        self.door()
    }

    pub(crate) fn inside(self, x: i32, y: i32) -> bool {
        let [left, top] = self.lot_origin().map(i32::from);
        let [width, height] = self.plan.footprint().map(i32::from);
        (left..left + width).contains(&x) && (top..top + height).contains(&y)
    }

    pub(crate) fn valid(self) -> bool {
        let lot = usize::from(self.lot);
        !self.farm_store
            && lot < LOTS.len()
            && self
                .origin
                .is_none_or(|origin| origin == LOTS[lot] || origin == LEGACY_LOTS[lot])
    }

    pub(crate) fn valid_for(self, village: bool) -> bool {
        if !village {
            return self.valid();
        }
        usize::from(self.lot) < LOTS.len()
            && self.origin.is_some_and(|origin| {
                if origin == RECLAIMED_HOME_ORIGIN {
                    self.plan == HousePlan::Single
                } else {
                    !self.farm_store && COMPATIBLE_HOME_ORIGINS.contains(&origin)
                }
            })
    }

    pub(crate) fn is_reclaimed(self) -> bool {
        self.origin == Some(RECLAIMED_HOME_ORIGIN)
    }

    pub(crate) fn is_farm_store(self) -> bool {
        self.farm_store
    }

    pub(crate) fn with_village_origin(mut self, origin: [u8; 2]) -> Self {
        self.origin = Some(origin);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_original_legacy_origin(mut self) -> Self {
        self.origin = None;
        self
    }

    pub(crate) fn is_new_site(self) -> bool {
        self.origin
            .is_some_and(|origin| origin == LOTS[usize::from(self.lot)])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct House {
    order: Order,
    next_phase: u64,
    moved_in: u8,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HousingState {
    houses: [Option<House>; LOTS.len()],
    active: Option<u8>,
    // None is the original dwelling. A new house is assigned only by MoveIn.
    homes: [Option<u8>; 2],
    last_complete: u64,
}

impl HousingState {
    pub(crate) fn active_order(&self) -> Option<Order> {
        self.houses[usize::from(self.active?)].map(|house| house.order)
    }

    #[cfg(test)]
    pub(crate) fn stage_render_fixture(&mut self, plan: HousePlan, stage: HousingPhase, now: u64) {
        let shared = plan == HousePlan::Shared;
        let mut order = Order::new(plan, u8::from(shared))
            .with_village_origin(COMPATIBLE_HOME_ORIGINS[if shared { 0 } else { 2 }]);
        order.phase = stage.next().unwrap_or(stage);
        let finished = stage == HousingPhase::MoveIn;
        let occupied = if finished {
            if shared { 0b11 } else { 0b01 }
        } else {
            0
        };
        self.houses[usize::from(order.lot)] = Some(House {
            order,
            next_phase: now,
            moved_in: occupied,
        });
        self.active = (!finished).then_some(order.lot);
        if finished {
            self.homes[0] = Some(order.lot);
            self.homes[1] = shared.then_some(order.lot);
        }
    }

    pub(crate) fn next_order(&self, now: u64) -> Option<Order> {
        let house = self.houses[usize::from(self.active?)]?;
        (house.next_phase <= now).then_some(house.order)
    }

    #[cfg(test)]
    pub(crate) fn proposals(&self, now: u64) -> impl Iterator<Item = Order> + '_ {
        self.proposals_for(now, false)
    }

    pub(crate) fn proposals_for(
        &self,
        now: u64,
        village: bool,
    ) -> impl Iterator<Item = Order> + '_ {
        let day = now / DAY;
        let shared = self.homes[0].is_some();
        let both_housed = self.homes.iter().all(Option::is_some);
        let allowed = self.active.is_none()
            && if both_housed {
                village
            } else if shared {
                self.homes[1].is_none()
                    && day >= 230
                    && now.saturating_sub(self.last_complete) >= 50 * DAY
            } else {
                day >= 100
            };
        let plan = if shared && !both_housed {
            HousePlan::Shared
        } else {
            HousePlan::Single
        };
        let reclaimed_used = self.orders().any(Order::is_reclaimed);
        LOTS.iter().enumerate().flat_map(move |(lot, _)| {
            if !allowed || self.houses[lot].is_some() {
                return [None, None].into_iter().flatten();
            }
            let order = Order::new(plan, lot as u8);
            if !village {
                return [Some(order), None].into_iter().flatten();
            }
            let reclaimed = (!reclaimed_used && plan == HousePlan::Single).then(|| Order {
                origin: Some(RECLAIMED_HOME_ORIGIN),
                farm_store: both_housed,
                ..order
            });
            let origin = COMPATIBLE_HOME_ORIGINS[if plan == HousePlan::Shared { 0 } else { 2 }];
            let usual = (!both_housed && !self.orders().any(|house| house.lot_origin() == origin))
                .then(|| order.with_village_origin(origin));
            [reclaimed, usual].into_iter().flatten()
        })
    }

    pub(crate) fn reclaimed_project_candidate(&self, now: u64) -> Option<Order> {
        self.proposals_for(now, true)
            .find(|order| order.is_reclaimed())
    }

    /// An empty reclaimed dwelling can serve the farm after the household
    /// moves to its completed shared home.
    pub(crate) fn farm_store_door(&self) -> Option<[u8; 2]> {
        if self.homes.iter().any(Option::is_none) {
            return None;
        }
        self.houses.iter().flatten().find_map(|house| {
            let order = house.order;
            (order.is_reclaimed()
                && order.phase == HousingPhase::MoveIn
                && self.active != Some(order.lot)
                && house.moved_in == 0)
                .then(|| order.door())
        })
    }

    #[cfg(test)]
    pub(crate) fn accepts(&self, order: Order, now: u64) -> bool {
        self.accepts_for(order, now, false)
    }

    pub(crate) fn accepts_for(&self, order: Order, now: u64, village: bool) -> bool {
        order.valid_for(village)
            && (self.next_order(now) == Some(order)
                || (order.phase == HousingPhase::Clear
                    && self.proposals_for(now, village).any(|proposal| {
                        proposal.plan == order.plan
                            && proposal.lot == order.lot
                            && proposal.farm_store == order.farm_store
                            && (proposal.origin == order.origin || order.origin.is_none())
                    })))
    }

    pub(crate) fn target_occupant(&self, order: Order) -> Option<usize> {
        if order.is_farm_store() {
            // Ayu completes commissioning without taking up residence.
            return (self.active == Some(order.lot)).then_some(1);
        }
        let expected = if order.plan == HousePlan::Single {
            0b01
        } else {
            0b11
        };
        let moved = self
            .houses
            .get(usize::from(order.lot))
            .and_then(|house| *house)
            .map_or(0, |house| house.moved_in);
        (0..2).find(|&person| expected & !moved & (1 << person) != 0)
    }

    pub(crate) fn order_for_person(&self, person: usize) -> Option<Order> {
        self.houses[usize::from(self.homes.get(person).copied().flatten()?)]
            .map(|house| house.order)
    }

    pub(crate) fn housing_site(&self, lot: usize) -> Option<(Order, HousingPhase, u8)> {
        let house = self.houses.get(lot).copied().flatten()?;
        let shown = if self.active == Some(lot as u8) {
            match house.order.phase {
                HousingPhase::Clear | HousingPhase::Foundation => HousingPhase::Clear,
                HousingPhase::Frame => HousingPhase::Foundation,
                HousingPhase::Roof => HousingPhase::Frame,
                HousingPhase::FitOut => HousingPhase::Roof,
                HousingPhase::MoveIn => HousingPhase::FitOut,
            }
        } else {
            HousingPhase::MoveIn
        };
        Some((house.order, shown, house.moved_in))
    }

    pub(crate) fn has_sites(&self) -> bool {
        self.houses.iter().any(Option::is_some)
    }

    pub(crate) fn orders(&self) -> impl Iterator<Item = Order> + '_ {
        self.houses.iter().flatten().map(|house| house.order)
    }

    pub(crate) fn blocks(&self, x: i32, y: i32) -> bool {
        self.houses
            .iter()
            .flatten()
            .any(|house| house.order.phase >= HousingPhase::Frame && house.order.inside(x, y))
    }

    pub(crate) fn commit(&mut self, order: Order, person: usize, now: u64) {
        let village = order.valid_for(true) && !order.valid();
        debug_assert!(self.accepts_for(order, now, village));
        let index = usize::from(order.lot);
        if order.phase == HousingPhase::Clear {
            self.active = Some(order.lot);
            self.houses[index] = Some(House {
                order: Order {
                    phase: HousingPhase::Foundation,
                    ..order
                },
                next_phase: now.saturating_add(order.phase.pause_days(order.plan) * DAY),
                moved_in: 0,
            });
        } else if let Some(phase) = order.phase.next() {
            let house = self.houses[index].as_mut().expect("current project");
            house.order.phase = phase;
            house.next_phase = now.saturating_add(order.phase.pause_days(order.plan) * DAY);
        } else if order.is_farm_store() {
            debug_assert_eq!(self.target_occupant(order), Some(person));
            self.active = None;
            self.last_complete = now;
        } else {
            debug_assert_eq!(self.target_occupant(order), Some(person));
            let house = self.houses[index].as_mut().expect("current project");
            house.moved_in |= 1 << person;
            if self.target_occupant(order).is_none() {
                if order.plan == HousePlan::Shared {
                    for home in self.homes.iter().flatten() {
                        if let Some(previous) = &mut self.houses[usize::from(*home)] {
                            previous.moved_in = 0;
                        }
                    }
                    self.homes = [Some(order.lot); 2];
                } else {
                    self.homes[0] = Some(order.lot);
                }
                self.active = None;
                self.last_complete = now;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn validate(&self, now: u64) -> bool {
        self.validate_for(now, false)
    }

    pub(crate) fn validate_for(&self, now: u64, village: bool) -> bool {
        if self.last_complete > now
            || self.active.is_some_and(|lot| {
                self.houses
                    .get(usize::from(lot))
                    .is_none_or(Option::is_none)
            })
        {
            return false;
        }
        if village {
            let mut origins = [false; COMPATIBLE_HOME_ORIGINS.len() + 1];
            for house in self.houses.iter().flatten() {
                let Some(origin) = house.order.origin else {
                    return false;
                };
                let Some(parcel) = COMPATIBLE_HOME_ORIGINS
                    .iter()
                    .chain(std::iter::once(&RECLAIMED_HOME_ORIGIN))
                    .position(|&candidate| candidate == origin)
                else {
                    return false;
                };
                if origins[parcel] || (parcel == 0) != (house.order.plan == HousePlan::Shared) {
                    return false;
                }
                origins[parcel] = true;
                if house.order.is_farm_store()
                    && (house.moved_in != 0 || self.homes.iter().any(Option::is_none))
                {
                    return false;
                }
            }
        }
        self.houses.iter().enumerate().all(|(index, house)| {
            house.is_none_or(|house| {
                let active = self.active == Some(index as u8);
                usize::from(house.order.lot) == index
                    && house.order.valid_for(village)
                    && house.order.phase != HousingPhase::Clear
                    && house.next_phase <= now.saturating_add(4 * DAY)
                    && (!active || house.moved_in == 0 || house.order.phase == HousingPhase::MoveIn)
                    && (active || house.order.phase == HousingPhase::MoveIn)
                    && house.moved_in
                        & if house.order.plan == HousePlan::Single {
                            !0b01
                        } else {
                            !0b11
                        }
                        == 0
            })
        }) && self.homes.iter().enumerate().all(|(person, home)| {
            home.is_none_or(|lot| {
                self.active != Some(lot)
                    && self
                        .houses
                        .get(usize::from(lot))
                        .and_then(|house| *house)
                        .is_some_and(|house| house.moved_in & (1 << person) != 0)
            })
        })
    }

    pub(crate) fn migrate_origins(&mut self) -> Result<[[u8; 2]; 3], String> {
        let mut used = [false; 3];
        let mut origins = [[0, 0]; 3];
        // Keep a shared house on its designated parcel regardless of old slot index.
        for plan in [HousePlan::Shared, HousePlan::Single] {
            for (index, house) in self.houses.iter_mut().enumerate() {
                let Some(house) = house.as_mut().filter(|house| house.order.plan == plan) else {
                    continue;
                };
                let candidates: &[usize] = if plan == HousePlan::Shared {
                    &[0]
                } else {
                    &[2, 1]
                };
                let parcel = candidates
                    .iter()
                    .copied()
                    .find(|&parcel| !used[parcel])
                    .ok_or_else(|| format!("旧住宅地籍 {} 无无冲突的兼容落点", index + 1))?;
                used[parcel] = true;
                origins[index] = COMPATIBLE_HOME_ORIGINS[parcel];
                house.order = house.order.with_village_origin(origins[index]);
            }
        }
        Ok(origins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(housing: &mut HousingState, order: Order, mut clock: u64) -> u64 {
        assert!(housing.accepts_for(order, clock, true));
        housing.commit(order, 1, clock);
        while housing.active_order().is_some() {
            clock += 4 * DAY;
            let next = housing.next_order(clock).unwrap();
            let worker = if next.phase == HousingPhase::MoveIn {
                housing.target_occupant(next).unwrap()
            } else {
                1
            };
            housing.commit(next, worker, clock);
            assert!(housing.validate_for(clock, true));
        }
        clock
    }

    #[test]
    fn existing_shared_residence_gets_one_store_without_changing_either_home() {
        let mut housing = HousingState::default();
        let initial = housing.reclaimed_project_candidate(100 * DAY).unwrap();
        assert!(!initial.is_farm_store());
        let original_single = housing
            .proposals_for(100 * DAY, true)
            .find(|order| !order.is_reclaimed())
            .unwrap();
        complete(&mut housing, original_single, 100 * DAY);
        let shared = housing.proposals_for(230 * DAY, true).next().unwrap();
        assert_eq!(shared.plan, HousePlan::Shared);
        let complete_shared = complete(&mut housing, shared, 230 * DAY);
        let homes = [housing.order_for_person(0), housing.order_for_person(1)];
        assert_eq!(homes[0], homes[1]);
        assert!(housing.farm_store_door().is_none());

        let now = complete_shared;
        let store = housing.reclaimed_project_candidate(now).unwrap();
        assert!(store.is_farm_store());
        assert_eq!(store.door(), [27, 37]);
        let complete_store = complete(&mut housing, store, now);
        assert_eq!(housing.farm_store_door(), Some(store.door()));
        assert_eq!(
            [housing.order_for_person(0), housing.order_for_person(1)],
            homes
        );
        assert!(
            housing
                .reclaimed_project_candidate(complete_store + 100 * DAY)
                .is_none()
        );
        let snapshot = serde_json::to_vec(&housing).unwrap();
        let reloaded: HousingState = serde_json::from_slice(&snapshot).unwrap();
        assert!(reloaded.validate_for(complete_store, true));
        assert_eq!(reloaded, housing);
    }

    #[test]
    fn reclaimed_first_home_is_used_by_xiaohe_then_reused_by_the_farm() {
        let mut housing = HousingState::default();
        let initial = housing.reclaimed_project_candidate(100 * DAY).unwrap();
        assert_eq!(initial.lot_origin(), RECLAIMED_HOME_ORIGIN);
        assert!(
            !serde_json::to_string(&initial)
                .unwrap()
                .contains("farm_store")
        );
        complete(&mut housing, initial, 100 * DAY);
        assert_eq!(housing.order_for_person(0).unwrap().door(), initial.door());
        assert_eq!(housing.order_for_person(1), None);
        assert!(housing.farm_store_door().is_none());
        let shared = housing.proposals_for(230 * DAY, true).next().unwrap();
        let now = complete(&mut housing, shared, 230 * DAY);
        assert_eq!(housing.farm_store_door(), Some(initial.door()));
        assert!(
            housing
                .reclaimed_project_candidate(now + 100 * DAY)
                .is_none()
        );
    }
}
