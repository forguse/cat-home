//! Render the staged two-texel art through the actual viewport and sampling path.
#![cfg(windows)]
#![allow(dead_code)]

use cat_home::living_view::{Camera, LivingView, premium_source_size};
use cat_home::living_world::{
    Activity, ActorKind, DAY_MS, HousePlan, HousingPhase, Item, LivingWorld, PlotStage, Pose,
};
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::time::Instant;

fn frame(
    out: &Path,
    name: &str,
    view: &mut LivingView,
    world: &LivingWorld,
    size: (i32, i32),
    camera: Camera,
) -> io::Result<()> {
    let before = serde_json::to_vec(world).map_err(io::Error::other)?;
    view.render_premium(world, &camera, size, &[], None, &[]);
    assert_eq!(before, serde_json::to_vec(world).map_err(io::Error::other)?);
    let source = premium_source_size(size);
    let rgba = view.pixels();
    assert_eq!(rgba.len(), (source.0 * source.1 * 4) as usize);
    let mut file = BufWriter::new(File::create(out.join(format!("{name}.pam")))?);
    writeln!(
        file,
        "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR",
        size.0, size.1
    )?;
    let mut row = Vec::with_capacity(size.0 as usize * 4);
    for y in 0..size.1 {
        row.clear();
        for x in 0..size.0 {
            let (sx, sy) = (cat_home::viewport::source_coordinate(x), cat_home::viewport::source_coordinate(y));
            let start = ((sy * source.0 + sx) * 4) as usize;
            let pixel = &rgba[start..start + 4];
            let alpha = u16::from(pixel[3]);
            for channel in [2, 1, 0] {
                let value = if alpha == 0 {
                    0
                } else {
                    (u16::from(pixel[channel]) * 255 + alpha / 2) / alpha
                };
                row.push(value as u8);
            }
            row.push(pixel[3]);
        }
        file.write_all(&row)?;
    }
    file.flush()
}

fn benchmark(view: &mut LivingView, world: &LivingWorld, camera: &Camera) {
    for size in [(480, 300), (600, 375), (900, 480)] {
        for premium in [false, true] {
            let mut samples = [0; 120];
            let mut changes = 0;
            let mut test_world = world.clone();
            let mut step = || {
                test_world.advance_world_ms(1_600);
                let start = Instant::now();
                let changed = if premium {
                    view.render_premium(&test_world, camera, size, &[], None, &[])
                } else {
                    view.render(&test_world, camera, (size.0 / 3, size.1 / 3), &[], None)
                };
                (start.elapsed().as_micros(), changed.is_some())
            };
            for _ in 0..8 {
                step();
            }
            for sample in &mut samples {
                let (elapsed, changed) = step();
                *sample = elapsed;
                changes += u32::from(changed);
            }
            samples.sort_unstable();
            println!(
                "{}x{} {} source: p50={}µs p95={}µs max={}µs changes={}/{}",
                size.0,
                size.1,
                if premium { "2x" } else { "1x" },
                samples[59],
                samples[113],
                samples[119],
                changes,
                samples.len(),
            );
        }
    }
}

fn action_frames(out: &Path, view: &mut LivingView, camera: Camera) -> io::Result<()> {
    let mut world = LivingWorld::new(0);
    let mut captured = [false; 5];
    for _ in 0..(21 * 1440) {
        world.advance_world_ms(60_000);
        let people = &world.actors()[..2];
        let is_work = |activity| {
            people
                .iter()
                .any(|actor| actor.pose == Pose::Work && actor.activity == activity)
        };
        let orange = &world.actors()[2];
        let scenes = [
            ("11-cook", is_work(Activity::Cook)),
            (
                "12-shared-meal",
                people.iter().all(|actor| actor.pose == Pose::Eat),
            ),
            ("16-cat-drinks", orange.pose == Pose::Drink),
            ("17-cat-rests", orange.pose == Pose::Rest),
            ("24-fill-cat-bowl", is_work(Activity::FillBowl)),
        ];
        for (index, (name, matches)) in scenes.into_iter().enumerate() {
            if !captured[index] && matches {
                frame(out, name, view, &world, (720, 420), camera)?;
                captured[index] = true;
            }
        }
        if captured.iter().all(|&done| done) {
            break;
        }
    }
    if captured.iter().any(|&done| !done) {
        eprintln!("Some autonomous action frames were not observed: {captured:?}");
    }
    // Run the actual fishing workflow under its low-stock trigger. Ordinary
    // first-year households can keep fishing deferred when crops cover meals.
    let mut world = LivingWorld::new(0);
    for item in [Item::Food, Item::Crops, Item::Seeds] {
        let initial = world.stock().count(item);
        assert!(world.spend_stock(item, initial));
    }
    let mut caught = [false; 3];
    for _ in 0..(21 * 1440) {
        world.advance_world_ms(60_000);
        let people = &world.actors()[..2];
        let frames = [
            (
                "13-fishing-low-stock",
                people
                    .iter()
                    .any(|actor| actor.pose == Pose::Work && actor.activity == Activity::Fish),
            ),
            (
                "14-fish-caught-low-stock",
                people
                    .iter()
                    .any(|actor| actor.fishing_result() == Some(true)),
            ),
            (
                "15-fish-empty-low-stock",
                people
                    .iter()
                    .any(|actor| actor.fishing_result() == Some(false)),
            ),
        ];
        for (index, (name, matches)) in frames.into_iter().enumerate() {
            if !caught[index] && matches {
                frame(out, name, view, &world, (720, 420), camera)?;
                caught[index] = true;
            }
        }
        if caught.iter().all(|&done| done) {
            break;
        }
    }
    if caught.iter().any(|&done| !done) {
        eprintln!("Some low-stock fishing frames were not observed: {caught:?}");
    }
    Ok(())
}

