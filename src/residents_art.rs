use super::*;
use crate::legacy_content::{LegacyActor, LegacyPlace};
use crate::living_world::{BuildingKind, OrchardPhase};

pub(super) const ARTIST: usize = 8;
const DRAW_TIME: u64 = 8 * MINUTE;
const MIN_VISIT_INTERVAL: u64 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VillageArtSubject {
    Blossom(u8),
    Reflection(u8),
    DuskSwatches([u8; 3]),
    DuskStreet([u8; 3]),
}

const DUSK_START: u64 = 18 * 60;
const DUSK_END: u64 = 19 * 60 + 40;

fn valid_swatches(colors: [u8; 3]) -> bool {
    colors[0] == 1 && colors[1] < 2 && colors[2] < 4
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VillageArtVisual {
    pub tile: TilePos,
    pub subject: VillageArtSubject,
    pub slot: Option<u8>,
    pub progress: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Subject {
    image: VillageArtSubject,
    target: TilePos,
    easel: TilePos,
}

impl Subject {
    fn valid(self) -> bool {
        let valid_id = match self.image {
            VillageArtSubject::Blossom(tree) => tree < 4 && distance(self.target, self.easel) <= 2,
            VillageArtSubject::Reflection(level) => {
                (2..=4).contains(&level)
                    && resolve(PhaseMask::ALL, "home_fishing") == Some(self.easel)
            }
            VillageArtSubject::DuskSwatches(colors) | VillageArtSubject::DuskStreet(colors) => {
                valid_swatches(colors)
                    && self.target == house(ARTIST).door
                    && self.easel == self.target
            }
        };
        valid_id && self.target.index().is_some() && self.easel.index().is_some()
    }

    fn witness(self, world: &mut LivingWorld) -> u64 {
        let (base, place) = match self.image {
            VillageArtSubject::Blossom(_) => ("H09-01", LegacyPlace::Expanded),
            VillageArtSubject::Reflection(_) => ("H09-06", LegacyPlace::Expanded),
            VillageArtSubject::DuskSwatches(_) => {
                world.record_legacy_completion(
                    "H09-03", LegacyActor::Resident(ARTIST as u8), self.easel,
                    self.target, LegacyPlace::Nearby, 1, None,
                );
                return 0;
            }
            VillageArtSubject::DuskStreet(_) => return 0,
        };
        world.record_village_exhibit(base, LegacyActor::Resident(ARTIST as u8), self.easel, self.target, place)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Stage {
    WalkToSubject,
    Paint { started: u64 },
    CarryHome { finished: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Work {
    subject: Subject,
    stage: Stage,
    /// Panel 0..3 from the resident's own fixed reusable studio kit.
    panel: u8,
    #[serde(default)]
    display_days: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Picture {
    image: VillageArtSubject,
    panel: u8,
    #[serde(default)]
    reserved_until: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    last_visit: Option<u64>,
    work: Option<Work>,
    gallery: [Option<Picture>; 3],
    next_slot: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    palette: Option<DuskPalette>,
    #[serde(skip_serializing_if = "Option::is_none")]
    leaf_work: Option<LeafWork>,
    #[serde(skip_serializing_if = "Option::is_none")]
    leaf_card: Option<LeafCard>,
    #[serde(skip_serializing_if = "Option::is_none")]
    leaf_attempt: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DuskPalette {
    colors: [u8; 3],
    sampled_day: u64,
    painted: bool,
}

const ORCHARD_CYCLE: u64 = 112;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum LeafStage {
    Collect,
    CarryHome,
    Rub { started: u64 },
    ReturnLeaf,
    ReturnEmpty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeafWork {
    tree: u8,
    source: TilePos,
    day: u64,
    due: u64,
    stage: LeafStage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeafCard {
    tree: u8,
    day: u64,
}

impl State {
    pub(super) fn empty(&self) -> bool {
        self.last_visit.is_none() && self.leaf_card.is_none() && self.leaf_attempt.is_none()
    }

    pub(super) fn valid(&self, now: u64) -> bool {
        let mut panels = 0;
        if self.next_slot >= 3
            || self.last_visit.is_some_and(|day| day > now / DAY_MS)
            || self.last_visit.is_none()
                && (self.work.is_some() || self.gallery.iter().any(Option::is_some))
        {
            return false;
        }
        for picture in self.gallery.iter().flatten() {
            if picture.panel >= 4
                || panels & (1 << picture.panel) != 0
                || picture.reserved_until > now.saturating_add(28 * DAY_MS)
                || match picture.image {
                    VillageArtSubject::Blossom(tree) => tree >= 4,
                    VillageArtSubject::Reflection(level) => !(2..=4).contains(&level),
                    VillageArtSubject::DuskSwatches(_) => true,
                    VillageArtSubject::DuskStreet(colors) => !valid_swatches(colors),
                }
            {
                return false;
            }
            panels |= 1 << picture.panel;
        }
        self.leaf_attempt.is_none_or(|day| day <= now / DAY_MS)
            && self
                .leaf_card
                .is_none_or(|card| (8..12).contains(&card.tree) && card.day <= now / DAY_MS)
            && self.leaf_work.is_none_or(|leaf| {
                self.work.is_none()
                    && self.leaf_attempt == Some(leaf.day)
                    && (8..12).contains(&leaf.tree)
                    && leaf.source.index().is_some()
                    && leaf.due <= leaf.day * DAY_MS + 18 * HOUR
                    && match leaf.stage {
                        LeafStage::Rub { started } => started <= now,
                        _ => true,
                    }
            })
            && self.palette.is_none_or(|palette| {
                palette.sampled_day <= now / DAY_MS && valid_swatches(palette.colors)
            })
            && self.work.is_none_or(|work| {
                work.panel < 4
                    && matches!(work.display_days, 0 | 14 | 28)
                    && (work.display_days == 0 || matches!(work.stage, Stage::CarryHome { finished: true }))
                    && panels & (1 << work.panel) == 0
                    && work.subject.valid()
                    && match work.stage {
                        Stage::Paint { started } => started <= now,
                        _ => true,
                    }
            })
    }

    pub(super) fn active(&self) -> bool {
        self.work.is_some() || self.leaf_work.is_some()
    }

    fn available_mount(&self, now: u64) -> Option<usize> {
        let len = self.gallery.len();
        (0..len).map(|offset| (usize::from(self.next_slot) + offset) % len)
            .find(|&slot| self.gallery[slot].is_none_or(|picture| picture.reserved_until <= now))
    }

    pub(super) fn leaf_walk_valid(&self, walker: &Walker) -> bool {
        self.leaf_work.is_none_or(|work| {
            walker.target
                == match work.stage {
                    LeafStage::Collect | LeafStage::ReturnLeaf => work.source,
                    _ => house(ARTIST).door,
                }
                && (!matches!(work.stage, LeafStage::Rub { .. }) || walker.at == house(ARTIST).door)
        })
    }

    pub(super) fn holds_leaf(&self) -> bool {
        self.leaf_work.is_some_and(|work| {
            matches!(
                work.stage,
                LeafStage::CarryHome | LeafStage::Rub { .. } | LeafStage::ReturnLeaf
            )
        })
    }

    pub(super) fn leaf_display(&self, now: u64) -> Option<(TilePos, u8, u8)> {
        let home = house(ARTIST).door;
        if let Some(work) = self.leaf_work
            && let LeafStage::Rub { started } = work.stage
        {
            return Some((
                home,
                work.tree,
                (now.saturating_sub(started).min(DRAW_TIME) * 100 / DRAW_TIME) as u8,
            ));
        }
        self.leaf_card.map(|card| (home, card.tree, 100))
    }

    pub(super) fn leaf_candidate(&self, world: &LivingWorld, now: u64) -> Option<(u8, TilePos)> {
        let day = now / DAY_MS;
        if self.active()
            || self.leaf_attempt == Some(day)
            || !world.town_allows(TownEra::MarketStreet)
            || !(450..840).contains(&(now % DAY_MS / MINUTE))
            || self
                .leaf_card
                .is_some_and(|card| card.day / ORCHARD_CYCLE == day / ORCHARD_CYCLE)
        {
            return None;
        }
        (8..12).find_map(|tree| world.village_leaf_rubbing_foot(tree).map(|at| (tree, at)))
    }

    pub(super) fn start_leaf(&mut self, walker: &mut Walker, tree: u8, source: TilePos, now: u64) {
        self.leaf_attempt = Some(now / DAY_MS);
        self.leaf_work = Some(LeafWork {
            tree,
            source,
            day: now / DAY_MS,
            due: now + 4 * HOUR,
            stage: LeafStage::Collect,
        });
        walker.redirect(source, super::State::Outbound, now);
    }

    fn advance_leaf(&mut self, world: &mut LivingWorld, walker: &mut Walker, now: u64) -> bool {
        let Some(mut work) = self.leaf_work else {
            return false;
        };
        if walker.edge.is_some() {
            return true;
        }
        let home = house(ARTIST).door;
        let wet = matches!(world.weather(), YardWeather::Rain | YardWeather::Snow);
        match work.stage {
            LeafStage::Collect
                if now >= work.due
                    || world.village_leaf_rubbing_foot(work.tree) != Some(work.source) =>
            {
                work.stage = LeafStage::ReturnEmpty;
                walker.redirect(home, super::State::Outbound, now);
            }
            LeafStage::Collect if walker.at == work.source => {
                work.stage = LeafStage::CarryHome;
                walker.redirect(home, super::State::Outbound, now);
            }
            LeafStage::CarryHome | LeafStage::Rub { .. } if wet || now >= work.due => {
                work.stage = LeafStage::ReturnLeaf;
                walker.redirect(work.source, super::State::Outbound, now);
            }
            LeafStage::CarryHome if walker.at == home => {
                work.stage = LeafStage::Rub { started: now };
                walker.state = super::State::AtWork;
                walker.due = now + DRAW_TIME;
            }
            LeafStage::Rub { started } if walker.at == home && now >= started + DRAW_TIME => {
                self.leaf_card = Some(LeafCard {
                    tree: work.tree,
                    day: work.day,
                });
                self.leaf_work = None;
                walker.state = super::State::Inside;
                world.record_legacy_completion(
                    "H09-07",
                    LegacyActor::Resident(ARTIST as u8),
                    home,
                    home,
                    LegacyPlace::Nearby,
                    1,
                    None,
                );
                return true;
            }
            LeafStage::ReturnLeaf if walker.at == work.source => {
                work.stage = LeafStage::ReturnEmpty;
                walker.redirect(home, super::State::Outbound, now);
            }
            LeafStage::ReturnEmpty if walker.at == home => {
                self.leaf_work = None;
                walker.state = super::State::Inside;
                return true;
            }
            _ => {}
        }
        self.leaf_work = Some(work);
        true
    }

    pub(super) fn carries_panel(&self) -> bool {
        self.work
            .is_some_and(|work| !matches!(work.stage, Stage::Paint { .. }))
    }

    pub(super) fn subject_destination(&self) -> Option<TilePos> {
        if let Some(work) = self.leaf_work {
            return Some(work.source);
        }
        self.work
            .filter(|work| !matches!(work.stage, Stage::CarryHome { .. }))
            .map(|work| work.subject.easel)
    }

    fn actual_subjects(world: &LivingWorld) -> impl Iterator<Item = Subject> + '_ {
        let blooms = (0..4).filter_map(|tree| {
            if world.orchard_phase(tree + 8) != Some(OrchardPhase::Flower) {
                return None;
            }
            let position = world.trees().get(tree + 8)?.position;
            let target = TilePos::new(
                (position.x as u16) / TILE_PX as u16,
                (position.y as u16) / TILE_PX as u16,
            );
            let easel = [(0, 1), (1, 0), (-1, 0), (0, -1), (0, 2)]
                .into_iter()
                .filter_map(|(dx, dy)| target.offset(dx, dy))
                .find(|tile| world.can_walk(tile.x.into(), tile.y.into()))?;
            Some(Subject {
                image: VillageArtSubject::Blossom(tree as u8),
                target,
                easel,
            })
        });
        let pool = world.buildings().iter().find_map(|building| {
            let easel = resolve(world.phase_mask(), "home_fishing")?;
            if building.kind != BuildingKind::WestHouse || !(2..=4).contains(&building.level) {
                return None;
            }
            let target = TilePos::new(
                (building.position.x as u16) / TILE_PX as u16,
                (building.position.y as u16) / TILE_PX as u16,
            );
            if distance(target, easel) > 24 {
                return None;
            }
            Some(Subject {
                image: VillageArtSubject::Reflection(building.level),
                target,
                easel,
            })
        });
        blooms.chain(pool)
    }

    pub(super) fn candidate(
        &self,
        world: &LivingWorld,
        now: u64,
    ) -> Option<(TilePos, VillageArtSubject)> {
        if !self.active()
            && let Some(subject) = self.dusk_subject(world, now)
            && (matches!(subject.image, VillageArtSubject::DuskSwatches(_)) || self.available_mount(now).is_some())
        {
            return Some((subject.easel, subject.image));
        }
        let day = now / DAY_MS;
        if self.active()
            || self.available_mount(now).is_none()
            || self
                .last_visit
                .is_some_and(|last| day < last + MIN_VISIT_INTERVAL)
            || !world.town_allows(TownEra::MarketStreet)
            || day % 7 == 2
            || !(450..540).contains(&(now % DAY_MS / MINUTE))
            || matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            return None;
        }
        Self::actual_subjects(world)
            .find(|subject| {
                !self
                    .gallery
                    .iter()
                    .flatten()
                    .any(|picture| picture.image == subject.image)
            })
            .map(|subject| (subject.easel, subject.image))
    }

    pub(super) fn waiting_for_dusk(&self, world: &LivingWorld, now: u64) -> bool {
        let day = now / DAY_MS;
        world.town_allows(TownEra::MarketStreet)
            && (16 * 60 + 30..DUSK_END - 12).contains(&(now % DAY_MS / MINUTE))
            && !matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
            && self.palette.is_none_or(|palette| {
                palette.sampled_day / 28 != day / 28
                    || !palette.painted && day > palette.sampled_day
            })
    }

    fn dusk_subject(&self, world: &LivingWorld, now: u64) -> Option<Subject> {
        let minute = now % DAY_MS / MINUTE;
        if !self.waiting_for_dusk(world, now) || minute < DUSK_START {
            return None;
        }
        let day = now / DAY_MS;
        let home = house(ARTIST).door;
        if !world.village_building_active(house(ARTIST).id)
            || !world.can_walk(home.x.into(), home.y.into())
            || ![
                (home.x.saturating_sub(1), home.y),
                (home.x + 1, home.y),
                (home.x, home.y + 1),
            ]
            .into_iter()
            .any(|(x, y)| world.ground(x.into(), y.into()) == Ground::Grass)
        {
            return None;
        }
        let image = if let Some(palette) = self
            .palette
            .filter(|palette| palette.sampled_day / 28 == day / 28)
        {
            VillageArtSubject::DuskStreet(palette.colors)
        } else {
            let sky = u8::from(world.weather() == YardWeather::Clouds);
            let grass = match world.season() {
                crate::living_world::YardSeason::Spring => 0,
                crate::living_world::YardSeason::Summer => 1,
                crate::living_world::YardSeason::Autumn => 2,
                crate::living_world::YardSeason::Winter => 3,
            };
            VillageArtSubject::DuskSwatches([1, sky, grass])
        };
        Some(Subject {
            image,
            target: home,
            easel: home,
        })
    }

    pub(super) fn collect(
        &mut self,
        world: &LivingWorld,
        walker: &mut Walker,
        at: TilePos,
        image: VillageArtSubject,
        now: u64,
    ) -> bool {
        if walker.at != house(ARTIST).door || walker.edge.is_some() || self.active() {
            return false;
        }
        let Some(subject) = Self::actual_subjects(world)
            .chain(self.dusk_subject(world, now))
            .find(|source| source.easel == at && source.image == image)
        else {
            return false;
        };
        if !matches!(subject.image, VillageArtSubject::DuskSwatches(_)) && self.available_mount(now).is_none() {
            return false;
        }
        // Three display mounts and one spare board. A fitted picture is never
        // used as the in-flight board. A retired picture returns to the same
        // porch rack at replacement, so tomorrow's work cannot create stock.
        let Some(panel) = (0..4).find(|id| {
            self.gallery
                .iter()
                .flatten()
                .all(|picture| picture.panel != *id)
        }) else {
            return false;
        };
        self.work = Some(Work {
            subject,
            stage: Stage::WalkToSubject,
            panel,
            display_days: 0,
        });
        self.last_visit = Some(now / DAY_MS);
        walker.redirect(at, super::State::Outbound, now);
        true
    }

    pub(super) fn advance(&mut self, world: &mut LivingWorld, walker: &mut Walker, now: u64) {
        if self.advance_leaf(world, walker, now) {
            return;
        }
        let Some(mut work) = self.work else { return };
        let home = house(ARTIST).door;
        if walker.edge.is_some() {
            return;
        }
        let dusk = matches!(
            work.subject.image,
            VillageArtSubject::DuskSwatches(_) | VillageArtSubject::DuskStreet(_)
        );
        let subject_still_present = if dusk {
            (DUSK_START..DUSK_END).contains(&(now % DAY_MS / MINUTE))
        } else {
            now % DAY_MS / MINUTE < 970
                && Self::actual_subjects(world).any(|subject| subject == work.subject)
        };
        if !matches!(work.stage, Stage::CarryHome { .. })
            && (matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
                || !subject_still_present)
        {
            work.stage = Stage::CarryHome { finished: false };
            walker.redirect(home, super::State::Outbound, now);
        }
        match work.stage {
            Stage::WalkToSubject if walker.at == work.subject.easel => {
                work.stage = Stage::Paint { started: now };
                walker.due = now + DRAW_TIME;
                walker.state = super::State::AtWork;
            }
            Stage::Paint { started }
                if now >= started + DRAW_TIME && walker.at == work.subject.easel =>
            {
                work.display_days = work.subject.witness(world);
                work.stage = Stage::CarryHome { finished: true };
                walker.redirect(home, super::State::Outbound, now);
            }
            Stage::CarryHome { finished } if walker.at == home => {
                if finished && let VillageArtSubject::DuskSwatches(colors) = work.subject.image {
                    self.palette = Some(DuskPalette {
                        colors,
                        sampled_day: now / DAY_MS,
                        painted: false,
                    });
                } else if finished && let Some(slot) = self.available_mount(now) {
                    if let VillageArtSubject::DuskStreet(colors) = work.subject.image
                        && let Some(palette) = &mut self.palette
                        && palette.colors == colors
                    {
                        palette.painted = true;
                    }
                    let replacement_days = if self.gallery[slot].is_some() && self.gallery.iter().all(Option::is_some) {
                        world.record_village_exhibit(
                            "H09-10",
                            LegacyActor::Resident(ARTIST as u8),
                            home,
                            home,
                            LegacyPlace::Nearby,
                        )
                    } else {
                        0
                    };
                    let display_days = work.display_days.max(replacement_days);
                    self.gallery[slot] = Some(Picture {
                        image: work.subject.image,
                        panel: work.panel,
                        reserved_until: if display_days == 0 { 0 } else { now.saturating_add(display_days * DAY_MS) },
                    });
                    self.next_slot = ((slot + 1) % self.gallery.len()) as u8;
                }
                self.work = None;
                walker.state = super::State::Inside;
                return;
            }
            _ => {}
        }
        self.work = Some(work);
    }

    pub(super) fn visuals(&self, now: u64) -> impl Iterator<Item = VillageArtVisual> + '_ {
        self.gallery
            .iter()
            .enumerate()
            .filter_map(|(slot, picture)| {
                Some(VillageArtVisual {
                    tile: house(ARTIST).door,
                    subject: picture.as_ref()?.image,
                    slot: Some(slot as u8),
                    progress: 100,
                })
            })
            .chain(self.work.iter().filter_map(move |work| {
                let Stage::Paint { started } = work.stage else {
                    return None;
                };
                Some(VillageArtVisual {
                    tile: work.subject.easel,
                    subject: work.subject.image,
                    slot: None,
                    progress: (now.saturating_sub(started).min(DRAW_TIME) * 100 / DRAW_TIME) as u8,
                })
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance_clock(world: &mut LivingWorld, duration: u64) {
        let end = world.world_ms() + duration;
        world.queue_world_time(duration);
        for _ in 0..1024 {
            if world.advance_pending_to(end) && world.world_ms() == end {
                return;
            }
        }
        panic!("fixture world clock did not reach {}", end);
    }

    #[test]
    fn expansion_leaf_card_keeps_one_source_and_rejects_calendar_only_leaf_fall() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(60);
        let mut value = serde_json::to_value(&world).unwrap();
        value["village"]["orchard"][0]["phase"] = "AfterHarvest".into();
        value["village"]["orchard"][0]["fruit"] = 0.into();
        let snapshot = |day: u64| {
            let mut d = value.clone();
            d["now_ms"] = (day * DAY_MS + 8 * HOUR).into();
            serde_json::from_value::<LivingWorld>(d).unwrap()
        };
        assert!((77..83).all(|day| snapshot(day).village_leaf_rubbing_foot(8).is_none()));
        let mut world = (56..77)
            .map(snapshot)
            .find(|day| day.village_leaf_rubbing_foot(8).is_some())
            .expect("dry source during the real lifted-fruit period");
        let now = world.world_ms();
        let home = house(ARTIST).door;
        let mut art = State::default();
        let mut walker = Walker::new(home, super::super::State::AtWork);
        let (tree, source) = art.leaf_candidate(&world, now).unwrap();
        assert_eq!(tree, 8);
        assert_ne!(world.village_leaf_mulch_site().unwrap().1, source);
        art.start_leaf(&mut walker, tree, source, now);
        assert!(!art.holds_leaf());
        walker = Walker::new(source, super::super::State::AtWork);
        art.advance(&mut world, &mut walker, now + MINUTE);
        assert!(art.holds_leaf());
        assert_eq!(art.leaf_work.unwrap().tree, tree);
        let mut art: State = serde_json::from_slice(&serde_json::to_vec(&art).unwrap()).unwrap();
        assert!(art.leaf_walk_valid(&walker));
        walker = Walker::new(home, super::super::State::AtWork);
        art.advance(&mut world, &mut walker, now + 2 * MINUTE);
        assert_eq!(art.leaf_display(now + 3 * MINUTE).unwrap().1, tree);
        art.advance(&mut world, &mut walker, now + 2 * MINUTE + DRAW_TIME);
        assert_eq!(art.leaf_display(now + HOUR), Some((home, tree, 100)));
        assert!(!art.holds_leaf());
        assert!(art.leaf_candidate(&world, now + DAY_MS).is_none());
    }

    #[test]
    fn expansion_actual_dusk_palette_feeds_only_the_following_finished_picture() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(60);
        let home = house(ARTIST).door;
        let mut state = State::default();
        let mut walker = Walker::new(home, super::super::State::AtWork);
        assert!(state.dusk_subject(&world, world.world_ms()).is_none());
        for _ in 0..10 {
            let now = world.world_ms();
            let dusk = (now / DAY_MS) * DAY_MS + DUSK_START * MINUTE;
            let when = if dusk > now { dusk } else { dusk + DAY_MS };
            advance_clock(&mut world, when - now);
            if state.dusk_subject(&world, world.world_ms()).is_some() {
                break;
            }
        }
        let now = world.world_ms();
        let sample = state
            .dusk_subject(&world, now)
            .expect("dry live porch at actual dusk");
        let VillageArtSubject::DuskSwatches(colors) = sample.image else {
            panic!()
        };
        assert!(state.collect(&world, &mut walker, home, sample.image, now));
        state.advance(&mut world, &mut walker, now);
        assert!(
            state
                .visuals(now)
                .any(|visual| visual.subject == sample.image && visual.progress == 0)
        );
        advance_clock(&mut world, DRAW_TIME);
        state.advance(&mut world, &mut walker, now + DRAW_TIME);
        state.advance(&mut world, &mut walker, now + DRAW_TIME);
        assert_eq!(state.palette.unwrap().colors, colors);
        assert!(!state.palette.unwrap().painted);
        assert!(state.gallery.iter().all(Option::is_none));
        assert!(state.dusk_subject(&world, now + DRAW_TIME).is_none());
        let mut state: State =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert!(state.valid(world.world_ms()));
        advance_clock(&mut world, DAY_MS - DRAW_TIME);
        for _ in 0..7 {
            if state.dusk_subject(&world, world.world_ms()).is_some() {
                break;
            }
            advance_clock(&mut world, DAY_MS);
        }
        let next = world.world_ms();
        let sketch = state.dusk_subject(&world, next).unwrap();
        assert_eq!(sketch.image, VillageArtSubject::DuskStreet(colors));
        assert!(state.collect(&world, &mut walker, home, sketch.image, next));
        state.advance(&mut world, &mut walker, next);
        advance_clock(&mut world, DRAW_TIME);
        state.advance(&mut world, &mut walker, next + DRAW_TIME);
        state.advance(&mut world, &mut walker, next + DRAW_TIME);
        assert!(state.palette.unwrap().painted);
        assert_eq!(state.gallery[0].unwrap().image, sketch.image);
    }

    fn make_subjects(mut world: LivingWorld) -> LivingWorld {
        advance_clock(&mut world, MINUTE);
        let mut data = serde_json::to_value(world).unwrap();
        for tree in data["village"]["orchard"].as_array_mut().unwrap() {
            tree["phase"] = "Flower".into();
        }
        let house = data["buildings"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["kind"] == "WestHouse")
            .unwrap();
        house["level"] = 2.into();
        serde_json::from_value(data).unwrap()
    }

    #[test]
    fn weather_variants_keep_the_same_painted_board_on_display() {
        use crate::living_world::YardWeather::{Clear, Clouds};
        let cases = [
            ("H09-01~11022", 0, 10, Clouds, 14),
            ("H09-06~10102", 2, 10, Clear, 14),
            ("H09-06~10223", 3, 10, Clear, 28),
            ("H09-10~11143", 2, 10, Clouds, 28),
            ("H09-10~21043", 0, 18, Clouds, 28),
        ];
        let mut initial = make_subjects(LivingWorld::new_village_wall_ms(0));
        initial.set_test_town_age(60);
        let home = house(ARTIST).door;
        for (id, season, hour, weather, days) in cases {
            let make_world = |now| {
                let mut state = serde_json::to_value(&initial).unwrap();
                state["now_ms"] = serde_json::Value::from(now);
                state["target_ms"] = serde_json::Value::from(now);
                serde_json::from_value::<LivingWorld>(state).unwrap()
            };
            let now = (21_505 + season * 28..21_504 + (season + 1) * 28)
                .map(|day| day * DAY_MS + hour * HOUR + 10 * MINUTE)
                .find(|&clock| make_world(clock).weather() == weather)
                .unwrap();
            let mut world = make_world(now);
            let mut studio = State::default();
            if id.starts_with("H09-10") {
                studio.last_visit = Some(now / DAY_MS);
                studio.gallery = [0, 1, 2].map(|panel| Some(Picture {
                    image: VillageArtSubject::Blossom(panel),
                    panel,
                    reserved_until: 0,
                }));
            }
            let subject = if hour == 18 {
                studio.palette = Some(DuskPalette { colors: [1, 1, 0], sampled_day: now / DAY_MS - 1, painted: false });
                let subject = studio.dusk_subject(&world, now).unwrap();
                assert!(matches!(subject.image, VillageArtSubject::DuskStreet(_)));
                subject
            } else {
                State::actual_subjects(&world).find(|subject| {
                    if id.starts_with("H09-01") {
                        matches!(subject.image, VillageArtSubject::Blossom(_))
                    } else {
                        matches!(subject.image, VillageArtSubject::Reflection(_))
                    }
                }).unwrap()
            };
            let mut walker = Walker::new(home, super::super::State::Inside);
            assert!(studio.collect(&world, &mut walker, subject.easel, subject.image, now), "{id}");
            let panel = studio.work.unwrap().panel;
            assert_eq!(walker.target, subject.easel);
            walker = Walker::new(subject.easel, super::super::State::AtWork);
            studio.advance(&mut world, &mut walker, now);
            let done = now + DRAW_TIME;
            world = make_world(done);
            studio.advance(&mut world, &mut walker, done);
            assert_eq!(walker.target, home);
            assert!(studio.carries_panel());
            let bytes = serde_json::to_vec(&studio).unwrap();
            let mut studio: State = serde_json::from_slice(&bytes).unwrap();
            assert!(studio.valid(done));
            walker = Walker::new(home, super::super::State::AtWork);
            studio.advance(&mut world, &mut walker, done);
            assert!(world.legacy_behaviors().observed_ids().any(|seen| seen == id), "{id}");
            let slot = studio.gallery.iter().position(|entry| entry.is_some_and(|picture| picture.panel == panel)).unwrap();
            let held = studio.gallery[slot].unwrap();
            let until = done + days * DAY_MS;
            assert_eq!(held.reserved_until, until, "{id}");
            assert!(studio.visuals(done).any(|view| view.slot == Some(slot as u8) && view.subject == subject.image));
            studio.next_slot = slot as u8;
            assert_ne!(studio.available_mount(until - 1), Some(slot));
            assert_eq!(studio.available_mount(until), Some(slot));
        }
    }

    #[test]
    fn legacy_residents_artist_requires_actual_source_and_reuses_four_physical_boards() {
        let mut world = make_subjects(LivingWorld::new_village_wall_ms(0));
        while matches!(world.weather(), YardWeather::Rain | YardWeather::Snow) {
            advance_clock(&mut world, HOUR);
        }
        let now = world.world_ms();
        let home = house(ARTIST).door;
        let mut walker = Walker::new(home, super::super::State::Inside);
        let mut studio = State::default();
        assert_eq!(studio.visuals(now).count(), 0);
        let mut actual: Vec<_> = State::actual_subjects(&world).collect();
        actual.sort_by_key(|subject| matches!(subject.image, VillageArtSubject::Blossom(_)));
        assert!(
            actual
                .iter()
                .any(|subject| matches!(subject.image, VillageArtSubject::Reflection(2)))
        );
        assert!(
            actual
                .iter()
                .any(|subject| matches!(subject.image, VillageArtSubject::Blossom(_)))
        );
        assert!(!studio.collect(
            &world,
            &mut walker,
            home,
            VillageArtSubject::Reflection(2),
            now
        ));
        for subject in actual.into_iter().take(4) {
            let now = world.world_ms();
            walker = Walker::new(home, super::super::State::Inside);
            assert!(
                studio.collect(&world, &mut walker, subject.easel, subject.image, now),
                "fixture subject {subject:?} remained available at {now}"
            );
            assert!(studio.carries_panel());
            assert_eq!(walker.target, subject.easel);
            studio.advance(&mut world, &mut walker, now);
            assert!(studio.visuals(now).all(|visual| visual.slot.is_some()));
            walker = Walker::new(subject.easel, super::super::State::AtWork);
            studio.advance(&mut world, &mut walker, now);
            assert!(
                studio
                    .visuals(now)
                    .any(|v| v.slot.is_none() && v.progress == 0)
            );
            let encoded = serde_json::to_vec(&studio).unwrap();
            studio = serde_json::from_slice(&encoded).unwrap();
            assert!(studio.valid(now));
            advance_clock(&mut world, DRAW_TIME);
            studio.advance(&mut world, &mut walker, now + DRAW_TIME);
            assert!(studio.carries_panel());
            assert_eq!(walker.target, home);
            walker = Walker::new(home, super::super::State::AtWork);
            studio.advance(&mut world, &mut walker, now + DRAW_TIME);
            assert!(!studio.active());
            assert!(studio.valid(now + DRAW_TIME));
        }
        assert_eq!(studio.visuals(world.world_ms()).count(), 3);
        assert!(
            world
                .legacy_behaviors()
                .recent()
                .iter()
                .any(|e| e.id().starts_with("H09-01"))
        );
        assert!(
            world
                .legacy_behaviors()
                .recent()
                .iter()
                .any(|e| e.id().starts_with("H09-06"))
        );
        assert!(
            world
                .legacy_behaviors()
                .recent()
                .iter()
                .any(|e| e.id().starts_with("H09-10"))
        );
    }

    #[test]
    fn legacy_residents_artist_returns_original_board_without_exhibiting_if_subject_is_gone() {
        let mut world = make_subjects(LivingWorld::new_village_wall_ms(0));
        let mut studio = State::default();
        let home = house(ARTIST).door;
        let now = world.world_ms();
        let subject = State::actual_subjects(&world)
            .find(|s| matches!(s.image, VillageArtSubject::Blossom(_)))
            .unwrap();
        let mut walker = Walker::new(home, super::super::State::Inside);
        assert!(studio.collect(&world, &mut walker, subject.easel, subject.image, now));
        let panel = studio.work.unwrap().panel;
        let mut data = serde_json::to_value(&world).unwrap();
        for tree in data["village"]["orchard"].as_array_mut().unwrap() {
            tree["phase"] = "AfterHarvest".into();
        }
        world = serde_json::from_value(data).unwrap();
        walker = Walker::new(subject.easel, super::super::State::AtWork);
        studio.advance(&mut world, &mut walker, now);
        assert_eq!(walker.target, home);
        assert_eq!(studio.work.unwrap().panel, panel);
        assert!(studio.carries_panel());
        assert_eq!(studio.visuals(now).count(), 0);
        let encoded = serde_json::to_vec(&studio).unwrap();
        let mut studio: State = serde_json::from_slice(&encoded).unwrap();
        walker = Walker::new(home, super::super::State::AtWork);
        studio.advance(&mut world, &mut walker, now);
        assert!(!studio.active());
        assert_eq!(studio.visuals(now).count(), 0);
    }
}
