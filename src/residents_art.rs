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
        };
        valid_id && self.target.index().is_some() && self.easel.index().is_some()
    }

    fn witness(self, world: &mut LivingWorld) {
        let base = match self.image {
            VillageArtSubject::Blossom(_) => "H09-01",
            VillageArtSubject::Reflection(_) => "H09-06",
        };
        world.record_legacy_completion(
            base,
            LegacyActor::Resident(ARTIST as u8),
            self.easel,
            self.target,
            LegacyPlace::Expanded,
            1,
            None,
        );
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Picture {
    image: VillageArtSubject,
    panel: u8,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    last_visit: Option<u64>,
    work: Option<Work>,
    gallery: [Option<Picture>; 3],
    next_slot: u8,
}

impl State {
    pub(super) fn empty(&self) -> bool {
        self.last_visit.is_none()
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
                || match picture.image {
                    VillageArtSubject::Blossom(tree) => tree >= 4,
                    VillageArtSubject::Reflection(level) => !(2..=4).contains(&level),
                }
            {
                return false;
            }
            panels |= 1 << picture.panel;
        }
        self.work.is_none_or(|work| {
            work.panel < 4
                && panels & (1 << work.panel) == 0
                && work.subject.valid()
                && match work.stage {
                    Stage::Paint { started } => started <= now,
                    _ => true,
                }
        })
    }

    pub(super) fn active(&self) -> bool {
        self.work.is_some()
    }

    pub(super) fn carries_panel(&self) -> bool {
        self.work
            .is_some_and(|work| !matches!(work.stage, Stage::Paint { .. }))
    }

    pub(super) fn subject_destination(&self) -> Option<TilePos> {
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
        let day = now / DAY_MS;
        if self.active()
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
        let Some(subject) =
            Self::actual_subjects(world).find(|source| source.easel == at && source.image == image)
        else {
            return false;
        };
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
        });
        self.last_visit = Some(now / DAY_MS);
        walker.redirect(at, super::State::Outbound, now);
        true
    }

    pub(super) fn advance(&mut self, world: &mut LivingWorld, walker: &mut Walker, now: u64) {
        let Some(mut work) = self.work else { return };
        let home = house(ARTIST).door;
        if walker.edge.is_some() {
            return;
        }
        if !matches!(work.stage, Stage::CarryHome { .. })
            && (matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
                || now % DAY_MS / MINUTE >= 970
                || !Self::actual_subjects(world).any(|subject| subject == work.subject))
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
                work.subject.witness(world);
                work.stage = Stage::CarryHome { finished: true };
                walker.redirect(home, super::State::Outbound, now);
            }
            Stage::CarryHome { finished } if walker.at == home => {
                if finished {
                    let replacement = self.gallery[usize::from(self.next_slot)].replace(Picture {
                        image: work.subject.image,
                        panel: work.panel,
                    });
                    self.next_slot = (self.next_slot + 1) % 3;
                    if replacement.is_some() && self.gallery.iter().all(Option::is_some) {
                        world.record_legacy_completion(
                            "H09-10",
                            LegacyActor::Resident(ARTIST as u8),
                            home,
                            home,
                            LegacyPlace::Nearby,
                            1,
                            None,
                        );
                    }
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
