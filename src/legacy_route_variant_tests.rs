use super::legacy_household_variant_tests::{at, boundary, finish, reach_stage, ready, set_time, time, tree_warning};
use super::*;

fn scene_mode(id: &str) -> usize {
    usize::from(id.as_bytes().last().unwrap() - b'1')
}

fn note(world: &LivingWorld, id: &str, duration: u64) -> VillageObservationNote {
    assert!(world.legacy_behaviors.observed_ids().any(|seen| seen == id),
        "{id} missing at d{} min{} {:?}; {:?}",
        world.now_ms / DAY_MS, world.minute_of_day(), world.weather(),
        world.legacy_behaviors.recent().iter().map(|event| event.id()).collect::<Vec<_>>());
    let (base, _) = id.split_once('~').unwrap();
    let note = *world.active_village_observations().find(|n| n.kind.base() == base)
        .expect("real outcome left on the map");
    assert_eq!(note.expires(), note.at + duration);
    let mut reloaded: LivingWorld = serde_json::from_slice(&serde_json::to_vec(world).unwrap()).unwrap();
    assert!(reloaded.active_village_observations().any(|n| *n == note));
    set_time(&mut reloaded, note.expires());
    assert!(!reloaded.active_village_observations().any(|n| *n == note));
    note
}

fn move_routes(world: &mut LivingWorld, base: &str) {
    for _ in 0..256 {
        if world.active_village_observations().any(|n| n.kind.base() == base) {
            return;
        }
        let i = (0..2).min_by_key(|&i| world.actors[i].until).unwrap();
        let now = world.now_ms.max(world.actors[i].until);
        set_time(world, now);
        if let Some(j) = world.pending_route_actor() {
            world.pump_route(j, &mut 64);
        } else {
            let previous = world.actors[i].cell;
            world.step(i);
            assert!(previous.distance(world.actors[i].cell) <= 1);
            assert_ne!(world.actors[0].cell, world.actors[1].cell);
        }
    }
    panic!("two actors could not complete their real paths");
}

fn real_pass(id: &str, clock: u64, bearing: u8) {
    let mut world = ready(clock);
    let direction = match bearing {
        b'3' => (-1, 0),
        b'4' => (0, 1),
        b'5' => (1, 0),
        _ => unreachable!(),
    };
    let (entry, ahead, behind) = (18..38).flat_map(|y| (12..35).map(move |x| TilePos::new(x, y)))
        .find_map(|entry| {
            let ahead = entry.offset(direction.0, direction.1)?;
            let behind = entry.offset(-direction.0, -direction.1)?;
            let legal = |tile: TilePos| world.can_walk(tile.x.into(), tile.y.into());
            let shoulder = [(0,-1), (-1,0), (1,0), (0,1)].into_iter()
                .filter_map(|(x, y)| entry.offset(x, y))
                .any(|pos| pos != ahead && pos != behind && legal(pos));
            (legal(entry) && legal(ahead) && legal(behind) && shoulder
                && world.real_trackable_edge(entry, ahead).is_some()).then_some((entry, ahead, behind))
        }).unwrap();
    at(&mut world, 0, entry);
    at(&mut world, 1, ahead);
    world.actors[0].task = Some(Task::External(Cell::from(ahead)));
    world.actors[1].task = Some(Task::External(Cell::from(behind)));
    world.carry(1, Item::Crops, 3);
    world.walk(0, Stage::WalkToSite, vec![Cell::from(ahead)]);
    world.actors[0].route_goal = Some(Cell::from(ahead));
    world.walk(1, Stage::WalkToSite, vec![Cell::from(entry), Cell::from(behind)]);
    world.actors[1].route_goal = Some(Cell::from(behind));
    move_routes(&mut world, "S01");
    assert!(world.has_carry(1, Item::Crops, 3));
    assert_eq!(world.actor_tile(ActorKind::Xiaohe), ahead);
    assert_eq!(world.actor_tile(ActorKind::Ayu), behind);
    assert!(matches!(note(&world, id, [30, 60, 90][scene_mode(id)] * MINUTE).evidence,
        ObservationEvidence::Step(step) if step.from == entry && step.to != ahead && step.to != behind));
}

