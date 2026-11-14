//! Time-based camera for the village. Preferences are owned by the host.
use super::Camera;
use crate::living_world::{ActorKind, LivingWorld, Point, Pose, TILE_PX};
use crate::map_data::{TilePos, WILLOW_VILLAGE};
use crate::residents::{TripStage, TripVisual};
use std::time::Duration;

#[derive(Default)]
pub struct VillageCamera {
    initialized: bool,
    focus: Option<ActorKind>,
    look: (f32, f32),
    velocity: (f32, f32),
    moving: bool,
}

impl VillageCamera {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn moving(&self) -> bool {
        self.moving
    }

    pub fn update(
        &mut self,
        camera: &mut Camera,
        world: &LivingWorld,
        size: (i32, i32),
        elapsed: Duration,
        trip: Option<TripVisual>,
    ) {
        self.update_viewport(camera, world, size, elapsed, trip, true);
    }

    pub fn update_physical(
        &mut self,
        camera: &mut Camera,
        world: &LivingWorld,
        physical: (i32, i32),
        elapsed: Duration,
        trip: Option<TripVisual>,
    ) {
        self.update(camera, world, safe_logical_size(physical), elapsed, trip);
    }

    pub fn update_follow_physical(
        &mut self,
        camera: &mut Camera,
        world: &LivingWorld,
        physical: (i32, i32),
        elapsed: Duration,
    ) {
        self.update_follow(camera, world, safe_logical_size(physical), elapsed);
    }

    pub fn retain_awake_physical(
        &mut self,
        camera: &mut Camera,
        world: &LivingWorld,
        physical: (i32, i32),
    ) {
        let size = safe_logical_size(physical);
        camera.clamp_in_world(world, size);
        retain_awake(camera, world, size, self.focus);
        self.moving = false;
    }

    fn update_viewport(
        &mut self,
        camera: &mut Camera,
        world: &LivingWorld,
        size: (i32, i32),
        elapsed: Duration,
        trip: Option<TripVisual>,
        automatic: bool,
    ) {
        let home = world
            .home_of(ActorKind::Xiaohe)
            .or_else(|| world.resolve_poi("home_xiaohe.door"))
            .unwrap_or(Point::at_tile(12, 16));
        let home_margin = (size.0 as f32 * 0.22).clamp(20.0, 65.0);
        let home_origin = (
            home.x - home_margin,
            home.y - (size.1 as f32 * 0.64).clamp(28.0, 67.0),
        );
        let dt = elapsed.as_secs_f32().min(0.25);
        let active = trip.filter(|t| !matches!(t.stage, TripStage::Pack));
        let eligible = awake_protagonists(world);
        let valid = |kind: &ActorKind| eligible.contains(&Some(*kind));
        let named = camera
            .follow
            .and_then(|i| world.actors().get(i as usize))
            .map(|a| a.kind);
        let preferred = if automatic {
            active.map(|trip| trip.actor).filter(valid).or_else(|| {
                self.focus
                    .into_iter()
                    .chain([ActorKind::Xiaohe, ActorKind::Ayu])
                    .filter(valid)
                    .find(|&kind| outdoors(world, kind))
            })
        } else {
            named.filter(valid)
        };
        let fit = |kind| VisibleOrigin::for_actor(world, kind, size);
        let current = (camera.x, camera.y);
        let is_visible = |&kind: &ActorKind| valid(&kind) && fit(kind).contains(current);
        let visible = eligible.into_iter().flatten().find(is_visible);
        let focus = preferred
            .filter(|kind| {
                (!automatic && !self.initialized) || visible.is_none() || is_visible(kind)
            })
            .or_else(|| self.focus.filter(is_visible))
            .or(visible)
            .or(preferred)
            .or_else(|| nearest_awake(camera, world, size));
        if focus != self.focus {
            self.look = (0.0, 0.0);
        }
        self.focus = focus;
        let target = if let Some(focus) =
            focus.filter(|kind| preferred.is_some() || !fit(*kind).contains(home_origin))
        {
            let actor = &world.actors()[focus as usize];
            // An edge's direction has unit length throughout the step. Fade
            // direction continuously at corners and into a stop.
            let direction = actor.walking_segment().map_or((0.0, 0.0), |(from, to)| {
                let (dx, dy) = (to.x - from.x, to.y - from.y);
                let length = dx.hypot(dy).max(1.0);
                (dx / length * 10.0, dy / length * 10.0)
            });
            let gain = -(-dt / 0.30).exp_m1();
            self.look.0 += (direction.0 - self.look.0) * gain;
            self.look.1 += (direction.1 - self.look.1) * gain;
            follow_origin(camera, actor.position, self.look, size)
        } else {
            self.look = (0.0, 0.0);
            home_origin
        };
        let mut target = Camera {
            x: target.0,
            y: target.1,
            follow: None,
        };
        target.clamp_in_world(world, size);
        retain_awake(&mut target, world, size, focus);
        let target = (target.x, target.y);
        let settled = self.move_camera(camera, world, size, target, elapsed, !self.initialized);
        let unclamped = (camera.x, camera.y);
        retain_awake(camera, world, size, focus);
        if unclamped.0 != camera.x {
            self.velocity.0 = 0.0;
        }
        if unclamped.1 != camera.y {
            self.velocity.1 = 0.0;
        }
        self.moving = !settled
            || self.velocity.0.hypot(self.velocity.1) > 0.15
            || focus.is_some_and(|kind| {
                world.actors()[kind as usize].walking_segment().is_some()
                    || self.look.0.hypot(self.look.1) > 0.25
            });
    }

