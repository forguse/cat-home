use super::*;

pub(super) fn at(world: &mut LivingWorld, i: usize, tile: TilePos) {
    assert!(
        world.can_walk(tile.x.into(), tile.y.into()),
        "inaccessible foot {tile:?}"
    );
    world.idle(i, DAY_MS);
    let actor = &mut world.actors[i];
    actor.cell = Cell::from(tile);
    actor.position = actor.cell.point();
    actor.hunger = 15;
    actor.thirst = 15;
    actor.stamina = 70;
    actor.inside = false;
}

pub(super) fn set_time(world: &mut LivingWorld, now: u64) {
    world.now_ms = now;
    world.target_ms = now;
}

pub(super) fn time(season: YardSeason, minute: u64, weather: YardWeather) -> u64 {
    let season = match season {
        YardSeason::Spring => 0,
        YardSeason::Summer => 1,
        YardSeason::Autumn => 2,
        YardSeason::Winter => 3,
    };
    (21_504 + season * 28..21_504 + (season + 1) * 28)
        .map(|d| d * DAY_MS + minute * MINUTE)
        .find(|&ms| LivingWorld::weather_at(ms) == weather)
        .expect("real clock window")
}

fn clear_dusk(season: YardSeason, early_minutes: u64) -> u64 {
    let first = if season == YardSeason::Spring {
        21_504
    } else {
        21_560
    };
    (first..first + 28)
        .map(|day| day * DAY_MS + 18 * HOUR)
        .find(|&at| {
            LivingWorld::weather_at(at) == YardWeather::Clear
                && matches!(
                    LivingWorld::weather_at(at - early_minutes * MINUTE),
                    YardWeather::Clear | YardWeather::Clouds
                )
        })
        .unwrap()
        - early_minutes * MINUTE
}

pub(super) fn ready(now: u64) -> LivingWorld {
    let mut world = LivingWorld::new_village_wall_ms(0);
    set_time(&mut world, now);
    world.set_test_town_age(365);
    let opening = &mut world.village.as_mut().unwrap().opening;
    opening.boxes_stowed = 3;
    opening.slept_at_home = 3;
    opening.shared_eaten = 3;
    opening.rod_on_rack = true;
    opening.cat_ate = true;
    opening.cat_visited = 3;
    opening.rod_returned = true;
    opening.parallel_work = true;
    opening.late_meal_eaten = true;
    opening.shared_rest = true;
    opening.stable_days = 7;
    assert!(opening.completed("Q03"));
    for (i, foot) in [
        TilePos::new(12, 16),
        TilePos::new(24, 16),
        TilePos::new(18, 22),
    ]
    .into_iter()
    .enumerate()
    {
        at(&mut world, i, foot);
    }
    world
}

/// Use the real path pump, adjacent movement, Prepare and Work stages.
/// Time is advanced to the actor's actual deadlines. Nonparticipating
/// workers and unrelated simulation policies are left parked in this fixture.
pub(super) fn finish(world: &mut LivingWorld, i: usize, task: Task) {
    reach_stage(world, i, task, Stage::Finish);
}

pub(super) fn reach_stage(world: &mut LivingWorld, i: usize, task: Task, until: Stage) {
    assert_eq!(world.actors[i].task, Some(task));
    let mut previous = world.actors[i].cell;
    let mut steps = 0;
    for _ in 0..512 {
        if world.actors[i].stage == until {
            assert_eq!(world.actors[i].cell, world.site(i, task));
            assert!(steps > 0, "did not walk to the actual target for {task:?}");
            return;
        }
        let now = world.now_ms.max(world.actors[i].until);
        set_time(world, now);
        if world.pending_route_actor() == Some(i) {
            let mut budget = 64;
            world.pump_route(i, &mut budget);
        } else {
            world.step(i);
        }
        let current = world.actors[i].cell;
        if previous != current {
            assert_eq!(previous.distance(current), 1);
            steps += 1;
            previous = current;
        }
        assert_ne!(
            world.actors[i].stage,
            Stage::Idle,
            "failed while executing {task:?}"
        );
    }
    panic!("did not complete {task:?}: {:?}", world.actors[i]);
}

