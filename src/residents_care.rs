use super::*;
use crate::legacy_content::{LegacyActor, LegacyPlace};

const COOK: usize = 3;
const IRRIGATOR: usize = 16;
const PACKAGE: Cargo = Cargo {
    item: Item::Food,
    quantity: 1,
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct GuestBooking {
    slot: u8,
    identity: u8,
    leave_by: u64,
    place: TilePos,
}

impl GuestBooking {
    fn matches(self, guest: &Guest, now: u64) -> bool {
        guest.identity == self.identity
            && guest.leave_by == self.leave_by
            && guest.leave_by >= now.saturating_add(2 * MINUTE)
            && match guest.walk.state {
                super::State::Resting => guest.walk.at == self.place && guest.walk.edge.is_none(),
                super::State::Visiting => guest.walk.target == self.place,
                _ => false,
            }
            && guest.cargo.is_none()
            && !guest.gifted
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Load {
    Scraps,
    Meal(GuestBooking),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Delivery {
    origin: TilePos,
    target: TilePos,
    load: Load,
    deadline: u64,
    returning: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    work_at: Option<u64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Drain {
    plot: u8,
    site: TilePos,
    deadline: u64,
    returning: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    work_at: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    #[serde(skip_serializing_if = "Option::is_none")]
    scraps: Option<TilePos>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dish: Option<Delivery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    drain: Option<Drain>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_gift: Option<u64>,
}

fn courier_witness(
    world: &mut LivingWorld,
    base: &'static str,
    id: usize,
    at: TilePos,
    target: TilePos,
    place: LegacyPlace,
) {
    world.record_legacy_completion(
        base,
        LegacyActor::Resident(id as u8),
        at,
        target,
        place,
        1,
        None,
    );
}

impl State {
    pub(super) fn empty(&self) -> bool {
        self.scraps.is_none()
            && self.dish.is_none()
            && self.drain.is_none()
            && self.last_gift.is_none()
    }

    pub(super) fn valid(&self, life: &VillageLife, now: u64) -> bool {
        let is_kitchen =
            |p| resolve(PhaseMask::ALL, "home_kitchen.door").is_some_and(|k| distance(k, p) <= 3);
        let recent = |due| due <= now.saturating_add(3 * HOUR);
        self.scraps.is_none_or(is_kitchen)
            && self.last_gift.is_none_or(|day| day <= now / DAY_MS)
            && self.dish.is_none_or(|order| {
                is_kitchen(order.origin)
                    && order.target.index().is_some()
                    && recent(order.deadline)
                    && order
                        .work_at
                        .is_none_or(|start| start <= now && start <= order.deadline)
                    && life.people[COOK].walk.target
                        == if order.returning {
                            order.origin
                        } else {
                            order.target
                        }
                    && match order.load {
                        Load::Scraps => self.scraps.is_none(),
                        Load::Meal(who) => {
                            usize::from(who.slot) < EXTERNAL_COUNT
                                && usize::from(who.identity) < RESIDENT_COUNT
                                && who.place.index().is_some()
                                && distance(order.target, who.place) == 1
                                && life.people[COOK].count(Item::Food) < HARD_STOCK_CAP
                                && life.people[COOK].guest_food_reserved
                        }
                    }
            })
            && self.drain.is_none_or(|order| {
                order.plot < 32
                    && order.site.index().is_some()
                    && recent(order.deadline)
                    && order
                        .work_at
                        .is_none_or(|start| start <= now && start <= order.deadline)
                    && life.people[IRRIGATOR].walk.target
                        == if order.returning {
                            house(IRRIGATOR).door
                        } else {
                            order.site
                        }
            })
    }

    pub(super) fn note_scraps(&mut self, at: TilePos) {
        if self.scraps.is_none()
            && !self
                .dish
                .is_some_and(|order| matches!(order.load, Load::Scraps))
            && resolve(PhaseMask::ALL, "home_kitchen.door")
                .is_some_and(|door| distance(door, at) <= 3)
        {
            self.scraps = Some(at);
        }
    }

    pub(super) fn active(&self, resident: usize) -> bool {
        match resident {
            COOK => self.dish.is_some(),
            IRRIGATOR => self.drain.is_some(),
            _ => false,
        }
    }

    pub(super) fn carries_gift(&self) -> bool {
        self.dish
            .is_some_and(|order| matches!(order.load, Load::Meal(_)))
    }

    pub(super) fn work_frame(&self, resident: usize, now: u64) -> Option<usize> {
        let started = match resident {
            COOK => self.dish.filter(|order| !order.returning)?.work_at?,
            IRRIGATOR => self.drain.filter(|order| !order.returning)?.work_at?,
            _ => return None,
        };
        Some((now.saturating_sub(started) / (2 * MINUTE)).min(2) as usize)
    }

    pub(super) fn held(&self, resident: usize) -> Option<VisitorHeld> {
        match resident {
            COOK => Some(VisitorHeld::Parcel(match self.dish?.load {
                Load::Scraps => "kitchen_scraps",
                Load::Meal(_) => "guest_food_basket",
            })),
            IRRIGATOR if self.drain.is_some() => Some(VisitorHeld::Tool(VisitorTool::Hoe)),
            _ => None,
        }
    }

    pub(super) fn name(&self, resident: usize) -> Option<&'static str> {
        match resident {
            COOK => Some(match self.dish?.load {
                Load::Scraps => "运厨房菜皮到堆肥桶",
                Load::Meal(_) => "配送来客礼篮",
            }),
            IRRIGATOR if self.drain.is_some() => Some("清开积水田浅槽"),
            _ => None,
        }
    }

    pub(super) fn positions(&self) -> impl Iterator<Item = TilePos> + '_ {
        self.scraps
            .into_iter()
            .chain(self.dish.iter().flat_map(|task| [task.origin, task.target]))
            .chain(self.drain.iter().map(|task| task.site))
    }

    pub(super) fn schedule(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        id: usize,
        mask: PhaseMask,
        now: u64,
        allow_start: bool,
    ) -> bool {
        match id {
            COOK => self.cook(life, world, mask, now, allow_start),
            IRRIGATOR => self.irrigate(life, world, now, allow_start),
            _ => false,
        }
    }

    fn cook(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        mask: PhaseMask,
        now: u64,
        allow_start: bool,
    ) -> bool {
        let at = life.people[COOK].walk.at;
        if let Some(mut order) = self.dish {
            let valid_guest = |key: GuestBooking| {
                life.guests[usize::from(key.slot)]
                    .as_ref()
                    .filter(|g| key.matches(g, now) && distance(order.target, key.place) == 1)
            };
            if !order.returning
                && (now >= order.deadline
                    || matches!(order.load, Load::Meal(key) if valid_guest(key).is_none()))
            {
                order.returning = true;
                life.people[COOK]
                    .walk
                    .redirect(order.origin, super::State::Outbound, now);
                self.dish = Some(order);
            }
            if order.returning {
                if at == order.origin {
                    match order.load {
                        Load::Scraps => self.scraps = Some(order.origin),
                        Load::Meal(_) => {
                            life.people[COOK].guest_food_reserved = false;
                            assert!(
                                life.people[COOK].incoming(PACKAGE),
                                "the booked portion kept its cupboard capacity"
                            );
                        }
                    }
                    self.dish = None;
                }
                return true;
            }
            if at != order.target {
                return true;
            }
            if let Load::Meal(key) = order.load
                && !valid_guest(key).is_some_and(|g| g.walk.state == super::State::Resting)
            {
                return true;
            }
            let started = *order.work_at.get_or_insert(now);
            self.dish = Some(order);
            if now < started + 4 * MINUTE {
                return true;
            }
            match order.load {
                Load::Scraps => {
                    if world.put_kitchen_scrap(at) {
                        courier_witness(world, "H06-05", COOK, at, at, LegacyPlace::Front);
                        self.dish = None;
                    } else {
                        order.returning = true;
                        self.dish = Some(order);
                        life.people[COOK]
                            .walk
                            .redirect(order.origin, super::State::Outbound, now);
                    }
                }
                Load::Meal(key) => {
                    let Some(g) = valid_guest(key) else {
                        return true;
                    };
                    let target = g.walk.at;
                    let g = life.guests[usize::from(key.slot)].as_mut().unwrap();
                    g.gifted = true;
                    g.eating_until = Some(now + 2 * MINUTE);
                    life.people[COOK].guest_food_reserved = false;
                    courier_witness(world, "H06-07", COOK, at, target, LegacyPlace::Nearby);
                    self.dish = None;
                }
            }
            return true;
        }
        if !allow_start
            || life.people[COOK].counter.is_some()
            || life.people[COOK].job.is_some()
            || life.social.autumn_cook_preparing(world, COOK)
            || !resolve(mask, "home_kitchen.door").is_some_and(|door| distance(door, at) <= 3)
        {
            return false;
        }
        let free = |target: TilePos| {
            world.can_walk(target.x.into(), target.y.into())
                && !life.occupied(world, target, Some(COOK), None)
                && !life.social.is_destination_reserved(target)
        };
        if self.scraps == Some(at)
            && world.town_allows(TownEra::Workshop)
            && let Some(target) = world
                .village_compost_drop_tile()
                .filter(|&target| target != at && free(target))
        {
            self.scraps = None;
            self.dish = Some(Delivery {
                origin: at,
                target,
                load: Load::Scraps,
                deadline: now + 2 * HOUR,
                returning: false,
                work_at: None,
            });
            life.people[COOK]
                .walk
                .redirect(target, super::State::Outbound, now);
            return true;
        }
        let day = now / DAY_MS;
        if self.last_gift == Some(day)
            || day % 7 == 5
            || !world.town_allows(TownEra::Market)
            || life.people[COOK].count(Item::Food) < 4
            || matches!(world.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            return false;
        }
        let Some((key, target)) = life.guests.iter().enumerate().find_map(|(slot, guest)| {
            let g = guest.as_ref()?;
            let place = match g.walk.state {
                super::State::Resting => g.walk.at,
                super::State::Visiting if (72..=118).contains(&g.walk.target.x) => g.walk.target,
                _ => return None,
            };
            let key = GuestBooking {
                slot: slot as u8,
                identity: g.identity,
                leave_by: g.leave_by,
                place,
            };
            let flight = u64::from(distance(at, place)) * 65_000 + 20 * MINUTE;
            if !key.matches(g, now)
                || g.cargo.is_some()
                || g.leave_by < now + flight
                || flight >= 3 * HOUR
            {
                return None;
            }
            let target = [(-1, 0), (1, 0), (0, 1), (0, -1)]
                .into_iter()
                .filter_map(|(dx, dy)| place.offset(dx, dy))
                .find(|&p| free(p))?;
            Some((key, target))
        }) else {
            return false;
        };
        if !life.people[COOK].outgoing(PACKAGE) {
            return false;
        }
        life.people[COOK].guest_food_reserved = true;
        self.last_gift = Some(day);
        self.dish = Some(Delivery {
            origin: at,
            target,
            load: Load::Meal(key),
            deadline: key.leave_by.min(now + 3 * HOUR),
            returning: false,
            work_at: None,
        });
        life.people[COOK]
            .walk
            .redirect(target, super::State::Outbound, now);
        true
    }

    fn irrigate(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        now: u64,
        allow_start: bool,
    ) -> bool {
        let home = house(IRRIGATOR).door;
        let at = life.people[IRRIGATOR].walk.at;
        if let Some(mut order) = self.drain {
            if order.returning && at == home {
                self.drain = None;
                return true;
            }
            let needs_drain = world.village_plot_is_flooded(order.plot);
            if !order.returning && needs_drain && at == order.site && now < order.deadline {
                let started = *order.work_at.get_or_insert(now);
                self.drain = Some(order);
                if now < started + 6 * MINUTE {
                    return true;
                }
            }
            if !order.returning && (at == order.site || now >= order.deadline || !needs_drain) {
                if at == order.site
                    && now < order.deadline
                    && world.village_local_work("drain", Some(order.plot))
                {
                    let target = WILLOW_VILLAGE
                        .crops
                        .get(usize::from(order.plot))
                        .map_or(at, |crop| TilePos::new(crop.rect.x, crop.rect.y));
                    courier_witness(
                        world,
                        "H08-04",
                        IRRIGATOR,
                        at,
                        target,
                        LegacyPlace::Expanded,
                    );
                }
                order.returning = true;
                self.drain = Some(order);
                life.people[IRRIGATOR]
                    .walk
                    .redirect(home, super::State::Outbound, now);
            }
            return true;
        }
        if !allow_start
            || at != home
            || life.people[IRRIGATOR].job.is_some()
            || life.occupied(world, home, Some(IRRIGATOR), None)
        {
            return false;
        }
        let Some((plot, site)) = world.village_flooded_worksite() else {
            return false;
        };
        if life.occupied(world, site, Some(IRRIGATOR), None)
            || life.social.is_destination_reserved(site)
        {
            return false;
        }
        self.drain = Some(Drain {
            plot,
            site,
            deadline: now + 3 * HOUR,
            returning: false,
            work_at: None,
        });
        // The worker is at the cupboard and can exchange the usual watering
        // can for their single dedicated trench hoe without carrying both.
        life.people[IRRIGATOR].tool = None;
        life.people[IRRIGATOR]
            .walk
            .redirect(site, super::State::Outbound, now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saw(world: &LivingWorld, id: &str) -> bool {
        world
            .legacy_behaviors()
            .recent()
            .iter()
            .any(|event| event.id().starts_with(id))
    }

    fn local_work_world() -> LivingWorld {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.set_test_town_age(30);
        let mut data = serde_json::to_value(world).unwrap();
        data["village"]["opening"]["boxes_stowed"] = 3.into();
        data["village"]["opening"]["slept_at_home"] = 3.into();
        data["village"]["opening"]["rod_returned"] = true.into();
        data["village"]["opening"]["shared_eaten"] = 3.into();
        serde_json::from_value(data).unwrap()
    }

    #[test]
    fn continuation_scraps_require_real_cooking_and_keep_custody_through_work() {
        let mut world = local_work_world();
        let mask = world.phase_mask();
        let kitchen = resolve(mask, "home_kitchen.door").unwrap();
        let bin = world.village_compost_drop_tile().unwrap();
        let now = world.world_ms();
        let mut life = VillageLife::default();
        life.people[COOK].walk = Walker::new(kitchen, super::super::State::AtWork);
        assert!(!life.schedule_care(&mut world, COOK, mask, now, true));
        let recipe = cargo(Item::Crops, 2);
        assert!(life.people[COOK].incoming(recipe));
        let crops = life.people[COOK].count(Item::Crops);
        assert!(life.convert(COOK, recipe, cargo(Item::Food, 2)));
        assert_eq!(life.people[COOK].count(Item::Crops), crops - 2);
        assert!(life.schedule_care(&mut world, COOK, mask, now, true));
        assert_eq!(
            life.resident_held(COOK),
            Some(VisitorHeld::Parcel("kitchen_scraps"))
        );
        assert!(!saw(&world, "H06-05"));
        life.people[COOK].walk = Walker::new(bin, super::super::State::AtWork);
        assert!(life.schedule_care(&mut world, COOK, mask, now, false));
        assert_eq!(life.care.work_frame(COOK, now + 2 * MINUTE), Some(1));
        let mut life: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(life.care.valid(&life, now));
        assert!(life.schedule_care(&mut world, COOK, mask, now + 4 * MINUTE, false));
        assert!(saw(&world, "H06-05"));
        assert_eq!(life.resident_held(COOK), None);
        assert!(!life.schedule_care(&mut world, COOK, mask, now + 4 * MINUTE, true));
    }

    #[test]
    fn continuation_gift_failed_recipient_preserves_one_reserved_return_slot() {
        let mut world = local_work_world();
        let now = world.world_ms();
        let mask = world.phase_mask();
        let origin = resolve(mask, "home_kitchen.door").unwrap();
        let target = origin.offset(0, 1).unwrap();
        let mut life = VillageLife::default();
        let book = |life: &mut VillageLife| {
            life.people[COOK].walk = Walker::new(origin, super::super::State::AtWork);
            assert!(life.people[COOK].outgoing(PACKAGE));
            life.people[COOK].guest_food_reserved = true;
            life.care.dish = Some(Delivery {
                origin,
                target,
                load: Load::Meal(GuestBooking {
                    slot: 0,
                    identity: 8,
                    leave_by: now + HOUR,
                    place: target.offset(1, 0).unwrap(),
                }),
                deadline: now + HOUR,
                returning: false,
                work_at: None,
            });
            life.people[COOK]
                .walk
                .redirect(target, super::super::State::Outbound, now);
        };
        life.people[COOK].inventory[item_index(Item::Food)] = HARD_STOCK_CAP;
        book(&mut life);
        assert!(!life.people[COOK].incoming(PACKAGE));
        let mut life: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(life.care.valid(&life, now));
        let wrong = target.offset(1, 0).unwrap();
        life.people[COOK].walk = Walker::new(wrong, super::super::State::AtWork);
        life.schedule_care(&mut world, COOK, mask, now, false);
        assert_eq!(life.people[COOK].count(Item::Food), HARD_STOCK_CAP - 1);
        assert_eq!(life.people[COOK].walk.target, origin);
        life.people[COOK].walk = Walker::new(origin, super::super::State::AtWork);
        life.schedule_care(&mut world, COOK, mask, now, false);
        assert_eq!(life.people[COOK].count(Item::Food), HARD_STOCK_CAP);
        assert!(!saw(&world, "H06-07"));
        book(&mut life);
        life.guests[0] = Some(Guest {
            identity: 8,
            walk: Walker::new(wrong, super::super::State::Resting),
            leave_by: now + HOUR,
            cargo: None,
            bed: None,
            wake: None,
            received: true,
            gifted: false,
            eating_until: None,
        });
        life.people[COOK].walk = Walker::new(target, super::super::State::AtWork);
        life.schedule_care(&mut world, COOK, mask, now, false);
        assert!(!life.guests[0].as_ref().unwrap().gifted);
        life.schedule_care(&mut world, COOK, mask, now + 4 * MINUTE, false);
        assert!(life.guests[0].as_ref().unwrap().gifted);
        assert!(saw(&world, "H06-07"));
        assert!(!life.people[COOK].guest_food_reserved);
        assert_eq!(life.people[COOK].count(Item::Food), HARD_STOCK_CAP - 1);
    }

    #[test]
    fn continuation_drain_water_remains_until_actual_six_minute_work_finishes() {
        let mut data = serde_json::to_value(local_work_world()).unwrap();
        data["village"]["flooded_plots"] = 1.into();
        let mut world: LivingWorld = serde_json::from_value(data).unwrap();
        let mask = world.phase_mask();
        let now = world.world_ms();
        let home = house(IRRIGATOR).door;
        let (_, tile) = world.village_flooded_worksite().unwrap();
        let mut life = VillageLife::default();
        life.people[IRRIGATOR].walk = Walker::new(home, super::super::State::Inside);
        assert!(life.schedule_care(&mut world, IRRIGATOR, mask, now, true));
        assert_eq!(
            life.resident_held(IRRIGATOR),
            Some(VisitorHeld::Tool(VisitorTool::Hoe))
        );
        life.people[IRRIGATOR].walk = Walker::new(tile, super::super::State::AtWork);
        life.schedule_care(&mut world, IRRIGATOR, mask, now, false);
        assert!(world.village_flooded_worksite().is_some());
        assert!(!saw(&world, "H08-04"));
        life.schedule_care(&mut world, IRRIGATOR, mask, now + 6 * MINUTE, false);
        assert!(saw(&world, "H08-04"));
        assert!(world.village_flooded_worksite().is_none());
        assert_eq!(life.people[IRRIGATOR].walk.target, home);
        life.people[IRRIGATOR].walk = Walker::new(home, super::super::State::AtWork);
        life.schedule_care(&mut world, IRRIGATOR, mask, now + 7 * MINUTE, false);
        assert_eq!(life.resident_held(IRRIGATOR), None);
    }
}
