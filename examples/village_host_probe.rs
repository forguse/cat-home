//! Tests the actual Windows adapter, including residents, plugins and saves.
#![cfg(windows)]
#![allow(dead_code)]
#[path = "../src/home_host.rs"]
mod home_host;
use cat_home::living_world::{DAY_MS, TownEra};
use home_host::HomeHost;
use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const START: u64 = 1_790_000_000_000;
fn authority(h: &HomeHost) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&(&h.world, h.scenes.state(), &h.life))?)
}
fn settled(h: &HomeHost) -> bool {
    !h.world.is_catching_up() && !h.world.navigation_pending() && !h.life.navigation_pending()
}
fn advance(h: &mut HomeHost, now: u64, paint: bool) -> Result<usize> {
    advance_measured(h, now, paint, &mut 0)
}

fn advance_measured(
    h: &mut HomeHost,
    now: u64,
    paint: bool,
    worst_tick_us: &mut u128,
) -> Result<usize> {
    let mut prev_world_ms = h.world.world_ms();
    let mut stalled = 0;
    for i in 0.. {
        let tick = Instant::now();
        h.tick(now);
        *worst_tick_us = (*worst_tick_us).max(tick.elapsed().as_micros());
        if paint && i % 101 == 0 {
            let before = authority(h)?;
            h.render_physical((481, 301));
            h.render_physical((1007, 272));
            assert_eq!(before, authority(h)?, "render changed simulation");
        }
        if settled(h) {
            return Ok(i + 1);
        }
        if h.world.world_ms() != prev_world_ms {
            prev_world_ms = h.world.world_ms();
            stalled = 0;
        } else {
            stalled += 1;
            if stalled >= 10_000 {
                break;
            }
        }
    }
    Err(io::Error::other(format!(
        "village stopped advancing at {} of {}",
        h.world.world_ms(),
        h.world.pending_world_target_ms()
    ))
    .into())
}
fn restore(root: &Path) -> Result<()> {
    let dir = root.join("save-restore");
    let mut h = HomeHost::open(dir.clone(), START, false)?;
    h.size = (812, 476);
    h.camera_follow(None);
    advance(&mut h, START + 60 * 60_000, true)?;
    assert!(h.life.trades() > 0, "first market did not trade");
    h.save()?;
    let a = authority(&h)?;
    let file = dir.join("world-v4.json");
    let bytes = fs::read(&file)?;
    assert!(bytes.len() < 256 * 1024);
    drop(h);
    let recovered = HomeHost::open(dir.clone(), START + 60 * 60_000, false)?;
    assert_eq!(a, authority(&recovered)?);
    assert_eq!(recovered.size, (812, 476));
    assert!(!recovered.auto_camera);
    let mut h = recovered;
    let second = root.join("checkpoint");
    fs::create_dir_all(&second)?;
    fs::write(second.join("world-v4.json"), &bytes)?;
    let mut checkpoint = HomeHost::open(second, START + 60 * 60_000, false)?;
    advance(&mut h, START + 100 * 60_000, false)?;
    advance(&mut checkpoint, START + 100 * 60_000, false)?;
    assert_eq!(
        authority(&h)?,
        authority(&checkpoint)?,
        "restored execution diverged"
    );
    // A failed atomic save must retain the existing snapshot and report it.
    let before_failed_write = fs::read(&file)?;
    fs::create_dir(dir.join("world-v4.pending"))?;
    assert!(h.save().is_err());
    assert!(
        fs::read(&file)? == before_failed_write,
        "failed save overwrote previous bytes"
    );
    fs::rename(
        dir.join("world-v4.pending"),
        dir.join("preserved-obstruction"),
    )?;
    h.save()?;
    // A corrupt in-memory candidate must not replace the last readable snapshot.
    let valid_world = h.world.clone();
    let mut invalid_world = serde_json::to_value(&h.world)?;
    invalid_world["actors"][0]["position"]["x"] = 9_999_999.into();
    h.world = serde_json::from_value(invalid_world)?;
    let stable_bytes = fs::read(&file)?;
    assert!(h.save().is_err());
    assert_eq!(fs::read(&file)?, stable_bytes);
    h.world = valid_world;
    let mut invalid: serde_json::Value = serde_json::from_slice(&bytes)?;
    invalid["schema"] = 99.into();
    let bad = root.join("invalid-version");
    fs::create_dir_all(&bad)?;
    let bad_bytes = serde_json::to_vec(&invalid)?;
    fs::write(bad.join("world-v4.json"), &bad_bytes)?;
    assert!(HomeHost::open(bad.clone(), START, false).is_err());
    assert_eq!(fs::read(bad.join("world-v4.json"))?, bad_bytes);
    println!(
        "v4 restore/replay, first market, odd-size paint purity, failure preservation: pass; {} bytes",
        bytes.len()
    );
    Ok(())
}
fn slices(root: &Path) -> Result<()> {
    let mut single = HomeHost::open(root.join("single"), START, true)?;
    let mut sliced = HomeHost::open(root.join("sliced"), START, true)?;
    let target = 90 * 60_000;
    let ticks = advance(&mut single, START + target, false)?;
    for t in (30_000..=target).step_by(30_000) {
        advance(&mut sliced, START + t, false)?;
    }
    if authority(&single)? != authority(&sliced)? {
        fs::write(root.join("single-state.json"), authority(&single)?)?;
        fs::write(root.join("sliced-state.json"), authority(&sliced)?)?;
        return Err(io::Error::other("slice schedules diverged; inspect saved authorities").into());
    }
    println!(
        "slice/large-jump deterministic: pass, {ticks} ticks, trips={}, trades={}",
        single.life.trips(),
        single.life.trades()
    );
    let mut whole = HomeHost::open(root.join("calendar-single"), START, true)?;
    let mut days = HomeHost::open(root.join("calendar-sliced"), START, true)?;
    advance(&mut whole, START + 8 * DAY_MS, false)?;
    for day in 1..=8 {
        let wall = START + day * DAY_MS;
        advance(&mut days, wall - 1, false)?;
        advance(&mut days, wall, false)?;
    }
    assert!(whole.world.town_allows(TownEra::Workshop));
    assert!(days.world.town_allows(TownEra::Workshop));
    assert_eq!(
        authority(&whole)?,
        authority(&days)?,
        "8-day offline slices diverged"
    );
    println!(
        "eight real days through the first calendar permit and one paid workday: slice/replay pass"
    );
    Ok(())
}
fn migration(root: &Path, source: &Path) -> Result<()> {
    let dir = root.join("player-copy");
    fs::create_dir_all(&dir)?;
    let bytes = fs::read(source)?;
    let original: serde_json::Value = serde_json::from_slice(&bytes)?;
    let name = format!("world-v{}.json", original["schema"].as_u64().unwrap());
    let copy = dir.join(name);
    fs::write(&copy, &bytes)?;
    let old: cat_home::living_world::LivingWorld =
        serde_json::from_value(original["world"].clone())?;
    let now = old.last_wall_ms();
    let h = HomeHost::open(dir.clone(), now, false)?;
    assert!(h.world.is_village());
    assert_eq!(h.world.world_ms(), old.world_ms());
    assert_eq!(
        serde_json::to_vec(h.world.stock())?,
        serde_json::to_vec(old.stock())?
    );
    assert_eq!(h.world.story().completed(), old.story().completed());
    h.save()?;
    assert_eq!(fs::read(&copy)?, bytes);
    let before = authority(&h)?;
    drop(h);
    let recovered = HomeHost::open(dir.clone(), now, false)?;
    assert_eq!(before, authority(&recovered)?);
    let mut h = recovered;
    advance(&mut h, now + 5 * 60_000, true)?;
    // Exercise migrated active work across many days, not only the first frame.
    let resumed_wall = now + 24 * 60 * 60_000;
    advance(&mut h, resumed_wall, false)?;
    assert!(h.world.validate() && h.life.validate_for_world(&h.world));
    h.save()?;
    drop(h);
    let loaded = HomeHost::open(dir.clone(), resumed_wall, false)?;
    drop(loaded);
    let compat = HomeHost::open_compat(dir.clone(), now, false)?;
    assert!(!compat.world.is_village());
    compat.save()?;
    assert_eq!(fs::read(&copy)?, bytes);
    println!(
        "actual player clone migrated and resumed through 24 offline hours; original bytes unchanged; explicit old-world recovery: pass"
    );
    Ok(())
}
fn long_run(root: &Path, days: u64) -> Result<()> {
    let dir = root.join("long-run");
    let mut h = HomeHost::open(dir.clone(), START, false)?;
    let mut wall = START;
    let mut max = 0;
    let begin = Instant::now();
    let mut next_report = 30;
    while h.world.world_ms() / DAY_MS < days {
        wall += 60 * 60_000;
        advance(&mut h, wall, false)?;
        assert!(h.world.validate());
        assert!(h.life.validate_for_world(&h.world));
        assert!(h.scenes.validate_for_world(&h.world));
        h.save()?;
        max = max.max(fs::metadata(dir.join("world-v4.json"))?.len());
        let day = h.world.world_ms() / DAY_MS;
        if day >= next_report {
            let completed: Vec<_> = cat_home::chapter_catalog::CHAPTERS
                .iter()
                .filter(|c| {
                    h.world.village_chapter_completed(c.id)
                        || h.life.village_chapter_completed(c.id)
                })
                .map(|c| c.id)
                .collect();
            println!(
                "long-run day={day}, bytes_max={max}, infrastructure={}/24, mainline={}/72 {:?}, trips={}, trades={}",
                h.world.story().completed(),
                completed.len(),
                completed,
                h.life.trips(),
                h.life.trades()
            );
            next_report += 30;
        }
    }
    let before = authority(&h)?;
    drop(h);
    let restored = HomeHost::open(dir, wall, false)?;
    assert_eq!(before, authority(&restored)?);
    assert!(!restored.world.village_chapter_completed("Q72"));
    println!(
        "{days} world days full host: passed, max {max} bytes, elapsed {:?}",
        begin.elapsed()
    );
    Ok(())
}

