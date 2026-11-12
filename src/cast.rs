//! The residents and guests run short, deterministic scenes drawn from the role table.

use std::sync::OnceLock;

use crate::activities::{Motion, Prop};
use crate::ecology::{Event, Garden, Kind, Plot, Stage};
use crate::simulation::{CareAction, CatAction, DayPart, Season, Snapshot, Visitor, Weather};

pub const ROLE_COUNT: usize = 30;
const ACTION_SECONDS: u64 = 120;
const GUEST_WINDOW: u64 = 31 * 60;
const RULES: &str = include_str!("../design/cast-rules.tsv");
const RESIDENTS: &str = include_str!("../design/character-residents.txt");
const EXCLUSIVE: [&str; 3] = [
    include_str!("../design/character-humans.tsv"),
    include_str!("../design/character-animals-a.tsv"),
    include_str!("../design/character-animals-b.tsv"),
];
const SHARED: &str = include_str!("../design/character-shared.tsv");
static CATALOG: OnceLock<Catalog> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Role(pub u8);

impl Role {
    pub fn label(self) -> &'static str {
        catalog()
            .names
            .get(usize::from(self.0))
            .copied()
            .unwrap_or("")
    }

    pub fn is_extra(self) -> bool {
        can_visit(self) && !matches!(self.0, 11..=17)
    }

    pub fn from_visitor(guest: Visitor) -> Option<Self> {
        let index = match guest {
            Visitor::None => return None,
            Visitor::GreyCat => 11,
            Visitor::BlackCat => 12,
            Visitor::Sparrow => 13,
            Visitor::Squirrel => 14,
            Visitor::Hedgehog => 15,
            Visitor::Rabbit => 16,
            Visitor::Frog => 17,
        };
        Some(Self(index))
    }
}

pub fn can_visit(role: Role) -> bool {
    matches!(role.0, 1..=9 | 11..=29)
}

pub struct Context<'a> {
    pub state: &'a Snapshot,
    pub garden: &'a Garden,
    pub water: bool,
    pub lamp: bool,
    pub food: bool,
    pub knock: bool,
    pub fish: bool,
    pub guest: Option<Role>,
}

