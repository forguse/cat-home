//! Fixed-size yard state changed by the results of completed role actions.

use std::fmt::Write as _;

use crate::{
    cast::{Action, EffectKind, ROLE_COUNT, Role, Target},
    ecology::{Event, Garden, Plot, PlotCare, SITE_COUNT, Stage},
    projects::LOTS,
};

const HOURS: u64 = 3_600;
const MARK_COUNT: usize = EffectKind::ALL.len();
const STOCK_LIMIT: u8 = 24;
const NO_SITE: u8 = 255;
const PAIR_SLOTS: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mark {
    at: u64,
    pub role: Role,
    pub site: u8,
    pub variant: u8,
}

impl Mark {
    pub fn age(self) -> u64 {
        self.at.saturating_sub(1)
    }
}

const EMPTY_MARK: Mark = Mark {
    at: 0,
    role: Role(0),
    site: NO_SITE,
    variant: 0,
};

#[derive(Clone, Copy, Debug)]
struct PairMark {
    at: u64,
    left: Role,
    right: Role,
    meetings: u8,
}

const EMPTY_PAIR: PairMark = PairMark {
    at: 0,
    left: Role(0),
    right: Role(0),
    meetings: 0,
};

#[derive(Clone, Debug)]
pub struct Effects {
    pub plots: [PlotCare; SITE_COUNT],
    pub work: [u16; LOTS],
    marks: [Mark; MARK_COUNT],
    seen: [u64; ROLE_COUNT],
    trust: [u8; ROLE_COUNT],
    pairs: [PairMark; PAIR_SLOTS],
    food: u8,
    seeds: u8,
    finds: u8,
    water: u8,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            plots: [PlotCare::default(); SITE_COUNT],
            work: [0; LOTS],
            marks: [EMPTY_MARK; MARK_COUNT],
            seen: [0; ROLE_COUNT],
            trust: [0; ROLE_COUNT],
            pairs: [EMPTY_PAIR; PAIR_SLOTS],
            food: 0,
            seeds: 0,
            finds: 0,
            water: 3,
        }
    }
}

impl Effects {
    pub fn mark(&self, kind: EffectKind) -> Option<Mark> {
        let mark = self.marks[kind as usize];
        (mark.at != 0).then_some(mark)
    }

    pub fn active(&self, kind: EffectKind, age: u64) -> Option<Mark> {
        use EffectKind::*;
        let duration = match kind {
            Art | Deliver | Nest | Shelter => 24 * HOURS,
            Trail | Signal | Rest | Bond => HOURS,
            Water | Drain | Lamp | Tidy => 3 * HOURS,
            _ => 6 * HOURS,
        };
        self.mark(kind).filter(|mark| {
            let duration = duration / 2 * [2, 1, 4, 6][usize::from(mark.variant)];
            age.checked_sub(mark.age())
                .is_some_and(|since| since < duration)
        })
    }

    pub fn has_food(&self) -> bool {
        self.food > 0
    }
    pub fn has_water(&self) -> bool {
        self.water > 0
    }
    pub fn leave_food(&mut self) {
        increase(&mut self.food);
    }
    pub fn refill(&mut self) {
        self.water = 3;
    }

    pub fn accepts(&self, action: Action) -> bool {
        let role = usize::from(action.role.0);
        role < ROLE_COUNT
            && action.effect_kind.allows(action.role)
            && self.seen[role] < action.window.saturating_add(1)
    }

