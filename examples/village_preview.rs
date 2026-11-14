//! Native-source village capture. Reads optional probe saves without writing them.
#[cfg(windows)]
#[allow(dead_code)]
#[path = "../src/home_host.rs"]
mod home_host;

use cat_home::companionship::REWARDS;
use cat_home::living_view::{Camera, LivingView, VillageCamera, premium_source_size};
use cat_home::living_world::{DAY_MS, LivingWorld};
use cat_home::map_data::WILLOW_VILLAGE;
use cat_home::residents::{TripStage, VillageLife};
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::time::Duration;

#[derive(serde::Deserialize)]
struct SavedWorld {
    world: LivingWorld,
    life: VillageLife,
}

fn capture(
    out: &Path,
    name: &str,
    view: &mut LivingView,
    world: &LivingWorld,
    life: &VillageLife,
    size: (i32, i32),
    camera: Camera,
) -> io::Result<()> {
    let before = serde_json::to_vec(world).map_err(io::Error::other)?;
    view.set_companionship_rewards(life.companionship_received());
    view.set_village_public_corner(life.social_public_corner());
    view.set_resident_presence(life.residents());
    use cat_home::living_world::ActorKind;
    view.set_village_household_payload([
        life.household_held(ActorKind::Xiaohe),
        life.household_held(ActorKind::Ayu),
    ]);
    view.render_village(
        world,
        &camera,
        size,
        &[],
        None,
        &life.visuals(world),
        &life.wildlife_visuals(world),
        &life.stall_visuals(world),
    );
    assert_eq!(before, serde_json::to_vec(world).map_err(io::Error::other)?);
    let source = premium_source_size(size);
    let mut file = BufWriter::new(File::create(out.join(format!("{name}.pam")))?);
    writeln!(
        file,
        "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR",
        size.0, size.1
    )?;
    let rgba = view.pixels();
    let decorated = std::env::var_os("CAT_HOME_CAPTURE_FRAME").map(|_| {
        let mut physical = Vec::with_capacity((size.0 * size.1) as usize);
        for y in 0..size.1 {
            let sy = cat_home::viewport::source_coordinate(y);
            for x in 0..size.0 {
                let sx = cat_home::viewport::source_coordinate(x);
                let start = ((sy * source.0 + sx) * 4) as usize;
                physical.push(u32::from_le_bytes(
                    rgba[start..start + 4].try_into().unwrap(),
                ));
            }
        }
        cat_home::widget_frame::decorate(&mut physical, size, [0, 0, size.0, size.1]);
        physical
    });
    let mut row = Vec::with_capacity(size.0 as usize * 4);
    for y in 0..size.1 {
        row.clear();
        for x in 0..size.0 {
            let sx = cat_home::viewport::source_coordinate(x);
            let sy = cat_home::viewport::source_coordinate(y);
            let start = ((sy * source.0 + sx) * 4) as usize;
            let decorated_pixel;
            let p = if let Some(physical) = &decorated {
                decorated_pixel = physical[(y * size.0 + x) as usize].to_le_bytes();
                &decorated_pixel
            } else {
                &rgba[start..start + 4]
            };
            let alpha = u16::from(p[3]);
            for c in [2, 1, 0] {
                row.push(if alpha == 0 {
                    0
                } else {
                    ((u16::from(p[c]) * 255 + alpha / 2) / alpha).min(255) as u8
                });
            }
            row.push(p[3]);
        }
        file.write_all(&row)?;
    }
    file.flush()
}

fn place(id: &str, size: (i32, i32), world: &LivingWorld) -> Camera {
    let p = WILLOW_VILLAGE.poi(id).expect("known site").tile;
    let mut camera = Camera {
        x: f32::from(p.x) * 8.0 - size.0 as f32 / cat_home::viewport::WORLD_PIXEL as f32 * 0.5,
        y: f32::from(p.y) * 8.0 - size.1 as f32 / cat_home::viewport::WORLD_PIXEL as f32 * 0.6,
        follow: None,
    };
    camera.clamp_in_world(world, cat_home::viewport::logical_size(size));
    camera
}