    pub fn update_follow(
        &mut self,
        camera: &mut Camera,
        world: &LivingWorld,
        size: (i32, i32),
        elapsed: Duration,
    ) {
        self.update_viewport(camera, world, size, elapsed, None, false);
    }

    fn move_camera(
        &mut self,
        camera: &mut Camera,
        world: &LivingWorld,
        size: (i32, i32),
        target: (f32, f32),
        elapsed: Duration,
        snap: bool,
    ) -> bool {
        let dt = elapsed.as_secs_f32().min(0.25);
        if snap {
            (camera.x, camera.y) = target;
            self.velocity = (0.0, 0.0);
        } else if dt > 0.0 {
            let dx = target.0 - camera.x;
            let dy = target.1 - camera.y;
            let distance = dx.hypot(dy);
            let gain = (128.0 / (distance * 4.0).max(0.001)).min(1.0) * 4.0;
            let (x, vx) = coast(camera.x, self.velocity.0, dx * gain, dt);
            let (y, vy) = coast(camera.y, self.velocity.1, dy * gain, dt);
            (camera.x, camera.y) = (x, y);
            self.velocity = (vx, vy);
        }
        let next = (camera.x, camera.y);
        camera.clamp_in_world(world, size);
        if next.0 != camera.x {
            self.velocity.0 = 0.0;
        }
        if next.1 != camera.y {
            self.velocity.1 = 0.0;
        }
        let settled = (camera.x - target.0).hypot(camera.y - target.1) <= 0.08;
        if settled && self.velocity.0.hypot(self.velocity.1) <= 0.15 {
            (camera.x, camera.y) = target;
            self.velocity = (0.0, 0.0);
        }
        self.initialized = true;
        settled
    }
}

/// Exact integration toward a constant requested velocity over this sample.
fn coast(position: f32, velocity: f32, request: f32, seconds: f32) -> (f32, f32) {
    const DAMPING: f32 = 0.085;
    let gain = -(-seconds / DAMPING).exp_m1();
    let distance = request * seconds + (velocity - request) * DAMPING * gain;
    (position + distance, velocity + (request - velocity) * gain)
}

fn safe_logical_size((width, height): (i32, i32)) -> (i32, i32) {
    // Work inside the complete physical tiles and the native six-pixel frame.
    (
        (width.clamp(1, 3840) - 12).max(crate::viewport::WORLD_PIXEL)
            / crate::viewport::WORLD_PIXEL,
        (height.clamp(1, 2160) - 12).max(crate::viewport::WORLD_PIXEL)
            / crate::viewport::WORLD_PIXEL,
    )
}

