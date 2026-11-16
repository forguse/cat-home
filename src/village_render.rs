//! Willow Village rendering using the same high-density compositing as the yard.
use super::*;
use crate::companionship::{REWARDS, RewardStyle};
use crate::living_world::{ScenicResourceView, ScenicStage, SoilProfile};
use crate::map_data::{
    ArtRect, BuildingDef, PhaseMask, ScenicKind, ScenicObject, SlotPos, TilePos, TileRect,
    WILLOW_VILLAGE,
};
use crate::residents::{VillageArtSubject, VillageArtVisual};
use crate::visitors::{VisitorHeld, VisitorPose, VisitorTool};

const CHUNK_ART: i32 = 256;
const MAP_CHUNKS: usize = 96;
const CHUNK_COLUMNS: usize = 12;
const CAT_LAMP: TilePos = TilePos::new(20, 17);
pub(super) const STONE_PROP_SCALE: Scale = Scale::new(2, 5);
const STATIC_PROPS: [(&str, &str, Option<&str>); 8] = [
    ("home_table", "shared_table", Some("home_table.tabletop")),
    ("home_table", "shared_stool", Some("home_table.seat_a")),
    ("home_table", "shared_stool", Some("home_table.seat_b")),
    ("home_well", "well_stone", Some("home_well.wellhead")),
    ("cat_water", "prop_bowl", None),
    ("market_well", "well_stone", None),
    ("market_notice", "village_notice", None),
    ("commons_garden", "garden", None),
];

fn dusk_board_colors([wall, sky, grass]: [u8; 3]) -> [[u8; 3]; 3] {
    let walls = [[155, 115, 84], [185, 127, 88], [194, 149, 105], [126, 103, 91]];
    let skies = [[205, 139, 122], [141, 128, 154], [131, 141, 154], [166, 158, 175]];
    let grasses = [[120, 142, 83], [89, 124, 79], [159, 133, 65], [134, 133, 112]];
    [walls[usize::from(wall.min(3))], skies[usize::from(sky.min(3))], grasses[usize::from(grass.min(3))]]
}

pub(super) fn item_asset(item: Item) -> &'static str {
    match item {
        Item::Fish => "prop_fish",
        Item::Seeds => "prop_seeds",
        Item::Food => "prop_meal",
        Item::Crops => "prop_crop",
        Item::Wood => "prop_wood",
        Item::Water => "prop_pail",
        Item::Stone => "prop_stone",
        Item::Rope => "prop_rope",
    }
}

pub(super) fn held_asset(held: VisitorHeld) -> Option<(&'static str, Scale)> {
    let result = match held {
        VisitorHeld::Goods(item) => (item_asset(item), Scale::new(3, 5)),
        VisitorHeld::MoveBox => ("housing_moving_box", Scale::new(3, 4)),
        VisitorHeld::Parcel("kitchen_scraps" | "guest_food_basket") => ("prop_basket", Scale::new(1, 2)),
        VisitorHeld::Parcel("warm_cat_supper") => ("prop_bowl", Scale::new(3, 5)),
        VisitorHeld::Parcel("covered_snack_tray" | "empty_snack_tray" | "saucer_meal" | "empty_saucer" | "wrapped_letter" | "wrapped_seed_packet" | "orchard_leaf") => return None,
        VisitorHeld::Parcel(_) => ("housing_moving_box", Scale::new(2, 5)),
        VisitorHeld::Tool(tool) => (
            match tool {
                VisitorTool::Hammer => "prop_hammer",
                VisitorTool::Rod => "prop_rod",
                VisitorTool::Hoe => "prop_hoe",
                VisitorTool::WateringCan => "prop_water_can",
                VisitorTool::Basket => "prop_basket",
                VisitorTool::RopeKit => "prop_rope",
                VisitorTool::Knife => return None,
            },
            Scale::new(3, 4),
        ),
    };
    Some(result)
}

#[derive(Clone, Copy)]
struct ScenicArt {
    bank: Bank,
    key: &'static str,
    scale: Scale,
    flip: bool,
    root_offset: i32,
    shadow_radii: (i32, i32),
}

impl ScenicArt {
    fn position(self, (x, y): (i32, i32)) -> (i32, i32) {
        (x * 2, (y + self.root_offset) * 2)
    }

    fn shadow_center(self, (x, y): (i32, i32)) -> (i32, i32) {
        (x + 3, y + self.root_offset + 1)
    }

    fn shadow_bounds(self, foot: (i32, i32)) -> ArtRect {
        let (x, y) = self.shadow_center(foot);
        let (rx, ry) = self.shadow_radii;
        ArtRect {
            x: (x - rx) * 2,
            y: (y - ry) * 2,
            w: rx * 4,
            h: ry * 4,
        }
    }
}

fn scenic_art(item: &ScenicObject) -> ScenicArt {
    let (tx, ty) = (i32::from(item.tile.x), i32::from(item.tile.y));
    let seed = terrain_hash(tx, ty);
    let forest = item.kind == ScenicKind::Tree && item.id.contains(".forest");
    let (bank, key, scale, shadow_radii) = match item.kind {
        ScenicKind::Tree => {
            let (bank, key) = if forest {
                (Bank::Village, forest_crown(tx, ty, seed))
            } else {
                let trees = [
                    "tree_oak",
                    "tree_birch",
                    "tree_maple",
                    "tree_oak",
                    "tree_cherry",
                ];
                (Bank::Core, trees[seed as usize % trees.len()])
            };
            (
                bank,
                key,
                Scale::new(9 + ((seed >> 4) % 4) as i32, 12),
                (12, 4),
            )
        }
        ScenicKind::Shrub => (Bank::Village, "village_hedge", Scale::IDENTITY, (8, 3)),
        ScenicKind::Rock => (Bank::Core, "rocks_moss", Scale::new(3, 4), (6, 2)),
        ScenicKind::Fence => (
            Bank::Village,
            "village_garden_wall",
            Scale::IDENTITY,
            (8, 2),
        ),
        ScenicKind::Bench => (Bank::Village, "village_bench", Scale::IDENTITY, (8, 3)),
        ScenicKind::Crate => (Bank::Village, "village_crate", Scale::IDENTITY, (7, 3)),
        ScenicKind::Flowers => (Bank::Core, "flowers_wild", Scale::new(5, 8), (4, 2)),
        ScenicKind::Grass if item.temporary_ground => (
            Bank::Core,
            "bush_herb",
            Scale::new(3 + ((seed >> 5) & 1) as i32, 4),
            (4, 2),
        ),
        ScenicKind::Grass => (Bank::Core, "bush_herb", Scale::new(2, 5), (3, 2)),
        ScenicKind::Sign => (Bank::Village, "village_notice", Scale::new(4, 5), (6, 2)),
        ScenicKind::Lamp => (Bank::Village, "village_lantern", Scale::IDENTITY, (5, 3)),
        ScenicKind::Reeds if item.temporary_ground => (
            Bank::Village,
            "village_reeds",
            Scale::new(4 + ((seed >> 5) & 1) as i32, 5),
            (6, 3),
        ),
        ScenicKind::Reeds => (Bank::Village, "village_reeds", Scale::new(4, 5), (6, 3)),
        ScenicKind::Log => (Bank::Core, "logs", Scale::new(4, 5), (7, 3)),
        ScenicKind::WoodPile => (Bank::Village, "village_materials", Scale::new(4, 5), (8, 3)),
    };
    // Natural forest roots can vary within their blocking tile.
    let root_offset = if forest {
        ((seed >> 7) % 5) as i32 - 2
    } else {
        0
    };
    ScenicArt {
        bank,
        key,
        scale,
        flip: seed & 2 != 0,
        root_offset,
        shadow_radii,
    }
}

pub(super) struct VillageIndex {
    chunks: [Vec<usize>; MAP_CHUNKS],
    bounds: Vec<Option<ArtRect>>,
    scenic: [Vec<usize>; MAP_CHUNKS],
    scenic_bounds: Vec<ArtRect>,
}

impl VillageIndex {
    fn new(core: Option<&ArtAtlas>, village: Option<&ArtAtlas>) -> Self {
        let mut chunks: [Vec<usize>; MAP_CHUNKS] = std::array::from_fn(|_| Vec::new());
        let mut envelopes = vec![None; WILLOW_VILLAGE.buildings.len()];
        for (id, b) in WILLOW_VILLAGE.buildings.iter().enumerate() {
            if family_building(b.id) {
                continue;
            }
            let (bank, key) = structure(b.id, b.kind);
            let atlas = match bank {
                Bank::Core => core,
                Bank::Village => village,
                Bank::Wildlife => None,
            };
            let anchor = structure_anchor(b);
            let mut bounds = atlas
                .and_then(|a| a.metadata(key))
                .map_or(b.envelope, |f| ArtRect {
                    x: (anchor.x * 2.0) as i32 - i32::from(f.anchor[0]),
                    y: (anchor.y * 2.0) as i32 - i32::from(f.anchor[1]),
                    w: i32::from(f.rect[2]),
                    h: i32::from(f.rect[3]),
                });
            // Include the largest real ground shadow and bounded door attachments.
            let shadow_half = i32::from(b.footprint.w) * 8;
            let left = bounds.x.min((anchor.x * 2.0) as i32 - shadow_half + 6);
            let right = (bounds.x + bounds.w).max((anchor.x * 2.0) as i32 + shadow_half + 6);
            bounds.x = left;
            bounds.w = right - left;
            bounds.h += 18;
            envelopes[id] = Some(bounds);
            for_each_chunk(bounds, |chunk| chunks[chunk].push(id));
        }
        let mut scenic: [Vec<usize>; MAP_CHUNKS] = std::array::from_fn(|_| Vec::new());
        let scenic_bounds: Vec<_> = WILLOW_VILLAGE
            .scenic
            .iter()
            .map(|item| {
                let art = scenic_art(item);
                let foot = local(point(item.tile), (0, 0));
                let shadow = art.shadow_bounds(foot);
                let bounds = art
                    .bank
                    .atlas(core, village, None)
                    .and_then(|atlas| atlas.metadata(art.key))
                    .map_or(shadow, |frame| {
                        let sprite = Sprite::new(
                            frame.rect,
                            frame.anchor,
                            art.position(foot),
                            art.flip,
                            art.scale,
                        );
                        union(shadow, sprite_bounds(sprite))
                    });
                if matches!(
                    item.kind,
                    ScenicKind::Tree
                        | ScenicKind::Rock
                        | ScenicKind::Grass
                        | ScenicKind::Reeds
                        | ScenicKind::Shrub
                        | ScenicKind::Log
                        | ScenicKind::WoodPile
                ) {
                    union(bounds, worked_resource_bounds(foot, item.kind))
                } else {
                    bounds
                }
            })
            .collect();
        for (id, &bounds) in scenic_bounds.iter().enumerate() {
            for_each_chunk(bounds, |chunk| scenic[chunk].push(id));
        }
        Self {
            chunks,
            bounds: envelopes,
            scenic,
            scenic_bounds,
        }
    }

    fn in_view(&self, view: ArtRect) -> u64 {
        let mut result = 0_u64;
        for_each_chunk(view, |chunk| {
            for &id in &self.chunks[chunk] {
                if self.bounds[id].is_some_and(|bounds| intersects(bounds, view)) {
                    result |= 1_u64.checked_shl(id as u32).unwrap_or(0);
                }
            }
        });
        result
    }

    fn scenic_in_view(&self, view: ArtRect, phases: crate::map_data::PhaseMask) -> Vec<usize> {
        let mut out = Vec::with_capacity(64);
        for_each_chunk(view, |chunk| {
            out.extend(self.scenic[chunk].iter().copied().filter(|&id| {
                intersects(self.scenic_bounds[id], view)
                    && phases.contains(WILLOW_VILLAGE.scenic[id].phase)
            }));
        });
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn for_each_chunk(rect: ArtRect, mut apply: impl FnMut(usize)) {
    if rect.w <= 0 || rect.h <= 0 {
        return;
    }
    let left = rect.x.div_euclid(CHUNK_ART).clamp(0, 12);
    let right = (rect.x + rect.w - 1).div_euclid(CHUNK_ART).clamp(-1, 11);
    let top = rect.y.div_euclid(CHUNK_ART).clamp(0, 8);
    let bottom = (rect.y + rect.h - 1).div_euclid(CHUNK_ART).clamp(-1, 7);
    for y in top..=bottom {
        for x in left..=right {
            apply(y as usize * CHUNK_COLUMNS + x as usize);
        }
    }
}

fn family_building(id: &str) -> bool {
    id.starts_with("home_") || id.starts_with("new_")
}

fn structure_anchor(b: &BuildingDef) -> Point {
    if b.kind == "stall" && b.door.y < b.footprint.y {
        // This booth serves through its northern opening. Its camera-side
        // roof and posts still rise from the southern edge of the real site.
        let mut p = Point::at_tile(
            i32::from(b.door.x),
            i32::from(b.footprint.y + b.footprint.h - 1),
        );
        p.y += 4.0;
        p
    } else {
        point(b.door)
    }
}

fn point(pos: TilePos) -> Point {
    Point::at_tile(i32::from(pos.x), i32::from(pos.y))
}

fn forest_crown(x: i32, y: i32, variation: u32) -> &'static str {
    let group = terrain_hash(x.div_euclid(6), y.div_euclid(5)) % 11;
    match (group + variation % 3) % 11 {
        0..=4 => "village_forest_camphor",
        5..=8 => "village_forest_laurel",
        _ => "village_forest_hornbeam",
    }
}

#[derive(Clone, Copy)]
struct CanopySubject {
    area: ArtRect,
    depth: i32,
}

fn canopy_subjects(
    world: &LivingWorld,
    origin: (i32, i32),
    view: ArtRect,
    commands: &[FrameCommand],
    store: Option<&PluginStore>,
) -> [Option<CanopySubject>; 3] {
    [0, 1, 2].map(|i| {
        let actor = &world.actors()[i];
        if actor.inside() || actor.pose == Pose::Sleep
            || actor.kind == ActorKind::Orange && world.cat_bloom_target().is_none()
        {
            return None;
        }
        let overlay = store.and_then(|_| {
            commands
                .iter()
                .find(|c| c.actor_override == Some(actor.kind))
        });
        let p = overlay.map_or(actor.position, |c| c.point);
        let (x, y) = local(p, origin);
        let mut area = ArtRect {
            x: x * 2 - 15,
            y: y * 2 - 48,
            w: 31,
            h: 52,
        };
        if actor.kind == ActorKind::Orange {
            area = ArtRect { x: x * 2 - 13, y: y * 2 - 23, w: 27, h: 27 };
        }
        let mut depth = overlay.map_or(p.y as i32, |c| c.depth);
        if overlay.is_none()
            && actor.pose == Pose::Work
            && let Some(target) = world.scenic_work_target(actor.kind)
        {
            let target = point(target);
            let (tx, ty) = local(target, origin);
            // Reveal the actual work material near the worker, including the
            // bottom of the trunk and any pending cargo beside the foothold.
            if (tx - x).abs() + (ty - y).abs() <= 8 {
                let left = area.x.min(tx * 2 - 23);
                let top = area.y.min(ty * 2 - 17);
                let right = (area.x + area.w).max(tx * 2 + 24);
                let bottom = (area.y + area.h).max(ty * 2 + 4);
                area = ArtRect {
                    x: left,
                    y: top,
                    w: right - left,
                    h: bottom - top,
                };
                depth = depth.min(target.y as i32);
            }
        }
        let view = ArtRect {
            x: 0,
            y: 0,
            w: view.w,
            h: view.h,
        };
        intersects(area, view).then_some(CanopySubject { area, depth })
    })
}

fn local(p: Point, origin: (i32, i32)) -> (i32, i32) {
    (p.x.round() as i32 - origin.0, p.y.round() as i32 - origin.1)
}

pub(super) fn at_village_seat((x, y): (i32, i32)) -> bool {
    ["home_table.seat_a", "home_table.seat_b"]
        .into_iter()
        .any(|id| {
            matches!(WILLOW_VILLAGE.slot("home_table", id).map(|slot| slot.position),
            Some(SlotPos::Outdoor(tile)) if local(point(tile), (0,0)) == (x,y))
        })
}

fn clip(view: &LivingView, (x, y): (i32, i32), margin: (i32, i32), below: i32) -> bool {
    x + margin.0 > 0
        && x - margin.0 < (view.width + 1) / 2
        && y + below > 0
        && y - margin.1 < (view.height + 1) / 2
}

fn visible_character_motion(view: &LivingView, p: Point, origin: (i32, i32)) -> bool {
    clip(view, local(p, origin), (12, 30), 7)
}

fn temporary_ground_visible(
    world: &LivingWorld,
    tile: TilePos,
    phases: PhaseMask,
    construction_sites: u64,
    housing_footprints: &[TileRect],
) -> bool {
    if world.ground(i32::from(tile.x), i32::from(tile.y)) != Ground::Grass {
        return false;
    }
    if WILLOW_VILLAGE
        .buildings
        .iter()
        .enumerate()
        .any(|(index, b)| {
            b.footprint.contains(tile)
                && (phases.contains(b.phase) || construction_sites & (1 << index) != 0)
        })
        || housing_footprints
            .iter()
            .any(|footprint| footprint.contains(tile))
    {
        return false;
    }
    WILLOW_VILLAGE
        .crops
        .iter()
        .position(|crop| crop.rect.contains(tile))
        .is_none_or(|index| {
            !phases.contains(WILLOW_VILLAGE.crops[index].phase)
                || world.plot_stage(index) == Some(PlotStage::Grass)
        })
}

fn housing_footprint(site: HousingView) -> TileRect {
    TileRect::new(
        site.lot_origin.x as u16 / TILE_PX as u16,
        site.lot_origin.y as u16 / TILE_PX as u16,
        u16::from(site.footprint[0]),
        u16::from(site.footprint[1]),
    )
}

fn development_building(project: &Development) -> Option<&'static str> {
    match project.kind {
        DevelopmentKind::Market | DevelopmentKind::Stall => Some("stall_produce"),
        DevelopmentKind::Guesthouse => Some("craft_inn"),
        DevelopmentKind::Depot => Some("market_depot"),
        _ => None,
    }
}

fn intersects(a: ArtRect, b: ArtRect) -> bool {
    intersection(a, b).is_some()
}

fn intersection(a: ArtRect, b: ArtRect) -> Option<ArtRect> {
    let (x, y) = (a.x.max(b.x), a.y.max(b.y));
    let (w, h) = (
        (a.x + a.w).min(b.x + b.w) - x,
        (a.y + a.h).min(b.y + b.h) - y,
    );
    (w > 0 && h > 0).then_some(ArtRect { x, y, w, h })
}

