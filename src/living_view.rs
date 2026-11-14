//! Read-only viewport renderer. No clock, inventory or job mutation is allowed here.
use crate::living_world::story::{FixtureStatus, ImprovementOrder};
use crate::living_world::*;
use crate::plugin_scene::FrameCommand;
use crate::plugins::{AssetIndex, LocalId, PackId, PluginStore};
use crate::visitors::VisitorVisual;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[path = "art_assets.rs"]
mod art_assets;
use art_assets::ArtAtlas;
#[path = "village_camera.rs"]
mod village_camera;
pub use village_camera::VillageCamera;
#[path = "village_render.rs"]
mod village_render;
use crate::residents::{StallVisual, WildlifeVisual};
use village_render::VillageIndex;

const PAGES: [&[u8]; 16] = [
    include_bytes!("../assets/life/page_00.bgra"),
    include_bytes!("../assets/life/page_01.bgra"),
    include_bytes!("../assets/life/page_02.bgra"),
    include_bytes!("../assets/life/page_03.bgra"),
    include_bytes!("../assets/life/page_04.bgra"),
    include_bytes!("../assets/life/page_05.bgra"),
    include_bytes!("../assets/life/page_06.bgra"),
    include_bytes!("../assets/life/page_07.bgra"),
    include_bytes!("../assets/life/page_08.bgra"),
    include_bytes!("../assets/life/page_09.bgra"),
    include_bytes!("../assets/life/page_10.bgra"),
    include_bytes!("../assets/life/page_11.bgra"),
    include_bytes!("../assets/life/page_12.bgra"),
    include_bytes!("../assets/life/page_13.bgra"),
    include_bytes!("../assets/life/page_14.bgra"),
    include_bytes!("../assets/life/page_15.bgra"),
];
const VISITOR_PAGES: [&[u8]; 10] = [
    include_bytes!("../assets/visitors/page_00.bgra"),
    include_bytes!("../assets/visitors/page_01.bgra"),
    include_bytes!("../assets/visitors/page_02.bgra"),
    include_bytes!("../assets/visitors/page_03.bgra"),
    include_bytes!("../assets/visitors/page_04.bgra"),
    include_bytes!("../assets/visitors/page_05.bgra"),
    include_bytes!("../assets/visitors/page_06.bgra"),
    include_bytes!("../assets/visitors/page_07.bgra"),
    include_bytes!("../assets/visitors/page_08.bgra"),
    include_bytes!("../assets/visitors/page_09.bgra"),
];
const DEVELOPMENT_PAGES: [&[u8]; 2] = [
    include_bytes!("../assets/developments/page_00.bgra"),
    include_bytes!("../assets/developments/page_01.bgra"),
];
static CORE_INDEX: LazyLock<AssetIndex> = LazyLock::new(|| {
    serde_json::from_slice(include_bytes!("../assets/life/index.json")).expect("baked core atlas")
});
static VISITOR_INDEX: LazyLock<AssetIndex> = LazyLock::new(|| {
    serde_json::from_slice(include_bytes!("../assets/visitors/index.json"))
        .expect("baked visitor atlas")
});
static DEVELOPMENT_INDEX: LazyLock<AssetIndex> = LazyLock::new(|| {
    serde_json::from_slice(include_bytes!("../assets/developments/index.json"))
        .expect("baked development atlas")
});
// 28 embedded atlas pages plus at most 16 decoded package pages. The name
// labels share the final visitor page; the old cat strips are separate.
const PLUGIN_PAGE_LIMIT: usize = 16;
const MAX_DRAWN_VISITORS: usize = 8;
const TERRAIN_CACHE_LIMIT: usize = 2 * 1024 * 1024;
const CAT_IDLE: &[u8] = include_bytes!("../assets/world/cat_idle.bgra");
const CAT_WALK: &[u8] = include_bytes!("../assets/world/cat_walk.bgra");
const CAT_SLEEP: &[u8] = include_bytes!("../assets/world/cat_sleep.bgra");
const CAT_EAT: &[u8] = include_bytes!("../assets/world/cat_eat.bgra");
const TABLE: (i32, i32) = (18 * TILE_PX, 16 * TILE_PX + TILE_PX / 2);
const SEATS: [(i32, i32); 2] = [
    (16 * TILE_PX + TILE_PX / 2, TABLE.1),
    (19 * TILE_PX + TILE_PX / 2, TABLE.1),
];
const BOWL: (i32, i32) = (14 * TILE_PX + TILE_PX / 2, 16 * TILE_PX + TILE_PX / 2);
// The smallest viewport leaves the west home intact. The default 480-pixel
// viewport reveals the east house at the same scale and world coordinates.
const YARD_ORIGIN: (f32, f32) = (72.0, 47.0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerrainStamp {
    origin: (i32, i32),
    size: (i32, i32),
    revision: u32,
    village: bool,
}

const GRASS_TONE: [u8; 3] = [129, 149, 91];
const PATH_TONE: [u8; 3] = [171, 149, 112];

pub use crate::viewport::source_size as premium_source_size;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub follow: Option<u8>,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            x: YARD_ORIGIN.0,
            y: YARD_ORIGIN.1,
            follow: None,
        }
    }
}
impl Camera {
    pub fn set_follow(&mut self, follow: Option<u8>) {
        self.follow = follow.filter(|index| *index < 3);
    }

    pub fn pan_to(&mut self, x: f32, y: f32, size: (i32, i32)) {
        self.set_follow(None);
        (self.x, self.y) = (x, y);
        self.clamp(size);
    }

    pub fn migrate_legacy_view(mut self) -> Self {
        // The old default's follow bit was also used by the explicit menu
        // selection. Migrate only the original crop, preserving that choice.
        if (self.x, self.y, self.follow) == (56.0, 64.0, Some(0)) {
            (self.x, self.y) = YARD_ORIGIN;
        }
        self
    }

    pub fn clamp(&mut self, size: (i32, i32)) {
        if !self.x.is_finite() || !self.y.is_finite() {
            *self = Self::default();
        }
        self.x = self
            .x
            .clamp(0.0, (MAP_WIDTH as i32 * TILE_PX - size.0).max(0) as f32);
        self.y = self
            .y
            .clamp(0.0, (MAP_HEIGHT as i32 * TILE_PX - size.1).max(0) as f32);
        self.follow = self.follow.filter(|i| *i < 3);
    }

    pub fn clamp_in_world(&mut self, world: &LivingWorld, size: (i32, i32)) {
        let (width, height) = world.map_extent();
        if !self.x.is_finite() || !self.y.is_finite() {
            *self = Self::default();
        }
        self.x = self
            .x
            .clamp(0.0, (i32::from(width) * TILE_PX - size.0).max(0) as f32);
        self.y = self
            .y
            .clamp(0.0, (i32::from(height) * TILE_PX - size.1).max(0) as f32);
        self.follow = self.follow.filter(|i| *i < 3);
    }

    pub fn pan_to_in_world(&mut self, x: f32, y: f32, world: &LivingWorld, size: (i32, i32)) {
        self.follow = None;
        (self.x, self.y) = (x, y);
        self.clamp_in_world(world, size);
    }

    pub fn fit_viewport(&mut self, size: (i32, i32)) {
        if (self.x, self.y, self.follow) == (YARD_ORIGIN.0, YARD_ORIGIN.1, None) {
            self.y = Self::yard_top(size.1);
        }
        self.clamp(size);
    }

    fn yard_top(height: i32) -> f32 {
        YARD_ORIGIN.1 + (72 - height).max(0) as f32
    }

    pub fn update(&mut self, world: &LivingWorld, size: (i32, i32)) {
        if let Some(actor) = self.follow.and_then(|i| world.actors().get(i as usize)) {
            let p = actor.position;
            let margin_x = 24.0_f32.min(size.0 as f32 / 4.0);
            let margin_y = 28.0_f32.min(size.1 as f32 / 3.0);
            let tx = if p.x < self.x + margin_x {
                p.x - margin_x
            } else if p.x > self.x + size.0 as f32 - margin_x {
                p.x - size.0 as f32 + margin_x
            } else {
                self.x
            };
            let ty = if p.y < self.y + margin_y {
                p.y - margin_y
            } else if p.y > self.y + size.1 as f32 - margin_y {
                p.y - size.1 as f32 + margin_y
            } else {
                self.y
            };
            // Keep the opening composition as long as the actor is in its
            // inner region. Crossing either edge resumes ordinary following.
            let short = size.1 < 72;
            let yard_top = Self::yard_top(size.1);
            let ground_margin = if short { 2.0 } else { 7.0 };
            let in_yard = (YARD_ORIGIN.0 + 12.0..YARD_ORIGIN.0 + size.0 as f32 - 12.0)
                .contains(&p.x)
                && (yard_top + 28.0..yard_top + size.1 as f32 - ground_margin).contains(&p.y);
            let (tx, ty) = if let Some((x, y)) = Self::house_framing(world, actor, size) {
                (x, y)
            } else if in_yard {
                (YARD_ORIGIN.0, yard_top)
            } else {
                (tx, ty)
            };
            self.x += (tx - self.x).clamp(-3.0, 3.0);
            self.y += (ty - self.y).clamp(-3.0, 3.0);
        }
        self.clamp(size);
    }

    fn house_framing(world: &LivingWorld, actor: &Actor, size: (i32, i32)) -> Option<(f32, f32)> {
        let home = world.home_of(actor.kind)?;
        let site = world
            .housing_sites()
            .find(|site| site.occupied && site.position == home)?;
        let left = site.lot_origin.x;
        let top = site.lot_origin.y;
        let [w, h] = site.footprint.map(|side| f32::from(side) * TILE_PX as f32);
        if !(left - 16.0..left + w + 16.0).contains(&actor.position.x)
            || !(top - 4.0..top + h + 16.0).contains(&actor.position.y)
        {
            return None;
        }
        let x = left + w / 2.0 - size.0 as f32 / 2.0;
        let y = top - (size.1 as f32 - h - 10.0).clamp(0.0, 28.0);
        Some((x, y))
    }
}

struct CachedPage {
    pack: PackId,
    page: LocalId,
    pixels: Vec<u8>,
    touched: u64,
}

/// Move the intersection of two terrain windows in-place. Paint only texels
/// newly revealed by the camera. Dynamic objects never enter this cache.
fn shift_terrain(pixels: &mut [u8], (width, height): (i32, i32), [dx, dy]: [i32; 2]) -> Option<[usize; 4]> {
    let (left, top) = ((-dx).max(0), (-dy).max(0));
    let (right, bottom) = (width - dx.max(0), height - dy.max(0));
    if right <= left || bottom <= top {
        return None;
    }
    for offset in 0..bottom - top {
        let y = if dy < 0 { bottom - 1 - offset } else { top + offset };
        let src = (((y + dy) * width + left + dx) * 4) as usize;
        let len = ((right - left) * 4) as usize;
        let dst = ((y * width + left) * 4) as usize;
        pixels.copy_within(src..src + len, dst);
    }
    Some([left as usize, top as usize, right as usize, bottom as usize])
}

