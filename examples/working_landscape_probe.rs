//! Observe autonomous landscape work through the same Windows host and save
//! format as the widget. Reads an optional input save and writes only to a
//! newly created output directory.
#![cfg(windows)]
#![allow(dead_code)]
#[path = "../src/home_host.rs"]
mod home_host;

use cat_home::living_world::story::{CHAPTERS, Material};
use cat_home::living_world::{
    Activity, DAY_MS, HousingPhase, Item, LivingWorld, PlotStage, Pose, ScenicStage,
};
use cat_home::map_data::{ScenicKind, TilePos, WILLOW_VILLAGE};
use home_host::HomeHost;
use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const FRESH_WALL_START: u64 = 1_790_000_000_000;
const LIVE_SAVE: &str = "world-v4.json";
const RECLAIMED_DOOR: TilePos = TilePos::new(27, 37);
const RECLAIMED_WORK_YARD: TilePos = TilePos::new(28, 37);
const FIELD_STANDS: [TilePos; 2] = [TilePos::new(23, 57), TilePos::new(50, 58)];

const SUPPLY: &[&str] = &[
    "cut-tree",
    "strike-stone",
    "cut-grass",
    "pick-up-wood",
    "pick-up-stone",
    "pick-up-fibre",
    "carry-wood",
    "carry-stone",
    "carry-fibre",
    "store-wood",
    "store-stone",
    "store-fibre",
    "credited-wood",
    "credited-stone",
    "credited-fibre",
];
const GARDEN: &[&str] = &[
    "twist-rope",
    "rope-made",
    "carry-rope",
    "store-rope",
    "rope-credited",
    "field-30-clear",
    "field-30-sow",
    "field-30-harvest",
    "field-30-yield",
    "field-31-clear",
    "field-31-sow",
    "field-31-harvest",
    "field-31-yield",
];
const MATURE: &[&str] = &[
    "house-clear",
    "house-foundation",
    "house-frame",
    "house-roof",
    "house-fit_out",
    "house-move_in",
    "house-foundation-work",
    "house-frame-work",
    "house-foundation-payment",
    "house-frame-payment",
    "farm-store-commissioned",
];
const STORE: &[&str] = &[
    "farm-store-sorts",
    "farm-store-dries",
    "farm-store-packs",
    "farm-store-unloads",
    "farm-store-credited",
];

fn point_tile(point: cat_home::living_world::Point) -> TilePos {
    TilePos::new((point.x / 8.0) as u16, (point.y / 8.0) as u16)
}

fn authority(host: &HomeHost) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&(
        &host.world,
        host.scenes.state(),
        &host.life,
    ))?)
}

fn advance(host: &mut HomeHost, wall: u64) -> Result<()> {
    for _ in 0..10_000 {
        host.tick(wall);
        if !host.world.is_catching_up()
            && !host.world.navigation_pending()
            && !host.life.navigation_pending()
        {
            return Ok(());
        }
    }
    Err(io::Error::other("landscape probe stopped advancing").into())
}

