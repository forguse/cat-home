//! Bounded stock, equipment, maintenance and notice state for village jobs.
use super::{ActorKind, Cargo, DAY_MS, Fixture, MINUTE, TilePos};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const SERVICE_INPUTS: &str = include_str!("../assets/maps/village-services.tsv");
const FIXTURES: [&str; 24] = [
    "tool_handle",
    "dry_rack",
    "fruit_crate",
    "furniture",
    "fish_basket",
    "tablecloth",
    "orchard_tool",
    "sapling_support",
    "flower_vase",
    "sign_board",
    "road_stone",
    "bridge_board",
    "east_branch",
    "field_tool",
    "work_coat",
    "grainshed_eave",
    "scale",
    "packing_basket",
    "pond_outlet",
    "observation_scope",
    "clay_tray",
    "well_tray",
    "inn_basin",
    "door_latch",
];
const ASSETS: [&str; 23] = [
    "letter",
    "reply",
    "address_card",
    "booking_slip",
    "seed_packet",
    "empty_seed_bag",
    "wood_sample",
    "old_beam",
    "tray",
    "fishing_rod",
    "mending_patch",
    "sapling",
    "flower_stem",
    "sketchboard",
    "marker",
    "tool",
    "sizing_gauge",
    "basket",
    "loom_cloth",
    "reed",
    "observation_scope",
    "ceramic_piece",
    "lost_property",
];
const NOTICES: [&str; 18] = [
    "market_hours",
    "bridge_load",
    "fishing_window",
    "fishing_suspend",
    "pollination",
    "frost",
    "fruit_flower_shift",
    "house_sketch",
    "exhibit",
    "exit_directions",
    "south_loop",
    "lot_survey",
    "pickup_sheet",
    "price_board",
    "water_level",
    "reed_boundary",
    "bird_watch",
    "repair_work",
];

fn find(names: &[&str], name: &str) -> Option<u8> {
    names.iter().position(|&id| id == name).map(|n| n as u8)
}

pub(super) const FROM_CUPBOARD: u8 = 20;
pub(super) const OUTSIDE_CUSTODY: u8 = 21;
pub(super) const PREMISES: u8 = 22;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct Token {
    owner: u8,
    quantity: u8,
    changed: Option<u64>,
    #[serde(default)]
    carrier: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    station: Option<TilePos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pickup: Option<TilePos>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    returning: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq, Default)]