fn real_kitchen_detour(id: &str, now: u64) {
    let mut world = ready(now);
    let i = ActorKind::Ayu.index();
    let source = world.tool_pickup(i, Task::Eat);
    at(&mut world, i, source.tile());
    world.actors[i].task = Some(Task::Eat);
    world.actors[i].stage = Stage::Tool;
    world.stock.0[Item::Food as usize] = 0;
    world.step(i);
    assert_eq!(world.village.as_ref().unwrap().household_rules.empty_meal_source[i].unwrap().tile, source.tile());
    assert_eq!(world.active_village_observations().count(), 0);
    let crops = world.stock.count(Item::Crops);
    assert!(crops >= 2);
    assert!(world.begin(i, Task::Cook));
    finish(&mut world, i, Task::Cook);
    assert!(world.has_carry(i, Item::Food, 5) || world.has_carry(i, Item::Food, 7));
    assert!(world.stock.count(Item::Crops) < crops);
    assert!(matches!(note(&world, id, [30, 60, 90][scene_mode(id)] * MINUTE).evidence,
        ObservationEvidence::Step(step) if Cell::from(step.from).distance(source) <= 3));
}

fn real_cleanup(id: &str, now: u64, bearing: u8) {
    use scenic_resources::{Product, Work, WorkKind};
    let mut world = ready(now);
    let worker = ActorKind::Xiaohe.index();
    let offset = match bearing { b'3' => (-1, 0), b'4' => (0, 1), b'5' => (1, 0), _ => unreachable!() };
    let (work, item, quantity, source) = WILLOW_VILLAGE.scenic.iter().enumerate().find_map(|(i, object)| {
        if !land_reuse::RECLAIMED_HOME.contains(object.tile)
            && !land_reuse::RESTORED_FIELDS.iter().any(|field| field.rect.contains(object.tile)) {
            return None;
        }
        let rule = scenic_resources::recipe(i)?;
        let Product::Stock(item) = rule.product else { return None };
        if rule.roots != 0 || rule.amount < 2 || world.scenic_resource(i)?.stage != ScenicStage::Standing {
            return None;
        }
        let stand = object.tile.offset(offset.0, offset.1)?;
        let work = Work { id: i as u16, stand, kind: WorkKind::Cut };
        (work.valid() && world.can_walk(stand.x.into(), stand.y.into()))
            .then_some((work, item, u16::from(rule.amount), object.tile))
    }).expect("actual reclaimable rock or log and an authored work foot");
    at(&mut world, worker, work.stand);
    world.actors[worker].tool = work.tool();
    let original = world.stock.count(item);
    for _ in 0..scenic_resources::recipe(usize::from(work.id)).unwrap().shifts {
        world.commit_scenic(worker, work);
    }
    assert_eq!(world.scenic_resource(usize::from(work.id)).unwrap().pending, quantity as u8);
    world.actors[worker].tool = None;
    world.commit_scenic(worker, Work { kind: WorkKind::Gather, ..work });
    assert!(world.has_carry(worker, item, quantity));
    assert_eq!(world.stock.count(item), original);
    assert!(world.active_village_observations().next().is_none());
    let task = Task::Store;
    assert!(world.begin(worker, task));
    finish(&mut world, worker, task);
    assert_eq!(world.stock.count(item), original + quantity);
    assert_eq!(world.scenic_resource(usize::from(work.id)).unwrap().stage, ScenicStage::Cleared);
    let n = note(&world, id, [1, 4, 12][scene_mode(id)] * HOUR);
    assert_eq!(n.target, source);
    assert_eq!(n.evidence, ObservationEvidence::Removed(quantity as u8));
}