fn capture_saved_day(out: &Path, path: &Path) -> io::Result<()> {
    use cat_home::living_world::Pose;

    let source = fs::read(path)?;
    let SavedWorld {
        mut world,
        mut life,
    } = serde_json::from_slice(&source).map_err(io::Error::other)?;
    let (mut view, mut auto, mut camera) =
        (LivingView::new(), VillageCamera::new(), Camera::default());
    view.enable_builtin_premium().map_err(io::Error::other)?;
    let size = (481, 272);
    let start = world.world_ms();
    let end = start + DAY_MS;
    world.queue_world_time(DAY_MS);
    let mut sleep_captured = false;
    let mut indoor_captured = false;
    let mut exterior_captured = false;
    let mut previous = start;
    for _ in 0..300_000 {
        let now = world.world_ms();
        world.set_external_occupied(&life.occupied_tiles());
        if !world.advance_pending_to(now) || world.navigation_pending() {
            continue;
        }
        life.advance(&mut world);
        auto.update_physical(
            &mut camera,
            &world,
            size,
            Duration::from_millis(now.saturating_sub(previous) / 48),
            life.trip_visual(&world),
        );
        previous = now;
        let living = &world.actors()[..2];
        let any_awake = living.iter().any(|actor| actor.pose != Pose::Sleep);
        let any_outdoor = living
            .iter()
            .any(|actor| actor.pose != Pose::Sleep && !actor.inside());
        let sample = if !any_awake && !sleep_captured {
            sleep_captured = true;
            Some("sleep")
        } else if any_awake && !any_outdoor && !indoor_captured {
            indoor_captured = true;
            Some("awake-indoor")
        } else if any_outdoor && !exterior_captured {
            exterior_captured = true;
            Some("awake-outside")
        } else {
            None
        };
        if let Some(sample) = sample {
            let id = format!("saved-day-{}-{sample}", now / DAY_MS);
            capture(out, &id, &mut view, &world, &life, size, camera)?;
            println!(
                "{id} minute={} camera=({:.1},{:.1})",
                world.minute_of_day(),
                camera.x,
                camera.y
            );
        }
        if now >= end || (sleep_captured && indoor_captured && exterior_captured) {
            break;
        }
        let next = world
            .next_update_ms()
            .min(life.next_update_ms(&world).unwrap_or(end))
            .min(end)
            .max(now + 1);
        let _ = world.advance_pending_to(next);
    }
    assert_eq!(source, fs::read(path)?);
    if !exterior_captured {
        return Err(io::Error::other(
            "no awake outside scene in the selected replay day",
        ));
    }
    println!(
        "saved replay capture: sleep={sleep_captured} indoor={indoor_captured} outside={exterior_captured}"
    );
    Ok(())
}

fn capture_town(out: &Path, source: Option<&Path>) -> io::Result<()> {
    let (world, life) = if let Some(path) = source {
        let SavedWorld { world, life } =
            serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
        (world, life)
    } else {
        (LivingWorld::new_village_wall_ms(0), VillageLife::default())
    };
    let days = world.town_real_days();
    let mut view = LivingView::new();
    view.enable_builtin_premium().map_err(io::Error::other)?;
    let size = (720, 340);
    for (name, id) in [
        ("north", "residence_seed_merchant.door"),
        ("market", "stall_seed.door"),
        ("east", "east_duplex_1.door"),
        ("public", "commons_hall.door"),
        ("home-maintenance", "home_table"),
    ] {
        let mut camera = place(id, size, &world);
        if name == "market" {
            camera.x += 40.0;
            camera.y += 16.0;
        }
        let image = format!("real-day-{days}-{name}");
        capture(out, &image, &mut view, &world, &life, size, camera)?;
    }
    let size = (720, 530);
    let pond_camera = Camera {
        x: 14.0 * 8.0,
        y: 26.5 * 8.0,
        follow: None,
    };
    capture(
        out,
        "18-east-of-home-pond",
        &mut view,
        &world,
        &life,
        size,
        pond_camera,
    )?;
    println!(
        "town days={days}, era={:?}, built={}/24, world_day={}, table_renewed={}",
        world.town_era(),
        world.story().completed(),
        world.world_ms() / DAY_MS,
        world.town_corner_renewed()
    );
    Ok(())
}

