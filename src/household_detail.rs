//! Short physical routines triggered by completed care or a real weather change.
use super::*;

const WEATHER_BLOCK: u64 = 3 * HOUR;
const CAT_WET_WINDOW: u64 = 6 * HOUR;
const CAT_LAMP: TilePos = TilePos::new(20, 17);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Detail {
    Groom(TilePos),
    Stretch(TilePos),
    Shelter(TilePos),
    DryCoat { stand: TilePos, lamp: TilePos },
    NoticeLight { stand: TilePos, spell: u64 },
    RegisterHarvest(TilePos),
    FileRepair { stand: TilePos, plot: u8 },
    WatchHarvest { stand: TilePos, plot: u8, farmer: u8, ends: u64 },
    RubDoor(TilePos),
    WatchEmptyHook { stand: TilePos, fisher: u8, ends: u64 },
    WaitAtRoadside(TilePos),
    ReturnFromYield(TilePos),
    WaitUnderEave(TilePos),
    PostShift { stand: TilePos, project: u8, next: u64 },
    Startled { stand: TilePos, origin: TilePos, heard: u64 },
    SmellBloom { stand: TilePos, tree: u8, opened: u64 },
    DressApron(TilePos),
    HangApron(TilePos),
    Contact(family_contacts::Work),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(world: &mut LivingWorld, who: usize, cell: Cell) {
        world.actors[who].cell = cell;
        world.actors[who].position = cell.point();
    }

    #[test]
    fn first_bloom_visit_keeps_that_flower_alive_and_is_once_per_new_day() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let cat = ActorKind::Orange.index();
        world.now_ms = (0..8).map(|cycle| (15 + cycle * 112) * DAY_MS + 8 * HOUR)
            .find(|&at| matches!(LivingWorld::weather_at(at), YardWeather::Clear | YardWeather::Clouds)).unwrap();
        let day = world.now_ms / DAY_MS;
        world.village.as_mut().unwrap().advance_orchard(day - 1);
        world.village.as_mut().unwrap().advance_orchard(day);
        let tree = village::tree_cells()[8];
        at(&mut world, cat, Cell(tree.0, tree.1 + 2));
        world.actors[cat].hunger = 20;
        world.actors[cat].thirst = 20;
        world.actors[cat].stamina = 90;
        let task @ Task::Detail(action @ Detail::SmellBloom { tree: 8, .. }) = world.cat_bloom_task().unwrap()
            else { panic!("no route to the new blossom"); };
        let before = world.village.as_ref().unwrap().orchard[0];
        world.commit(cat, task);
        assert!(world.cat_bloom_task().is_some());
        at(&mut world, cat, Cell::from(action.stand()));
        world.commit(cat, task);
        world.commit(cat, task);
        assert_eq!(world.village.as_ref().unwrap().orchard[0], before);
        assert!(world.cat_bloom_task().is_none());
        let mut reopened: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert!(reopened.cat_bloom_task().is_none());
        reopened.now_ms += DAY_MS;
        reopened.village.as_mut().unwrap().advance_orchard(day + 1);
        assert_eq!(reopened.first_bloom_today(8), None);
        assert!(reopened.cat_bloom_task().is_none());
    }

    #[test]
    fn apron_belongs_to_actual_dressing_and_stays_on_through_a_paid_recipe() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let kind = ActorKind::Xiaohe;
        let i = kind.index();
        let door = world.home_door(i);
        let dress = Task::Detail(Detail::DressApron(door.tile()));
        world.commit(i, dress);
        assert!(!world.household_wears_apron(kind));
        at(&mut world, i, door);
        world.commit(i, dress);
        assert!(world.household_wears_apron(kind));
        let mut world: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        let kitchen = world.building_door(2);
        at(&mut world, i, kitchen);
        world.commit(i, Task::Cook);
        assert!(!world.village.as_ref().unwrap().family_routines.detail.apron.unwrap().used);
        assert!(world.stock.take(Item::Crops, 2));
        world.carry(i, Item::Crops, 2);
        world.commit(i, Task::Cook);
        assert!(world.village.as_ref().unwrap().family_routines.detail.apron.unwrap().used);
        assert!(world.has_carry(i, Item::Food, 5));
        assert_eq!(world.apron_return(i), None);
        let pantry = world.site(i, Task::Store);
        at(&mut world, i, pantry);
        world.commit(i, Task::Store);
        let hang = world.apron_return(i).unwrap();
        at(&mut world, i, door);
        world.commit(i, hang);
        assert!(!world.household_wears_apron(kind));
    }

    #[test]
    fn cat_grooms_only_after_a_real_drink_and_only_once() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let cat = ActorKind::Orange.index();
        let dish = village::poi("cat_water");
        at(&mut world, cat, dish);
        world.actors[cat].hunger = 20;
        world.actors[cat].thirst = 70;
        world.bowl = 1;
        world.commit(cat, Task::Drink);
        assert_eq!(world.bowl, 0);
        assert_eq!(world.actors[cat].thirst, 8);
        assert_eq!(world.cat_detail(), Some(Task::Detail(Detail::Groom(dish.tile()))));
        let mut world: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        world.commit(cat, Task::Detail(Detail::Groom(dish.tile())));
        assert!(world.village.as_ref().unwrap().family_routines.detail.cat_water_at.is_none());
        world.commit(cat, Task::Detail(Detail::Groom(dish.tile())));
        assert_eq!(world.bowl, 0);
        assert_eq!(world.actors[cat].thirst, 8);
    }

    #[test]
    fn a_full_real_morning_sleep_schedules_the_door_stretch_except_under_urgent_needs() {
        let mut baseline = LivingWorld::new_village_wall_ms(0);
        let cat = ActorKind::Orange.index();
        let door = baseline.home_door(0);
        baseline.now_ms = (4..28).map(|day| day * DAY_MS + 7 * HOUR)
            .find(|&at| LivingWorld::weather_at(at) != YardWeather::Rain).unwrap();
        at(&mut baseline, cat, door);
        baseline.actors[cat].task = Some(Task::CatSleepInside(door.tile()));
        baseline.actors[cat].stage = Stage::Work;
        baseline.actors[cat].inside = true;
        baseline.actors[cat].since = baseline.now_ms - 3 * HOUR;
        baseline.actors[cat].until = baseline.now_ms;
        baseline.actors[cat].thirst = 45;
        let mut thirsty = baseline.clone();
        thirsty.actors[cat].thirst = 83;
        thirsty.step(cat);
        assert_eq!(thirsty.actors[cat].stage, Stage::Finish);
        baseline.step(cat);
        let stretch = Task::Detail(Detail::Stretch(door.tile()));
        assert_eq!(baseline.actors[cat].task, Some(stretch));
        baseline.commit(cat, stretch);
        assert_eq!(baseline.village.as_ref().unwrap().family_routines.detail.cat_awoke_at, None);
    }

    #[test]
    fn cat_pressing_shelter_and_drying_need_real_rain_and_a_lit_authored_lamp() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let cat = ActorKind::Orange.index();
        let door = world.home_door(0);
        world.now_ms = (32..32 + 80 * 8).map(|spell| spell * WEATHER_BLOCK).find(|now| {
            LivingWorld::weather_at(*now) == YardWeather::Rain
                && LivingWorld::weather_at(*now + WEATHER_BLOCK) != YardWeather::Rain
                && (18 * HOUR..21 * HOUR).contains(&((*now + WEATHER_BLOCK) % DAY_MS))
        }).unwrap();
        world.actors[cat].hunger = 20;
        world.actors[cat].thirst = 20;
        let landing = Cell(door.0 + 1, door.1);
        at(&mut world, cat, landing);
        world.observe_cat_weather_edge(ActorKind::Orange, door, landing);
        assert!(world.cat_coat_wet());
        assert_eq!(world.cat_detail(), Some(Task::Detail(Detail::Shelter(door.tile()))));
        world.commit(cat, Task::Detail(Detail::Shelter(door.tile())));
        assert_eq!(world.cat_detail(), Some(Task::Detail(Detail::Shelter(door.tile()))));
        at(&mut world, cat, door);
        world.commit(cat, Task::Detail(Detail::Shelter(door.tile())));
        assert!(world.cat_coat_wet());
        world.now_ms += WEATHER_BLOCK;
        assert!(world.household_lamp_lit());
        let task @ Task::Detail(action @ Detail::DryCoat { .. }) = world.cat_detail().unwrap() else { panic!("drying station unavailable") };
        at(&mut world, cat, Cell::from(action.stand()));
        world.commit(cat, task);
        assert!(!world.cat_coat_wet());
    }

    #[test]
    fn same_pond_fish_reaches_the_real_plate_only_once_through_reload() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let (worker, cat) = (ActorKind::Ayu.index(), ActorKind::Orange.index());
        let dish = village::poi("cat_water");
        at(&mut world, worker, dish);
        let stock = world.stock.clone();
        world.carry(worker, Item::Fish, 1);
        world.village.as_mut().unwrap().household_rules.cast[worker] = Some(CastResult::FishInHand);
        world.village.as_mut().unwrap().opening.cat_food = true;
        world.commit(worker, Task::ServeCatFish);
        assert!(world.has_carry(worker, Item::Fish, 1));
        assert!(!world.cat_fish_served());
        world.village.as_mut().unwrap().opening.cat_food = false;
        world.commit(worker, Task::ServeCatFish);
        assert!(world.cat_fish_served());
        assert_eq!(world.actors[worker].carried, None);
        let mut world: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        at(&mut world, cat, dish);
        world.actors[cat].hunger = 60;
        world.commit(cat, Task::CatSmellMeal);
        world.commit(cat, Task::CatEat);
        assert!(!world.cat_fish_served());
        assert!(!world.cat_food_served());
        assert_eq!(world.actors[cat].hunger, 4);
        world.commit(cat, Task::CatEat);
        assert_eq!(world.actors[cat].hunger, 4);
        assert_eq!(world.stock, stock);
    }

    #[test]
    fn brightness_notice_belongs_to_one_real_cloud_break_per_actor() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let worker = ActorKind::Xiaohe.index();
        let gate = village::poi("home_gate");
        at(&mut world, worker, gate);
        world.actors[worker].hunger = 20;
        world.actors[worker].thirst = 20;
        world.now_ms = (33..33 + 56 * 8).map(|spell| spell * WEATHER_BLOCK).find(|now| {
            (6 * HOUR..18 * HOUR).contains(&(now % DAY_MS))
                && LivingWorld::weather_at(*now) == YardWeather::Clear
                && LivingWorld::weather_at(*now - WEATHER_BLOCK) == YardWeather::Clouds
        }).unwrap();
        let task = world.notice_light(worker).unwrap();
        world.now_ms += MINUTE;
        world.commit(worker, task);
        assert!(world.notice_light(worker).is_none());
        assert_eq!(world.village.as_ref().unwrap().family_routines.detail.bright_seen[worker], world.current_bright_spell());
    }

    #[test]
    fn interrupted_patrol_waits_at_its_eave_then_finishes_its_original_leg() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let cat = ActorKind::Orange.index();
        let door = world.home_door(0);
        at(&mut world, 0, village::poi("home_workshop.door"));
        at(&mut world, 1, village::poi("home_well"));
        world.now_ms = (40..40 + 56 * 8).map(|spell| spell * WEATHER_BLOCK).find(|now| {
            LivingWorld::weather_at(*now) == YardWeather::Rain
                && LivingWorld::weather_at(*now - WEATHER_BLOCK) != YardWeather::Rain
                && LivingWorld::weather_at(*now + WEATHER_BLOCK) != YardWeather::Rain
        }).unwrap();
        let step = Cell(door.0 + 1, door.1);
        at(&mut world, cat, step);
        world.actors[cat].hunger = 20;
        world.actors[cat].thirst = 20;
        world.observe_cat_weather_edge(ActorKind::Orange, door, step);
        assert!(world.divert_patrol_to_eave(cat, Task::Explore(3)));
        assert!(!world.village.as_ref().unwrap().family_routines.detail.rain_patrol.unwrap().arrived);
        at(&mut world, cat, door);
        world.commit(cat, Task::Detail(Detail::Shelter(door.tile())));
        assert_eq!(world.cat_detail(), Some(Task::Detail(Detail::WaitUnderEave(door.tile()))));
        world.now_ms += WEATHER_BLOCK;
        assert_eq!(world.cat_detail(), Some(Task::Explore(3)));
        world.mark_patrol_resume(cat, Task::Explore(2));
        assert!(!world.village.as_ref().unwrap().family_routines.detail.rain_patrol.unwrap().resumed);
        world.mark_patrol_resume(cat, Task::Explore(3));
        let goal = world.cat_spot(3);
        at(&mut world, cat, goal);
        world.commit(cat, Task::Explore(3));
        assert!(world.village.as_ref().unwrap().family_routines.detail.rain_patrol.is_none());
    }

    #[test]
    fn cat_waits_at_shoulder_for_the_loaded_pass_and_reoccupies_origin() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let cat = ActorKind::Orange.index();
        let carrier = ActorKind::Ayu.index();
        let (origin, shoulder, departed) = (Cell(20, 20), Cell(20, 21), Cell(21, 20));
        at(&mut world, cat, origin);
        world.actors[cat].task = Some(Task::Explore(0));
        world.carry(carrier, Item::Crops, 1);
        world.remember_cat_yield(cat, carrier, shoulder);
        world.see_cat_yield_edge(carrier, origin.tile(), departed.tile());
        assert!(!world.village.as_ref().unwrap().family_routines.detail.cat_yield.unwrap().passed);
        at(&mut world, cat, shoulder);
        world.see_cat_yield_edge(cat, origin.tile(), shoulder.tile());
        world.see_cat_yield_edge(carrier, origin.tile(), departed.tile());
        world.commit(cat, Task::Detail(Detail::ReturnFromYield(origin.tile())));
        assert!(world.village.as_ref().unwrap().family_routines.detail.cat_yield.is_some());
        at(&mut world, cat, origin);
        world.commit(cat, Task::Detail(Detail::ReturnFromYield(origin.tile())));
        assert!(world.village.as_ref().unwrap().family_routines.detail.cat_yield.is_none());
        assert!(world.has_carry(carrier, Item::Crops, 1));
    }

    #[test]
    fn notebook_tally_requires_the_batch_to_be_processed_and_stowed() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.now_ms = (4..28).map(|d| d * DAY_MS + 15 * HOUR).find(|&at| {
            matches!(LivingWorld::weather_at(at), YardWeather::Clear | YardWeather::Clouds)
        }).unwrap();
        let worker = ActorKind::Xiaohe.index();
        let warehouse = world.site(worker, Task::Store);
        world.carry(worker, Item::Crops, 2);
        at(&mut world, worker, warehouse);
        world.commit(worker, Task::Store);
        assert_eq!(world.season_harvest_register(), None);

        world.carry(worker, Item::Crops, 3);
        world.actors[worker].harvest_processing = 1;
        for phase in 1..=4 {
            let task = Task::ProcessHarvest(phase);
            let foot = world.site(worker, task);
            at(&mut world, worker, foot);
            world.commit(worker, task);
        }
        assert_eq!(world.season_harvest_register(), None);
        at(&mut world, worker, warehouse);
        world.commit(worker, Task::Store);
        assert_eq!(world.season_harvest_register(), Some((0, 3)));
        let table = village::poi("home_table");
        let write = Task::Detail(Detail::RegisterHarvest(table.tile()));
        world.commit(worker, write);
        assert_eq!(world.season_harvest_register(), Some((0, 3)));
        at(&mut world, worker, table);
        world.commit(worker, write);
        world.commit(worker, write);
        assert_eq!(world.season_harvest_register(), Some((3, 0)));
    }

    #[test]
    fn notice_sign_reports_only_the_actual_paused_public_shift() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.now_ms = (4..28).map(|d| d * DAY_MS + 15 * HOUR).find(|&at| {
            matches!(LivingWorld::weather_at(at), YardWeather::Clear | YardWeather::Clouds)
        }).unwrap();
        let worker = ActorKind::Ayu.index();
        let stand = village::poi("market_notice");
        at(&mut world, worker, stand);
        world.actors[worker].hunger = 20;
        world.actors[worker].thirst = 20;
        let id = CHAPTERS.iter().position(|c| c.work_shifts >= 2).unwrap() as u8;
        let next = world.now_ms + HOUR;
        world.story.running = Some(Project { id, begun: world.now_ms, shifts: 1, next_shift: next });
        let task = world.next_shift_notice_task(worker).unwrap();
        world.story.running.as_mut().unwrap().next_shift += 10 * MINUTE;
        world.commit(worker, task);
        assert_eq!(world.yard_posted_construction_shift(), None);
        let updated = world.next_shift_notice_task(worker).unwrap();
        world.commit(worker, updated);
        assert_eq!(world.yard_posted_construction_shift(), Some((stand.tile(), next + 10 * MINUTE)));
        world.now_ms = next + 11 * MINUTE;
        assert_eq!(world.yard_posted_construction_shift(), None);
    }

    #[test]
    fn paid_garden_planter_keeps_the_original_shift_and_cannot_install_early() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let worker = ActorKind::Ayu.index();
        let before = ImprovementOrder { fixture: Fixture::GardenPlanter, phase: 0 };
        let order = ImprovementOrder { phase: 1, ..before };
        let task = Task::Improve(order);
        world.story.finished = CHAPTERS.len() as u8;
        world.now_ms = 21 * DAY_MS;
        world.story.improvements.commit(before, world.now_ms);
        world.now_ms += DAY_MS;
        let foot = world.site(worker, task);
        at(&mut world, worker, foot);
        world.actors[worker].tool = Some(Tool::Hammer);
        world.carry(worker, Item::Wood, 1);
        world.actors[worker].stage = Stage::Prepare;
        world.actors[worker].task = Some(task);
        let started = world.now_ms;
        world.step(worker);
        for _ in 0..4 {
            world.now_ms = world.actors[worker].until;
            assert!(!world.story.improvements.working(Fixture::GardenPlanter));
            assert!(world.has_carry(worker, Item::Wood, 1));
            world.step(worker);
        }
        assert_eq!(world.now_ms, started + task.duration());
        assert!(world.story.improvements.working(Fixture::GardenPlanter));
        assert!(world.actors[worker].carried.is_none());
        assert_eq!(world.village_planter_joint_count(ActorKind::Ayu), None);
    }

    #[test]
    fn real_felled_tree_alert_reacts_once_only_after_leaving_the_landing() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let cutter = ActorKind::Ayu.index();
        let cat = ActorKind::Orange.index();
        let id = world.trees.iter().position(|tree| !tree.protected && tree.stage == TreeStage::Standing).unwrap();
        let origin = Cell::at(world.trees[id].position).unwrap();
        let cut = Task::FellTree(id as u8);
        let stand = world.site(cutter, cut);
        at(&mut world, cutter, stand);
        world.actors[cutter].tool = Some(Tool::Axe);
        world.actors[cat].hunger = 20;
        world.actors[cat].thirst = 20;
        assert!(world.startled_task(cat).is_none());
        world.commit(cutter, cut);
        assert_eq!(world.trees[id].stage, TreeStage::Felled);
        let mut route = None;
        for y in -3..=3 {
            for x in -3..=3 {
                let Some(start) = origin.tile().offset(x,y) else { continue; };
                let from = Cell::from(start);
                if from.distance(origin) > 5 || !world.valid_world_cell(from) { continue; }
                at(&mut world, cat, from);
                if let Some(task @ Task::Detail(action @ Detail::Startled { .. })) = world.startled_task(cat) {
                    route = Some((from, task, action));
                    break;
                }
            }
            if route.is_some() { break; }
        }
        let (from, task, action) = route.unwrap();
        assert_eq!(Cell::from(action.stand()).distance(origin), from.distance(origin) + 2);
        world.commit(cat, task);
        assert!(world.startled_task(cat).is_some());
        at(&mut world, cat, Cell::from(action.stand()));
        world.commit(cat, task);
        assert!(world.startled_task(cat).is_none());
        assert!(world.startled_task(cutter).is_none());
    }
}