fn verify(world: &LivingWorld, id: &str, actor: ActorKind) {
    let all = world.legacy_behaviors().recent();
    let Some(row) = all.iter().find(|row| row.id() == id) else {
        panic!(
            "exact receipt {id} missing; {:?}",
            all.iter()
                .map(|x| (x.id(), x.now_ms, x.at, x.target))
                .collect::<Vec<_>>()
        );
    };
    assert_eq!(row.actor, LegacyActor::Household(actor as u8));
    assert_eq!(row.at, world.actor_tile(actor));
    assert_eq!(row.now_ms, world.now_ms);
    let minute = row.now_ms % DAY_MS / MINUTE;
    println!(
        "PROOF {} d{} {:02}:{:02} {:?} {:?} ({},{}) -> ({},{})",
        id,
        row.now_ms / DAY_MS,
        minute / 60,
        minute % 60,
        world.weather(),
        world.season(),
        row.at.x,
        row.at.y,
        row.target.x,
        row.target.y,
    );
}

fn family_rest(id: &str, clock: u64) {
    let mut world = ready(clock);
    let i = ActorKind::Xiaohe.index();
    let remembered = world.site(i, Task::Rest).tile();
    at(&mut world, i, remembered);
    world.commit(i, Task::Rest);
    assert_eq!(
        world.village.as_ref().unwrap().household_rules.last_stop[i],
        Some(remembered)
    );
    let start = [(0, 1), (-1, 0), (1, 0)]
        .iter()
        .filter_map(|&(x, y)| remembered.offset(x, y))
        .find(|&tile| world.quiet_visit_site_available(i, tile))
        .unwrap();
    at(&mut world, i, start);
    let visit = world
        .quiet_visit(i)
        .expect("routine offers the earlier rest location");
    assert_eq!(visit, QuietVisit::Pause(remembered));
    let task = Task::QuietVisit(visit);
    assert!(world.begin(i, task));
    finish(&mut world, i, task);
    assert_eq!(world.actors[i].stamina, 78);
    assert_eq!(
        world.village.as_ref().unwrap().household_rules.last_stop[i],
        Some(remembered)
    );
    verify(&world, id, ActorKind::Xiaohe);
    check_observation(&world, Observation::RestStop, id);
}

#[test]
fn s02_spring_clouds_day() {
    family_rest(
        "S02~11041",
        time(YardSeason::Spring, 9 * 60, YardWeather::Clouds),
    );
}

#[test]
fn s02_spring_clear_dusk() {
    family_rest("S02~20041", clear_dusk(YardSeason::Spring, 2));
}

fn install(world: &mut LivingWorld, fixture: Fixture, i: usize, when: u64) {
    let site = world.improvement_site(fixture).tile();
    world.story.improvements = Default::default();
    for (phase, now) in [(0, when - DAY_MS), (1, when)] {
        set_time(world, now);
        at(world, i, site);
        let order = ImprovementOrder { fixture, phase };
        let (material, quantity) = order.material();
        let item = chapter_item(material);
        // Fixtures start from an ordinary legal paid-work inventory.
        if world.stock.count(item) < quantity {
            assert!(world.stock.add(item, quantity - world.stock.count(item)));
        }
        let before = world.stock.count(item);
        assert!(world.take_stock(item, quantity));
        world.carry(i, item, quantity);
        world.actors[i].tool = Some(Tool::Hammer);
        world.commit(i, Task::Improve(order));
        assert!(world.actors[i].carried.is_none());
        assert_eq!(world.stock.count(item), before - quantity);
    }
    assert!(world.story.improvements.working(fixture));
}

fn inspect(id: &str, now: u64, left: bool) {
    inspect_from(id, now, if left { (-1, 0) } else { (0, 1) });
}

