//! Observes naturally scheduled resident jobs from a fresh world or a copied save.
//! Usage: resident_expansion_probe OUTPUT SAVE_OR_DASH WALL_HOURS [STRIDE_MS]
#[allow(dead_code)]
#[path = "../src/home_host.rs"]
mod home_host;

use cat_home::residents::VillageArtSubject;
use cat_home::visitors::{VisitorHeld, VisitorPose};
use home_host::HomeHost;
use std::{collections::BTreeSet, env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("missing output directory")?);
    let source = args.next().ok_or("missing source save or '-' for fresh")?;
    let hours = args.next().ok_or("missing wall hours")?.parse::<u64>()?;
    let stride = args.next().unwrap_or("30000".to_owned()).parse::<u64>()?;
    if stride == 0 || hours == 0 {
        return Err("hours and stride must both be positive".into());
    }
    let run = output.join("run");
    if run.join("world-v4.json").exists() {
        return Err("use a new output directory for each replay".into());
    }
    fs::create_dir_all(&run)?;
    let start = if source == "-" {
        0
    } else {
        let bytes = fs::read(&source)?;
        let parsed: serde_json::Value = serde_json::from_slice(&bytes)?;
        fs::write(run.join("world-v4.json"), bytes)?;
        parsed["world"]["last_wall_ms"].as_u64().unwrap()
    };
    let mut host = HomeHost::open(run.clone(), start, false)?;
    let start_world = host.world.world_ms();
    let finished = start.saturating_add(hours.saturating_mul(3_600_000));
    let watched = [
        "H02-01", "H02-08", "H06-01", "H06-03", "H06-07", "H06-08", "H06-09", "H07-04", "H08-02",
        "H08-03", "H08-04", "H09-03", "H09-06", "H09-07", "H10-04",
    ];
    let mut seen = BTreeSet::new();
    let mut captured = BTreeSet::new();
    let mut now = start;
    while now < finished {
        now = now.saturating_add(stride).min(finished);
        for _ in 0..10_000 {
            host.tick(now);
            let views = host.life.visuals(&host.world);
            let first = host
                .world
                .legacy_behaviors()
                .recent()
                .iter()
                .filter(|event| event.now_ms > start_world)
                .filter_map(|event| {
                    watched
                        .iter()
                        .find(|key| event.id().starts_with(**key))
                        .map(|key| (*key, event.id(), event.now_ms))
                })
                .collect::<Vec<_>>();
            for (_, id, time) in first {
                if seen.insert(id) {
                    println!(
                        "receipt {id} d{} min{}",
                        time / 86_400_000,
                        time % 86_400_000 / 60_000
                    );
                }
            }
            let at = |key: &str, pose| {
                views.iter().any(|v| {
                    matches!(v.held, Some(VisitorHeld::Parcel(parcel)) if parcel == key)
                        && v.pose == pose
                })
            };
            let art = host
                .life
                .village_art_visuals(&host.world)
                .collect::<Vec<_>>();
            let views = [
                ("cat-supper", at("warm_cat_supper", VisitorPose::Working)),
                (
                    "covered-meal",
                    at("covered_snack_tray", VisitorPose::Working),
                ),
                (
                    "empty-tray-return",
                    at("empty_snack_tray", VisitorPose::Carrying),
                ),
                (
                    "wrapped-letter",
                    views
                        .iter()
                        .any(|v| v.held == Some(VisitorHeld::Parcel("wrapped_letter"))),
                ),
                (
                    "mulch-work",
                    views.iter().any(|v| {
                        v.activity == "到真实果树根旁铺落叶" && v.pose == VisitorPose::Working
                    }),
                ),
                (
                    "mulched-orchard",
                    (8..12).any(|tree| host.world.village_tree_mulch(tree).is_some()),
                ),
                (
                    "dusk-swatches",
                    art.iter()
                        .any(|v| matches!(v.subject, VillageArtSubject::DuskSwatches(_))),
                ),
                (
                    "dusk-street",
                    art.iter()
                        .any(|v| matches!(v.subject, VillageArtSubject::DuskStreet(_))),
                ),
                (
                    "rain-intake",
                    host.life
                        .rain_barrel()
                        .is_some_and(|(_, units, _)| units > 0),
                ),
                (
                    "barrel-leak",
                    host.life
                        .rain_barrel()
                        .is_some_and(|(_, _, leaking)| leaking),
                ),
                (
                    "barrel-repair",
                    views.iter().any(|v| {
                        v.activity == "给真实漏水的桶贴补板" && v.pose == VisitorPose::Working
                    }),
                ),
                ("soil-window", host.life.soil_window(&host.world).is_some()),
                (
                    "soil-result",
                    (29..=31).any(|plot| host.world.village_soil_profile(plot).is_some()),
                ),
                (
                    "seed-rewrap",
                    views.iter().any(|v| {
                        v.activity == "在干盘上给受潮种袋换防水封" && v.pose == VisitorPose::Working
                    }),
                ),
                (
                    "sealed-seed-in-transit",
                    views
                        .iter()
                        .any(|v| v.held == Some(VisitorHeld::Parcel("wrapped_seed_packet"))),
                ),
                (
                    "sealed-seed-after-sale",
                    [
                        cat_home::living_world::ActorKind::Xiaohe,
                        cat_home::living_world::ActorKind::Ayu,
                    ]
                    .iter()
                    .any(|&id| {
                        host.life.household_held(id)
                            == Some(VisitorHeld::Parcel("wrapped_seed_packet"))
                    }),
                ),
                (
                    "orchard-leaf-carry",
                    at("orchard_leaf", VisitorPose::Carrying),
                ),
                (
                    "leaf-card-rubbing",
                    host.life
                        .village_leaf_card(&host.world)
                        .is_some_and(|(_, _, phase)| phase < 100),
                ),
                (
                    "finished-leaf-card",
                    host.life
                        .village_leaf_card(&host.world)
                        .is_some_and(|(_, _, phase)| phase == 100),
                ),
                ("saucer-carry", at("saucer_meal", VisitorPose::Carrying)),
                (
                    "saucer-on-mud",
                    host.life
                        .village_food_saucer()
                        .is_some_and(|(_, full)| full),
                ),
                (
                    "saucer-empty",
                    host.life
                        .village_food_saucer()
                        .is_some_and(|(_, full)| !full),
                ),
                ("saucer-return", at("empty_saucer", VisitorPose::Carrying)),
            ];
            for (name, show) in views {
                if show && !captured.contains(name) && host.save().is_ok() {
                    let destination = output.join(format!("{name}.json"));
                    fs::copy(run.join("world-v4.json"), &destination)?;
                    captured.insert(name);
                    println!(
                        "capture {name} d{} min{} {}",
                        host.world.world_ms() / 86_400_000,
                        host.world.minute_of_day(),
                        destination.display()
                    );
                }
            }
            if !host.world.is_catching_up()
                && !host.world.navigation_pending()
                && !host.life.navigation_pending()
            {
                break;
            }
        }
    }
    host.save()?;
    let ended = host.world.world_ms();
    let state = serde_json::to_vec(&host.life)?;
    drop(host);
    let reopened = HomeHost::open(run.clone(), finished, false)?;
    assert_eq!(reopened.world.world_ms(), ended);
    assert_eq!(serde_json::to_vec(&reopened.life)?, state);
    println!(
        "end d{} seen {seen:?} captured {captured:?} size {} reopens",
        ended / 86_400_000,
        fs::metadata(run.join("world-v4.json"))?.len()
    );
    Ok(())
}