fn save_observation(host: &HomeHost, out: &Path, label: &str, detail: &str) -> Result<()> {
    let reference = out.join(label);
    fs::create_dir(&reference)?;
    host.save()?;
    let bytes = fs::copy(out.join("live").join(LIVE_SAVE), reference.join(LIVE_SAVE))?;
    if bytes >= 256 * 1024 {
        return Err(io::Error::other("save exceeded the host buffer").into());
    }
    println!(
        "{label:>24} day {:>6}  {:>8}ms  {bytes:>6}B  {detail}",
        host.world.world_ms() / DAY_MS,
        host.world.world_ms() % DAY_MS,
    );
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct Hands {
    source: Option<(&'static str, TilePos)>,
    pickup: Option<(&'static str, TilePos, Option<Item>, u16)>,
    grain: Option<usize>,
    twisting: bool,
    product: Option<Item>,
    amount: u16,
    unloading: bool,
    unloading_crop: bool,
    local_material: u16,
    payment: Option<Payment>,
}

#[derive(Clone, Copy)]
enum Payment {
    Chapter { id: usize, at: TilePos },
    House { at: TilePos, becomes: HousingPhase },
}

impl Payment {
    fn did_commit(self, world: &LivingWorld) -> bool {
        match self {
            Self::Chapter { id, .. } => {
                world
                    .story()
                    .active()
                    .is_some_and(|project| usize::from(project.id) == id)
                    || world.story().completed() > id
            }
            Self::House { at, becomes } => world
                .housing_sites()
                .any(|house| point_tile(house.position) == at && house.phase == becomes),
        }
    }

    fn site(self) -> TilePos {
        match self {
            Self::Chapter { at, .. } | Self::House { at, .. } => at,
        }
    }
}

fn tracked_stock(item: Item) -> Option<usize> {
    match item {
        Item::Wood => Some(0),
        Item::Stone => Some(1),
        Item::Rope => Some(3),
        _ => None,
    }
}

fn material_name(item: Item) -> &'static str {
    match item {
        Item::Wood => "wood",
        Item::Stone => "stone",
        Item::Rope => "rope",
        _ => unreachable!("not a construction material"),
    }
}

struct Observer {
    saved: BTreeSet<String>,
    prior: [Hands; 2],
    inventory: [u16; 4],
    locally_sourced: [u16; 4],
    crop_stock: u16,
}

fn stocks(world: &LivingWorld) -> [u16; 4] {
    [
        world.stock().count(Item::Wood),
        world.stock().count(Item::Stone),
        u16::from(world.landscape_fibre_stock()),
        world.stock().count(Item::Rope),
    ]
}

impl Observer {
    fn new(host: &HomeHost) -> Self {
        Self {
            saved: BTreeSet::new(),
            prior: [Hands::default(); 2],
            inventory: stocks(&host.world),
            locally_sourced: [0; 4],
            crop_stock: host.world.stock().count(Item::Crops),
        }
    }

    fn note(&mut self, host: &HomeHost, out: &Path, label: &str, detail: &str) -> Result<()> {
        if self.saved.insert(label.to_owned()) {
            save_observation(host, out, label, detail)?;
        }
        Ok(())
    }

    fn observe(&mut self, host: &HomeHost, out: &Path, objects: &[Option<usize>]) -> Result<()> {
        let world = &host.world;
        let inventory = stocks(world);
        let crop_stock = world.stock().count(Item::Crops);
        let mut credited = [0; 4];
        for (index, actor) in world.actors()[..2].iter().enumerate() {
            let at = world.actor_tile(actor.kind);
            let fibre = world.landscape_carried_fibre(actor.kind);
            let (product, amount) = if fibre > 0 {
                (None, u16::from(fibre))
            } else {
                (actor.carried, actor.carried_amount())
            };
            let previous = self.prior[index];
            let mut hands = Hands {
                source: None,
                pickup: previous.pickup,
                grain: previous.grain,
                twisting: actor.activity == Activity::TwistFibre && fibre == 3,
                product,
                amount,
                unloading: false,
                unloading_crop: false,
                local_material: 0,
                payment: None,
            };
            if previous.unloading_crop
                && amount == 0
                && at == RECLAIMED_DOOR
                && crop_stock == self.crop_stock + previous.amount
            {
                self.note(
                    host,
                    out,
                    "farm-store-credited",
                    &format!(
                        "{:?} at {at:?}, {} delivered crops; warehouse {} -> {crop_stock}",
                        actor.kind, previous.amount, self.crop_stock,
                    ),
                )?;
            }
            if amount == 0
                && let (Some(payment), Some(item)) = (previous.payment, previous.product)
                && payment.did_commit(world)
                && at == payment.site()
            {
                let name = material_name(item);
                let detail = format!(
                    "{:?} at {at:?}, consumed {} {name}, at least {} traced from local deposits",
                    actor.kind, previous.amount, previous.local_material,
                );
                if previous.local_material > 0 {
                    self.note(
                        host,
                        out,
                        &format!("local-{name}-used-in-construction"),
                        &detail,
                    )?;
                }
                if let Payment::House {
                    at: RECLAIMED_DOOR,
                    becomes,
                } = payment
                {
                    match becomes {
                        HousingPhase::Foundation => {
                            self.note(host, out, "house-foundation-payment", &detail)?
                        }
                        HousingPhase::Frame => {
                            self.note(host, out, "house-frame-payment", &detail)?
                        }
                        _ => {}
                    }
                }
            }
            if previous.unloading
                && amount == 0
                && let Some((name, source)) = previous.source
            {
                let slot = match name {
                    "wood" => 0,
                    "stone" => 1,
                    "fibre" => 2,
                    "rope" => 3,
                    _ => unreachable!("unknown landscape material"),
                };
                if inventory[slot] == self.inventory[slot] + previous.amount {
                    credited[slot] += previous.amount;
                    let label = if name == "rope" {
                        "rope-credited".to_owned()
                    } else {
                        format!("credited-{name}")
                    };
                    self.note(
                        host,
                        out,
                        &label,
                        &format!(
                            "{source:?} delivered by {:?} at {at:?}, warehouse {} -> {}",
                            actor.kind, self.inventory[slot], inventory[slot]
                        ),
                    )?;
                }
            }
            if amount > 0 && previous.amount == amount && previous.product == product {
                hands.source = previous.source;
                hands.local_material = previous.local_material;
            } else if let Some(slot) = product.and_then(tracked_stock)
                && amount > 0
                && self.inventory[slot].saturating_sub(inventory[slot]) == amount
            {
                // Once material is pooled, credit only the lower bound that
                // has no other origin. Existing or unattributed purchases
                // are always counted on the other side of the same stock.
                let unrelated = self.inventory[slot].saturating_sub(self.locally_sourced[slot]);
                hands.local_material = amount.saturating_sub(unrelated);
            }
            if previous.twisting && actor.carried == Some(Item::Rope) && amount == 1 {
                hands.source = Some(("rope", TilePos::new(45, 13)));
                self.note(
                    host,
                    out,
                    "rope-made",
                    &format!("1 rope held by {:?} at {at:?}", actor.kind),
                )?;
            }
            if let Some(bed) = hands.grain
                && actor.carried == Some(Item::Crops)
                && amount == 3
                && world.plot_stage(bed) == Some(PlotStage::Tilled)
            {
                self.note(
                    host,
                    out,
                    &format!("field-{bed}-yield"),
                    &format!("{:?} carrying 3 new crops at {at:?}", actor.kind),
                )?;
                hands.grain = None;
            }
            if at == FIELD_STANDS[0] || at == FIELD_STANDS[1] {
                let bed = WILLOW_VILLAGE.crops.len() + usize::from(at == FIELD_STANDS[1]);
                if actor.activity == Activity::Harvest
                    && actor.pose == Pose::Work
                    && world.plot_stage(bed) == Some(PlotStage::Mature)
                {
                    hands.grain = Some(bed);
                }
            }
            if let Some((name, tile, expected, quantity)) = hands.pickup
                && product == expected
                && amount == quantity
            {
                let id = objects[tile.index().expect("work tile").0 as usize].expect("work object");
                if world.scenic_resource(id).is_some_and(|state| {
                    matches!(state.stage, ScenicStage::Stump | ScenicStage::Cleared)
                }) {
                    hands.source = Some((name, tile));
                    hands.pickup = None;
                }
            }
            if let Some(tile) = world.scenic_work_target(actor.kind) {
                let id = objects[tile.index().expect("compiled coordinate").0 as usize]
                    .expect("work target must be an original scenic object");
                let object = &WILLOW_VILLAGE.scenic[id];
                let name = match object.kind {
                    ScenicKind::Tree => Some(("wood", "cut-tree", Item::Wood)),
                    ScenicKind::Rock => Some(("stone", "strike-stone", Item::Stone)),
                    ScenicKind::Grass => Some(("fibre", "cut-grass", Item::Rope)),
                    _ => None,
                };
                if let Some((name, cut, product)) = name {
                    let detail = format!(
                        "{} at {tile:?}, worker at {at:?}, tool {:?}",
                        object.id, actor.tool
                    );
                    if actor.pose == Pose::Work {
                        match actor.activity {
                            Activity::GatherScenic => {
                                self.note(host, out, &format!("pick-up-{name}"), &detail)?;
                                if let Some(view) =
                                    world.scenic_resource(id).filter(|view| view.pending > 0)
                                {
                                    hands.pickup = Some((
                                        name,
                                        tile,
                                        (name != "fibre").then_some(product),
                                        u16::from(view.pending),
                                    ));
                                }
                            }
                            Activity::FellTree | Activity::MineScenic | Activity::ClearScenic
                                if world
                                    .scenic_resource(id)
                                    .is_some_and(|state| state.stage == ScenicStage::Standing) =>
                            {
                                self.note(host, out, cut, &detail)?;
                            }
                            _ => {}
                        }
                    }
                    if (name == "fibre" && fibre > 0)
                        || (name != "fibre" && actor.carried == Some(product))
                    {
                        hands.source = Some((name, tile));
                        hands.pickup = None;
                    }
                }
            }
            if let Some((name, source)) = hands.source {
                if actor.walking_segment().is_some() && actor.pose == Pose::Carry {
                    self.note(
                        host,
                        out,
                        &format!("carry-{name}"),
                        &format!("from {source:?}, worker at {at:?}, {amount} units"),
                    )?;
                }
                if actor.activity == Activity::Store && actor.pose == Pose::Work {
                    hands.unloading = true;
                    self.note(
                        host,
                        out,
                        &format!("store-{name}"),
                        &format!("from {source:?}, unload at {at:?}, {amount} units"),
                    )?;
                }
            }
            if actor.activity == Activity::TwistFibre && actor.pose == Pose::Work {
                self.note(
                    host,
                    out,
                    "twist-rope",
                    &format!("at {at:?}, {fibre} fibre bundles"),
                )?;
            }
            if actor.pose == Pose::Work && amount > 0 {
                if actor.activity == Activity::Build {
                    hands.payment = world.housing_sites().find_map(|house| {
                        if point_tile(house.position) != at {
                            return None;
                        }
                        let becomes = match (house.phase, product) {
                            (HousingPhase::Clear, Some(Item::Stone)) => HousingPhase::Foundation,
                            (HousingPhase::Foundation, Some(Item::Wood)) => HousingPhase::Frame,
                            (HousingPhase::Frame, Some(Item::Wood)) => HousingPhase::Roof,
                            (HousingPhase::Roof, Some(Item::Rope)) => HousingPhase::FitOut,
                            _ => return None,
                        };
                        Some(Payment::House { at, becomes })
                    });
                    if let Some(Payment::House {
                        at: RECLAIMED_DOOR,
                        becomes,
                    }) = hands.payment
                    {
                        let label = match becomes {
                            HousingPhase::Foundation => Some("house-foundation-work"),
                            HousingPhase::Frame => Some("house-frame-work"),
                            _ => None,
                        };
                        if let Some(label) = label {
                            self.note(
                                host,
                                out,
                                label,
                                &format!("{product:?} × {amount} in hands at {at:?}"),
                            )?;
                        }
                    }
                } else if actor.activity == Activity::Roadwork && world.story().active().is_none() {
                    let id = world.story().completed();
                    if let Some(chapter) = CHAPTERS.get(id) {
                        let item = match chapter.material {
                            Material::Wood => Item::Wood,
                            Material::Stone => Item::Stone,
                            Material::Rope => Item::Rope,
                        };
                        if product == Some(item) && amount == chapter.amount {
                            hands.payment = Some(Payment::Chapter { id, at });
                        }
                    }
                }
            }
            if world.farm_store_door() == Some(RECLAIMED_DOOR)
                && actor.pose == Pose::Work
                && actor.carried == Some(Item::Crops)
            {
                let label = match (at, actor.activity) {
                    (RECLAIMED_DOOR, Activity::SortHarvest) => Some("farm-store-sorts"),
                    (RECLAIMED_WORK_YARD, Activity::DryHarvest) => Some("farm-store-dries"),
                    (RECLAIMED_DOOR, Activity::PackHarvest) => Some("farm-store-packs"),
                    (RECLAIMED_DOOR, Activity::Store) => {
                        hands.unloading_crop = true;
                        Some("farm-store-unloads")
                    }
                    _ => None,
                };
                if let Some(label) = label {
                    self.note(host, out, label, &format!("at {at:?}, {amount} crops"))?;
                }
            }
            self.prior[index] = hands;
        }
        self.locally_sourced = std::array::from_fn(|slot| {
            self.locally_sourced[slot]
                .saturating_sub(self.inventory[slot].saturating_sub(inventory[slot]))
                .saturating_add(credited[slot])
                .min(inventory[slot])
        });
        self.inventory = inventory;
        self.crop_stock = crop_stock;
        for (offset, stand) in FIELD_STANDS.into_iter().enumerate() {
            let index = WILLOW_VILLAGE.crops.len() + offset;
            if world.restored_plot_site(index).is_none() {
                continue;
            }
            self.note(
                host,
                out,
                &format!("field-{index}-clear"),
                &format!("terrain available; plot {:?}", world.plot_stage(index)),
            )?;
            for actor in &world.actors()[..2] {
                if world.actor_tile(actor.kind) != stand || actor.pose != Pose::Work {
                    continue;
                }
                let action = match (actor.activity, world.plot_stage(index)) {
                    (Activity::Sow, Some(PlotStage::Tilled)) => "sow",
                    (Activity::Water, Some(PlotStage::Seed | PlotStage::Growing)) => "water",
                    (Activity::Harvest, Some(PlotStage::Mature)) => "harvest",
                    _ => continue,
                };
                self.note(
                    host,
                    out,
                    &format!("field-{index}-{action}"),
                    &format!("work at {stand:?}"),
                )?;
            }
        }
        for house in world.housing_sites() {
            if point_tile(house.position) == RECLAIMED_DOOR {
                self.note(
                    host,
                    out,
                    &format!("house-{}", house.phase.asset_name()),
                    &format!("door {RECLAIMED_DOOR:?}, beds occupied {}", house.occupied),
                )?;
                if house.phase == HousingPhase::MoveIn && !house.occupied {
                    self.note(
                        host,
                        out,
                        "farm-store-commissioned",
                        "no household relocation",
                    )?;
                }
            }
        }
        Ok(())
    }
}

fn verify_reopen(out: &Path, label: &str) -> Result<()> {
    let sample = out.join(label).join(LIVE_SAVE);
    if !sample.is_file() {
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&sample)?)?;
    // Load the saved wall cursor before creating the independent copies.
    let world: LivingWorld = serde_json::from_value(value["world"].clone())?;
    let wall = world.last_wall_ms();
    let mut forks = Vec::new();
    for suffix in ["a", "b"] {
        let dir = out.join(format!("resume-{label}-{suffix}"));
        fs::create_dir(&dir)?;
        fs::copy(&sample, dir.join(LIVE_SAVE))?;
        forks.push(HomeHost::open(dir, wall, false)?);
    }
    assert_eq!(authority(&forks[0])?, authority(&forks[1])?);
    for fork in &mut forks {
        advance(fork, wall + 30 * 60_000)?;
        assert!(fork.world.validate());
        fork.save()?;
    }
    assert_eq!(
        authority(&forks[0])?,
        authority(&forks[1])?,
        "replay differs for {label}"
    );
    println!("reopened {label}; both copies agreed after 30 wall minutes");
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let out = PathBuf::from(args.next().ok_or(
            "working_landscape_probe OUTPUT WALL_HOURS [INPUT_SAVE] [--supply|--garden|--mature|--house|--use-store]",
    )?);
    let hours: u64 = args
        .next()
        .ok_or("missing WALL_HOURS")?
        .to_string_lossy()
        .parse()?;
    let mut input = None;
    let mut work = Vec::from(SUPPLY);
    for arg in args {
        match arg.to_str() {
            Some("--supply") => {}
            Some("--garden") => work.extend(GARDEN),
            Some("--mature") => {
                work.extend(GARDEN);
                work.extend(MATURE);
                work.extend(STORE);
            }
            Some("--house") => {
                work = MATURE.iter().chain(STORE).copied().collect();
            }
            Some("--use-store") => {
                work = STORE.to_vec();
            }
            _ if input.is_none() => input = Some(PathBuf::from(arg)),
            _ => return Err(io::Error::other("expected at most one input save").into()),
        }
    }
    if hours == 0 || hours > 24 * 730 {
        return Err(io::Error::other("expected 1–17520 hours and at most one input save").into());
    }
    fs::create_dir(&out)?;
    let live = out.join("live");
    fs::create_dir(&live)?;
    let wall = if let Some(input) = input {
        let bytes = fs::read(input)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        let world: LivingWorld = serde_json::from_value(value["world"].clone())?;
        fs::write(live.join(LIVE_SAVE), bytes)?;
        world.last_wall_ms()
    } else {
        FRESH_WALL_START
    };
    let mut host = HomeHost::open(live, wall, false)?;
    let mut objects = vec![None; cat_home::map_data::TILE_COUNT];
    for (i, scenic) in WILLOW_VILLAGE.scenic.iter().enumerate() {
        objects[scenic.tile.index().expect("valid scenery").0 as usize] = Some(i);
    }
    let mut observation = Observer::new(&host);
    let deadline = wall
        .checked_add(hours * 3_600_000)
        .ok_or("observation window exceeds wall clock range")?;
    let mut now = wall;
    while now < deadline {
        now = now.saturating_add(1_000).min(deadline);
        advance(&mut host, now)?;
        observation.observe(&host, &out, &objects)?;
    }
    save_observation(&host, &out, "end", &host.world.landscape_summary())?;
    for label in [
        "strike-stone",
        "carry-wood",
        "carry-fibre",
        "house-foundation-payment",
        "farm-store-unloads",
    ] {
        verify_reopen(&out, label)?;
    }
    let missing: Vec<_> = work
        .iter()
        .filter(|&&name| !observation.saved.contains(name))
        .collect();
    if !missing.is_empty() {
        return Err(
            io::Error::other(format!("not observed in this input/window {missing:?}")).into(),
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
