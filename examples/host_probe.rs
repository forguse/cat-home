//! Regression probe for explicit --recover-v3 household compatibility.
//! The village Windows adapter is covered by village_host_probe.rs.
#![cfg(windows)]
#![allow(dead_code)]

#[path = "../src/home_host.rs"]
mod home_host;

use cat_home::living_view::Camera;
use cat_home::living_world::DAY_MS;
use home_host::HomeHost;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const HOUR: u64 = 3_600_000;
const START: u64 = 1_790_000_000_000;
const MAX_TICKS: usize = 200_000;

fn authority(host: &HomeHost) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&(
        &host.world,
        host.scenes.state(),
        &host.visitors,
    ))?)
}

fn advance(host: &mut HomeHost, now: u64, draw: bool) -> Result<usize> {
    for i in 0..MAX_TICKS {
        host.tick(now);
        if draw && i % 7 == 0 {
            let before = authority(host)?;
            host.render((160, 100));
            assert_eq!(
                before,
                authority(host)?,
                "paint changed authoritative state"
            );
        }
        if !host.world.is_catching_up() {
            return Ok(i + 1);
        }
    }
    Err(io::Error::other("host never finished advancing to its queued target").into())
}

fn install(host: &mut HomeHost, pack: &Path) -> Result<()> {
    host.install(pack).map_err(io::Error::other)?;
    Ok(())
}

fn saved_restore(root: &Path) -> Result<()> {
    let root = root.join("saved-size");
    let mut host = HomeHost::open_compat(root.clone(), START, false)?;
    host.size = (812, 476);
    host.camera = Camera {
        x: 21.0,
        y: 43.0,
        follow: None,
    };
    assert!(!root.join("world-compat-v3.json").exists());
    host.tick(START + 29_999);
    assert!(!root.join("world-compat-v3.json").exists());
    host.tick(START + 30_000);
    let saved = fs::read(root.join("world-compat-v3.json"))?;
    let before = authority(&host)?;
    drop(host);
    let mut recovered = HomeHost::open_compat(root.clone(), START + 30_000, false)?;
    assert_eq!(recovered.size, (812, 476));
    assert_eq!(
        (
            recovered.camera.x,
            recovered.camera.y,
            recovered.camera.follow
        ),
        (21.0, 43.0, None)
    );
    assert_eq!(before, authority(&recovered)?);
    assert_eq!(recovered.camera_origin((1280, 720)), (0.0, 0.0));
    recovered.render((1280, 720));
    assert_eq!(recovered.camera_origin((160, 100)), (21.0, 43.0));
    assert_eq!(before, authority(&recovered)?);
    let mut old: serde_json::Value = serde_json::from_slice(&saved)?;
    assert_eq!(old["schema"], 3);
    old["schema"] = 2.into();
    for key in ["size", "visitors"] {
        old.as_object_mut().unwrap().remove(key);
    }
    let old_root = root.join("old-v2-copy");
    fs::create_dir_all(&old_root)?;
    let v2 = old_root.join("world-v2.json");
    let old_bytes = serde_json::to_vec(&old)?;
    fs::write(&v2, &old_bytes)?;
    let old_host = HomeHost::open_compat(old_root.clone(), START + 30_000, false)?;
    assert_eq!(old_host.size, (480, 300));
    assert_eq!(old_host.world, recovered.world);
    assert_eq!(
        (old_host.camera.x, old_host.camera.y, old_host.camera.follow),
        (21.0, 43.0, None)
    );
    old_host.save()?;
    assert!(old_root.join("world-compat-v3.json").exists());
    assert_eq!(fs::read(&v2)?, old_bytes);

    old["camera"] = serde_json::json!({"x":56.0,"y":64.0,"follow":0});
    let legacy_follow = root.join("v2-follow-camera");
    fs::create_dir_all(&legacy_follow)?;
    fs::write(
        legacy_follow.join("world-v2.json"),
        serde_json::to_vec(&old)?,
    )?;
    let mut follower = HomeHost::open_compat(legacy_follow.clone(), START + 30_000, false)?;
    let opening = Camera::default();
    assert_eq!(
        (follower.camera.x, follower.camera.y, follower.camera.follow),
        (opening.x, opening.y, Some(0))
    );
    follower.camera.set_follow(Some(2));
    follower.save()?;
    drop(follower);
    assert_eq!(
        HomeHost::open_compat(legacy_follow, START + 30_000, false)?
            .camera
            .follow,
        Some(2)
    );
    println!(
        "autosave and restore: v3 exact world and camera, 812x476, v2 intact, legacy views and follow preserved, {} bytes",
        saved.len()
    );
    Ok(())
}