    pub fn apply(
        &mut self,
        action: Action,
        age: u64,
        garden: &Garden,
        project: Option<usize>,
    ) -> bool {
        use EffectKind::*;
        let kind = action.effect_kind;
        let role = usize::from(action.role.0);
        let sequence = action.window.saturating_add(1);
        if !self.accepts(action) {
            return false;
        }
        let at = age.saturating_add(1);
        let variant = action.variant.min(3);
        let amount = [1, 2, 2, 3][usize::from(variant)];
        let focused = variant == 1;
        let joint = variant == 2;
        let sustained = variant == 3;
        let is_human = role < 10;
        let plant_target = |plot: &Plot| {
            let Some(species) = plot.kind.species_index() else {
                return false;
            };
            match action.target {
                Target::Flower => species < 4,
                Target::Tree => (4..8).contains(&species),
                Target::Crop => species >= 8,
                _ => true,
            }
        };
        let plant = |eligible: fn(&Plot) -> bool| {
            find_plot(garden, action.plot, |plot| {
                plant_target(plot) && eligible(plot)
            })
        };
        let mut site = action.plot.unwrap_or(NO_SITE);
        match kind {
            Trail | Signal | Rest | Forecast | Art | Lamp => {}
            Shelter | Nest if joint => increase(&mut self.finds),
            Shelter | Nest => {}
            Deliver => match variant {
                1 => increase(&mut self.seeds),
                2 => add_stock(&mut self.finds, 2),
                3 => add_stock(&mut self.food, 2),
                _ => {}
            },
            Survey => {
                if let Some(index) = project {
                    site = 32 + index as u8;
                } else if sustained && let Some(plot) = plant(can_grow) {
                    site = plot.id;
                }
            }
            Bond => add_stock(&mut self.trust[role], amount),
            MoveObject => {
                add_stock(&mut self.finds, amount);
                if joint && matches!(action.target, Target::Flower | Target::Tree | Target::Crop) {
                    increase(&mut self.seeds);
                }
            }
            Forage => {
                if self.food > 0 && !matches!(role, 21 | 22 | 23 | 26) {
                    self.food = self.food.saturating_sub(if joint { 2 } else { 1 });
                } else {
                    add_stock(&mut self.finds, amount);
                }
                if sustained
                    && matches!(action.target, Target::Flower | Target::Tree | Target::Crop)
                {
                    increase(&mut self.seeds);
                }
            }
            Tidy => {
                self.marks[Trail as usize] = EMPTY_MARK;
                self.marks[MoveObject as usize] = EMPTY_MARK;
                self.finds = self.finds.saturating_sub(amount);
                if sustained {
                    self.marks[Forage as usize] = EMPTY_MARK;
                }
                if joint && self.water > 0 {
                    self.water = 3;
                }
            }
            Water if !is_human => {
                let drinks = if focused { 2 } else { 1 };
                self.water = self.water.saturating_sub(drinks);
            }
            Water => {
                self.water = 3;
                if matches!(
                    action.target,
                    Target::Garden | Target::Flower | Target::Tree | Target::Crop
                ) && let Some(plot) = plant(can_grow)
                {
                    self.advance(plot.id, [20, 28, 32, 48][usize::from(variant)]);
                    site = plot.id;
                }
            }
            Drain | Repair => {
                let damage = find_plot(garden, action.plot, |plot| {
                    if kind == Drain {
                        plot.event == Event::Flood
                    } else {
                        plot.event != Event::None
                    }
                });
                if let Some(plot) = damage {
                    self.plots[usize::from(plot.id)].repaired = at;
                    site = plot.id;
                    if joint {
                        self.water = 3;
                    }
                } else if kind == Drain {
                    self.water = if focused {
                        1
                    } else if sustained {
                        3
                    } else {
                        2
                    };
                } else if let Some(index) = project {
                    self.help(index, [6, 9, 12, 16][usize::from(variant)]);
                    site = 32 + index as u8;
                }
            }
            Sow => {
                if let Some(plot) = plant(|plot| {
                    plot.event == Event::None && matches!(plot.stage, Stage::Seed | Stage::Harvest)
                }) {
                    let care = &mut self.plots[usize::from(plot.id)];
                    if care.sowed == 0 || at.saturating_sub(care.sowed) >= 24 * HOURS {
                        let species = plot.kind.species_index().unwrap_or(0);
                        let species = if is_human {
                            species
                        } else if role == 14 && action.target == Target::Tree {
                            4 + usize::from(action.role.0) % 4
                        } else {
                            usize::from(action.role.0) % 4
                        };
                        care.sowed = at;
                        care.species = species as u8 + 1;
                        care.growth_minutes = [0, 8, 15, 35][usize::from(variant)];
                        self.seeds = self.seeds.saturating_sub(1);
                        if joint {
                            increase(&mut self.seeds);
                        }
                        site = plot.id;
                    } else {
                        add_stock(&mut self.seeds, amount);
                    }
                } else {
                    add_stock(&mut self.seeds, amount);
                }
            }
            TendPlant | Pollinate => {
                if let Some(plot) = plant(if kind == Pollinate {
                    |plot| {
                        plot.event == Event::None
                            && matches!(plot.stage, Stage::Budding | Stage::Mature)
                    }
                } else {
                    can_grow
                }) {
                    let minutes = if kind == Pollinate {
                        [18, 28, 35, 54]
                    } else {
                        [12, 20, 27, 44]
                    };
                    self.advance(plot.id, minutes[usize::from(variant)]);
                    site = plot.id;
                }
                if kind == Pollinate {
                    add_stock(&mut self.seeds, if joint { 2 } else { 1 });
                }
            }
            Harvest => {
                if let Some(plot) =
                    plant(|plot| plot.event == Event::None && plot.stage == Stage::Mature)
                {
                    self.advance(plot.id, crate::ecology::remaining_growth_minutes(plot));
                    site = plot.id;
                    add_stock(&mut self.seeds, if variant != 0 { 2 } else { 1 });
                    add_stock(&mut self.food, if joint || sustained { 2 } else { 1 });
                }
            }
            Build => {
                if let Some(index) = project {
                    self.help(index, [8, 12, 17, 22][usize::from(variant)]);
                    site = 32 + index as u8;
                } else if let Some(plot) = find_plot(garden, action.plot, |plot| {
                    matches!(plot.stage, Stage::Groundwork | Stage::Framing)
                }) {
                    self.advance(plot.id, [8, 12, 17, 22][usize::from(variant)]);
                    site = plot.id;
                }
            }
            SeedTrade => {
                if self.finds > 0 || self.food > 0 {
                    if self.finds > 0 {
                        self.finds -= 1
                    } else {
                        self.food -= 1
                    }
                    add_stock(&mut self.seeds, amount);
                }
            }
            Food => add_stock(&mut self.food, amount),
        }
        if let Some(partner) = action.partner {
            self.remember_pair(action.role, partner, at);
            if joint && usize::from(partner.0) < ROLE_COUNT {
                increase(&mut self.trust[usize::from(partner.0)]);
            }
        }
        self.seen[role] = sequence;
        self.marks[kind as usize] = Mark {
            at,
            role: action.role,
            site,
            variant,
        };
        if joint {
            let secondary = match kind {
                Trail | Drain | Repair | Deliver | Art => Some(Survey),
                Signal => Some(Bond),
                Rest => Some(Shelter),
                Nest => Some(MoveObject),
                Forecast | Lamp | Survey => Some(Signal),
                Shelter => Some(Rest),
                _ => None,
            };
            if let Some(secondary) = secondary {
                self.marks[secondary as usize] = Mark {
                    at,
                    role: action.role,
                    site,
                    variant: 0,
                };
            }
        }
        true
    }

