//! Local routines that keep one concrete plant, person, or window as their subject.
use super::*;

const SAPLING: usize = 11;
const ORCHARD: std::ops::Range<usize> = 8..12;
const TAP_WINDOW: u64 = 4 * MINUTE;
pub(super) const APRON_GREETING_TIME: u64 = 14 * MINUTE;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Work {
    PackSapling(TilePos),
    SeeApple { stand: TilePos, tree: u8, day: u64 },
    WriteApple { stand: TilePos, tree: u8, day: u64 },
    RubApron { stand: TilePos, door: TilePos, owner: u8, expires: u64 },
    HearTap { stand: TilePos, window: TilePos, heard: u64, route: Option<u8>, prior_rest: Option<u64> },
}

impl Work {
    pub(super) fn stand(self) -> TilePos {
        match self {
            Self::PackSapling(stand) | Self::SeeApple { stand, .. } | Self::WriteApple { stand, .. }
            | Self::RubApron { stand, .. } | Self::HearTap { stand, .. } => stand,
        }
    }
    pub(super) fn valid(self) -> bool {
        match self {
            Self::PackSapling(stand) => Cell::from(stand).distance(village::tree_cells()[SAPLING]) == 1,
            Self::SeeApple { stand, tree, .. } => ORCHARD.contains(&usize::from(tree))
                && Cell::from(stand).distance(village::tree_cells()[usize::from(tree)]) == 1,
            Self::WriteApple { stand, tree, .. } => ORCHARD.contains(&usize::from(tree))
                && stand == village::poi("home_table").tile(),
            Self::RubApron { stand, door, owner, .. } => owner < 2 && door.index().is_some()
                && Cell::from(stand).distance(Cell::from(door)) == 1,
            Self::HearTap { stand, window, route, prior_rest, .. } => window.index().is_some()
                && Cell::from(stand).distance(Cell::from(window)) <= 3
                && route.is_none_or(|spot| usize::from(spot) < village::VILLAGE_CAT_SPOTS)
                && prior_rest.is_none_or(|left| route.is_none() && left <= Task::Rest.duration()),
        }
    }
    pub(super) fn actor_accepts(self, i: usize) -> bool {
        match self {
            Self::PackSapling(_) | Self::SeeApple { .. } | Self::WriteApple { .. } => i == 0,
            Self::RubApron { .. } | Self::HearTap { .. } => i == ActorKind::Orange.index(),
        }
    }
    pub(super) fn minutes(self) -> u64 {
        match self {
            Self::PackSapling(_) => 3,
            Self::SeeApple { .. } | Self::WriteApple { .. } | Self::HearTap { .. } => 2,
            Self::RubApron { .. } => 1,
        }
    }
    pub(super) fn activity(self) -> Activity {
        match self {
            Self::PackSapling(_) => Activity::PackSaplingSoil,
            Self::SeeApple { .. } => Activity::ObserveNewAppleFlower,
            Self::WriteApple { .. } => Activity::WriteFirstFlower,
            Self::RubApron { .. } => Activity::GreetApron,
            Self::HearTap { .. } => Activity::HearWindowTap,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Greeting { owner: u8, door: TilePos, expires: u64 }

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    packed_day: Option<u64>,
    seen_apple: Option<(u8, u64)>,
    apple_note: Option<(u8, u64)>,
    greeting: Option<Greeting>,
    fond_until: Option<(u8, u64)>,
    tap: Option<(TilePos, u64)>,
}

impl State {
    pub(super) fn is_empty(&self) -> bool { *self == Self::default() }
    pub(super) fn valid(&self, now: u64) -> bool {
        let valid_bloom = |(tree, day)| ORCHARD.contains(&tree) && day <= now / DAY_MS && day <= 15;
        self.packed_day.is_none_or(|day| day <= now / DAY_MS)
            && self.seen_apple.is_none_or(|(tree, day)| valid_bloom((usize::from(tree), day)))
            && self.apple_note.is_none_or(|(tree, day)| valid_bloom((usize::from(tree), day)))
            && (self.apple_note.is_none() || self.seen_apple.is_none())
            && self.greeting.is_none_or(|note| note.owner < 2 && note.door.index().is_some()
                && note.expires <= now.saturating_add(APRON_GREETING_TIME))
            && self.fond_until.is_none_or(|(owner, until)| owner < 2 && until <= now.saturating_add(2 * HOUR))
            && self.tap.is_none_or(|(window, at)| at <= now && window.index().is_some())
    }
}

fn task(work: Work) -> Task { Task::Detail(Detail::Contact(work)) }

impl LivingWorld {
    fn contact_state(&self) -> Option<&State> {
        Some(&self.village.as_ref()?.family_routines.detail.contacts)
    }
    fn contact_state_mut(&mut self) -> Option<&mut State> {
        Some(&mut self.village.as_mut()?.family_routines.detail.contacts)
    }

    fn tree_work_foot(&self, actor: usize, tree: usize) -> Option<TilePos> {
        let root = village::tree_cells()[tree];
        let from = self.actors[actor].cell;
        [(0, -1), (-1, 0), (0, 1), (1, 0)].into_iter()
            .filter_map(|(x, y)| root.tile().offset(x, y))
            .filter(|p| self.quiet_visit_site_available(actor, *p))
            .min_by_key(|p| from.distance(Cell::from(*p)))
    }

    pub fn sapling_soil_ring(&self) -> Option<TilePos> {
        let day = self.contact_state()?.packed_day?;
        (self.orchard_phase(SAPLING) == Some(OrchardPhase::Sapling)
            && self.now_ms / DAY_MS <= day + 12)
            .then(|| village::tree_cells()[SAPLING].tile())
    }

    pub fn first_apple_bloom_note(&self) -> Option<(u8, u64)> {
        self.contact_state()?.apple_note
    }

    fn opening_apple_bloom(&self) -> Option<(u8, u64)> {
        let today = self.now_ms / DAY_MS;
        let first = self.village.as_ref()?.orchard.iter().enumerate().filter_map(|(index, tree)| {
            Some((tree.flower_opened_day?, (index + 8) as u8))
        }).min()?;
        (first.0 == today && today <= 15).then_some((first.1, today))
    }

    pub(super) fn household_contact_task(&self, i: usize) -> Option<Task> {
        let state = self.contact_state()?;
        let actor = &self.actors[i];
        if i != ActorKind::Xiaohe.index() || actor.hunger >= 59 || actor.thirst >= 59 || actor.stamina < 42
            || actor.carried.is_some() || actor.moving_box || self.stock.count(Item::Food) < 5
            || !(7..17).contains(&(self.minute_of_day() / 60))
            || !matches!(self.weather(), YardWeather::Clear | YardWeather::Clouds)
        { return None; }
        if let Some((tree, day)) = state.seen_apple.filter(|(_, day)| self.now_ms / DAY_MS <= *day + 1) {
            let table = village::poi("home_table").tile();
            if self.quiet_visit_site_available(i, table) {
                return Some(if actor.tool.is_some() { Task::ReturnTool }
                    else { task(Work::WriteApple { stand: table, tree, day }) });
            }
        }
        if state.apple_note.is_none() && state.seen_apple.is_none()
            && let Some((tree, day)) = self.opening_apple_bloom()
            && let Some(stand) = self.tree_work_foot(i, usize::from(tree))
        {
            return Some(if actor.tool.is_some() { Task::ReturnTool } else { task(Work::SeeApple { stand, tree, day }) });
        }
        let sapling = &self.village.as_ref()?.orchard[SAPLING - 8];
        if state.packed_day.is_none() && sapling.phase == OrchardPhase::Sapling && sapling.age_days < 19
            && let Some(stand) = self.tree_work_foot(i, SAPLING)
        {
            return Some(if actor.tool.is_some() { Task::ReturnTool } else { task(Work::PackSapling(stand)) });
        }
        None
    }

    pub(super) fn offer_apron_greeting(&mut self, i: usize) -> bool {
        if i >= 2 || !self.household_wears_apron(self.actors[i].kind) { return false; }
        let cat = &self.actors[ActorKind::Orange.index()];
        let door = self.home_door(i);
        if self.actors[i].cell != door || cat.inside || cat.outside || cat.hunger >= 65 || cat.thirst >= 62
            || self.actors[i].hunger >= 57 || self.actors[i].thirst >= 62
            || cat.pose == Pose::Sleep || cat.cell.distance(door) > 14
            || !self.in_household_region(cat.cell)
            || !matches!(cat.task, None | Some(Task::Explore(_) | Task::Rest))
        { return false; }
        let expires = self.now_ms + APRON_GREETING_TIME;
        self.contact_state_mut().unwrap().greeting = Some(Greeting { owner: i as u8, door: door.tile(), expires });
        true
    }

    pub(super) fn cat_apron_greeting_task(&self) -> Option<Task> {
        let note = self.contact_state()?.greeting?;
        let i = ActorKind::Orange.index();
        let cat = &self.actors[i];
        let owner = &self.actors[usize::from(note.owner)];
        if self.now_ms + MINUTE >= note.expires || owner.inside || owner.cell.tile() != note.door
            || !self.household_wears_apron(owner.kind) || cat.hunger >= 65 || cat.thirst >= 62
        { return None; }
        let from = cat.cell;
        let stand = [(0, 1), (-1, 0), (1, 0)].into_iter().filter_map(|(x, y)| note.door.offset(x, y))
            .filter(|p| self.quiet_visit_site_available(i, *p) && from.distance(Cell::from(*p)) <= 14)
            .min_by_key(|p| from.distance(Cell::from(*p)))?;
        Some(task(Work::RubApron { stand, door: note.door, owner: note.owner, expires: note.expires }))
    }

    pub fn cat_apron_fondness(&self, who: ActorKind) -> bool {
        self.contact_state().and_then(|s| s.fond_until).is_some_and(|(owner, until)| who as u8 == owner && self.now_ms < until)
    }

    pub fn cat_apron_greeting_visual(&self) -> Option<(TilePos, u64)> {
        let cat = &self.actors[ActorKind::Orange.index()];
        let Task::Detail(Detail::Contact(Work::RubApron { door, .. })) = cat.task? else { return None; };
        (cat.stage == Stage::Work).then_some((door, self.now_ms.saturating_sub(cat.since)))
    }

    fn window_tap_anchor(&self) -> Option<TilePos> {
        // The window is in the front wall, one tile to the right of the owned entrance.
        self.home_door(0).tile().offset(1, -1)
    }

    /// Accepts a genuine foreground action. This only queues the two-beat response.
    pub fn request_cat_window_tap(&mut self) -> bool {
        let Some(window) = self.window_tap_anchor() else { return false; };
        let Some(state) = self.contact_state() else { return false; };
        let i = ActorKind::Orange.index();
        let cat = &self.actors[i];
        if cat.inside || cat.outside || cat.pose == Pose::Sleep || cat.hunger >= 68 || cat.thirst >= 65
            || Cell::from(window).distance(cat.cell) > 3
            || state.tap.is_some_and(|(_, heard)| self.now_ms.saturating_sub(heard) < TAP_WINDOW)
            || !matches!(cat.task, None | Some(Task::Explore(_) | Task::Rest))
        { return false; }
        let heard = self.now_ms;
        let original = cat.task;
        let moving = cat.stage == Stage::WalkToSite;
        self.contact_state_mut().unwrap().tap = Some((window, heard));
        if !moving && let Some(work) = self.cat_window_tap_task(original) {
            self.begin(i, work);
        }
        true
    }

    pub(super) fn cat_window_tap_task(&self, original: Option<Task>) -> Option<Task> {
        let (window, heard) = self.contact_state()?.tap?;
        let cat = &self.actors[ActorKind::Orange.index()];
        let (route, prior_rest) = match original {
            Some(Task::Explore(index)) => (Some(index), None),
            Some(Task::Rest) => (None, Some(if cat.stage == Stage::Work { cat.until.saturating_sub(self.now_ms) } else { 0 })),
            None => (None, None),
            _ => return None,
        };
        if self.now_ms.saturating_sub(heard) >= 2 * MINUTE || Some(window) != self.window_tap_anchor()
            || cat.pose == Pose::Sleep || cat.inside || cat.outside || cat.hunger >= 68 || cat.thirst >= 65
            || cat.cell.distance(Cell::from(window)) > 3
        { return None; }
        Some(task(Work::HearTap { stand: cat.cell.tile(), window, heard, route, prior_rest }))
    }

    pub(super) fn resume_after_window_tap(&mut self, i: usize, work: Work) -> bool {
        match work {
            Work::HearTap { route: Some(spot), .. } => self.begin(i, Task::Explore(spot)),
            Work::HearTap { prior_rest: Some(0), .. } => self.begin(i, Task::Rest),
            Work::HearTap { prior_rest: Some(remaining), .. } => {
                self.actors[i].task = Some(Task::Rest);
                self.actors[i].activity = Activity::Rest;
                self.stage(i, Stage::Work, Pose::Rest, remaining);
                true
            }
            _ => false,
        }
    }

    pub fn cat_window_tap_visual(&self) -> Option<(TilePos, u8)> {
        let cat = &self.actors[ActorKind::Orange.index()];
        let Task::Detail(Detail::Contact(Work::HearTap { window, .. })) = cat.task? else { return None; };
        (cat.stage == Stage::Work).then_some((window, u8::from(self.now_ms.saturating_sub(cat.since) >= MINUTE)))
    }

    pub(super) fn commit_family_contact(&mut self, i: usize, work: Work) {
        if !work.valid() || !work.actor_accepts(i) || self.actors[i].cell.tile() != work.stand() { return; }
        let Some(state) = self.contact_state().copied() else { return; };
        let dry = matches!(self.weather(), YardWeather::Clear | YardWeather::Clouds);
        let today = self.now_ms / DAY_MS;
        let (id, at, parcel) = match work {
            Work::PackSapling(_) if dry && state.packed_day.is_none()
                && self.orchard_phase(SAPLING) == Some(OrchardPhase::Sapling) => {
                self.contact_state_mut().unwrap().packed_day = Some(today);
                ("H01-07", village::tree_cells()[SAPLING].tile(), LegacyPlace::Expanded)
            }
            Work::SeeApple { tree, day, .. } if dry && state.seen_apple.is_none() && state.apple_note.is_none()
                && self.opening_apple_bloom() == Some((tree, day)) => {
                self.contact_state_mut().unwrap().seen_apple = Some((tree, day));
                return;
            }
            Work::WriteApple { tree, day, .. } if dry && state.seen_apple == Some((tree, day))
                && state.apple_note.is_none() && today <= day + 1 => {
                let next = self.contact_state_mut().unwrap();
                next.seen_apple = None;
                next.apple_note = Some((tree, day));
                ("H01-55", village::tree_cells()[usize::from(tree)].tile(), LegacyPlace::Front)
            }
            Work::RubApron { door, owner, expires, .. } if state.greeting == Some(Greeting { owner, door, expires })
                && self.now_ms <= expires && self.actors[usize::from(owner)].cell.tile() == door
                && !self.actors[usize::from(owner)].inside && self.household_wears_apron(self.actors[usize::from(owner)].kind) => {
                let until = self.now_ms + 2 * HOUR;
                let next = self.contact_state_mut().unwrap();
                next.greeting = None;
                next.fond_until = Some((owner, until));
                let wearer = &mut self.actors[usize::from(owner)];
                if wearer.stage == Stage::Finish && matches!(wearer.task, Some(Task::Detail(Detail::DressApron(_)))) {
                    wearer.until = wearer.until.min(self.now_ms + 20_000);
                }
                self.record_legacy_completion("A01-38", LegacyActor::Household(i as u8), work.stand(), door,
                    LegacyPlace::Front, 1, Some(LegacyActor::Household(owner)));
                return;
            }
            Work::HearTap { window, heard, .. } if state.tap == Some((window, heard))
                && self.now_ms.saturating_sub(heard) <= TAP_WINDOW && self.window_tap_anchor() == Some(window) => {
                self.contact_state_mut().unwrap().tap = None;
                ("A01-55", window, LegacyPlace::Front)
            }
            _ => return,
        };
        self.record_household_completion(id, i, at, parcel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(world: &mut LivingWorld, who: usize, foot: TilePos) {
        let worker = &mut world.actors[who];
        worker.cell = Cell::from(foot);
        worker.position = worker.cell.point();
        worker.inside = false;
        worker.hunger = 20;
        worker.thirst = 20;
        worker.stamina = 85;
    }

    #[test]
    fn packed_ring_requires_physical_sapling_and_stops_when_it_grows() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let before = world.stock.clone();
        world.now_ms = (4..12).map(|day| day * DAY_MS + 10 * HOUR)
            .find(|&at| matches!(LivingWorld::weather_at(at), YardWeather::Clear | YardWeather::Clouds)).unwrap();
        let foot = world.tree_work_foot(0, SAPLING).unwrap();
        world.commit(0, task(Work::PackSapling(foot)));
        assert!(world.sapling_soil_ring().is_none());
        put(&mut world, 0, foot);
        world.commit(0, task(Work::PackSapling(foot)));
        assert_eq!(world.sapling_soil_ring(), Some(village::tree_cells()[SAPLING].tile()));
        assert_eq!(world.stock, before);
        let mut world: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert!(world.sapling_soil_ring().is_some());
        let orchard = &mut world.village.as_mut().unwrap().orchard[SAPLING - 8];
        orchard.phase = OrchardPhase::Bud;
        assert!(world.sapling_soil_ring().is_none());
    }

    #[test]
    fn flower_notebook_requires_the_same_new_apple_seen_before_writing() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.now_ms = (9 * 24 + 10) * HOUR;
        assert!(matches!(world.weather(), YardWeather::Clear | YardWeather::Clouds));
        world.village.as_mut().unwrap().advance_orchard(8);
        world.village.as_mut().unwrap().advance_orchard(9);
        let (tree, day) = world.opening_apple_bloom().unwrap();
        let table = village::poi("home_table").tile();
        let write = task(Work::WriteApple { stand: table, tree, day });
        put(&mut world, 0, table);
        world.commit(0, write);
        assert!(world.first_apple_bloom_note().is_none());
        let foot = world.tree_work_foot(0, usize::from(tree)).unwrap();
        put(&mut world, 0, foot);
        world.commit(0, task(Work::SeeApple { stand: foot, tree, day }));
        assert!(world.first_apple_bloom_note().is_none());
        let mut world: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        put(&mut world, 0, table);
        world.commit(0, write);
        assert_eq!(world.first_apple_bloom_note(), Some((tree, day)));
        assert_eq!(world.orchard_phase(usize::from(tree)), Some(OrchardPhase::Flower));
    }

    #[test]
    fn apron_greeting_matches_the_same_quiet_door_and_affinity_expires() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let cat = ActorKind::Orange.index();
        let door = world.home_door(0).tile();
        let stand = door.offset(0, 1).unwrap();
        put(&mut world, 0, door);
        put(&mut world, cat, stand);
        assert!(!world.offer_apron_greeting(0));
        world.commit(0, Task::Detail(Detail::DressApron(door)));
        assert!(world.offer_apron_greeting(0));
        let greet = world.cat_apron_greeting_task().unwrap();
        put(&mut world, cat, stand.offset(0, 1).unwrap());
        world.commit(cat, greet);
        assert!(!world.cat_apron_fondness(ActorKind::Xiaohe));
        put(&mut world, cat, stand);
        world.commit(cat, greet);
        assert!(world.cat_apron_fondness(ActorKind::Xiaohe));
        assert!(!world.cat_apron_fondness(ActorKind::Ayu));
        assert!(world.cat_apron_greeting_task().is_none());
        world.now_ms += 2 * HOUR;
        assert!(!world.cat_apron_fondness(ActorKind::Xiaohe));
    }

    #[test]
    fn real_tap_near_the_window_is_two_beats_and_resumes_the_same_patrol() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let cat = ActorKind::Orange.index();
        let door = world.home_door(0).tile();
        put(&mut world, cat, door);
        world.actors[cat].pose = Pose::Sleep;
        assert!(!world.request_cat_window_tap());
        world.actors[cat].pose = Pose::Idle;
        world.actors[cat].task = Some(Task::Explore(1));
        assert!(world.request_cat_window_tap());
        assert!(!world.request_cat_window_tap());
        let Some(Task::Detail(Detail::Contact(work))) = world.actors[cat].task else { panic!("missing tap action") };
        assert!(matches!(work, Work::HearTap { route: Some(1), .. }));
        world.stage(cat, Stage::Work, Pose::Work, 2 * MINUTE);
        assert_eq!(world.cat_window_tap_visual().unwrap().1, 0);
        world.now_ms += MINUTE;
        assert_eq!(world.cat_window_tap_visual().unwrap().1, 1);
        let mut world: LivingWorld = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        world.now_ms += MINUTE;
        world.step(cat);
        assert_eq!(world.actors[cat].task, Some(Task::Explore(1)));
        assert!(world.contact_state().unwrap().tap.is_none());
    }

    #[test]
    fn the_same_pause_resumes_without_extending_the_original_rest() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let i = ActorKind::Orange.index();
        let foot = world.home_door(0).tile();
        put(&mut world, i, foot);
        world.actors[i].task = Some(Task::Rest);
        world.stage(i, Stage::Work, Pose::Rest, 8 * MINUTE);
        assert!(world.request_cat_window_tap());
        world.stage(i, Stage::Work, Pose::Work, 2 * MINUTE);
        world.now_ms += 2 * MINUTE;
        world.step(i);
        assert_eq!(world.actors[i].task, Some(Task::Rest));
        assert_eq!(world.actors[i].until - world.now_ms, 8 * MINUTE);
        assert_eq!(world.actors[i].cell.tile(), foot);
    }
}