fn upgrade_frames(out: &Path, view: &mut LivingView, camera: Camera) -> io::Result<()> {
    let mut world = LivingWorld::new(0);
    let mut captured = [false; 4];
    let mut building_levels = [[false; 2]; 5];
    let names = ["west", "east", "kitchen", "storehouse", "workshop"];
    for _ in 0..(366 * 24) {
        world.advance_world_ms(60 * 60_000);
        let work = world
            .buildings()
            .iter()
            .any(|b| b.level >= 2 && b.progress > 0);
        let first_upgrade = world.buildings().iter().any(|b| b.level >= 3);
        let final_upgrade = world.buildings().iter().any(|b| b.level == 4);
        let fixture = world.building_fixtures().next().is_some();
        let frames = [
            ("18-house-upgrade-work", work),
            ("19-house-level-3", first_upgrade),
            ("20-house-level-4", final_upgrade),
            ("21-installed-fixture", fixture),
        ];
        for (index, (name, matches)) in frames.into_iter().enumerate() {
            if !captured[index] && matches {
                frame(out, name, view, &world, (900, 420), camera)?;
                captured[index] = true;
            }
        }
        for (index, building) in world.buildings().iter().enumerate() {
            if (3..=4).contains(&building.level) && building.progress == 0 {
                let offset = usize::from(building.level - 3);
                if !building_levels[index][offset] {
                    let name = format!("23-{}-level-{}", names[index], building.level);
                    frame(out, &name, view, &world, (900, 420), camera)?;
                    building_levels[index][offset] = true;
                }
            }
        }
        if captured.iter().all(|&done| done) && building_levels.iter().flatten().all(|&done| done) {
            break;
        }
    }
    if captured.iter().any(|&done| !done) {
        eprintln!("Some natural structural frames were not observed: {captured:?}");
    }
    if building_levels.iter().flatten().any(|&done| !done) {
        eprintln!("Some individual building level frames were not observed: {building_levels:?}");
    }
    Ok(())
}