impl Detail {
    pub(super) fn stand(self) -> TilePos {
        match self {
            Self::Groom(stand) | Self::Stretch(stand) | Self::Shelter(stand) | Self::RegisterHarvest(stand) | Self::RubDoor(stand)
            | Self::DryCoat { stand, .. } | Self::NoticeLight { stand, .. } | Self::FileRepair { stand, .. }
            | Self::WatchHarvest { stand, .. } | Self::WatchEmptyHook { stand, .. }
            | Self::WaitAtRoadside(stand) | Self::ReturnFromYield(stand) | Self::WaitUnderEave(stand)
            | Self::PostShift { stand, .. } | Self::Startled { stand, .. }
            | Self::SmellBloom { stand, .. } | Self::DressApron(stand) | Self::HangApron(stand) => stand,
            Self::Contact(work) => work.stand(),
        }
    }

    pub(super) fn valid(self) -> bool {
        self.stand().index().is_some() && match self {
            Self::DryCoat { stand, lamp } => lamp.index().is_some() && Cell::from(stand).distance(Cell::from(lamp)) == 1,
            Self::FileRepair { plot, .. } => usize::from(plot) < village::ORIGINAL_VILLAGE_PLOTS && plot != grain::FIELD,
            Self::WatchHarvest { farmer, plot, .. } => farmer < 2 && usize::from(plot) < village::VILLAGE_PLOT_CAP
                && plot != grain::FIELD,
            Self::WatchEmptyHook { fisher, .. } => fisher < 2,
            Self::PostShift { project, .. } => usize::from(project) < CHAPTERS.len(),
            Self::Startled { stand, origin, .. } => origin.index().is_some() && stand != origin,
            Self::SmellBloom { stand, tree, .. } => (8..12).contains(&tree)
                && Cell::from(stand).distance(village::tree_cells()[usize::from(tree)]) == 1,
            Self::Contact(work) => work.valid(),
            _ => true,
        }
    }

