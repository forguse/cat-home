//! The residents and guests run short, deterministic scenes drawn from the role table.

use std::sync::OnceLock;

use crate::activities::{Motion, Prop};
use crate::ecology::{Event, Garden, Kind, Plot, Stage};
use crate::simulation::{CareAction, CatAction, DayPart, Season, Snapshot, Visitor, Weather};

pub const ROLE_COUNT: usize = 30;
const ACTION_SECONDS: u64 = 120;
const GUEST_WINDOW: u64 = 31 * 60;
const RULES: &str = include_str!("../design/cast-rules.tsv");
const SCENES: &str = include_str!("../design/cast-scenes.tsv");
const PAIRS: &str = include_str!("../design/cast-pairs.tsv");
const RESIDENTS: &str = include_str!("../design/character-residents.txt");
const EXCLUSIVE: [&str; 3] = [
    include_str!("../design/character-humans.tsv"),
    include_str!("../design/character-animals-a.tsv"),
    include_str!("../design/character-animals-b.tsv"),
];
const SHARED: &str = include_str!("../design/character-shared.tsv");
static CATALOG: OnceLock<Catalog> = OnceLock::new();
static SCENE_INDEX: OnceLock<SceneIndex> = OnceLock::new();
static PAIR_INDEX: OnceLock<Vec<(Role, Role)>> = OnceLock::new();

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
    pub companion: Option<Role>,
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
            companion: None,
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
#[repr(u8)]
pub enum EffectKind {
    Trail,
    Signal,
    Rest,
    Bond,
    MoveObject,
    Forage,
    Tidy,
    Water,
    Drain,
    Sow,
    TendPlant,
    Pollinate,
    Harvest,
    Build,
    Repair,
    Shelter,
    Nest,
    Deliver,
    Forecast,
    Art,
    SeedTrade,
    Food,
    Lamp,
    Survey,
}

impl EffectKind {
    pub const ALL: [Self; 24] = [
        Self::Trail,
        Self::Signal,
        Self::Rest,
        Self::Bond,
        Self::MoveObject,
        Self::Forage,
        Self::Tidy,
        Self::Water,
        Self::Drain,
        Self::Sow,
        Self::TendPlant,
        Self::Pollinate,
        Self::Harvest,
        Self::Build,
        Self::Repair,
        Self::Shelter,
        Self::Nest,
        Self::Deliver,
        Self::Forecast,
        Self::Art,
        Self::SeedTrade,
        Self::Food,
        Self::Lamp,
        Self::Survey,
    ];

