//! Capture actual household and optional plugin states as lossless BMP previews.
#![cfg(windows)]
#![allow(dead_code)]

#[path = "../src/home_host.rs"]
mod home_host;

use cat_home::living_view::{Camera, LivingView};
use cat_home::living_world::{Activity, ActorKind, DAY_MS, Item, LivingWorld, Pose, YardWeather};
use cat_home::plugin_scene::{FrameCommand, SceneRuntime};
use cat_home::plugins::PluginStore;
use cat_home::visitors::{VisitorPose, VisitorState, VisitorVisual};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const MINUTE: u64 = 60_000;

struct Capture {
    out: PathBuf,
    view: LivingView,
    report: String,
}

impl Capture {
    fn frame(
        &mut self,
        name: &str,
        world: &LivingWorld,
        camera: Camera,
        size: (i32, i32),
        commands: &[FrameCommand],
        store: Option<&PluginStore>,
    ) -> io::Result<()> {
        self.frame_living(name, world, camera, size, commands, store, &[])
    }

    fn frame_living(
        &mut self,
        name: &str,
        world: &LivingWorld,
        camera: Camera,
        size: (i32, i32),
        commands: &[FrameCommand],
        store: Option<&PluginStore>,
        visitors: &[VisitorVisual],
    ) -> io::Result<()> {
        let before = serde_json::to_vec(world).map_err(io::Error::other)?;
        self.view
            .render_living(world, &camera, size, commands, store, visitors);
        let after = serde_json::to_vec(world).map_err(io::Error::other)?;
        assert_eq!(before, after, "presentation changed simulation at {name}");
        write_bmp(
            &self.out.join(format!("{name}.bmp")),
            size,
            self.view.pixels(),
        )?;
        let minute = world.minute_of_day();
        writeln!(self.report, "{name}: day {} {:02}:{:02}, logical {}x{}, camera ({:.0},{:.0}), {:?}, {} plugin draw commands", world.world_ms() / DAY_MS + 1, minute / 60, minute % 60, size.0, size.1, camera.x, camera.y, world.weather(), commands.len()).unwrap();
        for actor in world.actors() {
            writeln!(self.report, "  {} ({:.1},{:.1}) {:?}/{:?}, outfit {:?}, tool {:?}, carried {:?} x{}, indoors {}", actor.kind.label(), actor.position.x, actor.position.y, actor.activity, actor.pose, actor.outfit(), actor.tool, actor.carried, actor.carried_amount(), actor.inside()).unwrap();
            if let Some(success) = actor.fishing_result() {
                writeln!(
                    self.report,
                    "  fishing result: {}",
                    if success { "catch" } else { "empty hook" }
                )
                .unwrap();
            }
        }
        for command in commands {
            writeln!(
                self.report,
                "  plugin {} @ ({:.1},{:.1}) depth {}",
                command.frame_id.as_str(),
                command.point.x,
                command.point.y,
                command.depth
            )
            .unwrap();
        }
        for visitor in visitors {
            writeln!(
                self.report,
                "  visitor {} @ ({:.1},{:.1}) {:?} {}",
                visitor.label,
                visitor.position.x,
                visitor.position.y,
                visitor.pose,
                visitor.activity
            )
            .unwrap();
        }
        Ok(())
    }
}

fn write_bmp(path: &Path, size: (i32, i32), bgra: &[u8]) -> io::Result<()> {
    let (width, height) = size;
    let raw_size = (width * height * 4) as u32;
    if bgra.len() != raw_size as usize {
        return Err(io::Error::other("unexpected view buffer size"));
    }
    let mut out = BufWriter::new(File::create(path)?);
    out.write_all(b"BM")?;
    out.write_all(&(54 + raw_size).to_le_bytes())?;
    out.write_all(&[0; 4])?;
    out.write_all(&54_u32.to_le_bytes())?;
    out.write_all(&40_u32.to_le_bytes())?;
    out.write_all(&width.to_le_bytes())?;
    out.write_all(&(-height).to_le_bytes())?;
    out.write_all(&1_u16.to_le_bytes())?;
    out.write_all(&32_u16.to_le_bytes())?;
    out.write_all(&0_u32.to_le_bytes())?;
    out.write_all(&raw_size.to_le_bytes())?;
    out.write_all(&[0; 16])?;
    out.write_all(bgra)?;
    out.flush()
}

fn camera(x: f32, y: f32) -> Camera {
    Camera { x, y, follow: None }
}