fn real_rod_cast(id: &str, clock: u64) {
    let mut world = ready(clock);
    let worker = ActorKind::Xiaohe.index();
    world.stock.0[Item::Food as usize] = 4;
    world.stock.0[Item::Crops as usize] = 2;
    let bank = world.site(worker, Task::Fish).tile();
    world.bowl = 3;
    assert_eq!(world.support_task(worker), Some(Task::Fish));
    assert!(world.begin(worker, Task::Fish));
    assert!(world.active_village_observations().next().is_none());
    reach_stage(&mut world, worker, Task::Fish, Stage::Work);
    assert_eq!(world.actor_tile(ActorKind::Xiaohe), bank);
    assert_eq!(world.actors[worker].tool, Some(Tool::FishingRod));
    let n = note(&world, id, [5, 15, 30][scene_mode(id)] * MINUTE);
    assert_eq!(world.ground(n.target.x.into(), n.target.y.into()), Ground::Water);
    assert_eq!(Cell::from(n.target).distance(Cell::from(bank)), 1);
    let fish = world.fish_in_pond;
    let due = world.actors[worker].until;
    set_time(&mut world, due);
    world.step(worker);
    assert_eq!(world.actors[worker].stage, Stage::Finish);
    assert!(matches!(
        (world.village.as_ref().unwrap().household_rules.cast[worker], world.fish_in_pond),
        (Some(CastResult::Empty), remaining) if remaining == fish
    ) || world.has_carry(worker, Item::Fish, 1) && world.fish_in_pond == fish - 1);
}

fn real_sunbreak(id: &str, season: YardSeason, hour: u64, bearing: u8) {
    let base_day = 21_504 + season as u64 * 28;
    for day in (base_day..base_day + 500).filter(|day| day % 112 / 28 == season as u64) {
        let now = day * DAY_MS + hour * HOUR;
        if LivingWorld::weather_at(now) != YardWeather::Clear
            || LivingWorld::weather_at(now - 3 * HOUR) != YardWeather::Clouds {
            continue;
        }
        let mut world = ready(now);
        let worker = ActorKind::Xiaohe.index();
        let stand = (16..28).flat_map(|y| (13..31).map(move |x| TilePos::new(x, y)))
            .find(|stand| [-1, 0, 1].into_iter().all(|x| {
                let tile = stand.offset(x, 0).unwrap();
                world.can_walk(tile.x.into(), tile.y.into())
                    && matches!(world.ground(tile.x.into(), tile.y.into()), Ground::Soil | Ground::Path | Ground::Grass)
            })).unwrap();
        at(&mut world, worker, stand);
        let Some(task @ Task::Detail(Detail::NoticeLight { focus: Some(focus), .. })) = world.notice_light(worker) else {
            continue;
        };
        let matches = match bearing {
            b'3' => stand.x < focus.x, b'4' => stand == focus, b'5' => stand.x > focus.x,
            _ => false,
        };
        if !matches { continue; }
        assert!(world.begin(worker, task));
        for _ in 0..5 {
            if world.actors[worker].stage == Stage::Finish { break; }
            let next = world.actors[worker].until;
            set_time(&mut world, next);
            world.step(worker);
        }
        assert_eq!(note(&world, id, [5, 15, 30][scene_mode(id)] * MINUTE).target, focus);
        if focus.x != stand.x { assert_eq!(world.actors[worker].facing_left, focus.x < stand.x); }
        return;
    }
    panic!("no actual clearing sky and available view");
}