fn housing_frames(out: &Path, view: &mut LivingView) -> io::Result<()> {
    let mut world = LivingWorld::new(0);
    let mut captured = [[false; 6]; 2];
    for _ in 0..(366 * 12) {
        world.advance_world_ms(2 * 60 * 60_000);
        for site in world.housing_sites() {
            let plan = usize::from(site.kind == HousePlan::Shared);
            let phase = match site.phase {
                HousingPhase::Clear => 0,
                HousingPhase::Foundation => 1,
                HousingPhase::Frame => 2,
                HousingPhase::Roof => 3,
                HousingPhase::FitOut => 4,
                HousingPhase::MoveIn => 5,
            };
            if !captured[plan][phase] {
                let camera = Camera {
                    x: (site.position.x - 67.0).max(0.0),
                    y: (site.lot_origin.y - 32.0).max(0.0),
                    follow: None,
                };
                let name = format!("22-{}-{}", site.kind.asset_name(), site.phase.asset_name());
                frame(out, &name, view, &world, (480, 330), camera)?;
                if site.phase == HousingPhase::MoveIn && site.occupied {
                    let mut follow = Camera::default();
                    let owner = if world.home_of(ActorKind::Xiaohe) == Some(site.position) {
                        0
                    } else {
                        1
                    };
                    follow.set_follow(Some(owner));
                    let mut arrival = world.clone();
                    for _ in 0..(2 * 1440) {
                        let actor = &arrival.actors()[owner as usize];
                        if !actor.inside()
                            && (actor.position.x - site.position.x).abs() < 14.0
                            && (actor.position.y - site.position.y).abs() < 12.0
                        {
                            break;
                        }
                        arrival.advance_world_ms(60_000);
                    }
                    for _ in 0..160 {
                        follow.update(&arrival, (160, 100));
                    }
                    let name = format!("25-{}-follow-480", site.kind.asset_name());
                    frame(out, &name, view, &arrival, (480, 300), follow)?;
                }
                captured[plan][phase] = true;
            }
        }
        if captured.iter().flatten().all(|&done| done) {
            break;
        }
    }
    if captured.iter().flatten().any(|&done| !done) {
        eprintln!("Some natural housing phases were not observed: {captured:?}");
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let atlas = args.next().ok_or_else(|| {
        io::Error::other("usage: premium_preview ATLAS_DIR OUTPUT_DIR [--benchmark]")
    })?;
    let out = args
        .next()
        .ok_or_else(|| io::Error::other("missing output path"))?;
    let (atlas, out) = (Path::new(&atlas), Path::new(&out));
    let index = fs::read(atlas.join("index.json"))?;
    let metadata: serde_json::Value = serde_json::from_slice(&index)?;
    let count = metadata
        .get("pages")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    if count == 0 || count > 16 {
        return Err(io::Error::other("invalid atlas page count").into());
    }
    let mut pages: Vec<&'static [u8]> = Vec::with_capacity(count);
    for page in 0..count {
        let bytes = fs::read(atlas.join(format!("page_{page:02}.bgra")))?;
        pages.push(Box::leak(bytes.into_boxed_slice()));
    }
    let mut view = LivingView::new();
    view.set_premium_assets(&index, Box::leak(pages.into_boxed_slice()))
        .map_err(io::Error::other)?;
    fs::create_dir_all(out)?;
    let mut world = LivingWorld::new(0);
    let opening = Camera::default();
    if args.next().as_deref() == Some(std::ffi::OsStr::new("--benchmark")) {
        benchmark(&mut view, &world, &opening);
    }
    frame(
        out,
        "01-opening-480",
        &mut view,
        &world,
        (480, 300),
        opening,
    )?;
    frame(
        out,
        "02-opening-600",
        &mut view,
        &world,
        (600, 375),
        opening,
    )?;
    frame(
        out,
        "06-centered-600",
        &mut view,
        &world,
        (600, 375),
        Camera {
            x: 64.0,
            y: 59.0,
            follow: None,
        },
    )?;
    frame(
        out,
        "03-yard-900",
        &mut view,
        &world,
        (900, 480),
        Camera {
            x: 34.0,
            y: 47.0,
            follow: None,
        },
    )?;
    frame(
        out,
        "26-orchard-480",
        &mut view,
        &world,
        (480, 300),
        Camera {
            x: 225.0,
            y: 110.0,
            follow: None,
        },
    )?;
    world.advance_world_ms(24 * 60_000);
    frame(
        out,
        "04-garden-work",
        &mut view,
        &world,
        (660, 390),
        Camera {
            x: 91.0,
            y: 61.0,
            follow: None,
        },
    )?;
    let garden = Camera {
        x: 91.0,
        y: 53.0,
        follow: None,
    };
    let mut beds = world.clone();
    let mut crops_captured = [false; 2];
    for _ in 0..(3 * 1440) {
        beds.advance_world_ms(60_000);
        for (i, (name, stage)) in [
            ("10-crop-growth", PlotStage::Growing),
            ("10-crop-mature", PlotStage::Mature),
        ]
        .into_iter()
        .enumerate()
        {
            if !crops_captured[i] && beds.plots().iter().any(|plot| plot.stage == stage) {
                frame(out, name, &mut view, &beds, (660, 390), garden)?;
                crops_captured[i] = true;
            }
        }
        if crops_captured.iter().all(|&done| done) {
            break;
        }
    }
    world.advance_world_ms((13 * 60 + 9) * 60_000);
    while world.is_catching_up() {
        world.advance_world_ms(0);
    }
    frame(out, "05-night", &mut view, &world, (600, 375), opening)?;
    world.advance_world_ms(365 * DAY_MS);
    while world.is_catching_up() {
        world.advance_world_ms(0);
    }
    frame(
        out,
        "07-year-southern-market",
        &mut view,
        &world,
        (900, 420),
        Camera {
            x: 47.0,
            y: 176.0,
            follow: None,
        },
    )?;
    let mut clipped = Camera {
        x: 76.0,
        y: 88.0,
        follow: Some(0),
    };
    let opening_world = LivingWorld::new(0);
    for _ in 0..16 {
        clipped.update(&opening_world, (336, 91));
    }
    frame(
        out,
        "08-restored-1007x272",
        &mut view,
        &opening_world,
        (1007, 272),
        clipped,
    )?;
    let mut minimum = opening;
    minimum.fit_viewport((80, 60));
    frame(
        out,
        "09-minimum-240x180",
        &mut view,
        &opening_world,
        (240, 180),
        minimum,
    )?;
    action_frames(out, &mut view, opening)?;
    upgrade_frames(out, &mut view, opening)?;
    housing_frames(out, &mut view)?;
    Ok(())
}