fn errors(root: &Path) -> Result<()> {
    let root = root.join("save-error");
    let mut host = HomeHost::open_compat(root.clone(), START, false)?;
    host.save()?;
    let path = root.join("world-compat-v3.json");
    let previous = fs::read(&path)?;
    let obstruction = root.join("world-compat-v3.pending");
    fs::create_dir(&obstruction)?;
    host.tick(START + 30_000);
    let error = host
        .last_error
        .as_deref()
        .ok_or_else(|| io::Error::other("periodic save swallowed failure"))?;
    assert_eq!(
        fs::read(&path)?,
        previous,
        "failed save changed live snapshot"
    );
    assert!(host.save().is_err(), "direct save swallowed failure");
    println!("blocked periodic save retains prior bytes and exposes error: {error}");
    let moved = root.join("world-compat-v3-obstruction-preserved");
    fs::rename(obstruction, moved)?;
    host.tick(START + 60_000);
    assert_ne!(fs::read(&path)?, previous);
    assert!(
        host.last_error.is_none(),
        "a successful retry left an old error visible"
    );
    println!("successful retry replaced the snapshot and cleared the previous error");

    let invalid = root.join("invalid-copy");
    fs::create_dir_all(&invalid)?;
    let invalid_file = invalid.join("world-v2.json");
    let invalid_bytes = b"{\"schema\": 123, \"world\": null}";
    fs::write(&invalid_file, invalid_bytes)?;
    let rejected = HomeHost::open_compat(invalid, START, false)
        .err()
        .ok_or_else(|| io::Error::other("malformed snapshot was accepted"))?;
    assert_eq!(fs::read(invalid_file)?, invalid_bytes);
    println!("malformed snapshot was rejected and preserved: {rejected}");
    Ok(())
}

fn slices(root: &Path, pack: &Path) -> Result<()> {
    let mut fast = HomeHost::open_compat(root.join("slices-fast"), START, true)?;
    let mut visual = HomeHost::open_compat(root.join("slices-rendered"), START, true)?;
    install(&mut fast, pack)?;
    install(&mut visual, pack)?;
    let elapsed = 8 * HOUR;
    let start = Instant::now();
    let fast_ticks = advance(&mut fast, START + elapsed, false)?;
    let mut visual_ticks = 0;
    for time in (120_000..=elapsed).step_by(120_000) {
        visual_ticks += advance(&mut visual, START + time, true)?;
    }
    if authority(&fast)? != authority(&visual)? {
        for (name, same) in [
            ("world", fast.world == visual.world),
            ("visitors", fast.visitors == visual.visitors),
            (
                "plugin",
                serde_json::to_vec(fast.scenes.state())?
                    == serde_json::to_vec(visual.scenes.state())?,
            ),
        ] {
            println!("slice state equal {name}={same}");
        }
        fs::write(root.join("slices-fast.json"), authority(&fast)?)?;
        fs::write(root.join("slices-rendered.json"), authority(&visual)?)?;
        return Err(
            io::Error::other("render/slice schedules do not produce identical authority").into(),
        );
    }
    assert!(fast.visitors.visits() > 0);
    assert!(fast.scenes.state().made_contacts() > 0);
    println!(
        "slice/paint equality: {fast_ticks} offline vs {visual_ticks} rendered host ticks in {:?}, {} completed visits and {} ball contacts",
        start.elapsed(),
        fast.visitors.visits(),
        fast.scenes.state().made_contacts()
    );
    Ok(())
}

fn combined_year(root: &Path, pack: &Path) -> Result<()> {
    let root = root.join("combined-year");
    let mut host = HomeHost::open_compat(root.clone(), START, false)?;
    install(&mut host, pack)?;
    let start = Instant::now();
    let mut ticks = 0;
    let mut largest = 0;
    let mut wall = START;
    while host.world.world_ms() < 365 * DAY_MS {
        wall += 8 * HOUR;
        ticks += advance(&mut host, wall, false)?;
        assert!(host.world.validate());
        assert!(host.visitors.validate_for_world(&host.world));
        host.save()?;
        largest = largest.max(fs::metadata(root.join("world-compat-v3.json"))?.len());
    }
    assert_eq!(host.world.story().completed(), 24);
    assert!(host.scenes.state().made_contacts() > 0);
    assert!(host.visitors.visits() > 100);
    let before = authority(&host)?;
    let days = host.world.world_ms() as f64 / DAY_MS as f64;
    let chapters = host.world.story().completed();
    let visits = host.visitors.visits();
    let trades = host.visitors.trades();
    let contacts = host.scenes.state().made_contacts();
    drop(host);
    let recovered = HomeHost::open_compat(root, wall, false)?;
    assert_eq!(before, authority(&recovered)?);
    println!(
        "combined year: {days:.2} world days, {ticks} host ticks, {:?}, {chapters} story stages, {visits} visits, {trades} visitor trades, {contacts} ball contacts, max full snapshot {largest} bytes",
        start.elapsed()
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let out = PathBuf::from(
        args.next()
            .ok_or_else(|| io::Error::other("usage: host_probe OUT [CATPACK] [--year]"))?,
    );
    let pack = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("content-packs/basketball/庭院篮球.catpack"));
    let year = args.next().is_some_and(|arg| arg == "--year");
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let root = out.join(format!("host-probe-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root)?;
    println!("probe artifacts: {}", root.display());
    saved_restore(&root)?;
    errors(&root)?;
    let comparison = slices(&root, &pack);
    if year {
        combined_year(&root, &pack)?;
    }
    comparison
}