fn household(capture: &mut Capture) -> io::Result<()> {
    let mut world = LivingWorld::new_wall_ms(0);
    let start = world.world_ms();
    capture.frame(
        "01-small-open",
        &world,
        camera(56.0, 64.0),
        (160, 100),
        &[],
        None,
    )?;
    let small = capture.view.pixels().to_vec();
    capture.frame(
        "02-large-open",
        &world,
        camera(56.0, 64.0),
        (272, 144),
        &[],
        None,
    )?;
    for y in 0..84 {
        assert_eq!(
            &small[(y * 160 * 4)..(y * 160 + 144) * 4],
            &capture.view.pixels()[(y * 272 * 4)..(y * 272 + 144) * 4]
        );
    }

    let samples: [(&str, (f32, f32), (i32, i32), fn(&LivingWorld) -> bool); 5] = [
        ("03-first-tilling", (120.0, 64.0), (224, 136), |world| {
            world
                .actors()
                .iter()
                .any(|actor| actor.activity == Activity::Till && actor.pose == Pose::Work)
        }),
        ("04-woodcutting", (144.0, 64.0), (208, 144), |world| {
            world
                .actors()
                .iter()
                .any(|actor| actor.activity == Activity::FellTree && actor.pose == Pose::Work)
        }),
        ("05-shared-lunch", (56.0, 64.0), (200, 120), |world| {
            world.actors()[..2]
                .iter()
                .all(|actor| actor.pose == Pose::Eat)
        }),
        ("06-first-harvest", (120.0, 64.0), (224, 136), |world| {
            world
                .actors()
                .iter()
                .any(|actor| actor.activity == Activity::Harvest && actor.pose == Pose::Work)
        }),
        ("07-first-night", (56.0, 64.0), (160, 100), |world| {
            world.minute_of_day() >= 1_300
                && world.actors()[..2]
                    .iter()
                    .all(|actor| actor.inside() && actor.pose == Pose::Sleep)
        }),
    ];
    let mut seen = [false; 5];
    while world.world_ms() < start + 4 * DAY_MS && !seen.iter().all(|&yes| yes) {
        world.advance_world_ms(MINUTE);
        for (i, (name, (x, y), size, predicate)) in samples.iter().enumerate() {
            if !seen[i] && predicate(&world) {
                capture.frame(name, &world, camera(*x, *y), *size, &[], None)?;
                seen[i] = true;
            }
        }
    }
    if !seen.iter().all(|&yes| yes) {
        return Err(io::Error::other(format!(
            "missing household frames: {seen:?}"
        )));
    }
    Ok(())
}

fn basketball(capture: &mut Capture, pack: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let store_path = capture.out.join("preview-pack-store");
    let mut store = PluginStore::open(&store_path, home_host::host_config())?;
    let pack_id = store.inspect(pack)?.manifest.id;
    if store.package(&pack_id).is_none() {
        store.install(pack)?;
    }
    let mut world = LivingWorld::new_wall_ms(0);
    let mut scene = SceneRuntime::default();
    let mut first_court = false;
    let mut first_play = false;
    let mut flight = false;
    while world.world_ms() < 75 * DAY_MS {
        scene.advance(&mut world, &store);
        let commands = scene.visuals(&store, &world);
        let court = commands
            .iter()
            .find(|c| c.frame_id.as_str().contains("court"));
        if let Some(court) = court {
            let ball = commands
                .iter()
                .find(|c| c.frame_id.as_str().contains("ball"));
            let basketball_camera = camera(
                (court.point.x - 44.0).max(0.0),
                (court.point.y - 32.0).max(0.0),
            );
            if !first_court {
                capture.frame(
                    "08-plugin-court",
                    &world,
                    basketball_camera,
                    (176, 128),
                    &commands,
                    Some(&store),
                )?;
                first_court = true;
            }
            if ball.is_some()
                && !first_play
                && commands.iter().any(|c| {
                    c.actor_override.is_some() && c.frame_id.as_str().starts_with("bounce_")
                })
            {
                capture.frame(
                    "09-plugin-play",
                    &world,
                    basketball_camera,
                    (176, 128),
                    &commands,
                    Some(&store),
                )?;
                first_play = true;
            }
            if !flight && let Some(ball) = ball {
                if let Some(shooter) = commands.iter().find(|c| {
                    c.actor_override.is_some() && c.frame_id.as_str().starts_with("shoot_")
                }) && shooter.point.y - ball.point.y > 27.0
                {
                    capture.frame(
                        "10-plugin-ball-motion",
                        &world,
                        basketball_camera,
                        (176, 128),
                        &commands,
                        Some(&store),
                    )?;
                    flight = true;
                }
            }
        }
        if scene.state().made_contacts() > 0 && first_court && first_play && flight {
            break;
        }
        let now = world.world_ms();
        let mut next = world
            .next_update_ms()
            .min(scene.next_update_ms(&world).unwrap_or(u64::MAX))
            .max(now + 1);
        if first_play && !flight {
            next = next.min(now + 2_500);
        }
        world.advance_world_ms(next - now);
    }
    writeln!(
        capture.report,
        "plugin completed {}, contacts {}",
        scene.state().completed(),
        scene.state().made_contacts()
    )
    .unwrap();
    if !(first_court && first_play && flight && scene.state().made_contacts() > 0) {
        return Err(
            io::Error::other("did not observe court, dribble, airborne shot and contact").into(),
        );
    }
    Ok(())
}