fn real_calendar(root: &Path, resume: Option<&Path>) -> Result<()> {
    let folder = root.join("real-calendar");
    let first_wall = if let Some(source) = resume {
        let file = fs::read(source)?;
        let saved: serde_json::Value = serde_json::from_slice(&file)?;
        let wall = saved["world"]["last_wall_ms"]
            .as_u64()
            .ok_or_else(|| io::Error::other("checkpoint has no wall clock"))?;
        fs::create_dir_all(&folder)?;
        fs::write(folder.join("world-v4.json"), file)?;
        wall
    } else {
        START
    };
    let mut h = HomeHost::open(folder.clone(), first_wall, false)?;
    let start_time = Instant::now();
    let mut worst_tick_us = 0;
    let mut bytes_max = 0;
    let mut ticks = 0;
    if resume.is_none() {
        let mut max_open_stalls = 0;
        for after in 0..=140 {
            ticks += advance_measured(&mut h, START + after * 60_000, false, &mut worst_tick_us)?;
            let open = h
                .life
                .stall_visuals(&h.world)
                .into_iter()
                .filter(|stall| stall.open)
                .count();
            max_open_stalls = max_open_stalls.max(open);
        }
        assert_eq!(
            max_open_stalls, 4,
            "first week did not operate all four temporary counters"
        );
        assert!(h.life.trades() > 0, "no first-week finite-stock exchange");
        assert_eq!(h.world.town_era(), TownEra::Humble);
        assert_eq!(h.world.story().completed(), 0);
    }
    for era in TownEra::ALL.into_iter().skip(1) {
        let wall = START + era.real_day() * DAY_MS;
        if wall <= first_wall {
            continue;
        }
        ticks += advance_measured(&mut h, wall - 1, false, &mut worst_tick_us)?;
        assert!(!h.world.town_allows(era), "{era:?} became available early");
        h.save()?;
        let before = authority(&h)?;
        let snapshot = fs::read(folder.join("world-v4.json"))?;
        bytes_max = bytes_max.max(snapshot.len());
        let twin_path = root.join(format!("boundary-{}", era.real_day()));
        fs::create_dir_all(&twin_path)?;
        fs::write(twin_path.join("world-v4.json"), snapshot)?;
        let mut restored = HomeHost::open(twin_path, wall - 1, false)?;
        assert_eq!(authority(&restored)?, before);
        ticks += advance_measured(&mut h, wall, false, &mut worst_tick_us)?;
        assert!(
            h.world.town_allows(era),
            "{era:?} failed to open on the boundary"
        );
        assert_eq!(h.world.town_real_days(), era.real_day());
        advance(&mut restored, wall, false)?;
        assert_eq!(
            authority(&h)?,
            authority(&restored)?,
            "{era:?} restart diverged"
        );
        assert!(h.world.validate() && h.life.validate_for_world(&h.world));
        let missing: Vec<_> = cat_home::chapter_catalog::CHAPTERS
            .iter()
            .filter(|c| {
                c.id != "Q72"
                    && !h.world.village_chapter_completed(c.id)
                    && !h.life.village_chapter_completed(c.id)
            })
            .map(|c| c.id)
            .collect();
        println!(
            "real_day={} sim_day={} offered={era:?} attained={:?} projects={}/24 chapters={}/71 missing={missing:?} trades={} bytes_max={bytes_max} ticks={ticks} worst_tick_us={worst_tick_us} elapsed={:?}",
            era.real_day(),
            h.world.world_ms() / DAY_MS,
            h.world.town_era(),
            h.world.story().completed(),
            71 - missing.len(),
            h.life.trades(),
            start_time.elapsed()
        );
    }
    // A day-730 permit still needs a full working visit with actual materials.
    ticks += advance_measured(
        &mut h,
        START + 730 * DAY_MS + 120 * 60_000,
        false,
        &mut worst_tick_us,
    )?;
    assert_eq!(h.world.town_era(), TownEra::Heritage);
    assert_eq!(h.world.story().completed(), 24);
    let missing: Vec<_> = cat_home::chapter_catalog::CHAPTERS
        .iter()
        .filter(|c| {
            c.id != "Q72"
                && !h.world.village_chapter_completed(c.id)
                && !h.life.village_chapter_completed(c.id)
        })
        .map(|c| c.id)
        .collect();
    assert!(
        missing.is_empty(),
        "real-calendar autonomous chapters missing: {missing:?}"
    );
    assert!(
        !h.world.village_chapter_completed("Q72"),
        "synthetic wall elapsed must not grant active time"
    );
    h.save()?;
    bytes_max = bytes_max.max(fs::metadata(folder.join("world-v4.json"))?.len() as usize);
    assert!(bytes_max < 256 * 1024);
    println!(
        "real-calendar 730d: 71/71 attained; heritage paid separately; {} actual trades; {bytes_max} bytes max; {ticks} ticks worst {worst_tick_us} µs elapsed {:?}",
        h.life.trades(),
        start_time.elapsed()
    );
    Ok(())
}