    fn remember_pair(&mut self, first: Role, second: Role, at: u64) {
        if first == second || usize::from(second.0) >= ROLE_COUNT {
            return;
        }
        let (left, right) = if first.0 < second.0 {
            (first, second)
        } else {
            (second, first)
        };
        let index = self
            .pairs
            .iter()
            .position(|pair| pair.at != 0 && pair.left == left && pair.right == right)
            .or_else(|| self.pairs.iter().position(|pair| pair.at == 0))
            .unwrap_or_else(|| {
                self.pairs
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, pair)| pair.at)
                    .unwrap()
                    .0
            });
        let meetings = if self.pairs[index].left == left && self.pairs[index].right == right {
            self.pairs[index].meetings.saturating_add(1)
        } else {
            1
        };
        self.pairs[index] = PairMark {
            at,
            left,
            right,
            meetings,
        };
    }

    fn advance(&mut self, site: u8, minutes: u16) {
        let slot = &mut self.plots[usize::from(site)];
        slot.growth_minutes = slot
            .growth_minutes
            .saturating_add(minutes)
            .min(crate::ecology::MAX_GROWTH_MINUTES);
    }

    fn help(&mut self, lot: usize, minutes: u16) {
        self.work[lot] = self.work[lot]
            .saturating_add(minutes)
            .min(crate::projects::MAX_HELP_MINUTES);
    }

    pub fn append_notebook(&self, text: &mut String) {
        let _ = write!(
            text,
            "\n\n庭院存放  种子 {} · 食物 {} · 拾物 {} · 水位 {}/3",
            self.seeds, self.food, self.finds, self.water
        );
        let mut recent: [_; MARK_COUNT] =
            std::array::from_fn(|index| (self.marks[index], EffectKind::ALL[index]));
        recent.sort_unstable_by_key(|(mark, _)| std::cmp::Reverse(mark.at));
        if recent[0].0.at != 0 {
            text.push_str("\n最近的庭院变化");
        }
        for (mark, kind) in recent.into_iter().filter(|(mark, _)| mark.at != 0).take(5) {
            let _ = write!(text, "\n  {} · {}", mark.role.label(), kind.label());
        }
        if let Some((role, score)) = self
            .trust
            .iter()
            .copied()
            .enumerate()
            .max_by_key(|(_, value)| *value)
            && score != 0
        {
            let _ = write!(
                text,
                "\n来往最多  {} · 熟悉度 {}",
                Role(role as u8).label(),
                score
            );
        }
        if let Some(pair) = self
            .pairs
            .iter()
            .filter(|pair| pair.at != 0)
            .max_by_key(|pair| pair.at)
        {
            let _ = write!(
                text,
                "\n最近结伴  {} 与 {} · {} 次",
                pair.left.label(),
                pair.right.label(),
                pair.meetings
            );
        }
    }

    pub fn encode(&self, out: &mut String) {
        out.push_str("effect_seen=");
        for value in &self.seen {
            let _ = write!(out, "{value},");
        }
        out.push('\n');
        out.push_str("effect_plots=");
        for plot in &self.plots {
            let _ = write!(
                out,
                "{}:{}:{}:{},",
                plot.sowed, plot.species, plot.growth_minutes, plot.repaired
            );
        }
        out.push_str("\neffect_marks=");
        for mark in &self.marks {
            let _ = write!(
                out,
                "{}:{}:{}:{},",
                mark.at, mark.role.0, mark.site, mark.variant
            );
        }
        out.push_str("\neffect_work=");
        for value in &self.work {
            let _ = write!(out, "{value},");
        }
        out.push_str("\neffect_trust=");
        for value in &self.trust {
            let _ = write!(out, "{value},");
        }
        let _ = writeln!(
            out,
            "\neffect_stock={},{},{},{}",
            self.food, self.seeds, self.finds, self.water
        );
        out.push_str("effect_pairs=");
        for pair in &self.pairs {
            let _ = write!(
                out,
                "{}:{}:{}:{},",
                pair.at, pair.left.0, pair.right.0, pair.meetings
            );
        }
        out.push('\n');
    }

    pub fn decode_field(&mut self, key: &str, value: &str) {
        match key {
            "effect_seen" => read_numbers(&mut self.seen, value, u64::MAX),
            "effect_work" => read_numbers(&mut self.work, value, crate::projects::MAX_HELP_MINUTES),
            "effect_trust" => read_numbers(&mut self.trust, value, STOCK_LIMIT),
            "effect_stock" => {
                let mut values = [self.food, self.seeds, self.finds, self.water];
                read_numbers(&mut values, value, STOCK_LIMIT);
                [self.food, self.seeds, self.finds, self.water] = values;
                self.water = self.water.min(3);
            }
            "effect_plots" => {
                for (slot, encoded) in self.plots.iter_mut().zip(value.split(',')) {
                    if let Some([sowed, species, growth, repaired]) = split_record(encoded) {
                        *slot = PlotCare {
                            sowed,
                            species: species.min(12) as u8,
                            growth_minutes: growth
                                .min(u64::from(crate::ecology::MAX_GROWTH_MINUTES))
                                as u16,
                            repaired,
                        };
                    }
                }
            }
            "effect_marks" => {
                for (slot, encoded) in self.marks.iter_mut().zip(value.split(',')) {
                    let parsed = split_record(encoded).or_else(|| {
                        split_record(encoded).map(|[at, role, site]| [at, role, site, 0])
                    });
                    if let Some([at, role, site, variant]) = parsed
                        && role < ROLE_COUNT as u64
                        && site <= 255
                    {
                        *slot = Mark {
                            at,
                            role: Role(role as u8),
                            site: site as u8,
                            variant: variant.min(3) as u8,
                        };
                    }
                }
            }
            "effect_pairs" => {
                for (slot, encoded) in self.pairs.iter_mut().zip(value.split(',')) {
                    if let Some([at, left, right, meetings]) = split_record(encoded)
                        && left < right
                        && right < ROLE_COUNT as u64
                        && meetings > 0
                    {
                        *slot = PairMark {
                            at,
                            left: Role(left as u8),
                            right: Role(right as u8),
                            meetings: meetings.min(255) as u8,
                        };
                    }
                }
            }
            _ => {}
        }
    }
}