fn union(a: ArtRect, b: ArtRect) -> ArtRect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    ArtRect {
        x,
        y,
        w: (a.x + a.w).max(b.x + b.w) - x,
        h: (a.y + a.h).max(b.y + b.h) - y,
    }
}

fn sprite_bounds(sprite: Sprite) -> ArtRect {
    ArtRect {
        x: sprite.left,
        y: sprite.top,
        w: sprite.width,
        h: sprite.height,
    }
}

// The largest processed state is the fallen trunk reaching 22 texels on
// either side of the original root. These local marks share its tile anchor.
fn worked_resource_bounds((x, y): (i32, i32), kind: ScenicKind) -> ArtRect {
    let (half_width, height) = match kind {
        ScenicKind::Tree | ScenicKind::Log | ScenicKind::WoodPile => (27, 24),
        ScenicKind::Rock => (19, 16),
        _ => (13, 10),
    };
    ArtRect {
        x: x * 2 - half_width,
        y: y * 2 - height,
        w: half_width * 2 + 1,
        h: height + 8,
    }
}

fn crown_has_ink(overlap: ArtRect, sprite: Sprite, page: &[u8]) -> bool {
    let [left, top, width, height] = sprite.rect.map(i32::from);
    for y in overlap.y..overlap.y + overlap.h {
        let sy = top + ((y - sprite.top) * 2 + 1) * height / (sprite.height * 2);
        for x in overlap.x..overlap.x + overlap.w {
            let dx = x - sprite.left;
            let local = if sprite.flip {
                sprite.width - 1 - dx
            } else {
                dx
            };
            let sx = left + (local * 2 + 1) * width / (sprite.width * 2);
            if page[((sy * 256 + sx) * 4 + 3) as usize] > 110 {
                return true;
            }
        }
    }
    false
}

fn blit_canopy(
    out: &mut [u8],
    stride: i32,
    page: &[u8],
    sprite: Sprite,
    visible: ArtRect,
    roots: i32,
    cutaways: [Option<ArtRect>; 3],
) {
    let [left, top, width, height] = sprite.rect.map(i32::from);
    for y in visible.y..visible.y + visible.h {
        let sy = top + ((y - sprite.top) * 2 + 1) * height / (sprite.height * 2);
        for x in visible.x..visible.x + visible.w {
            let sx = x - sprite.left;
            let sx = if sprite.flip {
                sprite.width - 1 - sx
            } else {
                sx
            };
            let sx = left + (sx * 2 + 1) * width / (sprite.width * 2);
            let src = ((sy * 256 + sx) * 4) as usize;
            let source = &page[src..src + 4];
            if source[3] == 0 {
                continue;
            }

            let opacity = if y >= roots {
                255
            } else {
                cutaways.iter().flatten().fold(255, |opacity, area| {
                    let dx = (area.x - x).max(x - (area.x + area.w - 1)).max(0);
                    let dy = (area.y - y).max(y - (area.y + area.h - 1)).max(0);
                    opacity.min(36 + (dx.max(dy) * 32).clamp(0, 219))
                }) as u16
            };
            let dst = ((y * stride + x) * 4) as usize;
            let alpha = 255 - u16::from(source[3]) * opacity / 255;
            for (old, &incoming) in out[dst..dst + 4].iter_mut().zip(source) {
                *old = (u16::from(incoming) * opacity / 255 + u16::from(*old) * alpha / 255)
                    .min(255) as u8;
            }
        }
    }
}

