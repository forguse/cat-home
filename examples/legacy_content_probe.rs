//! Ordinary host replay. Capture real first completions without altering stock,
//! tasks, age, clocks, weather or actor positions in the input snapshot.
#![cfg(windows)]
#![allow(dead_code)]
#[path = "../src/home_host.rs"]
mod home_host;
use home_host::HomeHost;
use std::{collections::BTreeSet, error::Error, fs, io, path::PathBuf, time::Instant};

fn advance(host: &mut HomeHost, wall: u64) -> Result<(), Box<dyn Error>> {
    for _ in 0..100_000 {
        host.tick(wall);
        if !host.world.is_catching_up() && !host.world.navigation_pending()
            && !host.life.navigation_pending() { return Ok(()); }
    }
    Err(io::Error::other("host did not finish bounded replay slice").into())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args=std::env::args_os().skip(1);
    let out=PathBuf::from(args.next().ok_or_else(||io::Error::other("OUTPUT HOURS [SAVE]"))?);
    let hours=args.next().ok_or_else(||io::Error::other("missing hours"))?.to_string_lossy().parse::<u64>()?;
    if !(1..=240).contains(&hours) { return Err(io::Error::other("hours must be 1..240").into()); }
    fs::create_dir(&out)?;
    let live=out.join("live");fs::create_dir(&live)?;
    let start=if let Some(input)=args.next() {
        let bytes=fs::read(input)?;
        let data:serde_json::Value=serde_json::from_slice(&bytes)?;
        fs::write(live.join("world-v4.json"),&bytes)?;
        data["world"]["last_wall_ms"].as_u64().ok_or_else(||io::Error::other("missing saved wall anchor"))?
    } else { 1_790_000_000_000 };
    let mut host=HomeHost::open(live.clone(),start,false)?;
    let mut bases=BTreeSet::new();
    for id in host.world.legacy_behaviors().observed_ids() { bases.insert(id.split(['~','+']).next().unwrap().to_owned()); }
    let initial_ids:BTreeSet<_>=host.world.legacy_behaviors().observed_ids().map(str::to_owned).collect();
    let initial_world_ms=host.world.world_ms();
    let mut executed_bases=BTreeSet::new();
    let mut peak_save_bytes=0;
    let begun=Instant::now();
    for second in 1..=hours*3600 {
        advance(&mut host,start+second*1000)?;
        let found:Vec<_>=host.world.legacy_behaviors().recent().iter()
            .filter_map(|event| {
                let base=event.id().split(['~','+']).next().unwrap();
                if event.now_ms>initial_world_ms { executed_bases.insert(base.to_owned()); }
                bases.insert(base.to_owned()).then_some((base.to_owned(),event.id().to_owned(),event.actor,event.at,event.target,event.now_ms))
            }).collect();
        for (base,id,actor,at,target,world_ms) in found {
            host.save()?;
            let capture=out.join(&base);fs::create_dir(&capture)?;
            fs::copy(live.join("world-v4.json"),capture.join("world-v4.json"))?;
            peak_save_bytes=peak_save_bytes.max(fs::metadata(live.join("world-v4.json"))?.len());
            println!("observed {id} by {actor:?} at {at:?} target {target:?} world_ms={world_ms} elapsed_wall_seconds={second}");
        }
    }
    host.save()?;
    peak_save_bytes=peak_save_bytes.max(fs::metadata(live.join("world-v4.json"))?.len());
    let before=serde_json::to_vec(&(&host.world,&host.life,host.scenes.state()))?;
    drop(host); // Release the exclusive plugin-store lock before reopening.
    let host=HomeHost::open(live.clone(),start+hours*3_600_000,false)?;
    let after=serde_json::to_vec(&(&host.world,&host.life,host.scenes.state()))?;
    assert_eq!(before,after,"reopening changed the completed world");
    let ids:Vec<_>=host.world.legacy_behaviors().observed_ids().collect();
    let new_ids:Vec<_>=ids.iter().copied().filter(|id| !initial_ids.contains(*id)).collect();
    let report=serde_json::json!({"wall_hours":hours,"elapsed_seconds":begun.elapsed().as_secs_f64(),
        "registered":cat_home::legacy_content::BehaviorLedger::registered_counts(),"initial_observed_ids":initial_ids,
        "observed_ids":ids,"new_observed_ids":new_ids,"observed_bases":bases,"executed_bases_this_replay":executed_bases,
        "save_bytes":fs::metadata(live.join("world-v4.json"))?.len(),"peak_captured_save_bytes":peak_save_bytes,
        "save_reopen_equal":true,"scope":"ordinary native host on a copy; no fixture state edits"});
    fs::write(out.join("observations.json"),serde_json::to_vec_pretty(&report)?)?;
    println!("{report}");
    Ok(())
}