impl<'a> Context<'a> {
    pub fn new(state: &'a Snapshot, garden: &'a Garden) -> Self {
        Self {
            state,
            garden,
            water: true,
            lamp: matches!(state.day_part, DayPart::Dusk | DayPart::Night),
            food: false,
            knock: false,
            fish: false,
            guest: guest(state),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Porch,
    Path,
    Mat,
    Fence,
    Water,
    Sky,
    Mailbox,
    Pantry,
    Lamp,
    Flower,
    Tree,
    Crop,
    Build,
    Damage,
    Garden,
    Pond,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Approach,
    Work,
    Result,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Action {
    pub role: Role,
    pub id: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub effect: &'static str,
    pub motion: Motion,
    pub prop: Prop,
    pub target: Target,
    pub x: i32,
    pub y: i32,
    pub phase: Phase,
    pub phase_progress: u8,
    pub window: u64,
    pub plot: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Gate {
    Any,
    Day,
    Dawn,
    Dusk,
    Night,
    Rain,
    Wet,
    Snow,
    Winter,
    Autumn,
    Spring,
    Expanded,
    Water,
    Lamp,
    Food,
    Knock,
    Fish,
    Treat,
    Eat,
    Sleep,
    Nap,
    Build,
    Finished,
    Frame,
    Harvest,
    Budding,
    Mature,
    Sprout,
    Growing,
    Seed,
    Damage,
    Storm,
    Flood,
    Frost,
    Drought,
    Rest,
}

#[derive(Clone, Copy)]
struct Rule {
    id: &'static str,
    label: &'static str,
    detail: &'static str,
    effect: &'static str,
    target: Target,
    gate: Gate,
    motion: Motion,
    prop: Prop,
    partner: Option<Role>,
    tags: u16,
}

struct Catalog {
    names: [&'static str; ROLE_COUNT],
    tags: [u16; ROLE_COUNT],
    roles: [Vec<Rule>; ROLE_COUNT],
    shared: Vec<Rule>,
}

fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(Catalog::load)
}

impl Catalog {
    fn load() -> Self {
        let mut lines = RULES.lines();
        let mut names = [""; ROLE_COUNT];
        let mut tags = [0; ROLE_COUNT];
        for _ in 0..ROLE_COUNT {
            let [code, name, role_tags] = fields(lines.next().unwrap(), '\t');
            let index = usize::from(parse_role(code.strip_prefix('@').unwrap()).0);
            names[index] = name;
            tags[index] = tag_mask(role_tags.split_whitespace());
        }
        let mut roles: [Vec<Rule>; ROLE_COUNT] = std::array::from_fn(|index| {
            Vec::with_capacity(if matches!(index, 0 | 10) { 60 } else { 10 })
        });
        let mut role = Role(0);
        for text in RESIDENTS.lines().filter(|line| !line.is_empty()) {
            if let Some(code) = text.strip_prefix('@') {
                role = parse_role(code);
            } else {
                let [label, _, detail, effect] = fields(text, '|');
                let rule = Rule::new(lines.next().unwrap(), label, detail, effect, 0);
                assert_eq!(parse_role(&rule.id[..3]), role);
                roles[usize::from(role.0)].push(rule);
            }
        }
        for text in EXCLUSIVE.into_iter().flat_map(str::lines) {
            let [id, role, _, _, label, _, detail, effect] = fields(text, '\t');
            let rule = Rule::new(lines.next().unwrap(), label, detail, effect, 0);
            assert_eq!(rule.id, id);
            roles[usize::from(parse_role(role).0)].push(rule);
        }
        let mut shared = Vec::with_capacity(24);
        for text in SHARED.lines() {
            let [id, eligibility, label, _, detail, effect] = fields(text, '\t');
            let rule = Rule::new(
                lines.next().unwrap(),
                label,
                detail,
                effect,
                tag_mask(eligibility.split('|')),
            );
            assert_eq!(rule.id, id);
            shared.push(rule);
        }
        assert!(lines.next().is_none());
        Self {
            names,
            tags,
            roles,
            shared,
        }
    }
}

fn fields<const N: usize>(line: &str, separator: char) -> [&str; N] {
    let mut parts = line.split(separator);
    let result = std::array::from_fn(|_| parts.next().expect("missing cast table field").trim());
    assert!(parts.next().is_none(), "too many cast table fields: {line}");
    result
}

fn parse_role(code: &str) -> Role {
    let number = code[1..].parse::<u8>().expect("invalid cast role number");
    let role = match code.as_bytes()[0] {
        b'H' => number - 1,
        b'A' => number + 9,
        _ => panic!("invalid cast role {code}"),
    };
    assert!(usize::from(role) < ROLE_COUNT);
    Role(role)
}

fn tag_mask<'a>(tags: impl Iterator<Item = &'a str>) -> u16 {
    tags.fold(0, |mask, tag| {
        mask | match tag {
            "all" => u16::MAX,
            "human" => 1,
            "cat" => 2,
            "visitor" => 4,
            "ground" => 8,
            "plant" => 16,
            "build" => 32,
            "social" => 64,
            "wet" => 128,
            "night" => 256,
            "flight" => 512,
            "small" => 1024,
            _ => panic!("unknown cast trait {tag}"),
        }
    })
}

impl Rule {
    fn new(
        data: &'static str,
        label: &'static str,
        detail: &'static str,
        effect: &'static str,
        tags: u16,
    ) -> Self {
        let [id, gate, target, motion, prop, partner] = fields(data, '\t');
        Self {
            id,
            label,
            detail,
            effect,
            tags,
            partner: (partner != "-").then(|| parse_role(partner)),
            gate: match gate {
                "any" => Gate::Any,
                "day" => Gate::Day,
                "dawn" => Gate::Dawn,
                "dusk" => Gate::Dusk,
                "night" => Gate::Night,
                "rain" => Gate::Rain,
                "wet" => Gate::Wet,
                "snow" => Gate::Snow,
                "winter" => Gate::Winter,
                "autumn" => Gate::Autumn,
                "spring" => Gate::Spring,
                "expanded" => Gate::Expanded,
                "water" => Gate::Water,
                "lamp" => Gate::Lamp,
                "food" => Gate::Food,
                "knock" => Gate::Knock,
                "fish" => Gate::Fish,
                "treat" => Gate::Treat,
                "eat" => Gate::Eat,
                "sleep" => Gate::Sleep,
                "nap" => Gate::Nap,
                "build" => Gate::Build,
                "finished" => Gate::Finished,
                "frame" => Gate::Frame,
                "harvest" => Gate::Harvest,
                "budding" => Gate::Budding,
                "mature" => Gate::Mature,
                "sprout" => Gate::Sprout,
                "growing" => Gate::Growing,
                "seed" => Gate::Seed,
                "damage" => Gate::Damage,
                "storm" => Gate::Storm,
                "flood" => Gate::Flood,
                "frost" => Gate::Frost,
                "drought" => Gate::Drought,
                "rest" => Gate::Rest,
                _ => panic!("invalid cast gate {gate}"),
            },
            target: match target {
                "porch" => Target::Porch,
                "path" => Target::Path,
                "mat" => Target::Mat,
                "fence" => Target::Fence,
                "water" => Target::Water,
                "sky" => Target::Sky,
                "mailbox" => Target::Mailbox,
                "pantry" => Target::Pantry,
                "lamp" => Target::Lamp,
                "flower" => Target::Flower,
                "tree" => Target::Tree,
                "crop" => Target::Crop,
                "build" => Target::Build,
                "damage" => Target::Damage,
                "garden" => Target::Garden,
                "pond" => Target::Pond,
                _ => panic!("invalid cast target {target}"),
            },
            motion: match motion {
                "bob" => Motion::Bob,
                "hop" => Motion::Hop,
                "nap" => Motion::Nap,
                "patrol" => Motion::Patrol,
                "still" => Motion::Still,
                "tilt" => Motion::Tilt,
                _ => panic!("invalid cast motion {motion}"),
            },
            prop: match prop {
                "bell" => Prop::Bell,
                "book" => Prop::Book,
                "butterfly" => Prop::Butterfly,
                "crumb" => Prop::Crumb,
                "feather" => Prop::Feather,
                "flower" => Prop::Flower,
                "food" => Prop::Food,
                "heart" => Prop::Heart,
                "lamp" => Prop::Lamp,
                "leaf" => Prop::Leaf,
                "mist" => Prop::Mist,
                "note" => Prop::Note,
                "paw" => Prop::Paw,
                "puddle" => Prop::Puddle,
                "rain" => Prop::Rain,
                "seed" => Prop::Seed,
                "snow" => Prop::Snow,
                "spark" => Prop::Spark,
                "star" => Prop::Star,
                "stone" => Prop::Stone,
                "tool" => Prop::Tool,
                "water" => Prop::Water,
                "wood" => Prop::Wood,
                _ => panic!("invalid cast prop {prop}"),
            },
        }
    }
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn natural(role: Role, state: &Snapshot, minute: u16) -> bool {
    let day = (450..1_080).contains(&minute);
    let dawn = (300..450).contains(&minute);
    let dusk = (1_080..1_200).contains(&minute);
    let night = !(day || dawn || dusk);
    let dry = matches!(state.weather, Weather::Clear | Weather::Clouds);
    let warm = state.season != Season::Winter;
    match role.0 {
        1 => (day || dawn) && state.weather != Weather::Snow,
        2 => day && dry && state.build_stage < 3,
        3 => dusk || night,
        4 => !dry || state.weather == Weather::Clouds,
        5 => (660..900).contains(&minute),
        6 => (720..1_080).contains(&minute),
        7 => state.weather == Weather::Rain || (day && state.build_stage >= 2),
        8 => (dawn || dusk) && dry,
        9 => day && dry && (state.phase_seconds / 86_400).is_multiple_of(3),
        18 => day && dry && warm,
        19 => day && state.weather != Weather::Snow,
        20 => state.weather == Weather::Rain || (night && state.weather == Weather::Clouds),
        21 | 23 | 26 => day && dry && warm,
        22 => (dusk || night) && warm && dry,
        24 | 29 => day && state.weather != Weather::Snow,
        25 => dawn || (day && !warm),
        27 => (720..1_080).contains(&minute) && state.weather == Weather::Clear && warm,
        28 => dusk || (night && dry),
        _ => false,
    }
}

pub fn guest(state: &Snapshot) -> Option<Role> {
    let slot = state.phase_seconds / GUEST_WINDOW;
    let stamp = mix(state.world_start ^ slot.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let start = 5 * 60 + stamp % (11 * 60);
    let offset = (state.phase_seconds % GUEST_WINDOW).checked_sub(start)?;
    if offset >= 5 * 60 {
        return None;
    }
    let minute = (u64::from(state.minutes_of_day) + 1_440 - offset / 60) % 1_440;
    let roles = (1..ROLE_COUNT as u8)
        .map(Role)
        .filter(|role| role.is_extra() && natural(*role, state, minute as u16));
    let count = roles.clone().count();
    if count == 0 {
        None
    } else {
        roles.into_iter().nth((stamp >> 24) as usize % count)
    }
}

fn plot_matches(plot: &Plot, target: Target, gate: Gate) -> bool {
    let kind = plot.kind.species_index();
    let matching_kind = match target {
        Target::Flower => kind.is_some_and(|n| n < 4),
        Target::Tree => kind.is_some_and(|n| (4..8).contains(&n)),
        Target::Crop => kind.is_some_and(|n| n >= 8),
        Target::Build => kind.is_none(),
        Target::Garden => kind.is_some(),
        Target::Pond => plot.kind == Kind::Pond,
        _ => true,
    };
    matching_kind
        && match gate {
            Gate::Seed => plot.stage == Stage::Seed,
            Gate::Sprout => plot.stage == Stage::Sprout,
            Gate::Growing => plot.stage == Stage::Growing,
            Gate::Budding => plot.stage == Stage::Budding,
            Gate::Mature => plot.stage == Stage::Mature,
            Gate::Harvest => plot.stage == Stage::Harvest,
            Gate::Frame => plot.stage == Stage::Framing,
            Gate::Finished => plot.stage == Stage::Complete,
            Gate::Damage => plot.event != Event::None,
            Gate::Storm => plot.event == Event::Storm,
            Gate::Flood => plot.event == Event::Flood,
            Gate::Frost => plot.event == Event::Frost,
            Gate::Drought => plot.event == Event::Drought,
            _ => target != Target::Damage || plot.event != Event::None,
        }
}

fn condition(rule: &Rule, context: &Context<'_>, seed: u64) -> Option<Option<Plot>> {
    let state = context.state;
    if let Some(partner) = rule.partner {
        let present = match partner.0 {
            0 => state.caretaker != CareAction::Away,
            10 => state.day_part != DayPart::Night || state.cat != CatAction::Sleep,
            _ => {
                Role::from_visitor(state.visitor) == Some(partner) || context.guest == Some(partner)
            }
        };
        if !present {
            return None;
        }
    }
    let applies = match rule.gate {
        Gate::Day => state.day_part == DayPart::Day,
        Gate::Dawn => state.day_part == DayPart::Dawn,
        Gate::Dusk => state.day_part == DayPart::Dusk,
        Gate::Night => state.day_part == DayPart::Night,
        Gate::Rain => state.weather == Weather::Rain,
        Gate::Wet => {
            state.weather == Weather::Rain
                || context
                    .garden
                    .plots()
                    .iter()
                    .any(|p| p.event == Event::Flood)
        }
        Gate::Snow => state.weather == Weather::Snow,
        Gate::Winter => state.season == Season::Winter,
        Gate::Autumn => state.season == Season::Autumn,
        Gate::Spring => state.season == Season::Spring,
        Gate::Expanded => context.garden.plots().len() > 3,
        Gate::Water => context.water,
        Gate::Lamp => context.lamp,
        Gate::Food => context.food,
        Gate::Knock => context.knock,
        Gate::Fish => context.fish,
        Gate::Treat => state.cat == CatAction::Treat,
        Gate::Eat => state.cat == CatAction::Eat || state.caretaker == CareAction::Feeding,
        Gate::Sleep => state.cat == CatAction::Sleep,
        Gate::Nap => state.cat == CatAction::Sleep && (780..815).contains(&state.minutes_of_day),
        Gate::Build => {
            state.caretaker == CareAction::Building
                || context.garden.job().is_some_and(|p| p.working)
        }
        Gate::Rest => state.caretaker == CareAction::Resting,
        _ => true,
    };
    if !applies {
        return None;
    }

    let needs_stage = matches!(
        rule.gate,
        Gate::Harvest
            | Gate::Budding
            | Gate::Mature
            | Gate::Sprout
            | Gate::Growing
            | Gate::Seed
            | Gate::Damage
            | Gate::Storm
            | Gate::Flood
            | Gate::Frost
            | Gate::Drought
            | Gate::Frame
            | Gate::Finished
    );
    let plot_target = matches!(
        rule.target,
        Target::Flower
            | Target::Tree
            | Target::Crop
            | Target::Build
            | Target::Damage
            | Target::Garden
            | Target::Pond
    );
    if !(needs_stage || plot_target) {
        return Some(None);
    }
    let plots = context
        .garden
        .plots()
        .iter()
        .filter(|plot| plot_matches(plot, rule.target, rule.gate));
    let count = plots.clone().count();
    if count != 0 {
        return Some(plots.into_iter().nth(seed as usize % count).copied());
    }
    if needs_stage && !(rule.gate == Gate::Finished && state.build_stage != 0) {
        return None;
    }
    Some(None)
}

fn anchor(target: Target) -> (i32, i32) {
    match target {
        Target::Porch => (48, 75),
        Target::Path => (70, 84),
        Target::Mat => (111, 79),
        Target::Fence => (135, 75),
        Target::Water => (89, 82),
        Target::Sky => (122, 61),
        Target::Mailbox => (39, 73),
        Target::Pantry => (91, 83),
        Target::Lamp => (42, 69),
        Target::Flower => (126, 83),
        Target::Tree => (131, 72),
        Target::Crop => (100, 86),
        Target::Build => (118, 64),
        Target::Damage => (115, 82),
        Target::Garden => (78, 86),
        Target::Pond => (135, 83),
    }
}

fn choose<'a>(
    rows: &'a [Rule],
    context: &Context<'_>,
    seed: u64,
    traits: u16,
    keep: impl Fn(&Rule) -> bool,
) -> Option<(&'a Rule, Option<Plot>)> {
    if rows.is_empty() {
        return None;
    }
    let begin = seed as usize % rows.len();
    rows[begin..]
        .iter()
        .chain(&rows[..begin])
        .filter(|rule| keep(rule) && (rule.tags == 0 || rule.tags & traits != 0))
        .find_map(|rule| condition(rule, context, seed).map(|plot| (rule, plot)))
}

pub fn action(role: Role, context: &Context<'_>) -> Option<Action> {
    let catalog = catalog();
    let index = usize::from(role.0);
    let rows = catalog.roles.get(index)?;
    let traits = catalog.tags[index];
    let age = context
        .state
        .phase_seconds
        .saturating_add(u64::from(role.0) * 11);
    let window = age / ACTION_SECONDS;
    let seconds = age % ACTION_SECONDS;
    let seed = mix(window ^ context.state.world_start ^ (u64::from(role.0) * 137));
    let resident_sleep = role.0 == 10 && context.state.cat == CatAction::Sleep;
    let resident_eat = role.0 == 10 && context.state.cat == CatAction::Eat;
    let allowed = |rule: &Rule| {
        if resident_sleep {
            return matches!(rule.gate, Gate::Sleep | Gate::Nap);
        }
        if resident_eat {
            return rule.gate == Gate::Eat;
        }
        true
    };
    let override_rule = |rule: &Rule| {
        (context.knock && rule.gate == Gate::Knock)
            || (context.fish && rule.gate == Gate::Fish)
            || (context.food && rule.gate == Gate::Food)
            || (role.0 == 10 && context.state.cat == CatAction::Treat && rule.gate == Gate::Treat)
    };
    let forced = choose(rows, context, seed, traits, |rule| {
        allowed(rule) && override_rule(rule)
    });
    // The original residents and first seven visitors still show their own
    // animation schedules. Added guests always use the cast table.
    if forced.is_none() && !role.is_extra() && window % 4 == 0 {
        return None;
    }
    let (rule, plot) = forced.or_else(|| {
        if window % 6 == 5 && !resident_sleep && !resident_eat {
            choose(&catalog.shared, context, seed, traits, allowed)
                .or_else(|| choose(rows, context, seed, traits, allowed))
        } else {
            choose(rows, context, seed, traits, allowed)
        }
    })?;
    let flying = traits & 512 != 0;
    let (tx, ty) = if let Some(plot) = plot {
        (plot.x as i32, plot.y as i32 + if flying { -3 } else { 5 })
    } else {
        anchor(rule.target)
    };
    let variant = rule.id.as_bytes().last().copied().unwrap_or(b'0') as i32 - i32::from(b'0');
    let (tx, ty) = (
        (tx + variant % 5 - 2).max(10),
        (ty + variant % 3 - 1).max(25),
    );
    let (phase, phase_progress) = match seconds {
        _ if forced.is_some() && (context.knock || context.state.cat == CatAction::Treat) => {
            (Phase::Work, 50)
        }
        0..30 => (Phase::Approach, seconds * 100 / 30),
        30..96 => (Phase::Work, (seconds - 30) * 100 / 66),
        _ => (Phase::Result, (seconds - 96) * 100 / 24),
    };
    let (motion, prop) = match phase {
        Phase::Approach if !resident_sleep => (
            if flying { Motion::Hop } else { Motion::Patrol },
            Prop::None,
        ),
        Phase::Result => (
            if resident_sleep {
                Motion::Nap
            } else {
                Motion::Still
            },
            rule.prop,
        ),
        _ => (rule.motion, rule.prop),
    };
    Some(Action {
        role,
        id: rule.id,
        label: rule.label,
        detail: rule.detail,
        effect: rule.effect,
        motion,
        prop,
        target: rule.target,
        x: tx,
        y: ty,
        phase,
        phase_progress: phase_progress as u8,
        window,
        plot: plot.map(|p| p.id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecology::plots;
    use crate::simulation::{WeatherMode, snapshot};
    use std::collections::BTreeSet;

    const DAY: u64 = 86_400;

    #[test]
    fn cast_table_retains_all_authored_ids_and_role_counts() {
        let catalog = catalog();
        let mut ids = BTreeSet::new();
        for (index, roles) in catalog.roles.iter().enumerate() {
            assert!(!Role(index as u8).label().is_empty());
            assert_eq!(roles.len(), if matches!(index, 0 | 10) { 60 } else { 10 });
            for rule in roles {
                assert!(ids.insert(rule.id), "duplicate {}", rule.id);
                assert!(
                    !rule.label.is_empty() && !rule.detail.is_empty() && !rule.effect.is_empty()
                );
            }
        }
        for rule in &catalog.shared {
            assert!(ids.insert(rule.id));
            assert!(
                catalog
                    .tags
                    .iter()
                    .filter(|&&tags| tags & rule.tags != 0)
                    .count()
                    >= 2
            );
        }
        assert_eq!(catalog.shared.len(), 24);
        assert_eq!(ids.len(), 424);
    }

    #[test]
    fn all_new_guests_visit_without_invitation() {
        let started = 1_800_000_000;
        let mut seen = BTreeSet::new();
        for age in (0..42 * DAY).step_by(180) {
            let time = ((age / 60 + 390) % 1_440) as u16;
            let state = snapshot(
                started + age,
                started,
                time,
                Season::Spring,
                WeatherMode::Auto,
                None,
            );
            if let Some(role) = guest(&state) {
                assert!(role.is_extra());
                seen.insert(role.0);
            }
        }
        assert_eq!(seen, (1..=9).chain(18..=29).collect());
    }

    fn state_for(rule: &Rule) -> Snapshot {
        let (minute, part) = match rule.gate {
            Gate::Dawn => (390, DayPart::Dawn),
            Gate::Dusk => (1_140, DayPart::Dusk),
            Gate::Night => (1_270, DayPart::Night),
            Gate::Nap => (795, DayPart::Day),
            _ => (720, DayPart::Day),
        };
        let season = match rule.gate {
            Gate::Autumn => Season::Autumn,
            Gate::Winter => Season::Winter,
            _ => Season::Spring,
        };
        let weather = match rule.gate {
            Gate::Rain | Gate::Wet => WeatherMode::Rain,
            Gate::Snow => WeatherMode::Snow,
            _ => WeatherMode::Clear,
        };
        let mut state = snapshot(1_800_000_000, 1_800_000_000, minute, season, weather, None);
        state.day_part = part;
        state.cat = match rule.gate {
            Gate::Sleep | Gate::Nap => CatAction::Sleep,
            Gate::Eat => CatAction::Eat,
            Gate::Treat => CatAction::Treat,
            _ => CatAction::Idle,
        };
        state.caretaker = match rule.gate {
            Gate::Build => CareAction::Building,
            Gate::Eat => CareAction::Feeding,
            Gate::Rest => CareAction::Resting,
            _ => CareAction::Tending,
        };
        state
    }

    fn prepare<'a>(state: &'a Snapshot, garden: &'a Garden, rule: &Rule) -> Context<'a> {
        Context {
            state,
            garden,
            water: true,
            lamp: true,
            food: rule.gate == Gate::Food,
            knock: rule.gate == Gate::Knock,
            fish: rule.gate == Gate::Fish,
            guest: rule.partner,
        }
    }

    fn assert_reachable(role: Role, rule: &Rule, gardens: &[Garden]) {
        let mut state = state_for(rule);
        let garden = gardens
            .iter()
            .find(|garden| condition(rule, &prepare(&state, garden, rule), 0).is_some())
            .unwrap_or_else(|| {
                panic!(
                    "no physical context for {} {:?} {:?}",
                    rule.id, rule.gate, rule.target
                )
            });
        let seen = (0..2_000).any(|window| {
            state.phase_seconds = window * ACTION_SECONDS;
            action(role, &prepare(&state, garden, rule))
                .is_some_and(|selected| selected.id == rule.id)
        });
        assert!(seen, "not selected: {} {:?}", rule.id, rule.gate);
    }

    #[test]
    fn every_rule_can_be_selected_in_its_world_context() {
        let catalog = catalog();
        let gardens: Vec<_> = (0..720 * DAY)
            .step_by(4 * 3_600)
            .map(|age| plots(age, 0, 300, 200, Season::Spring, Weather::Clear))
            .collect();
        for (index, roles) in catalog.roles.iter().enumerate() {
            for rule in roles {
                assert_reachable(Role(index as u8), rule, &gardens);
            }
        }
        for rule in &catalog.shared {
            let index = catalog
                .tags
                .iter()
                .position(|&tags| tags & rule.tags != 0)
                .unwrap();
            assert_reachable(Role(index as u8), rule, &gardens);
        }
    }

    #[test]
    fn each_timeline_keeps_its_target_and_reveals_props_after_approach() {
        let role = Role(8);
        let garden = plots(0, 0, 160, 100, Season::Spring, Weather::Clear);
        let mut state = snapshot(
            1_800_000_000,
            1_800_000_000,
            390,
            Season::Spring,
            WeatherMode::Clear,
            None,
        );
        let local_offset = u64::from(role.0) * 11;
        let mut scene = |seconds| {
            state.phase_seconds = 2 * ACTION_SECONDS - local_offset + seconds;
            action(role, &Context::new(&state, &garden)).unwrap()
        };
        let [approach, work, result] = [15, 45, 108].map(&mut scene);
        assert_eq!(
            [approach.phase, work.phase, result.phase],
            [Phase::Approach, Phase::Work, Phase::Result]
        );
        assert_eq!(approach.prop, Prop::None);
        assert_eq!(work.prop, result.prop);
        assert_ne!(result.prop, Prop::None);
        assert_eq!(approach.id, result.id);
        assert_eq!((approach.x, approach.y), (result.x, result.y));
    }
}
