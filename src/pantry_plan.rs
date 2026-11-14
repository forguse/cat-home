//! A bounded emergency reserve, with evidence from a full lean-weather week.
use super::*;

const RATION_STOCK: u8 = 6;
const WATER_STOCK: u8 = 3;
const FUEL_STOCK: u8 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    pub rations: u8,
    pub water: u8,
    pub fuel: u8,
    pub nights_covered: u8,
    pub food_days: u8,
    pub last_day: Option<u64>,
    pub lean_days: u8,
    pub ready: bool,
    pub provisioned: bool,
    pub supplied_from_reserve: bool,
    pub surplus_units: u16,
    pub seasonal: u8,
}

impl State {
    pub(super) fn completed(&self, chapter: &str) -> bool {
        match chapter {
            "Q15" => self.ready,
            "Q16" => self.ready && self.surplus_units >= 2,
            "Q23" => self.seasonal == 0b1111,
            _ => false,
        }
    }
    pub(super) fn prepare(&mut self, stock: &mut Stock) {
        if self.rations == 0
            && stock.count(Item::Food) >= 8
            && stock.take(Item::Food, 2)
        {
            self.rations = 2;
        }
        if self.water == 0
            && stock.count(Item::Water) >= 6
            && stock.take(Item::Water, u16::from(WATER_STOCK))
        {
            self.water = WATER_STOCK;
        }
        if self.fuel == 0
            && stock.count(Item::Wood) >= 7
            && stock.take(Item::Wood, u16::from(FUEL_STOCK))
        {
            self.fuel = FUEL_STOCK;
        }
        self.provisioned |= self.rations > 0 && self.water > 0 && self.fuel > 0;
    }
    pub(super) fn day(&mut self, day: u64, lean: bool, ingredients_ok: bool, stock: &mut Stock) {
        if !ingredients_ok || self.last_day == Some(day) {
            return;
        }
        if !self.ready && day >= 28 {
            let supplied = self.food_days == 0b11 && self.nights_covered == 0b11;
            if !lean {
                self.lean_days = 0;
            } else if supplied && self.provisioned {
                self.lean_days = self.lean_days.saturating_add(1).min(7);
            } else {
                self.lean_days = 0;
            }
            self.ready = self.lean_days == 7 && self.supplied_from_reserve;
        }
        self.food_days = 0;
        self.nights_covered = 0;
        self.last_day = Some(day);
        if !lean {
            self.prepare(stock);
        } else if self.provisioned && self.fuel > 0 && (1..3).contains(&self.lean_days) {
            // The insulated pantry can keep the first two cold days stocked
            // with reserve fuel. Other cooking continues from regular inputs.
            self.fuel -= 1;
        }
        if lean {
            self.replenish(stock);
        }
    }
    pub(super) fn replenish(&mut self, stock: &mut Stock) {
        for (item, count, needed) in [
            (Item::Food, &mut self.rations, 6),
            (Item::Water, &mut self.water, 3),
        ] {
            let used = u16::from(*count).min(needed - stock.count(item).min(needed));
            if used > 0 && stock.add(item, used) {
                *count -= used as u8;
                self.supplied_from_reserve = true;
            }
        }
    }
    pub(super) fn meal(&mut self, actor: usize) {
        if actor < 2 {
            self.food_days |= 1 << actor;
        }
    }
    pub(super) fn rested(&mut self, actor: usize) {
        if actor < 2 {
            self.nights_covered |= 1 << actor;
        }
    }
    pub(super) fn sale(&mut self, item: Item, qty: u16, stock: &Stock) {
        if self.ready
            && item == Item::Crops
            && qty > 0
            && stock.count(Item::Food) >= 6
            && stock.count(Item::Seeds) >= 2
            && stock.count(Item::Crops) >= 6
        {
            self.surplus_units = self.surplus_units.saturating_add(qty).min(16);
        }
    }
    pub(super) fn season_event(&mut self, season: YardSeason) {
        self.seasonal |= 1 << (season as u8);
    }

    pub(super) fn needs_season_event(&self, season: YardSeason) -> bool {
        self.seasonal & (1 << season as u8) == 0
    }
    pub(super) fn validate(&self, now: u64) -> bool {
        self.rations <= RATION_STOCK
            && self.water <= WATER_STOCK
            && self.fuel <= FUEL_STOCK
            && self.nights_covered <= 0b11
            && self.food_days <= 0b11
            && self.lean_days <= 7
            && self.last_day.is_none_or(|day| day <= now / DAY_MS)
            && self.seasonal <= 0b1111
            && self.surplus_units <= 16
            && (!self.ready || self.lean_days == 7 && self.provisioned && self.supplied_from_reserve)
            && (self.surplus_units == 0 || self.ready)
    }
}