fn can_grow(plot: &Plot) -> bool {
    plot.event == Event::None && plot.stage != Stage::Harvest
}

fn increase(value: &mut u8) {
    add_stock(value, 1);
}

fn add_stock(value: &mut u8, amount: u8) {
    *value = value.saturating_add(amount).min(STOCK_LIMIT);
}

fn find_plot(
    garden: &Garden,
    preferred: Option<u8>,
    predicate: impl Fn(&Plot) -> bool,
) -> Option<&Plot> {
    let mut eligible = garden.plots().iter().filter(|plot| predicate(plot));
    eligible
        .clone()
        .find(|plot| Some(plot.id) == preferred)
        .or_else(|| eligible.next())
}

fn split_record<const N: usize>(value: &str) -> Option<[u64; N]> {
    let mut parts = value.split(':');
    let mut result = [0; N];
    for entry in &mut result {
        *entry = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(result)
}

fn read_numbers<T: std::str::FromStr + Ord + Copy>(out: &mut [T], encoded: &str, cap: T) {
    for (slot, value) in out.iter_mut().zip(encoded.split(',')) {
        if let Ok(value) = value.parse::<T>() {
            *slot = value.min(cap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        activities::{Motion, Prop},
        cast::Phase,
        ecology,
        simulation::{Season, Weather},
    };

    #[test]
    fn every_effect_mode_records_a_result_and_preserves_pair_history() {
        let garden = ecology::plots_with_care(
            5_000,
            1,
            272,
            145,
            Season::Spring,
            Weather::Clear,
            &[1; SITE_COUNT],
            &[PlotCare::default(); SITE_COUNT],
        );
        for kind in EffectKind::ALL {
            let role = (0..ROLE_COUNT as u8)
                .map(Role)
                .find(|&role| kind.allows(role))
                .unwrap();
            for variant in 0..4 {
                let partner = Role(if role.0 == 19 { 17 } else { 19 });
                let action = Action {
                    role,
                    id: "test",
                    label: "",
                    detail: "",
                    effect: "",
                    effect_kind: kind,
                    motion: Motion::Still,
                    prop: Prop::None,
                    target: Target::Garden,
                    x: 66,
                    y: 88,
                    phase: Phase::Result,
                    phase_progress: 100,
                    completes_now: false,
                    window: 1,
                    plot: None,
                    variant,
                    partner: Some(partner),
                };
                let mut effects = Effects::default();
                assert!(effects.apply(action, 5_000, &garden, Some(0)));
                assert_eq!(effects.mark(kind).unwrap().variant, variant);
                let mut file = String::new();
                effects.encode(&mut file);
                let mut reloaded = Effects::default();
                for (key, value) in file.lines().filter_map(|line| line.split_once('=')) {
                    reloaded.decode_field(key, value);
                }
                assert_eq!(reloaded.mark(kind), effects.mark(kind));
                assert!(!reloaded.accepts(action));
                assert_eq!(reloaded.pairs[0].meetings, 1);
                assert_eq!(reloaded.pairs[0].at, 5_001);
            }
        }
    }
}
