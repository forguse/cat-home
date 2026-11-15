//! One shared roof barrel with one physical watering load.
use super::*;
use crate::legacy_content::{LegacyActor, LegacyPlace};

const WORKER: usize = 16;
const CAPACITY: u8 = 6;
const ALIGN_TIME: u64 = 6 * MINUTE;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Job {
    Align {
        ready: u64,
    },
    Water {
        plot: u8,
        at: TilePos,
        due: u64,
        started: Option<u64>,
        returning: bool,
    },
    Repair {
        cupboard: TilePos,
        due: u64,
        has_wood: bool,
        started: Option<u64>,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    aligned: bool,
    water: u8,
    last_collection: Option<u64>,
    job: Option<Job>,
    wear: u8,
    leaked: bool,
    last_leak: Option<u64>,
}

fn roof() -> TilePos {
    house(WORKER).door
}

impl State {
    pub(super) fn empty(&self) -> bool {
        !self.aligned && self.job.is_none()
    }

    pub(super) fn valid(&self, person: &Resident, now: u64) -> bool {
        self.water.saturating_add(u8::from(self.owns_bucket())) <= CAPACITY
            && self
                .last_collection
                .is_none_or(|block| self.aligned && block <= now / (3 * HOUR))
            && (self.aligned || self.water == 0 && self.last_collection.is_none())
            && self.wear <= 6
            && (!self.leaked || self.aligned && self.wear >= 4)
            && self
                .last_leak
                .is_none_or(|block| self.leaked && block <= now / (3 * HOUR))
            && self.job.is_none_or(|job| match job {
                Job::Align { ready } => {
                    !self.aligned
                        && person.walk.target == roof()
                        && ready <= now.saturating_add(ALIGN_TIME)
                }
                Job::Water {
                    plot,
                    at,
                    due,
                    started,
                    returning,
                } => {
                    self.aligned
                        && plot < 32
                        && at.index().is_some()
                        && due <= now.saturating_add(3 * HOUR)
                        && started.is_none_or(|time| time <= now && time <= due)
                        && person.walk.target == if returning { roof() } else { at }
                }
                Job::Repair {
                    cupboard,
                    due,
                    has_wood,
                    started,
                } => {
                    self.leaked
                        && slot(PhaseMask::ALL, "pantry_loading", "load") == Some(cupboard)
                        && due <= now.saturating_add(3 * HOUR)
                        && started.is_none_or(|time| has_wood && time <= now)
                        && person.walk.target == if has_wood { roof() } else { cupboard }
                }
            })
    }

    pub(super) fn active(&self, id: usize) -> bool {
        id == WORKER && self.job.is_some()
    }

    fn owns_bucket(&self) -> bool {
        matches!(self.job, Some(Job::Water { .. }))
    }

    pub(super) fn held(&self, id: usize) -> Option<Item> {
        if id != WORKER {
            return None;
        }
        match self.job? {
            Job::Water { .. } => Some(Item::Water),
            Job::Repair { has_wood: true, .. } => Some(Item::Wood),
            _ => None,
        }
    }

    pub(super) fn name(&self, id: usize) -> Option<&'static str> {
        self.active(id).then(|| match self.job.unwrap() {
            Job::Align { .. } => "把檐下短管对准集雨桶",
            Job::Water {
                returning: false, ..
            } => "提取集雨浇田",
            Job::Water {
                returning: true, ..
            } => "把未用雨水送回原桶",
            Job::Repair {
                has_wood: false, ..
            } => "去仓房取雨桶补板",
            Job::Repair { has_wood: true, .. } => "给真实漏水的桶贴补板",
        })
    }

    pub(super) fn site(&self) -> Option<TilePos> {
        self.job.map(|job| match job {
            Job::Water {
                at,
                returning: false,
                ..
            } => at,
            Job::Repair {
                cupboard,
                has_wood: false,
                ..
            } => cupboard,
            _ => roof(),
        })
    }

    pub(super) fn visual(&self) -> Option<(TilePos, u8, bool)> {
        (self.aligned || matches!(self.job, Some(Job::Align { .. })))
            .then(|| (roof(), self.water, self.leaked))
    }

    pub(super) fn rain_fill(&mut self, world: &LivingWorld, now: u64) {
        if !self.aligned {
            return;
        }
        let block = now / (3 * HOUR);
        if self.wear >= 4 && self.water > 0 && self.last_leak != Some(block) {
            self.water -= 1;
            self.leaked = true;
            self.last_leak = Some(block);
        }
        if self.last_collection == Some(block) || world.weather() != YardWeather::Rain {
            return;
        }
        self.last_collection = Some(block);
        let limit = CAPACITY - u8::from(self.owns_bucket());
        if self.water < limit {
            self.water += 1;
            self.wear = self.wear.saturating_add(1).min(6);
        }
    }

    pub(super) fn schedule(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        now: u64,
        allow_start: bool,
    ) -> bool {
        let home = roof();
        let person = &life.people[WORKER];
        let at = person.walk.at;
        if let Some(job) = self.job {
            match job {
                Job::Align { ready } => {
                    if at == home && now >= ready {
                        self.job = None;
                        if world.weather() == YardWeather::Rain {
                            self.aligned = true;
                            self.rain_fill(world, now);
                            world.record_legacy_completion(
                                "H08-03",
                                LegacyActor::Resident(WORKER as u8),
                                home,
                                home,
                                LegacyPlace::Nearby,
                                1,
                                None,
                            );
                        }
                    }
                }
                Job::Water {
                    plot,
                    at: site,
                    due,
                    mut started,
                    returning,
                } => {
                    if returning {
                        if at == home {
                            self.water += 1;
                            self.job = None;
                        }
                    } else if now >= due || at == site && world.weather() == YardWeather::Rain {
                        self.job = Some(Job::Water {
                            plot,
                            at: site,
                            due,
                            started,
                            returning: true,
                        });
                        life.people[WORKER]
                            .walk
                            .redirect(home, super::State::Outbound, now);
                    } else if at == site {
                        let first = *started.get_or_insert(now);
                        self.job = Some(Job::Water {
                            plot,
                            at: site,
                            due,
                            started,
                            returning: false,
                        });
                        if now >= first + 4 * MINUTE {
                            if world.village_water_from_barrel(plot, at) {
                                self.job = None;
                            } else {
                                self.job = Some(Job::Water {
                                    plot,
                                    at: site,
                                    due,
                                    started,
                                    returning: true,
                                });
                                life.people[WORKER].walk.redirect(
                                    home,
                                    super::State::Outbound,
                                    now,
                                );
                            }
                        }
                    }
                }
                Job::Repair {
                    cupboard,
                    due,
                    has_wood,
                    mut started,
                } => {
                    if has_wood && at == home {
                        let first = *started.get_or_insert(now);
                        if now >= first + ALIGN_TIME {
                            self.leaked = false;
                            self.wear = 0;
                            self.last_leak = None;
                            self.job = None;
                            world.record_legacy_completion(
                                "H08-02",
                                LegacyActor::Resident(WORKER as u8),
                                home,
                                home,
                                LegacyPlace::Nearby,
                                1,
                                None,
                            );
                        } else {
                            self.job = Some(Job::Repair {
                                cupboard,
                                due,
                                has_wood,
                                started,
                            });
                        }
                    } else if !has_wood && (now >= due || at == cupboard) {
                        if now < due
                            && Self::wood_for_repair(world)
                            && world.spend_stock(Item::Wood, 1)
                        {
                            self.job = Some(Job::Repair {
                                cupboard,
                                due,
                                has_wood: true,
                                started: None,
                            });
                            life.people[WORKER]
                                .walk
                                .redirect(home, super::State::Outbound, now);
                        } else {
                            self.job = None;
                        }
                    }
                }
            }
            return true;
        }
        if !allow_start
            || at != home
            || life.occupied(world, home, Some(WORKER), None)
            || !world.village_building_active(house(WORKER).id)
            || person.job.is_some()
            || person.escrow.is_some()
            || person.service_material.is_some()
            || person.returning_cargo.is_some()
            || life.services.carried_asset(WORKER as u8).is_some()
        {
            return false;
        }
        if !self.aligned && world.weather() == YardWeather::Rain {
            self.job = Some(Job::Align {
                ready: now + ALIGN_TIME,
            });
            life.people[WORKER]
                .walk
                .redirect(home, super::State::Outbound, now);
            return true;
        }
        if self.leaked
            && Self::wood_for_repair(world)
            && let Some(cupboard) = slot(world.phase_mask(), "pantry_loading", "load")
            && !life.occupied(world, cupboard, Some(WORKER), None)
            && !life.social.is_destination_reserved(cupboard)
        {
            self.job = Some(Job::Repair {
                cupboard,
                due: now + 3 * HOUR,
                has_wood: false,
                started: None,
            });
            life.people[WORKER]
                .walk
                .redirect(cupboard, super::State::Outbound, now);
            return true;
        }
        let Some((plot, site)) = world.village_barrel_watering_site() else {
            return false;
        };
        if self.water == 0
            || world.weather() == YardWeather::Rain
            || life.occupied(world, site, Some(WORKER), None)
            || life.social.is_destination_reserved(site)
        {
            return false;
        }
        self.water -= 1;
        self.job = Some(Job::Water {
            plot,
            at: site,
            due: now + 3 * HOUR,
            started: None,
            returning: false,
        });
        life.people[WORKER]
            .walk
            .redirect(site, super::State::Outbound, now);
        true
    }

    fn wood_for_repair(world: &LivingWorld) -> bool {
        let essential =
            world
                .next_construction_material_requirement()
                .map_or(
                    0,
                    |(item, count)| if item == Item::Wood { count } else { 0 },
                );
        world.stock().count(Item::Wood) > essential.max(8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expansion_actual_leak_takes_water_and_the_same_paid_board_seals_it() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.test_market_visual_stage(24);
        assert!(world.exchange_stock_atomic(&[], &[(Item::Wood, 10)]));
        let now = world.world_ms();
        let mut life = VillageLife::default();
        let home = roof();
        life.people[WORKER].walk = Walker::new(home, super::super::State::AtWork);
        life.rain = State {
            aligned: true,
            water: 3,
            last_collection: Some(now / (3 * HOUR)),
            wear: 4,
            ..State::default()
        };
        life.rain.rain_fill(&world, now);
        life.rain.rain_fill(&world, now + MINUTE);
        assert_eq!(life.rain.visual(), Some((home, 2, true)));
        let wood = world.stock().count(Item::Wood);
        let storage = slot(world.phase_mask(), "pantry_loading", "load").unwrap();
        assert!(life.schedule_rain(&mut world, now, true));
        assert_eq!(life.people[WORKER].walk.target, storage);
        assert_eq!(world.stock().count(Item::Wood), wood);
        life.people[WORKER].walk = Walker::new(storage, super::super::State::AtWork);
        life.schedule_rain(&mut world, now, false);
        assert_eq!(world.stock().count(Item::Wood), wood - 1);
        assert_eq!(life.rain.held(WORKER), Some(Item::Wood));
        let mut life: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(life.rain.valid(&life.people[WORKER], now));
        life.people[WORKER].walk = Walker::new(home, super::super::State::AtWork);
        life.schedule_rain(&mut world, now, false);
        life.schedule_rain(&mut world, now + ALIGN_TIME, false);
        assert_eq!(life.rain.visual(), Some((home, 2, false)));
        assert_eq!(life.rain.held(WORKER), None);
        assert_eq!(world.stock().count(Item::Wood), wood - 1);
    }
}