pub struct LivingView {
    pixels: Vec<u8>,
    previous: Vec<u8>,
    width: i32,
    height: i32,
    density: i32,
    art: Option<ArtAtlas>,
    wildlife_art: Option<ArtAtlas>,
    village_art: Option<ArtAtlas>,
    village_index: Option<VillageIndex>,
    companionship_rewards: u16,
    village_public_corner: Option<(crate::map_data::TilePos, u8)>,
    indoor_reveals: [Option<(i32, i32)>; 2],
    awake_screen_feet: [Option<(i32, i32)>; 2],
    visible_motion: bool,
    vacant_residences: u64,
    village_handheld: [Option<crate::visitors::VisitorHeld>; 2],
    terrain: Vec<Ground>,
    yard_surfaces: Vec<Option<YardSurface>>,
    terrain_pixels: Vec<u8>,
    terrain_stamp: Option<TerrainStamp>,
    cache: Vec<CachedPage>,
    serial: u64,
}
enum Drawable<'a> {
    Building(&'a Building),
    Tree(usize, &'a Tree),
    Decoration(Decoration),
    Table,
    Bowl,
    Actor(usize),
    Visitor(&'a VisitorVisual),
    Development(Development),
    Housing(HousingView),
    MarketFittings(MarketLayout),
    FormerHome(FormerHomeUse, Point),
    InstalledFixture(FixtureStatus, Point),
    FixtureWork(ImprovementOrder, Point),
    Plugin(&'a FrameCommand),
}

#[derive(Clone, Copy)]
struct Decoration {
    key: &'static str,
    x: i32,
    y: i32,
    flip: bool,
    scale: Scale,
}

// A sheltered western patch, reeds near the pond, and the orchard understorey.
// The table, front yards and the meadow between the plots remain open.
const YARD_CLUSTERS: [(&str, i32, i32, i32); 15] = [
    ("bush_herb", 59, 105, 4),
    ("rocks_moss", 64, 101, 3),
    ("flowers_wild", 55, 107, 2),
    ("bush_herb", 82, 73, 4),
    ("bush_berry", 88, 72, 3),
    ("rocks_moss", 110, 178, 4),
    ("bush_herb", 114, 177, 3),
    ("flowers_wild", 105, 179, 2),
    ("bush_herb", 76, 176, 4),
    ("bush_herb", 74, 171, 2),
    ("rocks_moss", 250, 167, 3),
    ("bush_herb", 255, 169, 4),
    ("flowers_wild", 262, 172, 3),
    ("bush_berry", 278, 185, 4),
    ("bush_herb", 289, 190, 4),
];
impl Default for LivingView {
    fn default() -> Self {
        Self::new()
    }
}
impl LivingView {
    pub fn new() -> Self {
        Self {
            pixels: Vec::new(),
            previous: Vec::new(),
            width: 0,
            height: 0,
            density: 1,
            art: None,
            wildlife_art: None,
            village_art: None,
            village_index: None,
            companionship_rewards: 0,
            village_public_corner: None,
            indoor_reveals: [None; 2],
            awake_screen_feet: [None; 2],
            visible_motion: false,
            vacant_residences: 0,
            village_handheld: [None; 2],
            terrain: Vec::new(),
            yard_surfaces: Vec::new(),
            terrain_pixels: Vec::new(),
            terrain_stamp: None,
            cache: Vec::new(),
            serial: 0,
        }
    }
    pub fn clear_plugin_cache(&mut self) {
        self.cache.clear();
    }
    pub fn cache_bytes(&self) -> usize {
        self.cache.iter().map(|p| p.pixels.len()).sum()
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn visible_motion(&self) -> bool {
        self.visible_motion
    }

    pub fn set_companionship_rewards(&mut self, mask: u16) {
        self.companionship_rewards = mask;
    }

    pub fn set_village_public_corner(&mut self, corner: Option<(crate::map_data::TilePos, u8)>) {
        self.village_public_corner = corner;
    }

    pub fn set_resident_presence(
        &mut self,
        residents: impl Iterator<Item = crate::residents::ResidentStatus>,
    ) {
        let mut offered = 0_u64;
        let mut occupied = 0_u64;
        let buildings = crate::map_data::WILLOW_VILLAGE.buildings;
        for resident in residents {
            if let Ok(i) = buildings.binary_search_by_key(&resident.home, |b| b.id)
                && let Some(bit) = 1_u64.checked_shl(i as u32)
            {
                offered |= bit;
                if resident.settled {
                    occupied |= bit;
                }
            }
        }
        self.vacant_residences = offered & !occupied;
    }

    pub fn set_village_household_payload(
        &mut self,
        held: [Option<crate::visitors::VisitorHeld>; 2],
    ) {
        self.village_handheld = held;
    }

    pub fn set_premium_assets(
        &mut self,
        index: &[u8],
        pages: &'static [&'static [u8]],
    ) -> Result<(), &'static str> {
        self.art = Some(ArtAtlas::load(index, pages)?);
        self.terrain_stamp = None;
        self.village_index = None;
        Ok(())
    }

    pub fn enable_builtin_premium(&mut self) -> Result<(), &'static str> {
        let (core, wildlife, village) = art_assets::builtin()?;
        self.art = Some(core);
        self.wildlife_art = Some(wildlife);
        self.village_art = Some(village);
        self.terrain_stamp = None;
        self.village_index = None;
        Ok(())
    }
    /// Returns a logical-pixel damage rectangle. Offscreen sprites never load their pages.
    pub fn render(
        &mut self,
        world: &LivingWorld,
        camera: &Camera,
        size: (i32, i32),
        commands: &[FrameCommand],
        store: Option<&PluginStore>,
    ) -> Option<[i32; 4]> {
        self.render_living(world, camera, size, commands, store, &[])
    }

    pub fn render_living(
        &mut self,
        world: &LivingWorld,
        camera: &Camera,
        size: (i32, i32),
        commands: &[FrameCommand],
        store: Option<&PluginStore>,
        visitors: &[VisitorVisual],
    ) -> Option<[i32; 4]> {
        let (w, h) = (size.0.clamp(1, 1280), size.1.clamp(1, 720));
        self.render_at_density(
            world,
            camera,
            (w, h),
            1,
            commands,
            store,
            visitors,
            &[],
            &[],
        )
    }

    /// Produce a two-texel-per-world-pixel BGRA frame and damage in source texels.
    /// `physical_size` is the destination window size; viewport owns the fixed magnification.
    pub fn render_premium(
        &mut self,
        world: &LivingWorld,
        camera: &Camera,
        physical_size: (i32, i32),
        commands: &[FrameCommand],
        store: Option<&PluginStore>,
        visitors: &[VisitorVisual],
    ) -> Option<[i32; 4]> {
        self.render_at_density(
            world,
            camera,
            premium_source_size(physical_size),
            2,
            commands,
            store,
            visitors,
            &[],
            &[],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_village(
        &mut self,
        world: &LivingWorld,
        camera: &Camera,
        physical_size: (i32, i32),
        commands: &[FrameCommand],
        store: Option<&PluginStore>,
        visitors: &[VisitorVisual],
        wildlife: &[WildlifeVisual],
        stalls: &[StallVisual],
    ) -> Option<[i32; 4]> {
        self.render_at_density(
            world,
            camera,
            premium_source_size(physical_size),
            2,
            commands,
            store,
            visitors,
            wildlife,
            stalls,
        )
    }

    /// Render the padded native village source. The presenter crops the
    /// physical-pixel phase; atlas sampling stays at an integer magnification.
    #[allow(clippy::too_many_arguments)]
    pub fn render_village_overscan(
        &mut self,
        world: &LivingWorld,
        origin: crate::viewport::PixelOrigin,
        physical_size: (i32, i32),
        commands: &[FrameCommand],
        store: Option<&PluginStore>,
        visitors: &[VisitorVisual],
        wildlife: &[WildlifeVisual],
        stalls: &[StallVisual],
    ) -> Option<[i32; 4]> {
        self.render_at_density(
            world,
            &Camera { x: origin.world.0 as f32, y: origin.world.1 as f32, follow: None },
            crate::viewport::overscan_source_size(physical_size),
            2,
            commands,
            store,
            visitors,
            wildlife,
            stalls,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_at_density(
        &mut self,
        world: &LivingWorld,
        camera: &Camera,
        (w, h): (i32, i32),
        density: i32,
        commands: &[FrameCommand],
        store: Option<&PluginStore>,
        visitors: &[VisitorVisual],
        wildlife: &[WildlifeVisual],
        stalls: &[StallVisual],
    ) -> Option<[i32; 4]> {
        let resized = (w, h, density) != (self.width, self.height, self.density);
        let bytes = (w * h * 4) as usize;
        let shrunk = bytes < self.pixels.len();
        self.width = w;
        self.height = h;
        self.density = density;
        self.visible_motion = false;
        self.serial = self.serial.wrapping_add(1);
        for surface in [&mut self.pixels, &mut self.previous] {
            surface.resize(bytes, 0);
            if shrunk && surface.capacity() > bytes * 2 {
                surface.shrink_to_fit();
            }
        }
        std::mem::swap(&mut self.pixels, &mut self.previous);
        let (cx, cy) = (camera.x.round() as i32, camera.y.round() as i32);
        if world.is_village() && density == 2 {
            let scrolled = self.terrain_stamp.is_some_and(|stamp| stamp.origin != (cx, cy));
            self.paint_village(world, (cx, cy), commands, store, visitors, wildlife, stalls);
            return self.damage(resized || scrolled);
        }
        if density == 2 {
            self.paint_premium_ground(world, (cx, cy));
        } else {
            self.terrain_stamp = None;
            self.terrain_pixels = Vec::new();
            self.paint_terrain(world, (cx, cy), None);
        }
        // Small fixed scenery is tied to world positions, independent of viewport dimensions.
        if density == 1 {
            for tx in (6..36).step_by(2) {
                self.rect(tx * 8 - cx, 10 * 8 - cy, 1, 4, [141, 98, 60]);
                self.rect(tx * 8 - cx, 10 * 8 + 1 - cy, 8, 1, [194, 148, 89]);
            }
        }
        for plot in world.plots() {
            if density == 2 {
                self.premium_plot(plot, cx, cy);
                continue;
            }
            let x = plot.position.x as i32 - cx;
            let y = plot.position.y as i32 - cy;
            self.rect(x - 4, y - 3, 8, 6, [118, 85, 52]);
            let color = match plot.stage {
                PlotStage::Grass => [128, 156, 79],
                PlotStage::Cleared => [165, 119, 72],
                _ => [146, 100, 58],
            };
            self.rect(x - 3, y - 2, 6, 4, color);
            if matches!(
                plot.stage,
                PlotStage::Tilled | PlotStage::Seed | PlotStage::Growing | PlotStage::Mature
            ) {
                self.rect(x - 2, y - 1, 1, 3, [96, 71, 49]);
                self.rect(x + 1, y - 1, 1, 3, [96, 71, 49]);
            }
            for dx in [-2, 1] {
                match plot.stage {
                    PlotStage::Seed => {
                        self.pixel(x + dx, y, [209, 170, 88]);
                    }
                    PlotStage::Growing | PlotStage::Mature => {
                        let tall = if plot.stage == PlotStage::Mature {
                            4
                        } else {
                            2
                        };
                        self.rect(x + dx, y - tall, 1, tall, [68, 119, 62]);
                        self.rect(x + dx - 1, y - tall, 3, 1, [107, 155, 62]);
                        if tall == 4 {
                            self.pixel(x + dx, y - 2, [232, 148, 67]);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut drawables: Vec<(i32, Drawable<'_>)> = Vec::with_capacity(128);
        let visitors = &visitors[..visitors.len().min(MAX_DRAWN_VISITORS)];
        for site in world
            .developments()
            .into_iter()
            .filter(|site| site.phase > 0)
        {
            let floor = matches!(site.kind, DevelopmentKind::Road | DevelopmentKind::Bridge);
            let depth = if floor {
                i32::MIN
            } else {
                site.position.y.round() as i32
            };
            drawables.push((depth, Drawable::Development(site)));
        }
        for b in world.buildings() {
            drawables.push((building_baseline(b), Drawable::Building(b)));
        }
        if density == 2 {
            for home in world.housing_sites() {
                drawables.push((home.position.y.round() as i32 - 4, Drawable::Housing(home)));
            }
            if let Some(market) = world.market_layout() {
                drawables.push((
                    market.counter.y.round() as i32,
                    Drawable::MarketFittings(market),
                ));
            }
            for (actor, building) in [ActorKind::Xiaohe, ActorKind::Ayu]
                .into_iter()
                .zip(world.buildings())
            {
                if let Some(use_kind) = world.previous_home_use(actor) {
                    drawables.push((
                        building.position.y.round() as i32,
                        Drawable::FormerHome(use_kind, building.position),
                    ));
                }
            }
        }
        for (index, t) in world.trees().iter().enumerate() {
            drawables.push((t.position.y as i32, Drawable::Tree(index, t)));
        }
        if density == 2 && self.art.is_some() {
            for (key, x, y, size) in YARD_CLUSTERS {
                if !matches!(
                    world.ground(x / TILE_PX, y / TILE_PX),
                    Ground::Grass | Ground::Rock
                ) {
                    continue;
                }
                drawables.push((
                    y,
                    Drawable::Decoration(Decoration {
                        key,
                        x,
                        y,
                        flip: terrain_hash(x, y) % 2 != 0,
                        scale: Scale::new(size, 6),
                    }),
                ));
            }
            drawables.push((
                143,
                Drawable::Decoration(Decoration {
                    key: "well_stone",
                    x: 240,
                    y: 143,
                    flip: false,
                    scale: Scale::IDENTITY,
                }),
            ));
            for (status, point) in world.building_fixtures() {
                drawables.push((
                    point.y.round() as i32,
                    Drawable::InstalledFixture(status, point),
                ));
            }
            if let Some((order, point)) = world.active_building_improvement() {
                drawables.push((point.y.round() as i32, Drawable::FixtureWork(order, point)));
            }
        }
        drawables.push((TABLE.1, Drawable::Table));
        drawables.push((BOWL.1, Drawable::Bowl));
        let commands = &commands[..commands.len().min(128)];
        for (i, a) in world.actors().iter().enumerate() {
            if !a.inside() && !commands.iter().any(|c| c.actor_override == Some(a.kind)) {
                drawables.push((a.position.y as i32, Drawable::Actor(i)));
            }
        }
        for c in commands {
            drawables.push((c.depth, Drawable::Plugin(c)));
        }
        for visitor in visitors {
            drawables.push((
                visitor.position.y.round() as i32,
                Drawable::Visitor(visitor),
            ));
        }
        drawables.sort_by_key(|d| d.0);
        for (_, d) in drawables {
            match d {
                Drawable::Building(b) => self.building(b, cx, cy, world.minute_of_day()),
                Drawable::Tree(index, t) => self.tree(
                    index,
                    t.position.x as i32 - cx,
                    t.position.y as i32 - cy,
                    t.stage,
                ),
                Drawable::Decoration(item) => {
                    let position = (item.x - cx, item.y - cy);
                    self.contact_shadow(position, (7, 3), 27);
                    self.premium_at_scale(
                        item.key,
                        (position.0 * 2, position.1 * 2),
                        item.flip,
                        item.scale,
                        Palette::Unchanged,
                    );
                }
                Drawable::Actor(i) => self.actor(&world.actors()[i], cx, cy, world.world_ms()),
                Drawable::Table => self.table(world, cx, cy),
                Drawable::Bowl => self.bowl(world, cx, cy),
                Drawable::Visitor(visitor) => self.visitor(visitor, cx, cy),
                Drawable::Development(site) => self.development(site, cx, cy),
                Drawable::Housing(home) => self.housing(home, cx, cy, world.minute_of_day()),
                Drawable::MarketFittings(market) => self.market_fittings(market, cx, cy),
                Drawable::FormerHome(use_kind, point) => {
                    self.premium_frame(
                        use_kind.asset_name(),
                        (
                            point.x.round() as i32 - cx - 8,
                            point.y.round() as i32 - cy - 3,
                        ),
                        false,
                        Palette::Unchanged,
                    );
                }
                Drawable::InstalledFixture(status, point) => {
                    self.premium_frame(
                        status.fixture.asset_name(),
                        (
                            point.x.round() as i32 - cx + 9,
                            point.y.round() as i32 - cy - 2,
                        ),
                        false,
                        Palette::Unchanged,
                    );
                }
                Drawable::FixtureWork(order, point) => {
                    let phase = if order.phase == 0 {
                        "scaffold_foundation"
                    } else {
                        "scaffold_frame"
                    };
                    let position = (
                        point.x.round() as i32 - cx - 10,
                        point.y.round() as i32 - cy,
                    );
                    if !self.premium_at_scale(
                        phase,
                        (position.0 * 2, position.1 * 2),
                        false,
                        Scale::new(2, 3),
                        Palette::Unchanged,
                    ) {
                        self.premium_frame("logs", position, false, Palette::Unchanged);
                    }
                }
                Drawable::Plugin(command) => {
                    if let Some(store) = store {
                        self.plugin(command, cx, cy, store)
                    }
                }
            }
        }
        // A single scene-wide tint, after depth composition, also covers new content.
        let minute = world.minute_of_day();
        let light = if !(300..1200).contains(&minute) {
            [70u16, 83, 119]
        } else if !(360..1140).contains(&minute) {
            [99, 91, 92]
        } else {
            [100, 100, 100]
        };
        if light != [100; 3] {
            for pixel in self.pixels.chunks_exact_mut(4) {
                for channel in 0..3 {
                    pixel[channel] =
                        (u16::from(pixel[channel]) * light[2 - channel] / 100).min(255) as u8;
                }
            }
        }
        self.precipitation(world.weather(), world.world_ms(), cx, cy);
        self.visitor_labels(visitors, cx, cy);
        if density == 2 {
            self.clip_world_edge((cx, cy));
        }
        // Resize affordance is always inside the viewport; the world itself is never stretched.
        if density == 1 {
            for d in [3, 6, 9] {
                for n in 0..d {
                    self.pixel(w - 2 - n, h - 2 - d + n, [241, 226, 181]);
                }
            }
        }
        self.damage(resized)
    }

    fn damage(&self, resized: bool) -> Option<[i32; 4]> {
        let (w, h) = (self.width, self.height);
        if resized {
            return Some([0, 0, w, h]);
        }
        let mut rect = [w, h, 0, 0];
        for (i, (a, b)) in self
            .pixels
            .chunks_exact(4)
            .zip(self.previous.chunks_exact(4))
            .enumerate()
        {
            if a != b {
                let x = i as i32 % w;
                let y = i as i32 / w;
                rect[0] = rect[0].min(x);
                rect[1] = rect[1].min(y);
                rect[2] = rect[2].max(x + 1);
                rect[3] = rect[3].max(y + 1);
            }
        }
        (rect[2] > rect[0]).then_some(rect)
    }

    fn paint_premium_ground(&mut self, world: &LivingWorld, origin: (i32, i32)) {
        let stamp = TerrainStamp {
            origin,
            size: (self.width, self.height),
            revision: world.terrain_revision(),
            village: world.is_village(),
        };
        let bytes = self.pixels.len();
        if bytes > TERRAIN_CACHE_LIMIT {
            self.terrain_stamp = None;
            self.terrain_pixels = Vec::new();
            self.paint_terrain(world, origin, None);
        } else {
            if self.terrain_stamp != Some(stamp) {
                let prior = self.terrain_stamp.filter(|prior| {
                    (prior.size, prior.revision, prior.village) == (stamp.size, stamp.revision, stamp.village)
                });
                self.terrain_pixels.resize(bytes, 0);
                if self.terrain_pixels.capacity() > bytes * 2
                    || self.terrain_pixels.capacity() > TERRAIN_CACHE_LIMIT
                {
                    self.terrain_pixels.shrink_to_fit();
                }
                std::mem::swap(&mut self.pixels, &mut self.terrain_pixels);
                let kept = prior.and_then(|prior| shift_terrain(&mut self.pixels, stamp.size, [
                    (origin.0 - prior.origin.0) * self.density,
                    (origin.1 - prior.origin.1) * self.density,
                ]));
                self.paint_terrain(world, origin, kept);
                std::mem::swap(&mut self.pixels, &mut self.terrain_pixels);
                self.terrain_stamp = Some(stamp);
            }
            self.pixels.copy_from_slice(&self.terrain_pixels);
        }
        self.paint_ripples(
            origin,
            (world.world_ms() / 100_000) as i32,
            world.is_village(),
        );
    }

    fn paint_ripples(&mut self, (cx, cy): (i32, i32), pulse: i32, village: bool) {
        let tile = TILE_PX * 2;
        let (left, top) = (cx * 2, cy * 2);
        let (right, bottom) = (left + self.width, top + self.height);
        let first_tx = left.div_euclid(tile) - 1;
        let first_ty = top.div_euclid(tile) - 1;
        let last_tx = (right - 1).div_euclid(tile) + 1;
        let stride = (last_tx - first_tx + 1) as usize;
        let last_ty = (bottom - 1).div_euclid(tile);
        for ty in first_ty + 1..=last_ty {
            let y_start = (ty * tile).max(top);
            let y_end = ((ty + 1) * tile).min(bottom);
            let y_start = y_start + (-y_start).rem_euclid(13);
            let line = (ty - first_ty) as usize * stride;
            for tx in first_tx + 1..last_tx {
                let cell = line + (tx - first_tx) as usize;
                if self.terrain[cell] != Ground::Water {
                    continue;
                }
                let x_start = (tx * tile).max(left);
                let x_end = ((tx + 1) * tile).min(right);
                for y in (y_start..y_end).step_by(13) {
                    for x in x_start..x_end {
                        if !water_ripple(x, y, pulse)
                            || !village && (18..22).contains(&ty) && pond_bank_depth(x, y) < 5
                            || terrain_edges(&self.terrain, cell, stride, Ground::Water, x, y)
                                < edge_jitter(x, y) + 5
                        {
                            continue;
                        }
                        let offset = (((y - top) * self.width + x - left) * 4) as usize;
                        let pixel = &mut self.pixels[offset..offset + 4];
                        let coverage = u16::from(pixel[3]);
                        for (dst, color) in pixel[..3].iter_mut().zip([164_u16, 175, 122]) {
                            let accent = color * coverage / 255;
                            *dst = ((u16::from(*dst) * 2 + accent) / 3) as u8;
                        }
                    }
                }
            }
        }
    }

    fn paint_terrain(&mut self, world: &LivingWorld, (cx, cy): (i32, i32), kept: Option<[usize; 4]>) {
        let density = self.density;
        let village = world.is_village();
        let (map_width, map_height) = world.map_extent();
        let last_x = cx + (self.width - 1) / density;
        let last_y = cy + (self.height - 1) / density;
        let padding = i32::from(density == 2);
        let first_tx = cx.div_euclid(TILE_PX) - padding;
        let first_ty = cy.div_euclid(TILE_PX) - padding;
        let last_tx = last_x.div_euclid(TILE_PX) + padding;
        let last_ty = last_y.div_euclid(TILE_PX) + padding;
        let tile_columns = (last_tx - first_tx + 1) as usize;
        self.terrain.clear();
        self.yard_surfaces.clear();
        let mut village_tiles = Vec::new();
        let phases = if village {
            Some(world.phase_mask())
        } else {
            None
        };
        let market_courts = (village && density == 2).then(|| {
            (
                world.village_building_active("stall_produce"),
                world.village_building_active("stall_fish"),
            )
        });
        let (market_lane, market_stage) = world.market_construction_paving();
        let mut public_paving = Vec::new();
        for ty in first_ty..=last_ty {
            for tx in first_tx..=last_tx {
                let tile = phases.and_then(|phase| {
                    crate::map_data::WILLOW_VILLAGE
                        .tile(crate::map_data::TilePos::new(tx as u16, ty as u16), phase)
                });
                let ground =
                    if tile.is_some_and(|t| t.terrain == crate::map_data::Terrain::Boundary) {
                        Ground::Grass
                    } else if tile.is_some_and(|tile| {
                        matches!(tile.terrain, crate::map_data::Terrain::Grass)
                            && tile.region.is_some_and(|r| r.id == "quarry")
                    }) && tx > 160
                        && (ty < 12 || ty > 24)
                        && terrain_hash(tx / 3, ty / 3) % 7 < 2
                    {
                        Ground::Rock
                    } else {
                        world.ground(tx, ty)
                    };
                self.terrain.push(ground);
                if density == 2 {
                    self.yard_surfaces.push(world.yard_surface(tx, ty));
                }
                if village {
                    village_tiles.push(tile);
                    let at = crate::map_data::TilePos::new(tx as u16, ty as u16);
                    public_paving.push(match market_stage {
                        2 if market_lane
                            .get(..3)
                            .is_some_and(|first| first.contains(&at)) =>
                        {
                            1
                        }
                        3 if market_lane.contains(&at) => 2,
                        _ => 0,
                    });
                }
            }
        }
        let pulse = (world.world_ms() / 100_000) as i32;
        let art = self.art.as_ref().filter(|_| density == 2);
        let textures = [
            "ground_grass",
            "ground_path",
            "ground_soil",
            "ground_water",
            "ground_rock",
            "ground_water_shallow",
        ]
        .map(|key| art.and_then(|atlas| atlas.frame(key)));
        let [left, top, right, bottom] = kept.unwrap_or([0; 4]);
        for (y, row) in self
            .pixels
            .chunks_exact_mut(self.width as usize * 4)
            .enumerate()
        {
            let wy = cy + y as i32 / density;
            let ty = wy.div_euclid(TILE_PX);
            let line = (ty - first_ty) as usize * tile_columns;
            let kept_columns = if (top..bottom).contains(&y) { left..right } else { 0..0 };
            let (first, rest) = row.split_at_mut(kept_columns.start * 4);
            let (_, last) = rest.split_at_mut((kept_columns.end - kept_columns.start) * 4);
            let first = first.chunks_exact_mut(4).enumerate();
            let last = (kept_columns.end..).zip(last.chunks_exact_mut(4));
            for (x, pixel) in first.chain(last) {
                let wx = cx + x as i32 / density;
                let tx = wx.div_euclid(TILE_PX);
                let cell = line + (tx - first_tx) as usize;
                let ground = if density == 2 && self.terrain[cell] == Ground::Soil {
                    Ground::Grass
                } else {
                    self.terrain[cell]
                };
                let (art_x, art_y) = (cx * density + x as i32, cy * density + y as i32);
                let surface = self.yard_surfaces.get(cell).copied().flatten();
                if density == 2
                    && (!(0..i32::from(map_width) * TILE_PX).contains(&wx)
                        || !(0..i32::from(map_height) * TILE_PX).contains(&wy))
                {
                    pixel.fill(0);
                    continue;
                }
                let (mut visible_ground, mut shore, mut edge) = if density == 2
                    && matches!(ground, Ground::Path | Ground::Water | Ground::Rock)
                {
                    let side =
                        terrain_edges(&self.terrain, cell, tile_columns, ground, art_x, art_y);
                    let raised = match ground {
                        Ground::Rock => 4,
                        Ground::Water => 3,
                        _ => 2,
                    };
                    let lip = edge_jitter(art_x, art_y) + raised;
                    if side < lip {
                        (Ground::Grass, ground == Ground::Water, side - lip)
                    } else {
                        (
                            ground,
                            ground == Ground::Water && side < lip + 4,
                            side - lip,
                        )
                    }
                } else {
                    (ground, false, 32)
                };
                if village && density == 2 && matches!(visible_ground, Ground::Path) {
                    let paving = public_paving[cell];
                    if paving != 0 {
                        let [r, g, b] = village_render::public_paving(art_x, art_y, edge, paving);
                        pixel.copy_from_slice(&[b, g, r, 255]);
                        continue;
                    }
                }
                let apron = if let Some(courts) = market_courts {
                    village_tiles[cell]
                        .and_then(|info| info.region)
                        .and_then(|zone| {
                            village_render::decorated_ground(
                                zone.id, ground, art_x, art_y, edge, courts,
                            )
                        })
                } else {
                    None
                };
                if let Some(([r, g, b], 255)) = apron {
                    pixel.copy_from_slice(&[b, g, r, 255]);
                    continue;
                }
                if village && density == 2 && ground != Ground::Building {
                    let geometry = village_tiles[cell];
                    if geometry.is_some_and(|tile| tile.terrain == crate::map_data::Terrain::Bridge)
                    {
                        let rgb = village_render::bridge_planks(art_x, art_y);
                        pixel.copy_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
                        continue;
                    }
                    let adjacent_water = ground == Ground::Water
                        || (cell % tile_columns > 0 && self.terrain[cell - 1] == Ground::Water)
                        || self.terrain.get(cell + 1) == Some(&Ground::Water)
                        || (cell >= tile_columns
                            && self.terrain[cell - tile_columns] == Ground::Water)
                        || self.terrain.get(cell + tile_columns) == Some(&Ground::Water);
                    if adjacent_water
                        && let Some(bank) = village_render::village_water_depth(art_x, art_y)
                    {
                        edge = bank;
                        if ground != Ground::Path || bank >= 0 {
                            visible_ground = if bank >= 0 {
                                Ground::Water
                            } else {
                                Ground::Grass
                            };
                            shore = (-5..6).contains(&bank);
                        }
                    }
                }
                if !village && density == 2 && ground == Ground::Water && (18..22).contains(&ty) {
                    // The west pond is a basin within its original blocked cells.
                    // Left/right shelves differ in shape and stay on the water side.
                    let bank = pond_bank_depth(art_x, art_y);
                    edge = edge.min(bank);
                    visible_ground = if edge < 0 {
                        Ground::Grass
                    } else {
                        Ground::Water
                    };
                    shore = edge < 5;
                }
                if !village
                    && density == 2
                    && ground == Ground::Rock
                    && (32..36).contains(&tx)
                    && (5..10).contains(&ty)
                {
                    edge = edge.min(quarry_depth(art_x, art_y));
                    if edge < 0 {
                        visible_ground = Ground::Grass;
                    }
                }
                let dry_basin = density == 2
                    && ground == Ground::Water
                    && visible_ground == Ground::Grass
                    && edge <= -5;
                if village
                    && density == 2
                    && ground == Ground::Path
                    && visible_ground == Ground::Path
                    && village_tiles[cell]
                        .and_then(|info| info.region)
                        .is_some_and(|r| matches!(r.id, "market" | "craft"))
                {
                    let rgb = village_render::market_pavers(art_x, art_y, edge);
                    pixel.copy_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
                    continue;
                }
                let tint = surface.and_then(|surface| {
                    if visible_ground == Ground::Grass && edge < 0 {
                        return None;
                    }
                    let local = edge_distance(
                        &self.yard_surfaces,
                        cell,
                        tile_columns,
                        Some(surface),
                        art_x,
                        art_y,
                        9,
                    );
                    let weight = (local + 2 - edge_jitter(art_x, art_y)).clamp(0, 8);
                    Some((yard_surface_color(surface), weight))
                });
                let texture_index = match visible_ground {
                    Ground::Path => 1,
                    Ground::Soil => 2,
                    Ground::Water => 3,
                    Ground::Rock if ty != 16 || !(17..19).contains(&tx) => 4,
                    _ => 0,
                };
                let texture = if shore && visible_ground == Ground::Water {
                    textures[5]
                } else {
                    textures[texture_index].filter(|_| !shore || dry_basin)
                };
                if let Some((frame, page)) = texture {
                    let [left, top, width, height] = frame.rect.map(i32::from);
                    let sx = left + mirrored_tile_coordinate(art_x, width);
                    let sy = top + mirrored_tile_coordinate(art_y, height);
                    let start = ((sy * 256 + sx) * 4) as usize;
                    let mix = match visible_ground {
                        Ground::Grass => {
                            let patch = terrain_hash(art_x.div_euclid(197), art_y.div_euclid(149));
                            34 + (patch % 18) as u16
                        }
                        Ground::Path => 84,
                        Ground::Soil => 182,
                        Ground::Water => 176,
                        Ground::Rock => 96,
                        _ => 160,
                    };
                    let tone_ground = if dry_basin { Ground::Grass } else { ground };
                    let [r, g, b] = premium_ground_tone(
                        ground_surface_tint(
                            premium_flat_color(visible_ground, (tx, ty), shore && !dry_basin),
                            tint,
                        ),
                        tone_ground,
                        (art_x, art_y),
                        edge,
                    );
                    let base = [b, g, r, 255];
                    let mix = if matches!(ground, Ground::Path | Ground::Rock) {
                        mix * (edge + 2).clamp(0, 7) as u16 / 7
                    } else {
                        mix
                    };
                    for ((channel, value), base) in
                        pixel.iter_mut().zip(&page[start..start + 4]).zip(base)
                    {
                        let blended =
                            (u16::from(*value) * mix + u16::from(base) * (255 - mix)) / 255;
                        *channel = blended as u8;
                    }
                } else {
                    let [r, g, b] = if density == 2 {
                        let tone_ground = if dry_basin { Ground::Grass } else { ground };
                        premium_ground_tone(
                            ground_surface_tint(
                                premium_terrain_color(
                                    visible_ground,
                                    (art_x, art_y),
                                    (tx, ty),
                                    shore && !dry_basin,
                                ),
                                tint,
                            ),
                            tone_ground,
                            (art_x, art_y),
                            edge,
                        )
                    } else {
                        terrain_color(ground, wx, wy, tx, ty, pulse)
                    };
                    for (channel, value) in pixel.iter_mut().zip([b, g, r, 255]) {
                        *channel = value;
                    }
                }
                if let Some(([r, g, b], weight)) = apron {
                    let blend = u16::from(weight);
                    for (dst, ink) in pixel.iter_mut().zip([b, g, r]) {
                        *dst = ((u16::from(*dst) * (255 - blend) + u16::from(ink) * blend) / 255)
                            as u8;
                    }
                }
            }
        }
        if density == 2 {
            for tx in [12, 24] {
                for ty in 14..=15 {
                    if world.yard_surface(tx, ty) == Some(YardSurface::Doorstep) {
                        let x = tx * TILE_PX + TILE_PX / 2 - cx;
                        let y = ty * TILE_PX + if ty == 14 { 5 } else { 2 } - cy;
                        if !self.premium_frame(
                            "ground_door_stone",
                            (x, y),
                            tx == 24,
                            Palette::Unchanged,
                        ) {
                            self.rect(x - 3, y + 1, 6, 2, [123, 120, 95]);
                            self.rect(x - 3, y, 5, 2, [174, 164, 132]);
                            self.rect(x - 2, y - 1, 4, 1, [181, 173, 140]);
                        }
                    }
                }
            }
        }
    }

    fn clip_world_edge(&mut self, (cx, cy): (i32, i32)) {
        let density = self.density;
        let world_width = MAP_WIDTH as i32 * TILE_PX;
        let world_height = MAP_HEIGHT as i32 * TILE_PX;
        if cx >= world_width
            || cy >= world_height
            || cx <= -(self.width + density - 1) / density
            || cy <= -(self.height + density - 1) / density
        {
            self.pixels.fill(0);
            return;
        }
        let left = (-cx * density).clamp(0, self.width);
        let top = (-cy * density).clamp(0, self.height);
        let right = ((world_width - cx) * density).clamp(0, self.width);
        let bottom = ((world_height - cy) * density).clamp(0, self.height);
        if (left, top, right, bottom) == (0, 0, self.width, self.height) {
            return;
        }
        let pitch = self.width as usize * 4;
        self.pixels[..top as usize * pitch].fill(0);
        self.pixels[bottom as usize * pitch..].fill(0);
        for row in
            self.pixels[top as usize * pitch..bottom as usize * pitch].chunks_exact_mut(pitch)
        {
            row[..left as usize * 4].fill(0);
            row[right as usize * 4..].fill(0);
        }
    }

    fn premium_plot(&mut self, plot: &Plot, cx: i32, cy: i32) {
        let x = plot.position.x.round() as i32 - cx;
        let y = plot.position.y.round() as i32 - cy;
        let whole_bed = match plot.stage {
            PlotStage::Grass => "plot_grass",
            PlotStage::Cleared => "plot_cleared",
            PlotStage::Tilled => "plot_tilled",
            PlotStage::Seed => "plot_seed",
            PlotStage::Growing => "plot_growing",
            PlotStage::Mature => "plot_mature",
        };
        if self.premium_frame(whole_bed, (x, y), false, Palette::Unchanged) {
            return;
        }
        if plot.stage == PlotStage::Grass {
            return;
        }
        if plot.stage == PlotStage::Cleared
            || !self.premium_frame("plot_tilled", (x, y), false, Palette::Unchanged)
        {
            let texture = self
                .art
                .as_ref()
                .and_then(|atlas| atlas.frame("ground_soil"));
            for dy in -8i32..=7 {
                let sy = y * 2 + dy;
                if !(0..self.height).contains(&sy) {
                    continue;
                }
                for dx in -8i32..=7 {
                    let sx = x * 2 + dx;
                    let jitter = edge_jitter(sx + cx * 2, sy + cy * 2);
                    if !(0..self.width).contains(&sx) || dx * dx + dy * dy * 2 > 110 - jitter * 8 {
                        continue;
                    }
                    let rgb: [u8; 3] = if plot.stage != PlotStage::Cleared
                        && (-3..=3).contains(&dx)
                        && dy.rem_euclid(5) == 0
                    {
                        [113, 77, 58]
                    } else {
                        [145, 104, 74]
                    };
                    let at = ((sy * self.width + sx) * 4) as usize;
                    let mut bgra = [rgb[2], rgb[1], rgb[0], 255];
                    if let Some((frame, page)) = texture {
                        let [tx, ty, w, h] = frame.rect.map(i32::from);
                        let px = tx + mirrored_tile_coordinate(sx + cx * 2, w);
                        let py = ty + mirrored_tile_coordinate(sy + cy * 2, h);
                        let texel = ((py * 256 + px) * 4) as usize;
                        for (base, tint) in bgra[..3].iter_mut().zip(&page[texel..texel + 3]) {
                            *base = ((u16::from(*base) * 2 + u16::from(*tint)) / 3) as u8;
                        }
                    }
                    self.pixels[at..at + 4].copy_from_slice(&bgra);
                }
            }
        }
        let crop = match plot.stage {
            PlotStage::Seed => "crop_seed",
            PlotStage::Growing => "crop_growing",
            PlotStage::Mature => "crop_mature",
            _ => return,
        };
        if self.premium_frame(crop, (x, y), false, Palette::Unchanged) {
            return;
        }
        for (dx, dy) in [(-2, 0), (2, -1)] {
            let (x, y) = (x + dx, y + dy);
            if plot.stage == PlotStage::Seed {
                self.pixel(x, y, [211, 177, 109]);
            } else {
                let tall = if plot.stage == PlotStage::Mature {
                    3
                } else {
                    2
                };
                self.rect(x, y - tall, 1, tall, [65, 109, 70]);
                self.rect(x - 1, y - tall, 3, 1, [100, 148, 75]);
                if tall == 3 {
                    self.pixel(x + 1, y - 1, [224, 150, 69]);
                }
            }
        }
    }

    fn building(&mut self, building: &Building, cx: i32, cy: i32, minute: u16) {
        let stage = match building.progress {
            0 if building.level > 1 => 3,
            0 => 0,
            1..=65 => 1,
            _ => 2,
        };
        let (name, scale) = match building.kind {
            BuildingKind::WestHouse => ("west_house", Scale::new(32, 42)),
            BuildingKind::EastHouse => ("east_house", Scale::new(32, 42)),
            BuildingKind::Kitchen => ("kitchen", Scale::new(32, 49)),
            BuildingKind::Storehouse => ("storehouse", Scale::new(32, 49)),
            BuildingKind::Workshop => ("workshop", Scale::new(16, 49)),
        };
        let key = building_frame_key(name, stage);
        let position = (
            building.position.x.round() as i32 - cx,
            building_baseline(building) - cy,
        );
        if self.density == 2 {
            let width = if building.kind == BuildingKind::Workshop {
                16
            } else {
                24
            };
            self.contact_shadow((position.0 + 4, position.1 - 2), (width, 6), 44);
            self.contact_shadow((position.0 + 1, position.1 - 1), (width - 7, 3), 36);
        }
        let paint = self.roof_palette(!(330..1190).contains(&minute), position.1 * 2);
        if self.density == 2 && building.level >= 2 {
            for level in (2..=building.level).rev() {
                let finished = if level == 2 {
                    building_frame_key(name, 3)
                } else {
                    format!("{name}_level_{level}")
                };
                if self.premium_frame(&finished, position, false, paint) {
                    if building.progress > 0 {
                        let scaffolding = (position.0 - 10, position.1 + 2);
                        if !self.premium_at_scale(
                            "structure_upgrade_scaffold",
                            (scaffolding.0 * 2, scaffolding.1 * 2),
                            false,
                            Scale::new(3, 4),
                            Palette::Unchanged,
                        ) {
                            self.premium_frame("logs", scaffolding, false, Palette::Unchanged);
                        }
                    }
                    return;
                }
            }
        }
        if self.density == 2 && stage > 0 && !self.premium_frame_exists(&key) {
            let base = building_frame_key(name, 0);
            if self.premium_frame(&base, position, false, paint) {
                let phase = match (stage, building.level) {
                    (1 | 2, 2..) => "structure_upgrade_scaffold",
                    (1, _) => "scaffold_foundation",
                    (2, _) => "scaffold_frame",
                    _ => "scaffold_done",
                };
                if !self.premium_frame(
                    phase,
                    (position.0 - 10, position.1 + 2),
                    false,
                    Palette::Unchanged,
                ) && stage < 3
                {
                    self.premium_frame(
                        "logs",
                        (position.0 - 10, position.1 + 2),
                        false,
                        Palette::Unchanged,
                    );
                }
                return;
            }
        }
        self.core_with(&key, position, false, scale, paint);
    }

    fn housing(&mut self, home: HousingView, cx: i32, cy: i32, minute: u16) {
        let left = home.lot_origin.x.round() as i32 - cx;
        let top = home.lot_origin.y.round() as i32 - cy;
        let [width, height] = home.footprint.map(|tiles| i32::from(tiles) * TILE_PX);
        let door = (
            home.position.x.round() as i32 - cx,
            home.position.y.round() as i32 - cy - TILE_PX / 2,
        );
        let key = format!("{}_{}", home.kind.asset_name(), home.phase.asset_name());
        if home.phase >= HousingPhase::Frame {
            self.contact_shadow((door.0 + 4, door.1 - 2), (width / 2 + 4, 6), 46);
        }
        let paint = self.roof_palette(home.occupied && !(330..1190).contains(&minute), door.1 * 2);
        if self.premium_frame(&key, door, false, paint) {
            return;
        }

        // Show the actual bounded site while a dedicated atlas frame is absent.
        for (x, y) in [
            (left, top),
            (left + width - 2, top),
            (left, top + height - 2),
            (left + width - 2, top + height - 2),
        ] {
            self.rect(x, y, 2, 2, [158, 124, 84]);
        }
        let prop = match home.phase {
            HousingPhase::Clear => "logs",
            HousingPhase::Foundation => "scaffold_foundation",
            HousingPhase::Frame => "scaffold_frame",
            HousingPhase::Roof | HousingPhase::FitOut => "structure_upgrade_scaffold",
            HousingPhase::MoveIn => return,
        };
        self.premium_frame(prop, (door.0 - 9, door.1), false, Palette::Unchanged);
    }

    fn market_fittings(&mut self, layout: MarketLayout, cx: i32, cy: i32) {
        let position = |point: Point| (point.x.round() as i32 - cx, point.y.round() as i32 - cy);
        self.premium_frame(
            "market_counter",
            position(layout.counter),
            false,
            Palette::Unchanged,
        );
        for (index, bin) in layout.bins.into_iter().enumerate() {
            let key = if index == 0 {
                "market_bin_left"
            } else {
                "market_bin_right"
            };
            if !self.premium_frame(key, position(bin), false, Palette::Unchanged) {
                self.premium_frame("market_bin", position(bin), index != 0, Palette::Unchanged);
            }
        }
    }

    fn table(&mut self, world: &LivingWorld, cx: i32, cy: i32) {
        let (x, y) = (TABLE.0 - cx, TABLE.1 - cy);
        if self.density == 2 {
            self.contact_shadow((x + 2, y + 1), (12, 4), 40);
        }
        self.core("shared_table", x, y, false);
        for (seat_x, seat_y) in SEATS {
            self.core("shared_stool", seat_x - cx, seat_y - cy, false);
        }
        for (i, actor) in world.actors()[..2].iter().enumerate() {
            if actor.pose == Pose::Eat
                && actor.carried == Some(Item::Food)
                && (
                    actor.position.x.round() as i32,
                    actor.position.y.round() as i32,
                ) == SEATS[i]
            {
                let place = (x + if i == 0 { -4 } else { 4 }, y - 7);
                if self.premium_at_scale(
                    "prop_meal",
                    (place.0 * 2, place.1 * 2),
                    false,
                    Scale::new(2, 3),
                    Palette::Unchanged,
                ) {
                    continue;
                }
                self.core_with(
                    "prop_meal",
                    place,
                    false,
                    Scale::new(2, 5),
                    Palette::Unchanged,
                );
            }
        }
    }

    fn bowl(&mut self, world: &LivingWorld, cx: i32, cy: i32) {
        let (x, y) = (BOWL.0 - cx, BOWL.1 - cy);
        let wet = world.bowl_water() > 0;
        let key = if wet { "prop_bowl_full" } else { "prop_bowl" };
        if self.premium_frame(key, (x, y), false, Palette::Unchanged) {
            return;
        }
        self.core_with(
            "prop_bowl",
            (x, y - 1),
            false,
            Scale::new(1, 2),
            Palette::Unchanged,
        );
        if wet {
            self.rect(x - 2, y - 1, 4, 1, [100, 192, 208]);
        }
    }
    fn actor(&mut self, actor: &Actor, cx: i32, cy: i32, time: u64) {
        self.actor_at(actor, cx, cy, time, None);
    }

    fn actor_at(
        &mut self,
        actor: &Actor,
        cx: i32,
        cy: i32,
        time: u64,
        village: Option<&LivingWorld>,
    ) {
        let x = actor.position.x.round() as i32 - cx;
        let y = actor.position.y.round() as i32 - cy;
        if x < -48
            || y < -12
            || x * self.density > self.width + 48 * self.density
            || y * self.density > self.height + 48 * self.density
        {
            return;
        }
        let frame = match (village.is_some(), actor.walking_progress(time)) {
            (true, Some(progress)) => ((progress * 3.0).floor() as usize).min(2),
            _ => (time / 8000 % 3) as usize,
        };
        if self.density == 2 {
            self.contact_shadow((x + 1, y), (5, 2), 36);
        }
        if actor.kind == ActorKind::Orange {
            let (action, data) = match actor.pose {
                Pose::Walk | Pose::Carry => ("walk", CAT_WALK),
                Pose::Sleep => ("sleep", CAT_SLEEP),
                Pose::Rest => ("rest", CAT_SLEEP),
                Pose::Eat => ("eat", CAT_EAT),
                Pose::Drink => ("drink", CAT_EAT),
                _ => ("idle", CAT_IDLE),
            };
            let bowl = self.density == 2 && actor.pose == Pose::Drink;
            let drawing_x = if bowl && village.is_none() { x + 5 } else { x };
            if self.density == 2
                && self.premium_frame(
                    &format!("cat_{action}_{frame}"),
                    (drawing_x, y),
                    bowl || actor.facing_left,
                    Palette::Unchanged,
                )
            {
                return;
            }
            let len = 48 * 32 * 4;
            let n = data.len() / len;
            let offset = (time / 8000 % n as u64) as usize * len;
            let sprite = Sprite::new(
                [0, 0, 48, 32],
                [24, 29],
                (x, y),
                actor.facing_left,
                Scale::new(2, 3),
            )
            .at_density(self.density);
            blit_into(
                &mut self.pixels,
                (self.width, self.height),
                &data[offset..offset + len],
                48,
                sprite,
                Palette::Unchanged,
            );
            return;
        }
        let prefix = if actor.kind == ActorKind::Xiaohe {
            "female"
        } else {
            "male"
        };
        let (mut action, frame) = match actor.fishing_result_frame(time) {
            Some((true, frame)) => ("fish_success", frame),
            Some((false, frame)) => ("fish_fail", frame),
            None => (actor_clip(actor), frame),
        };
        let moving_box = actor.has_moving_box();
        let outside_payload = village.and_then(|_| {
            self.village_handheld
                .get(actor.kind as usize)
                .copied()
                .flatten()
        });
        let hand_goods = actor.carried.filter(|_| actor.carried_amount() > 0);
        if moving_box && actor.pose == Pose::Walk {
            action = "carry";
        }
        if village.is_some() && actor.fishing_result().is_none() {
            action = village_actor_clip(actor);
            if (hand_goods.is_some() || outside_payload.is_some()) && actor.pose == Pose::Walk {
                action = "carry";
            }
        }
        if self.density == 2 && actor.pose == Pose::Work {
            let specific = match (village.is_some(), actor.activity) {
                (false, Activity::GetWater) => Some("get_water"),
                (false, Activity::TendGarden) => Some("tend_garden"),
                (false, Activity::Roadwork) => Some("roadwork"),
                _ => None,
            };
            if let Some(specific) = specific
                && self.premium_frame_exists(&format!("{prefix}_{specific}_{frame}"))
            {
                action = specific;
            }
        }
        let seat_pos = (
            actor.position.x.round() as i32,
            actor.position.y.round() as i32,
        );
        let seated = matches!(actor.pose, Pose::Eat | Pose::Rest)
            && if village.is_some() {
                village_render::at_village_seat(seat_pos)
            } else {
                SEATS.contains(&seat_pos)
            };
        let facing_left =
            if self.density == 2 && actor.pose == Pose::Work && actor.activity == Activity::Cook {
                true
            } else if self.density == 2
                && actor.pose == Pose::Work
                && actor.activity == Activity::FillBowl
            {
                false
            } else if self.density == 2
                && actor.activity == Activity::Fish
                && (actor.pose == Pose::Work || actor.fishing_result().is_some())
            {
                false
            } else if seated {
                actor.kind == ActorKind::Ayu
            } else {
                actor.facing_left
            };
        let actor_scale = if self.density == 2 {
            Scale::new(3, 4)
        } else {
            Scale::IDENTITY
        };
        let prop_scale = if self.density == 2 {
            Scale::new(3, 5)
        } else {
            Scale::IDENTITY
        };
        let frame_key = format!("{prefix}_{action}_{frame}");
        let feet = (x, if seated { y - 2 } else { y });
        let hand = self.premium_hand_at_scale(&frame_key, feet, facing_left, actor_scale);
        let bobber = format!("fish_bobber_{frame}");
        let cast = self.density == 2
            && actor.pose == Pose::Work
            && actor.activity == Activity::Fish
            && (village.is_none() || actor.tool == Some(Tool::FishingRod));
        let float = cast
            .then(|| {
                if let Some(world) = village {
                    village_render::visible_fishing_float(world, actor.position)
                        .map(|p| (p.x as i32 - cx, p.y as i32 - cy))
                } else {
                    Some((x + 7, y + 16))
                }
            })
            .flatten();
        if village.is_none()
            && self.density == 2
            && actor.pose == Pose::Work
            && actor.activity == Activity::Cook
        {
            self.premium_frame("prop_stove", (x - 9, y - 1), false, Palette::Unchanged);
        }
        if let (Some((fx, fy)), Some(rod_tip)) = (float, hand) {
            self.fishing_line(rod_tip, (fx * 2, fy * 2));
        }
        let paint = clothing_palette(actor.kind, actor.outfit());
        if village.is_none()
            || !self.premium_actor_body(
                prefix,
                action,
                &frame_key,
                feet,
                facing_left,
                actor_scale,
                paint,
            )
        {
            self.core_with(&frame_key, feet, facing_left, actor_scale, paint);
        }
        if let (Some((fx, fy)), Some(_)) = (float, hand)
            && !self.premium_frame(&bobber, (fx, fy), false, Palette::Unchanged)
        {
            self.pixel(fx, fy, [195, 92, 71]);
        }
        if moving_box {
            let hand = hand.unwrap_or((x * self.density, (y - 7) * self.density));
            self.premium_at_texel("housing_moving_box", hand, facing_left, Palette::Unchanged);
        }
        if !moving_box
            && actor.pose != Pose::Sleep
            && let Some((key, scale)) = outside_payload.and_then(village_render::held_asset)
        {
            let hand =
                hand.unwrap_or((x * self.density, (y - actor_scale.apply(8)) * self.density));
            self.core_at_texel(key, hand, facing_left, scale, Palette::Unchanged);
        }
        if actor.fishing_result().is_none() && !moving_box && actor.pose != Pose::Sleep {
            let legacy_carry = matches!(
                actor.pose,
                Pose::Walk | Pose::Carry | Pose::Idle | Pose::Rest
            );
            let empty_work_hands = village.is_some()
                && actor.pose == Pose::Work
                && !matches!(
                    action,
                    "axe" | "hammer" | "fish" | "hoe_clear" | "hoe_till" | "water" | "harvest"
                );
            if let Some(tool) = actor.tool.filter(|_| legacy_carry || empty_work_hands) {
                let key = match tool {
                    Tool::Hoe => "prop_hoe",
                    Tool::Axe => "prop_axe",
                    Tool::WateringCan => "prop_water_can",
                    Tool::Hammer => "prop_hammer",
                    Tool::HarvestBasket => "prop_basket",
                    Tool::FishingRod => "prop_rod",
                };
                let hand_x = actor_scale.apply(5);
                let hand_y = actor_scale.apply(if actor.pose == Pose::Rest { 0 } else { 10 });
                let hand = hand.unwrap_or((
                    (x + if facing_left { -hand_x } else { hand_x }) * self.density,
                    (y - hand_y) * self.density,
                ));
                self.core_at_texel(key, hand, facing_left, prop_scale, Palette::Unchanged);
            }
            if let Some(item) = hand_goods.filter(|_| {
                outside_payload.is_none()
                    && (legacy_carry
                        || empty_work_hands
                        || village.is_some() && actor.activity == Activity::Trade)
            }) {
                let key = village_render::item_asset(item);
                let carried = if actor.tool.is_some() { None } else { hand };
                self.core_at_texel(
                    key,
                    carried
                        .unwrap_or((x * self.density, (y - actor_scale.apply(8)) * self.density)),
                    false,
                    prop_scale,
                    Palette::Unchanged,
                );
            }
        }
    }

    fn fishing_line(&mut self, rod: (i32, i32), float: (i32, i32)) {
        let delta = (float.0 - rod.0, float.1 - rod.1);
        let steps = delta.0.abs().max(delta.1.abs());
        for i in 0..steps {
            let x = rod.0 + i * delta.0 / steps;
            let y = rod.1 + i * delta.1 / steps;
            if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
                let at = ((y * self.width + x) * 4) as usize;
                let pixel = &mut self.pixels[at..at + 4];
                for (dst, ink) in pixel[..3].iter_mut().zip([155_u16, 169, 174]) {
                    *dst = ((u16::from(*dst) + ink) / 2) as u8;
                }
                pixel[3] = 128 + pixel[3] / 2;
            }
        }
    }
    fn tree(&mut self, index: usize, x: i32, y: i32, stage: TreeStage) {
        let varieties = [
            "tree_oak",
            "tree_birch",
            "tree_apple",
            "tree_cherry",
            "tree_maple",
        ];
        let key = match stage {
            TreeStage::Standing => varieties[index % varieties.len()],
            _ => "tree_stump",
        };
        if self.density == 2 && self.premium_frame_exists(key) {
            let standing = stage == TreeStage::Standing;
            self.contact_shadow((x + 2, y + 1), (if standing { 13 } else { 6 }, 4), 44);
            let scale = if standing {
                Scale::IDENTITY
            } else {
                Scale::new(3, 5)
            };
            self.premium_at_scale(key, (x * 2, y * 2), false, scale, Palette::Unchanged);
            if stage == TreeStage::Felled {
                self.premium_at_scale(
                    "logs",
                    ((x + 7) * 2, (y + 1) * 2),
                    false,
                    scale,
                    Palette::Unchanged,
                );
            }
            return;
        }
        if x < -20 || x > self.width + 20 || y < 0 || y > self.height + 35 {
            return;
        }
        if stage != TreeStage::Standing {
            self.rect(x - 3, y - 2, 6, 3, [113, 77, 48]);
            if stage == TreeStage::Felled {
                self.rect(x + 4, y - 2, 12, 3, [164, 114, 62]);
            }
            return;
        }
        self.rect(x - 2, y - 15, 4, 15, [112, 75, 48]);
        self.rect(x, y - 14, 1, 14, [161, 112, 64]);
        for yy in -30i32..-7 {
            let half = if yy < -20 {
                (yy + 31) * 2 / 3 + 2
            } else {
                (8 - (yy + 18).abs() / 2).max(3)
            };
            self.rect(
                x - half,
                y + yy,
                half * 2 + 1,
                1,
                if yy % 5 < -2 {
                    [83, 127, 69]
                } else {
                    [98, 148, 73]
                },
            );
        }
        self.rect(x - 5, y - 25, 5, 2, [143, 174, 86]);
        self.rect(x + 2, y - 17, 4, 2, [159, 182, 89]);
    }

    fn development(&mut self, site: Development, cx: i32, cy: i32) {
        let name = match site.kind {
            DevelopmentKind::Road => "road",
            DevelopmentKind::Bridge => "bridge",
            DevelopmentKind::Market => "market",
            DevelopmentKind::Stall => "stall",
            DevelopmentKind::Guesthouse => "guesthouse",
            DevelopmentKind::Depot => "depot",
            DevelopmentKind::Well => "well",
            DevelopmentKind::Garden => "garden",
        };
        let stage = if site.complete { 3 } else { site.phase.min(2) };
        let key = format!("{name}_{stage}");
        let position = (
            site.position.x.round() as i32 - cx,
            site.position.y.round() as i32 - cy,
        );
        if self.density == 2
            && !matches!(site.kind, DevelopmentKind::Road | DevelopmentKind::Bridge)
        {
            self.contact_shadow((position.0 + 3, position.1), (17, 5), 37);
        }
        if self.density == 2
            && let Some((frame, _)) = self.art.as_ref().and_then(|atlas| atlas.frame(&key))
        {
            // Public building fronts may overhang the square. Limit each side
            // to the interval to its neighboring public site, preserving aspect.
            let sides = match site.kind {
                DevelopmentKind::Market => Some((48, 32)),
                DevelopmentKind::Stall => Some((15, 40)),
                DevelopmentKind::Guesthouse => Some((39, 39)),
                DevelopmentKind::Depot => Some((39, 25)),
                DevelopmentKind::Well => Some((22, 38)),
                DevelopmentKind::Garden => Some((40, 48)),
                _ => None,
            };
            let mut scale = Scale::IDENTITY;
            if let Some((left, right)) = sides {
                for (limit, actual) in [
                    (left, i32::from(frame.anchor[0])),
                    (right, i32::from(frame.rect[2] - frame.anchor[0])),
                ] {
                    if limit * scale.denominator < actual * scale.numerator {
                        scale = Scale::new(limit, actual);
                    }
                }
            }
            self.premium_at_scale(
                &key,
                (position.0 * 2, position.1 * 2),
                false,
                scale,
                Palette::Unchanged,
            );
            return;
        }
        baked_frame(
            &mut self.pixels,
            (self.width, self.height),
            &DEVELOPMENT_INDEX,
            &DEVELOPMENT_PAGES,
            &key,
            position,
            false,
            self.density,
        );
    }

    fn visitor(&mut self, visitor: &VisitorVisual, cx: i32, cy: i32) {
        let x = visitor.position.x.round() as i32 - cx;
        let y = visitor.position.y.round() as i32 - cy;
        if x < -40
            || x * self.density >= self.width + 40 * self.density
            || y < -5
            || y * self.density >= self.height + 36 * self.density
        {
            return;
        }
        let key = format!(
            "{}_{}_{}",
            visitor.profession.asset_name(),
            visitor.pose.asset_name(),
            visitor.animation_frame % 3
        );
        baked_frame(
            &mut self.pixels,
            (self.width, self.height),
            &VISITOR_INDEX,
            &VISITOR_PAGES,
            &key,
            (x, y),
            visitor.facing_left,
            self.density,
        );
    }

    fn visitor_labels(&mut self, visitors: &[VisitorVisual], cx: i32, cy: i32) {
        let width = (self.width + self.density - 1) / self.density;
        let height = (self.height + self.density - 1) / self.density;
        let mut labels = Vec::with_capacity(MAX_DRAWN_VISITORS);
        for visitor in visitors {
            let x = visitor.position.x.round() as i32 - cx;
            let y = visitor.position.y.round() as i32 - cy;
            if x < -12 || x >= width + 12 || y < 0 || y >= height + 26 {
                continue;
            }
            let key = format!("{}_label", visitor.profession.asset_name());
            if let Some(label) = VISITOR_INDEX
                .frames
                .iter()
                .find(|frame| frame.id.as_str() == key)
            {
                let half = label.anchor[0].max(label.rect[2] - label.anchor[0]);
                labels.push((x, y, i32::from(half), label));
            }
        }
        labels.sort_by_key(|&(x, y, _, _)| (y, x));
        let mut occupied: Vec<(i32, i32, i32)> = Vec::with_capacity(MAX_DRAWN_VISITORS);
        for (x, feet, half, label) in labels {
            let x = x.clamp(
                half.min(width / 2),
                (width - half - 1).max(half.min(width / 2)),
            );
            let desired = (feet - 28).max(12);
            let mut y = desired;
            while occupied.iter().any(|&(other_x, other_y, other_half)| {
                (x - other_x).abs() <= half + other_half + 1 && (y - other_y).abs() < 13
            }) {
                y -= 13;
            }
            if y < 12 {
                y = desired + 13;
                while occupied.iter().any(|&(other_x, other_y, other_half)| {
                    (x - other_x).abs() <= half + other_half + 1 && (y - other_y).abs() < 13
                }) {
                    y += 13;
                }
            }
            if y >= height {
                continue;
            }
            occupied.push((x, y, half));
            let sprite = Sprite::new(label.rect, label.anchor, (x, y), false, Scale::IDENTITY)
                .at_density(self.density);
            if let Some(page) = VISITOR_INDEX
                .pages
                .iter()
                .position(|page| page.id == label.page)
            {
                blit_into(
                    &mut self.pixels,
                    (self.width, self.height),
                    VISITOR_PAGES[page],
                    128,
                    sprite,
                    Palette::Unchanged,
                );
            }
        }
    }

    fn precipitation(&mut self, weather: YardWeather, time: u64, cx: i32, cy: i32) {
        let (spacing, travel, rain) = match weather {
            YardWeather::Rain => (17, 6, true),
            YardWeather::Snow => (23, 2, false),
            _ => return,
        };
        let tick = (time / 6_000) as u32;
        // Deterministic world-aligned cells. A wider camera reveals additional drops.
        let (width, height) = (
            (self.width + self.density - 1) / self.density,
            (self.height + self.density - 1) / self.density,
        );
        for gy in (cy - spacing).div_euclid(spacing)..=(cy + height).div_euclid(spacing) {
            for gx in (cx - spacing).div_euclid(spacing)..=(cx + width).div_euclid(spacing) {
                let seed =
                    (gx as u32).wrapping_mul(0x9E37_79B9) ^ (gy as u32).wrapping_mul(0x45D9_F3B);
                let jump = tick.wrapping_add(seed >> 8).wrapping_mul(travel);
                let x = gx * spacing + (seed % spacing as u32) as i32 - cx;
                let y = gy * spacing + (jump % spacing as u32) as i32 - cy;
                if rain {
                    self.blend_weather(x, y, [142, 186, 211], 140);
                    self.blend_weather(x - 1, y + 1, [142, 186, 211], 102);
                    self.blend_weather(x - 1, y + 2, [142, 186, 211], 65);
                } else {
                    self.blend_weather(
                        x + ((tick / 3).wrapping_add(seed) % 3) as i32 - 1,
                        y,
                        [230, 237, 230],
                        155,
                    );
                }
            }
        }
    }

    fn blend_weather(&mut self, x: i32, y: i32, rgb: [u8; 3], alpha: u16) {
        let d = self.density;
        for yy in (y * d).max(0)..((y + 1) * d).min(self.height) {
            for xx in (x * d).max(0)..((x + 1) * d).min(self.width) {
                let start = ((yy * self.width + xx) * 4) as usize;
                let coverage = u16::from(self.pixels[start + 3]);
                for (dst, color) in self.pixels[start..start + 3]
                    .iter_mut()
                    .zip(rgb.into_iter().rev())
                {
                    let color = u16::from(color) * coverage / 255;
                    *dst = ((color * alpha + u16::from(*dst) * (255 - alpha)) / 255) as u8;
                }
            }
        }
    }
    fn core(&mut self, key: &str, x: i32, y: i32, flip: bool) {
        self.core_with(key, (x, y), flip, Scale::IDENTITY, Palette::Unchanged);
    }

    fn core_with(
        &mut self,
        key: &str,
        position: (i32, i32),
        flip: bool,
        scale: Scale,
        palette: Palette,
    ) {
        self.core_at_texel(
            key,
            (position.0 * self.density, position.1 * self.density),
            flip,
            scale,
            palette,
        );
    }

    fn core_at_texel(
        &mut self,
        key: &str,
        position: (i32, i32),
        flip: bool,
        scale: Scale,
        palette: Palette,
    ) {
        if self.premium_at_scale(key, position, flip, scale, palette) {
            return;
        }
        let Some(frame) = CORE_INDEX
            .frames
            .iter()
            .find(|f| f.id.as_str() == key)
            .cloned()
        else {
            return;
        };
        let Some(page) = CORE_INDEX.pages.iter().position(|p| p.id == frame.page) else {
            return;
        };
        let mut sprite =
            Sprite::new(frame.rect, frame.anchor, (0, 0), flip, scale).at_density(self.density);
        sprite.left += position.0;
        sprite.top += position.1;
        blit_into(
            &mut self.pixels,
            (self.width, self.height),
            PAGES[page],
            128,
            sprite,
            palette,
        );
    }

    fn premium_frame(
        &mut self,
        key: &str,
        position: (i32, i32),
        flip: bool,
        palette: Palette,
    ) -> bool {
        self.premium_at_texel(key, (position.0 * 2, position.1 * 2), flip, palette)
    }

    fn premium_at_texel(
        &mut self,
        key: &str,
        position: (i32, i32),
        flip: bool,
        palette: Palette,
    ) -> bool {
        self.premium_at_scale(key, position, flip, Scale::IDENTITY, palette)
    }

    fn premium_at_scale(
        &mut self,
        key: &str,
        position: (i32, i32),
        flip: bool,
        scale: Scale,
        palette: Palette,
    ) -> bool {
        if self.density != 2 {
            return false;
        }
        let Some(atlas) = self.art.as_ref() else {
            return false;
        };
        let Some(frame) = atlas.metadata(key) else {
            return false;
        };
        let sprite = Sprite::new(frame.rect, frame.anchor, position, flip, scale);
        if !sprite.visible((self.width, self.height)) {
            return true;
        }
        blit_into(
            &mut self.pixels,
            (self.width, self.height),
            atlas.page(frame),
            256,
            sprite,
            palette,
        );
        true
    }

    fn premium_frame_exists(&self, key: &str) -> bool {
        self.density == 2
            && self
                .art
                .as_ref()
                .is_some_and(|atlas| atlas.metadata(key).is_some())
    }

    fn premium_actor_body(
        &mut self,
        prefix: &str,
        action: &str,
        body_key: &str,
        feet: (i32, i32),
        facing_left: bool,
        scale: Scale,
        palette: Palette,
    ) -> bool {
        let Some(atlas) = self.art.as_ref().filter(|_| self.density == 2) else {
            return false;
        };
        let Some(body) = atlas.metadata(body_key) else {
            return false;
        };
        let origin = (feet.0 * 2, feet.1 * 2);
        let body_sprite = Sprite::new(body.rect, body.anchor, origin, facing_left, scale);
        let head_action = match action {
            "idle" | "walk" | "carry" => "idle",
            "sleep" | "fish_success" | "fish_fail" => {
                self.premium_at_scale(body_key, origin, facing_left, scale, palette);
                return true;
            }
            _ => action,
        };
        let key = format!("{prefix}_{head_action}_0");
        let Some(head) = atlas.metadata(&key) else {
            return false;
        };
        let head_sprite = Sprite::new(head.rect, head.anchor, origin, facing_left, scale);
        let join = origin.1 - scale.apply(22);
        let size = (self.width, self.height);
        for (frame, sprite, rows) in [
            (head, head_sprite, 0..join),
            (body, body_sprite, join..self.height),
        ] {
            if sprite.visible(size) {
                blit_rows(
                    &mut self.pixels,
                    size,
                    atlas.page(frame),
                    256,
                    sprite,
                    palette,
                    rows,
                );
            }
        }
        true
    }

    fn premium_hand(&self, key: &str, feet: (i32, i32), flip: bool) -> Option<(i32, i32)> {
        self.premium_hand_at_scale(key, feet, flip, Scale::IDENTITY)
    }

    fn premium_hand_at_scale(
        &self,
        key: &str,
        feet: (i32, i32),
        flip: bool,
        scale: Scale,
    ) -> Option<(i32, i32)> {
        if self.density != 2 {
            return None;
        }
        let frame = self.art.as_ref()?.metadata(key)?;
        let [x, y] = frame.hand_anchor?;
        let dx = scale.apply(x) - scale.apply(frame.anchor[0]);
        let dy = scale.apply(y) - scale.apply(frame.anchor[1]);
        Some((feet.0 * 2 + if flip { -dx } else { dx }, feet.1 * 2 + dy))
    }
    fn plugin(&mut self, c: &FrameCommand, cx: i32, cy: i32, store: &PluginStore) {
        let Some(pack) = store.package(&c.pack_id) else {
            return;
        };
        let Some(frame) = pack.pack.frame(&c.frame_id).cloned() else {
            return;
        };
        let position = (c.point.x.round() as i32 - cx, c.point.y.round() as i32 - cy);
        let sprite = Sprite::new(frame.rect, frame.anchor, position, c.flip, Scale::IDENTITY)
            .at_density(self.density);
        if !sprite.visible((self.width, self.height)) {
            return;
        }
        let slot = if let Some(i) = self
            .cache
            .iter()
            .position(|p| p.pack == c.pack_id && p.page == frame.page)
        {
            i
        } else {
            if self.cache.len() >= PLUGIN_PAGE_LIMIT {
                let index = self
                    .cache
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, p)| p.touched)
                    .unwrap()
                    .0;
                self.cache.remove(index);
            }
            let Ok(pixels) = pack.pack.read_page(&frame.page) else {
                return;
            };
            self.cache.push(CachedPage {
                pack: c.pack_id.clone(),
                page: frame.page.clone(),
                pixels,
                touched: self.serial,
            });
            self.cache.len() - 1
        };
        self.cache[slot].touched = self.serial;
        // Borrow distinct fields rather than duplicating the decoded page for every sprite.
        blit_into(
            &mut self.pixels,
            (self.width, self.height),
            &self.cache[slot].pixels,
            128,
            sprite,
            Palette::Unchanged,
        );
    }
    fn pixel(&mut self, x: i32, y: i32, c: [u8; 3]) {
        let d = self.density;
        for yy in (y * d).max(0)..((y + 1) * d).min(self.height) {
            for xx in (x * d).max(0)..((x + 1) * d).min(self.width) {
                let i = ((yy * self.width + xx) * 4) as usize;
                self.pixels[i..i + 4].copy_from_slice(&[c[2], c[1], c[0], 255]);
            }
        }
    }

    fn contact_shadow(&mut self, center: (i32, i32), radii: (i32, i32), opacity: u16) {
        let (x, y) = (center.0 * 2, center.1 * 2);
        let (rx, ry) = (radii.0 * 2, radii.1 * 2);
        let r2 = (rx * ry).pow(2);
        for sy in (y - ry).max(0)..(y + ry).min(self.height) {
            let dy2 = ((sy - y) * rx).pow(2);
            for sx in (x - rx).max(0)..(x + rx).min(self.width) {
                let distance = ((sx - x) * ry).pow(2) + dy2;
                if distance >= r2 {
                    continue;
                }
                let weight = if distance * 3 < r2 {
                    opacity
                } else {
                    opacity / 2
                };
                let at = ((sy * self.width + sx) * 4) as usize;
                let pixel = &mut self.pixels[at..at + 4];
                let alpha = u16::from(pixel[3]);
                for (dst, color) in pixel[..3].iter_mut().zip([61, 63, 56]) {
                    let overlay = color * alpha / 255;
                    *dst = ((u16::from(*dst) * (255 - weight) + overlay * weight) / 255) as u8;
                }
            }
        }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: [u8; 3]) {
        for yy in y.max(0)..(y + h).min((self.height + self.density - 1) / self.density) {
            for xx in x.max(0)..(x + w).min((self.width + self.density - 1) / self.density) {
                self.pixel(xx, yy, c);
            }
        }
    }
}
fn building_baseline(building: &Building) -> i32 {
    let floor = building.position.y.round() as i32 - TILE_PX / 2;
    // The kitchen has a walkable working aisle south of its two equipment rows.
    if building.kind == BuildingKind::Kitchen {
        floor - TILE_PX
    } else {
        floor
    }
}

fn baked_frame(
    out: &mut [u8],
    size: (i32, i32),
    index: &AssetIndex,
    pages: &[&[u8]],
    key: &str,
    position: (i32, i32),
    flip: bool,
    density: i32,
) {
    let Some(frame) = index.frames.iter().find(|frame| frame.id.as_str() == key) else {
        return;
    };
    let sprite =
        Sprite::new(frame.rect, frame.anchor, position, flip, Scale::IDENTITY).at_density(density);
    if !sprite.visible(size) {
        return;
    }
    let Some(page) = index
        .pages
        .iter()
        .position(|page| page.id == frame.page)
        .and_then(|i| pages.get(i))
    else {
        return;
    };
    blit_into(out, size, page, 128, sprite, Palette::Unchanged);
}

fn terrain_color(ground: Ground, wx: i32, wy: i32, tx: i32, ty: i32, pulse: i32) -> [u8; 3] {
    let hash = (wx.wrapping_mul(31) ^ wy.wrapping_mul(17) ^ tx.wrapping_mul(97)).unsigned_abs();
    match ground {
        Ground::Water if (wx + wy / 2 + pulse).rem_euclid(17) < 3 => [114, 186, 187],
        Ground::Water => [73, 145, 167],
        Ground::Path if hash % 13 < 2 => [195, 172, 128],
        Ground::Path => [218, 194, 146],
        Ground::Soil if wy % 4 == 0 => [126, 86, 57],
        Ground::Soil => [156, 111, 72],
        Ground::Rock if ty != 16 || !(17..19).contains(&tx) => {
            if hash % 7 < 2 {
                [142, 150, 133]
            } else {
                [166, 169, 144]
            }
        }
        _ if hash % 23 == 0 => [160, 185, 100],
        _ if hash % 11 == 0 => [123, 159, 82],
        _ => [143, 175, 94],
    }
}

fn terrain_hash(x: i32, y: i32) -> u32 {
    let seed = (x as u32).wrapping_mul(0x8DA6_B343) ^ (y as u32).wrapping_mul(0xD816_3841);
    (seed ^ (seed >> 16)).wrapping_mul(0x85EB_CA6B)
}

fn mirrored_tile_coordinate(coordinate: i32, length: i32) -> i32 {
    let sample = coordinate.rem_euclid(length * 2);
    sample.min(length * 2 - 1 - sample)
}

fn edge_jitter(x: i32, y: i32) -> i32 {
    let jitter = terrain_hash(x.div_euclid(5), y.div_euclid(5));
    (jitter % 3) as i32
}

fn water_ripple(x: i32, y: i32, pulse: i32) -> bool {
    y.rem_euclid(13) == 0 && (x + y / 4 + pulse * 2).rem_euclid(43) < 7
}

fn pond_bank_depth(x: i32, y: i32) -> i32 {
    let dx = x - 184;
    let dy = y - 320;
    let width = if dx > 0 { 35 } else { 34 };
    let height = if dy > 0 { 26 } else { 27 };
    let radius = width * height;
    let d2 = (dx * height).pow(2) + (dy * width).pow(2);
    (radius.pow(2) - d2) / (radius * 65) + edge_jitter(x, y) - 1
}

fn quarry_depth(x: i32, y: i32) -> i32 {
    let mut depth = -12;
    for (cx, cy, width, height) in [(534, 110, 20, 22), (551, 131, 19, 13), (532, 147, 13, 9)] {
        let dx = x - cx;
        let dy = y - cy;
        let radius = width * height;
        let d2 = (dx * height).pow(2) + (dy * width).pow(2);
        depth = depth.max((radius.pow(2) - d2) / (radius * 41));
    }
    depth + edge_jitter(x, y) - 1
}

fn premium_flat_color(ground: Ground, (tx, ty): (i32, i32), shore: bool) -> [u8; 3] {
    if shore {
        return if ground == Ground::Water {
            [115, 165, 132]
        } else {
            [134, 149, 103]
        };
    }
    match ground {
        Ground::Water => [79, 141, 151],
        Ground::Path => PATH_TONE,
        Ground::Soil => [151, 113, 79],
        Ground::Rock if ty != 16 || !(17..19).contains(&tx) => [148, 151, 122],
        _ => GRASS_TONE,
    }
}

fn yard_surface_color(surface: YardSurface) -> [u8; 3] {
    match surface {
        YardSurface::Doorstep => [163, 151, 124],
        YardSurface::Dining => [145, 147, 104],
        YardSurface::Loading => [156, 145, 116],
        YardSurface::Bank => [134, 146, 112],
        YardSurface::GardenAisle => [142, 147, 101],
    }
}

fn ground_surface_tint(base: [u8; 3], tint: Option<([u8; 3], i32)>) -> [u8; 3] {
    let Some((tint, weight)) = tint else {
        return base;
    };
    std::array::from_fn(|i| {
        ((i32::from(base[i]) * (8 - weight) + i32::from(tint[i]) * weight) / 8) as u8
    })
}

fn premium_ground_tone(rgb: [u8; 3], ground: Ground, (x, y): (i32, i32), edge: i32) -> [u8; 3] {
    if ground == Ground::Water {
        return rgb;
    }
    if matches!(ground, Ground::Path | Ground::Rock) {
        let weight = if edge < 0 { 0 } else { (edge + 2).min(7) };
        return std::array::from_fn(|channel| {
            ((i32::from(rgb[channel]) * weight + i32::from(GRASS_TONE[channel]) * (7 - weight)) / 7)
                as u8
        });
    }
    if !matches!(ground, Ground::Grass | Ground::Building) {
        return rgb;
    }

    // Broad world-space shade and sparse, asymmetric grass tufts under the trees.
    let mut value = 0;
    for (left, top, right, bottom, tint) in [
        (101, 128, 202, 224, 6),
        (284, 246, 460, 403, 5),
        (474, 301, 727, 501, -9),
        (111, 279, 285, 431, -5),
    ] {
        let d = (x - left).min(right - x).min(y - top).min(bottom - y);
        value += d.clamp(0, 35) * tint / 35;
    }
    let seed = terrain_hash(x.div_euclid(31), y.div_euclid(29));
    let dx = x.rem_euclid(31) - 8 - (seed % 15) as i32;
    let dy = y.rem_euclid(29) - 8 - ((seed >> 6) % 13) as i32;
    if seed % 5 == 0 && (-3..=3).contains(&dx) && (0..=3).contains(&dy) {
        if (dx == -3 + dy || dx == 2 - dy || dx == 0 && dy >= 2) && value < 7 {
            value += 10;
        }
    }
    rgb.map(|channel| (i32::from(channel) + value).clamp(0, 255) as u8)
}

fn terrain_edges(
    terrain: &[Ground],
    cell: usize,
    stride: usize,
    ground: Ground,
    x: i32,
    y: i32,
) -> i32 {
    let radius = match ground {
        Ground::Water => 21,
        Ground::Rock => 14,
        _ => 8,
    };
    edge_distance(terrain, cell, stride, ground, x, y, radius)
}

fn edge_distance<T: Copy + PartialEq>(
    terrain: &[T],
    cell: usize,
    stride: usize,
    ground: T,
    x: i32,
    y: i32,
    radius: i32,
) -> i32 {
    let tile = TILE_PX * 2;
    let local_x = x.rem_euclid(tile);
    let local_y = y.rem_euclid(tile);
    let mut distance = tile;
    for (other, edge) in [
        (cell - 1, local_x),
        (cell + 1, tile - 1 - local_x),
        (cell - stride, local_y),
        (cell + stride, tile - 1 - local_y),
    ] {
        if terrain[other] != ground {
            distance = distance.min(edge);
        }
    }
    // Two intersecting shore edges get a broader pixel curve within the water tile.
    for (a, b, dx, dy) in [
        (cell - 1, cell - stride, local_x, local_y),
        (cell + 1, cell - stride, tile - 1 - local_x, local_y),
        (cell - 1, cell + stride, local_x, tile - 1 - local_y),
        (
            cell + 1,
            cell + stride,
            tile - 1 - local_x,
            tile - 1 - local_y,
        ),
    ] {
        if terrain[a] != ground && terrain[b] != ground {
            distance -= (radius - dx - dy).max(0) / 2;
        }
    }
    distance
}

fn premium_terrain_color(
    ground: Ground,
    (x, y): (i32, i32),
    (tx, ty): (i32, i32),
    shore: bool,
) -> [u8; 3] {
    if shore {
        return if ground == Ground::Water {
            [115, 165, 132]
        } else {
            [134, 149, 103]
        };
    }
    match ground {
        Ground::Water => [79, 141, 151],
        Ground::Path => {
            if terrain_hash(x.div_euclid(2), y.div_euclid(2)) % 91 == 0 {
                [163, 141, 107]
            } else {
                PATH_TONE
            }
        }
        Ground::Soil => {
            if y.rem_euclid(11) < 2 {
                [126, 94, 70]
            } else {
                [151, 113, 79]
            }
        }
        Ground::Rock if ty != 16 || !(17..19).contains(&tx) => [148, 151, 122],
        _ => {
            let patch = terrain_hash(x.div_euclid(37), y.div_euclid(37));
            let dx = x.rem_euclid(37) - (7 + (patch % 23) as i32);
            let dy = y.rem_euclid(37) - (7 + ((patch >> 8) % 23) as i32);
            if patch % 4 == 0 && (dx * dx + dy * dy * 3) < 26 && terrain_hash(x / 2, y / 2) % 9 == 0
            {
                [127, 152, 86]
            } else {
                GRASS_TONE
            }
        }
    }
}

#[cfg(test)]
fn world_contains(x: i32, y: i32) -> bool {
    (0..MAP_WIDTH as i32 * TILE_PX).contains(&x) && (0..MAP_HEIGHT as i32 * TILE_PX).contains(&y)
}

fn building_frame_key(name: &str, stage: u8) -> String {
    if stage == 0 && !matches!(name, "west_house" | "east_house") {
        name.to_owned()
    } else {
        format!("{name}_{stage}")
    }
}

fn actor_clip(actor: &Actor) -> &'static str {
    match actor.pose {
        Pose::Walk => "walk",
        Pose::Carry => "carry",
        Pose::Sleep => "sleep",
        Pose::Rest => "rest",
        Pose::Drink => "drink",
        Pose::Eat => "eat",
        Pose::Idle => "idle",
        Pose::Work => match actor.activity {
            Activity::Clear => "hoe_clear",
            Activity::Till => "hoe_till",
            Activity::TendGarden => "hoe_till",
            Activity::Sow => "sow",
            Activity::Water => "water",
            Activity::Harvest => "harvest",
            Activity::Store => "store",
            Activity::WashHarvest | Activity::DryHarvest => "water",
            Activity::SortHarvest | Activity::PackHarvest => "store",
            Activity::Orchard => "harvest",
            Activity::Cook => "cook",
            Activity::FillBowl => "fill_bowl",
            Activity::GetWater => "water",
            Activity::FellTree => "axe",
            Activity::Build | Activity::Roadwork => "hammer",
            Activity::Fish => "fish",
            _ => "idle",
        },
    }
}

fn village_actor_clip(actor: &Actor) -> &'static str {
    let has = |item| actor.carried == Some(item) && actor.carried_amount() != 0;
    match actor.pose {
        Pose::Eat if !has(Item::Food) => "idle",
        Pose::Drink if !has(Item::Water) => "idle",
        Pose::Walk if actor.has_moving_box() || actor.carried_amount() != 0 => "carry",
        Pose::Work => match actor.activity {
            Activity::Clear | Activity::Till | Activity::TendGarden
                if actor.tool == Some(Tool::Hoe) =>
            {
                actor_clip(actor)
            }
            Activity::FellTree if actor.tool == Some(Tool::Axe) => "axe",
            Activity::Build | Activity::Roadwork if actor.tool == Some(Tool::Hammer) => "hammer",
            Activity::Fish if actor.tool == Some(Tool::FishingRod) => "fish",
            Activity::Water if actor.tool == Some(Tool::WateringCan) && has(Item::Water) => "water",
            Activity::Sow if has(Item::Seeds) => "sow",
            Activity::Cook => "store",
            Activity::Clear
            | Activity::Till
            | Activity::TendGarden
            | Activity::FellTree
            | Activity::Build
            | Activity::Roadwork
            | Activity::Fish
            | Activity::Water
            | Activity::Sow
            | Activity::Harvest
            | Activity::Orchard
            | Activity::GetWater
            | Activity::FillBowl
            | Activity::WashHarvest
            | Activity::DryHarvest => "idle",
            _ => actor_clip(actor),
        },
        _ => actor_clip(actor),
    }
}

#[derive(Clone, Copy)]
struct Scale {
    numerator: i32,
    denominator: i32,
}

impl Scale {
    const IDENTITY: Self = Self::new(1, 1);

    const fn new(numerator: i32, denominator: i32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    fn apply(self, value: u16) -> i32 {
        (i32::from(value) * self.numerator + self.denominator / 2) / self.denominator
    }
}

#[derive(Clone, Copy)]
struct Sprite {
    rect: [u16; 4],
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    flip: bool,
}

impl Sprite {
    fn new(
        rect: [u16; 4],
        anchor: [u16; 2],
        position: (i32, i32),
        flip: bool,
        scale: Scale,
    ) -> Self {
        let width = scale.apply(rect[2]).max(1);
        let height = scale.apply(rect[3]).max(1);
        let anchor_x = scale.apply(anchor[0]);
        let anchor_x = if flip { width - 1 - anchor_x } else { anchor_x };
        Self {
            rect,
            left: position.0 - anchor_x,
            top: position.1 - scale.apply(anchor[1]),
            width,
            height,
            flip,
        }
    }

    fn visible(self, (width, height): (i32, i32)) -> bool {
        self.left < width
            && self.top < height
            && self.left + self.width > 0
            && self.top + self.height > 0
    }

    fn at_density(mut self, density: i32) -> Self {
        self.left *= density;
        self.top *= density;
        self.width *= density;
        self.height *= density;
        self
    }
}

#[derive(Clone, Copy)]
enum Palette {
    Unchanged,
    Clothes {
        original: &'static [[u8; 3]; 4],
        colors: &'static [[u8; 3]; 4],
    },
    ResidentClothes {
        female: bool,
        color: [[u8; 3]; 3],
    },
    LitWindow,
    ForestCanopy {
        roots_start: i32,
    },
    IndoorCutaway {
        feet: [Option<(i32, i32)>; 2],
        lit: bool,
    },
}

impl Palette {
    fn color(self, bgra: &[u8]) -> [u8; 3] {
        let rgb = [bgra[2], bgra[1], bgra[0]];
        let color = match self {
            Self::Clothes { original, colors } if bgra[3] == 255 => original
                .iter()
                .position(|&sample| sample == rgb)
                .map_or(rgb, |i| colors[i]),
            Self::ResidentClothes { female, color } if bgra[3] > 225 => {
                let [r, g, b] = rgb;
                let cloth = if female {
                    g > r && g.saturating_sub(b) > 11 && r < 175
                } else {
                    g.saturating_sub(r) > 8 && g.abs_diff(b) < 20
                };
                if cloth {
                    let shade = if g < 97 {
                        0
                    } else if g < 145 {
                        1
                    } else {
                        2
                    };
                    let chosen = color[shade];
                    let detail = i16::from(g % 13) - 6;
                    chosen.map(|channel| (i16::from(channel) + detail).clamp(0, 255) as u8)
                } else {
                    rgb
                }
            }
            Self::LitWindow | Self::IndoorCutaway { lit: true, .. } if bgra[3] == 255 => {
                match rgb {
                    [126, 156, 151] | [215, 171, 118] => [247, 194, 114],
                    _ => rgb,
                }
            }
            _ => rgb,
        };
        [color[2], color[1], color[0]]
    }
}

impl LivingView {
    fn roof_palette(&self, lit: bool, baseline: i32) -> Palette {
        let feet = self
            .awake_screen_feet
            .map(|p| p.filter(|&(_, y)| y < baseline));
        let revealed = [0, 1].map(|i| self.indoor_reveals[i].or(feet[i]));
        if revealed.iter().flatten().next().is_some() {
            Palette::IndoorCutaway {
                feet: revealed,
                lit,
            }
        } else if lit {
            Palette::LitWindow
        } else {
            Palette::Unchanged
        }
    }
}

fn clothing_palette(kind: ActorKind, outfit: Outfit) -> Palette {
    // Exact shirt, sleeve, apron and seam colors from the actor atlas generator.
    const GARDENER: [[u8; 3]; 4] = [
        [103, 131, 93],
        [154, 171, 105],
        [66, 109, 98],
        [129, 169, 138],
    ];
    const CARPENTER: [[u8; 3]; 4] = [
        [170, 105, 78],
        [210, 140, 92],
        [217, 160, 80],
        [246, 205, 121],
    ];
    const WORK: [[u8; 3]; 4] = [
        [86, 113, 143],
        [128, 157, 168],
        [70, 93, 129],
        [178, 190, 191],
    ];
    const RAIN: [[u8; 3]; 4] = [
        [169, 137, 61],
        [223, 178, 76],
        [188, 156, 71],
        [242, 209, 120],
    ];
    const HOME: [[u8; 3]; 4] = [
        [137, 114, 144],
        [175, 146, 161],
        [113, 91, 131],
        [205, 173, 173],
    ];
    let original = match kind {
        ActorKind::Xiaohe => &GARDENER,
        ActorKind::Ayu => &CARPENTER,
        ActorKind::Orange => return Palette::Unchanged,
    };
    let colors = match outfit {
        Outfit::Garden => &GARDENER,
        Outfit::Work => &WORK,
        Outfit::Raincoat => &RAIN,
        Outfit::Home => &HOME,
    };
    Palette::Clothes { original, colors }
}

fn blit_into(
    out: &mut [u8],
    (width, height): (i32, i32),
    data: &[u8],
    stride: usize,
    sprite: Sprite,
    palette: Palette,
) {
    blit_rows(
        out,
        (width, height),
        data,
        stride,
        sprite,
        palette,
        0..height,
    );
}

fn blit_rows(
    out: &mut [u8],
    (width, height): (i32, i32),
    data: &[u8],
    stride: usize,
    sprite: Sprite,
    palette: Palette,
    rows: std::ops::Range<i32>,
) {
    let [source_x, source_y, source_width, source_height] = sprite.rect.map(i32::from);
    for dy in
        sprite.top.max(rows.start).max(0)..(sprite.top + sprite.height).min(rows.end).min(height)
    {
        let sy = source_y + ((dy - sprite.top) * 2 + 1) * source_height / (sprite.height * 2);
        let row_opacity = match palette {
            Palette::ForestCanopy { roots_start } if dy < roots_start => 89,
            _ => 255,
        };
        for dx in sprite.left.max(0)..(sprite.left + sprite.width).min(width) {
            let local_x = dx - sprite.left;
            let local_x = if sprite.flip {
                sprite.width - 1 - local_x
            } else {
                local_x
            };
            let sx = source_x + (local_x * 2 + 1) * source_width / (sprite.width * 2);
            let i = (sy as usize * stride + sx as usize) * 4;
            let Some(p) = data.get(i..i + 4) else {
                continue;
            };
            if p[3] == 0 {
                continue;
            }
            let color = palette.color(p);
            let opacity = match palette {
                Palette::IndoorCutaway { feet, .. } => {
                    feet.into_iter().flatten().fold(255, |alpha, (x, y)| {
                        let horizontal = (dx - x).abs();
                        if horizontal < 26 && dy >= y - 48 && dy <= y + 5 {
                            alpha.min(if horizontal < 20 { 36 } else { 132 })
                        } else {
                            alpha
                        }
                    })
                }
                _ => row_opacity,
            };
            let d = ((dy * width + dx) * 4) as usize;
            if opacity == 255 {
                let alpha = 255 - u16::from(p[3]);
                for (channel, source) in color.into_iter().chain([p[3]]).enumerate() {
                    out[d + channel] = (u16::from(source)
                        + u16::from(out[d + channel]) * alpha / 255)
                        .min(255) as u8;
                }
            } else {
                let alpha = 255 - u16::from(p[3]) * opacity / 255;
                for (channel, source) in color.into_iter().chain([p[3]]).enumerate() {
                    let source = u16::from(source) * opacity / 255;
                    out[d + channel] =
                        (source + u16::from(out[d + channel]) * alpha / 255).min(255) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_is_read_only_and_resize_reveals_same_world_pixels() {
        let world = LivingWorld::new(1);
        let before = serde_json::to_vec(&world).unwrap();
        let mut view = LivingView::new();
        let cam = Camera {
            follow: None,
            ..Camera::default()
        };
        view.render(&world, &cam, (160, 100), &[], None);
        let small = view.pixels.clone();
        view.render(&world, &cam, (220, 140), &[], None);
        for y in 0..85 {
            assert_eq!(
                &small[y * 160 * 4..(y * 160 + 145) * 4],
                &view.pixels[(y * 220) * 4..(y * 220 + 145) * 4]
            );
        }
        assert_eq!(before, serde_json::to_vec(&world).unwrap());
        assert_eq!(view.cache_bytes(), 0);
    }
    #[test]
    fn camera_does_not_create_unbounded_world() {
        let mut c = Camera {
            x: f32::NAN,
            y: f32::INFINITY,
            follow: Some(99),
        };
        c.clamp((160, 100));
        assert!(c.x.is_finite() && c.y.is_finite());
        c.x = 99999.0;
        c.y = 99999.0;
        c.clamp((160, 100));
        assert_eq!((c.x, c.y), (608.0, 412.0));
    }

    #[test]
    fn default_yard_is_stable_and_follow_recovers_the_house_roofs() {
        let mut world = LivingWorld::new(0);
        let mut fixed = Camera::default();
        let yard = (fixed.x, fixed.y);
        assert_eq!(yard, YARD_ORIGIN);
        for size in [(80, 60), (160, 100), (200, 125), (336, 91)] {
            fixed.update(&world, size);
            assert_eq!((fixed.x, fixed.y, fixed.follow), (yard.0, yard.1, None));
        }
        let mut minimum = fixed;
        minimum.fit_viewport((80, 60));
        assert_eq!((minimum.x, minimum.y), (72.0, 59.0));

        let mut followed = Camera {
            x: 76.0,
            y: 88.0,
            follow: Some(0),
        };
        for _ in 0..14 {
            followed.update(&world, (336, 91));
        }
        assert_eq!(
            (followed.x, followed.y, followed.follow),
            (yard.0, yard.1, Some(0))
        );
        for _ in 0..4 {
            followed.update(&world, (80, 60));
        }
        assert_eq!(
            (followed.x, followed.y, followed.follow),
            (72.0, 59.0, Some(0))
        );

        world.advance_world_ms(24 * 60_000);
        fixed.update(&world, (160, 100));
        assert_eq!((fixed.x, fixed.y), yard);
    }

    #[test]
    fn shrinking_releases_the_previous_large_surfaces() {
        let world = LivingWorld::new(1);
        let camera = Camera {
            follow: None,
            ..Camera::default()
        };
        let mut view = LivingView::new();
        view.render(&world, &camera, (680, 400), &[], None);
        view.render(&world, &camera, (160, 100), &[], None);
        assert_eq!(view.pixels.len(), 160 * 100 * 4);
        assert!(view.pixels.capacity() <= view.pixels.len() * 2);
        assert!(view.previous.capacity() <= view.previous.len() * 2);
    }

    #[test]
    fn flipped_frame_enters_view_from_its_actual_bounds() {
        let rect = [0, 0, 4, 1];
        let anchor = [0, 0];
        let pixel = [70, 80, 90, 255];
        let mut out = [0; 12];
        let sprite = Sprite::new(rect, anchor, (3, 0), true, Scale::IDENTITY);
        assert!(sprite.visible((3, 1)));
        assert!(!Sprite::new(rect, anchor, (3, 0), false, Scale::IDENTITY).visible((3, 1)));
        blit_into(
            &mut out,
            (3, 1),
            &pixel.repeat(4),
            4,
            sprite,
            Palette::Unchanged,
        );
        assert_eq!(out[..4], pixel);
    }

    #[test]
    fn clothes_palette_preserves_skin_and_translucent_ink() {
        let rain = clothing_palette(ActorKind::Xiaohe, Outfit::Raincoat);
        assert_eq!(rain.color(&[93, 131, 103, 255]), [61, 137, 169]);
        assert_eq!(rain.color(&[148, 195, 244, 255]), [148, 195, 244]);
        assert_eq!(rain.color(&[93, 131, 103, 180]), [93, 131, 103]);
    }

    #[test]
    fn all_household_stages_and_table_assets_have_embedded_pages() {
        assert_eq!(CORE_INDEX.pages.len(), PAGES.len());
        for name in [
            "west_house",
            "east_house",
            "kitchen",
            "storehouse",
            "workshop",
        ] {
            for stage in 0..4 {
                let key = building_frame_key(name, stage);
                assert!(
                    CORE_INDEX
                        .frames
                        .iter()
                        .any(|frame| frame.id.as_str() == key),
                    "missing {key}"
                );
            }
        }
        for key in [
            "shared_table",
            "shared_stool",
            "prop_meal",
            "female_fish_fail_1",
            "male_fish_success_2",
        ] {
            assert!(
                CORE_INDEX
                    .frames
                    .iter()
                    .any(|frame| frame.id.as_str() == key),
                "missing {key}"
            );
        }
    }

    #[test]
    fn all_developments_and_visitors_have_drawable_frames_under_atlas_budget() {
        assert_eq!(VISITOR_INDEX.pages.len(), VISITOR_PAGES.len());
        assert_eq!(DEVELOPMENT_INDEX.pages.len(), DEVELOPMENT_PAGES.len());
        assert_eq!(VISITOR_INDEX.frames.len(), 190);
        assert_eq!(DEVELOPMENT_INDEX.frames.len(), 24);
        assert_eq!(
            VISITOR_INDEX
                .frames
                .iter()
                .filter(|frame| frame.id.as_str().ends_with("_label"))
                .count(),
            10
        );
        assert!(
            PAGES.len() + VISITOR_PAGES.len() + DEVELOPMENT_PAGES.len() + PLUGIN_PAGE_LIMIT < 48
        );
    }

    #[test]
    fn near_visitors_get_two_visible_labels_without_modifying_the_world() {
        use crate::visitors::{Profession, VisitorPose};
        let world = LivingWorld::new(1);
        let before = serde_json::to_vec(&world).unwrap();
        let camera = Camera {
            x: 0.0,
            y: 0.0,
            follow: None,
        };
        let first = VisitorVisual {
            profession: Profession::MailCarrier,
            pose: VisitorPose::Trading,
            position: Point { x: 48.0, y: 72.0 },
            facing_left: false,
            animation_frame: 0,
            label: Profession::MailCarrier.label(),
            activity: "交接",
            held: None,
        };
        let second = VisitorVisual {
            profession: Profession::SeedMerchant,
            position: Point { x: 58.0, y: 72.0 },
            facing_left: true,
            label: Profession::SeedMerchant.label(),
            ..first
        };
        let mut view = LivingView::new();
        view.render(&world, &camera, (120, 100), &[], None);
        let bare = view.pixels().to_vec();
        view.render_living(&world, &camera, (120, 100), &[], None, &[first, second]);
        for y in [31, 44] {
            let range = y * 120 * 4..(y + 1) * 120 * 4;
            assert_ne!(view.pixels()[range.clone()], bare[range]);
        }
        assert_eq!(serde_json::to_vec(&world).unwrap(), before);
    }

    #[test]
    fn daytime_precipitation_is_sparse_and_camera_independent() {
        let mut small = LivingView::new();
        let mut large = LivingView::new();
        (small.width, small.height) = (80, 60);
        (large.width, large.height) = (200, 120);
        for view in [&mut small, &mut large] {
            view.pixels
                .resize((view.width * view.height * 4) as usize, 60);
            view.precipitation(YardWeather::Rain, 8_000, 70, 50);
        }
        let changed = small
            .pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] != 60)
            .count();
        assert!((4..150).contains(&changed));
        for row in 0..60 {
            let a = row * 80 * 4;
            let b = row * 200 * 4;
            assert_eq!(small.pixels[a..a + 320], large.pixels[b..b + 320]);
        }
    }

    #[test]
    fn premium_odd_viewports_keep_the_same_world_alpha_and_sampling_phase() {
        let world = LivingWorld::new(0);
        let before = serde_json::to_vec(&world).unwrap();
        let camera = Camera {
            x: 71.0,
            y: 48.0,
            follow: None,
        };
        let mut view = LivingView::new();
        let small_size = premium_source_size((479, 301));
        let large_size = premium_source_size((703, 410));
        assert_eq!(
            view.render_premium(&world, &camera, (479, 301), &[], None, &[]),
            Some([0, 0, small_size.0, small_size.1])
        );
        let small = view.pixels().to_vec();
        let grid = |origin: i32, source: i32| {
            ((origin + (source - 1) / 2).div_euclid(TILE_PX) - origin.div_euclid(TILE_PX) + 3)
                as usize
        };
        assert_eq!(
            view.terrain.len(),
            grid(71, small_size.0) * grid(48, small_size.1)
        );
        assert!(small.chunks_exact(4).all(|p| p[3] == 255));
        assert_eq!(
            view.render_premium(&world, &camera, (479, 301), &[], None, &[]),
            None
        );
        view.render_premium(&world, &camera, (703, 410), &[], None, &[]);
        for y in 0..small_size.1 as usize {
            let stride = small_size.0 as usize * 4;
            let dest = y * large_size.0 as usize * 4;
            assert_eq!(
                &small[y * stride..(y + 1) * stride],
                &view.pixels()[dest..dest + stride]
            );
        }
        assert_eq!(serde_json::to_vec(&world).unwrap(), before);
    }

    #[test]
    fn premium_pond_uses_water_only_inside_the_original_blocked_cells() {
        let world = LivingWorld::new(0);
        let mut view = LivingView::new();
        let camera = Camera {
            x: 55.0,
            y: 130.0,
            follow: None,
        };
        view.render_premium(&world, &camera, (450, 360), &[], None, &[]);
        let mut water = 0;
        let mut visible_bank = 0;
        for (i, pixel) in view.pixels().chunks_exact(4).enumerate() {
            let x = 55 + (i as i32 % view.width) / 2;
            let y = 130 + (i as i32 / view.width) / 2;
            let ground = world.ground(x / TILE_PX, y / TILE_PX);
            if pixel == [151, 141, 79, 255] {
                assert_eq!(ground, Ground::Water);
                assert!(!world.can_walk(x / TILE_PX, y / TILE_PX));
                water += 1;
            } else if pixel == [103, 149, 134, 255] && ground == Ground::Water {
                visible_bank += 1;
            }
        }
        assert!(water > 1000 && visible_bank > 100);
    }

    #[test]
    fn calm_ground_texture_is_sparse_compared_to_legacy_scatter() {
        let legacy = (0..120)
            .flat_map(|y| (0..120).map(move |x| (x, y)))
            .filter(|&(x, y)| terrain_color(Ground::Grass, x, y, x / 8, y / 8, 0) != [143, 175, 94])
            .count();
        let premium = (0..240)
            .flat_map(|y| (0..240).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                premium_terrain_color(Ground::Grass, (x, y), (x / 16, y / 16), false) != GRASS_TONE
            })
            .count();
        assert!(
            premium * 10 < legacy * 4,
            "noise ratio premium {premium}/57600 vs old {legacy}/14400"
        );
    }

    #[test]
    fn premium_atlas_keeps_individual_texels_and_only_replaces_opted_in_render() {
        static PAGE: [u8; 256 * 256 * 4] = {
            let mut pixels = [0; 256 * 256 * 4];
            pixels[0] = 200;
            pixels[3] = 255;
            pixels[5] = 200;
            pixels[7] = 255;
            pixels
        };
        static PAGES: &[&[u8]] = &[&PAGE];
        let manifest = br#"{"pages":[{"width":256,"height":256}],"frames":[{"id":"shared_table","page":0,"rect":[0,0,2,1],"anchor":[0,0],"hand_anchor":[1,0]}]}"#;
        let world = LivingWorld::new(0);
        let camera = Camera {
            x: 71.0,
            y: 56.0,
            follow: None,
        };
        let mut view = LivingView::new();
        view.render(&world, &camera, (160, 100), &[], None);
        let old = view.pixels().to_vec();
        view.set_premium_assets(manifest, PAGES).unwrap();
        view.render(&world, &camera, (160, 100), &[], None);
        assert_eq!(view.pixels(), old);
        view.render_premium(&world, &camera, (960, 600), &[], None, &[]);
        assert_eq!(
            view.premium_hand("shared_table", (7, 9), false),
            Some((15, 18))
        );
        assert_eq!(
            view.premium_hand("shared_table", (7, 9), true),
            Some((13, 18))
        );
        let table = (((TABLE.1 - 56) * 2) * 320 + (TABLE.0 - 71) * 2) as usize * 4;
        assert_eq!(
            &view.pixels()[table..table + 8],
            &[200, 0, 0, 255, 0, 200, 0, 255]
        );
    }

    #[test]
    fn premium_ground_cache_reuses_the_base_and_keeps_ripples_live() {
        let mut world = LivingWorld::new(0);
        let mut view = LivingView::new();
        let camera = Camera {
            x: 65.0,
            y: 128.0,
            follow: None,
        };
        view.render_premium(&world, &camera, (318, 209), &[], None, &[]);
        let cached = view.terrain_pixels.clone();
        let first_frame = view.pixels.clone();
        let stamp = view.terrain_stamp;

        world.advance_world_ms(100_000);
        view.render_premium(&world, &camera, (318, 209), &[], None, &[]);
        assert_eq!(view.terrain_stamp, stamp);
        assert_eq!(view.terrain_pixels, cached);
        assert_ne!(view.pixels, first_frame);

        let mut fresh = LivingView::new();
        fresh.render_premium(&world, &camera, (318, 209), &[], None, &[]);
        assert_eq!(view.pixels, fresh.pixels);
        view.render_premium(&world, &camera, (3000, 1680), &[], None, &[]);
        assert!(view.terrain_pixels.is_empty());
        assert_eq!(view.terrain_stamp, None);
        view.render_premium(&world, &camera, (318, 209), &[], None, &[]);
        assert_eq!(view.pixels, fresh.pixels);
    }

    #[test]
    fn premium_building_progress_preserves_the_high_density_base() {
        static PAGE: [u8; 256 * 256 * 4] = {
            let mut pixels = [0; 256 * 256 * 4];
            pixels[0] = 130;
            pixels[3] = 255;
            pixels[5] = 150;
            pixels[7] = 255;
            pixels
        };
        static PAGES: &[&[u8]] = &[&PAGE];
        let manifest = br#"{"pages":[{"width":256,"height":256}],"frames":[
            {"id":"west_house_0","page":0,"rect":[0,0,1,1],"anchor":[0,0]},
            {"id":"west_house_level_3","page":0,"rect":[1,0,1,1],"anchor":[0,0]}]}"#;
        let world = LivingWorld::new(0);
        let mut west = world.buildings()[0];
        let mut view = LivingView::new();
        view.set_premium_assets(manifest, PAGES).unwrap();
        (view.width, view.height, view.density) = (16, 16, 2);
        view.pixels.resize(16 * 16 * 4, 0);
        let cx = west.position.x as i32 - 4;
        let cy = building_baseline(&west) - 4;
        let at = (8 * 16 + 8) * 4;

        west.progress = 28;
        view.building(&west, cx, cy, 480);
        assert_eq!(&view.pixels[at..at + 4], &[130, 0, 0, 255]);

        west.level = 4;
        west.progress = 0;
        view.building(&west, cx, cy, 480);
        assert_eq!(&view.pixels[at..at + 4], &[0, 150, 0, 255]);
    }

    #[test]
    fn only_the_real_world_bounds_are_transparent() {
        let world = LivingWorld::new(0);
        let mut view = LivingView::new();
        for camera in [
            Camera {
                x: -9.0,
                y: -6.0,
                follow: None,
            },
            Camera {
                x: MAP_WIDTH as f32 * TILE_PX as f32 - 9.0,
                y: MAP_HEIGHT as f32 * TILE_PX as f32 - 6.0,
                follow: None,
            },
            Camera {
                x: 127.0,
                y: 13.0,
                follow: None,
            },
        ] {
            view.render_premium(&world, &camera, (121, 97), &[], None, &[]);
            for (i, pixel) in view.pixels().chunks_exact(4).enumerate() {
                let x = camera.x as i32 + (i as i32 % view.width) / 2;
                let y = camera.y as i32 + (i as i32 / view.width) / 2;
                if world_contains(x, y) {
                    assert_eq!(pixel[3], 255);
                } else {
                    assert_eq!(pixel, [0; 4]);
                }
            }
        }
    }
}