fn real_sheltered_return(id: &str, season: YardSeason, hour: u64) {
    let base_day = 21_504 + season as u64 * 28;
    let mut world = ready(base_day * DAY_MS + 6 * HOUR);
    let cat = ActorKind::Orange.index();
    let door = world.home_door(0);
    at(&mut world, 0, village::poi("home_workshop.door").tile());
    at(&mut world, 1, village::poi("home_well").tile());
    let begin = if hour == 6 { 6 } else { 9 };
    let end = if hour == 6 { 6 } else { 15 };
    for rain in (base_day..base_day + 28).flat_map(|day| (begin..=end).step_by(3)
        .map(move |hour| day * DAY_MS + hour * HOUR))
    {
        if LivingWorld::weather_at(rain) != YardWeather::Rain
            || LivingWorld::weather_at(rain - 3 * HOUR) == YardWeather::Rain
            || LivingWorld::weather_at(rain + 3 * HOUR) == YardWeather::Rain {
            continue;
        }
        if world.legacy_behaviors.observed_ids().any(|seen| seen == id) { break; }
        if rain < world.now_ms { continue; }
        set_time(&mut world, rain);
        let from = door.tile().offset(0, 2).unwrap();
        let to = door.tile().offset(0, 1).unwrap();
        at(&mut world, cat, from);
        let patrol = Task::Explore(3);
        world.actors[cat].task = Some(patrol);
        world.walk(cat, Stage::WalkToSite, vec![Cell::from(to)]);
        world.actors[cat].route_goal = Some(world.cat_spot(3));
        let next = world.actors[cat].until;
        set_time(&mut world, next);
        world.step(cat);
        let shelter = Task::Detail(Detail::Shelter(door.tile()));
        finish(&mut world, cat, shelter);
        assert_eq!(world.actor_tile(ActorKind::Orange), door.tile());
        assert_eq!(world.cat_detail(), Some(Task::Detail(Detail::WaitUnderEave(door.tile()))));
        if !world.legacy_behaviors.observed_ids().any(|s| s.starts_with("S04")) {
            assert!(world.active_village_observations().all(|n| n.kind != Observation::RainShelter));
        }
        set_time(&mut world, rain + 3 * HOUR);
        assert_eq!(world.cat_detail(), Some(patrol));
        let mut reopened: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert!(reopened.begin(cat, patrol));
        finish(&mut reopened, cat, patrol);
        assert_eq!(reopened.actor_tile(ActorKind::Orange), world.cat_spot(3).tile());
        if reopened.legacy_behaviors.observed_ids().any(|seen| seen == id) {
            assert_eq!(note(&reopened, id, [5, 15, 30][scene_mode(id)] * MINUTE).target, door.tile());
            return;
        }
        world = reopened;
    }
    panic!("no real rain and preserved-route completion for {id}");
}

#[test]
fn registered_routes_leave_visible_traces_only_after_real_completed_work() {
    let mut failures = Vec::new();
    let cases = include_str!("../assets/maps/village-legacy-exact-scenes.tsv").lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(_, evidence)| evidence.starts_with("Completed route:") || evidence.starts_with("Outdoor observation:"));
    for (id, _) in cases {
        let (base, context) = id.split_once('~').unwrap();
        let context = context.as_bytes();
        let weather = [YardWeather::Clear, YardWeather::Clouds, YardWeather::Rain, YardWeather::Snow][usize::from(context[1] - b'0')];
        let season = [YardSeason::Spring, YardSeason::Autumn, YardSeason::Winter][usize::from(context[2] - b'0')];
        let minute = [380, 602, 1110, 1200][usize::from(context[0] - b'0')];
        let clock = time(season, minute, weather);
        let status = std::panic::catch_unwind(|| match base {
            "S01" => real_pass(id, clock, context[3]),
            "S14" => real_kitchen_detour(id, clock),
            "S09" => real_cleanup(id, if context[0] == b'3' { clock - 25 * MINUTE } else { clock }, context[3]),
            "H01-29" => real_rod_cast(id, clock - 40 * MINUTE),
            "S12" => real_sunbreak(id, season, [6, 9, 18][usize::from(context[0] - b'0')], context[3]),
            "S04" => real_sheltered_return(id, season, if context[0] == b'0' { 6 } else { 9 }),
            "S22" => {
                let now = boundary(season, if context[0] == b'1' { 12 } else { 18 }, weather, 1) + MINUTE;
                tree_warning(id, now - Task::FellTree(0).duration(), context[3]);
            }
            _ => unreachable!(),
        });
        if status.is_err() { failures.push(id); }
    }
    assert!(failures.is_empty(), "invalid variant cases: {failures:?}");
}