    pub(super) fn actor_accepts(self, actor: usize) -> bool {
        match self {
            Self::NoticeLight { .. } | Self::Startled { .. } => actor < 3,
            Self::RegisterHarvest(_) | Self::FileRepair { .. } | Self::PostShift { .. }
            | Self::DressApron(_) | Self::HangApron(_) => actor < 2,
            Self::Contact(work) => work.actor_accepts(actor),
            _ => actor == ActorKind::Orange.index(),
        }
    }

    pub(super) fn minutes(self) -> u64 {
        match self {
            Self::Stretch(_) | Self::SmellBloom { .. } | Self::DressApron(_) | Self::HangApron(_) => 2,
            Self::Shelter(_) | Self::WaitUnderEave(_) => 6,
            Self::Contact(work) => work.minutes(),
            _ => 1,
        }
    }

    pub(super) fn activity(self) -> Activity {
        match self {
            Self::Groom(_) => Activity::GroomWhiskers,
            Self::Stretch(_) => Activity::StretchAtDoor,
            Self::Shelter(_) => Activity::ShelterFromRain,
            Self::DryCoat { .. } => Activity::DryCoat,
            Self::NoticeLight { .. } => Activity::NoticeLight,
            Self::RegisterHarvest(_) => Activity::RecordHarvest,
            Self::FileRepair { .. } => Activity::RecordRepair,
            Self::WatchHarvest { .. } => Activity::WatchHarvest,
            Self::RubDoor(_) => Activity::RubDoor,
            Self::WatchEmptyHook { .. } => Activity::WatchEmptyHook,
            Self::WaitAtRoadside(_) | Self::ReturnFromYield(_) => Activity::YieldToCarrier,
            Self::WaitUnderEave(_) => Activity::ShelterFromRain,
            Self::PostShift { .. } => Activity::PostNextShift,
            Self::Startled { .. } => Activity::MoveAwayFromNoise,
            Self::SmellBloom { .. } => Activity::SmellFirstBloom,
            Self::DressApron(_) | Self::HangApron(_) => Activity::ChangeClothes,
            Self::Contact(work) => work.activity(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    pub(super) cat_water_at: Option<u64>,
    pub(super) cat_awoke_at: Option<u64>,
    pub(super) cat_wet_at: Option<u64>,
    pub(super) cat_fish_plate: bool,
    cat_fish_day: Option<u64>,
    cat_shelter_spell: Option<u64>,
    bright_seen: [Option<u64>; 3],
    pub(super) garden_clothes_ready: [bool; 2],
    pub(super) rain_shift_done: [bool; 2],
    raincoat_hook: [bool; 2],
    pub(super) processed_harvest: [bool; 2],
    harvest_log: Option<HarvestLog>,
    repair_log: Option<RepairNotice>,
    pub(super) full_patrol: u8,
    pub(super) cat_fur_at: Option<(TilePos, u64)>,
    empty_hook: Option<EmptyHook>,
    cat_yield: Option<CatYield>,
    rain_patrol: Option<RainPatrol>,
    posted_shift: Option<(u8, u64)>,
    pub(super) planter_joints: [Option<(u8, u64)>; 2],
    last_impact: Option<Impact>,
    watched_harvest: Option<(u8, u64)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    smelled_bloom: Option<(u8, u64)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    apron: Option<Apron>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_apron_day: Option<u64>,
    #[serde(skip_serializing_if = "family_contacts::State::is_empty")]
    pub(super) contacts: family_contacts::State,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Apron { owner: u8, dressed: u64, used: bool }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyHook { fisher: u8, bank: TilePos, ends: u64 }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatYield { worker: u8, origin: TilePos, shoulder: TilePos, expires: u64, aside: bool, passed: bool }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RainPatrol { spot: u8, shelter: TilePos, expires: u64, arrived: bool, resumed: bool }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Impact { source: u8, location: TilePos, at: u64, responded: u8 }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HarvestLog {
    season: u64,
    pending: u16,
    total: u16,
    last_written_day: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairNotice { plot: u8, reported: u64, fixed: Option<u64>, filed: bool }

impl State {
    pub(super) fn is_empty(&self) -> bool { self == &Self::default() }

    pub(super) fn valid(&self, now: u64, cat_food: bool, actors: &[Actor; 3]) -> bool {
        self.contacts.valid(now)
            && [self.cat_water_at, self.cat_awoke_at, self.cat_wet_at].into_iter()
            .all(|at| at.is_none_or(|at| at <= now))
            && self.cat_shelter_spell.is_none_or(|spell| spell <= now / WEATHER_BLOCK)
            && self.bright_seen.iter().all(|spell| spell.is_none_or(|spell| spell <= now / WEATHER_BLOCK))
            && self.cat_fish_day.is_none_or(|day| day <= now / DAY_MS)
            && (!self.cat_fish_plate || cat_food && self.cat_fish_day.is_some())
            && self.harvest_log.is_none_or(|log| log.season <= now / (28 * DAY_MS)
                && log.last_written_day.is_none_or(|day| day <= now / DAY_MS && day / 28 == log.season))
            && self.repair_log.is_none_or(|notice| usize::from(notice.plot) < village::ORIGINAL_VILLAGE_PLOTS
                && notice.plot != grain::FIELD && notice.reported <= now
                && notice.fixed.is_none_or(|time| (notice.reported..=now).contains(&time))
                && (!notice.filed || notice.fixed.is_some()))
            && self.full_patrol <= 0b111111
            && self.cat_fur_at.is_none_or(|(tile, day)| tile.index().is_some() && day <= now / DAY_MS)
            && self.empty_hook.is_none_or(|wait| wait.fisher < 2 && wait.bank.index().is_some() && wait.ends <= now.saturating_add(4 * MINUTE))
            && self.cat_yield.is_none_or(|mark| mark.worker < 2 && mark.origin.index().is_some()
                && mark.shoulder.index().is_some() && Cell::from(mark.origin).distance(Cell::from(mark.shoulder)) == 1
                && (!mark.passed || mark.aside) && mark.expires <= now.saturating_add(20 * MINUTE))
            && self.rain_patrol.is_none_or(|route| usize::from(route.spot) < village::VILLAGE_CAT_SPOTS
                && route.expires <= now.saturating_add(6 * HOUR) && route.shelter.index().is_some()
                && (!route.resumed || route.arrived))
            && self.posted_shift.is_none_or(|(project, next)| usize::from(project) < CHAPTERS.len()
                && next <= now.saturating_add(7 * DAY_MS))
            && self.planter_joints.iter().enumerate().all(|(i, progress)| progress.is_none_or(|(n, section)| {
                n < 4 && (2 * MINUTE..=5 * MINUTE).contains(&section) && actors[i].stage == Stage::Work
                    && matches!(actors[i].task, Some(Task::Improve(order)) if order.fixture == Fixture::GardenPlanter && order.phase == 1)
            }))
            && self.processed_harvest.iter().enumerate().all(|(i, packed)| !packed || {
                actors[i].carried == Some(Item::Crops) && actors[i].harvest_processing == 0
            })
            && self.last_impact.is_none_or(|impact| impact.source < 2 && impact.responded <= 0b111
                && impact.location.index().is_some() && impact.at <= now)
            && self.watched_harvest.is_none_or(|(plot, day)| usize::from(plot) < village::VILLAGE_PLOT_CAP
                && day <= now / DAY_MS)
            && self.smelled_bloom.is_none_or(|(tree, day)| (8..12).contains(&tree) && day <= now / DAY_MS)
            && self.last_apron_day.is_none_or(|day| day <= now / DAY_MS)
            && self.apron.is_none_or(|apron| apron.owner < 2 && apron.dressed <= now
                && self.last_apron_day == Some(apron.dressed / DAY_MS))
    }

    pub(super) fn report_flood(&mut self, plot: u8, now: u64) {
        if plot != grain::FIELD {
            self.repair_log = Some(RepairNotice { plot, reported: now, fixed: None, filed: false });
        }
    }

    pub(super) fn repair_flood(&mut self, plot: u8, now: u64) {
        if let Some(notice) = &mut self.repair_log && notice.plot == plot {
            notice.fixed = Some(now);
        }
    }
}

impl LivingWorld {
    pub fn household_wears_apron(&self, kind: ActorKind) -> bool {
        self.village.as_ref().is_some_and(|state| state.family_routines.detail.apron
            .is_some_and(|apron| apron.owner == kind as u8))
    }

    pub(super) fn prepare_cooking_apron(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        let detail = &state.family_routines.detail;
        let actor = self.actors.get(i)?;
        let door = self.home_door(i);
        let day = self.now_ms / DAY_MS;
        (i < 2 && state.opening.completed("Q03") && detail.apron.is_none()
            && detail.last_apron_day.is_none_or(|last| day.saturating_sub(last) >= 7)
            && !self.actors[..2].iter().any(|other| matches!(other.task, Some(Task::Detail(Detail::DressApron(_)))))
            && actor.carried.is_none() && actor.tool.is_none() && !actor.moving_box
            && actor.hunger < 48 && actor.thirst < 50 && actor.stamina >= 60
            && self.stock.count(Item::Food) >= 4
            && (8..17).contains(&(self.minute_of_day() / 60))
            && matches!(self.weather(), YardWeather::Clear | YardWeather::Clouds)
            && actor.cell.distance(door) <= 18
            && self.quiet_visit_site_available(i, door.tile()))
            .then_some(Task::Detail(Detail::DressApron(door.tile())))
    }

    pub(super) fn apron_return(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        let apron = state.family_routines.detail.apron?;
        let actor = self.actors.get(i)?;
        let door = self.home_door(i);
        (usize::from(apron.owner) == i
            && (apron.used || self.now_ms.saturating_sub(apron.dressed) >= HOUR)
            && actor.carried.is_none() && actor.tool.is_none()
            && self.quiet_visit_site_available(i, door.tile()))
            .then_some(Task::Detail(Detail::HangApron(door.tile())))
    }

    pub(super) fn finish_apron_meal(&mut self, i: usize) {
        if i >= 2 || self.actors[i].cell != self.building_door(2)
            || self.actors[i].carried != Some(Item::Food) { return; }
        let Some(state) = &mut self.village else { return; };
        let Some(apron) = &mut state.family_routines.detail.apron else { return; };
        if usize::from(apron.owner) != i || apron.used || self.now_ms.saturating_sub(apron.dressed) > 2 * HOUR {
            return;
        }
        apron.used = true;
        self.record_household_completion("H01-50", i, self.building_door(2).tile(), LegacyPlace::Front);
    }

    pub(super) fn note_timber_impact(&mut self, i: usize, at: TilePos) {
        if i < 2 && let Some(state) = &mut self.village {
            state.family_routines.detail.last_impact = Some(Impact {
                source: i as u8, location: at, at: self.now_ms, responded: 0,
            });
        }
    }

    pub(super) fn startled_task(&self, i: usize) -> Option<Task> {
        let state = self.village.as_ref()?;
        let impact = state.family_routines.detail.last_impact?;
        let actor = &self.actors[i];
        let origin = Cell::from(impact.location);
        let distance = actor.cell.distance(origin);
        if i == usize::from(impact.source) || impact.responded & (1 << i) != 0
            || self.now_ms.saturating_sub(impact.at) >= 4 * MINUTE || distance > 5
            || actor.inside || actor.outside || actor.carried.is_some() || actor.tool.is_some()
            || self.mat_in_hand(i) || self.landscape_carried_fibre(actor.kind) > 0
            || actor.hunger >= 62 || actor.thirst >= 62
        { return None; }
        let stand = [(0,-1),(-1,0),(1,0),(0,1)].into_iter().find_map(|(x,y)| {
            let middle = actor.cell.tile().offset(x,y)?;
            let outer = middle.offset(x,y)?;
            (Cell::from(outer).distance(origin) == distance + 2
                && self.quiet_visit_site_available(i, middle)
                && self.quiet_visit_site_available(i, outer)).then_some(outer)
        })?;
        Some(Task::Detail(Detail::Startled { stand, origin: impact.location, heard: impact.at }))
    }

    pub fn yard_posted_construction_shift(&self) -> Option<(TilePos, u64)> {
        let (id, time) = self.village.as_ref()?.family_routines.detail.posted_shift?;
        let project = self.story.running?;
        (project.id == id && project.next_shift == time && time >= self.now_ms)
            .then_some((village::poi("market_notice").tile(), time))
    }

    pub(super) fn next_shift_notice_task(&self, i: usize) -> Option<Task> {
        if i >= 2 || !(8..18).contains(&(self.minute_of_day() / 60))
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            || self.resolve_poi("market_notice").is_none() { return None; }
        let detail = &self.village.as_ref()?.family_routines.detail;
        let project = self.story.running?;
        let actor = &self.actors[i];
        let table = village::poi("market_notice");
        let task = Task::Detail(Detail::PostShift { stand: table.tile(), project: project.id, next: project.next_shift });
        (project.shifts > 0 && project.shifts < CHAPTERS[usize::from(project.id)].work_shifts
            && project.next_shift > self.now_ms + 25 * MINUTE && detail.posted_shift != Some((project.id, project.next_shift))
            && actor.carried.is_none() && actor.tool.is_none() && actor.cell.distance(table) <= 85
            && actor.hunger < 52 && actor.thirst < 54 && actor.stamina >= 40
            && !self.task_in_use(task) && self.quiet_visit_site_available(i, table.tile())).then_some(task)
    }

    pub fn village_planter_joint_count(&self, kind: ActorKind) -> Option<u8> {
        self.village.as_ref()?.family_routines.detail.planter_joints.get(kind.index()).copied().flatten().map(|(joints, _)| joints)
    }

    pub(super) fn divert_patrol_to_eave(&mut self, i: usize, original: Task) -> bool {
        let Task::Explore(spot) = original else { return false; };
        let shelter = self.home_door(0);
        let actor = &self.actors[i];
        if i != ActorKind::Orange.index() || self.weather() != YardWeather::Rain
            || self.now_ms % WEATHER_BLOCK > 5 * MINUTE || self.now_ms < WEATHER_BLOCK
            || Self::weather_at(self.now_ms - WEATHER_BLOCK) == YardWeather::Rain
            || actor.hunger >= 65 || actor.thirst >= 60 || actor.cell == shelter || actor.cell.distance(shelter) > 12
            || !self.quiet_visit_site_available(i, shelter.tile())
            || self.village.as_ref().is_none_or(|state| state.family_routines.detail.rain_patrol
                .is_some_and(|route| route.expires >= self.now_ms))
        { return false; }
        if !self.begin(i, Task::Detail(Detail::Shelter(shelter.tile()))) { return false; }
        self.village.as_mut().unwrap().family_routines.detail.rain_patrol = Some(RainPatrol {
            spot, shelter: shelter.tile(), expires: self.now_ms + 6 * HOUR, arrived: false, resumed: false,
        });
        true
    }

    pub(super) fn mark_patrol_resume(&mut self, i: usize, task: Task) {
        if i != ActorKind::Orange.index() || !matches!(self.weather(), YardWeather::Clear | YardWeather::Clouds) { return; }
        let Some(state) = &mut self.village else { return; };
        if let Some(route) = &mut state.family_routines.detail.rain_patrol && route.arrived
            && route.expires >= self.now_ms && task == Task::Explore(route.spot)
            && self.actors[i].cell.tile() == route.shelter
        { route.resumed = true; }
    }

    pub(super) fn finish_sheltered_patrol(&mut self, i: usize, spot: u8) {
        if i != ActorKind::Orange.index() || !matches!(self.weather(), YardWeather::Clear | YardWeather::Clouds) { return; }
        let Some(state) = &mut self.village else { return; };
        if let Some(route) = state.family_routines.detail.rain_patrol
            && route.spot == spot && route.resumed && route.expires >= self.now_ms
        {
            state.family_routines.detail.rain_patrol = None;
            self.record_household_completion("S04", i, route.shelter, LegacyPlace::Front);
        }
    }

    pub(super) fn remember_cat_yield(&mut self, i: usize, other: usize, shoulder: Cell) {
        if i != ActorKind::Orange.index() || other >= 2 || self.actors[other].carried.is_none() && !self.actors[other].moving_box
            || !matches!(self.actors[i].task, Some(Task::Explore(_))) || self.actors[other].outside
        { return; }
        let Some(state) = &mut self.village else { return; };
        let detail = &mut state.family_routines.detail;
        if detail.cat_yield.is_some_and(|mark| mark.expires >= self.now_ms) { return; }
        detail.cat_yield = Some(CatYield { worker: other as u8, origin: self.actors[i].cell.tile(),
            shoulder: shoulder.tile(), expires: self.now_ms + 20 * MINUTE, aside: false, passed: false });
    }

    pub(super) fn see_cat_yield_edge(&mut self, who: usize, from: TilePos, to: TilePos) {
        let loaded = self.actors[who].carried.is_some() || self.actors[who].moving_box;
        let Some(state) = &mut self.village else { return; };
        if let Some(mark) = &mut state.family_routines.detail.cat_yield && mark.expires >= self.now_ms {
            mark.aside |= who == ActorKind::Orange.index() && from == mark.origin && to == mark.shoulder;
            mark.passed |= who == usize::from(mark.worker) && mark.aside && loaded && from == mark.origin && to != mark.shoulder;
        }
    }

    fn legal_cat_shoulder(&self, bank: TilePos, maximum_distance: u8) -> Option<TilePos> {
        let i = ActorKind::Orange.index();
        let cat = &self.actors[i];
        [(0,-1), (-1,0), (1,0), (0,1)].into_iter().filter_map(|(x,y)| bank.offset(x,y))
            .filter(|&tile| self.quiet_visit_site_available(i, tile))
            .map(|tile| (cat.cell.distance(Cell::from(tile)), tile))
            .filter(|(distance, _)| *distance <= u16::from(maximum_distance))
            .min().map(|(_, tile)| tile)
    }

    fn first_bloom_today(&self, tree: u8) -> Option<u64> {
        let index = usize::from(tree.checked_sub(8)?);
        let flower = self.village.as_ref()?.orchard.get(index)?;
        let today = self.now_ms / DAY_MS;
        (flower.phase == OrchardPhase::Flower && flower.flower_opened_day == Some(today)).then_some(today)
    }

    fn cat_bloom_task(&self) -> Option<Task> {
        let cat = &self.actors[ActorKind::Orange.index()];
        let detail = &self.village.as_ref()?.family_routines.detail;
        if cat.hunger >= 48 || cat.thirst >= 46 || cat.stamina < 65
            || !(7..15).contains(&(self.minute_of_day() / 60))
            || !matches!(self.weather(), YardWeather::Clear | YardWeather::Clouds)
        {
            return None;
        }
        (8..12).filter_map(|tree| {
            let opened = self.first_bloom_today(tree)?;
            if detail.smelled_bloom.is_some_and(|(_, prior)| prior == opened) { return None; }
            let root = village::tree_cells()[usize::from(tree)];
            let stand = self.legal_cat_shoulder(root.tile(), 83)?;
            Some((cat.cell.distance(Cell::from(stand)), tree, opened, stand))
        }).min().map(|(_, tree, opened, stand)| Task::Detail(Detail::SmellBloom { stand, tree, opened }))
    }

    pub fn cat_bloom_target(&self) -> Option<(TilePos, TilePos)> {
        let actor = &self.actors[ActorKind::Orange.index()];
        let Task::Detail(Detail::SmellBloom { stand, tree, opened }) = actor.task? else { return None; };
        (actor.stage == Stage::Work && self.first_bloom_today(tree) == Some(opened))
            .then(|| (stand, village::tree_cells()[usize::from(tree)].tile()))
    }

    pub(super) fn leave_time_for_empty_hook(&mut self, fisher: usize) -> bool {
        let cat = &self.actors[ActorKind::Orange.index()];
        if fisher >= 2 || cat.hunger >= 65 || cat.thirst >= 65 || self.weather() == YardWeather::Rain
            || self.village.as_ref().is_none_or(|state| state.household_rules.cast[fisher] != Some(CastResult::Empty))
        { return false; }
        let bank = self.actors[fisher].cell.tile();
        if self.legal_cat_shoulder(bank, 3).is_none() || cat.until > self.now_ms + 2 * MINUTE { return false; }
        self.village.as_mut().unwrap().family_routines.detail.empty_hook = Some(EmptyHook {
            fisher: fisher as u8, bank, ends: self.now_ms + 4 * MINUTE,
        });
        true
    }

    pub(super) fn begin_empty_hook_watch(&self, fisher: u8, ends: u64, at: TilePos) -> Option<u64> {
        let note = self.village.as_ref()?.family_routines.detail.empty_hook?;
        let actor = self.actors.get(usize::from(fisher))?;
        (note.fisher == fisher && note.ends == ends && actor.stage == Stage::Finish
            && actor.task == Some(Task::Fish) && actor.cell.tile() == note.bank
            && Cell::from(at).distance(actor.cell) == 1)
            .then(|| ends.saturating_sub(self.now_ms)).filter(|&left| left >= MINUTE)
    }

    pub fn cat_door_fur(&self) -> Option<TilePos> {
        let (at, day) = self.village.as_ref()?.family_routines.detail.cat_fur_at?;
        (day == self.now_ms / DAY_MS).then_some(at)
    }

    pub(super) fn cat_watch_harvest(&self) -> Option<Task> {
        let i = ActorKind::Orange.index();
        let cat = &self.actors[i];
        if cat.hunger >= 58 || cat.thirst >= 58 || !self.is_village()
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow) { return None; }
        let watched = self.village.as_ref()?.family_routines.detail.watched_harvest;
        let day = self.now_ms / DAY_MS;
        for (farmer, actor) in self.actors[..2].iter().enumerate() {
            let Some(Task::Harvest(plot)) = actor.task else { continue; };
            if actor.stage != Stage::Work || actor.until.saturating_sub(self.now_ms) < 3 * MINUTE
                || watched == Some((plot, day)) { continue; }
            let rect = self.available_plot_rect(usize::from(plot))?;
            let work = Self::plot_work_cell(usize::from(plot)).tile();
            let close_enough = if self.cat_apron_fondness(actor.kind) { 4 } else { 3 };
            if let Some(stand) = [(0,-1), (-1,0), (1,0), (0,1)].into_iter().filter_map(|(x,y)| work.offset(x,y))
                .filter(|&tile| !rect.contains(tile) && self.quiet_visit_site_available(i, tile))
                .map(|tile| (cat.cell.distance(Cell::from(tile)), tile))
                .filter(|(distance, _)| *distance <= close_enough).min().map(|(_, tile)| tile)
            {
                return Some(Task::Detail(Detail::WatchHarvest { stand, plot, farmer: farmer as u8, ends: actor.until }));
            }
        }
        None
    }

    pub(super) fn begin_watch_harvest(&self, farmer: u8, plot: u8, ends: u64) -> Option<u64> {
        let farmer = self.actors.get(usize::from(farmer))?;
        (farmer.task == Some(Task::Harvest(plot)) && farmer.stage == Stage::Work && farmer.until == ends
            && self.plots.get(usize::from(plot)).is_some_and(|plot| plot.stage == PlotStage::Mature)
            && !matches!(self.weather(), YardWeather::Rain | YardWeather::Snow))
            .then(|| ends.saturating_sub(self.now_ms)).filter(|&left| left >= MINUTE)
    }

    /// The latest inspection slot has one real affected tile; the second
    /// bit distinguishes drainage from updating the physical report.
    pub fn yard_flood_report(&self) -> Option<(TilePos, bool, bool)> {
        let notice = self.village.as_ref()?.family_routines.detail.repair_log?;
        let plot = self.available_plot_rect(usize::from(notice.plot))?;
        Some((TilePos::new(plot.x, plot.y), notice.fixed.is_some(), notice.filed))
    }

    pub(super) fn file_repair_task(&self, i: usize) -> Option<Task> {
        if i >= 2 || !(8..19).contains(&(self.minute_of_day() / 60))
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow) { return None; }
        let notice = self.village.as_ref()?.family_routines.detail.repair_log?;
        let actor = &self.actors[i];
        let table = village::poi("home_table");
        let task = Task::Detail(Detail::FileRepair { stand: table.tile(), plot: notice.plot });
        (notice.fixed.is_some() && !notice.filed && !self.village_plot_is_flooded(notice.plot)
            && actor.carried.is_none() && actor.tool.is_none() && actor.cell.distance(table) <= 14
            && actor.hunger < 60 && actor.thirst < 60 && actor.stamina >= 35
            && !self.task_in_use(task) && self.quiet_visit_site_available(i, table.tile())).then_some(task)
    }

    pub fn raincoat_on_home_hook(&self, kind: ActorKind) -> bool {
        let i = kind.index();
        i < 2 && self.village.as_ref().is_some_and(|state| state.family_routines.detail.raincoat_hook[i])
    }

    pub fn season_harvest_register(&self) -> Option<(u16, u16)> {
        let log = self.village.as_ref()?.family_routines.detail.harvest_log?;
        (log.season == self.now_ms / (28 * DAY_MS)).then_some((log.total, log.pending))
    }

    pub(super) fn note_processed_crop_storage(&mut self, i: usize, amount: u16) {
        if i >= 2 { return; }
        let Some(state) = &mut self.village else { return; };
        let detail = &mut state.family_routines.detail;
        if !std::mem::take(&mut detail.processed_harvest[i]) { return; }
        let season = self.now_ms / (28 * DAY_MS);
        let log = detail.harvest_log.filter(|log| log.season == season)
            .unwrap_or(HarvestLog { season, pending: 0, total: 0, last_written_day: None });
        detail.harvest_log = Some(HarvestLog { pending: log.pending.saturating_add(amount), ..log });
    }

    pub(super) fn harvest_register_task(&self, i: usize) -> Option<Task> {
        if i >= 2 || !(14..20).contains(&(self.minute_of_day() / 60))
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow) { return None; }
        let log = self.village.as_ref()?.family_routines.detail.harvest_log?;
        let actor = &self.actors[i];
        let table = village::poi("home_table");
        let task = Task::Detail(Detail::RegisterHarvest(table.tile()));
        (log.season == self.now_ms / (28 * DAY_MS) && log.pending != 0
            && log.last_written_day != Some(self.now_ms / DAY_MS)
            && actor.carried.is_none() && actor.tool.is_none() && actor.cell.distance(table) <= 14
            && actor.hunger < 60 && actor.thirst < 60 && actor.stamina >= 35
            && !self.task_in_use(task) && self.quiet_visit_site_available(i, table.tile())
            && self.actors[..2].iter().all(|actor| actor.carried != Some(Item::Crops)
                && !matches!(actor.task, Some(Task::Harvest(_) | Task::CareOrchard(_) | Task::ProcessHarvest(_)))))
            .then_some(task)
    }

    pub(super) fn register_change_of_clothes(&mut self, i: usize, before: Outfit, after: Outfit) {
        if i >= 2 || before == after { return; }
        let Some(state) = &mut self.village else { return; };
        let detail = &mut state.family_routines.detail;
        detail.garden_clothes_ready[i] = after == Outfit::Garden;
        if after == Outfit::Raincoat {
            detail.raincoat_hook[i] = false;
            detail.rain_shift_done[i] = false;
        } else if before == Outfit::Raincoat && std::mem::take(&mut detail.rain_shift_done[i]) {
            detail.raincoat_hook[i] = true;
            self.record_household_completion("H01-54", i, self.home_door(i).tile(), LegacyPlace::Front);
        }
    }

    pub(super) fn note_garden_clothes_work(&mut self, i: usize, target: TilePos) {
        if i < 2 && self.actors[i].outfit == Outfit::Garden
            && let Some(state) = &mut self.village
            && std::mem::take(&mut state.family_routines.detail.garden_clothes_ready[i])
        {
            self.record_household_completion("H01-49", i, target, LegacyPlace::Front);
        }
    }

    pub(crate) fn village_cat_supper_site(&self) -> Option<TilePos> {
        self.village.as_ref().filter(|state| !state.opening.cat_food
            && !self.task_in_use(Task::ServeCat) && !self.task_in_use(Task::ServeCatFish))
            .map(|_| village::poi("cat_water").tile())
    }

    pub(crate) fn village_receive_supper_for_cat(&mut self, at: TilePos) -> bool {
        if self.village_cat_supper_site() != Some(at) || !(17..21).contains(&(self.minute_of_day() / 60))
        {
            return false;
        }
        let Some(state) = &mut self.village else { return false; };
        if state.opening.cat_food { return false; }
        state.opening.cat_food = true;
        state.family_routines.cat_meal_sniffed = false;
        true
    }

    pub fn cat_fish_served(&self) -> bool {
        self.village.as_ref().is_some_and(|state| state.family_routines.detail.cat_fish_plate)
    }

    pub(super) fn cat_fish_due(&self, i: usize) -> bool {
        i < 2 && self.has_carry(i, Item::Fish, 1)
            && self.stock.count(Item::Food) >= 5 && self.actors[i].hunger < 65 && self.actors[i].thirst < 65
            && self.actors[ActorKind::Orange.index()].hunger >= 48
            && self.village.as_ref().is_some_and(|state| {
                state.soil.completed("Q21") && state.opening.cat_ate && !state.opening.cat_food
                    && state.household_rules.cast[i] == Some(CastResult::FishInHand)
                    && state.family_routines.detail.cat_fish_day.is_none_or(|day| day + 7 <= self.now_ms / DAY_MS)
            })
            && !(21..24).contains(&(self.minute_of_day() / 60))
            && !self.task_in_use(Task::ServeCatFish) && !self.task_in_use(Task::ServeCat)
    }

    pub(super) fn commit_cat_fish(&mut self, i: usize) {
        if i >= 2 || !self.has_carry(i, Item::Fish, 1) || !self.village.as_ref().is_some_and(|state| {
            !state.opening.cat_food && state.household_rules.cast[i] == Some(CastResult::FishInHand)
        }) { return; }
        self.drop_carry(i);
        let state = self.village.as_mut().unwrap();
        state.household_rules.cast[i] = None;
        state.opening.cat_food = true;
        state.family_routines.cat_meal_sniffed = false;
        state.family_routines.detail.cat_fish_plate = true;
        state.family_routines.detail.cat_fish_day = Some(self.now_ms / DAY_MS);
        self.record_household_completion("H01-30", i, village::poi("home_fishing").tile(), LegacyPlace::Front);
    }

    pub fn household_lamp_lit(&self) -> bool {
        static PHASE: std::sync::OnceLock<Option<crate::map_data::PhaseId>> = std::sync::OnceLock::new();
        let phase = PHASE.get_or_init(|| WILLOW_VILLAGE.scenic.iter().find(|object| {
            object.tile == CAT_LAMP && object.kind == crate::map_data::ScenicKind::Lamp
        }).map(|object| object.phase));
        self.is_village() && !(6..18).contains(&(self.minute_of_day() / 60))
            && phase.is_some_and(|phase| self.phase_mask().contains(phase))
    }

    pub fn cat_coat_wet(&self) -> bool {
        self.village.as_ref().is_some_and(|state| {
            state.family_routines.detail.cat_wet_at.is_some_and(|at| self.now_ms.saturating_sub(at) <= CAT_WET_WINDOW)
        })
    }

    /// Elapsed world time is supplied only during an actual detail action.
    pub fn household_detail_progress(&self, kind: ActorKind) -> Option<(u64, u64)> {
        let actor = &self.actors[kind.index()];
        (matches!(actor.task, Some(Task::Detail(_))) && actor.stage == Stage::Work)
            .then(|| (self.now_ms.saturating_sub(actor.since), actor.until.saturating_sub(actor.since)))
    }

    pub(super) fn observe_cat_weather_edge(&mut self, kind: ActorKind, from: Cell, to: Cell) {
        if kind != ActorKind::Orange || self.weather() != YardWeather::Rain
            || from == to || to == self.home_door(0)
        {
            return;
        }
        if let Some(state) = &mut self.village {
            state.family_routines.detail.cat_wet_at = Some(self.now_ms);
        }
    }

    fn current_bright_spell(&self) -> Option<u64> {
        let spell = self.now_ms / WEATHER_BLOCK;
        (spell > 0 && self.now_ms % WEATHER_BLOCK < 9 * MINUTE
            && (6 * HOUR..18 * HOUR).contains(&(self.now_ms % DAY_MS))
            && Self::weather_at((spell - 1) * WEATHER_BLOCK) == YardWeather::Clouds
            && self.weather() == YardWeather::Clear).then_some(spell)
    }

    pub(super) fn notice_light(&self, i: usize) -> Option<Task> {
        let actor = &self.actors[i];
        let state = self.village.as_ref()?;
        let spell = self.current_bright_spell()?;
        (!actor.inside && !actor.outside && actor.carried.is_none() && actor.tool.is_none()
            && actor.hunger < 60 && actor.thirst < 60
            && !matches!(self.ground(actor.cell.0.into(), actor.cell.1.into()), Ground::Building | Ground::Water)
            && state.family_routines.detail.bright_seen[i] != Some(spell))
            .then_some(Task::Detail(Detail::NoticeLight { stand: actor.cell.tile(), spell }))
    }

    pub(super) fn cat_detail(&self) -> Option<Task> {
        let i = ActorKind::Orange.index();
        let cat = &self.actors[i];
        let detail = &self.village.as_ref()?.family_routines.detail;
        let dish = village::poi("cat_water");
        let door = self.home_door(0);
        if cat.hunger >= 68 || cat.thirst >= 65 { return None; }
        if let Some(route) = detail.rain_patrol.filter(|route| route.arrived && !route.resumed
            && route.expires >= self.now_ms && cat.cell.tile() == route.shelter)
        {
            return Some(if self.weather() == YardWeather::Rain {
                Task::Detail(Detail::WaitUnderEave(route.shelter))
            } else { Task::Explore(route.spot) });
        }
        if detail.cat_water_at.is_some_and(|at| self.now_ms.saturating_sub(at) <= 6 * MINUTE)
            && cat.cell == dish
        {
            return Some(Task::Detail(Detail::Groom(dish.tile())));
        }
        if detail.cat_awoke_at.is_some_and(|at| self.now_ms.saturating_sub(at) <= 8 * MINUTE)
            && cat.cell == door && self.weather() != YardWeather::Rain
        {
            return Some(Task::Detail(Detail::Stretch(door.tile())));
        }
        if self.weather() == YardWeather::Rain && self.cat_coat_wet()
            && detail.cat_shelter_spell != Some(self.now_ms / WEATHER_BLOCK)
            && cat.cell != door && cat.cell.distance(door) <= 12
            && self.quiet_visit_site_available(i, door.tile())
        {
            return Some(Task::Detail(Detail::Shelter(door.tile())));
        }
        if matches!(self.weather(), YardWeather::Clear | YardWeather::Clouds) && self.cat_coat_wet()
            && self.household_lamp_lit()
        {
            if let Some(stand) = [(0, -1), (1, 0), (0, 1), (-1, 0)].into_iter()
                .filter_map(|(dx, dy)| CAT_LAMP.offset(dx, dy))
                .filter(|p| self.quiet_visit_site_available(i, *p))
                .map(|p| (cat.cell.distance(Cell::from(p)), p)).filter(|(distance, _)| *distance <= 12)
                .min().map(|(_, tile)| tile)
            {
                return Some(Task::Detail(Detail::DryCoat { stand, lamp: CAT_LAMP }));
            }
        }
        if detail.full_patrol == 0b111111 && self.cat_door_fur().is_none()
            && matches!(self.weather(), YardWeather::Clear | YardWeather::Clouds)
            && cat.cell.distance(door) <= 9 && self.quiet_visit_site_available(i, door.tile())
        {
            return Some(Task::Detail(Detail::RubDoor(door.tile())));
        }
        if let Some(task) = self.cat_watch_harvest() { return Some(task); }
        if let Some(task) = self.startled_task(i) { return Some(task); }
        if let Some(mark) = detail.cat_yield.filter(|mark| mark.aside && mark.expires >= self.now_ms)
            && cat.cell.tile() == mark.shoulder
        {
            if mark.passed && self.quiet_visit_site_available(i, mark.origin) {
                return Some(Task::Detail(Detail::ReturnFromYield(mark.origin)));
            }
            return Some(Task::Detail(Detail::WaitAtRoadside(mark.shoulder)));
        }
        if let Some(note) = detail.empty_hook.filter(|note| note.ends > self.now_ms + 2 * MINUTE)
            && let Some(stand) = self.legal_cat_shoulder(note.bank, 3)
        {
            return Some(Task::Detail(Detail::WatchEmptyHook { stand, fisher: note.fisher, ends: note.ends }));
        }
        if let Some(task) = self.cat_apron_greeting_task() { return Some(task); }
        if let Some(task) = self.cat_bloom_task() { return Some(task); }
        self.notice_light(i)
    }

    pub(super) fn commit_household_detail(&mut self, i: usize, action: Detail) {
        if !action.actor_accepts(i) || self.actors[i].cell.tile() != action.stand() { return; }
        if let Detail::Contact(work) = action {
            self.commit_family_contact(i, work);
            return;
        }
        let spell = self.now_ms / WEATHER_BLOCK;
        let weather = self.weather();
        let door = self.home_door(0).tile();
        let coat_wet = self.cat_coat_wet();
        let lamp_lit = self.household_lamp_lit();
        let light = self.current_bright_spell();
        let running = self.story.running;
        let fresh_bloom = match action {
            Detail::SmellBloom { tree, opened, .. } => self.first_bloom_today(tree) == Some(opened),
            _ => false,
        };
        let apron_home = match action {
            Detail::DressApron(at) | Detail::HangApron(at) => i < 2 && at == self.home_door(i).tile(),
            _ => false,
        };
        let repair_target = if let Detail::FileRepair { plot, .. } = action {
            self.available_plot_rect(usize::from(plot)).filter(|_| !self.village_plot_is_flooded(plot))
                .map(|rect| TilePos::new(rect.x, rect.y))
        } else { None };
        let harvested = if let Detail::WatchHarvest { stand, plot, farmer, ends } = action {
            let observation = self.actors.get(usize::from(farmer)).is_some_and(|actor| {
                actor.task == Some(Task::Harvest(plot)) && actor.stage == Stage::Finish
                    && actor.carried == Some(Item::Crops) && self.now_ms == ends
            });
            observation.then(|| self.available_plot_rect(usize::from(plot)))
                .flatten().filter(|rect| !rect.contains(stand)
                    && Cell::from(stand).distance(Self::plot_work_cell(usize::from(plot))) == 1)
                .map(|rect| TilePos::new(rect.x, rect.y))
        } else { None };
        let Some(state) = &mut self.village else { return; };
        let detail = &mut state.family_routines.detail;
        let (base, target) = match action {
            Detail::Groom(at) if at == village::poi("cat_water").tile()
                && detail.cat_water_at.is_some_and(|at| self.now_ms.saturating_sub(at) <= 8 * MINUTE) => {
                detail.cat_water_at = None;
                ("A01-04", at)
            }
            Detail::Stretch(at) if at == door && detail.cat_awoke_at.is_some_and(|at| self.now_ms.saturating_sub(at) <= 12 * MINUTE) => {
                detail.cat_awoke_at = None;
                ("A01-09", at)
            }
            Detail::Shelter(at) if at == door && weather == YardWeather::Rain && coat_wet
                && detail.cat_shelter_spell != Some(spell) => {
                detail.cat_shelter_spell = Some(spell);
                if let Some(route) = &mut detail.rain_patrol && route.shelter == at && route.expires >= self.now_ms {
                    route.arrived = true;
                }
                ("A01-50", at)
            }
            Detail::DryCoat { lamp, .. } if coat_wet && matches!(weather, YardWeather::Clear | YardWeather::Clouds)
                && lamp == CAT_LAMP && lamp_lit => {
                detail.cat_wet_at = None;
                ("A01-51", lamp)
            }
            Detail::NoticeLight { stand, spell } if light == Some(spell) && detail.bright_seen[i] != Some(spell) => {
                detail.bright_seen[i] = Some(spell);
                ("S12", stand)
            }
            Detail::RegisterHarvest(at) if at == village::poi("home_table").tile()
                && matches!(weather, YardWeather::Clear | YardWeather::Clouds)
                && let Some(log) = &mut detail.harvest_log && log.season == self.now_ms / (28 * DAY_MS)
                && log.pending != 0 && log.last_written_day != Some(self.now_ms / DAY_MS) => {
                log.total = log.total.saturating_add(std::mem::take(&mut log.pending));
                log.last_written_day = Some(self.now_ms / DAY_MS);
                ("H01-57", at)
            }
            Detail::FileRepair { stand, plot } if stand == village::poi("home_table").tile()
                && matches!(weather, YardWeather::Clear | YardWeather::Clouds)
                && let Some(target) = repair_target && let Some(notice) = &mut detail.repair_log
                && notice.plot == plot && notice.fixed.is_some() && !notice.filed => {
                notice.filed = true;
                ("H01-58", target)
            }
            Detail::WatchHarvest { plot, .. } if matches!(weather, YardWeather::Clear | YardWeather::Clouds)
                && let Some(target) = harvested => {
                detail.watched_harvest = Some((plot, self.now_ms / DAY_MS));
                ("A01-31", target)
            }
            Detail::RubDoor(at) if at == door && detail.full_patrol == 0b111111
                && matches!(weather, YardWeather::Clear | YardWeather::Clouds) => {
                detail.full_patrol = 0;
                detail.cat_fur_at = Some((at, self.now_ms / DAY_MS));
                ("A01-15", at)
            }
            Detail::WatchEmptyHook { stand, fisher, ends } if self.now_ms == ends
                && let Some(wait) = detail.empty_hook && wait.fisher == fisher && wait.ends == ends
                && self.actors[usize::from(fisher)].cell.tile() == wait.bank
                && Cell::from(stand).distance(Cell::from(wait.bank)) == 1
                && state.household_rules.cast[usize::from(fisher)] == Some(CastResult::Empty) => {
                detail.empty_hook = None;
                ("A01-12", wait.bank)
            }
            Detail::ReturnFromYield(at) if let Some(mark) = detail.cat_yield
                && mark.expires >= self.now_ms && mark.aside && mark.passed && mark.origin == at => {
                detail.cat_yield = None;
                ("A01-37", at)
            }
            Detail::PostShift { stand, project, next } if stand == village::poi("market_notice").tile()
                && matches!(weather, YardWeather::Clear | YardWeather::Clouds)
                && next > self.now_ms && running.is_some_and(|job| job.id == project && job.next_shift == next
                    && job.shifts > 0 && job.shifts < CHAPTERS[usize::from(project)].work_shifts)
                && detail.posted_shift != Some((project, next)) => {
                detail.posted_shift = Some((project, next));
                ("H01-56", stand)
            }
            Detail::Startled { stand, origin, heard } if let Some(impact) = &mut detail.last_impact
                && impact.location == origin && impact.at == heard && usize::from(impact.source) != i
                && impact.responded & (1 << i) == 0 && self.now_ms.saturating_sub(heard) < 8 * MINUTE
                && Cell::from(stand).distance(Cell::from(origin)) <= 7 => {
                impact.responded |= 1 << i;
                ("S22", origin)
            }
            Detail::SmellBloom { tree, opened, .. } if fresh_bloom
                && detail.smelled_bloom.is_none_or(|(_, prior)| prior != opened)
                && matches!(weather, YardWeather::Clear | YardWeather::Clouds) => {
                detail.smelled_bloom = Some((tree, opened));
                ("A01-25", village::tree_cells()[usize::from(tree)].tile())
            }
            Detail::DressApron(_) if apron_home && detail.apron.is_none() => {
                detail.apron = Some(Apron { owner: i as u8, dressed: self.now_ms, used: false });
                detail.last_apron_day = Some(self.now_ms / DAY_MS);
                return;
            }
            Detail::HangApron(_) if apron_home && detail.apron.is_some_and(|apron| usize::from(apron.owner) == i) => {
                detail.apron = None;
                return;
            }
            _ => return,
        };
        let parcel = if matches!(action, Detail::PostShift { .. } | Detail::SmellBloom { .. }) {
            LegacyPlace::Expanded
        } else { LegacyPlace::Front };
        self.record_household_completion(base, i, target, parcel);
    }
}