pub(super) fn awake_protagonists(world: &LivingWorld) -> [Option<ActorKind>; 2] {
    let protagonists = &world.actors()[..2];
    let has_outside = protagonists
        .iter()
        .any(|actor| actor.pose != Pose::Sleep && !actor.inside());
    [0, 1].map(|i| {
        let actor = &protagonists[i];
        (actor.pose != Pose::Sleep && (!has_outside || !actor.inside())).then_some(actor.kind)
    })
}

#[derive(Clone, Copy)]
struct VisibleOrigin {
    min: (f32, f32),
    max: (f32, f32),
}

impl VisibleOrigin {
    fn for_actor(world: &LivingWorld, kind: ActorKind, size: (i32, i32)) -> Self {
        let p = world.actors()[kind as usize].position;
        let map = world.map_extent();
        let limits = (
            (i32::from(map.0) * TILE_PX - size.0).max(0) as f32,
            (i32::from(map.1) * TILE_PX - size.1).max(0) as f32,
        );
        let w = size.0.max(1) as f32;
        let h = size.1.max(1) as f32;
        let (px, py) = (p.x, p.y);
        // Keep the activity comfortably in frame, not merely the last visible pixel.
        let side = (w * 0.30).max(16.0).min(w / 2.0);
        let feet = (h * 0.27).max(8.0).min(h / 3.0);
        let head = (h * 0.45).max(25.0).min(h * 2.0 / 3.0);
        Self {
            min: (
                (px + side - w).clamp(0.0, limits.0),
                (py + feet - h).clamp(0.0, limits.1),
            ),
            max: (
                (px - side).clamp(0.0, limits.0),
                (py - head).clamp(0.0, limits.1),
            ),
        }
    }
    fn contains(self, p: (f32, f32)) -> bool {
        (self.min.0..=self.max.0).contains(&p.0) && (self.min.1..=self.max.1).contains(&p.1)
    }
    fn clamp(self, p: (f32, f32)) -> (f32, f32) {
        (
            p.0.clamp(self.min.0, self.max.0.max(self.min.0)),
            p.1.clamp(self.min.1, self.max.1.max(self.min.1)),
        )
    }
}

fn nearest_awake(camera: &Camera, world: &LivingWorld, size: (i32, i32)) -> Option<ActorKind> {
    awake_protagonists(world)
        .into_iter()
        .flatten()
        .min_by(|&a, &b| {
            let distance = |kind| {
                let fit = VisibleOrigin::for_actor(world, kind, size);
                let p = fit.clamp((camera.x, camera.y));
                (p.0 - camera.x).hypot(p.1 - camera.y)
            };
            distance(a).total_cmp(&distance(b))
        })
}

fn retain_awake(
    camera: &mut Camera,
    world: &LivingWorld,
    size: (i32, i32),
    preferred: Option<ActorKind>,
) {
    let p = (camera.x, camera.y);
    let eligible = awake_protagonists(world);
    if eligible
        .into_iter()
        .flatten()
        .any(|kind| VisibleOrigin::for_actor(world, kind, size).contains(p))
    {
        return;
    }
    if let Some(kind) = preferred
        .filter(|kind| eligible.contains(&Some(*kind)))
        .or_else(|| nearest_awake(camera, world, size))
    {
        (camera.x, camera.y) = VisibleOrigin::for_actor(world, kind, size).clamp(p);
    }
}

fn outdoors(world: &LivingWorld, kind: ActorKind) -> bool {
    if kind == ActorKind::Orange || world.actors()[kind as usize].inside() {
        return false;
    }
    let is_home = |tile: TilePos| {
        WILLOW_VILLAGE
            .regions
            .iter()
            .any(|region| matches!(region.id, "home" | "farm") && region.rect.contains(tile))
    };
    let old = world.actor_tile(kind);
    if !is_home(old) {
        return true;
    }
    world
        .actor_next_tile(kind)
        .is_some_and(|next| !is_home(next))
}