fn inspect_from(id: &str, now: u64, side: (i32, i32)) {
    let mut world = ready(now);
    let builder = ActorKind::Xiaohe.index();
    let i = ActorKind::Ayu.index();
    let fixture = Fixture::KitchenBatchStove;
    world.buildings[usize::from(fixture.site())].level = 2;
    install(&mut world, fixture, builder, now - HOUR);
    set_time(&mut world, now);
    let anchor = world.improvement_site(fixture).tile();
    let expected = anchor.offset(side.0 * 2, side.1 * 2).unwrap();
    let start = anchor.offset(side.0 * 3, side.1 * 3).unwrap();
    at(&mut world, i, start);
    assert!(world.has_fixture_sightline(anchor, expected));
    let version = world
        .village
        .as_ref()
        .unwrap()
        .household_rules
        .new_fixture
        .unwrap();
    let visit = world
        .quiet_visit(i)
        .expect("real new fixture inspection selected");
    assert_eq!(
        visit,
        QuietVisit::Inspect {
            version,
            stand: expected
        }
    );
    let work = Task::QuietVisit(visit);
    assert!(world.begin(i, work));
    finish(&mut world, i, work);
    assert_eq!(
        world.village.as_ref().unwrap().household_rules.seen_fixture[i],
        Some(version)
    );
    assert!(world.quiet_visit(i).is_none());
    let reloaded: LivingWorld =
        serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    assert_eq!(
        reloaded
            .village
            .as_ref()
            .unwrap()
            .household_rules
            .seen_fixture[i],
        Some(version)
    );
    verify(&world, id, ActorKind::Ayu);
    check_observation(&world, Observation::Fixture, id);
}

#[test]
fn s13_winter_clear_day_left() {
    inspect(
        "S13~10231",
        time(YardSeason::Winter, 9 * 60, YardWeather::Clear),
        true,
    );
}
#[test]
fn s13_spring_clouds_day_front() {
    inspect(
        "S13~11041",
        time(YardSeason::Spring, 9 * 60, YardWeather::Clouds),
        false,
    );
}
#[test]
fn s13_autumn_clouds_day_left() {
    inspect(
        "S13~11131",
        time(YardSeason::Autumn, 9 * 60, YardWeather::Clouds),
        true,
    );
}
#[test]
fn s13_spring_clear_dusk_front() {
    inspect("S13~20041", clear_dusk(YardSeason::Spring, 1), false);
}
#[test]
fn s13_autumn_clear_dusk_left() {
    inspect("S13~20131", clear_dusk(YardSeason::Autumn, 1), true);
}

fn well(id: &str, now: u64) {
    let mut world = ready(now);
    world.story.finished = 21;
    world.village.as_mut().unwrap().apply_development(21, false);
    install(
        &mut world,
        Fixture::WellRope,
        ActorKind::Xiaohe.index(),
        now - 2 * HOUR,
    );
    set_time(&mut world, now);
    let stand = village::poi("market_well").tile();
    assert!(
        !world
            .village
            .as_ref()
            .unwrap()
            .household_rules
            .well_needs_trial
    );
    run_repaired_well(&mut world, stand);
    if !world.legacy_behaviors().recent().iter().any(|note| note.id() == id) {
        // Some old cases use a "nearby" site as well as the exact district.
        // The later real repair can select the other lifetime at the same well.
        set_time(&mut world, now + 10 * MINUTE);
        world.drop_carry(ActorKind::Ayu.index());
        run_repaired_well(&mut world, stand);
    }
    verify(&world, id, ActorKind::Ayu);
    check_observation(&world, Observation::Well, id);
}

fn run_repaired_well(world: &mut LivingWorld, stand: TilePos) {
    world.story.improvements.used(Fixture::WellRope);
    assert!(world.apply_community(CommunityAction::Mend(Fixture::WellRope), stand));
    assert!(
        world
            .village
            .as_ref()
            .unwrap()
            .household_rules
            .well_needs_trial
    );
    at(
        world,
        ActorKind::Xiaohe.index(),
        stand.offset(-2, 0).unwrap(),
    );
    let i = ActorKind::Ayu.index();
    at(world, i, stand.offset(0, 1).unwrap());
    world.actors[i].tool = Some(Tool::WateringCan);
    world.actors[i].outfit = world.outfit_for(i, false);
    world.well = 5;
    let water = world.stock.count(Item::Water);
    if water > 0 {
        assert!(world.take_stock(Item::Water, water));
    }
    assert_eq!(world.water_collection_task(i), Task::GetWater);
    assert_eq!(world.site(i, Task::GetWater).tile(), stand);
    world.plan(i);
    finish(world, i, Task::GetWater);
    assert!(world.has_carry(i, Item::Water, 4));
    assert_eq!(world.well, 1);
    assert_eq!(world.stock.count(Item::Water), 0);
    assert!(
        !world
            .village
            .as_ref()
            .unwrap()
            .household_rules
            .well_needs_trial
    );
}