fn profile_offline(root: &Path) -> Result<()> {
    let directory = root.join("offline-profile");
    let mut home = HomeHost::open(directory, START, false)?;
    let first_day = home.world.world_ms() / DAY_MS;
    let calendar_day = START + 8 * DAY_MS;
    let begin = Instant::now();
    let mut last_frame = begin;
    let mut frames = 0;
    let mut tick_us = Vec::new();
    while begin.elapsed() < Duration::from_secs(120) {
        let wake = Instant::now();
        home.tick(calendar_day);
        tick_us.push(wake.elapsed().as_micros());
        if last_frame.elapsed() >= Duration::from_millis(100) {
            home.render_physical((480, 300));
            last_frame = Instant::now();
            frames += 1;
        }
        if settled(&home) {
            break;
        }
        let sleep = Duration::from_millis(if home.offline_backlog() { 16 } else { 100 });
        std::thread::sleep(sleep.saturating_sub(wake.elapsed()));
    }
    assert!(settled(&home) && home.world.validate() && home.life.validate_for_world(&home.world));
    assert!(home.world.town_allows(TownEra::Workshop));
    assert!(!home.world.town_allows(TownEra::Market));
    home.save()?;
    let world_days = home.world.world_ms() / DAY_MS - first_day;
    tick_us.sort_unstable();
    println!(
        "offline ui cadence: 8 real days, {world_days} world days, {} ticks, {frames} 480x300 frames, per tick µs p50={} p95={} max={}, elapsed {:?}",
        tick_us.len(),
        tick_us[tick_us.len() / 2],
        tick_us[tick_us.len() * 95 / 100],
        tick_us.last().unwrap(),
        begin.elapsed()
    );
    Ok(())
}