fn follow_origin(
    camera: &Camera,
    point: Point,
    look: (f32, f32),
    (width, height): (i32, i32),
) -> (f32, f32) {
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    let x_margin = width * 0.43;
    let top_margin = height * 0.56;
    let bottom_margin = height * 0.36;
    let look = (
        look.0.clamp(-width * 0.10, width * 0.10),
        look.1.clamp(-height * 0.09, height * 0.09),
    );
    (
        camera
            .x
            .max(point.x + look.0 - width + x_margin)
            .min(point.x + look.0 - x_margin),
        camera
            .y
            .max(point.y + look.1 - height + bottom_margin)
            .min(point.y + look.1 - top_margin),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_camera_uses_elapsed_time_and_keeps_home_still() {
        let world = LivingWorld::new_village_wall_ms(0);
        let size = (160, 100);
        let mut a = VillageCamera::new();
        let mut b = VillageCamera::new();
        let mut once = Camera::default();
        let mut slices = once;
        a.update(&mut once, &world, size, Duration::ZERO, None);
        b.update(&mut slices, &world, size, Duration::ZERO, None);
        let home = (once.x, once.y);
        a.update(&mut once, &world, size, Duration::from_millis(100), None);
        assert_eq!((once.x, once.y), home);
        let destination = (600.0, 340.0);
        a.move_camera(
            &mut once,
            &world,
            size,
            destination,
            Duration::from_millis(100),
            false,
        );
        for _ in 0..4 {
            b.move_camera(
                &mut slices,
                &world,
                size,
                destination,
                Duration::from_millis(25),
                false,
            );
        }
        assert!(once.x > home.0);
        assert!((once.x - slices.x).abs().max((once.y - slices.y).abs()) < 0.01);
        let prior = (once.x, once.y);
        a.move_camera(
            &mut once,
            &world,
            size,
            destination,
            Duration::from_millis(300),
            false,
        );
        assert!((once.x - prior.0).hypot(once.y - prior.1) <= 32.01);
        assert!(
            once.x < 250.0,
            "a stalled frame snapped toward the distant target"
        );
    }

    #[test]
    fn auto_retains_awake_protagonist_in_small_odd_and_resized_windows() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut control = VillageCamera::new();
        let mut camera = Camera {
            x: 150.0,
            y: 120.0,
            ..Camera::default()
        };
        let home = Point::at_tile(24, 20);
        let away = Point::at_tile(92, 51);
        world.test_camera_actor(ActorKind::Xiaohe, home, Pose::Walk, false);
        world.test_camera_actor(ActorKind::Ayu, away, Pose::Work, false);
        let trip = Some(TripVisual {
            actor: ActorKind::Ayu,
            stage: TripStage::Outbound,
            position: away,
            target: Point::at_tile(94, 51),
            held: None,
        });
        let (wide, narrow) = ((660, 430), (349, 272));
        control.update_physical(&mut camera, &world, wide, Duration::ZERO, None);
        assert!(
            VisibleOrigin::for_actor(&world, ActorKind::Xiaohe, safe_logical_size(wide))
                .contains((camera.x, camera.y))
        );
        let prior = (camera.x, camera.y);
        control.update_physical(&mut camera, &world, wide, Duration::from_millis(80), trip);
        assert!(
            VisibleOrigin::for_actor(&world, ActorKind::Xiaohe, safe_logical_size(wide))
                .contains((camera.x, camera.y))
        );
        assert!((camera.x - prior.0).hypot(camera.y - prior.1) <= 10.25);
        world.test_camera_actor(ActorKind::Xiaohe, home, Pose::Sleep, true);
        control.update_physical(&mut camera, &world, narrow, Duration::ZERO, trip);
        assert!(
            VisibleOrigin::for_actor(&world, ActorKind::Ayu, safe_logical_size(narrow))
                .contains((camera.x, camera.y))
        );
        assert!(camera.x > 600.0);
        control.update_physical(&mut camera, &world, (251, 181), Duration::ZERO, None);
        assert!(
            VisibleOrigin::for_actor(&world, ActorKind::Ayu, safe_logical_size((251, 181)))
                .contains((camera.x, camera.y))
        );
    }

    #[test]
    fn readable_focus_keeps_activity_off_the_corners_at_double_magnification() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut control = VillageCamera::new();
        let p = Point::at_tile(35, 25);
        world.test_camera_actor(ActorKind::Xiaohe, p, Pose::Work, false);
        world.test_camera_actor(ActorKind::Ayu, Point::at_tile(24, 16), Pose::Sleep, true);
        for physical in [(720, 530), (481, 301), (251, 181), (1200, 800)] {
            let mut camera = Camera {
                x: p.x - 3.0,
                y: p.y - 3.0,
                follow: Some(0),
            };
            control.update_follow_physical(&mut camera, &world, physical, Duration::ZERO);
            let (w, h) = safe_logical_size(physical);
            let (x, y) = (p.x - camera.x, p.y - camera.y);
            assert!(
                x >= w as f32 * 0.29 && x <= w as f32 * 0.71,
                "{physical:?} horizontal {x}"
            );
            assert!(
                y >= h as f32 * 0.44 && y <= h as f32 * 0.74,
                "{physical:?} vertical {y}"
            );
        }
    }

    #[test]
    fn sleeping_requested_target_yields_to_the_only_awake_indoor_protagonist() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut control = VillageCamera::new();
        let mut camera = Camera::default();
        let spot = world.buildings()[2].position;
        world.test_camera_actor(ActorKind::Xiaohe, Point::at_tile(80, 44), Pose::Sleep, true);
        world.test_camera_actor(ActorKind::Ayu, spot, Pose::Work, true);
        camera.set_follow(Some(ActorKind::Xiaohe as u8));
        let physical = (387, 272);
        control.update_follow_physical(&mut camera, &world, physical, Duration::ZERO);
        assert_eq!(awake_protagonists(&world), [None, Some(ActorKind::Ayu)]);
        assert!(
            VisibleOrigin::for_actor(&world, ActorKind::Ayu, safe_logical_size(physical))
                .contains((camera.x, camera.y))
        );
        camera.x = 900.0;
        camera.y = 400.0;
        control.retain_awake_physical(&mut camera, &world, physical);
        assert!(
            VisibleOrigin::for_actor(&world, ActorKind::Ayu, safe_logical_size(physical))
                .contains((camera.x, camera.y))
        );
        world.test_camera_actor(ActorKind::Ayu, spot, Pose::Sleep, true);
        control.update_physical(
            &mut camera,
            &world,
            physical,
            Duration::from_millis(120),
            None,
        );
        assert_eq!(awake_protagonists(&world), [None, None]);
    }

    #[test]
    fn walking_turns_and_camera_return_settle_with_bounded_screen_steps() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut control = VillageCamera::new();
        let mut camera = Camera::default();
        let physical = (479, 301);
        let size = safe_logical_size(physical);
        let world_size = (192 * TILE_PX, 128 * TILE_PX);
        let mut p = Point::at_tile(40, 24);
        world.test_camera_actor(ActorKind::Ayu, Point::at_tile(30, 22), Pose::Sleep, true);
        let mut origin: Option<(i32, i32)> = None;
        let mut one_pixel = 0;
        let mut greatest = 0;
        for frame in 0..620 {
            let (dx, dy) = match frame {
                0..=200 => (0.128, 0.0),
                201..=340 => (0.0, 0.128),
                341..=450 => (-0.128, 0.0),
                _ => (0.0, 0.0),
            };
            p.x += dx;
            p.y += dy;
            world.test_camera_actor(ActorKind::Xiaohe, p, Pose::Work, false);
            camera.set_follow(Some(0));
            control.update_follow_physical(
                &mut camera,
                &world,
                physical,
                Duration::from_millis(16),
            );
            assert!(
                VisibleOrigin::for_actor(&world, ActorKind::Xiaohe, size)
                    .contains((camera.x, camera.y))
            );
            let next =
                crate::viewport::PixelOrigin::new((camera.x, camera.y), physical, world_size)
                    .physical();
            if let Some(prev) = origin {
                let step = (prev.0 - next.0).abs().max((prev.1 - next.1).abs());
                greatest = greatest.max(step);
                one_pixel += usize::from(step == 1);
            }
            origin = Some(next);
        }
        assert!(one_pixel > 150, "{one_pixel} single-pixel frames");
        assert!(greatest <= 2, "maximum step was {greatest} physical pixels");
        assert!(!control.moving());
    }
}