fn check_observation(world: &LivingWorld, kind: Observation, id: &str) {
    let note = world.active_village_observations().find(|note| note.kind == kind)
        .unwrap_or_else(|| panic!("{id} must leave a live observation"));
    let mode = id.as_bytes().last().unwrap() - b'0';
    let minutes = match kind {
        Observation::RestStop | Observation::CatNap | Observation::CatNight
        | Observation::HouseholdNight | Observation::GroomPause => [30, 120, 360],
        Observation::FallingTree => [5, 15, 30],
        _ => [60, 240, 720],
    };
    let duration = minutes[usize::from(mode - 1)];
    assert_eq!(note.expires(), note.at + duration * MINUTE);
    let mut reopened: LivingWorld =
        serde_json::from_slice(&serde_json::to_vec(world).unwrap()).unwrap();
    assert!(reopened.active_village_observations().any(|other| other == note));
    set_time(&mut reopened, note.expires() - 1);
    assert!(reopened.active_village_observations().any(|other| other == note));
    set_time(&mut reopened, note.expires());
    assert!(!reopened.active_village_observations().any(|other| other == note));
}

pub(super) fn boundary(season: YardSeason, hour: u64, weather: YardWeather, early: u64) -> u64 {
    let first = 21_504 + season as u64 * 28;
    (first..first + 28)
        .map(|day| day * DAY_MS + hour * HOUR)
        .find(|&at| {
            LivingWorld::weather_at(at) == weather
                && matches!(LivingWorld::weather_at(at - early * MINUTE), YardWeather::Clear | YardWeather::Clouds)
        })
        .expect("real dry start and matching completion weather")
        - early * MINUTE
}

pub(super) fn tree_warning(id: &str, now: u64, bearing: u8) {
    for tree in 0..TREES.len() {
        if tree_warning_at(id, now, bearing, tree) {
            return;
        }
    }
    panic!("no real tree and safe retreat for {id}");
}

fn tree_warning_at(id: &str, now: u64, bearing: u8, tree: usize) -> bool {
    let mut world = ready(now);
    let cutter = ActorKind::Ayu.index();
    let watcher = ActorKind::Orange.index();
    if world.trees[tree].protected || world.trees[tree].stage != TreeStage::Standing {
        return false;
    }
    let origin = Cell::at(world.trees[tree].position).unwrap().tile();
    let task = Task::FellTree(tree as u8);
    let foot = world.site(cutter, task).tile();
    if !world.can_walk(foot.x.into(), foot.y.into()) {
        return false;
    }
    at(&mut world, cutter, foot);
    world.actors[cutter].tool = Some(Tool::Axe);
    // Consume the real normal work stage at the stump and only then offer retreat.
    world.actors[cutter].task = Some(task);
    world.stage(cutter, Stage::Work, Pose::Work, task.duration());
    set_time(&mut world, now + task.duration());
    world.step(cutter);
    assert_eq!(world.trees[tree].stage, TreeStage::Felled);
    let selection = (-5..=5).flat_map(|y| (-5..=5).filter_map(move |x| origin.offset(x, y)))
        .filter(|tile| world.can_walk(tile.x.into(), tile.y.into()))
        .collect::<Vec<_>>()
        .into_iter()
        .find_map(|tile| {
            at(&mut world, watcher, tile);
            let task = world.startled_task(watcher)?;
            let stand = world.site(watcher, task).tile();
            let matches = match bearing {
                b'3' => stand.x < origin.x,
                b'4' => stand.x == origin.x && stand.y >= origin.y,
                b'5' => stand.x > origin.x,
                _ => false,
            };
            matches.then_some((tile, task))
        });
    let Some((from, retreat)) = selection else { return false };
    assert_ne!(from, world.site(watcher, retreat).tile());
    assert!(world.begin(watcher, retreat));
    finish(&mut world, watcher, retreat);
    assert!(world.startled_task(watcher).is_none());
    verify(&world, id, ActorKind::Orange);
    check_observation(&world, Observation::FallingTree, id);
    true
}

