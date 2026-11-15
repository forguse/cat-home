use super::*;
use crate::legacy_content::{LegacyActor, LegacyPlace};

const SEED_MERCHANT: usize = 1;
const COURIER: u8 = 0;
const COOK: u8 = 3;
const BOOKING_LIFETIME: u64 = 2 * HOUR;
const MENU_LIMIT: u8 = 2;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    #[serde(skip_serializing_if = "Option::is_none")]
    seed_shift: Option<SeedShift>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mail: Option<MailHold>,
    #[serde(skip_serializing_if = "Option::is_none")]
    menu: Option<Menu>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed_booking: Option<SeedBooking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_host: Option<PostHost>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PostHost {
    actor: ActorKind,
    site: TilePos,
    until: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedBooking {
    shop: u8,
    started: u64,
    buyer: ActorKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedShift {
    shift: u64,
    shop: u8,
    cupboard: u16,
    packed: u16,
    settled: u16,
    returned: u16,
    opened: bool,
}

impl SeedShift {
    fn matches(self, batch: CounterBatch) -> bool {
        self.shop == batch.shop && batch.cargo == shop(usize::from(self.shop)).goods
    }
    fn balanced(self) -> bool {
        self.opened && self.settled + self.returned == self.packed
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MailHold {
    asset: u8,
    source: TilePos,
    taken: u64,
    expired: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Menu {
    day: u64,
    servings: u16,
    booked: u8,
    pending: Option<u64>,
}

fn witness(
    world: &mut LivingWorld,
    behavior: &'static str,
    resident: u8,
    at: TilePos,
    target: TilePos,
    partner: Option<LegacyActor>,
) {
    world.record_legacy_completion(
        behavior,
        LegacyActor::Resident(resident),
        at,
        target,
        LegacyPlace::Nearby,
        1,
        partner,
    );
}

pub(super) fn seed_trade(shop: usize) -> bool {
    matches!(shop, 1 | MOBILE)
}

impl State {
    pub(super) fn is_empty(&self) -> bool {
        self.seed_shift.is_none()
            && self.mail.is_none()
            && self.menu.is_none()
            && self.seed_booking.is_none()
            && self.post_host.is_none()
    }

    pub(super) fn valid(&self, life: &VillageLife, now: u64) -> bool {
        self.post_host.is_none_or(|host| {
            host.actor != ActorKind::Orange
                && slot(PhaseMask::ALL, "home_trade", "customer") == Some(host.site)
                && host.until <= now.saturating_add(36 * MINUTE)
                && life.trip.is_none()
                && life.social.host().is_none()
                && life.people[0]
                    .job
                    .and_then(|job| contracts().nth(job as usize))
                    .is_some_and(|contract| contract.id == "RI001")
                && life.services.carried_asset(0) == Some("letter")
        }) && self.seed_booking.is_none_or(|booking| {
            seed_trade(usize::from(booking.shop))
                && life.trip.is_some_and(|t| {
                    t.shop == booking.shop
                        && t.started == booking.started
                        && t.actor == booking.buyer
                        && !t.accepted
                        && matches!(
                            t.stage,
                            TripStage::Pack | TripStage::Outbound | TripStage::Exchange
                        )
                        && life.people[SEED_MERCHANT].counter_goods(usize::from(booking.shop))
                            == t.incoming.quantity
                })
        }) && self.seed_shift.is_none_or(|shift| {
            let valid_shop = seed_trade(usize::from(shift.shop));
            valid_shop
                && shift.shift <= now / DAY_MS
                && shift.cupboard <= HARD_STOCK_CAP
                && shift.packed >= shop(usize::from(shift.shop)).goods.quantity
                && shift.packed <= HARD_STOCK_CAP
                && u32::from(shift.settled) + u32::from(shift.returned) <= u32::from(shift.packed)
                && life.people[SEED_MERCHANT]
                    .counter
                    .is_none_or(|batch| shift.matches(batch))
        }) && self.mail.is_none_or(|hold| {
            hold.taken <= now
                && life
                    .services
                    .is_custody("booking_slip", hold.asset, 1, COURIER, hold.source)
                && (!hold.expired || now.saturating_sub(hold.taken) >= BOOKING_LIFETIME)
        }) && self.menu.is_none_or(|menu| {
            menu.day <= now / DAY_MS
                && menu.booked <= MENU_LIMIT
                && matches!(menu.servings, 0 | 2)
                && menu.pending.is_none_or(|started| {
                    menu.booked > 0
                        && started / DAY_MS == menu.day
                        && life.trip.is_some_and(|trip| {
                            usize::from(trip.shop) == MEAL_SHOP && trip.started == started
                        })
                })
        })
    }

    pub(super) fn seed_packed(&mut self, now: u64, batch: CounterBatch, cupboard: u16) {
        let shift = now / DAY_MS;
        let same_shift = self
            .seed_shift
            .filter(|old| old.shift == shift && old.shop == batch.shop);
        if let Some(old) = same_shift {
            self.seed_shift = old
                .packed
                .checked_add(batch.cargo.quantity)
                .filter(|&packed| packed <= HARD_STOCK_CAP)
                .map(|packed| SeedShift { packed, ..old });
            return;
        }
        self.seed_shift = Some(SeedShift {
            shift,
            shop: batch.shop,
            cupboard,
            packed: batch.cargo.quantity,
            settled: 0,
            returned: 0,
            opened: false,
        });
    }

    pub(super) fn seed_returned(&mut self, batch: CounterBatch) {
        if let Some(shift) = &mut self.seed_shift
            && shift.matches(batch)
        {
            shift.returned += batch.cargo.quantity;
        }
    }

    pub(super) fn seed_exchanged(
        &mut self,
        world: &mut LivingWorld,
        batch: CounterBatch,
        buyer: ActorKind,
    ) {
        if let Some(shift) = &mut self.seed_shift
            && shift.matches(batch)
        {
            shift.opened = true;
            shift.settled += batch.cargo.quantity;
        }
        let at = booth_slot(usize::from(batch.shop));
        witness(
            world,
            "H10-02",
            SEED_MERCHANT as u8,
            at,
            world.actor_tile(buyer),
            Some(LegacyActor::Household(buyer as u8)),
        );
    }

    pub(super) fn seed_counter(
        &mut self,
        world: &mut LivingWorld,
        seller: &Resident,
        open: bool,
        trip: Option<Trip>,
    ) {
        let Some(shift) = &mut self.seed_shift else {
            return;
        };
        let at = seller.walk.at;
        let shop_id = usize::from(shift.shop);
        if !shift.opened
            && open
            && seller.walk.state == super::State::AtWork
            && at == booth_slot(shop_id)
            && seller.counter.is_some_and(|batch| shift.matches(batch))
        {
            shift.opened = true;
            witness(world, "H10-01", SEED_MERCHANT as u8, at, at, None);
        }
        if !open
            && seller.walk.edge.is_none()
            && at == house(SEED_MERCHANT).door
            && seller.counter.is_none()
            && shift.balanced()
            && !trip.is_some_and(|t| t.shop == shift.shop && !t.accepted)
        {
            self.seed_shift = None;
            witness(world, "H10-10", SEED_MERCHANT as u8, at, at, None);
        }
    }

    pub(super) fn seed_shop(&self) -> Option<usize> {
        self.seed_shift.map(|shift| usize::from(shift.shop))
    }

    pub(super) fn reserve_last_seed(
        &mut self,
        world: &mut LivingWorld,
        trip: Trip,
        seller: &Resident,
    ) {
        let shop = usize::from(trip.shop);
        if self.seed_booking.is_some()
            || !seed_trade(shop)
            || seller.walk.at != booth_slot(shop)
            || seller.walk.edge.is_some()
            || seller.walk.state != super::State::AtWork
            || seller.counter_goods(shop) != trip.incoming.quantity
        {
            return;
        }
        self.seed_booking = Some(SeedBooking {
            shop: trip.shop,
            started: trip.started,
            buyer: trip.actor,
        });
        let at = seller.walk.at;
        witness(world, "H10-07", SEED_MERCHANT as u8, at, at, None);
    }

    pub(super) fn seed_booking_permits(&self, trip: Trip) -> bool {
        self.seed_booking.is_none_or(|booking| {
            booking.shop == trip.shop
                && booking.started == trip.started
                && booking.buyer == trip.actor
        })
    }

    pub(super) fn sync_seed_booking(&mut self, trip: Option<Trip>, seller: &Resident) {
        if self.seed_booking.is_some()
            && !trip.is_some_and(|t| {
                self.seed_booking_permits(t)
                    && !t.accepted
                    && matches!(
                        t.stage,
                        TripStage::Pack | TripStage::Outbound | TripStage::Exchange
                    )
                    && seller.counter_goods(usize::from(t.shop)) == t.incoming.quantity
            })
        {
            self.seed_booking = None;
        }
    }

    pub(super) fn mail_picked_up(&mut self, asset: u8, source: TilePos, now: u64) {
        self.mail = Some(MailHold {
            asset,
            source,
            taken: now,
            expired: false,
        });
    }

    pub(super) fn mail_collected(&self, world: &mut LivingWorld, at: TilePos) {
        witness(world, "H07-01", COURIER, at, at, None);
    }

    pub(super) fn post_host(&self) -> Option<&ActorKind> {
        self.post_host.as_ref().map(|booking| &booking.actor)
    }

    pub(super) fn arrived_post_host(
        &self,
        world: &LivingWorld,
        post: TilePos,
    ) -> Option<ActorKind> {
        let booking = self.post_host?;
        (world.world_ms() < booking.until
            && world.activity_arrived(booking.actor)
            && world.actor_tile(booking.actor) == booking.site
            && distance(post, booking.site) <= 3)
            .then_some(booking.actor)
    }

    pub(super) fn offer_post_host(&mut self, world: &mut LivingWorld, site: TilePos, now: u64) {
        if self.post_host.is_some() {
            return;
        }
        for actor in HOMEBOUND[..2].iter().copied() {
            if distance(world.actor_tile(actor), site) > 32 || !world.reserve_activity(&[actor]) {
                continue;
            }
            if world.move_for_activity(actor, point(site)) {
                self.post_host = Some(PostHost {
                    actor,
                    site,
                    until: now + 36 * MINUTE,
                });
                return;
            }
            world.end_activity(&[actor], 0);
        }
    }

    pub(super) fn finish_post_host(&mut self, world: &mut LivingWorld) {
        if let Some(booking) = self.post_host.take() {
            world.end_activity(&[booking.actor], 0);
        }
    }

    pub(super) fn sync_post_host(&mut self, world: &mut LivingWorld, job: Option<u16>, now: u64) {
        if self.post_host.is_some_and(|booking| {
            now >= booking.until
                || world.activity_deadline(booking.actor).is_none()
                || job
                    .and_then(|id| contracts().nth(id as usize))
                    .is_none_or(|job| job.id != "RI001")
        }) {
            self.finish_post_host(world);
        }
    }

    pub(super) fn expire_mail(&mut self, now: u64) -> bool {
        let Some(hold) = &mut self.mail else {
            return false;
        };
        if hold.expired || now.saturating_sub(hold.taken) < BOOKING_LIFETIME {
            return false;
        }
        hold.expired = true;
        true
    }

    pub(super) fn mail_waiting(&self, now: u64) -> bool {
        self.mail
            .is_some_and(|hold| !hold.expired && now.saturating_sub(hold.taken) < BOOKING_LIFETIME)
    }

    pub(super) fn mail_returned(&mut self, world: &mut LivingWorld, asset: u8, at: TilePos) {
        if self
            .mail
            .is_some_and(|hold| hold.asset == asset && hold.source == at)
        {
            if self.mail.take().unwrap().expired {
                witness(world, "H07-06", COURIER, at, at, None);
            }
        }
    }

    pub(super) fn mail_transferred(
        &mut self,
        world: &mut LivingWorld,
        services: &mut ServiceLedger,
        id: &str,
        asset: u8,
        at: TilePos,
        recipient: u8,
        delivered_to: Option<(LegacyActor, TilePos)>,
    ) {
        match id {
            "RI001" | "ME062" => {
                if let Some((partner, target)) = delivered_to {
                    if let ("RI001", LegacyActor::Household(_)) = (id, partner) {
                        world.record_legacy_completion(
                            "H01-48",
                            partner,
                            target,
                            at,
                            LegacyPlace::Nearby,
                            1,
                            Some(LegacyActor::Resident(COURIER)),
                        );
                    }
                    if services.stamp_letter(asset, recipient, at, world.world_ms()) {
                        witness(world, "H07-05", COURIER, at, target, Some(partner));
                    }
                }
                if id == "RI001" {
                    self.finish_post_host(world);
                }
            }
            "RI005" => self.mail = None,
            _ => {}
        }
    }

    pub(super) fn update_menu(
        &mut self,
        world: &mut LivingWorld,
        servings: u16,
        trip: Option<Trip>,
    ) {
        let day = world.world_ms() / DAY_MS;
        self.sync_menu_trip(trip);
        if let Some(menu) = &mut self.menu {
            if menu.day != day && menu.pending.is_some() {
                menu.servings = 0;
                return;
            }
        }
        if self.menu.is_none_or(|menu| menu.day != day) {
            self.menu = (servings > 0).then_some(Menu {
                day,
                servings: 0,
                booked: 0,
                pending: None,
            });
        }
        if let Some(menu) = &mut self.menu {
            let newly_posted = menu.servings == 0 && servings > 0;
            menu.servings = servings;
            if newly_posted {
                let at = booth_slot(MEAL_SHOP);
                witness(world, "H06-09", COOK, at, at, None);
            }
        }
    }

    pub(super) fn sync_menu_trip(&mut self, trip: Option<Trip>) {
        if let Some(menu) = &mut self.menu
            && let Some(started) = menu.pending
            && !trip.is_some_and(|t| t.started == started && usize::from(t.shop) == MEAL_SHOP)
        {
            // A trip that disappeared before packing held only an appointment.
            menu.pending = None;
            menu.booked -= 1;
        }
    }

    pub(super) fn menu_can_book(&self, day: u64) -> bool {
        self.menu
            .is_none_or(|menu| menu.day != day || menu.booked < MENU_LIMIT)
    }

    pub(super) fn menu_booked(&mut self, now: u64) {
        if let Some(menu) = &mut self.menu
            && menu.day == now / DAY_MS
            && menu.servings > 0
            && menu.booked < MENU_LIMIT
            && menu.pending.is_none()
        {
            menu.pending = Some(now);
            menu.booked += 1;
        }
    }

    pub(super) fn menu_returned(&mut self, start: u64, sold: bool) {
        if let Some(menu) = &mut self.menu
            && menu.pending == Some(start)
        {
            menu.pending = None;
            if !sold {
                menu.booked -= 1;
            }
        }
    }

    pub(super) fn menu_offer(&self, day: u64) -> Option<(u16, u8)> {
        let menu = self
            .menu
            .filter(|menu| menu.day == day && menu.servings > 0)?;
        Some((menu.servings, MENU_LIMIT - menu.booked))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expansion_returned_seed_sleeve_carries_the_same_paid_counter_lot() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        let mut life = VillageLife::default();
        let doorway = house(SEED_MERCHANT).door;
        let stand = booth_slot(1);
        let goods = shop(1).goods;
        let before = life.people[SEED_MERCHANT].count(Item::Seeds);
        let merchant = &mut life.people[SEED_MERCHANT];
        assert!(merchant.pack_counter(1, 0));
        assert_eq!(merchant.count(Item::Seeds), before - goods.quantity);
        merchant.wet_returning_seed_lot();
        assert_eq!(merchant.counter.unwrap().paper, SeedPaper::Plain);
        merchant.counter.as_mut().unwrap().paper = SeedPaper::Offered;
        merchant.wet_returning_seed_lot();
        merchant.walk = Walker::new(stand, super::super::State::AtWork);
        assert!(!life.wrap_returned_seed_lot(&mut world, now));
        assert!(!life.people[SEED_MERCHANT].ready_to_exchange(1, false));
        life.people[SEED_MERCHANT].walk = Walker::new(doorway, super::super::State::AtWork);
        assert!(life.wrap_returned_seed_lot(&mut world, now));
        assert!(life.wrap_returned_seed_lot(&mut world, now + 3 * MINUTE));
        assert!(!life.people[SEED_MERCHANT].sealed_seed_stock);
        let mut life: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(life.wrap_returned_seed_lot(&mut world, now + 4 * MINUTE));
        let merchant = &mut life.people[SEED_MERCHANT];
        assert_eq!(merchant.seed_wraps_used, 1);
        assert!(merchant.sealed_seed_stock && merchant.counter.is_none());
        assert_eq!(merchant.count(Item::Seeds), before - goods.quantity);
        assert!(merchant.pack_counter(MOBILE, 0));
        assert_eq!(merchant.count(Item::Seeds), before - goods.quantity);
        assert!(!merchant.sealed_seed_stock);
        merchant.wet_returning_seed_lot();
        assert_eq!(merchant.counter.unwrap().paper, SeedPaper::Sealed);
        assert_eq!(merchant.counter.unwrap().cargo, goods);
        assert!(merchant.ready_to_exchange(MOBILE, false));
        assert_eq!(merchant.stow_counter().unwrap().cargo, goods);
        assert!(merchant.sealed_seed_stock);
        assert_eq!(merchant.seed_wraps_used, 1);
        assert_eq!(merchant.count(Item::Seeds) + goods.quantity, before);
    }

    fn saw(world: &LivingWorld, base: &str) -> bool {
        world
            .legacy_behaviors()
            .recent()
            .iter()
            .any(|event| event.id().starts_with(base))
    }

    #[test]
    fn legacy_residents_seed_shift_preserves_unsold_stock_and_waits_for_refund() {
        let mut life = VillageLife::default();
        let mut world = LivingWorld::new_village_wall_ms(0);
        let now = world.world_ms();
        let before = life.people[1].count(Item::Seeds);
        assert!(life.people[1].pack_counter(1, 0));
        let batch = life.people[1].counter.unwrap();
        life.legacy.seed_packed(now, batch, before);
        assert_eq!(
            life.people[1].count(Item::Seeds),
            before - batch.cargo.quantity
        );
        assert!(!saw(&world, "H10-01"));
        life.people[1].walk = Walker::new(booth_slot(1), super::super::State::AtWork);
        life.legacy
            .seed_counter(&mut world, &life.people[1], true, None);
        assert!(saw(&world, "H10-01"));
        let data = serde_json::to_vec(&life).unwrap();
        let mut reopened: VillageLife = serde_json::from_slice(&data).unwrap();
        assert!(reopened.legacy.valid(&reopened, now));
        reopened
            .legacy
            .seed_counter(&mut world, &reopened.people[1], false, None);
        assert!(!saw(&world, "H10-10"));
        reopened.people[1].walk = Walker::new(house(1).door, super::super::State::AtWork);
        reopened.stow_resident_counter(1);
        assert_eq!(reopened.people[1].count(Item::Seeds), before);
        let unpaid = Trip {
            actor: ActorKind::Xiaohe,
            shop: 1,
            started: now,
            due: now,
            expires: now + HOUR,
            stage: TripStage::Inbound,
            target: house(1).door,
            bag: Some(shop(1).cost),
            outgoing: shop(1).cost,
            incoming: shop(1).goods,
            accepted: false,
            sealed_seeds: false,
            leg: 0,
            hold: true,
            regular_meal: false,
        };
        reopened
            .legacy
            .seed_counter(&mut world, &reopened.people[1], false, Some(unpaid));
        assert!(!saw(&world, "H10-10"));
        reopened
            .legacy
            .seed_counter(&mut world, &reopened.people[1], false, None);
        assert!(saw(&world, "H10-10"));
        assert!(reopened.legacy.seed_shop().is_none());
    }

    #[test]
    fn legacy_residents_mail_uses_its_actual_postmark_and_returns_the_original_slip_after_reopen() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let original = house(1).door;
        let porch = life
            .interaction_position(world.phase_mask(), "home_trade")
            .unwrap();
        let now = world.world_ms();
        let ServiceOp::Move(letter, owner, recipient) = services::definitions()
            .find(|def| def.id == "RI001")
            .unwrap()
            .op
        else {
            panic!()
        };
        assert!(!life.services.stamp_letter(letter, recipient, porch, now));
        assert!(
            life.services
                .assign_carrier(letter, owner, COURIER, house(0).door)
        );
        assert!(
            life.services
                .move_asset(letter, owner, recipient, COURIER, porch, now)
        );
        let actual_recipient = Some((
            LegacyActor::Household(0),
            world.actor_tile(ActorKind::Xiaohe),
        ));
        life.legacy.mail_transferred(
            &mut world,
            &mut life.services,
            "RI001",
            letter,
            porch,
            recipient,
            actual_recipient,
        );
        assert!(saw(&world, "H07-05"));
        assert!(saw(&world, "H01-48"));
        assert!(!life.services.stamp_letter(letter, recipient, porch, now));
        let ServiceOp::Move(reply, owner, _) = services::definitions()
            .find(|def| def.id == "RI002")
            .unwrap()
            .op
        else {
            panic!()
        };
        assert!(life.services.assign_carrier(reply, owner, COURIER, porch));
        life.legacy.mail_collected(&mut world, porch);
        assert!(saw(&world, "H07-01"));
        life.services.request_return(COURIER);
        assert!(life.services.return_asset(reply, COURIER, porch));
        let ServiceOp::Move(slip, owner, _) = services::definitions()
            .find(|def| def.id == "RI005")
            .unwrap()
            .op
        else {
            panic!()
        };
        assert!(life.services.assign_carrier(slip, owner, COURIER, original));
        life.legacy.mail_picked_up(slip, original, now);
        assert!(!life.legacy.expire_mail(now + BOOKING_LIFETIME - 1));
        let data = serde_json::to_vec(&life).unwrap();
        let mut life: VillageLife = serde_json::from_slice(&data).unwrap();
        assert!(life.legacy.valid(&life, now));
        assert_eq!(
            life.resident_held(0),
            Some(VisitorHeld::Parcel("booking_slip"))
        );
        let expires = now + BOOKING_LIFETIME;
        world.queue_world_time(BOOKING_LIFETIME);
        for _ in 0..1024 {
            if world.advance_pending_to(expires) && world.world_ms() == expires {
                break;
            }
        }
        assert_eq!(world.world_ms(), expires);
        assert!(life.legacy.expire_mail(world.world_ms()));
        life.services.request_return(COURIER);
        assert!(!life.services.return_asset(slip, COURIER, porch));
        assert!(!saw(&world, "H07-06"));
        life.people[0].walk = Walker::new(original, super::super::State::AtWork);
        life.schedule(&mut world, PhaseMask::DAY_ONE, now + BOOKING_LIFETIME);
        assert!(life.services.holds(slip, owner));
        assert_eq!(life.services.stored_at(slip), Some(original));
        assert!(saw(&world, "H07-06"));
        assert!(!life.legacy.expire_mail(world.world_ms()));
    }

    #[test]
    fn legacy_residents_menu_has_two_owned_reservations_and_reopens_refunded_capacity() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let now = world.world_ms();
        let day = now / DAY_MS;
        let stock = serde_json::to_vec(world.stock()).unwrap();
        life.legacy.update_menu(&mut world, 0, None);
        assert!(life.legacy.menu_offer(day).is_none());
        assert!(!saw(&world, "H06-09"));
        assert!(life.people[3].pack_counter(MEAL_SHOP, 0));
        let servings = life.people[3].counter_goods(MEAL_SHOP);
        life.legacy.update_menu(&mut world, servings, None);
        assert!(saw(&world, "H06-09"));
        assert_eq!(life.legacy.menu_offer(day), Some((servings, 2)));
        life.legacy.menu_booked(now);
        life.legacy.menu_booked(now + MINUTE);
        assert_eq!(life.legacy.menu_offer(day), Some((servings, 1)));
        life.legacy.sync_menu_trip(None);
        assert_eq!(life.legacy.menu_offer(day), Some((servings, 2)));
        for i in 0..2 {
            life.legacy.menu_booked(now + i);
            life.legacy.menu_returned(now + i, true);
        }
        let state = serde_json::to_vec(&life).unwrap();
        let life: VillageLife = serde_json::from_slice(&state).unwrap();
        assert!(life.legacy.valid(&life, now));
        assert!(!life.legacy.menu_can_book(day));
        assert!(life.legacy.menu_can_book(day + 1));
        assert_eq!(serde_json::to_vec(world.stock()).unwrap(), stock);
    }

    #[test]
    fn legacy_residents_last_seed_bag_remains_held_for_the_same_buyer_and_releases_on_cancel() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut life = VillageLife::default();
        let now = world.world_ms();
        let tray = shop(1);
        assert!(life.people[1].pack_counter(1, 0));
        life.people[1].walk = Walker::new(booth_slot(1), super::super::State::AtWork);
        let trip = Trip {
            actor: ActorKind::Xiaohe,
            shop: 1,
            started: now,
            due: now,
            expires: now + HOUR,
            stage: TripStage::Pack,
            target: house(1).door,
            bag: None,
            outgoing: tray.cost,
            incoming: tray.goods,
            accepted: false,
            sealed_seeds: false,
            leg: 0,
            hold: true,
            regular_meal: false,
        };
        life.legacy
            .reserve_last_seed(&mut world, trip, &life.people[1]);
        life.trip = Some(trip);
        assert!(saw(&world, "H10-07"));
        assert_eq!(life.people[1].counter_goods(1), tray.goods.quantity);
        let mut reopened: VillageLife =
            serde_json::from_slice(&serde_json::to_vec(&life).unwrap()).unwrap();
        assert!(reopened.legacy.valid(&reopened, now));
        assert!(!reopened.legacy.seed_booking_permits(Trip {
            actor: ActorKind::Ayu,
            ..trip
        }));
        reopened.legacy.sync_seed_booking(None, &reopened.people[1]);
        assert!(reopened.legacy.seed_booking.is_none());
        assert_eq!(reopened.people[1].counter_goods(1), tray.goods.quantity);
    }
}