fn checkpoint_window(root: &Path, source: &Path, hours: u64, welfare: bool) -> Result<()> {
    let folder = root.join("real-checkpoint-window");
    let file = fs::read(source)?;
    let saved: serde_json::Value = serde_json::from_slice(&file)?;
    let wall = saved["world"]["last_wall_ms"]
        .as_u64()
        .ok_or_else(|| io::Error::other("checkpoint has no wall clock"))?;
    fs::create_dir_all(&folder)?;
    fs::write(folder.join("world-v4.json"), file)?;
    let mut home = HomeHost::open(folder, wall, false)?;
    let begin = Instant::now();
    let trades = home.life.trades();
    let first_day = home.world.world_ms() / DAY_MS;
    let mut worst = 0;
    let mut ticks = 0;
    let wellness_interval_seconds = 300;
    let step_seconds = if welfare { 10 } else { 3600 };
    let mut weekly_meal_weeks = std::collections::BTreeSet::new();
    let mut samples = 0;
    let mut critical = [[0; 3]; 3];
    let mut longest_critical = [[0; 3]; 3];
    let mut longest_carry = [0; 3];
    let mut running_carry = [0; 3];
    let mut observed_trades = trades;
    for seconds in (0..=hours * 3600).step_by(step_seconds) {
        ticks += advance_measured(&mut home, wall + seconds * 1_000, false, &mut worst)?;
        if !welfare && home.life.village_chapter_completed("Q67") {
            break;
        }
        if welfare {
            let actual_trades = home.life.trades();
            if actual_trades != observed_trades {
                observed_trades = actual_trades;
                let state = serde_json::to_value(&home.life)?;
                let trip = &state["trip"];
                if trip["regular_meal"].as_bool() == Some(true)
                    && trip["accepted"].as_bool() == Some(true)
                    && let Some(start) = trip["started"].as_u64()
                {
                    weekly_meal_weeks.insert(start / (7 * DAY_MS));
                }
            }
        }
        if welfare && seconds % wellness_interval_seconds == 0 {
            assert!(home.world.validate() && home.life.validate_for_world(&home.world));
            samples += 1;
            for (i, actor) in home.world.actors().iter().enumerate() {
                for (need, pending) in [
                    actor.hunger() >= 95,
                    actor.thirst() >= 95,
                    actor.stamina() == 0,
                ]
                .into_iter()
                .enumerate()
                {
                    critical[i][need] = if pending { critical[i][need] + 1 } else { 0 };
                    longest_critical[i][need] = longest_critical[i][need].max(critical[i][need]);
                }
                running_carry[i] = if actor.carried_amount() > 0 {
                    running_carry[i] + 1
                } else {
                    0
                };
                longest_carry[i] = longest_carry[i].max(running_carry[i]);
            }
        }
    }
    assert!(home.world.validate() && home.life.validate_for_world(&home.world));
    home.save()?;
    if welfare {
        let world = serde_json::to_value(&home.world)?;
        let life = serde_json::to_value(&home.life)?;
        let delta = |before: &serde_json::Value, after: &serde_json::Value, key: &str| {
            after[key].as_u64().unwrap_or(0) - before[key].as_u64().unwrap_or(0)
        };
        let initial_world = &saved["world"];
        let initial_life = &saved["life"];
        let work = &life["social"]["neighbours"];
        println!(
            "welfare: {samples} samples at {} wall minutes, longest critical hunger/thirst/no stamina={longest_critical:?}, longest continuous carry samples={longest_carry:?}",
            wellness_interval_seconds / 60,
        );
        println!(
            "work: meals +{}, drinks +{}, crops +{}, outings +{}, trades +{}, visitor returns +{}; sampled weekly meal sale weeks {weekly_meal_weeks:?}; latest neighbor work farmer={} tailor={}, latest suppers farmer={} tailor={}; stock {}",
            delta(initial_world, &world, "meals"),
            delta(initial_world, &world, "drinks"),
            delta(initial_world, &world, "harvested"),
            delta(initial_life, &life, "trips"),
            delta(initial_life, &life, "trades"),
            delta(initial_life, &life, "visits"),
            work["farmer_work_day"],
            work["tailor_work_day"],
            life["people"][10]["supper_day"],
            life["people"][11]["supper_day"],
            world["stock"],
        );
    }
    let missing: Vec<_> = cat_home::chapter_catalog::CHAPTERS
        .iter()
        .filter(|c| {
            c.id != "Q72"
                && !home.world.village_chapter_completed(c.id)
                && !home.life.village_chapter_completed(c.id)
        })
        .map(|c| c.id)
        .collect();
    println!(
        "calendar checkpoint age {} through {} world days, missing {missing:?}, {} new trades, {ticks} ticks max {worst}µs elapsed {:?}",
        home.world.town_real_days(),
        home.world.world_ms() / DAY_MS - first_day,
        home.life.trades() - trades,
        begin.elapsed()
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().unwrap_or_else(|| "target/probes".into());
    let calendar = first == "--real-calendar";
    let recovery = first == "--resume-real-calendar";
    let profile = first == "--profile-offline";
    let welfare = first == "--settled-window";
    let window = first == "--checkpoint-window" || welfare;
    let out = if calendar || recovery || profile || window {
        PathBuf::from(args.next().unwrap_or_else(|| "target/probes".into()))
    } else {
        PathBuf::from(first)
    };
    let source = args.next().map(PathBuf::from);
    let days = args
        .next()
        .and_then(|s| s.to_string_lossy().parse::<u64>().ok())
        .unwrap_or(0);
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let root = out.join(format!("village-host-{nonce}"));
    fs::create_dir_all(&root)?;
    println!("artifacts: {}", root.display());
    if profile {
        return profile_offline(&root);
    }
    if window {
        let source = source.ok_or_else(|| io::Error::other("missing real-calendar checkpoint"))?;
        if !(1..=24).contains(&days) {
            return Err(io::Error::other("checkpoint window needs one to 24 elapsed hours").into());
        }
        return checkpoint_window(&root, &source, days, welfare);
    }
    if recovery {
        let source = source.ok_or_else(|| io::Error::other("missing real-calendar checkpoint"))?;
        return real_calendar(&root, Some(&source));
    }
    if calendar {
        if let Some(source) = source {
            migration(&root, &source)?;
        }
        return real_calendar(&root, None);
    }
    restore(&root)?;
    slices(&root)?;
    if let Some(source) = source {
        migration(&root, &source)?;
    }
    if days > 0 {
        long_run(&root, days)?;
    }
    Ok(())
}
