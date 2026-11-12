//! Clock-based activity selection from the reviewed behavior table. The table
//! is indexed once; selecting a frame uses borrowed data and no allocations.

use std::fmt;
use std::sync::OnceLock;

use crate::ecology::Plot;
use crate::simulation::{CareAction, CatAction, DayPart, Season, Snapshot, Visitor, Weather};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Motion {
    Bob,
    Hop,
    Nap,
    Patrol,
    Still,
    Tilt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Prop {
    Bell,
    Book,
    Butterfly,
    Cloud,
    Crumb,
    Feather,
    Flower,
    Food,
    Heart,
    Lamp,
    Leaf,
    Mist,
    None,
    Note,
    Paw,
    Puddle,
    Rain,
    Seed,
    Snow,
    Spark,
    Star,
    Stone,
    Tool,
    Water,
    Wood,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Activity {
    pub id: &'static str,
    pub name: &'static str,
    pub motion: Motion,
    pub prop: Prop,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Active {
    pub cat: Option<&'static Activity>,
    pub keeper: Option<&'static Activity>,
    pub visitor: Option<&'static Activity>,
    pub yard: Option<&'static Activity>,
}

const GROWING_PLOTS: usize = 12;
const SPECIES_COUNT: usize = 12;
const GROWTH_STAGES: usize = 6;
const RESPONSE_STAGES: usize = 3;
const PLANT_ENTRIES: usize = GROWING_PLOTS * SPECIES_COUNT * GROWTH_STAGES;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EcologyEntry(usize);

impl fmt::Display for EcologyEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "E{:03}", self.0)
    }
}

/// Locate the active plot state in the design table without storing a row per
/// combination. Disaster states describe the plot until normal growth resumes.
pub fn ecology_entry(plot: &Plot) -> Option<EcologyEntry> {
    if let (Some(disaster), Some(response)) =
        (plot.event.disaster_index(), plot.stage.response_index())
    {
        return Some(EcologyEntry(
            PLANT_ENTRIES + disaster * RESPONSE_STAGES + response + 1,
        ));
    }

    let site = usize::from(plot.id);
    let species = plot.kind.species_index()?;
    let stage = plot.stage.growth_index()?;
    (site < GROWING_PLOTS)
        .then(|| EcologyEntry((site * SPECIES_COUNT + species) * GROWTH_STAGES + stage + 1))
}

#[derive(Clone, Copy)]
#[repr(usize)]
enum Gate {
    CatIdle,
    CatWalk,
    CatEat,
    CatSleep,
    CatTreat,
    KeeperFeeding,
    KeeperTending,
    KeeperBuilding,
    KeeperResting,
    Sparrow,
    GreyCat,
    BlackCat,
    Dawn,
    Day,
    Dusk,
    Night,
    Clear,
    Clouds,
    Rain,
    Snow,
    Spring,
    Autumn,
    Winter,
    Stage2,
    Stage3,
}

const GATE_COUNT: usize = Gate::Stage3 as usize + 1;
const ANIMALS: &str = include_str!("../design/animals.tsv");
const VISITORS_YARD: &str = include_str!("../design/visitors-yard.tsv");
const YARD_WINDOW: u64 = 73;
static CATALOG: OnceLock<Catalog> = OnceLock::new();

struct Catalog {
    groups: [Vec<Activity>; GATE_COUNT],
}

impl Catalog {
    fn load() -> Self {
        let mut groups = std::array::from_fn(|_| Vec::new());
        for (line_no, line) in ANIMALS.lines().chain(VISITORS_YARD.lines()).enumerate() {
            let mut columns = line.split('\t');
            let [id, actor, gate, motion, prop, x, y, name, trigger, result] =
                std::array::from_fn(|_| columns.next().unwrap_or_default());
            assert!(
                !id.is_empty()
                    && !name.is_empty()
                    && !trigger.is_empty()
                    && !result.is_empty()
                    && columns.next().is_none(),
                "invalid activity table row {}",
                line_no + 1,
            );
            let gate = parse_gate(actor, gate);
            let activity = Activity {
                id,
                name,
                motion: parse_motion(motion),
                prop: parse_prop(prop),
                x: x.parse().expect("invalid activity x coordinate"),
                y: y.parse().expect("invalid activity y coordinate"),
            };
            groups[gate as usize].push(activity);
        }
        Self { groups }
    }

