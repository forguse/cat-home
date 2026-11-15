use super::*;

fn at(world: &mut LivingWorld, i: usize, tile: TilePos) {
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

fn set_time(world: &mut LivingWorld, now: u64) {
    world.now_ms = now;
    world.target_ms = now;
}

fn time(season: YardSeason, minute: u64, weather: YardWeather) -> u64 {
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

fn ready(now: u64) -> LivingWorld {
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
fn finish(world: &mut LivingWorld, i: usize, task: Task) {
    assert_eq!(world.actors[i].task, Some(task));
    let mut previous = world.actors[i].cell;
    let mut steps = 0;
    for _ in 0..512 {
        if world.actors[i].stage == Stage::Finish {
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
    let mut world = ready(now);
    let builder = ActorKind::Xiaohe.index();
    let i = ActorKind::Ayu.index();
    let fixture = Fixture::KitchenBatchStove;
    world.buildings[usize::from(fixture.site())].level = 2;
    install(&mut world, fixture, builder, now - HOUR);
    set_time(&mut world, now);
    let anchor = world.improvement_site(fixture).tile();
    let expected = if left {
        anchor.offset(-2, 0).unwrap()
    } else {
        anchor.offset(0, 2).unwrap()
    };
    let start = if left {
        anchor.offset(-3, 0).unwrap()
    } else {
        anchor.offset(0, 3).unwrap()
    };
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
        &mut world,
        ActorKind::Xiaohe.index(),
        stand.offset(-2, 0).unwrap(),
    );
    let i = ActorKind::Ayu.index();
    at(&mut world, i, stand.offset(0, 1).unwrap());
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
    finish(&mut world, i, Task::GetWater);
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
    verify(&world, id, ActorKind::Ayu);
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
    let day = (21_504..21_532)
        .find(|&day| {
            LivingWorld::weather_at(day * DAY_MS + 10 * MINUTE) == YardWeather::Rain
                && LivingWorld::weather_at(day * DAY_MS + 6 * HOUR + 20 * MINUTE)
                    == YardWeather::Clear
        })
        .expect("mature spring heavy rain followed by clear dawn");
    let flood = day * DAY_MS + 10 * MINUTE;
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
    set_time(&mut world, day * DAY_MS + 6 * HOUR + 20 * MINUTE);
    assert_eq!(world.weather(), YardWeather::Clear);
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
    verify(&world, "H01-34~00001", ActorKind::Xiaohe);
}