fn fishing(capture: &mut Capture) -> io::Result<()> {
    let mut world = LivingWorld::new_wall_ms(0);
    let stock =
        [Item::Food, Item::Crops, Item::Seeds].map(|item| (item, world.stock().count(item)));
    assert!(world.spend_many(&stock));
    writeln!(capture.report, "fishing setup: withdrew the initial meals, crops and seeds once to exercise the low-stock fallback").unwrap();
    let mut seen = [false; 2];
    while world.world_ms() < 30 * DAY_MS && !seen.iter().all(|&yes| yes) {
        let now = world.world_ms();
        world.advance_world_ms(world.next_update_ms().saturating_sub(now));
        let result = world
            .actors()
            .iter()
            .find_map(|actor| actor.fishing_result());
        if let Some(success) = result
            && !seen[usize::from(success)]
        {
            world.advance_world_ms(42_000);
            assert!(
                world
                    .actors()
                    .iter()
                    .any(|actor| actor.fishing_result() == Some(success))
            );
            let name = if success {
                "12-fishing-catch"
            } else {
                "13-fishing-empty-hook"
            };
            capture.frame(name, &world, camera(56.0, 80.0), (160, 108), &[], None)?;
            seen[usize::from(success)] = true;
        }
    }
    if !seen.iter().all(|&yes| yes) {
        return Err(io::Error::other("did not observe both fishing outcomes"));
    }
    Ok(())
}

fn visitors(capture: &mut Capture) -> io::Result<()> {
    let mut world = LivingWorld::new_wall_ms(0);
    let mut state = VisitorState::default();
    let mut pairs = false;
    let mut transaction = false;
    let mut weather = false;
    while world.world_ms() < 30 * DAY_MS && !(pairs && transaction && weather) {
        let now = world.world_ms();
        world.advance_pending_to(now);
        state.advance(&mut world);
        let visuals = state.visuals(&world);
        if !pairs
            && let Some((a, b)) = visuals.iter().enumerate().find_map(|(i, a)| {
                visuals[i + 1..]
                    .iter()
                    .find(|b| {
                        (a.position.x - b.position.x).abs() <= 45.0
                            && (a.position.y - b.position.y).abs() <= 37.0
                            && (a.position.x - b.position.x).abs()
                                + (a.position.y - b.position.y).abs()
                                > 12.0
                    })
                    .map(|b| (a, b))
            })
        {
            let x = ((a.position.x + b.position.x) / 2.0 - 112.0).max(0.0);
            let y = ((a.position.y + b.position.y) / 2.0 - 70.0).max(0.0);
            capture.frame_living(
                "18-two-visitors",
                &world,
                camera(x, y),
                (224, 128),
                &[],
                None,
                &visuals,
            )?;
            pairs = true;
        }
        if !transaction
            && let Some(visitor) = visuals.iter().find(|visitor| {
                if visitor.pose != VisitorPose::Trading {
                    return false;
                }
                let meeting_near = world.actors()[..2].iter().any(|actor| {
                    actor.activity == Activity::OutsideActivity
                        && actor.pose == Pose::Idle
                        && (actor.position.x - visitor.position.x).abs()
                            + (actor.position.y - visitor.position.y).abs()
                            <= 18.0
                });
                let not_crowded = visuals.iter().all(|other| {
                    other.profession == visitor.profession
                        || (other.position.x - visitor.position.x).abs()
                            + (other.position.y - visitor.position.y).abs()
                            >= 24.0
                });
                meeting_near && not_crowded
            })
        {
            let x = (visitor.position.x - 84.0).max(0.0);
            let y = (visitor.position.y - 68.0).max(0.0);
            capture.frame_living(
                "19-visitor-exchange",
                &world,
                camera(x, y),
                (174, 120),
                &[],
                None,
                &visuals,
            )?;
            transaction = true;
        }
        if !weather
            && world.weather() == YardWeather::Rain
            && (7 * 60..17 * 60).contains(&world.minute_of_day())
        {
            capture.frame_living(
                "20-daytime-rain",
                &world,
                camera(56.0, 64.0),
                (224, 136),
                &[],
                None,
                &visuals,
            )?;
            weather = true;
        }
        let deadline = world
            .next_update_ms()
            .min(state.next_update_ms(&world).unwrap_or(u64::MAX))
            .max(now + 1);
        world.advance_world_ms(deadline.saturating_sub(now));
    }
    if !(pairs && transaction && weather) {
        return Err(io::Error::other(format!(
            "missing visitor/weather capture ({pairs}, {transaction}, {weather})"
        )));
    }
    Ok(())
}