    #[cfg(test)]
    pub fn from_u8(value: u8) -> Option<Self> {
        Self::ALL.get(usize::from(value)).copied()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Trail => "足迹与路径",
            Self::Signal => "招呼与提醒",
            Self::Rest => "休息与落脚",
            Self::Bond => "熟悉与互动",
            Self::MoveObject => "移动小物件",
            Self::Forage => "取食",
            Self::Tidy => "清理",
            Self::Water => "补水与水面变化",
            Self::Drain => "排水",
            Self::Sow => "落种",
            Self::TendPlant => "养护",
            Self::Pollinate => "授粉",
            Self::Harvest => "采收与备菜",
            Self::Build => "施工",
            Self::Repair => "修复",
            Self::Shelter => "遮护",
            Self::Nest => "巢窝与藏点",
            Self::Deliver => "投递",
            Self::Forecast => "气象记录",
            Self::Art => "图画与标记",
            Self::SeedTrade => "留种与交换",
            Self::Food => "备餐",
            Self::Lamp => "灯光",
            Self::Survey => "观察记录",
        }
    }

    fn from_rule(value: &str) -> Self {
        match value {
            "Trail" => Self::Trail,
            "Signal" => Self::Signal,
            "Rest" => Self::Rest,
            "Bond" => Self::Bond,
            "MoveObject" => Self::MoveObject,
            "Forage" => Self::Forage,
            "Tidy" => Self::Tidy,
            "Water" => Self::Water,
            "Drain" => Self::Drain,
            "Sow" => Self::Sow,
            "TendPlant" => Self::TendPlant,
            "Pollinate" => Self::Pollinate,
            "Harvest" => Self::Harvest,
            "Build" => Self::Build,
            "Repair" => Self::Repair,
            "Shelter" => Self::Shelter,
            "Nest" => Self::Nest,
            "Deliver" => Self::Deliver,
            "Forecast" => Self::Forecast,
            "Art" => Self::Art,
            "SeedTrade" => Self::SeedTrade,
            "Food" => Self::Food,
            "Lamp" => Self::Lamp,
            "Survey" => Self::Survey,
            _ => panic!("invalid cast effect {value}"),
        }
    }

    pub fn allows(self, role: Role) -> bool {
        match self {
            Self::Drain => matches!(role.0, 0 | 7 | 16),
            // People plant deliberately. Birds, squirrels and frogs can disperse seeds.
            Self::Sow => matches!(role.0, 0 | 1 | 13 | 14 | 17),
            // Rabbits contribute compost, and ladybirds protect new leaves.
            Self::TendPlant => matches!(role.0, 0 | 1 | 16 | 21),
            Self::Pollinate => matches!(role.0, 20 | 23 | 26),
            Self::Harvest | Self::Food => matches!(role.0, 0 | 5),
            Self::Build | Self::Repair => matches!(role.0, 0 | 2),
            Self::Nest => matches!(role.0, 13..=29),
            Self::Deliver => matches!(role.0, 0 | 3 | 6),
            Self::Forecast => role.0 == 4,
            Self::Art => role.0 == 8,
            Self::SeedTrade => matches!(role.0, 0 | 1 | 9),
            Self::Lamp => matches!(role.0, 0 | 3),
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Action {
    pub role: Role,
    pub id: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub effect: &'static str,
    pub effect_kind: EffectKind,
    pub motion: Motion,
    pub prop: Prop,
    pub target: Target,
    pub x: i32,
    pub y: i32,
    pub phase: Phase,
    pub phase_progress: u8,
    pub completes_now: bool,
    pub window: u64,
    pub plot: Option<u8>,
    pub variant: u8,
    pub partner: Option<Role>,
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
    effect_kind: EffectKind,
    target: Target,
    gate: Gate,
    motion: Motion,
    prop: Prop,
    partner: Option<Role>,
    tags: u16,
    place: u8,
    variant: u8,
}

const CLIMATES: usize = 4 * 4 * 3;
const SHARED_SCENES: usize = ROLE_COUNT;

#[derive(Clone, Copy, Default)]
struct SceneBucket {
    lines: &'static str,
    count: usize,
}

struct SceneIndex {
    buckets: [[SceneBucket; CLIMATES]; ROLE_COUNT + 1],
    climates_by_role: [u64; ROLE_COUNT],
}

impl SceneIndex {
    fn load() -> Self {
        let mut buckets = [[SceneBucket::default(); CLIMATES]; ROLE_COUNT + 1];
        let mut climates_by_role = [0; ROLE_COUNT];
        let mut block: Option<(usize, usize, usize)> = None;
        let mut offset = 0;
        let mut count = 0;
        for line in SCENES.split_inclusive('\n') {
            if let Some(profile) = line.strip_prefix('#') {
                let [role, climates] = fields(profile.trim_end(), '\t');
                climates_by_role[role.parse::<usize>().unwrap()] = climates.parse().unwrap();
            } else if let Some(header) = line.strip_prefix('@') {
                if let Some((role, climate, start)) = block {
                    buckets[role][climate] = SceneBucket {
                        lines: &SCENES[start..offset],
                        count,
                    };
                }
                let [role, climate] = fields(header.trim_end(), '\t');
                block = Some((
                    role.parse().unwrap(),
                    climate.parse().unwrap(),
                    offset + line.len(),
                ));
                count = 0;
            } else {
                count += 1;
            }
            offset += line.len();
        }
        if let Some((role, climate, start)) = block {
            buckets[role][climate] = SceneBucket {
                lines: &SCENES[start..offset],
                count,
            };
        }
        Self {
            buckets,
            climates_by_role,
        }
    }
}

fn scene_bucket(index: usize, state: &Snapshot) -> SceneBucket {
    SCENE_INDEX.get_or_init(SceneIndex::load).buckets[index][climate_index(state)]
}

fn shares_climate(role: Role, state: &Snapshot) -> bool {
    SCENE_INDEX
        .get_or_init(SceneIndex::load)
        .climates_by_role
        .get(usize::from(role.0))
        .is_some_and(|mask| mask & (1 << climate_index(state)) != 0)
}

fn climate_index(state: &Snapshot) -> usize {
    let period = match state.day_part {
        DayPart::Dawn => 0,
        DayPart::Day => 1,
        DayPart::Dusk => 2,
        DayPart::Night => 3,
    };
    let weather = match state.weather {
        Weather::Clear => 0,
        Weather::Clouds => 1,
        Weather::Rain => 2,
        Weather::Snow => 3,
    };
    let season = match state.season {
        Season::Spring => 0,
        Season::Autumn => 1,
        Season::Winter => 2,
    };
    (period * 4 + weather) * 3 + season
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

    fn original(&self, id: &str) -> Rule {
        if let Some(index) = id.strip_prefix('S') {
            self.shared[index.parse::<usize>().unwrap() - 1]
        } else {
            let (role, index) = id.split_once('-').unwrap();
            self.roles[usize::from(parse_role(role).0)][index.parse::<usize>().unwrap() - 1]
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
        let [id, gate, target, motion, prop, partner, effect_kind] = fields(data, '\t');
        Self {
            id,
            label,
            detail,
            effect,
            effect_kind: EffectKind::from_rule(effect_kind),
            tags,
            place: 0,
            variant: 0,
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

    fn expanded(line: &'static str, catalog: &Catalog) -> Self {
        let [id, base, _, _, _, place, variant, partner, label] = fields(line, '\t');
        let rule = catalog.original(base);
        debug_assert_eq!(base, rule.id);
        Self {
            id,
            label,
            place: place.parse().unwrap(),
            variant: variant.parse().unwrap(),
            partner: if partner == "-" {
                None
            } else {
                Some(parse_role(partner))
            },
            ..rule
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
        2 => day && dry,
        3 => dusk || night,
        4 => !dry || state.weather == Weather::Clouds,
        5 => (660..900).contains(&minute),
        6 => (720..1_080).contains(&minute),
        7 => state.weather == Weather::Rain || (day && state.build_stage >= 2),
        8 => (dawn || dusk || day && minute < 660) && dry,
        9 => day && dry && (state.phase_seconds / 86_400).is_multiple_of(3),
        18 => (dawn || day) && dry && warm,
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

fn paired_available(role: Role, state: &Snapshot) -> bool {
    if role.is_extra() {
        return natural(role, state, state.minutes_of_day);
    }
    let dry = matches!(state.weather, Weather::Clear | Weather::Clouds);
    let daytime = matches!(state.day_part, DayPart::Dawn | DayPart::Day);
    match role.0 {
        11 | 13 | 16 => daytime && state.weather != Weather::Snow,
        12 | 15 => matches!(state.day_part, DayPart::Dusk | DayPart::Night) && dry,
        14 => daytime && dry,
        17 => state.weather == Weather::Rain,
        _ => false,
    }
}

fn bucket_has_partner(lead: Role, companion: Role, state: &Snapshot) -> bool {
    scene_bucket(usize::from(lead.0), state)
        .lines
        .lines()
        .any(|line| {
            line.split('\t')
                .nth(7)
                .is_some_and(|value| value != "-" && parse_role(value) == companion)
        })
}

pub fn paired_guest(primary: Role, state: &Snapshot) -> Option<Role> {
    if !can_visit(primary) {
        return None;
    }
    let stamp = mix(state.world_start ^ (state.phase_seconds / 300) ^ u64::from(primary.0) * 137);
    if stamp % 3 != 0 {
        return None;
    }
    let pairs = PAIR_INDEX.get_or_init(|| {
        PAIRS
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let [lead, companion, _, _] = fields(line, '\t');
                (parse_role(lead), parse_role(companion))
            })
            .collect()
    });
    let begin = stamp as usize % pairs.len();
    pairs[begin..]
        .iter()
        .chain(&pairs[..begin])
        .find_map(|&(lead, companion)| {
            let other = if lead == primary {
                companion
            } else if companion == primary {
                lead
            } else {
                return None;
            };
            (can_visit(other)
                && paired_available(other, state)
                && bucket_has_partner(lead, companion, state))
            .then_some(other)
        })
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

fn partner_is_present(partner: Role, context: &Context<'_>) -> bool {
    let state = context.state;
    match partner.0 {
        0 => state.caretaker != CareAction::Away,
        10 => state.day_part != DayPart::Night || state.cat != CatAction::Sleep,
        _ => {
            Role::from_visitor(state.visitor) == Some(partner)
                || context.guest == Some(partner)
                || context.companion == Some(partner)
        }
    }
}

fn condition(rule: &Rule, context: &Context<'_>, seed: u64) -> Option<Option<Plot>> {
    let state = context.state;
    if rule
        .partner
        .is_some_and(|partner| !partner_is_present(partner, context))
    {
        return None;
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
    let needs_place = matches!(rule.place, 1 | 2);
    if !(needs_stage || plot_target || needs_place) {
        return Some(None);
    }
    let plots = context.garden.plots().iter().filter(|plot| {
        plot_matches(plot, rule.target, rule.gate)
            && match rule.place {
                1 => plot.id < 3,
                2 => plot.id >= 3,
                _ => true,
            }
    });
    let count = plots.clone().count();
    if count != 0 {
        return Some(plots.into_iter().nth(seed as usize % count).copied());
    }
    if needs_place || needs_stage && !(rule.gate == Gate::Finished && state.build_stage != 0) {
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

fn choose_scene(
    bucket: SceneBucket,
    catalog: &Catalog,
    context: &Context<'_>,
    role: Role,
    seed: u64,
    traits: u16,
    keep: impl Fn(&Rule) -> bool,
) -> Option<(Rule, Option<Plot>)> {
    if bucket.count == 0 {
        return None;
    }
    let start = seed as usize % bucket.count;
    bucket
        .lines
        .lines()
        .skip(start)
        .chain(bucket.lines.lines().take(start))
        .find_map(|line| {
            let mut rule = Rule::expanded(line, catalog);
            if !keep(&rule)
                || rule.tags != 0
                    && (rule.tags & traits == 0 || !shares_climate(role, context.state))
            {
                return None;
            }
            if let Some(required) = shared_company(rule.id) {
                if traits & required == 0 {
                    return None;
                }
                rule.partner = [
                    context.guest,
                    context.companion,
                    Role::from_visitor(context.state.visitor),
                    Some(Role(0)),
                    Some(Role(10)),
                ]
                .into_iter()
                .flatten()
                .find(|candidate| {
                    *candidate != role
                        && catalog
                            .tags
                            .get(usize::from(candidate.0))
                            .is_some_and(|tags| tags & required != 0)
                        && shares_climate(*candidate, context.state)
                        && partner_is_present(*candidate, context)
                });
                rule.partner?;
            }
            condition(&rule, context, seed).map(|plot| (rule, plot))
        })
}

fn shared_company(id: &str) -> Option<u16> {
    match id.get(..3)? {
        "S01" => Some(8),
        "S05" => Some(512),
        "S03" | "S17" | "S19" | "S24" => Some(64),
        _ => None,
    }
}

fn performer_can_act(role: Role, rule: &Rule, state: &Snapshot) -> bool {
    if !rule.effect_kind.allows(role) || rule.partner == Some(role) {
        return false;
    }
    if role.0 == 10 && state.cat == CatAction::Sleep {
        return matches!(rule.gate, Gate::Sleep | Gate::Nap)
            || rule.gate == Gate::Night && rule.effect_kind == EffectKind::Rest;
    }
    if role.0 == 10 && state.cat == CatAction::Eat {
        return rule.gate == Gate::Eat;
    }
    true
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
    let allowed = |rule: &Rule| performer_can_act(role, rule, context.state);
    let override_rule = |rule: &Rule| {
        (context.knock && rule.gate == Gate::Knock)
            || (context.fish && rule.gate == Gate::Fish)
            || (context.food && rule.gate == Gate::Food)
            || (role.0 == 10 && context.state.cat == CatAction::Treat && rule.gate == Gate::Treat)
    };
    let bucket = scene_bucket(index, context.state);
    let forced_scene = || {
        choose_scene(bucket, catalog, context, role, seed, traits, |rule| {
            allowed(rule) && override_rule(rule)
        })
    };
    let forced_base = || {
        choose(rows, context, seed, traits, |rule| {
            allowed(rule) && override_rule(rule)
        })
        .map(|(rule, plot)| (*rule, plot))
    };
    let forced = if !(context.knock
        || context.fish
        || context.food
        || role.0 == 10 && context.state.cat == CatAction::Treat)
    {
        None
    } else if window % 2 == 0 {
        forced_base().or_else(forced_scene)
    } else {
        forced_scene().or_else(forced_base)
    };
    // The original residents and first seven visitors still show their own
    // animation schedules. Added guests always use the cast table.
    if forced.is_none() && !role.is_extra() && window % 4 == 0 {
        return None;
    }
    let (rule, plot) = forced.or_else(|| {
        let original =
            || choose(rows, context, seed, traits, allowed).map(|(rule, plot)| (*rule, plot));
        let ordinary_scene = || {
            choose_scene(bucket, catalog, context, role, seed, traits, |rule| {
                rule.partner.is_none() && allowed(rule)
            })
        };
        let paired_scene = || {
            choose_scene(bucket, catalog, context, role, seed, traits, |rule| {
                rule.partner.is_some() && allowed(rule)
            })
        };
        if window % 6 == 5 && !resident_sleep && !resident_eat {
            let shared = scene_bucket(SHARED_SCENES, context.state);
            let expanded = || choose_scene(shared, catalog, context, role, seed, traits, allowed);
            let legacy = || {
                choose(&catalog.shared, context, seed, traits, allowed)
                    .map(|(rule, plot)| (*rule, plot))
            };
            if window / 6 % 3 == 0 {
                legacy().or_else(expanded)
            } else {
                expanded().or_else(legacy)
            }
            .or_else(original)
        } else if window % 5 < 2 {
            original().or_else(ordinary_scene).or_else(paired_scene)
        } else if matches!(window % 7, 1 | 3 | 4) {
            paired_scene().or_else(ordinary_scene).or_else(original)
        } else {
            ordinary_scene().or_else(original).or_else(paired_scene)
        }
    })?;
    Some(render_action(
        role,
        rule,
        plot,
        window,
        seconds,
        resident_sleep,
        forced.is_some(),
        traits,
    ))
}

fn render_action(
    role: Role,
    rule: Rule,
    plot: Option<Plot>,
    window: u64,
    seconds: u64,
    resident_sleep: bool,
    forced: bool,
    traits: u16,
) -> Action {
    let flying = traits & 512 != 0;
    let (tx, ty) = if let Some(plot) = plot {
        (plot.x as i32, plot.y as i32 + if flying { -3 } else { 5 })
    } else {
        anchor(rule.target)
    };
    let offset = if rule.variant == 0 {
        rule.id.as_bytes().last().copied().unwrap_or(b'0') as i32 - i32::from(b'0')
    } else {
        i32::from(rule.variant)
    };
    let side = match rule.place {
        3 => -7,
        5 => 7,
        _ => 0,
    };
    let (tx, ty) = (
        (tx + side + offset % 5 - 2).max(10),
        (ty + offset % 3 - 1).max(25),
    );
    let completes_now = forced && matches!(rule.gate, Gate::Knock | Gate::Treat);
    let (phase, phase_progress) = match seconds {
        _ if completes_now => (Phase::Work, 50),
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
    Action {
        role,
        id: rule.id,
        label: rule.label,
        detail: rule.detail,
        effect: rule.effect,
        effect_kind: rule.effect_kind,
        motion,
        prop,
        target: rule.target,
        x: tx,
        y: ty,
        phase,
        phase_progress: phase_progress as u8,
        completes_now,
        window,
        plot: plot.map(|p| p.id),
        variant: rule.variant,
        partner: rule.partner,
    }
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
    fn effects_cover_the_catalog_and_match_each_roles_capabilities() {
        let catalog = catalog();
        let mut used = [false; EffectKind::ALL.len()];
        for (index, rules) in catalog.roles.iter().enumerate() {
            let role = Role(index as u8);
            for rule in rules {
                assert!(rule.effect_kind.allows(role), "{}", rule.id);
                if index >= 10 {
                    assert_ne!(rule.prop, Prop::Tool, "{}", rule.id);
                }
                used[rule.effect_kind as usize] = true;
            }
        }
        for rule in &catalog.shared {
            for (index, &tags) in catalog.tags.iter().enumerate() {
                if tags & rule.tags != 0 {
                    assert!(rule.effect_kind.allows(Role(index as u8)), "{}", rule.id);
                }
            }
            used[rule.effect_kind as usize] = true;
        }
        assert!(used.into_iter().all(|effect| effect));
        for (index, kind) in EffectKind::ALL.into_iter().enumerate() {
            assert_eq!(EffectKind::from_u8(index as u8), Some(kind));
            assert_eq!(kind as usize, index);
            assert!(!kind.label().is_empty());
        }
        assert_eq!(EffectKind::from_u8(EffectKind::ALL.len() as u8), None);
    }

    #[test]
    fn expert_roles_and_seed_dispersers_use_their_own_effects() {
        let catalog = catalog();
        let rule = |role: u8, id: &str| {
            catalog.roles[usize::from(role)]
                .iter()
                .find(|rule| rule.id == id)
                .expect("expected example action")
        };
        let artist = rule(8, "H09-01");
        assert_eq!(
            (artist.effect_kind, artist.prop),
            (EffectKind::Art, Prop::Book)
        );
        let sparrow = rule(13, "A04-01");
        assert_eq!(
            (sparrow.effect_kind, sparrow.target, sparrow.prop),
            (EffectKind::Sow, Target::Flower, Prop::Seed)
        );
        let plumber = rule(7, "H08-04");
        assert_eq!(
            (plumber.effect_kind, plumber.target),
            (EffectKind::Drain, Target::Pond)
        );
        let bee = rule(23, "A14-01");
        assert_eq!(bee.effect_kind, EffectKind::Pollinate);
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
            companion: None,
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
        let seen = (0..8_000).any(|window| {
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

    fn state_in_climate(rule: &Rule, climate: usize) -> Snapshot {
        let mut state = state_for(rule);
        state.season = [Season::Spring, Season::Autumn, Season::Winter][climate % 3];
        state.weather = [
            Weather::Clear,
            Weather::Clouds,
            Weather::Rain,
            Weather::Snow,
        ][(climate / 3) % 4];
        state.day_part = [DayPart::Dawn, DayPart::Day, DayPart::Dusk, DayPart::Night][climate / 12];
        state.minutes_of_day = match state.day_part {
            DayPart::Dawn => 390,
            DayPart::Day if rule.gate == Gate::Nap => 795,
            DayPart::Day => 735,
            DayPart::Dusk => 1_110,
            DayPart::Night => 1_270,
        };
        if state.day_part == DayPart::Night {
            if rule.gate != Gate::Treat {
                state.cat = CatAction::Sleep;
            }
            state.caretaker = CareAction::Resting;
        }
        state
    }

    #[test]
    fn every_expanded_scene_can_be_chosen_and_retains_its_world_effect() {
        let catalog = catalog();
        let index = SCENE_INDEX.get_or_init(SceneIndex::load);
        let gardens: Vec<_> = (0..720 * DAY)
            .step_by(4 * 3_600)
            .map(|age| plots(age, 0, 300, 200, Season::Spring, Weather::Clear))
            .collect();
        let mut ids = BTreeSet::new();
        let mut totals = [0; 3];
        for (actor, buckets) in index.buckets.iter().enumerate() {
            for (climate, bucket) in buckets.iter().copied().enumerate() {
                assert_eq!(bucket.lines.lines().count(), bucket.count);
                for (offset, line) in bucket.lines.lines().enumerate() {
                    let rule = Rule::expanded(line, catalog);
                    let state = state_in_climate(&rule, climate);
                    assert!(ids.insert(rule.id), "duplicate {}", rule.id);
                    assert!((1..=3).contains(&rule.variant), "{}", rule.id);
                    let company = shared_company(rule.id);
                    let (role, companion) = if actor == SHARED_SCENES {
                        totals[1] += 1;
                        let role = catalog
                            .tags
                            .iter()
                            .enumerate()
                            .find_map(|(role, tags)| {
                                let candidate = Role(role as u8);
                                (tags & rule.tags != 0
                                    && shares_climate(candidate, &state)
                                    && performer_can_act(candidate, &rule, &state)
                                    && company.is_none_or(|kind| tags & kind != 0))
                                .then_some(candidate)
                            })
                            .unwrap();
                        let companion = company.and_then(|kind| {
                            catalog.tags.iter().enumerate().find_map(|(index, tags)| {
                                let candidate = Role(index as u8);
                                (candidate != role
                                    && tags & kind != 0
                                    && shares_climate(candidate, &state)
                                    && (candidate != Role(10) || state.cat != CatAction::Sleep))
                                    .then_some(candidate)
                            })
                        });
                        (role, companion)
                    } else {
                        totals[if rule.partner.is_some() { 2 } else { 0 }] += 1;
                        (Role(actor as u8), rule.partner)
                    };
                    assert_eq!(climate_index(&state), climate);
                    let with_context = |garden| {
                        let mut context = prepare(&state, garden, &rule);
                        context.companion = companion;
                        context
                    };
                    let garden = gardens
                        .iter()
                        .find(|garden| condition(&rule, &with_context(garden), 0).is_some())
                        .unwrap_or_else(|| {
                            panic!(
                                "no matching target for {} {:?} {:?}",
                                rule.id, rule.target, rule.gate
                            )
                        });
                    let context = with_context(garden);
                    let traits = catalog.tags[usize::from(role.0)];
                    let (chosen, plot) = choose_scene(
                        bucket,
                        catalog,
                        &context,
                        role,
                        offset as u64,
                        traits,
                        |rule| performer_can_act(role, rule, &state),
                    )
                    .unwrap_or_else(|| panic!("{} not selectable", rule.id));
                    assert_eq!(chosen.id, rule.id, "climate {climate}, role {role:?}");
                    let action = render_action(
                        role,
                        chosen,
                        plot,
                        0,
                        110,
                        state.cat == CatAction::Sleep,
                        false,
                        traits,
                    );
                    assert_eq!(
                        (action.variant, action.effect_kind),
                        (rule.variant, rule.effect_kind),
                        "{}",
                        rule.id
                    );
                    assert_eq!(action.phase, Phase::Result);
                    if company.is_some() || rule.partner.is_some() {
                        assert!(
                            action.partner.is_some_and(|partner| partner != role),
                            "{}",
                            rule.id
                        );
                    }
                    if rule.partner.is_some_and(can_visit) {
                        let mut alone = context;
                        alone.guest = None;
                        alone.companion = None;
                        assert!(condition(&rule, &alone, 0).is_none(), "{}", rule.id);
                    }
                }
            }
        }
        assert_eq!(totals, [2_600, 1_076, 900]);
        assert_eq!(ids.len() + 424, 5_000);
    }

    #[test]
    fn curated_guest_never_duplicates_the_primary_or_leaves_the_pair_profiles() {
        let mut visits = BTreeSet::new();
        for age in (0..10 * DAY).step_by(120) {
            let mut state = snapshot(
                1_800_000_000 + age,
                1_800_000_000,
                (age / 60 % 1_440) as u16,
                Season::Spring,
                WeatherMode::Auto,
                None,
            );
            state.caretaker = CareAction::Tending;
            for primary in (1..=9).chain(11..ROLE_COUNT as u8).map(Role) {
                if let Some(other) = paired_guest(primary, &state) {
                    assert_ne!(primary, other);
                    assert!(can_visit(other) && paired_available(other, &state));
                    assert!(
                        bucket_has_partner(primary, other, &state)
                            || bucket_has_partner(other, primary, &state)
                    );
                    visits.insert((primary.0, other.0));
                }
            }
        }
        assert!(visits.len() >= 20, "{visits:?}");
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