    fn group(&'static self, gate: Gate) -> &'static [Activity] {
        &self.groups[gate as usize]
    }

    fn pick(&'static self, gate: Gate, turn: u64) -> Option<&'static Activity> {
        let group = self.group(gate);
        if group.is_empty() {
            return None;
        }
        group.get((turn % group.len() as u64) as usize)
    }
}

pub fn current(state: &Snapshot) -> Active {
    let catalog = CATALOG.get_or_init(Catalog::load);
    let phase = state.phase_seconds;
    let (cat_gate, cat_seconds) = match state.cat {
        CatAction::Idle => (Gate::CatIdle, 37),
        CatAction::Walk => (Gate::CatWalk, 7),
        CatAction::Eat => (Gate::CatEat, 17),
        CatAction::Sleep => (Gate::CatSleep, 71),
        // A treat only lasts four seconds. Vary it across clicks without
        // stepping through a new pose on every simulation tick.
        CatAction::Treat => (Gate::CatTreat, 23),
    };
    let keeper = match state.caretaker {
        CareAction::Away => None,
        CareAction::Feeding => catalog.pick(Gate::KeeperFeeding, phase / 13),
        CareAction::Tending => catalog.pick(Gate::KeeperTending, phase / 19),
        CareAction::Building => catalog.pick(Gate::KeeperBuilding, phase / 23),
        CareAction::Resting => catalog.pick(Gate::KeeperResting, phase / 31),
    };
    let visitor = match state.visitor {
        Visitor::None => None,
        Visitor::Sparrow => catalog.pick(Gate::Sparrow, phase / 17),
        Visitor::GreyCat => catalog.pick(Gate::GreyCat, phase / 24),
        Visitor::BlackCat => catalog.pick(Gate::BlackCat, phase / 24),
    };
    Active {
        cat: catalog.pick(cat_gate, phase / cat_seconds),
        keeper,
        visitor,
        yard: yard_activity(catalog, state),
    }
}

fn yard_activity(catalog: &'static Catalog, state: &Snapshot) -> Option<&'static Activity> {
    let window = state.phase_seconds / YARD_WINDOW;
    let offset = state.phase_seconds % YARD_WINDOW;
    let variation = window.wrapping_mul(2_654_435_761).rotate_left(15);
    let starts_at = 12 + variation % 41;
    let duration = 6 + (variation >> 9) % 4;
    if !(starts_at..starts_at + duration).contains(&offset) {
        return None;
    }

    let day = match state.day_part {
        DayPart::Dawn => Gate::Dawn,
        DayPart::Day => Gate::Day,
        DayPart::Dusk => Gate::Dusk,
        DayPart::Night => Gate::Night,
    };
    let weather = match state.weather {
        Weather::Clear => Gate::Clear,
        Weather::Clouds => Gate::Clouds,
        Weather::Rain => Gate::Rain,
        Weather::Snow => Gate::Snow,
    };
    let season = match state.season {
        Season::Spring => Gate::Spring,
        Season::Autumn => Gate::Autumn,
        Season::Winter => Gate::Winter,
    };
    let groups = [
        Some(day),
        Some(weather),
        Some(season),
        (state.build_stage >= 2).then_some(Gate::Stage2),
        (state.build_stage >= 3).then_some(Gate::Stage3),
    ];
    let eligible = groups.into_iter().flatten();
    let count: usize = eligible.clone().map(|gate| catalog.group(gate).len()).sum();
    if count == 0 {
        return None;
    }
    let mut index = (window % count as u64) as usize;
    for gate in eligible {
        let group = catalog.group(gate);
        if let Some(activity) = group.get(index) {
            return Some(activity);
        }
        index -= group.len();
    }
    None
}

fn parse_gate(actor: &str, gate: &str) -> Gate {
    match (actor, gate) {
        ("cat", "idle") => Gate::CatIdle,
        ("cat", "walk") => Gate::CatWalk,
        ("cat", "eat") => Gate::CatEat,
        ("cat", "sleep") => Gate::CatSleep,
        ("cat", "treat") => Gate::CatTreat,
        ("keeper", "feeding") => Gate::KeeperFeeding,
        ("keeper", "tending") => Gate::KeeperTending,
        ("keeper", "building") => Gate::KeeperBuilding,
        ("keeper", "resting") => Gate::KeeperResting,
        ("visitor", "sparrow") => Gate::Sparrow,
        ("visitor", "grey") => Gate::GreyCat,
        ("visitor", "black") => Gate::BlackCat,
        ("yard", "dawn") => Gate::Dawn,
        ("yard", "day") => Gate::Day,
        ("yard", "dusk") => Gate::Dusk,
        ("yard", "night") => Gate::Night,
        ("yard", "clear") => Gate::Clear,
        ("yard", "clouds") => Gate::Clouds,
        ("yard", "rain") => Gate::Rain,
        ("yard", "snow") => Gate::Snow,
        ("yard", "spring") => Gate::Spring,
        ("yard", "autumn") => Gate::Autumn,
        ("yard", "winter") => Gate::Winter,
        ("yard", "stage2") => Gate::Stage2,
        ("yard", "stage3") => Gate::Stage3,
        _ => panic!("unknown activity gate: {actor}/{gate}"),
    }
}

fn parse_motion(value: &str) -> Motion {
    match value {
        "bob" => Motion::Bob,
        "hop" => Motion::Hop,
        "nap" => Motion::Nap,
        "patrol" => Motion::Patrol,
        "still" => Motion::Still,
        "tilt" => Motion::Tilt,
        _ => panic!("unknown activity motion: {value}"),
    }
}

fn parse_prop(value: &str) -> Prop {
    match value {
        "bell" => Prop::Bell,
        "book" => Prop::Book,
        "butterfly" => Prop::Butterfly,
        "cloud" => Prop::Cloud,
        "crumb" => Prop::Crumb,
        "feather" => Prop::Feather,
        "flower" => Prop::Flower,
        "food" => Prop::Food,
        "heart" => Prop::Heart,
        "lamp" => Prop::Lamp,
        "leaf" => Prop::Leaf,
        "mist" => Prop::Mist,
        "none" => Prop::None,
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
        _ => panic!("unknown activity prop: {value}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::{WeatherMode, snapshot};
    use std::collections::BTreeSet;

    fn fixture() -> Snapshot {
        snapshot(
            1_800_000_000,
            1_800_000_000,
            720,
            Season::Spring,
            WeatherMode::Clear,
            None,
        )
    }

    #[test]
    fn table_contains_112_distinct_drawable_activities() {
        let catalog = CATALOG.get_or_init(Catalog::load);
        let mut ids = BTreeSet::new();
        for activity in catalog.groups.iter().flatten() {
            assert!(ids.insert(activity.id), "duplicate id {}", activity.id);
            assert!(
                (0..160).contains(&activity.x),
                "{} falls off the yard",
                activity.id
            );
            assert!(
                (0..100).contains(&activity.y),
                "{} falls off the yard",
                activity.id
            );
        }
        assert_eq!(ids.len(), 112);
    }

    #[test]
    fn design_table_dimensions_match_live_ecology_entry_ranges() {
        let rules = include_str!("../design/variant-rules.tsv");
        let count = |prefix: &str| {
            rules
                .lines()
                .filter(|line| line.starts_with(prefix))
                .count()
        };
        assert_eq!(count("P\t"), GROWING_PLOTS);
        assert_eq!(count("S\t"), SPECIES_COUNT);
        assert_eq!(count("T\t"), GROWTH_STAGES);
        assert_eq!(RESPONSE_STAGES, 3);

        let events = count("D\t") * RESPONSE_STAGES;
        let manual = include_str!("../design/interaction.tsv").lines().count();
        let authored = ANIMALS.lines().count() + VISITORS_YARD.lines().count();
        assert_eq!(authored + PLANT_ENTRIES + events + manual, 1_000);
    }

    #[test]
    fn every_ecology_row_matches_a_reachable_garden_state() {
        use crate::ecology::{SITE_COUNT, plots_with_unlocks};

        let rules = include_str!("../design/variant-rules.tsv");
        let names = |prefix: &str| {
            rules
                .lines()
                .filter_map(|line| line.strip_prefix(prefix))
                .filter_map(|line| line.split('\t').next())
                .collect::<Vec<_>>()
        };
        let plots = names("P\t");
        let species = names("S\t");
        let growth = names("T\t");
        let disasters = names("D\t");
        let responses = ["受损", "修复", "恢复"];
        let total = PLANT_ENTRIES + disasters.len() * RESPONSE_STAGES;
        let mut seen = BTreeSet::new();
        let started = 1_800_000_000;
        let unlocked_at = [1; SITE_COUNT];

        for hour in 0..3 * 365 * 24 {
            let garden = plots_with_unlocks(
                started + hour * 3_600,
                started,
                300,
                200,
                Season::Spring,
                Weather::Clear,
                &unlocked_at,
            );
            for plot in garden.plots() {
                let Some(entry) = ecology_entry(plot) else {
                    continue;
                };
                if !seen.insert(entry) {
                    continue;
                }
                if entry.0 <= PLANT_ENTRIES {
                    let index = entry.0 - 1;
                    assert_eq!(
                        plot.site_label(),
                        plots[index / SPECIES_COUNT / GROWTH_STAGES]
                    );
                    assert_eq!(
                        plot.kind.label(),
                        species[index / GROWTH_STAGES % SPECIES_COUNT]
                    );
                    assert_eq!(plot.stage.label(), growth[index % GROWTH_STAGES]);
                } else {
                    let index = entry.0 - PLANT_ENTRIES - 1;
                    assert_eq!(plot.event.label(), disasters[index / RESPONSE_STAGES]);
                    assert_eq!(plot.stage.label(), responses[index % RESPONSE_STAGES]);
                }
            }
            if seen.len() == total {
                break;
            }
        }

        let expected = (1..=total).map(EcologyEntry).collect::<BTreeSet<_>>();
        assert_eq!(seen, expected);
    }

    #[test]
    fn all_rows_can_be_selected_by_their_actual_state_gates() {
        let mut ids = BTreeSet::new();
        let mut state = fixture();
        for (cat, prefix) in [
            (CatAction::Idle, 'C'),
            (CatAction::Walk, 'C'),
            (CatAction::Eat, 'C'),
            (CatAction::Sleep, 'C'),
            (CatAction::Treat, 'C'),
        ] {
            state.cat = cat;
            for phase in 0..550 {
                state.phase_seconds = phase;
                let activity = current(&state).cat.expect("cat activity");
                assert!(activity.id.starts_with(prefix));
                ids.insert(activity.id);
            }
        }
        for keeper in [
            CareAction::Feeding,
            CareAction::Tending,
            CareAction::Building,
            CareAction::Resting,
        ] {
            state.caretaker = keeper;
            for phase in 0..550 {
                state.phase_seconds = phase;
                ids.insert(current(&state).keeper.expect("keeper activity").id);
            }
        }
        for visitor in [Visitor::Sparrow, Visitor::GreyCat, Visitor::BlackCat] {
            state.visitor = visitor;
            for phase in 0..550 {
                state.phase_seconds = phase;
                ids.insert(current(&state).visitor.expect("visitor activity").id);
            }
        }
        state.build_stage = 3;
        for day in [DayPart::Dawn, DayPart::Day, DayPart::Dusk, DayPart::Night] {
            state.day_part = day;
            for weather in [
                Weather::Clear,
                Weather::Clouds,
                Weather::Rain,
                Weather::Snow,
            ] {
                state.weather = weather;
                for season in [Season::Spring, Season::Autumn, Season::Winter] {
                    state.season = season;
                    for phase in 0..900 {
                        state.phase_seconds = phase;
                        if let Some(activity) = current(&state).yard {
                            ids.insert(activity.id);
                        }
                    }
                }
            }
        }
        let all = CATALOG
            .get_or_init(Catalog::load)
            .groups
            .iter()
            .flatten()
            .map(|activity| activity.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(ids, all);
    }

    #[test]
    fn yard_is_quiet_most_of_the_time_and_respects_build_gates() {
        let mut state = fixture();
        let mut active_seconds = 0;
        for phase in 0..1_460 {
            state.phase_seconds = phase;
            let active = current(&state);
            assert!(active.keeper.is_none() && active.visitor.is_none());
            if let Some(yard) = active.yard {
                active_seconds += 1;
                assert_ne!(yard.id, "Y23");
                assert_ne!(yard.id, "Y24");
            }
        }
        assert!((120..=180).contains(&active_seconds));
    }
}