fn year(capture: &mut Capture) -> io::Result<()> {
    let mut world = LivingWorld::new_wall_ms(0);
    let start = world.world_ms();
    let started = Instant::now();
    let mut largest = 0;
    let mut completed = 0;
    for day in 1..=365 {
        world.advance_world_ms(DAY_MS);
        while world.is_catching_up() {
            world.advance_world_ms(0);
        }
        assert!(world.validate());
        let saved = serde_json::to_vec(&world).map_err(io::Error::other)?;
        largest = largest.max(saved.len());
        while completed < world.story().completed() {
            let chapter = &cat_home::living_world::story::CHAPTERS[completed];
            completed += 1;
            writeln!(
                capture.report,
                "development checkpoint day {day:>3}: {completed:02} {}",
                chapter.title
            )
            .unwrap();
            if let Some((name, x, y)) = match completed {
                3 => Some(("14-road-open", 0.0, 136.0)),
                6 => Some(("15-bridge-open", 49.0, 167.0)),
                9 => Some(("16-market-open", 81.0, 179.0)),
                24 => Some(("17-south-village", 100.0, 174.0)),
                _ => None,
            } {
                capture.frame(name, &world, camera(x, y), (278, 147), &[], None)?;
            }
        }
        if [1, 7, 30, 90, 180, 365].contains(&day) {
            writeln!(capture.report, "year day {day:>3}: food={}, crops={}, seeds={}, water={}, wood={}, stones={}, fish={}, building levels {:?}, hunger {:?}", world.stock().count(Item::Food), world.stock().count(Item::Crops), world.stock().count(Item::Seeds), world.stock().count(Item::Water), world.stock().count(Item::Wood), world.stock().count(Item::Stone), world.stock().count(Item::Fish), world.buildings().iter().map(|building| building.level).collect::<Vec<_>>(), world.actors().iter().map(|actor| (actor.kind, actor.hunger())).collect::<Vec<(ActorKind, u8)>>()).unwrap();
        }
    }
    assert_eq!(world.world_ms(), start + 365 * DAY_MS);
    assert_eq!(completed, cat_home::living_world::story::CHAPTER_COUNT);
    writeln!(capture.report, "year development: {}, elapsed-season milestones {:04b}, exchanges {}, spoiled units {}, garden maintenance {}", completed, world.story().milestones(), world.story().market_trades(), world.story().waste(), world.story().garden_visits()).unwrap();
    writeln!(
        capture.report,
        "year run: {:?} elapsed, max daily core JSON {} bytes",
        started.elapsed(),
        largest
    )
    .unwrap();
    capture.frame(
        "11-year-end",
        &world,
        camera(56.0, 64.0),
        (280, 144),
        &[],
        None,
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut out = None;
    let mut pack = None;
    let mut run_year = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--out") => out = Some(PathBuf::from(required_value(&mut args, "--out")?)),
            Some("--pack") => pack = Some(PathBuf::from(required_value(&mut args, "--pack")?)),
            Some("--year") => run_year = true,
            _ => {
                return Err(io::Error::other(format!(
                    "unknown argument: {}",
                    arg.to_string_lossy()
                ))
                .into());
            }
        }
    }
    let out = out.ok_or_else(|| {
        io::Error::other("usage: living_preview --out DIRECTORY [--pack FILE] [--year]")
    })?;
    fs::create_dir_all(&out)?;
    let mut capture = Capture {
        out,
        view: LivingView::new(),
        report: String::new(),
    };
    household(&mut capture)?;
    if let Some(pack) = pack {
        basketball(&mut capture, &pack)?;
    }
    if run_year {
        year(&mut capture)?;
    }
    fishing(&mut capture)?;
    visitors(&mut capture)?;
    fs::write(capture.out.join("captures.txt"), capture.report)?;
    Ok(())
}

fn required_value(args: &mut impl Iterator<Item = OsString>, flag: &str) -> io::Result<OsString> {
    args.next()
        .ok_or_else(|| io::Error::other(format!("missing value for {flag}")))
}