#[cfg(windows)]
fn capture_market_replay(out: &Path, source: &Path) -> io::Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let original = fs::read(source)?;
    use sha3::{Digest, Sha3_256};
    let digest = format!("{:x}", Sha3_256::digest(&original));
    let save: SavedWorld = serde_json::from_slice(&original).map_err(io::Error::other)?;
    let start = save.world.last_wall_ms();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let replay = Path::new("target").join(format!("market-replay-{nonce}"));
    fs::create_dir_all(&replay)?;
    fs::write(replay.join("world-v4.json"), &original)?;
    let mut host = home_host::HomeHost::open(replay, start, false)?;
    let age = host.world.town_real_days();
    let physical = (900, 468);
    let camera = Camera {
        x: 76.5 * 8.0,
        y: 37.25 * 8.0,
        follow: None,
    };
    let in_frame = |p: cat_home::living_world::Point| {
        (camera.x + 8.0
            ..camera.x + physical.0 as f32 / cat_home::viewport::WORLD_PIXEL as f32 - 8.0)
            .contains(&p.x)
            && (camera.y + 21.0
                ..camera.y + physical.1 as f32 / cat_home::viewport::WORLD_PIXEL as f32 - 5.0)
                .contains(&p.y)
    };
    let mut best_score = (0, 0, 0);
    let mut best = None;
    let last_day = host.world.world_ms() / DAY_MS + 9;
    let mut wall = start;
    while host.world.world_ms() / DAY_MS < last_day {
        for _ in 0..10_000 {
            host.tick(wall);
            if !host.world.is_catching_up()
                && !host.world.navigation_pending()
                && !host.life.navigation_pending()
            {
                break;
            }
        }
        if host.world.is_catching_up()
            || host.world.navigation_pending()
            || host.life.navigation_pending()
        {
            return Err(io::Error::other("host market replay did not settle"));
        }
        let stores = host.life.stall_visuals(&host.world);
        let open: Vec<_> = stores
            .iter()
            .filter(|stall| {
                stall.open
                    && stall.site != "home_trade"
                    && in_frame(cat_home::living_world::Point::at_tile(
                        i32::from(stall.tile.x),
                        i32::from(stall.tile.y),
                    ))
            })
            .collect();
        if open.len() >= 2 {
            let people = host.life.visuals(&host.world);
            let visible: Vec<_> = people
                .iter()
                .filter(|person| in_frame(person.position))
                .collect();
            let customers = visible
                .iter()
                .filter(|person| !matches!(person.pose, cat_home::visitors::VisitorPose::Working))
                .count();
            let score = (open.len(), customers, visible.len());
            if score > best_score {
                best_score = score;
                best = Some((
                    host.world.clone(),
                    host.life.clone(),
                    serde_json::json!({
                    "checkpoint": source.to_string_lossy(),
                    "checkpoint_sha3_256": digest,
                    "checkpoint_last_wall_ms": start,
                        "checkpoint_calendar_age": age,
                        "world_day": host.world.world_ms() / DAY_MS,
                        "world_minute": host.world.minute_of_day(),
                        "camera": [camera.x, camera.y],
                        "physical_size": [physical.0, physical.1],
                        "open_stalls": open.iter().map(|stall| serde_json::json!({"id": stall.site, "item": format!("{:?}", stall.item), "quantity": stall.quantity})).collect::<Vec<_>>(),
                        "visible_people": visible.iter().map(|person| serde_json::json!({"profession": person.profession.asset_name(), "pose": format!("{:?}", person.pose), "position": [person.position.x, person.position.y]})).collect::<Vec<_>>(),
                        "simulation": "Production HomeHost tick on isolated byte-identical checkpoint copy; no stock, pose, actor, or phase override.",
                        "capture": "Production LivingView and physical nearest-neighbor sampling from a fixed market view."
                    }),
                ));
            }
        }
        wall += 15_000;
    }
    let (world, life, evidence) = best
        .ok_or_else(|| io::Error::other("no concurrent trading appeared in the sampled market"))?;
    capture(
        out,
        "mature-market-open",
        &mut host.view,
        &world,
        &life,
        physical,
        camera,
    )?;
    fs::write(
        out.join("mature-market-open.json"),
        serde_json::to_vec_pretty(&evidence).map_err(io::Error::other)?,
    )?;
    assert_eq!(original, fs::read(source)?);
    println!("market replay: {evidence}");
    Ok(())
}

