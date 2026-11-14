//! Willow Village rendering using the same high-density compositing as the yard.
use super::*;
use crate::companionship::{REWARDS, RewardStyle};
use crate::map_data::{
    ArtRect, BuildingDef, ScenicKind, ScenicObject, SlotPos, TilePos, WILLOW_VILLAGE,
};
use crate::visitors::{VisitorHeld, VisitorPose, VisitorTool};

const CHUNK_ART: i32 = 256;
const MAP_CHUNKS: usize = 96;
const CHUNK_COLUMNS: usize = 12;
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

pub(super) struct VillageIndex {
    chunks: [Vec<usize>; MAP_CHUNKS],
    bounds: Vec<Option<ArtRect>>,
    scenic: [Vec<usize>; MAP_CHUNKS],
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
        for (id, obj) in WILLOW_VILLAGE.scenic.iter().enumerate() {
            let p = point(obj.tile);
            let bounds = ArtRect {
                x: (p.x * 2.0) as i32 - 39,
                y: (p.y * 2.0) as i32 - 86,
                w: 84,
                h: 102,
            };
            for_each_chunk(bounds, |chunk| scenic[chunk].push(id));
        }
        Self {
            chunks,
            bounds: envelopes,
            scenic,
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

    fn scenic_in_view(&self, view: ArtRect) -> Vec<usize> {
        let mut out = Vec::with_capacity(64);
        for_each_chunk(view, |chunk| out.extend_from_slice(&self.scenic[chunk]));
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

fn forest_actor_regions(world: &LivingWorld, origin: (i32, i32)) -> [Option<ArtRect>; 2] {
    [0, 1].map(|i| {
        let actor = &world.actors()[i];
        if actor.inside()
            || (actor.walking_segment().is_none()
                && !matches!(actor.activity, Activity::FellTree | Activity::CarryWood))
        {
            return None;
        }
        let (x, y) = local(actor.position, origin);
        Some(ArtRect {
            x: (x - 7) * 2,
            y: (y - 23) * 2,
            w: 28,
            h: 44,
        })
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

fn intersects(a: ArtRect, b: ArtRect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn crown_has_ink(crown: ArtRect, actor: ArtRect, sprite: Sprite, page: &[u8]) -> bool {
    let [left, top, width, height] = sprite.rect.map(i32::from);
    for y in crown.y.max(actor.y)..(crown.y + crown.h).min(actor.y + actor.h) {
        let sy = top + ((y - sprite.top) * 2 + 1) * height / (sprite.height * 2);
        for x in crown.x.max(actor.x)..(crown.x + crown.w).min(actor.x + actor.w) {
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

enum Visible<'a> {
    Legacy(Drawable<'a>),
    Building(&'static BuildingDef, bool),
    Prop(&'static str, Point),
    DoorTrim(&'static str, Point),
    Wild(&'a WildlifeVisual),
    Resident(&'a VisitorVisual),
    Scenic(&'static ScenicObject),
    Lamp(Point),
    Drain(Point),
    BridgeRail(TilePos, u8),
}

impl LivingView {
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
        self.indoor_reveals =
            [0, 1].map(|i| self.awake_screen_feet[i].filter(|_| world.actors()[i].inside()));
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
        let phases = world.phase_mask();
        let forest_actors = forest_actor_regions(world, origin);
        let rect = ArtRect {
            x: cx * 2,
            y: cy * 2,
            w: self.width,
            h: self.height,
        };
        let spatial = self
            .village_index
            .get_or_insert_with(|| VillageIndex::new(self.art.as_ref(), self.village_art.as_ref()));
        let mut active = spatial.in_view(rect);
        let scenic = spatial.scenic_in_view(rect);
        let mut drawables = Vec::with_capacity(128);
        let mut push = |depth, item| {
            if drawables.len() < 512 {
                drawables.push((depth, item));
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
        for site in world.housing_sites() {
            if clip(self, local(site.position, origin), (59, 58), 10) {
                push(
                    site.position.y as i32 - 4,
                    Visible::Legacy(Drawable::Housing(site)),
                );
            }
        }
        for project in world.developments() {
            if project.complete || project.progress == 0 {
                continue;
            }
            let (asset, site_id) = match project.kind {
                DevelopmentKind::Road | DevelopmentKind::Bridge => continue,
                DevelopmentKind::Market | DevelopmentKind::Stall => {
                    ("village_public_frame", Some("stall_produce"))
                }
                DevelopmentKind::Guesthouse => ("village_public_frame", Some("craft_inn")),
                DevelopmentKind::Depot => ("village_public_frame", Some("market_depot")),
                DevelopmentKind::Well | DevelopmentKind::Garden => {
                    ("village_public_foundation", None)
                }
            };
            let asset = if asset == "village_public_frame" && project.phase < 2 {
                "village_public_foundation"
            } else {
                asset
            };
            let structure = site_id
                .and_then(|id| WILLOW_VILLAGE.building(id))
                .map_or(project.position, structure_anchor);
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
            // One saved plot stage controls the full authored bed footprint.
            for dy in 0..def.rect.h {
                for dx in 0..def.rect.w {
                    let mut cell = *plot;
                    cell.position =
                        Point::at_tile(i32::from(def.rect.x + dx), i32::from(def.rect.y + dy));
                    cell.stage = world.plot_stage(index).unwrap_or(cell.stage);
                    self.premium_plot(&cell, cx, cy);
                }
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
                self.visible_motion |= matches!(v.pose, VisitorPose::Walking | VisitorPose::Carrying)
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
                    v.position.y as i32
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
        for id in scenic {
            let item = &WILLOW_VILLAGE.scenic[id];
            let pos = point(item.tile);
            let replaced = item.kind == ScenicKind::Lamp && lamps.contains(&item.tile);
            if !replaced
                && phases.contains(item.phase)
                && clip(self, local(pos, origin), (42, 45), 9)
            {
                push(pos.y as i32, Visible::Scenic(item));
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
                    self.draw_village_sprite(
                        Bank::Wildlife,
                        v.sprite,
                        (x * 2, y * 2),
                        v.facing_left,
                        Scale::IDENTITY,
                        Palette::Unchanged,
                    );
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
                Visible::Scenic(item) => self.village_scenic(item, origin, &forest_actors),
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
                    self.actor_at(actor, cx, cy, world.world_ms(), Some(world))
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
                        self.village_orchard(phase, (x, y), i % 2 == 1);
                    } else if t.stage == TreeStage::Standing {
                        let (tx, ty) = (t.position.x as i32 / 8, t.position.y as i32 / 8);
                        let sprite = forest_crown(tx, ty, terrain_hash(tx, ty));
                        self.contact_shadow((x + 2, y + 1), (13, 4), 44);
                        self.draw_forest_crown(
                            sprite,
                            (x * 2, y * 2),
                            y * 2,
                            i % 2 == 1,
                            Scale::IDENTITY,
                            &forest_actors,
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
        if let Some((prop, scale)) = v.held.and_then(held_asset) {
            let hand = self
                .premium_hand(&frame, (x, y), v.facing_left)
                .unwrap_or((x * 2, y * 2 - 15));
            self.core_at_texel(
                prop,
                if north {
                    ((x - 5) * 2, (y - 7) * 2)
                } else {
                    hand
                },
                false,
                scale,
                Palette::Unchanged,
            );
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

    fn village_orchard(&mut self, phase: OrchardPhase, (x, y): (i32, i32), flip: bool) {
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
        self.draw_village_sprite(
            Bank::Village,
            key,
            position,
            flip,
            Scale::IDENTITY,
            Palette::Unchanged,
        );
    }

    fn village_scenic(
        &mut self,
        item: &ScenicObject,
        origin: (i32, i32),
        actors: &[Option<ArtRect>; 2],
    ) {
        let (x, y) = local(point(item.tile), origin);
        let seed = terrain_hash(i32::from(item.tile.x), i32::from(item.tile.y));
        let forest = item.id.contains(".forest");
        let anchor_y = if forest && item.kind == ScenicKind::Tree {
            // Sub-tile root placement varies along the natural edge while
            // staying in the authoritative blocking tile.
            y + ((seed >> 7) % 5) as i32 - 2
        } else {
            y
        };
        let (bank, key, scale, shadow) = match item.kind {
            ScenicKind::Tree => {
                let (bank, key) = if forest {
                    let x = i32::from(item.tile.x);
                    let y = i32::from(item.tile.y);
                    (Bank::Village, forest_crown(x, y, seed))
                } else {
                    (
                        Bank::Core,
                        [
                            "tree_oak",
                            "tree_birch",
                            "tree_maple",
                            "tree_oak",
                            "tree_cherry",
                        ][seed as usize % 5],
                    )
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
            ScenicKind::Grass => (Bank::Core, "bush_herb", Scale::new(2, 5), (3, 2)),
            ScenicKind::Sign => (Bank::Village, "village_notice", Scale::new(4, 5), (6, 2)),
            ScenicKind::Lamp => (Bank::Village, "village_lantern", Scale::IDENTITY, (5, 3)),
            ScenicKind::Reeds => (Bank::Village, "village_reeds", Scale::new(4, 5), (6, 3)),
            ScenicKind::Log => (Bank::Core, "logs", Scale::new(4, 5), (7, 3)),
            ScenicKind::WoodPile => (Bank::Village, "village_materials", Scale::new(4, 5), (8, 3)),
        };
        self.contact_shadow(
            (x + 3, anchor_y + 1),
            shadow,
            if item.solid { 38 } else { 20 },
        );
        let position = (x * 2, anchor_y * 2);
        let flip = seed & 2 != 0;
        if forest && item.kind == ScenicKind::Tree {
            self.draw_forest_crown(key, position, y * 2, flip, scale, actors);
        } else {
            self.draw_village_sprite(bank, key, position, flip, scale, Palette::Unchanged);
        }
    }

    fn draw_forest_crown(
        &mut self,
        key: &str,
        position: (i32, i32),
        depth: i32,
        flip: bool,
        scale: Scale,
        actors: &[Option<ArtRect>; 2],
    ) {
        let Some(atlas) = &self.village_art else {
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
        let crown = ArtRect {
            x: sprite.left,
            y: sprite.top,
            w: sprite.width,
            h: roots - sprite.top,
        };
        let overlaps = actors.iter().flatten().any(|actor| {
            depth > actor.y + actor.h + 2
                && intersects(crown, *actor)
                && crown_has_ink(crown, *actor, sprite, page)
        });
        let palette = if overlaps {
            Palette::ForestCanopy { roots_start: roots }
        } else {
            Palette::Unchanged
        };
        blit_into(
            &mut self.pixels,
            (self.width, self.height),
            page,
            256,
            sprite,
            palette,
        );
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
        let atlas = match bank {
            Bank::Core => &self.art,
            Bank::Village => &self.village_art,
            Bank::Wildlife => &self.wildlife_art,
        };
        let Some(atlas) = atlas.as_ref() else {
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
    fn overscan_and_shifted_terrain_match_a_fresh_village_frame() {
        use crate::viewport::PixelOrigin;
        let world = LivingWorld::new_village_wall_ms(0);
        let physical = (481, 301);
        let map_world = (192 * 8, 128 * 8);
        let mut scrolled = LivingView::new();
        scrolled.enable_builtin_premium().unwrap();
        for (x, y) in [(87.1, 118.8), (87.8, 117.8), (85.3, 120.1), (88.9, 122.8), (1500.0, 1000.0)] {
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
        let camera = Camera { x: 60.0, y: 120.0, follow: None };
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
        return None;
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
    if ground == Ground::Grass {
        return crate::landscape::ground_cover(zone, x, y);
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