fn night_rest(id: &str, now: u64, kind: Observation) {
    let mut world = ready(now);
    let cat = kind == Observation::CatNight;
    let role = if cat { ActorKind::Orange } else { ActorKind::Xiaohe };
    let i = role.index();
    let door = world.home_door(0).tile();
    let approach = door.offset(0, 1).unwrap();
    at(&mut world, i, approach);
    world.actors[i].stamina = 25;
    if !cat { world.actors[i].outfit = Outfit::Home; }
    // The other resident must not be occupying the shared approach.
    if cat { at(&mut world, ActorKind::Xiaohe.index(), TilePos::new(18, 21)); }
    let task = if cat { Task::CatSleepInside(door) } else { Task::Sleep };
    world.plan(i);
    assert_eq!(world.actors[i].task, Some(task));
    for _ in 0..64 {
        let now = world.now_ms.max(world.actors[i].until);
        set_time(&mut world, now);
        if world.pending_route_actor() == Some(i) {
            world.pump_route(i, &mut 64);
        } else {
            world.step(i);
        }
        if world.actors[i].stage == Stage::Work {
            assert!(world.actors[i].inside);
            verify(&world, id, role);
            check_observation(&world, kind, id);
            return;
        }
    }
    panic!("did not enter the real bedroom");
}

fn cat_pause(id: &str, now: u64, kind: Observation) {
    let mut world = ready(now);
    let i = ActorKind::Orange.index();
    let task = if kind == Observation::GroomPause {
        let bowl = village::poi("cat_water").tile();
        at(&mut world, i, bowl);
        world.actors[i].thirst = 70;
        world.bowl = 1;
        world.commit(i, Task::Drink);
        assert_eq!(world.bowl, 0);
        assert!(matches!(world.cat_detail(), Some(Task::Detail(Detail::Groom(_)))));
        world.cat_detail().unwrap()
    } else {
        let plate = world.site(0, Task::ServeCat).tile();
        at(&mut world, 0, plate);
        assert!(world.stock.add(Item::Food, 1));
        assert!(world.take_stock(Item::Food, 1));
        world.carry(0, Item::Food, 1);
        world.commit(0, Task::ServeCat);
        at(&mut world, 0, TilePos::new(12, 16));
        let cat_plate = world.site(i, Task::CatEat).tile();
        at(&mut world, i, cat_plate);
        world.commit(i, Task::CatSmellMeal);
        world.commit(i, Task::CatEat);
        assert!(!world.cat_food_served());
        world.cat_post_meal_nap().expect("actual meal qualifies for a nap")
    };
    assert!(world.begin(i, task));
    if kind == Observation::GroomPause {
        // Grooming works at the drink spot without moving away to return.
        while world.actors[i].stage != Stage::Finish {
            let now = world.now_ms.max(world.actors[i].until);
            set_time(&mut world, now);
            assert_ne!(world.actors[i].stage, Stage::Idle);
            world.step(i);
        }
    } else {
        finish(&mut world, i, task);
    }
    verify(&world, id, ActorKind::Orange);
    check_observation(&world, kind, id);
}