fn main() -> io::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let out = args
        .next()
        .ok_or_else(|| io::Error::other("usage: village_preview OUTPUT_DIR"))?;
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    let option = args.next();
    if option
        .as_deref()
        .is_some_and(|option| option == "--saved-world")
    {
        let source = args
            .next()
            .ok_or_else(|| io::Error::other("--saved-world needs a read-only replay path"))?;
        return capture_saved_day(out, Path::new(&source));
    }
    if option
        .as_deref()
        .is_some_and(|option| option == "--town-site")
    {
        let source = args.next();
        return capture_town(out, source.as_deref().map(Path::new));
    }
    #[cfg(windows)]
    if option
        .as_deref()
        .is_some_and(|option| option == "--market-replay")
    {
        let source = args
            .next()
            .ok_or_else(|| io::Error::other("--market-replay needs a checkpoint path"))?;
        return capture_market_replay(out, Path::new(&source));
    }
    let stills = option.as_deref().is_some_and(|option| option == "--stills");
    let mut view = LivingView::new();
    view.enable_builtin_premium().map_err(io::Error::other)?;
    let mut world = LivingWorld::new_village_wall_ms(0);
    let mut life = VillageLife::default();
    let mut auto = VillageCamera::new();
    let mut camera = Camera::default();
    auto.update(
        &mut camera,
        &world,
        (160, 100),
        Duration::ZERO,
        life.trip_visual(&world),
    );
    capture(
        out,
        "01-home-480",
        &mut view,
        &world,
        &life,
        (480, 300),
        camera,
    )?;
    let mut large = VillageCamera::new();
    let mut cam_large = Camera::default();
    large.update(&mut cam_large, &world, (300, 160), Duration::ZERO, None);
    capture(
        out,
        "02-home-900",
        &mut view,
        &world,
        &life,
        (900, 480),
        cam_large,
    )?;
    for (name, id) in [
        ("03-pond", "home_fishing"),
        ("04-bridge", "bridge_wait_w"),
        ("05-north", "residence_seed_merchant.door"),
        ("06-orchard", "orchard_tools"),
        ("07-boundary", "outside_gate"),
    ] {
        capture(
            out,
            name,
            &mut view,
            &world,
            &life,
            (480, 300),
            place(id, (480, 300), &world),
        )?;
    }
    let forest_view = Camera {
        x: 12.0 * 8.0,
        y: 103.0 * 8.0,
        follow: None,
    };
    capture(
        out,
        "11-forest",
        &mut view,
        &world,
        &life,
        (480, 300),
        forest_view,
    )?;
    let mut keepsakes = VillageLife::default();
    keepsakes.update_companionship(REWARDS[6].threshold_ms);
    capture(
        out,
        "13-home-keepsakes-900",
        &mut view,
        &world,
        &keepsakes,
        (900, 480),
        cam_large,
    )?;
    keepsakes.update_companionship(REWARDS[8].threshold_ms);
    let marketside = place("market_notice", (900, 480), &world);
    capture(
        out,
        "14-market-keepsakes-900",
        &mut view,
        &world,
        &keepsakes,
        (900, 480),
        marketside,
    )?;
    if stills {
        return Ok(());
    }
    let end = world.world_ms() + 3 * DAY_MS;
    world.queue_world_time(end - world.world_ms());
    let mut market = false;
    let mut trip = false;
    let mut returning = false;
    let mut seen_custody = [false; 3];
    let mut seen_duck = [false; 2];
    let mut courier = None;
    let mut prior = world.world_ms();
    for _ in 0..300_000 {
        let now = world.world_ms();
        world.set_external_occupied(&life.occupied_tiles());
        if !world.advance_pending_to(now) || world.navigation_pending() {
            continue;
        }
        life.advance(&mut world);
        if !seen_duck.iter().all(|seen| *seen)
            && let Some(duck) = life
                .wildlife_visuals(&world)
                .into_iter()
                .find(|animal| animal.sprite.starts_with("wild_duckling_"))
        {
            let i = usize::from(duck.sprite == "wild_duckling_swim_0");
            if !seen_duck[i] {
                let size = (481, 301);
                let framing = Camera {
                    x: 62.0,
                    y: 230.0,
                    follow: None,
                };
                capture(
                    out,
                    if i == 0 {
                        "20-duck-ashore"
                    } else {
                        "21-duck-swimming"
                    },
                    &mut view,
                    &world,
                    &life,
                    size,
                    framing,
                )?;
                println!("duck: {} at {:?}", duck.sprite, duck.position);
                seen_duck[i] = true;
            }
        }
        let sample = life.trip_visual(&world);
        if let Some(trip) = sample {
            courier = Some(trip.actor);
        }
        if let Some(actor) = courier {
            let cargo = life.household_held(actor);
            let stage = match sample {
                Some(s) if s.stage == TripStage::Pack && cargo.is_none() => {
                    Some((0, "15-before-loading"))
                }
                Some(s) if s.stage == TripStage::Outbound && cargo.is_some() => {
                    Some((1, "16-carrying"))
                }
                None if seen_custody[1] && life.trips() > 0 && cargo.is_none() => {
                    Some((2, "17-after-deposit"))
                }
                _ => None,
            };
            if let Some((i, name)) = stage.filter(|(i, _)| !seen_custody[*i]) {
                let p = world.actors()[actor as usize].position;
                let size = (481, 301);
                let (w, h) = cat_home::viewport::logical_size(size);
                let mut framing = Camera {
                    x: p.x - w as f32 * 0.5,
                    y: p.y - h as f32 * 0.70,
                    follow: None,
                };
                framing.clamp_in_world(&world, (w, h));
                capture(out, name, &mut view, &world, &life, size, framing)?;
                println!(
                    "{name}: at {}, phase={:?}, cargo={cargo:?}, point={p:?}",
                    world.world_ms(),
                    sample.map(|s| s.stage)
                );
                seen_custody[i] = true;
            }
        }
        auto.update(
            &mut camera,
            &world,
            cat_home::viewport::logical_size((481, 301)),
            Duration::from_millis(now.saturating_sub(prior) / 48),
            sample,
        );
        prior = now;
        if !market
            && life
                .stall_visuals(&world)
                .iter()
                .filter(|stall| stall.open)
                .count()
                >= 4
        {
            let size = (480, 300);
            let framing = place("stall_meal.door", size, &world);
            let framing = Camera {
                x: framing.x + 26.0,
                y: framing.y - 24.0,
                ..framing
            };
            capture(
                out,
                "08-market-open",
                &mut view,
                &world,
                &life,
                size,
                framing,
            )?;
            let wide = Camera {
                x: framing.x - 43.0,
                y: framing.y - 24.0,
                ..framing
            };
            capture(
                out,
                "12-market-court-900",
                &mut view,
                &world,
                &life,
                (900, 480),
                wide,
            )?;
            market = true;
        }
        if !trip
            && sample.is_some_and(|trip| {
                trip.stage == TripStage::Outbound
                    && (60.0 * 8.0..66.0 * 8.0).contains(&trip.position.x)
                    && (42.0 * 8.0..43.0 * 8.0).contains(&trip.position.y)
            })
        {
            capture(
                out,
                "09-travel",
                &mut view,
                &world,
                &life,
                (481, 301),
                camera,
            )?;
            trip = true;
        }
        if trip && !returning && sample.is_some_and(|s| s.stage == TripStage::Unload) {
            capture(
                out,
                "10-unloading",
                &mut view,
                &world,
                &life,
                (480, 300),
                camera,
            )?;
            returning = true;
        }
        if now >= end || (market && returning && life.trips() > 0 && sample.is_none()) {
            break;
        }
        world.set_external_occupied(&life.occupied_tiles());
        let next = world
            .next_update_ms()
            .min(life.next_update_ms(&world).unwrap_or(end))
            .min(end)
            .max(now + 1);
        let _ = world.advance_pending_to(next);
    }
    println!(
        "runtime capture: shop={market} journey={trip} unloading={returning} trips={} trades={} events={}",
        life.trips(),
        life.trades(),
        life.events()
    );
    Ok(())
}