enum Visible<'a> {
    Legacy(Drawable<'a>),
    Building(&'static BuildingDef, bool),
    Prop(&'static str, Point),
    DoorTrim(&'static str, Point),
    Wild(&'a WildlifeVisual),
    Resident(&'a VisitorVisual),
    Scenic(&'static ScenicObject, Option<ScenicResourceView>),
    Lamp(Point),
    Drain(Point),
    BridgeRail(TilePos, u8),
    Menu(Point, u16, u8),
    Artwork(Point, VillageArtVisual),
    CatPlate(Point, bool),
    RainBarrel(Point, u8, bool),
    LeafCard(Point, u8, u8),
    FoodSaucer(Point, bool, bool),
    CoatHook(Point),
    DoorFur(Point),
    YardReport(Point, bool, bool),
    Compost(Point, u8, u8),
}

impl LivingView {
    fn village_food_saucer(&mut self, (x, y): (i32, i32), full: bool) {
        self.resource_line((x - 4, y + 1), (8, 0), [91, 90, 86]);
        self.resource_line((x - 3, y), (6, 0), [195, 174, 150]);
        self.resource_line((x - 2, y + 2), (4, 0), [158, 138, 116]);
        if full {
            self.resource_line((x - 2, y - 1), (4, 0), [215, 134, 68]);
            self.resource_texel((x + 1, y - 2), [165, 151, 83]);
        }
    }

    fn resource_texel(&mut self, (x, y): (i32, i32), [r, g, b]: [u8; 3]) {
        if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
            let at = ((y * self.width + x) * 4) as usize;
            self.pixels[at..at + 4].copy_from_slice(&[b, g, r, 255]);
        }
    }

    fn resource_line(&mut self, (x, y): (i32, i32), (dx, dy): (i32, i32), color: [u8; 3]) {
        let steps = dx.abs().max(dy.abs()).max(1);
        for step in 0..=steps {
            self.resource_texel((x + dx * step / steps, y + dy * step / steps), color);
        }
    }

    pub(super) fn village_cooking_apron(&mut self, (x, y): (i32, i32), facing_left: bool) {
        let x = x * 2 + if facing_left { -1 } else { 1 };
        let y = y * 2;
        let trim = [139, 98, 70];
        self.resource_line((x - 3, y - 23), (1, 4), trim);
        self.resource_line((x + 3, y - 23), (-1, 4), trim);
        for row in -19..=-10 {
            let radius = if row < -16 { 2 } else { 4 };
            let linen = if row == -16 || row == -10 { [174, 128, 86] } else { [231, 194, 137] };
            self.resource_line((x - radius, y + row), (radius * 2, 0), linen);
        }
        self.resource_line((x - 1, y - 13), (3, 0), [188, 143, 99]);
    }

    pub(super) fn village_oilpaper(&mut self, (x, y): (i32, i32), seeds: bool) {
        for row in -4..=1 {
            let ink = if row == -4 || row == 1 { [95, 62, 38] } else { [166, 117, 66] };
            self.resource_line((x - 4, y + row), (8, 0), ink);
        }
        self.resource_line((x - 1, y - 4), (1, 5), [229, 190, 120]);
        self.resource_line((x - 4, y - 1), (8, 0), [201, 158, 96]);
        self.resource_texel((x, y - 2), [239, 209, 146]);
        if seeds {
            self.resource_line((x + 1, y - 2), (2, -2), [114, 151, 81]);
        }
    }

    fn village_root_shoulder(&mut self, (x, y): (i32, i32), size: u8) {
        let (x, y) = (x * 2 + 3, y * 2);
        let radius = i32::from(size.clamp(1, 3)) + 2;
        self.resource_line((x - radius - 1, y + 3), (radius * 2 + 2, 0), [112, 80, 62]);
        for row in -2..=2 {
            let half = if row == -2 || row == 2 { radius - 1 } else { radius };
            let ink = if row <= -1 { [169, 95, 112] } else { [217, 152, 149] };
            self.resource_line((x - half, y + row), (half * 2, 0), ink);
        }
        self.resource_line((x - 2, y), (3, -1), [248, 206, 172]);
        self.resource_line((x, y - 3), (-2, -4), [92, 132, 76]);
        self.resource_line((x + 1, y - 3), (3, -3), [139, 158, 82]);
        self.resource_line((x - radius, y + 3), (2, 0), [160, 113, 81]);
    }

    fn village_soil_cut(&mut self, (x, y): (i32, i32), profile: SoilProfile) {
        let (x, y) = (x * 2, y * 2 + 1);
        let [top, mid, floor] = match profile {
            SoilProfile::Loam => [[148, 109, 74], [105, 76, 60], [77, 56, 46]],
            SoilProfile::Sand => [[217, 173, 107], [184, 139, 90], [145, 110, 75]],
            SoilProfile::Clay => [[177, 118, 85], [158, 83, 63], [118, 66, 55]],
        };
        self.resource_line((x - 5, y - 5), (10, 0), [91, 110, 66]);
        for (row, shade) in [top, top, mid, mid, mid, floor, floor].into_iter().enumerate() {
            let half = if row == 0 || row >= 5 { 4 } else { 6 };
            self.resource_line((x - half, y + row as i32 - 4), (2 * half, 0), shade);
        }
        for (dx, dy) in [(-4, -1), (1, -3), (3, 0)] {
            self.resource_texel((x + dx, y + dy), top.map(|n| n.saturating_add(18)));
        }
    }

    fn village_apple_leaf(&mut self, (x, y): (i32, i32)) {
        self.resource_line((x - 3, y + 1), (5, -4), [161, 106, 58]);
        self.resource_line((x - 1, y - 2), (3, 0), [205, 145, 68]);
        self.resource_texel((x + 2, y - 4), [181, 121, 65]);
    }

    fn village_leaf_rubbing(&mut self, (x, y): (i32, i32), tree: u8, progress: u8) {
        let (x, y) = (x * 2 + 10, y * 2 - 11);
        self.resource_line((x - 3, y + 4), (0, 7), [126, 85, 50]);
        self.resource_line((x + 5, y + 4), (0, 7), [126, 85, 50]);
        for row in -6..=4 {
            let ink = if row == 4 { [135, 101, 64] } else { [234, 202, 150] };
            self.resource_line((x - 7, y + row), (14, 0), ink);
        }
        if progress >= 20 && (8..12).contains(&tree) {
            self.resource_line((x - 4, y + 2), (7, -7), [125, 95, 68]);
            for (i, (dx, dy)) in [(-2, 0), (0, -2), (2, -4)].into_iter().enumerate() {
                if usize::from(progress) < 30 + i * 20 { break; }
                self.resource_line((x + dx, y + dy), (0, -3), [156, 116, 77]);
                self.resource_line((x + dx, y + dy), (3, -1), [156, 116, 77]);
            }
        }
    }

    fn village_family_detail(&mut self, world: &LivingWorld, actor: &Actor, origin: (i32, i32)) {
        let (x, y) = local(actor.position, origin);
        let (x, y) = (x * 2, y * 2);
        if world.harvest_seed_pouch(actor.kind) && !matches!(actor.pose, Pose::Sleep | Pose::Drink | Pose::Eat) {
            let side = if actor.facing_left { -1 } else { 1 };
            self.core_at_texel("prop_seeds", (x + 7 * side, y - 15), false, Scale::new(2, 5), Palette::Unchanged);
        }
        let detail = world.household_detail_progress(actor.kind);
        if actor.kind != ActorKind::Orange {
            if matches!(actor.activity, Activity::RecordHarvest | Activity::RecordRepair)
                && let Some((elapsed, duration)) = detail
                && let Some(table) = world.resolve_poi("home_table")
            {
                let (tx, ty) = local(table, origin);
                let (tx, ty) = (tx * 2 + 5, ty * 2 - 11);
                for dy in -4..=3 {
                    self.resource_line((tx - 5, ty + dy), (10, 0), [229, 204, 155]);
                }
                let marks = 1 + (elapsed.saturating_mul(2) / duration.max(1)).min(2) as i32;
                for row in 0..marks {
                    self.resource_line((tx - 3, ty + row * 2 - 2), (5, 0), [115, 93, 75]);
                }
                self.resource_line((tx + 3, ty + 2), (4, -4), [107, 80, 64]);
            }
            return;
        }
        if let Some((elapsed, duration)) = detail {
            let middle = (duration / 4..duration.saturating_mul(3) / 4).contains(&elapsed);
            let side = if actor.facing_left { -1 } else { 1 };
            match actor.activity {
                Activity::GroomWhiskers if middle => {
                    self.resource_line((x + side * 4, y - 6), (side * 2, 1), [173, 102, 81]);
                    self.resource_texel((x + side * 6, y - 5), [237, 162, 126]);
                }
                Activity::StretchAtDoor if middle => {
                    self.resource_line((x + side * 3, y - 1), (side * 5, 0), [214, 134, 76]);
                    self.resource_texel((x + side * 8, y - 1), [238, 164, 99]);
                }
                Activity::DryCoat if middle && world.cat_coat_wet() => {
                    let drift = (elapsed / 18_000 % 3) as i32;
                    for dx in [-1, 1] {
                        self.resource_texel((x + dx * (6 + drift), y - 7 - drift), [144, 181, 192]);
                        self.resource_texel((x + dx * (8 + drift), y - 3 + drift), [174, 197, 199]);
                    }
                }
                _ => {}
            }
        }
        if let Some((stand, tree)) = world.cat_bloom_target()
            && actor.position == point(stand)
        {
            let (tx, ty) = local(point(tree), origin);
            let side = if tree.x == stand.x { if actor.facing_left { -1 } else { 1 } } else { (tx * 2 - x).signum() };
            let bow = i32::from(detail.is_some_and(|(elapsed, total)| (total / 3..total * 2 / 3).contains(&elapsed)));
            let end = (x + side * (8 - bow), y - 8 + bow);
            let twig = (tx * 2 - side * 3, ty * 2 - 15);
            self.resource_line(twig, (end.0 - twig.0, end.1 - twig.1), [131, 95, 68]);
            for (dx, dy) in [(-1, 0), (0, -1), (1, 0)] {
                self.resource_texel((end.0 + dx, end.1 + dy), [243, 177, 168]);
            }
            self.resource_texel(end, [251, 224, 172]);
        }
        if let Some((window, turn)) = world.cat_window_tap_visual() {
            let (wx, wy) = local(point(window), origin);
            let toward = if wx * 2 == x { if actor.facing_left { -1 } else { 1 } } else { (wx * 2 - x).signum() };
            let ear = (x + toward * 4, y - 11);
            self.resource_line(ear, (toward * 2, -2), [232, 147, 89]);
            if turn == 0 {
                self.resource_texel((wx * 2, wy * 2 - 9), [221, 190, 141]);
            } else {
                self.resource_texel((x + toward * 6, y - 7), [81, 60, 44]);
                self.resource_texel((x + toward * 7, y - 6), [235, 173, 106]);
            }
        }
        if village_cat_smelling(world, actor) {
            for (i, puff) in [(-12, -13), (-8, -14)].into_iter().enumerate() {
                let offset = ((world.world_ms() / 14_000 + i as u64) % 3) as i32;
                let at = (x + puff.0 + offset, y + puff.1 - offset);
                self.resource_line(at, (2, -1), [239, 205, 137]);
                self.resource_texel((at.0 + 2, at.1 - 2), [253, 221, 156]);
            }
        }
        if let Some((snow, elapsed, total)) = world.cat_snow_work() {
            let (tx, ty) = local(point(snow), origin);
            let direction = ((tx * 2 - x).signum(), (ty * 2 - y).signum());
            let total = total.max(1);
            let contact = elapsed.saturating_mul(10) / total;
            let reach = contact.min(10_u64.saturating_sub(contact)).min(5) as i32;
            let foot = (x + direction.0 * 4, y + direction.1 * 4 - 2);
            let paw = (foot.0 + direction.0 * reach, foot.1 + direction.1 * reach);
            self.resource_line(foot, (direction.0 * reach, direction.1 * reach), [198, 111, 51]);
            self.resource_line((paw.0 - 1, paw.1), (2, 0), [244, 180, 102]);
            if reach >= 4 {
                self.resource_texel((paw.0 + direction.0 * 2, paw.1 + direction.1), [236, 235, 204]);
                self.resource_texel((paw.0 - 2, paw.1 - 2), [216, 225, 216]);
            }
        }
    }

    fn village_rain_barrel(&mut self, (x, y): (i32, i32), units: u8, leaking: bool) {
        let (x, y) = (x * 2, y * 2);
        self.resource_line((x - 2, y - 18), (0, 7), [94, 110, 103]);
        self.resource_line((x - 2, y - 11), (3, 0), [121, 137, 123]);
        for row in -10..=-1 {
            let half = if (-8..=-3).contains(&row) { 5 } else { 4 };
            let ink = if row == -8 || row == -3 { [97, 79, 60] } else { [152, 106, 68] };
            self.resource_line((x - half, y + row), (2 * half, 0), ink);
        }
        self.resource_line((x - 3, y - 9), (6, 0), [105, 72, 48]);
        if units > 0 {
            self.resource_line((x - 3, y - 9), (i32::from(units.min(6)), 0), [106, 170, 169]);
        }
        if leaking && units > 0 {
            self.resource_texel((x + 4, y - 1), [142, 192, 185]);
            self.resource_texel((x + 5, y + 1), [90, 150, 149]);
        }
    }

    fn village_field_water(&mut self, origin: (i32, i32), work: FieldWaterWork, now: u64) {
        let (x, y) = local(point(work.tile), origin);
        if !clip(self, (x, y), (24, 24), 16) { return; }
        let (x, y) = (x * 2, y * 2);
        let phase = i32::from(work.phase.min(2));
        for strip in 0..3 {
            let sy = y + strip * 5 - 6;
            match work.kind {
                FieldWaterKind::Soak if strip < phase => {
                    self.resource_line((x - 7, sy), (14, 1), [75, 89, 77]);
                    self.resource_line((x - 5, sy + 1), (9, 0), [101, 125, 103]);
                }
                FieldWaterKind::Drain if strip >= phase => {
                    let shift = ((now / 18_000 + strip as u64) % 4) as i32;
                    self.resource_line((x - 6, sy), (13, 0), [85, 135, 124]);
                    self.resource_line((x - 6 + shift, sy + 1), (8, 0), [148, 177, 141]);
                }
                _ => {}
            }
        }
        let sy = y + phase * 5 - 6;
        let drop = ((now / 12_000) % 4) as i32;
        match work.kind {
            FieldWaterKind::Soak => {
                self.resource_texel((x - 4 + drop * 2, sy - 1 + drop / 2), [179, 209, 180]);
                self.resource_texel((x - 7 + drop * 2, sy - 2 + drop / 2), [155, 190, 159]);
            }
            FieldWaterKind::Drain => {
                self.resource_line((x + 1, sy + 1), (6, 2), [86, 133, 106]);
                self.resource_texel((x + 3 + drop, sy + 1 + drop / 2), [149, 175, 140]);
            }
        }
    }

    fn village_compost_bin(&mut self, (x, y): (i32, i32), units: u8, stage: u8) {
        let (x, y) = (x * 2, y * 2);
        for inset in 0..6 {
            let ink = if inset % 3 == 0 { [102, 70, 42] } else { [156, 109, 63] };
            self.resource_line((x - 7, y - inset), (14, 0), ink);
        }
        let ink = match stage {
            0 => [143, 114, 61],
            1 => [107, 83, 57],
            2 => [90, 74, 54],
            _ => [74, 61, 47],
        };
        self.resource_line((x - 5, y - 6), (10, 0), ink);
        self.resource_line((x - 4, y - 7), (8, 0), ink);
        if stage == 0 {
            for (dx, dy) in [(-4,-7),(-1,-8),(2,-7),(4,-8)].into_iter().take(usize::from(units.clamp(1,4))) {
                self.resource_texel((x + dx, y + dy), [183, 144, 76]);
            }
        }
    }

    fn village_hoe_mud(&mut self, world: &LivingWorld, actor: &Actor, origin: (i32, i32)) {
        if !world.hoe_has_mud(actor.kind) || actor.has_moving_box() || actor.fishing_result().is_some() {
            return;
        }
        let cleaning = world.hoe_cleaning_progress_ms(actor.kind);
        if cleaning.is_none() && !matches!(actor.pose, Pose::Idle | Pose::Walk | Pose::Carry | Pose::Rest) {
            return;
        }
        let now = world.world_ms();
        let frame = actor.walking_progress(now).map_or((now / 8_000 % 3) as usize, |p| ((p * 3.0).floor() as usize).min(2));
        let gender = if actor.kind == ActorKind::Xiaohe { "female" } else { "male" };
        let key = format!("{gender}_{}_{frame}", village_actor_clip(actor));
        let side = if actor.facing_left { -1 } else { 1 };
        let (x, y) = local(actor.position, origin);
        let hand = self.premium_hand_at_scale(&key, (x, y), actor.facing_left, Scale::new(3, 4))
            .unwrap_or(((x + 3 * side) * 2, (y - if actor.pose == Pose::Rest { 0 } else { 7 }) * 2));
        let blade = (hand.0 - 8 * side, hand.1 + 6);
        let count = cleaning.map_or(6, |(elapsed, total)| (6 - (elapsed.saturating_mul(6) / total.max(1)).min(6)) as usize);
        for (dx, dy) in [(-1,0),(0,1),(1,0),(0,0),(-1,-1),(1,1)].into_iter().take(count) {
            self.resource_texel((blade.0 + side * dx, blade.1 + dy), [116, 76, 47]);
        }
        if let Some((elapsed, _)) = cleaning {
            let sweep = [-1, 0, 1, 0][(elapsed / 25_000 % 4) as usize];
            let palm = (blade.0 + sweep * side, blade.1 - 2);
            self.resource_line((palm.0 + 4 * side, palm.1 - 3), (-4 * side, 3), [164, 105, 68]);
            self.resource_line((palm.0 - side, palm.1), (2 * side, -1), [237, 183, 114]);
            if count > 0 && elapsed / 25_000 % 4 == 2 {
                self.resource_texel((blade.0 - 2 * side, blade.1 + 3), [120, 81, 54]);
            }
        }
    }

    fn village_menu_sign(&mut self, (x, y): (i32, i32), servings: u16, free: u8) {
        let (x, y) = (x * 2, y * 2);
        for dy in -15..-2 {
            let ink = if (-13..=-5).contains(&dy) { [201, 164, 99] } else { [106, 67, 40] };
            self.resource_line((x - 9, y + dy), (18, 0), ink);
            self.resource_texel((x - 9, y + dy), [111, 69, 43]);
            self.resource_texel((x + 9, y + dy), [111, 69, 43]);
        }
        self.resource_line((x - 5, y - 8), (8, 0), [242, 219, 159]);
        self.resource_line((x - 4, y - 7), (6, 0), [230, 198, 131]);
        if servings > 0 {
            self.resource_line((x - 3, y - 10), (4, 0), [169, 87, 52]);
            self.resource_line((x - 2, y - 11), (2, 0), [240, 179, 91]);
        }
        if servings > 0 && free > 0 {
            self.resource_line((x + 5, y - 11), (1, 2), [70, 125, 70]);
            self.resource_texel((x + 7, y - 12), [232, 213, 141]);
        } else {
            self.resource_line((x + 5, y - 10), (2, 0), [156, 74, 59]);
            self.resource_line((x + 5, y - 9), (2, 0), [217, 129, 77]);
        }
        self.resource_line((x - 6, y - 2), (0, 3), [112, 77, 47]);
        self.resource_line((x + 6, y - 2), (0, 3), [112, 77, 47]);
    }

    fn village_art_board(&mut self, (x, y): (i32, i32), board: VillageArtVisual) {
        let (x, y) = (x * 2, y * 2);
        if board.slot.is_none() {
            self.resource_line((x, y - 7), (-8, 7), [119, 74, 45]);
            self.resource_line((x, y - 7), (7, 7), [119, 74, 45]);
            self.resource_line((x + 1, y - 16), (0, 12), [164, 111, 63]);
        }
        for row in 0..12 {
            let color = if row == 0 || row == 11 { [119, 76, 43] } else { [229, 199, 145] };
            self.resource_line((x - 8, y + row - 17), (16, 0), color);
            self.resource_texel((x - 8, y + row - 17), [119, 76, 43]);
            self.resource_texel((x + 8, y + row - 17), [119, 76, 43]);
        }
        if board.progress < 15 { return; }
        match board.subject {
            VillageArtSubject::Blossom(tree) => {
                self.resource_line((x - 2,y - 7), (3,-6), [99, 75, 54]);
                self.resource_line((x, y - 10), (-4,-4), [131, 89, 57]);
                if board.progress >= 45 {
                    let pink = [[220,142,132],[233,165,146],[236,179,144],[214,132,140]][usize::from(tree.min(3))];
                    for (dx,dy) in [(-5,-14),(-4,-13),(-2,-15),(0,-14),(2,-13),(3,-15),(5,-12)] {
                        self.resource_line((x+dx,y+dy), (1, 0), pink);
                    }
                }
                if board.progress >= 80 {
                    self.resource_texel((x-4,y-14), [250,221,168]);
                    self.resource_texel((x+3,y-14), [248,218,168]);
                    self.resource_line((x-5,y-8), (3,0), [139,148,88]);
                }
            }
            VillageArtSubject::Reflection(level) => {
                for dy in -11..=-7 {
                    self.resource_line((x-7,y+dy),(14,0),if dy % 2 == 0 { [112,169,174] } else { [119,177,179] });
                }
                if board.progress >= 45 {
                    self.resource_line((x-7,y-12),(14,0),[110,134,81]);
                    self.resource_line((x-2,y-13),(5,0),[132,92,69]);
                    self.resource_line((x-2,y-14),(3+ i32::from(level.min(4)) / 2,0),[152,104,70]);
                }
                if board.progress >= 80 {
                    self.resource_line((x-2,y-9),(6,0),[216,203,152]);
                    self.resource_line((x-6,y-8),(4,0),[161,189,172]);
                }
            }
            VillageArtSubject::DuskSwatches(indices) => {
                for (i, color) in dusk_board_colors(indices).into_iter().enumerate() {
                    if usize::from(board.progress) < 20 + 27 * i { break; }
                    for dy in -14..=-9 {
                        self.resource_line((x - 7 + i as i32 * 5, y + dy), (3, 0), color);
                    }
                }
            }
            VillageArtSubject::DuskStreet(indices) => {
                let [wall, sky, grass] = dusk_board_colors(indices);
                for dy in -16..=-10 {
                    self.resource_line((x - 7, y + dy), (14, 0), sky);
                }
                if board.progress >= 40 {
                    for dy in -10..=-7 {
                        self.resource_line((x - 7, y + dy), (14, 0), grass);
                    }
                }
                if board.progress >= 65 {
                    for dy in -12..=-9 {
                        self.resource_line((x - 5, y + dy), (7, 0), wall);
                    }
                    let roof = wall.map(|c| c.saturating_sub(32));
                    self.resource_line((x - 6, y - 13), (5, -2), roof);
                    self.resource_line((x - 1, y - 15), (4, 2), roof);
                    self.resource_line((x - 5, y - 13), (7, 0), roof);
                }
                if board.progress >= 85 {
                    self.resource_line((x + 6, y - 7), (-3, -2), [188, 154, 111]);
                    self.resource_texel((x - 3, y - 11), [245, 200, 128]);
                    self.resource_texel((x, y - 11), [245, 200, 128]);
                }
            }
        }
    }

    pub(super) fn resource_fibre_bundle(&mut self, (x, y): (i32, i32), facing_left: bool) {
        let direction = if facing_left { -1 } else { 1 };
        for (row, color) in [
            [101, 75, 43],
            [210, 172, 89],
            [226, 189, 105],
            [181, 136, 62],
            [155, 111, 49],
            [88, 65, 39],
        ]
        .into_iter()
        .enumerate()
        {
            let y = y + row as i32 - 3;
            self.resource_line((x - 8 * direction, y + 2), (15 * direction, -4), color);
        }
        self.resource_line((x - direction, y - 3), (2 * direction, 6), [82, 52, 34]);
        self.resource_texel((x, y - 2), [243, 206, 135]);
        self.resource_texel((x + 7 * direction, y - 2), [239, 191, 99]);
    }

    fn felled_scenic_tree(&mut self, foot: (i32, i32), flip: bool, pending: u8, stump: bool) {
        let (x, y) = (foot.0 * 2, foot.1 * 2);
        self.contact_shadow((foot.0 + 1, foot.1), (8, 3), 30);
        if !stump {
            let direction = if flip { -1 } else { 1 };
            for (offset, ink) in [
                [79, 56, 40],
                [111, 77, 50],
                [148, 109, 66],
                [163, 118, 68],
                [94, 67, 44],
            ]
            .into_iter()
            .enumerate()
            {
                self.resource_line((x, y + offset as i32 - 3), (22 * direction, -8), ink);
            }
            self.resource_line(
                (x + 14 * direction, y - 7),
                (8 * direction, -11),
                [103, 76, 46],
            );
            self.resource_line(
                (x + 12 * direction, y - 6),
                (9 * direction, 2),
                [103, 76, 46],
            );
            self.resource_texel((x + 22 * direction, y - 9), [194, 150, 91]);
            self.resource_texel((x + 22 * direction, y - 8), [211, 169, 105]);
        }
        self.core_at_texel(
            "tree_stump",
            (x, y),
            flip,
            Scale::new(1, 2),
            Palette::Unchanged,
        );
        if pending > 0 {
            let offset = if flip { -8 } else { 8 };
            self.core_at_texel(
                "logs",
                (x + offset, y + 2),
                flip,
                Scale::new(2, 3),
                Palette::Unchanged,
            );
        }
    }

    fn cracked_scenic_rock(&mut self, (x, y): (i32, i32), flip: bool, pending: u8) {
        let (x, y) = (x * 2, y * 2);
        self.core_at_texel(
            "rocks_moss",
            (x - 7, y),
            flip,
            Scale::new(2, 5),
            Palette::Unchanged,
        );
        self.core_at_texel(
            "rocks_moss",
            (x + 5, y + 2),
            !flip,
            Scale::new(1, 3),
            Palette::Unchanged,
        );
        if pending > 0 {
            self.core_at_texel(
                "prop_stone",
                (x + 10, y - 2),
                false,
                STONE_PROP_SCALE,
                Palette::Unchanged,
            );
        }
    }

    fn cut_scenic_vegetation(&mut self, (x, y): (i32, i32), flip: bool, pending: u8) {
        let (x, y) = (x * 2, y * 2);
        for n in [-7, -2, 3, 8] {
            self.resource_line((x + n, y), (2, -2 - (n & 1)), [123, 112, 65]);
        }
        if pending > 0 {
            self.resource_fibre_bundle((x + 2, y - 3), flip);
        }
    }

    fn scenic_work_mark(&mut self, (x, y): (i32, i32), kind: ScenicKind, progress: u8, flip: bool) {
        let (x, y) = (x * 2, y * 2);
        let side = if flip { -1 } else { 1 };
        match kind {
            ScenicKind::Tree => {
                let width = i32::from(progress).clamp(1, 4);
                self.resource_line((x - side * 2, y - 8), (side * width, -2), [76, 49, 35]);
                self.resource_line((x - side * 2, y - 7), (side * width, -2), [190, 145, 82]);
            }
            ScenicKind::Rock => {
                self.resource_line((x + side * 2, y - 12), (side * 3, 4), [85, 82, 71]);
                self.resource_line((x + side * 5, y - 8), (-side * 2, 4), [82, 80, 70]);
                if progress > 1 {
                    self.resource_line((x + side * 3, y - 4), (-side * 4, 2), [101, 95, 79]);
                }
            }
            _ => self.resource_line((x - side * 3, y - 7), (side * 5, 4), [170, 151, 76]),
        }
    }

    pub(super) fn paint_village(
        &mut self,
        world: &LivingWorld,
        (cx, cy): (i32, i32),
        commands: &[FrameCommand],
        store: Option<&PluginStore>,
        visitors: &[VisitorVisual],
        wildlife: &[WildlifeVisual],
        stalls: &[StallVisual],
    ) {
        let origin = (cx, cy);
        let awake = super::village_camera::awake_protagonists(world);
        self.awake_screen_feet = [0, 1].map(|i| {
            let actor = &world.actors()[i];
            awake.contains(&Some(actor.kind)).then(|| {
                let (x, y) = local(actor.position, origin);
                (x * 2, y * 2)
            })
        });
        self.indoor_reveals = [0, 1].map(|i| {
            let actor = &world.actors()[i];
            if !actor.inside() { return None; }
            self.awake_screen_feet[i].or_else(|| {
                let (x, y) = local(actor.position, origin);
                (actor.pose == Pose::Work && clip(self, (x, y), (24, 32), 12)).then_some((x * 2, y * 2))
            })
        });
        self.paint_premium_ground(world, origin);
        for mark in world.village_footprints() {
            let (x, y) = local(point(mark.tile), origin);
            let (next_x, next_y) = local(point(mark.toward), origin);
            let (dx, dy) = ((next_x - x).signum(), (next_y - y).signum());
            let side = if mark.left_foot { -1 } else { 1 };
            let (x, y) = (x + dx * 2 - dy * side, y + dy * 2 + dx * side);
            let ink = if mark.snow {
                [177, 182, 156]
            } else {
                [103, 100, 65]
            };
            self.pixel(x, y, ink);
            if mark.kind != ActorKind::Orange {
                self.pixel(x - dx, y - dy, ink);
            }
        }
        for signal in world.active_village_observations() {
            use crate::living_world::{Observation, ObservationEvidence};
            let (tile, ink) = match signal.evidence {
                ObservationEvidence::Step(step) => (step.from, [125, 105, 74]),
                ObservationEvidence::Removed(_) => (signal.target, [152, 120, 82]),
                ObservationEvidence::Place => match signal.kind {
                    Observation::FallingTree => (signal.target, [161, 116, 57]),
                    Observation::CloudBreak => (signal.target, [205, 174, 101]),
                    Observation::RainShelter => (signal.target, [133, 156, 149]),
                    Observation::FishingWater => (signal.target, [120, 174, 173]),
                    _ => continue,
                },
            };
            let (x, y) = local(point(tile), origin);
            if clip(self, (x, y), (10, 8), 0) {
                if let ObservationEvidence::Step(step) = signal.evidence {
                    let dx = (i32::from(step.to.x) - i32::from(step.from.x)).signum();
                    let dy = (i32::from(step.to.y) - i32::from(step.from.y)).signum();
                    self.pixel(x + dx, y + dy, ink);
                    self.pixel(x + dx * 2, y + dy * 2, ink);
                    self.pixel(x - dx + dy, y - dy - dx, ink);
                } else {
                    for (dx, dy) in [(-2, 2), (-1, 1), (0, 0), (1, 1), (2, 2), (0, 2)] {
                        self.pixel(x + dx, y + dy, ink);
                    }
                }
            }
        }
        let phases = world.phase_mask();
        let rect = ArtRect {
            x: cx * 2,
            y: cy * 2,
            w: self.width,
            h: self.height,
        };
        let canopy_subjects = canopy_subjects(world, origin, rect, commands, store);
        let spatial = self
            .village_index
            .get_or_insert_with(|| VillageIndex::new(self.art.as_ref(), self.village_art.as_ref()));
        let mut active = spatial.in_view(rect);
        let scenic = spatial.scenic_in_view(rect, phases);
        let mut drawables = Vec::with_capacity(128 + scenic.len());
        let mut dynamic_count = 0;
        let mut push = |depth, item| {
            // The scenery list is already finite and exactly viewport-clipped.
            // Large windows may show hundreds of trees without exhausting the
            // separate budget for actors, facilities and moving objects.
            let fixed_scenery = matches!(item, Visible::Scenic(..));
            if fixed_scenery || dynamic_count < 512 {
                drawables.push((depth, item));
                dynamic_count += usize::from(!fixed_scenery);
            }
        };
        if world.village_market_drained()
            && let Some(well) = WILLOW_VILLAGE.poi("market_well")
        {
            // Flush grating along the west edge of the well approach.
            let mut p = point(well.tile);
            p.x -= 6.0;
            p.y += 5.0;
            if clip(self, local(p, origin), (5, 4), 1) {
                push(p.y as i32, Visible::Drain(p));
            }
        }
        while active != 0 {
            let id = active.trailing_zeros() as usize;
            active &= active - 1;
            let b = &WILLOW_VILLAGE.buildings[id];
            if world.village_building_active(b.id) {
                let north_facing = b.kind == "stall" && b.door.y < b.footprint.y;
                let inset = if b.kind == "stall" { 4 } else { 0 };
                let depth = if north_facing {
                    point(b.door).y as i32 - 12
                } else {
                    point(b.door).y as i32 + inset
                };
                let vacant = self.vacant_residences & (1_u64 << id) != 0;
                push(depth, Visible::Building(b, vacant));
            }
        }
        for b in world.buildings() {
            if clip(self, local(b.position, origin), (59, 60), 10) {
                push(building_baseline(b), Visible::Legacy(Drawable::Building(b)));
                if b.kind == BuildingKind::Kitchen {
                    let (x, y) = local(b.position, origin);
                    let p = Point {
                        x: b.position.x + 7.0,
                        y: b.position.y - 12.0,
                    };
                    push(
                        building_baseline(b) + 1,
                        Visible::DoorTrim("village_kitchen_mark", p),
                    );
                    // An installed stove belongs to the kitchen, even when cold.
                    // Show it through the existing indoor reveal, at its fixed point.
                    if self
                        .indoor_reveals
                        .iter()
                        .flatten()
                        .any(|&(ax, ay)| (ax - x * 2).abs() < 24 && (ay - y * 2).abs() < 14)
                    {
                        let p = Point {
                            x: b.position.x + 6.0,
                            y: b.position.y - 8.0,
                        };
                        push(building_baseline(b) + 1, Visible::DoorTrim("prop_stove", p));
                    }
                }
            }
        }
        if let Some(mill) = world
            .mill_display_site()
            .and_then(|site| WILLOW_VILLAGE.building(site))
        {
            let door = point(mill.door);
            // Fix the equipment to the south wall, on the solid part of the
            // structure. The actual operator and handoff still use the door.
            let p = Point {
                x: door.x - 12.0,
                y: door.y - 4.0,
            };
            if clip(self, local(p, origin), (19, 23), 5) {
                push(door.y as i32 + 1, Visible::DoorTrim("village_mill", p));
            }
        }
        let mut housing_footprints = [TileRect::new(0, 0, 0, 0); 3];
        for (site, footprint) in world.housing_sites().zip(&mut housing_footprints) {
            *footprint = housing_footprint(site);
            if clip(self, local(site.position, origin), (59, 58), 10) {
                push(
                    site.position.y as i32 - 4,
                    Visible::Legacy(Drawable::Housing(site)),
                );
            }
        }
        let mut construction_sites = 0_u64;
        for project in world.developments() {
            if project.complete || project.progress == 0 {
                continue;
            }
            if matches!(
                project.kind,
                DevelopmentKind::Road | DevelopmentKind::Bridge
            ) {
                continue;
            }
            let site = development_building(&project).and_then(|id| {
                WILLOW_VILLAGE
                    .buildings
                    .iter()
                    .enumerate()
                    .find(|(_, b)| b.id == id)
            });
            let structure = if let Some((index, building)) = site {
                construction_sites |= 1 << index;
                structure_anchor(building)
            } else {
                project.position
            };
            let asset = if site.is_some() && project.phase >= 2 {
                "village_public_frame"
            } else {
                "village_public_foundation"
            };
            if clip(self, local(structure, origin), (34, 46), 6) {
                push(structure.y as i32, Visible::Prop(asset, structure));
            }
        }
        for bridge in WILLOW_VILLAGE.bridges {
            if !phases.contains(bridge.open_phase) {
                continue;
            }
            let width = if bridge.upgrade.is_some_and(|p| !phases.contains(p)) {
                bridge.initial_width
            } else {
                bridge.full_width
            };
            let Some((first, last)) = bridge.axis.first().zip(bridge.axis.last()) else {
                continue;
            };
            let bridge_left = i32::from(first.x.min(last.x)) * 8 - cx;
            let bridge_right = (i32::from(first.x.max(last.x)) + 1) * 8 - cx;
            let bridge_top = i32::from(first.y) * 8 - cy;
            if bridge_right > 0
                && bridge_left < (self.width + 1) / 2
                && bridge_top + i32::from(width) * 8 > 0
                && bridge_top - 8 < (self.height + 1) / 2
            {
                self.village_bridge_rail(*first, *last, 0, origin);
                push(
                    (first.y as i32 + i32::from(width)) * 8,
                    Visible::BridgeRail(*first, width),
                );
            }
        }
        for (kind, b) in [ActorKind::Xiaohe, ActorKind::Ayu]
            .into_iter()
            .zip(world.buildings())
        {
            if clip(self, local(b.position, origin), (27, 32), 7) {
                if let Some(use_kind) = world.previous_home_use(kind) {
                    push(
                        b.position.y as i32,
                        Visible::Legacy(Drawable::FormerHome(use_kind, b.position)),
                    );
                } else if let Some(stage @ 0..=2) = world.home_renovation_stage(kind) {
                    let trim = if stage == 0 {
                        "village_relocation"
                    } else {
                        "village_remodel"
                    };
                    push(b.position.y as i32, Visible::DoorTrim(trim, b.position));
                }
            }
        }
        for (i, t) in world.trees().iter().enumerate() {
            if clip(self, local(t.position, origin), (21, 40), 9) {
                push(t.position.y as i32, Visible::Legacy(Drawable::Tree(i, t)));
            }
        }
        for (index, (plot, def)) in world.plots().iter().zip(WILLOW_VILLAGE.crops).enumerate() {
            if !phases.contains(def.phase)
                || !clip(self, local(plot.position, origin), (23, 22), 12)
            {
                continue;
            }
            self.village_crop_footprint(plot, world.plot_stage(index), def.rect, origin);
        }
        let restored = [0, 1].map(|slot| {
            let index = WILLOW_VILLAGE.crops.len() + slot;
            world
                .restored_plot_site(index)
                .map(|(rect, _)| (index, rect))
        });
        for (index, rect) in restored.iter().flatten().copied() {
            if let Some(plot) = world.plots().get(index)
                && clip(self, local(plot.position, origin), (23, 22), 12)
            {
                self.village_crop_footprint(plot, world.plot_stage(index), rect, origin);
            }
        }
        if let Some((tile, size)) = world.root_shoulder_visual() {
            let p = local(point(tile), origin);
            if clip(self, p, (16, 16), 3) {
                self.village_root_shoulder(p, size);
            }
        }
        if let Some((tile, profile)) = self.village_soil_window {
            let p = local(point(tile), origin);
            if clip(self, p, (16, 12), 3) {
                self.village_soil_cut(p, profile);
            }
        }
        for kind in [ActorKind::Xiaohe, ActorKind::Ayu] {
            if let Some(work) = world.field_water_work(kind) {
                self.village_field_water(origin, work, world.world_ms());
            }
        }
        for (index, actor) in world.actors().iter().enumerate() {
            let overridden = commands
                .iter()
                .any(|c| c.actor_override == Some(actor.kind));
            let revealed = self.indoor_reveals.get(index).is_some_and(Option::is_some);
            if (!actor.inside() || revealed)
                && !overridden
                && clip(self, local(actor.position, origin), (32, 35), 28)
            {
                push(
                    actor.position.y as i32 - if revealed { 24 } else { 0 },
                    Visible::Legacy(Drawable::Actor(index)),
                );
            }
        }
        for (id, key, slot) in STATIC_PROPS {
            let Some(mut p) = world.resolve_poi(id) else {
                continue;
            };
            if let Some(slot) = slot {
                let Some(SlotPos::Outdoor(tile)) =
                    WILLOW_VILLAGE.slot(id, slot).map(|s| s.position)
                else {
                    continue;
                };
                p = point(tile);
            }
            if matches!(id, "market_well" | "market_notice") {
                let Some(prop) = WILLOW_VILLAGE
                    .solid_props
                    .iter()
                    .find(|prop| prop.reader_poi == Some(id))
                else {
                    continue;
                };
                p = point(prop.tile);
            }
            if clip(self, local(p, origin), (34, 43), 6) {
                let key = if id == "cat_water" && world.bowl_water() != 0 {
                    "prop_bowl_full"
                } else {
                    key
                };
                push(p.y as i32, Visible::Prop(key, p));
            }
        }
        if world.cat_food_served() && let Some(bowls) = world.resolve_poi("cat_water") {
            let plate = Point { x: bowls.x - 8.0, y: bowls.y + 1.0 };
            if clip(self, local(plate, origin), (10, 10), 2) {
                push(plate.y as i32 - 2, Visible::CatPlate(plate, world.cat_fish_served()));
            }
        }
        if let Some((tile, units, stage)) = world.village_compost_visual() {
            let p = point(tile);
            let bin = Point { x: p.x + 6.0, y: p.y - 2.0 };
            if clip(self, local(bin, origin), (12, 10), 3) {
                push(bin.y as i32, Visible::Compost(bin, units, stage));
            }
        }
        if let Some((tile, units, leak)) = self.village_rain_barrel {
            let door = point(tile);
            let barrel = Point { x: door.x + 9.0, y: door.y - 1.0 };
            if clip(self, local(barrel, origin), (13, 20), 4) {
                push(door.y as i32 + 1, Visible::RainBarrel(barrel, units, leak));
            }
        }
        if let Some((tile, tree, progress)) = self.village_leaf_card {
            let p = point(tile);
            if clip(self, local(p, origin), (18, 20), 4) {
                push(p.y as i32 + 2, Visible::LeafCard(p, tree, progress));
            }
        }
        if let Some((tile, full)) = self.village_food_saucer {
            let p = point(tile);
            if clip(self, local(p, origin), (10, 8), 2) {
                push(p.y as i32, Visible::FoodSaucer(p, full, world.village_mud_at(tile)));
            }
        }
        if let Some(tile) = world.cat_door_fur() {
            let p = point(tile);
            if clip(self, local(p, origin), (9, 8), 3) {
                push(p.y as i32 + 1, Visible::DoorFur(p));
            }
        }
        if let Some((tile, repaired, filed)) = world.yard_flood_report() {
            let p = point(tile);
            if clip(self, local(p, origin), (10, 14), 3) {
                push(p.y as i32, Visible::YardReport(p, repaired, filed));
            }
        }
        if let Some((corners, count)) = world.yard_survey_line() {
            let count = usize::from(count.min(4));
            let ends = if count == 4 { 4 } else { count.saturating_sub(1) };
            for i in 0..ends {
                let (a, b) = (local(point(corners[i]), origin), local(point(corners[(i + 1) % 4]), origin));
                self.resource_line((a.0 * 2, a.1 * 2), ((b.0 - a.0) * 2, (b.1 - a.1) * 2), [191, 156, 81]);
            }
        }
        for kind in [ActorKind::Xiaohe, ActorKind::Ayu] {
            if world.raincoat_on_home_hook(kind) && let Some(door) = world.home_of(kind) {
                let hook = Point { x: door.x - 9.0, y: door.y - 5.0 };
                if clip(self, local(hook, origin), (8, 12), 3) {
                    push(door.y as i32 + 1, Visible::CoatHook(hook));
                }
            }
        }
        if let Some(stand) = stalls
            .iter()
            .find(|s| s.site == "home_trade" && s.open && s.quantity > 0)
        {
            let p = point(stand.tile);
            if clip(self, local(p, origin), (25, 25), 6) {
                push(p.y as i32, Visible::Prop("shared_table", p));
                let mut goods = p;
                goods.y -= 4.0;
                push(p.y as i32 + 1, Visible::Prop(item_asset(stand.item), goods));
            }
        }
        if let Some(SlotPos::Outdoor(tile)) = WILLOW_VILLAGE
            .slot("pantry_loading", "pantry_loading.tray")
            .map(|slot| slot.position)
            && world.wheat_drying_at(tile)
        {
            let p = point(tile);
            if clip(self, local(p, origin), (25, 25), 6) {
                push(p.y as i32, Visible::Prop("village_drying", p));
            }
        }
        let mut processing_tools = [false; 2];
        for actor in &world.actors()[..2] {
            if actor.pose != Pose::Work
                || actor.carried != Some(Item::Crops)
                || actor.carried_amount() == 0
            {
                continue;
            }
            let (i, site, slot, key) = match actor.activity {
                Activity::WashHarvest => {
                    (0, "home_well", "home_well.bucket_rest", "village_washing")
                }
                Activity::SortHarvest | Activity::PackHarvest => (
                    1,
                    "pantry_loading",
                    "pantry_loading.tray",
                    "village_sorting",
                ),
                _ => continue,
            };
            if processing_tools[i] {
                continue;
            }
            let Some(SlotPos::Outdoor(tile)) =
                WILLOW_VILLAGE.slot(site, slot).map(|slot| slot.position)
            else {
                continue;
            };
            let p = point(tile);
            if (p.x - actor.position.x).abs() <= 10.0
                && (p.y - actor.position.y).abs() <= 10.0
                && clip(self, local(p, origin), (18, 18), 3)
                && !world.wheat_drying_at(tile)
            {
                processing_tools[i] = true;
                push(p.y as i32, Visible::Prop(key, p));
            }
        }
        for v in visitors.iter().take(36) {
            if clip(self, local(v.position, origin), (26, 30), 6) {
                self.visible_motion |=
                    matches!(v.pose, VisitorPose::Walking | VisitorPose::Carrying)
                        && visible_character_motion(self, v.position, origin);
                push(v.position.y as i32, Visible::Resident(v));
            }
        }
        for v in wildlife.iter().take(96) {
            if clip(self, local(v.position, origin), (16, 19), 6) {
                let depth = if v.sprite == "wild_white_pigeon_idle_0" {
                    WILLOW_VILLAGE
                        .building("commons_hall")
                        .filter(|hall| {
                            world.village_building_active(hall.id)
                                && hall
                                    .door
                                    .offset(0, -4)
                                    .is_some_and(|tile| point(tile) == v.position)
                        })
                        .map_or(v.position.y as i32, |hall| point(hall.door).y as i32 + 1)
                } else {
                    crate::residents::wildlife_layered_depth(world, v).unwrap_or(v.position.y as i32)
                };
                push(depth, Visible::Wild(v));
            }
        }
        for command in commands.iter().take(128) {
            if clip(self, local(command.point, origin), (130, 130), 130) {
                push(command.depth, Visible::Legacy(Drawable::Plugin(command)));
            }
        }
        for (status, point) in world.building_fixtures() {
            if clip(self, local(point, origin), (48, 50), 10) {
                push(
                    point.y as i32,
                    Visible::Legacy(Drawable::InstalledFixture(status, point)),
                );
            }
        }
        if let Some((status, point)) = world.active_building_improvement()
            && clip(self, local(point, origin), (48, 50), 10)
        {
            push(
                point.y as i32,
                Visible::Legacy(Drawable::FixtureWork(status, point)),
            );
        }
        let lamps = world.market_lamp_tiles();
        let has_density_holdouts = !world.map_density_holdouts().is_empty();
        for id in scenic {
            let item = &WILLOW_VILLAGE.scenic[id];
            let pos = point(item.tile);
            let replaced = item.kind == ScenicKind::Lamp && lamps.contains(&item.tile);
            if !replaced
                && (!has_density_holdouts || world.map_density_scenic_visible(item.tile))
                && !restored
                    .iter()
                    .flatten()
                    .any(|(_, rect)| rect.contains(item.tile))
                && (!item.temporary_ground
                    || temporary_ground_visible(
                        world,
                        item.tile,
                        phases,
                        construction_sites,
                        &housing_footprints,
                    ))
            {
                let resource = world.scenic_resource(id);
                if resource.is_none_or(|resource| resource.stage != ScenicStage::Cleared) {
                    push(pos.y as i32, Visible::Scenic(item, resource));
                }
            }
        }
        for &tile in lamps {
            let p = point(tile);
            if clip(self, local(p, origin), (20, 21), 6) {
                push(p.y as i32, Visible::Lamp(p));
            }
        }
        for reward in REWARDS
            .iter()
            .filter(|r| self.companionship_rewards & r.bit != 0)
        {
            let mut p = point(reward.tile);
            let asset = match reward.style {
                RewardStyle::PawTag => "village_reward_paw",
                RewardStyle::LanternRibbon => {
                    p.y -= 8.0;
                    "village_reward_lantern"
                }
                RewardStyle::Keepsake => {
                    p.y -= 6.0;
                    "village_reward_stones"
                }
                RewardStyle::Card => "village_reward_card",
                RewardStyle::BenchCushion => {
                    p.y -= 5.0;
                    "village_reward_cushion"
                }
                RewardStyle::MemoryRibbon => {
                    p.y -= 8.0;
                    "village_reward_tree"
                }
                RewardStyle::Portrait => "village_reward_portrait",
                RewardStyle::AnniversaryPlaque => "village_reward_plaque",
                RewardStyle::CelebrationPennant => {
                    p.y -= 10.0;
                    "village_reward_flags"
                }
            };
            if clip(self, local(p, origin), (13, 14), 4) {
                push(point(reward.tile).y as i32 + 1, Visible::Prop(asset, p));
            }
        }
        if let Some((tile, season)) = self.village_public_corner {
            let mut p = point(tile);
            p.x += 7.0;
            p.y -= 7.0;
            if clip(self, local(p, origin), (13, 14), 4) {
                let decoration = match season {
                    0 => "village_reward_tree",
                    1 => "village_reward_lantern",
                    2 => "village_reward_flags",
                    _ => "village_reward_card",
                };
                push(point(tile).y as i32 + 1, Visible::Prop(decoration, p));
            }
        }
        if let Some((servings, free)) = self.village_menu
            && let Some(stall) = WILLOW_VILLAGE.building("stall_meal")
            && world.village_building_active(stall.id)
        {
            let door = point(stall.door);
            let sign = Point { x: door.x + 14.0, y: door.y + 5.0 };
            if clip(self, local(sign, origin), (13, 17), 3) {
                push(door.y as i32 + 12, Visible::Menu(sign, servings, free));
            }
        }
        for board in self.village_art_boards.into_iter().flatten() {
            let foot = point(board.tile);
            let (p, depth) = if let Some(slot @ 0..=2) = board.slot {
                (Point { x: foot.x - 15.0 + f32::from(slot) * 10.0, y: foot.y - 11.0 }, foot.y as i32 + 1)
            } else if board.slot.is_none() {
                (Point { x: foot.x - 7.0, y: foot.y + 3.0 }, foot.y as i32 + 3)
            } else {
                continue;
            };
            if clip(self, local(p, origin), (13, 16), 3) {
                push(depth, Visible::Artwork(p, board));
            }
        }
        drop(push);
        drawables.sort_by_key(|(depth, _)| *depth);
        let mut labels = Vec::with_capacity(12);
        for (_, drawable) in drawables {
            match drawable {
                Visible::Building(b, vacant) => {
                    self.village_building(b, world, origin, stalls, vacant)
                }
                Visible::Resident(v) => {
                    self.village_resident(v, origin);
                    labels.push(v);
                }
                Visible::Wild(v) => {
                    let (x, y) = local(v.position, origin);
                    if v.sprite.starts_with("wild_acorn") {
                        let (x, y) = (x * 2, y * 2);
                        match v.sprite {
                            "wild_acorn" | "wild_acorn_held" => {
                                self.resource_line((x - 2, y), (4, 0), [91, 64, 47]);
                                self.resource_line((x - 1, y - 1), (2, 0), [209, 156, 93]);
                                self.resource_texel((x, y - 2), [103, 79, 49]);
                            }
                            "wild_acorn_covered_soil" => {
                                self.resource_line((x - 4, y + 1), (8, 0), [123, 94, 61]);
                                self.resource_line((x - 2, y), (4, 0), [169, 123, 80]);
                            }
                            "wild_acorn_sprout" => {
                                self.resource_line((x, y + 2), (0, -7), [84, 111, 59]);
                                self.resource_line((x, y - 2), (-4, -3), [114, 157, 71]);
                                self.resource_line((x, y - 3), (4, -3), [162, 183, 82]);
                            }
                            "wild_acorn_young_oak" => {
                                self.resource_line((x, y + 1), (0, -11), [115, 87, 57]);
                                self.resource_line((x, y - 4), (-4, -4), [86, 120, 66]);
                                self.resource_line((x, y - 6), (4, -5), [109, 145, 62]);
                                self.resource_line((x - 5, y - 9), (6, 0), [146, 167, 74]);
                                self.resource_line((x + 2, y - 11), (4, 0), [159, 178, 79]);
                            }
                            "wild_acorn_cover_leaf" | "wild_acorn_leaf_held" => {
                                self.resource_line((x - 3, y + 1), (6, -2), [147, 96, 59]);
                                self.resource_line((x - 2, y), (4, -2), [210, 145, 67]);
                            }
                            "wild_acorn_leaf_scraps" => {
                                for (dx, dy) in [(-4, 1), (1, -2), (3, 2)] {
                                    self.resource_texel((x + dx, y + dy), [178, 122, 66]);
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }
                    if matches!(v.sprite, "wild_swallow_nest_base" | "wild_swallow_nest") {
                        let (x, y) = (x * 2, y * 2);
                        let finished = v.sprite == "wild_swallow_nest";
                        if finished {
                            for (dy, half, ink) in [
                                (2, 6, [109, 90, 63]),
                                (3, 7, [177, 135, 92]),
                                (4, 7, [151, 105, 76]),
                                (5, 6, [139, 98, 70]),
                                (6, 5, [125, 89, 66]),
                                (7, 4, [103, 75, 54]),
                                (8, 2, [88, 63, 49]),
                            ] {
                                self.resource_line((x - half, y + dy), (2 * half, 0), ink);
                            }
                            for (dx, dy) in [(-5, 4), (-2, 5), (2, 4), (4, 3)] {
                                self.resource_texel((x + dx, y + dy), [196, 151, 103]);
                            }
                        } else {
                            self.resource_line((x - 5, y + 3), (10, 0), [153, 114, 82]);
                            self.resource_line((x - 4, y + 4), (8, 0), [108, 80, 56]);
                        }
                        continue;
                    }
                    if v.sprite == "wild_swallow_mud_in_beak" {
                        let face = if v.facing_left { -1 } else { 1 };
                        let (x, y) = (x * 2 + 4 * face, y * 2 - 5);
                        self.resource_line((x, y), (2 * face, 0), [167, 122, 83]);
                        self.resource_texel((x + face, y + 1), [106, 77, 55]);
                        continue;
                    }
                    if matches!(v.sprite, "wild_snail_trace" | "wild_snail_trace_faint") {
                        let (x, y) = (x * 2, y * 2);
                        let ink = if v.sprite == "wild_snail_trace" { [143, 170, 144] } else { [118, 147, 123] };
                        self.resource_line((x - 1, y - 7), (2, 4), ink);
                        self.resource_line((x + 1, y - 3), (-2, 5), ink);
                        self.resource_line((x - 1, y + 2), (1, 5), ink);
                        continue;
                    }
                    if v.sprite == "wild_duck_track" {
                        let (tx, ty) = ((v.position.x / TILE_PX as f32) as i32, (v.position.y / TILE_PX as f32) as i32);
                        if matches!(world.ground(tx, ty), Ground::Grass | Ground::Soil) && world.can_walk(tx, ty) {
                            let ink = [105, 94, 55];
                            for (dx, dy) in [(-3, 1), (1, -2)] {
                                let (fx, fy) = (x * 2 + dx, y * 2 + dy);
                                self.resource_line((fx, fy), (0, 2), ink);
                                self.resource_line((fx - 1, fy), (2, -1), ink);
                            }
                        }
                        continue;
                    }
                    if v.sprite == "wild_sparrow_chirp" {
                        let (x, y) = (x * 2 + 4, y * 2 - 10);
                        self.resource_line((x, y + 4), (0, -4), [229, 209, 151]);
                        self.resource_texel((x + 1, y + 1), [234, 201, 146]);
                        self.resource_texel((x - 1, y + 4), [250, 226, 169]);
                        continue;
                    }
                    if crate::residents::wildlife_is_mouse_den(v) {
                        self.resource_line((x * 2 - 3, y * 2), (6, 0), [86, 79, 48]);
                        self.resource_line((x * 2 - 2, y * 2 - 1), (4, 0), [60, 58, 40]);
                        continue;
                    }
                    if v.sprite == "wild_frog_land_0" && world.ground(
                        (v.position.x / TILE_PX as f32).floor() as i32,
                        (v.position.y / TILE_PX as f32).floor() as i32,
                    ) == Ground::Water
                    {
                        let ink = [153, 193, 173];
                        self.resource_line((x * 2 - 3, y * 2 + 2), (6, 0), ink);
                        self.resource_texel((x * 2 - 5, y * 2), ink);
                        self.resource_texel((x * 2 + 5, y * 2), ink);
                    }
                    self.draw_village_sprite(
                        Bank::Wildlife,
                        v.sprite,
                        (x * 2, y * 2),
                        v.facing_left,
                        Scale::IDENTITY,
                        Palette::Unchanged,
                    );
                }
                Visible::Menu(p, servings, free) => self.village_menu_sign(local(p, origin), servings, free),
                Visible::Artwork(p, board) => self.village_art_board(local(p, origin), board),
                Visible::CatPlate(p, fish) => {
                    let (x, y) = local(p, origin);
                    if fish {
                        self.core_at_texel("prop_bowl", (x * 2, y * 2), false, Scale::new(2, 5), Palette::Unchanged);
                        self.core_at_texel("prop_fish", (x * 2, y * 2 - 2), false, Scale::new(2, 5), Palette::Unchanged);
                    } else {
                        self.core_at_texel("prop_meal", (x * 2, y * 2), false, Scale::new(2, 5), Palette::Unchanged);
                    }
                }
                Visible::Compost(p, units, stage) => self.village_compost_bin(local(p, origin), units, stage),
                Visible::RainBarrel(p, units, leak) => self.village_rain_barrel(local(p, origin), units, leak),
                Visible::FoodSaucer(p, full, muddy) => {
                    let (x, y) = local(p, origin);
                    let (x, y) = (x * 2, y * 2);
                    if muddy {
                        self.resource_line((x - 5, y + 2), (11, 0), [123, 93, 69]);
                        self.resource_line((x - 4, y + 3), (9, 0), [135, 105, 80]);
                    }
                    self.village_food_saucer((x, y), full);
                }
                Visible::LeafCard(p, tree, progress) => self.village_leaf_rubbing(local(p, origin), tree, progress),
                Visible::CoatHook(p) => {
                    let (x, y) = local(p, origin);
                    let (x, y) = (x * 2, y * 2);
                    for dy in -8..=0 {
                        let half = if dy < -5 { 2 } else { 4 };
                        self.resource_line((x - half, y + dy), (2 * half, 0), [166, 140, 69]);
                        self.resource_texel((x, y + dy), [200, 165, 86]);
                    }
                    self.resource_line((x, y - 10), (0, 2), [92, 78, 66]);
                }
                Visible::DoorFur(p) => {
                    let (x, y) = local(p, origin);
                    let (x, y) = (x * 2, y * 2);
                    self.resource_line((x + 3, y - 5), (3, -2), [191, 130, 74]);
                    self.resource_line((x + 5, y - 8), (2, 1), [230, 165, 97]);
                }
                Visible::YardReport(p, repaired, filed) => {
                    let (x, y) = local(p, origin);
                    let (x, y) = (x * 2 + 5, y * 2 - 3);
                    self.resource_line((x, y + 2), (0, -10), [110, 83, 52]);
                    let tint = if filed { [105, 145, 104] } else if repaired { [211, 180, 96] } else { [210, 131, 69] };
                    self.resource_line((x, y - 8), (4, 0), tint);
                    self.resource_line((x, y - 7), (3, 0), tint);
                }
                Visible::Prop(key, p) => self.village_prop(key, p, world, origin),
                Visible::DoorTrim(key, p) => {
                    let (x, y) = local(p, origin);
                    if key == "village_kitchen_mark" {
                        // A small stove insignia on the kitchen lintel.
                        self.rect(x - 2, y - 5, 5, 4, [100, 66, 45]);
                        self.rect(x - 1, y - 4, 3, 2, [208, 172, 103]);
                        self.pixel(x, y - 3, [232, 117, 76]);
                        continue;
                    }
                    self.draw_village_sprite(
                        if key == "prop_stove" {
                            Bank::Core
                        } else {
                            Bank::Village
                        },
                        key,
                        (x * 2, (y - 1) * 2),
                        false,
                        Scale::IDENTITY,
                        Palette::Unchanged,
                    );
                }
                Visible::Scenic(item, resource) => {
                    self.village_scenic(item, resource, origin, &canopy_subjects);
                    if item.kind == ScenicKind::Lamp && item.tile == CAT_LAMP && world.household_lamp_lit() {
                        let (x, y) = local(point(item.tile), origin);
                        self.rect(x - 1, y - 7, 3, 3, [229, 159, 80]);
                        self.pixel(x, y - 6, [255, 223, 145]);
                    }
                }
                Visible::Lamp(p) => self.market_lamp(p, origin, world.minute_of_day()),
                Visible::Drain(p) => {
                    let (x, y) = local(p, origin);
                    self.draw_village_sprite(
                        Bank::Village,
                        "village_drain",
                        (x * 2, y * 2),
                        false,
                        Scale::IDENTITY,
                        Palette::Unchanged,
                    );
                }
                Visible::BridgeRail(first, width) => {
                    if let Some(last) = WILLOW_VILLAGE
                        .bridges
                        .iter()
                        .find(|b| b.axis.first() == Some(&first))
                        .and_then(|b| b.axis.last())
                    {
                        self.village_bridge_rail(first, *last, width, origin);
                    }
                }
                Visible::Legacy(Drawable::Actor(i)) => {
                    let actor = &world.actors()[i];
                    self.visible_motion |= actor.walking_segment().is_some()
                        && visible_character_motion(self, actor.position, origin);
                    self.actor_at(actor, cx, cy, world.world_ms(), Some(world));
                    self.village_hoe_mud(world, actor, origin);
                    self.village_family_detail(world, actor, origin);
                }
                Visible::Legacy(Drawable::Building(b)) => {
                    self.building(b, cx, cy, world.minute_of_day())
                }
                Visible::Legacy(Drawable::Tree(i, t)) => {
                    let (x, y) = local(t.position, origin);
                    if let Some(phase) = world
                        .orchard_phase(i)
                        .filter(|_| t.stage == TreeStage::Standing)
                    {
                        if world.sapling_soil_ring().is_some_and(|root| point(root) == t.position) {
                            let (x, y) = (x * 2, y * 2);
                            for (dx, dy, width) in [(-6, -3, 12), (-8, -2, 16), (-8, -1, 16), (-7, 0, 14), (-6, 1, 12), (-4, 2, 8)] {
                                self.resource_line((x + dx, y + dy), (width, 0), if dy < 0 { [141, 102, 74] } else { [167, 124, 87] });
                            }
                        }
                        if let Some(mulch) = world.village_tree_mulch(i) {
                            let (mx, my) = local(point(mulch), origin);
                            for (dx, dy, end) in [(-7, -1, 4), (-5, 0, 5), (2, 0, 5), (-4, 2, 3), (1, 2, 4)] {
                                self.resource_line((mx * 2 + dx, my * 2 + dy), (end, 0), [153, 111, 62]);
                            }
                        }
                        self.village_orchard(
                            phase,
                            (x, y),
                            t.position.y as i32,
                            i % 2 == 1,
                            &canopy_subjects,
                        );
                    } else if t.stage == TreeStage::Standing {
                        let (tx, ty) = (t.position.x as i32 / 8, t.position.y as i32 / 8);
                        let sprite = forest_crown(tx, ty, terrain_hash(tx, ty));
                        self.contact_shadow((x + 2, y + 1), (13, 4), 44);
                        self.draw_tree_canopy(
                            Bank::Village,
                            sprite,
                            (x * 2, y * 2),
                            t.position.y as i32,
                            i % 2 == 1,
                            Scale::IDENTITY,
                            &canopy_subjects,
                        );
                    } else {
                        self.tree(i, x, y, t.stage);
                    }
                }
                Visible::Legacy(Drawable::Housing(site)) => {
                    self.housing(site, cx, cy, world.minute_of_day())
                }
                Visible::Legacy(Drawable::FormerHome(kind, p)) => {
                    let (x, y) = local(p, origin);
                    let key = match kind {
                        FormerHomeUse::GrainStorage => "village_grain_room",
                        FormerHomeUse::ToolRoom => "village_tool_room",
                    };
                    self.draw_village_sprite(
                        Bank::Village,
                        key,
                        ((x + 5) * 2, (y - 3) * 2),
                        false,
                        Scale::new(5, 4),
                        Palette::Unchanged,
                    );
                }
                Visible::Legacy(Drawable::Plugin(command)) => {
                    if let Some(store) = store {
                        self.plugin(command, cx, cy, store);
                    }
                }
                Visible::Legacy(Drawable::InstalledFixture(status, p)) => {
                    let (x, y) = local(p, origin);
                    self.core_with(
                        status.fixture.asset_name(),
                        (x + 9, y - 2),
                        false,
                        Scale::new(3, 4),
                        Palette::Unchanged,
                    );
                }
                Visible::Legacy(Drawable::FixtureWork(_, p)) => {
                    let (x, y) = local(p, origin);
                    self.premium_frame(
                        "structure_upgrade_scaffold",
                        (x - 8, y),
                        false,
                        Palette::Unchanged,
                    );
                }
                Visible::Legacy(_) => {}
            }
        }
        self.village_light(world.minute_of_day());
        self.precipitation(world.weather(), world.world_ms(), cx, cy);
        self.village_labels(&labels, origin);
        self.clip_village_edges(world, origin);
        self.indoor_reveals = [None; 2];
        self.awake_screen_feet = [None; 2];
    }

    fn village_crop_footprint(
        &mut self,
        plot: &Plot,
        stage: Option<PlotStage>,
        rect: TileRect,
        (cx, cy): (i32, i32),
    ) {
        // One saved plot stage controls all cells of the physical bed.
        let mut cell = *plot;
        cell.stage = stage.unwrap_or(plot.stage);
        for y in rect.y..rect.y + rect.h {
            for x in rect.x..rect.x + rect.w {
                cell.position = point(TilePos::new(x, y));
                self.premium_plot(&cell, cx, cy);
            }
        }
    }

    fn village_prop(&mut self, key: &str, p: Point, world: &LivingWorld, origin: (i32, i32)) {
        let (x, y) = local(p, origin);
        if !key.starts_with("village_reward_") {
            self.contact_shadow((x + 1, y), (7, 3), 25);
        }
        if key.starts_with("village_") {
            self.draw_village_sprite(
                Bank::Village,
                key,
                (x * 2, y * 2),
                false,
                Scale::IDENTITY,
                Palette::Unchanged,
            );
        } else {
            let scale = if matches!(key, "prop_stone" | "prop_rope") {
                Scale::new(3, 5)
            } else {
                Scale::IDENTITY
            };
            self.core_with(key, (x, y), false, scale, Palette::Unchanged);
        }
        if key == "shared_table" {
            let is_family_table = matches!(
                WILLOW_VILLAGE.slot("home_table", "home_table.tabletop").map(|slot| slot.position),
                Some(SlotPos::Outdoor(tile)) if p == point(tile)
            );
            if is_family_table && world.town_corner_renewed() {
                self.draw_village_sprite(
                    Bank::Village,
                    "village_reward_plaque",
                    ((x + 4) * 2, (y - 3) * 2),
                    false,
                    Scale::new(1, 2),
                    Palette::Unchanged,
                );
            }
            let guest_food = is_family_table && world.courtyard_guest_table_set();
            for (i, actor) in world.actors()[..2].iter().enumerate() {
                if guest_food
                    || (actor.pose == Pose::Eat
                        && actor.carried == Some(Item::Food)
                        && (actor.position.x - p.x).abs() < 15.0
                        && (actor.position.y - p.y).abs() < 8.0)
                {
                    let shift = if i == 0 { -4 } else { 4 };
                    self.premium_at_scale(
                        "prop_meal",
                        ((x + shift) * 2, (y - 7) * 2),
                        false,
                        Scale::new(2, 3),
                        Palette::Unchanged,
                    );
                }
            }
        }
    }

    fn village_bridge_rail(&mut self, a: TilePos, b: TilePos, row: u8, origin: (i32, i32)) {
        let lo = i32::from(a.x.min(b.x)) * 8 - origin.0;
        let hi = (i32::from(a.x.max(b.x)) + 1) * 8 - 1 - origin.0;
        let y = (i32::from(a.y) + i32::from(row)) * 8 - origin.1;
        self.rect(lo + 2, y - 4, hi - lo - 3, 1, [181, 139, 88]);
        self.rect(lo + 2, y - 3, hi - lo - 3, 1, [103, 77, 53]);
        self.rect(lo + 2, y - 1, hi - lo - 3, 1, [127, 92, 56]);
        for x in (lo + 2..hi).step_by(16) {
            self.rect(x, y - 6, 2, 7, [121, 89, 57]);
            self.rect(x, y - 7, 2, 1, [177, 137, 88]);
            self.rect(x + 2, y - 5, 1, 6, [82, 64, 47]);
        }
    }

    fn village_building(
        &mut self,
        b: &BuildingDef,
        world: &LivingWorld,
        origin: (i32, i32),
        stalls: &[StallVisual],
        vacant: bool,
    ) {
        let (x, y) = local(structure_anchor(b), origin);
        let (bank, key) = structure(b.id, b.kind);
        let palette = self.roof_palette(
            world.minute_of_day() > 1200 && b.beds != 0 && !vacant,
            y * 2,
        );
        let activity = stalls
            .iter()
            .find(|s| s.site == b.id || s.site == b.door_poi);
        if matches!(
            b.id,
            "stall_produce" | "stall_seed" | "stall_meal" | "stall_cloth"
        ) && !activity.is_some_and(|s| s.open)
        {
            // The shut canopy leaves its fixed counter on the solid part of
            // the footprint. Goods remain off-scene until actually unloaded.
            let ty = if b.door.y < b.footprint.y {
                b.footprint.y + 1
            } else {
                b.footprint.y + 2
            };
            let p = point(TilePos::new(b.door.x, ty));
            let (x, y) = local(p, origin);
            self.contact_shadow((x + 2, y), (10, 3), 36);
            self.core_with(
                "shared_table",
                (x, y),
                false,
                Scale::new(3, 4),
                Palette::Unchanged,
            );
            return;
        }
        self.contact_shadow((x + 3, y - 1), (i32::from(b.footprint.w) * 4, 5), 42);
        // The transparent opening exposes the merchant while canopy and counter
        // are in front. The ordinal places the customer on the near side.
        self.draw_village_sprite(bank, key, (x * 2, y * 2), false, Scale::IDENTITY, palette);
        if vacant {
            // The existing doorway and roof stay. A simple closed board occupies
            // the front wall rather than blocking the walkable threshold.
            self.draw_village_sprite(
                Bank::Village,
                "village_remodel",
                (x * 2, (y - 2) * 2),
                false,
                Scale::new(2, 3),
                Palette::Unchanged,
            );
        }
        if let Some(stall) = activity {
            let prop = item_asset(stall.item);
            if !stall.open || stall.quantity == 0 {
                return;
            }
            let north_facing = b.door.y < b.footprint.y;
            // A north-facing booth has its visual display within the occupied
            // storefront. Its exchange tile on the street stays clear.
            let shelf = if north_facing {
                point(TilePos::new(b.door.x, b.footprint.y + 3))
            } else {
                point(stall.tile)
            };
            let (sx, sy) = local(shelf, origin);
            let scale = if matches!(stall.item, Item::Rope | Item::Stone) {
                Scale::new(2, 5)
            } else {
                Scale::new(3, 4)
            };
            let spacing = if north_facing { 4 } else { 6 };
            for offset in [0, -spacing, spacing]
                .into_iter()
                .take(usize::from(stall.quantity).min(3))
            {
                self.core_at_texel(
                    prop,
                    ((sx + offset) * 2, (sy - 7) * 2),
                    false,
                    scale,
                    Palette::Unchanged,
                );
            }
        }
    }

    fn village_resident(&mut self, v: &VisitorVisual, origin: (i32, i32)) {
        let (x, y) = local(v.position, origin);
        let profession = v.profession.asset_name();
        let style = profession.bytes().fold(0_u8, u8::wrapping_add) as usize;
        let female = matches!(
            profession,
            "seed_merchant"
                | "cook"
                | "weaver"
                | "florist"
                | "tailor"
                | "baker"
                | "reed_weaver"
                | "bird_observer"
                | "potter"
                | "traveling_painter"
        );
        let prefix = if female { "female" } else { "male" };
        let action = match v.pose {
            VisitorPose::Walking => "walk",
            VisitorPose::Waiting => "idle",
            VisitorPose::Working => "idle",
            VisitorPose::Trading => "greet",
            VisitorPose::Carrying if v.held.is_some() => "carry",
            VisitorPose::Carrying => "walk",
            VisitorPose::Resting => "rest",
        };
        let frame = format!("{prefix}_{action}_{}", v.animation_frame % 3);
        let color = resident_palette(female, style);
        self.contact_shadow((x + 1, y), (5, 2), 36);
        let north = match profession {
            "cook" => Some("stall_meal"),
            "weaver" => Some("stall_cloth"),
            _ => None,
        }
        .filter(|_| matches!(v.pose, VisitorPose::Working | VisitorPose::Trading))
        .and_then(|id| WILLOW_VILLAGE.building(id))
        .filter(|b| b.door.y < b.footprint.y)
        .and_then(|b| {
            let merchant = format!("{}.merchant", b.door_poi);
            WILLOW_VILLAGE.slot(b.door_poi, &merchant)
        })
        .is_some_and(|slot| matches!(slot.position, SlotPos::Outdoor(p) if point(p) == v.position));
        let baked_goods = matches!(
            profession,
            "cook"
                | "fisher"
                | "weaver"
                | "fruit_grower"
                | "florist"
                | "traveling_painter"
                | "tailor"
                | "scale_merchant"
                | "baker"
                | "reed_weaver"
                | "irrigator"
                | "bird_observer"
                | "potter"
        );
        let silhouette = if matches!(v.pose, VisitorPose::Walking | VisitorPose::Carrying) {
            format!("{profession}_village_body_walk_{}", v.animation_frame % 2)
        } else {
            format!("{profession}_village_body")
        };
        let have_body = if north {
            self.resident_body_without_baked_goods(
                &format!("{profession}_village_counter_north"),
                prefix,
                "idle",
                0,
                (x, y),
                false,
                color,
            )
        } else if baked_goods {
            self.resident_body_without_baked_goods(
                &format!("{profession}_village_body"),
                prefix,
                action,
                v.animation_frame,
                (x, y),
                v.facing_left,
                color,
            )
        } else {
            self.draw_village_sprite(
                Bank::Village,
                &silhouette,
                (x * 2, y * 2),
                v.facing_left && !north,
                Scale::IDENTITY,
                Palette::Unchanged,
            )
        };
        if !have_body {
            self.premium_frame(&frame, (x, y), v.facing_left, color);
        }
        if let Some(held) = v.held {
            let hand = self
                .premium_hand(&frame, (x, y), v.facing_left)
                .unwrap_or((x * 2, y * 2 - 15));
            let at = if north { ((x - 5) * 2, (y - 7) * 2) } else { hand };
            if !matches!(held, VisitorHeld::Parcel("letter" | "booking_slip"))
                && let Some((prop, scale)) = held_asset(held)
            {
                self.core_at_texel(prop, at, false, scale, Palette::Unchanged);
            }
            match held {
                VisitorHeld::Parcel("letter" | "booking_slip") => {
                    for row in -4..=1 {
                        let ink = if row == -4 || row == 1 { [143, 110, 74] } else { [244, 222, 172] };
                        self.resource_line((at.0 - 4, at.1 + row), (8, 0), ink);
                    }
                    self.resource_line((at.0 - 3, at.1 - 3), (3, 2), [198, 158, 106]);
                    self.resource_line((at.0, at.1 - 1), (3, -2), [198, 158, 106]);
                    self.resource_texel((at.0, at.1 - 1), [166, 81, 61]);
                }
                VisitorHeld::Parcel("wrapped_letter" | "wrapped_seed_packet") => {
                    self.village_oilpaper(at, held == VisitorHeld::Parcel("wrapped_seed_packet"));
                }
                VisitorHeld::Parcel("orchard_leaf") => self.village_apple_leaf(at),
                VisitorHeld::Parcel("kitchen_scraps") => {
                    self.resource_line((at.0 - 2, at.1 - 3), (3, -2), [160, 116, 65]);
                    self.resource_texel((at.0 + 1, at.1 - 4), [194, 150, 73]);
                }
                VisitorHeld::Parcel("guest_food_basket") => {
                    self.resource_line((at.0 - 3, at.1 - 4), (6, 0), [231, 205, 154]);
                    self.resource_line((at.0, at.1 - 5), (1, 3), [125, 116, 67]);
                }
                VisitorHeld::Parcel("warm_cat_supper") => {
                    self.resource_line((at.0 - 2, at.1 - 2), (4, 0), [181, 124, 70]);
                    self.resource_texel((at.0 - 1, at.1 - 3), [230, 167, 89]);
                    self.resource_texel((at.0 + 1, at.1 - 3), [229, 183, 103]);
                }
                VisitorHeld::Parcel("saucer_meal" | "empty_saucer") => {
                    self.village_food_saucer(hand, held == VisitorHeld::Parcel("saucer_meal"));
                }
                VisitorHeld::Parcel("covered_snack_tray" | "empty_snack_tray") => {
                    self.resource_line((at.0 - 6, at.1), (12, 0), [148, 97, 60]);
                    self.resource_line((at.0 - 5, at.1 + 1), (10, 0), [101, 67, 45]);
                    if held == VisitorHeld::Parcel("covered_snack_tray") {
                        self.resource_line((at.0 - 4, at.1 - 1), (8, 0), [199, 181, 140]);
                        self.resource_line((at.0 - 3, at.1 - 2), (6, 0), [225, 201, 158]);
                        self.resource_line((at.0 - 1, at.1 - 3), (2, 0), [237, 216, 173]);
                    }
                }
                _ => {}
            }
        }
    }

    fn resident_body_without_baked_goods(
        &mut self,
        head_key: &str,
        prefix: &str,
        action: &str,
        phase: usize,
        feet: (i32, i32),
        flip: bool,
        palette: Palette,
    ) -> bool {
        let (Some(village), Some(core)) = (&self.village_art, &self.art) else {
            return false;
        };
        let Some(head) = village.metadata(head_key) else {
            return false;
        };
        let Some(body) = core.metadata(&format!("{prefix}_{action}_{}", phase % 3)) else {
            return false;
        };
        let anchor = (feet.0 * 2, feet.1 * 2);
        let join = anchor.1 - 22;
        let size = (self.width, self.height);
        for (atlas, sprite, rows, paint) in [
            (village, head, 0..join, Palette::Unchanged),
            (core, body, join..self.height, palette),
        ] {
            let source = Sprite::new(sprite.rect, sprite.anchor, anchor, flip, Scale::IDENTITY);
            if source.visible(size) {
                blit_rows(
                    &mut self.pixels,
                    size,
                    atlas.page(sprite),
                    256,
                    source,
                    paint,
                    rows,
                );
            }
        }
        true
    }

    fn village_labels(&mut self, labels: &[&VisitorVisual], origin: (i32, i32)) {
        let mut labels = labels.to_vec();
        labels.sort_by_key(|v| {
            (
                v.position.y as i32,
                v.position.x as i32,
                v.profession.asset_name(),
            )
        });
        let mut used: Vec<(i32, i32)> = Vec::with_capacity(labels.len().min(12));
        for v in labels {
            let (x, y) = local(v.position, origin);
            if x < 0 || y < 13 || x >= (self.width + 1) / 2 {
                continue;
            }
            let key = format!("{}_village_label", v.profession.asset_name());
            let y = (y * 2 - 42).max(13);
            let x = (x * 2).clamp(
                13.min(self.width / 2),
                (self.width - 13).max(self.width / 2),
            );
            if used
                .iter()
                .any(|&(ux, uy)| (ux - x).abs() < 29 && (uy - y).abs() < 14)
            {
                continue;
            }
            if self.draw_village_sprite(
                Bank::Village,
                &key,
                (x, y),
                false,
                Scale::IDENTITY,
                Palette::Unchanged,
            ) {
                used.push((x, y));
            }
        }
    }

    fn village_orchard(
        &mut self,
        phase: OrchardPhase,
        (x, y): (i32, i32),
        depth: i32,
        flip: bool,
        subjects: &[Option<CanopySubject>; 3],
    ) {
        let position = (x * 2, y * 2);
        self.contact_shadow((x + 4, y + 1), (12, 4), 42);
        let key = match phase {
            OrchardPhase::Sapling => "village_apple_sapling",
            OrchardPhase::Dormant => "village_apple_dormant",
            OrchardPhase::Bud => "village_apple_bud",
            OrchardPhase::Flower => "village_apple_flower",
            OrchardPhase::GreenFruit => "village_apple_green_fruit",
            OrchardPhase::Ripe => "village_apple",
            OrchardPhase::AfterHarvest => "village_apple_after",
        };
        self.draw_tree_canopy(
            Bank::Village,
            key,
            position,
            depth,
            flip,
            Scale::IDENTITY,
            subjects,
        );
    }

    fn village_scenic(
        &mut self,
        item: &ScenicObject,
        resource: Option<ScenicResourceView>,
        origin: (i32, i32),
        subjects: &[Option<CanopySubject>; 3],
    ) {
        let p = point(item.tile);
        let foot = local(p, origin);
        let art = scenic_art(item);
        if let Some(resource) = resource {
            match resource.stage {
                ScenicStage::Cleared => return,
                ScenicStage::Stump => {
                    self.felled_scenic_tree(foot, art.flip, resource.pending, true);
                    if resource.work_done > 0 {
                        self.scenic_work_mark(foot, item.kind, resource.work_done, art.flip);
                    }
                    return;
                }
                ScenicStage::Cut => {
                    match item.kind {
                        ScenicKind::Tree => {
                            self.felled_scenic_tree(foot, art.flip, resource.pending, false)
                        }
                        ScenicKind::Rock => {
                            self.cracked_scenic_rock(foot, art.flip, resource.pending)
                        }
                        ScenicKind::Shrub | ScenicKind::Log => {
                            self.cut_scenic_vegetation(foot, art.flip, 0);
                            if resource.pending > 0 {
                                self.core_at_texel(
                                    "logs",
                                    (foot.0 * 2, foot.1 * 2),
                                    art.flip,
                                    Scale::new(1, 2),
                                    Palette::Unchanged,
                                );
                            }
                        }
                        _ => self.cut_scenic_vegetation(foot, art.flip, resource.pending),
                    }
                    return;
                }
                ScenicStage::Standing => {}
            }
        }
        self.contact_shadow(
            art.shadow_center(foot),
            art.shadow_radii,
            if item.solid { 38 } else { 20 },
        );
        let position = art.position(foot);
        if item.kind == ScenicKind::Tree {
            self.draw_tree_canopy(
                art.bank, art.key, position, p.y as i32, art.flip, art.scale, subjects,
            );
        } else if item.kind == ScenicKind::Bench
            && let Some((tile, _)) = self.village_food_saucer
            && tile.offset(0, 1) == Some(item.tile)
        {
            let dish = point(tile);
            let (x, y) = local(dish, origin);
            let subject = CanopySubject { area: ArtRect { x: x * 2 - 6, y: y * 2 - 4, w: 13, h: 9 }, depth: dish.y as i32 };
            self.draw_tree_canopy(
                art.bank, art.key, position, p.y as i32, art.flip, art.scale, &[Some(subject), None, None],
            );
        } else {
            self.draw_village_sprite(
                art.bank,
                art.key,
                position,
                art.flip,
                art.scale,
                Palette::Unchanged,
            );
        }
        if let Some(resource) = resource.filter(|state| state.work_done > 0) {
            self.scenic_work_mark(foot, item.kind, resource.work_done, art.flip);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_tree_canopy(
        &mut self,
        bank: Bank,
        key: &str,
        position: (i32, i32),
        depth: i32,
        flip: bool,
        scale: Scale,
        subjects: &[Option<CanopySubject>; 3],
    ) {
        let Some(atlas) = bank.atlas(
            self.art.as_ref(),
            self.village_art.as_ref(),
            self.wildlife_art.as_ref(),
        ) else {
            return;
        };
        let Some(frame) = atlas.metadata(key) else {
            return;
        };
        let sprite = Sprite::new(frame.rect, frame.anchor, position, flip, scale);
        if !sprite.visible((self.width, self.height)) {
            return;
        }
        let page = atlas.page(frame);
        // Leave the bottom six world pixels and the contact shadow fully opaque.
        let roots = position.1 - 12;
        let bounds = sprite_bounds(sprite);
        let crown = ArtRect {
            h: (roots - bounds.y).min(bounds.h),
            ..bounds
        };
        let visible = ArtRect {
            x: 0,
            y: 0,
            w: self.width,
            h: self.height,
        };
        let cutaways = subjects.map(|subject| {
            let subject = subject.filter(|p| depth >= p.depth)?;
            let overlap = intersection(intersection(crown, subject.area)?, visible)?;
            crown_has_ink(overlap, sprite, page).then_some(subject.area)
        });
        let size = (self.width, self.height);
        if cutaways.iter().all(Option::is_none) {
            blit_into(
                &mut self.pixels,
                size,
                page,
                256,
                sprite,
                Palette::Unchanged,
            );
        } else if let Some(visible) = intersection(bounds, visible) {
            blit_canopy(
                &mut self.pixels,
                self.width,
                page,
                sprite,
                visible,
                roots,
                cutaways,
            );
        }
    }

    fn market_lamp(&mut self, p: Point, origin: (i32, i32), minute: u16) {
        let (x, y) = local(p, origin);
        self.contact_shadow((x + 3, y + 1), (5, 3), 38);
        self.draw_village_sprite(
            Bank::Village,
            "village_lantern",
            (x * 2, y * 2),
            false,
            Scale::IDENTITY,
            Palette::Unchanged,
        );
        if !(360..1140).contains(&minute) {
            self.rect(x - 1, y - 7, 3, 3, [220, 147, 72]);
            self.pixel(x, y - 6, [255, 218, 137]);
        }
    }

    fn village_light(&mut self, minute: u16) {
        let light = if !(300..1200).contains(&minute) {
            [70_u16, 83, 119]
        } else if !(360..1140).contains(&minute) {
            [99, 91, 92]
        } else {
            return;
        };
        for bgra in self.pixels.chunks_exact_mut(4) {
            for (channel, tone) in bgra[..3].iter_mut().zip(light.into_iter().rev()) {
                *channel = (u16::from(*channel) * tone / 100).min(255) as u8;
            }
        }
    }

    fn clip_village_edges(&mut self, world: &LivingWorld, origin: (i32, i32)) {
        let (w, h) = world.map_extent();
        let width = i32::from(w) * 16;
        let height = i32::from(h) * 16;
        if origin.0 >= 0
            && origin.1 >= 0
            && origin.0 * 2 + self.width <= width
            && origin.1 * 2 + self.height <= height
        {
            return;
        }
        for (y, row) in self
            .pixels
            .chunks_exact_mut(self.width as usize * 4)
            .enumerate()
        {
            for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
                if !(0..width).contains(&(origin.0 * 2 + x as i32))
                    || !(0..height).contains(&(origin.1 * 2 + y as i32))
                {
                    pixel.fill(0);
                }
            }
        }
    }

    fn draw_village_sprite(
        &mut self,
        bank: Bank,
        key: &str,
        position: (i32, i32),
        flip: bool,
        scale: Scale,
        palette: Palette,
    ) -> bool {
        let Some(atlas) = bank.atlas(
            self.art.as_ref(),
            self.village_art.as_ref(),
            self.wildlife_art.as_ref(),
        ) else {
            return false;
        };
        let Some(frame) = atlas.metadata(key) else {
            return false;
        };
        let sprite = Sprite::new(frame.rect, frame.anchor, position, flip, scale);
        if sprite.visible((self.width, self.height)) {
            blit_into(
                &mut self.pixels,
                (self.width, self.height),
                atlas.page(frame),
                256,
                sprite,
                palette,
            );
        }
        true
    }
}

#[cfg(test)]
#[path = "village_render_fixtures.rs"]
mod fixtures;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_water_marks_advance_only_inside_the_active_tile() {
        let mut view = LivingView::new();
        view.width = 64;
        view.height = 64;
        let blank = [112, 100, 81, 255].repeat(64 * 64);
        let tile = TilePos::new(2, 2);
        let origin = (4, 4);
        for kind in [FieldWaterKind::Soak, FieldWaterKind::Drain] {
            let mut previous = blank.clone();
            for phase in 0..=2 {
                view.pixels.clone_from(&blank);
                view.village_field_water(origin, FieldWaterWork { plot: 0, tile, kind, phase }, 36_000);
                assert_ne!(view.pixels, previous);
                for (i, (a, b)) in view.pixels.chunks_exact(4).zip(blank.chunks_exact(4)).enumerate() {
                    if a != b {
                        assert!((24..40).contains(&(i % 64)) && (24..40).contains(&(i / 64)));
                    }
                }
                previous.clone_from(&view.pixels);
            }
        }
    }

    #[test]
    fn kitchen_menu_pixels_distinguish_stock_from_booking_availability() {
        let mut view = LivingView::new();
        view.width = 64;
        view.height = 64;
        let render = |view: &mut LivingView, servings, slots| {
            view.pixels = [79, 116, 64, 255].repeat(64 * 64);
            view.village_menu_sign((16, 16), servings, slots);
            view.pixels.clone()
        };
        let open = render(&mut view, 3, 1);
        let booked = render(&mut view, 3, 0);
        let empty = render(&mut view, 0, 0);
        assert_ne!(open, booked);
        assert_ne!(booked, empty);
        assert_eq!(&open[0..4], &empty[0..4]);
    }

    #[test]
    fn village_artwork_shows_actual_subjects_and_stage_on_a_bounded_panel() {
        let mut view = LivingView::new();
        view.width = 64;
        view.height = 64;
        let render = |view: &mut LivingView, subject, slot, progress| {
            view.pixels = [79, 116, 64, 255].repeat(64 * 64);
            let board = VillageArtVisual { tile: TilePos::new(12, 21), subject, slot, progress };
            view.village_art_board((16, 16), board);
            view.pixels.clone()
        };
        let fresh = render(&mut view, VillageArtSubject::Blossom(0), None, 0);
        let bloom = render(&mut view, VillageArtSubject::Blossom(0), None, 100);
        let pond = render(&mut view, VillageArtSubject::Reflection(2), None, 100);
        let hung = render(&mut view, VillageArtSubject::Reflection(2), Some(0), 100);
        assert_ne!(fresh, bloom);
        assert_ne!(bloom, pond);
        assert_ne!(pond, hung);
        for (i, (panel, blank)) in hung.chunks_exact(4).zip([79, 116, 64, 255].repeat(64 * 64).chunks_exact(4)).enumerate() {
            if panel != blank {
                assert!((24..=40).contains(&(i % 64)) && (15..=26).contains(&(i / 64)));
            }
        }
        let dawn = render(&mut view, VillageArtSubject::DuskSwatches([1, 0, 0]), None, 100);
        let cloudy = render(&mut view, VillageArtSubject::DuskSwatches([1, 1, 2]), None, 100);
        let evening = render(&mut view, VillageArtSubject::DuskStreet([1, 1, 2]), Some(0), 100);
        assert_ne!(dawn, cloudy);
        assert_ne!(cloudy, evening);
        for [r, g, b] in dusk_board_colors([1, 1, 2]) {
            let pigment = [b, g, r, 255];
            assert!(cloudy.chunks_exact(4).any(|p| p == pigment));
            assert!(evening.chunks_exact(4).any(|p| p == pigment));
        }
    }

    fn save_landscape_frames(path: &std::path::Path, view: &LivingView, frames: &[Vec<u8>]) {
        use std::io::{BufWriter, Write};
        let mut image = BufWriter::new(std::fs::File::create(path).unwrap());
        let row_bytes = view.width as usize * 4;
        let strip_width = view.width as usize * frames.len();
        writeln!(image, "P6\n{strip_width} {}\n255", view.height).unwrap();
        let mut rgb = Vec::with_capacity(strip_width * 3);
        for y in 0..view.height as usize {
            rgb.clear();
            for frame in frames {
                for pixel in frame[y * row_bytes..(y + 1) * row_bytes].chunks_exact(4) {
                    rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
                }
            }
            image.write_all(&rgb).unwrap();
        }
        image.flush().unwrap();
    }

    #[test]
    fn processed_resource_art_stays_inside_the_index_and_only_shows_pending_goods() {
        let fallen: fn(&mut LivingView, (i32, i32), bool, u8) =
            |view, foot, flip, pending| view.felled_scenic_tree(foot, flip, pending, false);
        let mut view = LivingView::new();
        view.enable_builtin_premium().unwrap();
        view.density = 2;
        view.width = 128;
        view.height = 96;
        let blank = [91, 149, 129, 255].repeat((view.width * view.height) as usize);
        let foot = (32, 32);
        for (kind, draw) in [
            (ScenicKind::Tree, fallen),
            (ScenicKind::Rock, LivingView::cracked_scenic_rock),
            (ScenicKind::Grass, LivingView::cut_scenic_vegetation),
        ] {
            let bounds = worked_resource_bounds(foot, kind);
            for flip in [false, true] {
                view.pixels.clone_from(&blank);
                draw(&mut view, foot, flip, 1);
                let pending = view.pixels.clone();
                view.pixels.clone_from(&blank);
                draw(&mut view, foot, flip, 0);
                assert_ne!(
                    pending, view.pixels,
                    "a pending unit must visibly lie at its source"
                );
                for (i, (original, after)) in blank
                    .chunks_exact(4)
                    .zip(pending.chunks_exact(4))
                    .enumerate()
                {
                    if original != after {
                        assert!((bounds.x..bounds.x + bounds.w).contains(&(i as i32 % view.width)));
                        assert!((bounds.y..bounds.y + bounds.h).contains(&(i as i32 / view.width)));
                    }
                }
            }
        }
    }

    #[test]
    fn resource_stage_pixels_preserve_ground_cache_and_leave_no_standing_tree_after_collection() {
        let world = LivingWorld::new_village_wall_ms(0);
        let (id, item) = WILLOW_VILLAGE
            .scenic
            .iter()
            .enumerate()
            .find(|(id, item)| {
                item.kind == ScenicKind::Tree && world.scenic_resource(*id).is_some()
            })
            .unwrap();
        let p = point(item.tile);
        let size = (481, 301);
        let mut camera = Camera {
            x: p.x - 35.0,
            y: p.y - 30.0,
            follow: None,
        };
        camera.clamp_in_world(&world, crate::viewport::logical_size(size));
        let mut view = LivingView::new();
        view.enable_builtin_premium().unwrap();
        view.render_village(&world, &camera, size, &[], None, &[], &[], &[]);
        let original_base = view.terrain_pixels.clone();
        let mut frames = vec![view.pixels.clone()];
        let initial = serde_json::to_value(&world).unwrap();
        let stage = world.scenic_resource(id).unwrap();
        for ticks in [1, stage.work_total, stage.work_total + 1] {
            let mut fixture = initial.clone();
            fixture["village"]["landscape"]["unfinished"] =
                serde_json::json!([{ "id": id, "ticks": ticks }]);
            let restored: LivingWorld = serde_json::from_value(fixture).unwrap();
            assert_eq!(
                world.village_ground_revision(),
                restored.village_ground_revision()
            );
            view.render_village(&restored, &camera, size, &[], None, &[], &[], &[]);
            assert_eq!(view.terrain_pixels, original_base);
            assert_ne!(frames.last().unwrap(), &view.pixels);
            frames.push(view.pixels.clone());
        }
        let mut fixture = initial;
        let mut cleared = [[0_u64; 32]; 2];
        cleared[id / 2048][(id / 64) % 32] = 1 << (id % 64);
        fixture["village"]["landscape"]["cleared"] = serde_json::json!(cleared);
        let cleared: LivingWorld = serde_json::from_value(fixture).unwrap();
        assert_eq!(
            cleared.scenic_resource(id).unwrap().stage,
            ScenicStage::Cleared
        );
        let update = view
            .render_village(&cleared, &camera, size, &[], None, &[], &[], &[])
            .unwrap();
        assert_eq!(view.terrain_pixels, original_base);
        assert_ne!(&view.pixels, &frames[3]);
        assert_ne!(&view.pixels, &frames[0]);
        assert!(update[2] - update[0] < view.width / 2 && update[3] - update[1] < view.height / 2);
        if let Some(path) = std::env::var_os("CAT_HOME_LANDSCAPE_VISUAL_SEQUENCE") {
            frames.push(view.pixels.clone());
            save_landscape_frames(std::path::Path::new(&path), &view, &frames);
        }
    }

    #[test]
    fn real_scenic_work_reveals_its_source_and_collects_material_without_a_tool_in_hand() {
        let fresh = LivingWorld::new_village_wall_ms(0);
        for (kind, tool, clip, product, label) in [
            (ScenicKind::Tree, Tool::Axe, "axe", Some(Item::Wood), "wood"),
            (
                ScenicKind::Rock,
                Tool::Hammer,
                "hammer",
                Some(Item::Stone),
                "stone",
            ),
            (ScenicKind::Grass, Tool::Hoe, "hoe_clear", None, "fibre"),
        ] {
            let (id, object, working) = WILLOW_VILLAGE
                .scenic
                .iter()
                .enumerate()
                .filter(|(id, item)| item.kind == kind && fresh.scenic_resource(*id).is_some())
                .find_map(|(id, item)| {
                    let mut world = fresh.clone();
                    world
                        .test_scenic_visual_fixture(id, ScenicStage::Standing, true)
                        .then_some((id, item, world))
                })
                .unwrap();
            let worker = &working.actors()[1];
            assert_eq!(worker.pose, Pose::Work);
            assert_eq!(worker.tool, Some(tool));
            assert_eq!(village_actor_clip(worker), clip);
            let (tx, ty) = local(point(object.tile), (0, 0));
            let viewport = ArtRect {
                x: 0,
                y: 0,
                w: 3072,
                h: 2048,
            };
            let reveal = canopy_subjects(&working, (0, 0), viewport, &[], None)[1].unwrap();
            assert!((reveal.area.x..reveal.area.x + reveal.area.w).contains(&(tx * 2)));
            assert!((reveal.area.y..reveal.area.y + reveal.area.h).contains(&(ty * 2)));

            let mut gathering = fresh.clone();
            assert!(gathering.test_scenic_visual_fixture(id, ScenicStage::Cut, true));
            let before = gathering.scenic_resource(id).unwrap().pending;
            assert!(before > 0);
            assert_eq!(gathering.actors()[1].activity, Activity::GatherScenic);
            assert_eq!(gathering.actors()[1].tool, None);
            assert_eq!(gathering.actors()[1].carried, None);
            assert_eq!(gathering.landscape_carried_fibre(ActorKind::Ayu), 0);
            let pending = gathering.clone();
            gathering.advance_world_ms(3 * 60_000);
            assert_eq!(gathering.scenic_resource(id).unwrap().pending, 0);
            assert_eq!(gathering.actors()[1].tool, None);
            if let Some(item) = product {
                assert_eq!(gathering.actors()[1].carried, Some(item));
                assert_eq!(gathering.actors()[1].carried_amount(), u16::from(before));
            } else {
                assert_eq!(gathering.landscape_carried_fibre(ActorKind::Ayu), before);
                assert_eq!(gathering.landscape_fibre_stock(), 0);
            }
            if let Some(path) = std::env::var_os("CAT_HOME_LANDSCAPE_VISUAL_SEQUENCE") {
                let path = std::path::Path::new(&path)
                    .with_file_name(format!("visual-{label}-handoff.ppm"));
                let p = point(object.tile);
                let size = (481, 301);
                let mut camera = Camera {
                    x: p.x - 37.0,
                    y: p.y - 35.0,
                    follow: None,
                };
                camera.clamp_in_world(&working, crate::viewport::logical_size(size));
                let mut view = LivingView::new();
                view.enable_builtin_premium().unwrap();
                let frames = [&working, &pending, &gathering].map(|world| {
                    view.render_village(world, &camera, size, &[], None, &[], &[], &[]);
                    view.pixels.clone()
                });
                save_landscape_frames(&path, &view, &frames);
            }
        }
    }

    #[test]
    fn reclaimed_farm_bed_uses_its_authoritative_rectangle_without_stale_scenery() {
        let world = LivingWorld::new_village_wall_ms(0);
        let index = WILLOW_VILLAGE.crops.len();
        let rect = TileRect::new(22, 55, 2, 2);
        assert!(world.restored_plot_site(index).is_none());
        let mut cleared = [[0_u64; 32]; 2];
        for (id, item) in WILLOW_VILLAGE.scenic.iter().enumerate() {
            if rect.contains(item.tile) && world.scenic_resource(id).is_some() {
                cleared[id / 2048][(id / 64) % 32] |= 1 << (id % 64);
            }
        }
        let mut fixture = serde_json::to_value(&world).unwrap();
        fixture["village"]["landscape"]["cleared"] = serde_json::json!(cleared);
        let restored: LivingWorld = serde_json::from_value(fixture).unwrap();
        assert_eq!(restored.restored_plot_site(index).unwrap().0, rect);
        let camera = Camera {
            x: 19.0 * 8.0,
            y: 52.0 * 8.0,
            follow: None,
        };
        let mut view = LivingView::new();
        view.enable_builtin_premium().unwrap();
        view.render_village(&world, &camera, (480, 300), &[], None, &[], &[], &[]);
        let before = view.pixels.clone();
        view.render_village(&restored, &camera, (480, 300), &[], None, &[], &[], &[]);
        assert_ne!(before, view.pixels);
    }

    #[test]
    fn scenic_chunks_match_exact_screen_intersections_and_phase_gates() {
        use crate::map_data::PhaseMask;
        let (core, _, village) = art_assets::builtin().unwrap();
        let index = VillageIndex::new(Some(&core), Some(&village));
        let mut tested_offscreen_anchor = 0;
        for (id, item) in WILLOW_VILLAGE.scenic.iter().enumerate() {
            let art = scenic_art(item);
            let atlas = art.bank.atlas(Some(&core), Some(&village), None).unwrap();
            let frame = atlas
                .metadata(art.key)
                .unwrap_or_else(|| panic!("{} has no art", item.id));
            let foot = local(point(item.tile), (0, 0));
            let visible = sprite_bounds(Sprite::new(
                frame.rect,
                frame.anchor,
                art.position(foot),
                art.flip,
                art.scale,
            ));
            let sliver = ArtRect {
                x: visible.x,
                y: visible.y,
                w: 1,
                h: 1,
            };
            let exact = index.scenic_in_view(sliver, PhaseMask::ALL);
            assert!(
                exact.contains(&id),
                "{} became hidden at its viewport edge",
                item.id
            );
            assert!(exact.len() < WILLOW_VILLAGE.scenic.len() / 8);
            if !intersects(
                sliver,
                ArtRect {
                    x: foot.0 * 2,
                    y: foot.1 * 2,
                    w: 1,
                    h: 1,
                },
            ) {
                tested_offscreen_anchor += 1;
            }
            assert_eq!(
                index
                    .scenic_in_view(sliver, PhaseMask::DAY_ONE)
                    .contains(&id),
                PhaseMask::DAY_ONE.contains(item.phase),
            );
        }
        assert!(tested_offscreen_anchor > 100);

        let large = ArtRect {
            x: 6 * 16,
            y: 77 * 16,
            w: 1280,
            h: 720,
        };
        let selected = index.scenic_in_view(large, PhaseMask::ALL);
        let expected: Vec<_> = index
            .scenic_bounds
            .iter()
            .enumerate()
            .filter_map(|(id, &bounds)| intersects(bounds, large).then_some(id))
            .collect();
        assert!(selected.len() > 512 && selected.len() < WILLOW_VILLAGE.scenic.len());
        assert_eq!(selected, expected);
    }

    #[test]
    fn overlapping_tree_species_keep_a_stationary_awake_actor_readable() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let origin = (80, 80);
        let p = Point { x: 130.0, y: 152.0 };
        world.test_camera_actor(ActorKind::Xiaohe, p, Pose::Work, false);
        world.test_camera_actor(ActorKind::Ayu, p, Pose::Sleep, true);
        let view_rect = ArtRect {
            x: 160,
            y: 160,
            w: 200,
            h: 200,
        };
        let subjects = canopy_subjects(&world, origin, view_rect, &[], None);
        assert!(subjects[0].is_some() && subjects[1].is_none());

        // Place the same three real species in front of the worker. Pixels
        // outside the narrow cutaway, and below the tree roots, must agree.
        fn layered(subjects: &[Option<CanopySubject>; 3]) -> Vec<u8> {
            let mut view = LivingView::new();
            view.enable_builtin_premium().unwrap();
            view.width = 200;
            view.height = 200;
            view.pixels = [25, 77, 145, 255].repeat(200 * 200);
            for (bank, key, pos) in [
                (Bank::Core, "tree_oak", (95, 164)),
                (Bank::Village, "village_forest_laurel", (102, 161)),
                (Bank::Village, "village_apple", (110, 169)),
            ] {
                view.draw_tree_canopy(bank, key, pos, 163, false, Scale::IDENTITY, subjects);
            }
            view.pixels
        }
        let full = layered(&[None, None, None]);
        let cut = layered(&subjects);
        let mut in_front = subjects;
        in_front[0].as_mut().unwrap().depth = 164;
        assert_eq!(full, layered(&in_front));
        let mut read_back = 0;
        for (index, (opaque, clear)) in full.chunks_exact(4).zip(cut.chunks_exact(4)).enumerate() {
            let (x, y) = ((index % 200) as i32, (index / 200) as i32);
            if x < 78 || x > 122 || y > 156 {
                assert_eq!(opaque, clear, "outside the worker's canopy window: {x},{y}");
            }
            if (89..111).contains(&x) && (105..130).contains(&y) && opaque != clear {
                read_back += 1;
            }
        }
        assert!(
            read_back > 140,
            "only {read_back} source texels reveal the worker"
        );
    }

    #[test]
    fn market_ground_cover_survives_outside_active_paving() {
        let patch = WILLOW_VILLAGE
            .ground_cover
            .iter()
            .find(|p| p.region == "market")
            .unwrap();
        let (x, y) = (
            i32::from(patch.center.x) * 16,
            i32::from(patch.center.y) * 16,
        );
        let cover = crate::landscape::meadow_material("market", x, y).unwrap();
        assert!(cover.1 > 190);
        assert_eq!(
            decorated_ground("market", Ground::Grass, x, y, 32, (false, false)),
            None
        );
        assert_eq!(
            decorated_ground("market", Ground::Soil, x, y, 32, (false, false)),
            None
        );
        let paved =
            decorated_ground("market", Ground::Grass, 90 * 16, 46 * 16, 32, (true, false)).unwrap();
        assert_eq!(paved.1, 255);
    }

    #[test]
    fn temporary_farm_cover_leaves_after_actual_plot_preparation() {
        let world = LivingWorld::new_village_wall_ms(0);
        let expansion = WILLOW_VILLAGE.phase("farm_expansion").unwrap();
        let (item, plot) = WILLOW_VILLAGE
            .scenic
            .iter()
            .filter(|p| p.temporary_ground)
            .find_map(|item| {
                WILLOW_VILLAGE
                    .crops
                    .iter()
                    .position(|plot| plot.rect.contains(item.tile))
                    .map(|plot| (item, plot))
            })
            .unwrap();
        assert!(!item.solid && world.can_walk(i32::from(item.tile.x), i32::from(item.tile.y)));
        assert!(temporary_ground_visible(
            &world,
            item.tile,
            world.phase_mask(),
            0,
            &[],
        ));
        // Give the fixture just the expansion flag. Then use the same weeding
        // operation as a resident, with no state written by the renderer.
        let mut value = serde_json::to_value(&world).unwrap();
        value["village"]["region_flags"] =
            serde_json::json!(world.phase_mask().with(expansion).bits());
        let mut expanded: LivingWorld = serde_json::from_value(value).unwrap();
        assert!(expanded.phase_mask().contains(expansion));
        assert!(expanded.village_local_work("weed", Some(plot as u8)));
        assert_eq!(expanded.plot_stage(plot), Some(PlotStage::Cleared));
        assert!(!temporary_ground_visible(
            &expanded,
            item.tile,
            expanded.phase_mask(),
            0,
            &[],
        ));
    }

    #[test]
    fn temporary_cover_leaves_full_stall_on_start_or_opening_and_closed_lane_on_activation() {
        let world = LivingWorld::new_village_wall_ms(0);
        let (index, stall) = WILLOW_VILLAGE
            .buildings
            .iter()
            .enumerate()
            .find(|(_, b)| b.id == "stall_produce")
            .unwrap();
        let tile = (stall.footprint.y..stall.footprint.y + stall.footprint.h)
            .flat_map(|y| {
                (stall.footprint.x..stall.footprint.x + stall.footprint.w)
                    .map(move |x| TilePos::new(x, y))
            })
            .find(|&p| {
                !stall.solid.iter().any(|rect| rect.contains(p))
                    && world.ground(p.x.into(), p.y.into()) == Ground::Grass
            })
            .unwrap();
        assert!(temporary_ground_visible(
            &world,
            tile,
            world.phase_mask(),
            0,
            &[],
        ));
        assert!(!temporary_ground_visible(
            &world,
            tile,
            world.phase_mask(),
            1 << index,
            &[],
        ));
        let future_lane = TilePos::new(161, 110);
        assert!(temporary_ground_visible(
            &world,
            future_lane,
            world.phase_mask(),
            0,
            &[],
        ));
        let mut value = serde_json::to_value(&world).unwrap();
        let late = WILLOW_VILLAGE.phase("late").unwrap();
        value["village"]["region_flags"] =
            serde_json::json!(world.phase_mask().with(stall.phase).with(late).bits());
        let opened: LivingWorld = serde_json::from_value(value).unwrap();
        assert_eq!(opened.ground(tile.x.into(), tile.y.into()), Ground::Grass);
        assert!(opened.village_building_active(stall.id));
        assert!(!temporary_ground_visible(
            &opened,
            tile,
            opened.phase_mask(),
            0,
            &[],
        ));
        assert_eq!(
            opened.ground(future_lane.x.into(), future_lane.y.into()),
            Ground::Path
        );
        assert!(!temporary_ground_visible(
            &opened,
            future_lane,
            opened.phase_mask(),
            0,
            &[],
        ));
    }

    #[test]
    fn temporary_cover_yields_as_soon_as_actual_house_site_is_cleared() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let tile = TilePos::new(19, 6);
        assert!(temporary_ground_visible(
            &world,
            tile,
            world.phase_mask(),
            0,
            &[]
        ));
        world.test_house_visual_stage(HousePlan::Single, HousingPhase::Clear);
        let footprint = housing_footprint(world.housing_sites().next().unwrap());
        assert!(footprint.contains(tile));
        assert_eq!(world.ground(tile.x.into(), tile.y.into()), Ground::Grass);
        assert!(!temporary_ground_visible(
            &world,
            tile,
            world.phase_mask(),
            0,
            &[footprint]
        ));
    }

    #[test]
    fn overscan_and_shifted_terrain_match_a_fresh_village_frame() {
        use crate::viewport::PixelOrigin;
        let world = LivingWorld::new_village_wall_ms(0);
        let physical = (481, 301);
        let map_world = (192 * 8, 128 * 8);
        let mut scrolled = LivingView::new();
        scrolled.enable_builtin_premium().unwrap();
        for (x, y) in [
            (87.1, 118.8),
            (87.8, 117.8),
            (85.3, 120.1),
            (88.9, 122.8),
            (1500.0, 1000.0),
        ] {
            let origin = PixelOrigin::new((x, y), physical, map_world);
            let mut fresh = LivingView::new();
            fresh.enable_builtin_premium().unwrap();
            scrolled.render_village_overscan(&world, origin, physical, &[], None, &[], &[], &[]);
            fresh.render_village_overscan(&world, origin, physical, &[], None, &[], &[], &[]);
            assert_eq!(scrolled.pixels(), fresh.pixels());
        }
    }

    #[test]
    fn only_a_walking_character_near_the_view_requires_fast_display_updates() {
        use crate::visitors::Profession;
        let mut world = LivingWorld::new_village_wall_ms(0);
        let mut view = LivingView::new();
        let camera = Camera {
            x: 60.0,
            y: 120.0,
            follow: None,
        };
        let size = (479, 301);
        for kind in [ActorKind::Xiaohe, ActorKind::Ayu, ActorKind::Orange] {
            world.test_camera_actor(kind, Point::at_tile(80, 80), Pose::Sleep, false);
        }
        let mut person = VisitorVisual {
            profession: Profession::resident(0),
            pose: VisitorPose::Walking,
            position: Point::at_tile(80, 80),
            facing_left: false,
            animation_frame: 0,
            label: "邮差",
            activity: "回家",
            held: None,
        };
        view.render_village(&world, &camera, size, &[], None, &[person], &[], &[]);
        assert!(!view.visible_motion());
        person.position = Point::at_tile(11, 19);
        view.render_village(&world, &camera, size, &[], None, &[person], &[], &[]);
        assert!(view.visible_motion());
        person.pose = VisitorPose::Resting;
        view.render_village(&world, &camera, size, &[], None, &[person], &[], &[]);
        assert!(!view.visible_motion());
    }

    #[test]
    fn cross_chunk_sprite_is_indexed_even_when_its_door_is_offscreen() {
        let (core, _wild, village) = art_assets::builtin().unwrap();
        let index = VillageIndex::new(Some(&core), Some(&village));
        let (id, b) = WILLOW_VILLAGE
            .buildings
            .iter()
            .enumerate()
            .find(|(id, b)| {
                index.bounds[*id].is_some_and(|bounds| {
                    (bounds.x + 2) / CHUNK_ART != (point(b.door).x * 2.0) as i32 / CHUNK_ART
                })
            })
            .unwrap();
        let door = point(b.door);
        let bounds = index.bounds[id].unwrap();
        let view = ArtRect {
            x: bounds.x + 2,
            y: bounds.y + 2,
            w: 14,
            h: 1,
        };
        assert_ne!(view.x / CHUNK_ART, (door.x * 2.0) as i32 / CHUNK_ART);
        assert_ne!(index.in_view(view) & (1_u64 << id), 0);
        let beyond = ArtRect {
            x: bounds.x + bounds.w,
            ..view
        };
        assert_eq!(index.in_view(beyond) & (1_u64 << id), 0);
    }

    #[test]
    fn east_view_is_opaque_and_resize_preserves_source_overlap() {
        let world = LivingWorld::new_village_wall_ms(0);
        let mut view = LivingView::new();
        view.enable_builtin_premium().unwrap();
        // An origin beyond the old 96x64 terrain catches its legacy alpha clip.
        let camera = Camera {
            x: 1212.0,
            y: 390.0,
            follow: None,
        };
        view.render_village(&world, &camera, (479, 301), &[], None, &[], &[], &[]);
        let small = view.pixels().to_vec();
        assert!(small.chunks_exact(4).all(|pixel| pixel[3] == 255));
        view.render_village(&world, &camera, (703, 410), &[], None, &[], &[], &[]);
        let (small_w, small_h) = premium_source_size((479, 301));
        let (large_w, _) = premium_source_size((703, 410));
        for y in 0..small_h as usize {
            let a = y * small_w as usize * 4;
            let b = y * large_w as usize * 4;
            let row = small_w as usize * 4;
            assert_eq!(&small[a..a + row], &view.pixels()[b..b + row]);
        }
    }
}

#[derive(Clone, Copy)]
enum Bank {
    Core,
    Village,
    Wildlife,
}

impl Bank {
    fn atlas<'a>(
        self,
        core: Option<&'a ArtAtlas>,
        village: Option<&'a ArtAtlas>,
        wildlife: Option<&'a ArtAtlas>,
    ) -> Option<&'a ArtAtlas> {
        match self {
            Self::Core => core,
            Self::Village => village,
            Self::Wildlife => wildlife,
        }
    }
}

fn structure(id: &str, kind: &str) -> (Bank, &'static str) {
    let core = Bank::Core;
    let village = Bank::Village;
    match id {
        "craft_carpentry" | "residence_carpenter" | "quarry_shelter" => (village, "village_wood"),
        "craft_textile" | "residence_weaver" => (village, "village_textile"),
        "craft_pottery" => (village, "village_pottery"),
        "craft_bakery" | "residence_cook" => (village, "village_bakery"),
        "craft_inn" => (core, "guesthouse"),
        "craft_post" | "residence_mail_carrier" => (village, "village_post"),
        "market_depot" | "south_barn" => (village, "village_depot"),
        "commons_hall" => (village, "village_hall"),
        "stall_produce" | "stall_fruit_flower" => (village, "village_produce_empty"),
        "stall_fish" => (village, "village_fish_empty"),
        "stall_seed" => (village, "village_cloth_empty"),
        "stall_cloth" => (village, "village_cloth_rear"),
        "stall_meal" => (village, "village_meal_rear"),
        "residence_seed_merchant" | "residence_florist" | "residence_fruit_grower" => {
            (village, "village_seed")
        }
        "residence_fisher" | "residence_road_worker" => (core, "east_house_level_3"),
        "residence_traveling_painter" => (core, "west_house_level_3"),
        _ if kind == "duplex" => (core, "housing_shared_move_in"),
        _ => (village, "village_seed"),
    }
}

fn resident_palette(female: bool, style: usize) -> Palette {
    const OUTFITS: [[[u8; 3]; 4]; 8] = [
        [
            [57, 113, 122],
            [90, 152, 149],
            [42, 73, 84],
            [108, 168, 162],
        ],
        [
            [117, 83, 113],
            [162, 122, 156],
            [83, 61, 93],
            [192, 149, 181],
        ],
        [
            [110, 120, 72],
            [155, 151, 91],
            [73, 85, 65],
            [187, 188, 119],
        ],
        [[139, 87, 55], [179, 116, 67], [95, 66, 48], [208, 153, 93]],
        [
            [93, 110, 142],
            [139, 151, 177],
            [60, 75, 111],
            [168, 179, 189],
        ],
        [
            [164, 126, 66],
            [209, 169, 85],
            [99, 81, 52],
            [217, 177, 111],
        ],
        [
            [136, 81, 78],
            [181, 112, 100],
            [95, 62, 73],
            [211, 148, 128],
        ],
        [
            [94, 105, 101],
            [124, 142, 131],
            [56, 81, 73],
            [156, 169, 136],
        ],
    ];
    let colors = OUTFITS[style % OUTFITS.len()];
    Palette::ResidentClothes {
        female,
        color: [colors[2], colors[0], colors[1]],
    }
}

// Sample the exact compiled shapes in art texels. The small world-aligned
// perturbation softens the long banks without disconnecting the grid shoreline.
pub(super) fn village_water_depth(ax: i32, ay: i32) -> Option<i32> {
    use crate::map_data::WaterShape;
    let rx = ax as f32;
    let ry = ay as f32;
    let mut result = None;
    for water in WILLOW_VILLAGE.water {
        let distance = match water.shape {
            WaterShape::Pond { center, radius } => {
                let center = (f32::from(center.x) * 16.0, f32::from(center.y) * 16.0);
                let radius = (f32::from(radius.x) * 16.0, f32::from(radius.y) * 16.0);
                let dx = rx - center.0;
                let dy = ry - center.1;
                if dx.abs() > radius.0 + 19.0 || dy.abs() > radius.1 + 19.0 {
                    continue;
                }
                (1.0 - (dx / radius.0).hypot(dy / radius.1)) * radius.0.min(radius.1)
            }
            WaterShape::Polygon(polygon) => {
                if !(54 * 16..=71 * 16).contains(&ax) {
                    continue;
                }
                let mut crossings = [0.0_f32; 8];
                let mut count = 0;
                for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
                    let (from, to) = (f32::from(a.y) * 16.0, f32::from(b.y) * 16.0);
                    if (from > ry) == (to > ry) {
                        continue;
                    }
                    if count == crossings.len() {
                        break;
                    }
                    let (left, right) = (f32::from(a.x) * 16.0, f32::from(b.x) * 16.0);
                    crossings[count] = left + (right - left) * (ry - from) / (to - from);
                    count += 1;
                }
                if count == 0 {
                    continue;
                }
                crossings[..count].sort_unstable_by(f32::total_cmp);
                let mut depth = f32::NEG_INFINITY;
                for channel in crossings[..count].chunks_exact(2) {
                    depth = depth.max((rx - channel[0]).min(channel[1] - rx));
                }
                depth
            }
        };
        if distance.is_finite() {
            let taper = ((ax + ay * 2).rem_euclid(29) - 14).abs() / 5 - 1;
            let distance = distance as i32 + taper;
            result = Some(result.map_or(distance, |prior: i32| prior.max(distance)));
        }
    }
    result
}

pub(super) fn bridge_planks(ax: i32, ay: i32) -> [u8; 3] {
    let grain = (terrain_hash(ax.div_euclid(3), ay.div_euclid(7)) % 13) as u8;
    let joint = ax.rem_euclid(9);
    if joint == 0 {
        [91, 65, 45]
    } else if joint == 1 {
        [178 + grain, 141 + grain, 93 + grain]
    } else {
        [147 + grain, 108 + grain, 64 + grain]
    }
}

pub(super) fn market_pavers(ax: i32, ay: i32, edge: i32) -> [u8; 3] {
    let row = ay.div_euclid(7);
    let stone_x = ax + row.rem_euclid(2) * 5;
    let texture = (terrain_hash(stone_x.div_euclid(10), row) % 19) as u8;
    let mortar = (ay.rem_euclid(7) == 0 || stone_x.rem_euclid(10) == 0) && edge > 1;
    if mortar {
        [126, 109, 85]
    } else {
        [156 + texture, 139 + texture, 107 + texture]
    }
}

pub(super) fn public_paving(x: i32, y: i32, edge: i32, paving: u8) -> [u8; 3] {
    if paving == 1 {
        let gravel = (terrain_hash(x.div_euclid(3), y.div_euclid(2)) % 22) as u8;
        [113 + gravel, 102 + gravel, 80 + gravel]
    } else {
        let [r, g, b] = market_pavers(x, y, edge);
        [
            r.saturating_add(8),
            g.saturating_add(12),
            b.saturating_add(17),
        ]
    }
}

pub(super) fn decorated_ground(
    zone: &str,
    ground: Ground,
    x: i32,
    y: i32,
    edge: i32,
    market_courts: (bool, bool),
) -> Option<([u8; 3], u8)> {
    if matches!(ground, Ground::Water | Ground::Rock | Ground::Soil) {
        return None;
    }
    fn tone(p: (i32, i32), center: (i32, i32), radius: (i32, i32), taper: i32) -> i32 {
        let dx = (p.0 - center.0) * 64 / radius.0;
        let dy = (p.1 - center.1) * 64 / radius.1;
        64 * 64 + (taper - 3) * 150 - dx * dx - dy * dy
    }
    let p = (x, y);
    let taper = edge_jitter(x, y);
    if zone == "market" {
        let tx = x.div_euclid(16);
        let ty = y.div_euclid(16);
        if (79..=116).contains(&tx) && (40..=57).contains(&ty) {
            let (weekly, fixed) = market_courts;
            let weekly_strip =
                (80..=92).contains(&tx) && ((43..=49).contains(&ty) || (51..=53).contains(&ty));
            let fixed_strip = (99..=112).contains(&tx) && (43..=47).contains(&ty);
            let apron = weekly && weekly_strip || fixed && fixed_strip;
            if apron || ground == Ground::Path {
                let tint = if ground == Ground::Path { 0 } else { 9 };
                return Some((
                    market_pavers(x, y, edge).map(|c| c.saturating_sub(tint)),
                    255,
                ));
            }
        }
    }
    if zone == "home" {
        let courtyards = [
            ((12 * 16 + 8, 16 * 16 + 18), (46, 36)),
            ((24 * 16 + 8, 16 * 16 + 18), (42, 31)),
            ((17 * 16 + 8, 19 * 16 + 8), (49, 35)),
            ((30 * 16 + 8, 24 * 16), (33, 29)),
            ((35 * 16 + 8, 23 * 16), (44, 25)),
        ];
        let weight = courtyards
            .into_iter()
            .map(|(center, radius)| tone(p, center, radius, taper))
            .max()
            .unwrap_or(0);
        if weight > 0 {
            let patina = (terrain_hash(x.div_euclid(5), y.div_euclid(3)) % 14) as u8;
            let crushed = terrain_hash(x.div_euclid(3), y.div_euclid(4)) % 31;
            let base = if crushed == 0 {
                [155 + patina, 136 + patina, 103 + patina]
            } else {
                [142 + patina, 128 + patina, 91 + patina]
            };
            return Some((base, (weight / 6).clamp(0, 205) as u8));
        }
    }
    if zone == "forest" && ground == Ground::Grass {
        let canopy = [
            ((12 * 16, 104 * 16), (126, 75)),
            ((25 * 16, 105 * 16), (94, 54)),
            ((15 * 16, 115 * 16), (171, 75)),
            ((25 * 16, 119 * 16), (86, 42)),
        ];
        let weight = canopy
            .into_iter()
            .map(|(center, radius)| tone(p, center, radius, taper))
            .max()
            .unwrap_or(0);
        if weight > 0 {
            let moss = (terrain_hash(x.div_euclid(4), y.div_euclid(3)) % 15) as u8;
            return Some((
                [91 + moss, 103 + moss, 61 + moss],
                (weight / 6).clamp(0, 168) as u8,
            ));
        }
    }
    None
}

pub(super) fn visible_fishing_float(world: &LivingWorld, foot: Point) -> Option<Point> {
    let x = (foot.x as i32).div_euclid(8);
    let y = (foot.y as i32).div_euclid(8);
    for distance in 1_i32..=4 {
        for (dx, dy) in [(0, distance), (distance, 0), (-distance, 0), (0, -distance)] {
            if world.ground(x + dx, y + dy) == Ground::Water {
                let p = Point::at_tile(x + dx, y + dy);
                if village_water_depth((p.x * 2.0) as i32, (p.y * 2.0) as i32)
                    .is_some_and(|d| d >= 3)
                {
                    return Some(p);
                }
            }
        }
    }
    None
}