fn confirm_project_notice(id: &str, now: u64) {
    let mut world = ready(now);
    let worker = ActorKind::Ayu.index();
    let stand = village::poi("market_notice").tile();
    let approach = stand.offset(0, 1).unwrap();
    at(&mut world, worker, approach);
    let phase = CHAPTERS.iter().position(|chapter| chapter.work_shifts >= 2).unwrap() as u8;
    let next_shift = now + 2 * HOUR;
    world.story.running = Some(Project { id: phase, begun: now, shifts: 1, next_shift });
    let task = world.next_shift_notice_task(worker).expect("live partial project with a future shift");
    assert!(world.begin(worker, task));
    finish(&mut world, worker, task);
    assert_eq!(world.yard_posted_construction_shift(), Some((stand, next_shift)));
    let project = world.story.running.unwrap();
    assert_eq!(project.shifts, 1);
    assert_eq!(project.next_shift, next_shift);
    verify(&world, id, ActorKind::Ayu);
    check_observation(&world, Observation::WorkNotice, id);
}

#[test]
fn retained_observations_follow_production_tasks() {
    let scenarios = include_str!("../assets/maps/village-legacy-exact-scenes.tsv");
    let cases: Vec<_> = scenarios.lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(_, evidence)| evidence.starts_with("Retained observation:"))
        .collect();
    assert!(!cases.is_empty());
    let mut failed = Vec::new();
    for (id, _) in cases {
        let (base, dims) = id.split_once('~').unwrap();
        let dims = dims.as_bytes();
        let season = [YardSeason::Spring, YardSeason::Autumn, YardSeason::Winter][usize::from(dims[2] - b'0')];
        let weather = [YardWeather::Clear, YardWeather::Clouds, YardWeather::Rain, YardWeather::Snow][usize::from(dims[1] - b'0')];
        let result = std::panic::catch_unwind(|| match base {
            "S18" => {
                let minute = [6 * 60 + 30, 10 * 60, 18 * 60 + 30, 20 * 60 + 25][usize::from(dims[0] - b'0')];
                well(id, time(season, minute, weather));
            }
            "S13" => {
                let side = [(-1, 0), (0, 1), (1, 0)][usize::from(dims[3] - b'3')];
                let now = if dims[0] == b'2' {
                    boundary(season, 18, weather, 1)
                } else if matches!(weather, YardWeather::Rain | YardWeather::Snow) {
                    boundary(season, 12, weather, 1)
                } else {
                    time(season, 10 * 60, weather)
                };
                inspect_from(id, now, side);
            }
            "S02" => {
                let now = if dims[0] == b'2' {
                    boundary(season, 18, weather, 2)
                } else if weather == YardWeather::Rain {
                    boundary(season, 12, weather, 2)
                } else {
                    time(season, 10 * 60, weather)
                };
                family_rest(id, now);
            }
            "S22" => {
                let now = if dims[0] == b'2' {
                    boundary(season, 18, weather, 1)
                } else {
                    time(season, 10 * 60, weather)
                };
                let start = now.saturating_sub(Task::FellTree(0).duration());
                tree_warning(id, start, dims[3]);
            }
            "A01-10" | "H01-60" => {
                let kind = if base == "A01-10" { Observation::CatNight } else { Observation::HouseholdNight };
                night_rest(id, time(season, 22 * 60, weather), kind);
            }
            "A01-04" | "A01-08" => {
                let kind = if base == "A01-04" { Observation::GroomPause } else { Observation::CatNap };
                cat_pause(id, time(season, 12 * 60 + 15, weather), kind);
            }
            "H01-34" => {
                let hour = if dims[0] == b'0' { 6 } else { 18 };
                drain_after_true_flood(id, season, hour, weather);
            }
            "H01-56" => confirm_project_notice(id, time(season, 14 * 60, weather)),
            _ => panic!("unhandled test scenario {id}"),
        });
        if result.is_err() { failed.push(id); }
    }
    assert!(failed.is_empty(), "Unverified scenarios: {failed:?}");
}

#[test]
fn s18_spring_clear_dawn() {
    well(
        "S18~00001",
        time(YardSeason::Spring, 6 * 60 + 30, YardWeather::Clear),
    );
}
#[test]
fn s18_winter_snow_dusk() {
    well(
        "S18~23201",
        time(YardSeason::Winter, 18 * 60 + 30, YardWeather::Snow),
    );
}
#[test]
fn s18_spring_clear_night() {
    well(
        "S18~30021",
        time(YardSeason::Spring, 20 * 60 + 25, YardWeather::Clear),
    );
}
#[test]
fn s18_spring_clouds_night() {
    well(
        "S18~31021",
        time(YardSeason::Spring, 20 * 60 + 25, YardWeather::Clouds),
    );
}
#[test]
fn s18_winter_clouds_night() {
    well(
        "S18~31201",
        time(YardSeason::Winter, 20 * 60 + 25, YardWeather::Clouds),
    );
}

