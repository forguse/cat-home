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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mark {
    at: u64,
    pub role: Role,
    pub site: u8,
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
};

#[derive(Clone, Debug)]
pub struct Effects {
    pub plots: [PlotCare; SITE_COUNT],
    pub work: [u16; LOTS],
    marks: [Mark; MARK_COUNT],
    seen: [u64; ROLE_COUNT],
    trust: [u8; ROLE_COUNT],
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
            Trail | Signal | Rest | Shelter | Nest | Deliver | Forecast | Art | Lamp | Survey => {}
            Bond => self.trust[role] = self.trust[role].saturating_add(1).min(STOCK_LIMIT),
            MoveObject => increase(&mut self.finds),
            Forage => {
                if self.food > 0 && !matches!(role, 21 | 22 | 23 | 26) {
                    self.food -= 1
                } else {
                    increase(&mut self.finds)
                }
            }
            Tidy => {
                self.marks[Trail as usize] = EMPTY_MARK;
                self.marks[MoveObject as usize] = EMPTY_MARK;
                self.finds = self.finds.saturating_sub(1);
            }
            Water if !is_human => self.water = self.water.saturating_sub(1),
            Water => {
                self.water = 3;
                if matches!(
                    action.target,
                    Target::Garden | Target::Flower | Target::Tree | Target::Crop
                ) && let Some(plot) = plant(can_grow)
                {
                    self.advance(plot.id, 20);
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
                } else if kind == Drain {
                    self.water = 2;
                } else if let Some(index) = project {
                    self.help(index, 6);
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
                        care.growth_minutes = 0;
                        self.seeds = self.seeds.saturating_sub(1);
                        site = plot.id;
                    } else {
                        increase(&mut self.seeds);
                    }
                } else {
                    increase(&mut self.seeds);
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
                    self.advance(plot.id, if kind == Pollinate { 18 } else { 12 });
                    site = plot.id;
                }
                if kind == Pollinate {
                    increase(&mut self.seeds);
                }
            }
            Harvest => {
                if let Some(plot) =
                    plant(|plot| plot.event == Event::None && plot.stage == Stage::Mature)
                {
                    self.advance(plot.id, crate::ecology::remaining_growth_minutes(plot));
                    site = plot.id;
                    increase(&mut self.seeds);
                    increase(&mut self.food);
                }
            }
            Build => {
                if let Some(index) = project {
                    self.help(index, 8);
                    site = 32 + index as u8;
                } else if let Some(plot) = find_plot(garden, action.plot, |plot| {
                    matches!(plot.stage, Stage::Groundwork | Stage::Framing)
                }) {
                    self.advance(plot.id, 8);
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
                    increase(&mut self.seeds);
                }
            }
            Food => increase(&mut self.food),
        }
        self.seen[role] = sequence;
        self.marks[kind as usize] = Mark {
            at,
            role: action.role,
            site,
        };
        true
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
            let _ = write!(out, "{}:{}:{},", mark.at, mark.role.0, mark.site);
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
                    if let Some([at, role, site]) = split_record(encoded)
                        && role < ROLE_COUNT as u64
                        && site <= 255
                    {
                        *slot = Mark {
                            at,
                            role: Role(role as u8),
                            site: site as u8,
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
    *value = value.saturating_add(1).min(STOCK_LIMIT);
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