struct Notice {
    value: u16,
    issued: Option<u64>,
    read_by_house: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ServiceLedger {
    wear: [u8; FIXTURES.len()],
    assets: [Token; ASSETS.len()],
    notices: [Notice; NOTICES.len()],
    rapport: [u8; 20],
    last_rain: Option<u64>,
    last_weekly_close: Option<u64>,
    // Retain the field for existing v4 snapshots. More recent observations
    // identify a specific game and committed result.
    plugin_completion: u64,
    #[serde(default)]
    basketball_completion: u64,
    #[serde(default)]
    last_plugin_active: bool,
}

impl Default for ServiceLedger {
    fn default() -> Self {
        let owners = [
            0,
            FROM_CUPBOARD,
            0,
            1,
            1,
            FROM_CUPBOARD,
            2,
            2,
            3,
            4,
            5,
            6,
            7,
            8,
            9,
            10,
            11,
            15,
            14,
            15,
            17,
            18,
            9,
        ];
        Self {
            wear: [
                2, 2, 2, 1, 2, 2, 2, 2, 2, 1, 1, 2, 2, 1, 2, 2, 2, 2, 2, 1, 1, 2, 2, 2,
            ],
            assets: std::array::from_fn(|i| Token {
                owner: owners[i],
                quantity: if matches!(i, 4 | 5 | 12 | 19 | 21) {
                    2
                } else {
                    1
                },
                changed: None,
                carrier: None,
                station: None,
                pickup: None,
                returning: false,
            }),
            notices: [Notice::default(); NOTICES.len()],
            rapport: [0; 20],
            last_rain: None,
            last_weekly_close: None,
            plugin_completion: 0,
            basketball_completion: 0,
            last_plugin_active: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Audience {
    House,
    Person(u8),
    Open,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ServiceOp {
    Mend(u8, Cargo),
    Move(u8, u8, u8),
    Prepare(u8, Cargo),
    Publish(u8, &'static str),
    Talk(Audience),
    Home(super::CommunityAction, Cargo),
    HomeStock(super::CommunityAction, Cargo),
    Survey(u8, &'static str),
}

impl ServiceOp {
    pub(super) fn materials(self) -> Option<(Cargo, bool)> {
        match self {
            Self::Mend(_, cargo) | Self::Prepare(_, cargo) | Self::Home(_, cargo) => {
                Some((cargo, false))
            }
            Self::HomeStock(_, cargo) => Some((cargo, true)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Definition {
    pub id: &'static str,
    pub op: ServiceOp,
    pub description: &'static str,
}

fn item_amount(text: &str) -> Option<Cargo> {
    let (item, amount) = text.split_once('*')?;
    Some(Cargo {
        item: super::parse_item(item)?,
        quantity: amount.parse().ok()?,
    })
}

fn audience(s: &str) -> Option<Audience> {
    match s {
        "house" => Some(Audience::House),
        "open" => Some(Audience::Open),
        _ => s
            .parse::<u8>()
            .ok()
            .filter(|&p| p < 20)
            .map(Audience::Person),
    }
}

fn parse_op(kind: &str, a: &'static str, b: &'static str, c: &'static str) -> Option<ServiceOp> {
    let op = match kind {
        "mend" => ServiceOp::Mend(find(&FIXTURES, a)?, item_amount(b)?),
        "move" => ServiceOp::Move(find(&ASSETS, a)?, b.parse().ok()?, c.parse().ok()?),
        "prepare" => ServiceOp::Prepare(find(&ASSETS, a)?, item_amount(b)?),
        "publish" => ServiceOp::Publish(find(&NOTICES, a)?, b),
        "survey" => ServiceOp::Survey(find(&FIXTURES, a)?, b),
        "talk" => ServiceOp::Talk(audience(a)?),
        "home" | "home_stock" => {
            use super::CommunityAction::*;
            let effect = match a {
                "xiaohe_food" => ShareFood(ActorKind::Xiaohe),
                "ayu_food" => ShareFood(ActorKind::Ayu),
                "xiaohe_water" => ShareWater(ActorKind::Xiaohe),
                "ayu_water" => ShareWater(ActorKind::Ayu),
                "clean_bowl" => CleanCatBowl,
                "fill_bowl" => FillCatBowl,
                "release_fish" => FishRelease,
                "workshop" => Mend(Fixture::WorkshopToolBoard),
                "well" => Mend(Fixture::WellRope),
                _ => return None,
            };
            if kind == "home_stock" {
                ServiceOp::HomeStock(effect, item_amount(b)?)
            } else {
                ServiceOp::Home(effect, item_amount(b)?)
            }
        }
        _ => return None,
    };
    Some(op)
}

pub(super) fn definitions() -> impl Iterator<Item = Definition> {
    static DEFINITIONS: OnceLock<Vec<Definition>> = OnceLock::new();
    DEFINITIONS
        .get_or_init(|| {
            SERVICE_INPUTS
                .lines()
                .skip(1)
                .map(|line| {
                    let mut fields = line.splitn(6, '\t');
                    let id = fields.next().expect("service id");
                    let op = parse_op(
                        fields.next().expect("service type"),
                        fields.next().expect("service parameter"),
                        fields.next().expect("service parameter"),
                        fields.next().expect("service parameter"),
                    )
                    .unwrap_or_else(|| panic!("invalid service definition: {id}"));
                    let description = fields.next().expect("service acceptance result");
                    Definition {
                        id,
                        op,
                        description,
                    }
                })
                .collect()
        })
        .iter()
        .copied()
}

impl ServiceLedger {
    pub fn validate(&self, now: u64) -> bool {
        self.wear.iter().all(|&d| d <= 8)
            && self.assets.iter().all(|t| {
                t.owner <= PREMISES
                    && t.quantity <= 4
                    && t.changed.is_none_or(|time| time <= now)
                    && t.carrier.is_none_or(|person| person < 20)
                    && t.station.is_none_or(|tile| tile.index().is_some())
                    && t.pickup
                        .is_none_or(|tile| tile.index().is_some() && t.carrier.is_some())
                    && (!t.returning || t.carrier.is_some())
            })
            && (0..20).all(|person| {
                self.assets
                    .iter()
                    .filter(|a| a.carrier == Some(person))
                    .count()
                    <= 1
            })
            && self
                .notices
                .iter()
                .all(|n| n.issued.is_none_or(|time| time <= now))
            && self.rapport.iter().all(|&n| n <= 12)
            && self.last_rain.is_none_or(|time| time <= now)
            && self.last_weekly_close.is_none_or(|day| day <= now / DAY_MS)
    }

    pub fn note_conditions(&mut self, now: u64, rain: bool, market_load: bool) {
        if rain {
            self.last_rain = Some(now);
            if now / DAY_MS % 3 == 0 && now % DAY_MS == 12 * 60 * MINUTE {
                for i in [10, 11, 12, 15, 18] {
                    self.wear[i] = (self.wear[i] + 1).min(8);
                }
            }
        }
        if market_load && now % DAY_MS == 780 * MINUTE {
            for i in [2, 9, 16, 17] {
                self.wear[i] = (self.wear[i] + 1).min(8);
            }
        }
    }

    pub fn repair(&mut self, index: u8, repaired: u8) -> bool {
        let Some(wear) = self.wear.get_mut(usize::from(index)) else {
            return false;
        };
        if *wear == 0 {
            return false;
        }
        *wear = wear.saturating_sub(repaired);
        true
    }

    pub fn damaged(&self, index: u8) -> bool {
        self.wear
            .get(usize::from(index))
            .is_some_and(|&wear| wear > 0)
    }
    pub fn wear(&mut self, index: u8) {
        if let Some(n) = self.wear.get_mut(usize::from(index)) {
            *n = (*n + 1).min(8);
        }
    }
    pub fn wearing_status(&self, name: &str) -> Option<u8> {
        self.wear.get(usize::from(find(&FIXTURES, name)?)).copied()
    }
    pub fn observe_fixture(&self, index: u8) -> Option<u16> {
        self.wear.get(usize::from(index)).copied().map(u16::from)
    }

    pub fn move_asset(
        &mut self,
        index: u8,
        from: u8,
        to: u8,
        who: u8,
        at: TilePos,
        now: u64,
    ) -> bool {
        let Some(asset) = self.assets.get_mut(usize::from(index)) else {
            return false;
        };
        if asset.owner != from
            || from == to
            || asset.quantity == 0
            || asset.carrier != Some(who)
            || asset.returning
        {
            return false;
        }
        asset.owner = to;
        asset.changed = Some(now);
        asset.carrier = None;
        asset.station = Some(at);
        asset.pickup = None;
        true
    }

    pub fn holds(&self, index: u8, owner: u8) -> bool {
        self.assets
            .get(usize::from(index))
            .is_some_and(|a| a.owner == owner && a.quantity > 0 && a.carrier.is_none())
    }
    pub fn stored_at(&self, asset: u8) -> Option<TilePos> {
        self.assets.get(usize::from(asset))?.station
    }

    pub fn assign_carrier(&mut self, asset: u8, owner: u8, carrier: u8, at: TilePos) -> bool {
        if !self.holds(asset, owner) || self.carried_asset(carrier).is_some() {
            return false;
        }
        let token = &mut self.assets[usize::from(asset)];
        if token.station.is_some_and(|station| station != at) {
            return false;
        }
        token.carrier = Some(carrier);
        token.pickup = Some(at);
        true
    }
    pub fn carried_asset(&self, who: u8) -> Option<&'static str> {
        self.assets
            .iter()
            .position(|token| token.carrier == Some(who) && token.quantity != 0)
            .map(|index| ASSETS[index])
    }

    pub fn carriers(&self) -> impl Iterator<Item = (u8, bool)> + '_ {
        self.assets
            .iter()
            .filter_map(|asset| asset.carrier.map(|person| (person, asset.returning)))
    }
    pub fn carried_by(&self, index: u8, who: u8) -> bool {
        self.assets
            .get(usize::from(index))
            .is_some_and(|token| token.carrier == Some(who) && !token.returning)
    }
    pub fn request_return(&mut self, who: u8) {
        if let Some(token) = self
            .assets
            .iter_mut()
            .find(|token| token.carrier == Some(who))
        {
            token.returning = true;
        }
    }

    pub fn returning_to(&self, who: u8) -> Option<(u8, u8, Option<TilePos>)> {
        let (id, token) = self
            .assets
            .iter()
            .enumerate()
            .find(|(_, token)| token.carrier == Some(who) && token.returning)?;
        Some((id as u8, token.owner, token.pickup.or(token.station)))
    }

    pub fn return_asset(&mut self, asset: u8, who: u8, at: TilePos) -> bool {
        let token = &mut self.assets[usize::from(asset)];
        if token.carrier != Some(who)
            || !token.returning
            || token
                .pickup
                .or(token.station)
                .is_some_and(|original| original != at)
        {
            return false;
        }
        token.carrier = None;
        token.returning = false;
        token.station = Some(at);
        token.pickup = None;
        true
    }

    pub fn room_for(&self, index: u8, owner: u8) -> bool {
        self.assets
            .get(usize::from(index))
            .is_some_and(|a| a.quantity < 4 && (a.owner == owner || a.quantity == 0))
    }

    pub fn make_asset(&mut self, index: u8, to: u8, at: TilePos, now: u64) -> bool {
        let Some(asset) = self.assets.get_mut(usize::from(index)) else {
            return false;
        };
        if asset.owner != to && asset.quantity > 0
            || asset.quantity >= 4
            || asset.carrier.is_some()
            || asset.station.is_some_and(|station| station != at)
        {
            return false;
        }
        asset.owner = to;
        asset.quantity += 1;
        asset.changed = Some(now);
        asset.station = Some(at);
        true
    }

    pub fn has_asset(&self, name: &str, owner: u8) -> bool {
        find(&ASSETS, name).is_some_and(|index| {
            self.assets[usize::from(index)].owner == owner
                && self.assets[usize::from(index)].quantity > 0
        })
    }

    pub fn publish(&mut self, index: u8, value: u16, now: u64) -> bool {
        let Some(n) = self.notices.get_mut(usize::from(index)) else {
            return false;
        };
        if n.issued.is_some() && n.value == value {
            return false;
        }
        *n = Notice {
            value,
            issued: Some(now),
            read_by_house: false,
        };
        true
    }

    pub fn new_notice(&self, index: u8, value: u16) -> bool {
        self.notices
            .get(usize::from(index))
            .is_some_and(|n| n.issued.is_none() || n.value != value)
    }

    pub fn notice(&self, name: &str) -> Option<(u16, u64)> {
        let n = &self.notices[usize::from(find(&NOTICES, name)?)];
        Some((n.value, n.issued?))
    }

    pub fn acknowledge(&mut self, name: &str) -> bool {
        let Some(index) = find(&NOTICES, name) else {
            return false;
        };
        let n = &mut self.notices[usize::from(index)];
        if n.issued.is_none() || n.read_by_house {
            return false;
        }
        n.read_by_house = true;
        true
    }

    pub fn build_rapport(&mut self, resident: usize) -> bool {
        let Some(n) = self.rapport.get_mut(resident) else {
            return false;
        };
        if *n >= 12 {
            return false;
        }
        *n += 1;
        true
    }

    pub fn rapport(&self, resident: usize) -> u8 {
        self.rapport[resident]
    }

    pub fn storm_recent(&self, now: u64) -> bool {
        self.last_rain
            .is_some_and(|t| now.saturating_sub(t) <= 4 * 60 * MINUTE)
    }

    pub fn close_weekly(&mut self, day: u64) -> bool {
        if self.last_weekly_close == Some(day) {
            return false;
        }
        self.last_weekly_close = Some(day);
        true
    }

    pub fn observe_basketball(&mut self, completed: u64, active: bool) -> bool {
        let changed = completed > self.basketball_completion;
        self.basketball_completion = self.basketball_completion.max(completed);
        self.last_plugin_active = active;
        changed
    }

    pub fn plugin_active(&self) -> bool {
        self.last_plugin_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_token_can_only_be_carried_once_and_then_reassigned() {
        let mut book = ServiceLedger::default();
        let letter = find(&ASSETS, "letter").unwrap();
        let start = TilePos::new(1, 1);
        let destination = TilePos::new(2, 1);
        assert!(!book.move_asset(letter, 0, FROM_CUPBOARD, 0, destination, 1));
        assert!(book.assign_carrier(letter, 0, 0, start));
        assert!(!book.assign_carrier(letter, 0, 1, start));
        assert!(book.carried_by(letter, 0));
        assert!(!book.make_asset(letter, 0, start, 2));
        assert!(!book.move_asset(letter, 0, FROM_CUPBOARD, 1, destination, 3));
        assert!(book.move_asset(letter, 0, FROM_CUPBOARD, 0, destination, 3));
        assert!(!book.carried_by(letter, 0));
        assert!(!book.assign_carrier(letter, FROM_CUPBOARD, 0, start));
        assert!(book.assign_carrier(letter, FROM_CUPBOARD, 0, destination));
        book.request_return(0);
        assert_eq!(book.carried_asset(0), Some("letter"));
        assert!(!book.return_asset(letter, 0, start));
        let mut book: ServiceLedger =
            serde_json::from_str(&serde_json::to_string(&book).unwrap()).unwrap();
        assert_eq!(
            book.returning_to(0),
            Some((letter, FROM_CUPBOARD, Some(destination)))
        );
        assert!(book.return_asset(letter, 0, destination));
        assert!(book.holds(letter, FROM_CUPBOARD));
    }

    #[test]
    fn identical_notice_does_not_replace_its_proof_or_issue_time() {
        let mut book = ServiceLedger::default();
        let board = find(&NOTICES, "bridge_load").unwrap();
        assert!(book.publish(board, 1, 3));
        assert!(book.acknowledge("bridge_load"));
        assert!(!book.publish(board, 1, 7));
        assert!(!book.acknowledge("bridge_load"));
        assert_eq!(book.notice("bridge_load"), Some((1, 3)));
        assert!(book.publish(board, 2, 9));
        assert!(book.acknowledge("bridge_load"));
        assert_eq!(book.notice("bridge_load"), Some((2, 9)));
    }
}