#[test]
fn a01_49_front_pawprint_at_winter_dawn() {
    let day = 95;
    let now = day * DAY_MS + 6 * HOUR + 25 * MINUTE;
    let mut world = ready(now);
    assert_eq!(world.weather(), YardWeather::Snow);
    let i = ActorKind::Orange.index();
    at(&mut world, i, TilePos::new(16, 26));
    let task = world
        .first_snow_probe()
        .expect("active snow and close safe footing");
    let Task::CatProbeSnow { stand, snow } = task else {
        panic!("{task:?}")
    };
    assert_eq!(stand.x, snow.x);
    assert!(stand.y > snow.y);
    world.plan(i);
    finish(&mut world, i, task);
    assert!(
        world
            .village_footprints()
            .any(|print| print.snow && print.tile == snow && print.toward == stand)
    );
    verify(&world, "A01-49~03241", ActorKind::Orange);
}

#[test]
fn h01_34_dawn_after_true_flood() {
    drain_after_true_flood("H01-34~00001", YardSeason::Spring, 6, YardWeather::Clear);
}

fn drain_after_true_flood(id: &str, season: YardSeason, hour: u64, weather: YardWeather) {
    let first = 21_504 + season as u64 * 28;
    let (flood, work_time) = (first..first + 28).find_map(|day| {
        let work_time = day * DAY_MS + hour * HOUR + 20 * MINUTE;
        if LivingWorld::weather_at(work_time) != weather {
            return None;
        }
        (0..hour / 3).rev().map(|spell| day * DAY_MS + spell * 3 * HOUR + 10 * MINUTE)
            .find(|&clock| LivingWorld::weather_at(clock) == YardWeather::Rain && clock / (3 * HOUR) % 4 == 0)
            .map(|flood| (flood, work_time))
    }).expect("real flooding rain before eligible work time");
    let mut world = ready(flood);
    world.story.finished = 18;
    world.village.as_mut().unwrap().apply_development(18, false);
    assert_eq!(world.weather(), YardWeather::Rain);
    let plot = (6..village::ORIGINAL_VILLAGE_PLOTS)
        .find(|&p| {
            p != usize::from(grain::FIELD)
                && (flood / (3 * HOUR) + p as u64) % 5 == 0
                && world.available_plot_rect(p).is_some()
        })
        .expect("wet active ordinary bed");
    let i = ActorKind::Xiaohe.index();
    let work = LivingWorld::plot_work_cell(plot).tile();
    at(&mut world, i, work);
    world.plots[plot].stage = PlotStage::Tilled;
    assert!(world.take_stock(Item::Seeds, 1));
    world.carry(i, Item::Seeds, 1);
    world.commit(i, Task::Sow(plot as u8));
    world.next_needs = flood;
    world.environment();
    assert!(world.village_plot_is_flooded(plot as u8));
    set_time(&mut world, work_time);
    assert_eq!(world.weather(), weather);
    let nearby = [(0, 1), (-1, 0), (1, 0)]
        .iter()
        .filter_map(|&(x, y)| work.offset(x, y))
        .find(|&tile| world.quiet_visit_site_available(i, tile))
        .unwrap();
    at(&mut world, i, nearby);
    world.actors[i].tool = Some(Tool::Hoe);
    let task = Task::Drain(plot as u8);
    assert!(world.begin(i, task));
    finish(&mut world, i, task);
    assert!(!world.village_plot_is_flooded(plot as u8));
    assert!(world.hoe_has_mud(ActorKind::Xiaohe));
    verify(&world, id, ActorKind::Xiaohe);
    check_observation(&world, Observation::HouseholdDrain, id);
}
