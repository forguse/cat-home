use super::*;
use crate::legacy_content::{LegacyActor, LegacyPlace};

const COOK: usize = 3;
const IRRIGATOR: usize = 16;
const ORCHARD_KEEPER: usize = 6;
const SAUCER_FIELD: TilePos = TilePos::new(86, 44);
const SAUCER_COOK: TilePos = TilePos::new(85, 44);
const SAUCER_GUEST: TilePos = TilePos::new(87, 44);
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    covered: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    saucer: bool,
}

impl GuestBooking {
    fn brings_tray(self) -> bool {
        self.covered || self.saucer
    }

    fn stands(self, cook: TilePos) -> bool {
        if self.saucer {
            !self.covered && cook == SAUCER_COOK && self.place == SAUCER_GUEST
        } else {
            distance(cook, self.place) == 1
        }
    }

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
    CatDinner,
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    served: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plate: Option<SaucerStage>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum SaucerStage {
    Set { since: u64 },
    Eating { until: u64 },
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mulch {
    tree: u8,
    site: TilePos,
    deadline: u64,
    started: Option<u64>,
    returning: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Survey {
    plot: u8,
    site: TilePos,
    deadline: u64,
    started: Option<u64>,
    returning: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    last_cat_dinner: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mulch: Option<Mulch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    survey: Option<Survey>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    last_tray_in_shelter: bool,
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
            && self.last_cat_dinner.is_none()
            && self.mulch.is_none()
            && self.survey.is_none()
            && !self.last_tray_in_shelter
    }

    pub(super) fn valid(&self, life: &VillageLife, now: u64) -> bool {
        let is_kitchen =
            |p| resolve(PhaseMask::ALL, "home_kitchen.door").is_some_and(|k| distance(k, p) <= 3);
        let recent = |due| due <= now.saturating_add(3 * HOUR);
        self.scraps.is_none_or(is_kitchen)
            && self.last_gift.is_none_or(|day| day <= now / DAY_MS)
            && self.last_cat_dinner.is_none_or(|day| day <= now / DAY_MS)
            && self.dish.is_none_or(|order| {
                is_kitchen(order.origin)
                    && order.target.index().is_some()
                    && recent(order.deadline)
                    && order
                        .work_at
                        .is_none_or(|start| start <= now && start <= order.deadline)
                    && order.plate.is_none_or(|plate| {
                        !order.returning
                            && life.people[COOK].walk.at == SAUCER_COOK
                            && matches!(order.load, Load::Meal(key) if key.saucer)
                            && match plate {
                                SaucerStage::Set { since } => !order.served && since <= now,
                                SaucerStage::Eating { until } => {
                                    order.served && until <= now + 2 * MINUTE
                                }
                            }
                    })
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
                                && who.stands(order.target)
                                && (order.served
                                    || life.people[COOK].count(Item::Food) < HARD_STOCK_CAP)
                                && (order.served != life.people[COOK].guest_food_reserved)
                                && (!order.served
                                    || who.brings_tray()
                                        && (order.returning || order.plate.is_some()))
                                && (!who.brings_tray()
                                    || life.services.carried_asset(COOK as u8) == Some("tray"))
                                && (!who.covered
                                    || covered_stands(PhaseMask::ALL)
                                        == Some((who.place, order.target)))
                        }
                        Load::CatDinner => {
                            resolve(PhaseMask::ALL, "cat_water") == Some(order.target)
                                && life.people[COOK].guest_food_reserved
                                && life.people[COOK].count(Item::Food) < HARD_STOCK_CAP
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
            && self.mulch.is_none_or(|work| {
                (8..=11).contains(&work.tree)
                    && work.site.index().is_some()
                    && recent(work.deadline)
                    && work
                        .started
                        .is_none_or(|time| time <= now && time <= work.deadline)
                    && life.people[ORCHARD_KEEPER].walk.target
                        == if work.returning {
                            house(ORCHARD_KEEPER).door
                        } else {
                            work.site
                        }
            })
            && self.survey.is_none_or(|work| {
                self.mulch.is_none()
                    && (29..=31).contains(&work.plot)
                    && work.site.index().is_some()
                    && recent(work.deadline)
                    && work
                        .started
                        .is_none_or(|time| time <= now && time <= work.deadline)
                    && life.people[ORCHARD_KEEPER].walk.target
                        == if work.returning {
                            house(ORCHARD_KEEPER).door
                        } else {
                            work.site
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
            ORCHARD_KEEPER => self.mulch.is_some() || self.survey.is_some(),
            _ => false,
        }
    }

    pub(super) fn carries_gift(&self) -> bool {
        self.dish.is_some_and(|order| {
            !order.served && matches!(order.load, Load::Meal(_) | Load::CatDinner)
        })
    }

    pub(super) fn carries_tray(&self, id: u8) -> bool {
        id == COOK as u8
            && self
                .dish
                .is_some_and(|order| matches!(order.load, Load::Meal(key) if key.brings_tray()))
    }

    pub(super) fn saucer(&self) -> Option<(TilePos, bool)> {
        let order = self.dish?;
        order.plate.map(|_| (SAUCER_FIELD, !order.served))
    }

    pub(super) fn has_placed_saucer(&self, id: usize) -> bool {
        id == COOK && self.saucer().is_some()
    }

    pub(super) fn work_frame(&self, resident: usize, now: u64) -> Option<usize> {
        let started = match resident {
            COOK => self.dish.filter(|order| !order.returning)?.work_at?,
            IRRIGATOR => self.drain.filter(|order| !order.returning)?.work_at?,
            ORCHARD_KEEPER => self
                .survey
                .filter(|order| !order.returning)
                .and_then(|order| order.started)
                .or_else(|| {
                    self.mulch
                        .filter(|order| !order.returning)
                        .and_then(|order| order.started)
                })?,
            _ => return None,
        };
        Some((now.saturating_sub(started) / (2 * MINUTE)).min(2) as usize)
    }

    pub(super) fn held(&self, resident: usize) -> Option<VisitorHeld> {
        match resident {
            COOK => {
                let dish = self.dish?;
                if dish.plate.is_some() {
                    return None;
                }
                Some(VisitorHeld::Parcel(match dish.load {
                    Load::Scraps => "kitchen_scraps",
                    Load::Meal(key) if key.saucer && dish.served => "empty_saucer",
                    Load::Meal(key) if key.saucer => "saucer_meal",
                    Load::Meal(key) if key.covered && dish.served => "empty_snack_tray",
                    Load::Meal(key) if key.covered => "covered_snack_tray",
                    Load::Meal(_) => "guest_food_basket",
                    Load::CatDinner => "warm_cat_supper",
                }))
            }
            IRRIGATOR if self.drain.is_some() => Some(VisitorHeld::Tool(VisitorTool::Hoe)),
            ORCHARD_KEEPER if self.active(ORCHARD_KEEPER) => {
                Some(VisitorHeld::Tool(VisitorTool::Hoe))
            }
            _ => None,
        }
    }

    pub(super) fn name(&self, resident: usize) -> Option<&'static str> {
        match resident {
            COOK => Some(match self.dish?.load {
                Load::Scraps => "运厨房菜皮到堆肥桶",
                Load::Meal(key) if key.saucer => "把点心碗架到泥地托碟上",
                Load::Meal(_) => "配送来客礼篮",
                Load::CatDinner => "给橘子送暖晚饭",
            }),
            IRRIGATOR if self.drain.is_some() => Some("清开积水田浅槽"),
            ORCHARD_KEEPER if self.survey.is_some() => Some("打开新畦剖面观察土层"),
            ORCHARD_KEEPER if self.mulch.is_some() => Some("到真实果树根旁铺落叶"),
            _ => None,
        }
    }

    pub(super) fn positions(&self) -> impl Iterator<Item = TilePos> + '_ {
        self.scraps
            .into_iter()
            .chain(self.dish.iter().flat_map(|task| [task.origin, task.target]))
            .chain(
                self.dish
                    .filter(|order| matches!(order.load, Load::Meal(key) if key.saucer))
                    .map(|_| SAUCER_FIELD),
            )
            .chain(self.drain.iter().map(|task| task.site))
            .chain(self.mulch.iter().map(|task| task.site))
            .chain(self.survey.iter().map(|task| task.site))
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
            ORCHARD_KEEPER => {
                self.survey_soil(life, world, now, allow_start)
                    || self.mulch_orchard(life, world, now, allow_start)
            }
            _ => false,
        }
    }

    pub(super) fn soil_survey_plot(&self) -> Option<u8> {
        self.survey
            .filter(|survey| !survey.returning)
            .map(|survey| survey.plot)
    }

    pub(super) fn soil_window(&self, now: u64) -> Option<u8> {
        let survey = self.survey.filter(|order| !order.returning)?;
        ((2 * MINUTE..7 * MINUTE).contains(&now.saturating_sub(survey.started?)))
            .then_some(survey.plot)
    }

    fn survey_soil(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        now: u64,
        allow_start: bool,
    ) -> bool {
        let home = house(ORCHARD_KEEPER).door;
        let gardener = &life.people[ORCHARD_KEEPER];
        let at = gardener.walk.at;
        if let Some(mut survey) = self.survey {
            if survey.returning {
                if at == home {
                    self.survey = None;
                }
                return true;
            }
            if now < survey.deadline && world.survey_work_still_possible(survey.plot, survey.site) {
                if at != survey.site {
                    return true;
                }
                let started = *survey.started.get_or_insert(now);
                self.survey = Some(survey);
                if now < started.saturating_add(8 * MINUTE) {
                    return true;
                }
                if world.record_soil_profile(survey.plot, at) {
                    let (target, _) = world
                        .village_subsoil(survey.plot)
                        .expect("surveyed a real field");
                    courier_witness(
                        world,
                        "H02-01",
                        ORCHARD_KEEPER,
                        at,
                        target,
                        LegacyPlace::Expanded,
                    );
                }
            }
            survey.returning = true;
            self.survey = Some(survey);
            life.people[ORCHARD_KEEPER]
                .walk
                .redirect(home, super::State::Outbound, now);
            return true;
        }
        if !allow_start
            || self.mulch.is_some()
            || at != home
            || !world.town_allows(TownEra::MarketStreet)
            || life.occupied(world, home, Some(ORCHARD_KEEPER), None)
            || gardener.job.is_some()
            || gardener.counter.is_some()
            || gardener.escrow.is_some()
            || gardener.produce_in_transit.is_some()
            || gardener.service_material.is_some()
            || gardener.returning_cargo.is_some()
            || life.services.carried_asset(ORCHARD_KEEPER as u8).is_some()
        {
            return false;
        }
        let Some((plot, site, _)) = world.village_unexamined_soil() else {
            return false;
        };
        if life.occupied(world, site, Some(ORCHARD_KEEPER), None)
            || life.social.is_destination_reserved(site)
        {
            return false;
        }
        self.survey = Some(Survey {
            plot,
            site,
            deadline: now + 3 * HOUR,
            started: None,
            returning: false,
        });
        life.people[ORCHARD_KEEPER].tool = None;
        life.people[ORCHARD_KEEPER]
            .walk
            .redirect(site, super::State::Outbound, now);
        true
    }

    fn mulch_orchard(
        &mut self,
        life: &mut VillageLife,
        world: &mut LivingWorld,
        now: u64,
        allow_start: bool,
    ) -> bool {
        let home = house(ORCHARD_KEEPER).door;
        let person = &life.people[ORCHARD_KEEPER];
        let at = person.walk.at;
        if let Some(mut work) = self.mulch {
            if work.returning {
                if at == home {
                    self.mulch = None;
                }
                return true;
            }
            if now < work.deadline {
                if at != work.site {
                    return true;
                }
                let started = *work.started.get_or_insert(now);
                self.mulch = Some(work);
                if now < started + 6 * MINUTE {
                    return true;
                }
                if world.place_village_leaf_mulch(work.tree, at) {
                    let tree = world.trees()[usize::from(work.tree)].position;
                    let target = TilePos::new(
                        tree.x as u16 / TILE_PX as u16,
                        tree.y as u16 / TILE_PX as u16,
                    );
                    courier_witness(
                        world,
                        "H02-08",
                        ORCHARD_KEEPER,
                        at,
                        target,
                        LegacyPlace::Expanded,
                    );
                }
            }
            work.returning = true;
            self.mulch = Some(work);
            life.people[ORCHARD_KEEPER]
                .walk
                .redirect(home, super::State::Outbound, now);
            return true;
        }
        if !allow_start
            || at != home
            || !world.town_allows(TownEra::MarketStreet)
            || life.occupied(world, home, Some(ORCHARD_KEEPER), None)
            || person.job.is_some()
            || person.counter.is_some()
            || person.escrow.is_some()
            || person.produce_in_transit.is_some()
            || person.service_material.is_some()
            || person.returning_cargo.is_some()
            || life.services.carried_asset(ORCHARD_KEEPER as u8).is_some()
        {
            return false;
        }
        let Some((tree, site)) = world.village_leaf_mulch_site() else {
            return false;
        };
        if life.occupied(world, site, Some(ORCHARD_KEEPER), None)
            || life.social.is_destination_reserved(site)
        {
            return false;
        }
        self.mulch = Some(Mulch {
            tree,
            site,
            deadline: now + 3 * HOUR,
            started: None,
            returning: false,
        });
        life.people[ORCHARD_KEEPER].tool = None;
        life.people[ORCHARD_KEEPER]
            .walk
            .redirect(site, super::State::Outbound, now);
        true
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
                    .filter(|g| key.matches(g, now) && key.stands(order.target))
            };
            if let Some(SaucerStage::Eating { until }) = order.plate {
                if at == order.target && now >= until {
                    courier_witness(world, "H06-08", COOK, at, SAUCER_FIELD, LegacyPlace::Nearby);
                    self.send_dish_back(life, order, now);
                }
                return true;
            }
            if !order.returning
                && (now >= order.deadline
                    || matches!(order.load, Load::Meal(key) if valid_guest(key).is_none()))
            {
                self.send_dish_back(life, order, now);
                return true;
            }
            if order.returning {
                if at == order.origin {
                    match order.load {
                        Load::Scraps => self.scraps = Some(order.origin),
                        Load::Meal(key) if key.brings_tray() => {
                            assert!(life.services.return_asset(
                                services::SNACK_TRAY,
                                COOK as u8,
                                at
                            ));
                            if !order.served {
                                life.people[COOK].guest_food_reserved = false;
                                assert!(life.people[COOK].incoming(PACKAGE));
                            } else {
                                self.last_tray_in_shelter = key.covered;
                            }
                        }
                        Load::Meal(_) | Load::CatDinner => {
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
                    if key.saucer {
                        match order.plate {
                            None => {
                                if !world.village_mud_at(SAUCER_FIELD) {
                                    self.send_dish_back(life, order, now);
                                } else if !life.occupied(world, SAUCER_FIELD, None, None) {
                                    order.plate = Some(SaucerStage::Set { since: now });
                                    self.dish = Some(order);
                                }
                                return true;
                            }
                            Some(SaucerStage::Set { since }) if now < since + MINUTE => {
                                return true;
                            }
                            Some(SaucerStage::Set { .. }) => {}
                            Some(SaucerStage::Eating { .. }) => {
                                unreachable!("handled the eating lease before a new portion")
                            }
                        }
                    }
                    let Some(g) = valid_guest(key) else {
                        return true;
                    };
                    let target = g.walk.at;
                    let g = life.guests[usize::from(key.slot)].as_mut().unwrap();
                    g.gifted = true;
                    g.eating_until = Some(now + 2 * MINUTE);
                    life.people[COOK].guest_food_reserved = false;
                    if key.saucer {
                        order.served = true;
                        order.plate = Some(SaucerStage::Eating {
                            until: now + 2 * MINUTE,
                        });
                        self.dish = Some(order);
                    } else if key.covered {
                        courier_witness(world, "H06-03", COOK, at, target, LegacyPlace::Nearby);
                        order.served = true;
                        self.send_dish_back(life, order, now);
                    } else {
                        courier_witness(world, "H06-07", COOK, at, target, LegacyPlace::Nearby);
                        self.dish = None;
                    }
                }
                Load::CatDinner => {
                    if world.village_receive_supper_for_cat(at) {
                        life.people[COOK].guest_food_reserved = false;
                        courier_witness(world, "H06-01", COOK, at, at, LegacyPlace::Front);
                        self.dish = None;
                    } else {
                        order.returning = true;
                        self.dish = Some(order);
                        life.people[COOK]
                            .walk
                            .redirect(order.origin, super::State::Outbound, now);
                    }
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
        let day = now / DAY_MS;
        let minute = now % DAY_MS / MINUTE;
        if (1020..1100).contains(&minute)
            && world.town_allows(TownEra::Market)
            && life.people[COOK].produced_day == Some(day)
            && life.people[COOK].count(Item::Food) >= 3
            && self.last_cat_dinner != Some(day)
            && let Some(target) = world
                .village_cat_supper_site()
                .filter(|&site| site != at && free(site))
        {
            assert!(life.people[COOK].outgoing(PACKAGE));
            self.last_cat_dinner = Some(day);
            life.people[COOK].guest_food_reserved = true;
            self.dish = Some(Delivery {
                origin: at,
                target,
                load: Load::CatDinner,
                deadline: now + HOUR,
                returning: false,
                work_at: None,
                served: false,
                plate: None,
            });
            life.people[COOK]
                .walk
                .redirect(target, super::State::Outbound, now);
            return true;
        }
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
                served: false,
                plate: None,
            });
            life.people[COOK]
                .walk
                .redirect(target, super::State::Outbound, now);
            return true;
        }
        if self.last_gift == Some(day)
            || day % 7 == 5
            || !world.town_allows(TownEra::Market)
            || life.people[COOK].count(Item::Food) < 4
            || world.weather() == YardWeather::Snow
        {
            return false;
        }
        let tray_available = life.services.holds(services::SNACK_TRAY, COOK as u8)
            && life
                .services
                .stored_at(services::SNACK_TRAY)
                .is_none_or(|source| source == at);
        let rain = world.weather() == YardWeather::Rain;
        let covered = (rain && tray_available)
            .then(|| covered_stands(mask))
            .flatten()
            .filter(|&(guest_stand, cook_stand)| free(guest_stand) && free(cook_stand));
        let muddy_courtyard = rain
            && now % (3 * HOUR) < 150 * MINUTE
            && tray_available
            && (self.last_tray_in_shelter || covered.is_none())
            && phase_active(mask, "weekly_market")
            && free(SAUCER_FIELD)
            && free(SAUCER_COOK)
            && free(SAUCER_GUEST)
            && world.village_mud_at(SAUCER_FIELD);
        let Some((mut key, target)) = life.guests.iter().enumerate().find_map(|(slot, guest)| {
            let g = guest.as_ref()?;
            let place = match g.walk.state {
                super::State::Resting => g.walk.at,
                super::State::Visiting if (72..=118).contains(&g.walk.target.x) => g.walk.target,
                _ => return None,
            };
            let use_saucer = muddy_courtyard && distance(place, SAUCER_GUEST) <= 14;
            let protected = if use_saucer {
                Some((SAUCER_GUEST, SAUCER_COOK))
            } else {
                covered
            };
            if rain && protected.is_none() {
                return None;
            }
            let key = GuestBooking {
                slot: slot as u8,
                identity: g.identity,
                leave_by: g.leave_by,
                place,
                covered: !use_saucer && covered.is_some(),
                saucer: use_saucer,
            };
            let meeting = protected.map_or(place, |(guest_stand, _)| guest_stand);
            let flight = u64::from(distance(at, meeting)) * 65_000 + 20 * MINUTE;
            if !key.matches(g, now)
                || g.cargo.is_some()
                || g.leave_by < now + flight
                || flight >= 3 * HOUR
            {
                return None;
            }
            if let Some((guest_stand, cook_stand)) = protected {
                return (distance(place, guest_stand) <= 16).then_some((key, cook_stand));
            }
            let target = [(-1, 0), (1, 0), (0, 1), (0, -1)]
                .into_iter()
                .filter_map(|(dx, dy)| place.offset(dx, dy))
                .find(|&p| free(p))?;
            Some((key, target))
        }) else {
            return false;
        };
        if key.brings_tray() {
            let guest_stand = if key.saucer {
                SAUCER_GUEST
            } else {
                covered.unwrap().0
            };
            if !life
                .services
                .assign_carrier(services::SNACK_TRAY, COOK as u8, COOK as u8, at)
            {
                return false;
            }
            key.place = guest_stand;
            life.guests[usize::from(key.slot)]
                .as_mut()
                .unwrap()
                .walk
                .redirect(guest_stand, super::State::Visiting, now);
        }
        assert!(life.people[COOK].outgoing(PACKAGE));
        life.people[COOK].guest_food_reserved = true;
        self.last_gift = Some(day);
        self.dish = Some(Delivery {
            origin: at,
            target,
            load: Load::Meal(key),
            deadline: key.leave_by.min(now + 3 * HOUR),
            returning: false,
            work_at: None,
            served: false,
            plate: None,
        });
        life.people[COOK]
            .walk
            .redirect(target, super::State::Outbound, now);
        true
    }

    fn send_dish_back(&mut self, life: &mut VillageLife, mut order: Delivery, now: u64) {
        if matches!(order.load, Load::Meal(key) if key.brings_tray()) {
            life.services.request_return(COOK as u8);
        }
        // A ground dish can only have been set under this stationary cook.
        debug_assert!(order.plate.is_none() || life.people[COOK].walk.at == order.target);
        order.plate = None;
        order.returning = true;
        life.people[COOK]
            .walk
            .redirect(order.origin, super::State::Outbound, now);
        self.dish = Some(order);
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

fn covered_stands(mask: PhaseMask) -> Option<(TilePos, TilePos)> {
    let guest = resolve(mask, "stall_seed.door")?;
    let cook = slot(mask, "stall_seed.door", "tool")?;
    (distance(guest, cook) == 1).then_some((guest, cook))
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
    fn expansion_cat_supper_cooks_and_delivers_one_owned_portion() {
        let mut world = local_work_world();
        for _ in 0..14 {
            let now = world.world_ms();
            let same_day = now / DAY_MS * DAY_MS + 17 * HOUR;
            let dusk = if same_day > now {
                same_day
            } else {
                same_day + DAY_MS
            };
            world.queue_world_time(dusk - now);
            for _ in 0..1024 {
                if world.advance_pending_to(dusk) && world.world_ms() == dusk {
                    break;
                }
            }
            assert_eq!(world.world_ms(), dusk);
            if world.village_cat_supper_site().is_some() {
                break;
            }
        }
        let end = world.world_ms();
        let mask = world.phase_mask();
        let kitchen = resolve(mask, "home_kitchen.door").unwrap();
        let dish = world
            .village_cat_supper_site()
            .expect("empty reachable supper plate");
        let mut life = VillageLife::default();
        life.people[COOK].walk = Walker::new(kitchen, super::super::State::AtWork);
        life.people[COOK].inventory[item_index(Item::Food)] = 3;
        assert!(!life.schedule_care(&mut world, COOK, mask, end, true));
        assert!(life.people[COOK].incoming(cargo(Item::Crops, 2)));
        let crops = life.people[COOK].count(Item::Crops);
        assert!(life.convert(COOK, cargo(Item::Crops, 2), cargo(Item::Food, 2)));
        life.people[COOK].produced_day = Some(end / DAY_MS);
        assert!(life.schedule_care(&mut world, COOK, mask, end, true));
        assert_eq!(life.people[COOK].count(Item::Crops), crops - 2);
        assert_eq!(life.people[COOK].count(Item::Food), 4);
        assert_eq!(life.people[COOK].walk.target, dish);
        assert_eq!(
            life.care.held(COOK),
            Some(VisitorHeld::Parcel("warm_cat_supper"))
        );
        assert!(!world.cat_food_served());
        let mut life: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(life.care.valid(&life, end));
        life.people[COOK].walk = Walker::new(dish, super::super::State::AtWork);
        assert!(life.schedule_care(&mut world, COOK, mask, end + MINUTE, false));
        assert!(!world.cat_food_served());
        assert!(life.schedule_care(&mut world, COOK, mask, end + 5 * MINUTE, false));
        assert!(world.cat_food_served());
        assert_eq!(life.people[COOK].count(Item::Food), 4);
        assert_eq!(life.care.held(COOK), None);
    }

    #[test]
    fn expansion_rain_meal_moves_the_guest_and_returns_the_same_empty_tray() {
        let mut world = local_work_world();
        world.test_market_visual_stage(10);
        for _ in 0..256 {
            if world.weather() == YardWeather::Rain && world.world_ms() / DAY_MS % 7 != 5 {
                break;
            }
            let end = world.world_ms() + HOUR;
            world.queue_world_time(HOUR);
            for _ in 0..1024 {
                if world.advance_pending_to(end) && world.world_ms() == end {
                    break;
                }
            }
        }
        assert_eq!(world.weather(), YardWeather::Rain);
        let mask = world.phase_mask();
        let (guest_at, cook_at) = covered_stands(mask).unwrap();
        let origin = resolve(mask, "home_kitchen.door").unwrap();
        let now = world.world_ms();
        let mut life = VillageLife::default();
        life.people[COOK].walk = Walker::new(origin, super::super::State::AtWork);
        life.people[COOK].inventory[item_index(Item::Food)] = 4;
        let before_place = guest_at.offset(-2, 3).unwrap();
        let mut arriving = Walker::new(TilePos::new(163, 50), super::super::State::Visiting);
        arriving.target = before_place;
        life.guests[0] = Some(Guest {
            identity: 8,
            walk: arriving,
            leave_by: now + 3 * HOUR,
            cargo: None,
            bed: None,
            wake: None,
            received: true,
            gifted: false,
            eating_until: None,
        });
        assert!(life.schedule_care(&mut world, COOK, mask, now, true));
        assert_eq!(life.guests[0].as_ref().unwrap().walk.target, guest_at);
        assert_eq!(life.people[COOK].walk.target, cook_at);
        assert_eq!(life.people[COOK].count(Item::Food), 3);
        assert_eq!(life.services.carried_asset(COOK as u8), Some("tray"));
        let mut life: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(life.care.valid(&life, now));
        life.people[COOK].walk = Walker::new(cook_at, super::super::State::AtWork);
        life.schedule_care(&mut world, COOK, mask, now + 5 * MINUTE, false);
        assert!(!life.guests[0].as_ref().unwrap().gifted);
        life.guests[0].as_mut().unwrap().walk = Walker::new(guest_at, super::super::State::Resting);
        life.schedule_care(&mut world, COOK, mask, now + 6 * MINUTE, false);
        life.schedule_care(&mut world, COOK, mask, now + 10 * MINUTE, false);
        assert!(life.guests[0].as_ref().unwrap().gifted);
        assert_eq!(
            life.care.held(COOK),
            Some(VisitorHeld::Parcel("empty_snack_tray"))
        );
        assert_eq!(life.people[COOK].walk.target, origin);
        life.people[COOK].walk = Walker::new(origin, super::super::State::AtWork);
        life.schedule_care(&mut world, COOK, mask, now + 11 * MINUTE, false);
        assert!(life.services.holds(services::SNACK_TRAY, COOK as u8));
        assert_eq!(life.services.stored_at(services::SNACK_TRAY), Some(origin));
        assert_eq!(life.people[COOK].count(Item::Food), 3);
    }

    #[test]
    fn expansion_guest_saucer_sits_on_real_mud_through_the_meal_and_returns() {
        let mut world = local_work_world();
        world.test_market_visual_stage(10);
        for _ in 0..512 {
            if world.village_mud_at(SAUCER_FIELD)
                && world.weather() == YardWeather::Rain
                && world.world_ms() % (3 * HOUR) < 150 * MINUTE
                && world.world_ms() / DAY_MS % 7 != 5
            {
                break;
            }
            let end = world.world_ms() + HOUR;
            world.queue_world_time(HOUR);
            for _ in 0..1024 {
                if world.advance_pending_to(end) && world.world_ms() == end {
                    break;
                }
            }
        }
        assert!(world.village_mud_at(SAUCER_FIELD));
        assert!(!world.village_mud_at(resolve(world.phase_mask(), "cat_water").unwrap()));
        let mask = world.phase_mask();
        let origin = resolve(mask, "home_kitchen.door").unwrap();
        let now = world.world_ms();
        let mut life = VillageLife::default();
        life.people[COOK].walk = Walker::new(origin, super::super::State::AtWork);
        life.people[COOK].inventory[item_index(Item::Food)] = 4;
        let before_place = SAUCER_GUEST.offset(0, 3).unwrap();
        life.care.last_tray_in_shelter = true;
        life.guests[0] = Some(Guest {
            identity: 8,
            walk: Walker::new(before_place, super::super::State::Resting),
            leave_by: now + 3 * HOUR,
            cargo: None,
            bed: None,
            wake: None,
            received: true,
            gifted: false,
            eating_until: None,
        });
        assert!(life.schedule_care(&mut world, COOK, mask, now, true));
        assert_eq!(
            life.resident_held(COOK),
            Some(VisitorHeld::Parcel("saucer_meal"))
        );
        assert_eq!(life.people[COOK].count(Item::Food), 3);
        assert_eq!(life.people[COOK].walk.target, SAUCER_COOK);
        assert_eq!(life.guests[0].as_ref().unwrap().walk.target, SAUCER_GUEST);
        assert!(life.village_food_saucer().is_none());
        life.people[COOK].walk = Walker::new(SAUCER_COOK, super::super::State::AtWork);
        life.guests[0].as_mut().unwrap().walk =
            Walker::new(SAUCER_GUEST, super::super::State::Resting);
        life.schedule_care(&mut world, COOK, mask, now + MINUTE, false);
        life.schedule_care(&mut world, COOK, mask, now + 5 * MINUTE, false);
        assert_eq!(life.village_food_saucer(), Some((SAUCER_FIELD, true)));
        assert_eq!(life.resident_held(COOK), None);
        assert!(life.occupied_tiles().contains(&SAUCER_FIELD));
        assert!(!life.guests[0].as_ref().unwrap().gifted);
        let bytes = serde_json::to_vec(&life).unwrap();
        let reopen = || serde_json::from_slice::<VillageLife>(&bytes).unwrap();
        let mut abandoned = reopen();
        assert!(abandoned.care.valid(&abandoned, now + 5 * MINUTE));
        abandoned.guests[0] = None;
        abandoned.schedule_care(&mut world, COOK, mask, now + 6 * MINUTE, false);
        assert!(abandoned.village_food_saucer().is_none());
        assert_eq!(abandoned.people[COOK].count(Item::Food), 3);
        abandoned.people[COOK].walk = Walker::new(origin, super::super::State::AtWork);
        abandoned.schedule_care(&mut world, COOK, mask, now + 7 * MINUTE, false);
        assert!(abandoned.services.holds(services::SNACK_TRAY, COOK as u8));
        assert_eq!(abandoned.people[COOK].count(Item::Food), 4);

        let mut life = reopen();
        life.schedule_care(&mut world, COOK, mask, now + 6 * MINUTE, false);
        assert_eq!(life.village_food_saucer(), Some((SAUCER_FIELD, false)));
        assert_eq!(
            life.guests[0].as_ref().unwrap().eating_until,
            Some(now + 8 * MINUTE)
        );
        life.schedule_care(&mut world, COOK, mask, now + 7 * MINUTE, false);
        assert!(life.village_food_saucer().is_some());
        life.schedule_care(&mut world, COOK, mask, now + 8 * MINUTE, false);
        assert!(life.village_food_saucer().is_none());
        assert_eq!(
            life.resident_held(COOK),
            Some(VisitorHeld::Parcel("empty_saucer"))
        );
        life.people[COOK].walk = Walker::new(origin, super::super::State::AtWork);
        life.schedule_care(&mut world, COOK, mask, now + 9 * MINUTE, false);
        assert!(life.services.holds(services::SNACK_TRAY, COOK as u8));
        assert_eq!(life.people[COOK].count(Item::Food), 3);
        assert_eq!(life.services.stored_at(services::SNACK_TRAY), Some(origin));
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
                    covered: false,
                    saucer: false,
                }),
                deadline: now + HOUR,
                returning: false,
                work_at: None,
                served: false,
                plate: None,
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
