//! Isolated render fixtures. They exercise pixels using explicit test state, not simulation progression.
use super::*;
use std::io::{BufWriter, Write};

const SIZE: (i32, i32) = (1320, 860);

fn frame(
    world: &LivingWorld,
    focus: Point,
    corner: Option<(TilePos, u8)>,
    commands: &[FrameCommand],
) -> (LivingView, (i32, i32)) {
    frame_with_wildlife(world, focus, corner, commands, &[])
}

fn frame_with_wildlife(
    world: &LivingWorld,
    focus: Point,
    corner: Option<(TilePos, u8)>,
    commands: &[FrameCommand],
    wildlife: &[WildlifeVisual],
) -> (LivingView, (i32, i32)) {
    let size = crate::viewport::logical_size(SIZE);
    let mut camera = Camera {
        x: focus.x - size.0 as f32 / 2.0,
        y: focus.y - size.1 as f32 * 0.63,
        follow: None,
    };
    camera.clamp_in_world(world, size);
    let origin = (camera.x.round() as i32, camera.y.round() as i32);
    let mut view = LivingView::new();
    view.enable_builtin_premium().unwrap();
    view.set_village_public_corner(corner);
    let before = serde_json::to_vec(world).unwrap();
    view.render_village(world, &camera, SIZE, commands, None, &[], wildlife, &[]);
    assert_eq!(before, serde_json::to_vec(world).unwrap());
    (view, origin)
}

fn save(name: &str, view: &LivingView) {
    let Some(folder) = std::env::var_os("CAT_HOME_VISUAL_FIXTURE_DIR") else {
        return;
    };
    let folder = std::path::Path::new(&folder);
    std::fs::create_dir_all(folder).unwrap();
    let mut out =
        BufWriter::new(std::fs::File::create(folder.join(format!("{name}.ppm"))).unwrap());
    writeln!(out, "P6\n{} {}\n255", SIZE.0, SIZE.1).unwrap();
    let mut row = Vec::with_capacity(SIZE.0 as usize * 3);
    for y in 0..SIZE.1 {
        row.clear();
        let sy = crate::viewport::source_coordinate(y);
        for x in 0..SIZE.0 {
            let sx = crate::viewport::source_coordinate(x);
            let i = ((sy * view.width + sx) * 4) as usize;
            let pixel = &view.pixels()[i..i + 4];
            assert_eq!(pixel[3], 255);
            row.extend([pixel[2], pixel[1], pixel[0]]);
        }
        out.write_all(&row).unwrap();
    }
    out.flush().unwrap();
}

#[test]
fn protagonist_walk_head_is_identical_while_legs_change() {
    let mut view = LivingView::new();
    view.enable_builtin_premium().unwrap();
    let feet = (80, 65);
    let scale = Scale::new(3, 4);
    let paint = Palette::Unchanged;
    let area = (160, 130);
    view.width = area.0;
    view.height = area.1 + 16;
    view.density = 2;
    for role in ["female", "male"] {
        for action in ["idle", "walk", "carry", "hammer"] {
            let mut frames = Vec::new();
            for i in 0..3 {
                view.pixels
                    .resize((view.width * view.height * 4) as usize, 0);
                view.pixels.fill(0);
                assert!(view.premium_actor_body(
                    role,
                    action,
                    &format!("{role}_{action}_{i}"),
                    (feet.0 / 2, feet.1),
                    false,
                    scale,
                    paint
                ));
                frames.push(view.pixels.clone());
            }
            let head_end = ((feet.1 * 2 - scale.apply(22)) * area.0 * 4) as usize;
            assert_eq!(
                frames[0][..head_end],
                frames[1][..head_end],
                "{role} {action} head"
            );
            assert_eq!(
                frames[1][..head_end],
                frames[2][..head_end],
                "{role} {action} head"
            );
            if action == "walk" {
                assert_ne!(frames[0][head_end..], frames[1][head_end..]);
            }
        }
    }
    if let Some(out) = std::env::var_os("CAT_HOME_VISUAL_FIXTURE_DIR") {
        let out = std::path::Path::new(&out);
        std::fs::create_dir_all(out).unwrap();
        for (role, row) in [("female", 0), ("male", 1)] {
            for i in 0..3 {
                for (mode, corrected) in [("before", false), ("after", true)] {
                    view.pixels.fill(0);
                    let key = format!("{role}_walk_{i}");
                    if corrected {
                        view.premium_actor_body(
                            role,
                            "walk",
                            &key,
                            (feet.0 / 2, feet.1),
                            false,
                            scale,
                            paint,
                        );
                    } else {
                        view.premium_at_scale(&key, (feet.0, feet.1 * 2), false, scale, paint);
                    }
                    let path = out.join(format!("face-{mode}-{row}-{i}.pam"));
                    let mut file = BufWriter::new(std::fs::File::create(path).unwrap());
                    writeln!(
                        file,
                        "P7\nWIDTH 40\nHEIGHT 46\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR"
                    )
                    .unwrap();
                    for y in (feet.1 * 2 - 42)..(feet.1 * 2 + 4) {
                        for x in (feet.0 - 20)..(feet.0 + 20) {
                            let off = ((y * view.width + x) * 4) as usize;
                            let p = &view.pixels[off..off + 4];
                            file.write_all(&[p[2], p[1], p[0], p[3]]).unwrap();
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn resident_cargo_pixels_come_only_from_the_held_state() {
    use crate::visitors::{Profession, VisitorHeld, VisitorPose, VisitorVisual};

    let mut view = LivingView::new();
    view.enable_builtin_premium().unwrap();
    view.width = 140;
    view.height = 140;
    view.density = 2;
    view.pixels
        .resize((view.width * view.height * 4) as usize, 0);
    for profession in [Profession::Carpenter, Profession::Cook, Profession::Fisher] {
        let mut frame = VisitorVisual {
            profession,
            pose: VisitorPose::Carrying,
            position: Point { x: 35.0, y: 50.0 },
            facing_left: false,
            animation_frame: 0,
            label: "",
            activity: "",
            held: None,
        };
        view.village_resident(&frame, (0, 0));
        let bare = view.pixels.clone();
        view.pixels.fill(0);
        frame.held = Some(VisitorHeld::Goods(Item::Food));
        view.village_resident(&frame, (0, 0));
        assert_ne!(bare, view.pixels);
        view.pixels.fill(0);
        frame.held = None;
        view.village_resident(&frame, (0, 0));
        assert_eq!(bare, view.pixels);
        view.pixels.fill(0);
    }
}

fn art_matches(
    view: &LivingView,
    atlas: &ArtAtlas,
    id: &str,
    anchor: (i32, i32),
) -> (usize, usize) {
    let (frame, page) = atlas.frame(id).unwrap();
    let [left, top, width, height] = frame.rect.map(i32::from);
    let shift = (
        anchor.0 * 2 - i32::from(frame.anchor[0]),
        anchor.1 * 2 - i32::from(frame.anchor[1]),
    );
    let mut opaque = 0;
    let mut matching = 0;
    for y in 0..height {
        for x in 0..width {
            let (px, py) = (shift.0 + x, shift.1 + y);
            if !(0..view.width).contains(&px) || !(0..view.height).contains(&py) {
                continue;
            }
            let at = (((top + y) * 256 + left + x) * 4) as usize;
            if page[at + 3] == 255 {
                opaque += 1;
                let dst = ((py * view.width + px) * 4) as usize;
                matching += usize::from(page[at..at + 4] == view.pixels()[dst..dst + 4]);
            }
        }
    }
    (matching, opaque)
}

fn changed(a: &LivingView, b: &LivingView) -> Vec<(i32, i32)> {
    assert_eq!((a.width, a.height), (b.width, b.height));
    a.pixels()
        .chunks_exact(4)
        .zip(b.pixels().chunks_exact(4))
        .enumerate()
        .filter_map(|(i, (before, after))| {
            (before != after).then_some((i as i32 % a.width, i as i32 / a.width))
        })
        .collect()
}

#[test]
fn mill_is_drawn_once_then_moves_between_workshops() {
    for (site, installed) in [("home_workshop", 8), ("craft_carpentry", 10)] {
        let door = point(WILLOW_VILLAGE.building(site).unwrap().door);
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.test_market_visual_stage(9);
        assert_eq!(world.mill_display_site(), None);
        let (absent, origin) = frame(&world, door, None, &[]);
        world.test_market_visual_stage(installed);
        assert_eq!(world.mill_display_site(), Some(site));
        let (present, _) = frame(&world, door, None, &[]);
        let (x, y) = local(door, origin);
        let anchor = (x - 12, y - 5);
        let atlas = present.village_art.as_ref().unwrap();
        let (matches, opaque) = art_matches(&present, atlas, "village_mill", anchor);
        let (missing, _) = art_matches(&absent, atlas, "village_mill", anchor);
        assert!(opaque > 160 && matches * 10 >= opaque * 9 && missing * 4 < opaque);
        let diffs = changed(&absent, &present);
        assert!(diffs.len() > 160);
        assert!(
            diffs.iter().all(|&(px, py)| {
                (anchor.0 * 2 - 13..anchor.0 * 2 + 14).contains(&px)
                    && (anchor.1 * 2 - 25..anchor.1 * 2 + 1).contains(&py)
            }),
            "equipment changed more than its own display at {site}"
        );
        save(&format!("q58-{site}-uninstalled"), &absent);
        save(&format!("q58-{site}-installed"), &present);
    }
}

#[test]
fn market_surface_and_drain_change_at_authoritative_stages() {
    let water = WILLOW_VILLAGE.poi("market_well").unwrap().tile;
    let well = point(water);
    let wellhead = WILLOW_VILLAGE
        .solid_props
        .iter()
        .find(|prop| prop.reader_poi == Some("market_well"))
        .expect("market water point needs its own solid wellhead")
        .tile;
    let mut samples = Vec::new();
    for (stage, name) in [
        (1, "survey"),
        (2, "first-surface"),
        (3, "full-surface"),
        (6, "tested-drain"),
    ] {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.test_market_visual_stage(stage);
        let (lane, display) = world.market_construction_paving();
        assert_eq!(display, stage.min(3));
        assert!(
            lane.iter()
                .all(|at| world.can_walk(at.x.into(), at.y.into()))
        );
        assert!(world.can_walk(water.x.into(), water.y.into()));
        assert!(!world.can_walk(wellhead.x.into(), wellhead.y.into()));
        let (view, origin) = frame(&world, well, None, &[]);
        let (visible, total) = art_matches(
            &view,
            view.art.as_ref().unwrap(),
            "well_stone",
            local(point(wellhead), origin),
        );
        assert!(total > 300 && visible * 4 >= total * 3);
        let (x, y) = local(well, origin);
        let (matches, opaque) = art_matches(
            &view,
            view.village_art.as_ref().unwrap(),
            "village_drain",
            (x - 6, y + 5),
        );
        assert!(opaque > 30);
        assert_eq!(matches * 4 >= opaque * 3, stage >= 6);
        save(&format!("q54-q56-{name}"), &view);
        samples.push((view, origin));
    }
    let lane_tiles = |before: usize, after: usize| {
        let (view, origin) = &samples[before];
        assert_eq!(*origin, samples[after].1);
        changed(view, &samples[after].0)
            .into_iter()
            .map(|(x, y)| {
                TilePos::new(
                    ((x / 2 + origin.0) / 8) as u16,
                    ((y / 2 + origin.1) / 8) as u16,
                )
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    for (change, segment) in [(lane_tiles(0, 1), 97..=99), (lane_tiles(1, 2), 97..=102)] {
        let centered = (segment.clone()).map(|x| TilePos::new(x, 50));
        assert!(centered.into_iter().all(|tile| change.contains(&tile)));
        assert!(
            change
                .iter()
                .all(|tile| { (96..=103).contains(&tile.x) && (49..=51).contains(&tile.y) }),
            "road edge affected the rest of the market: {change:?}"
        );
    }
    assert!(!lane_tiles(2, 3).is_empty());
}

#[test]
fn each_house_stage_renders_on_its_own_parcel_and_road_stays_clear() {
    for plan in [HousePlan::Single, HousePlan::Shared] {
        let mut material_anchor = None;
        for stage in [
            HousingPhase::Clear,
            HousingPhase::Foundation,
            HousingPhase::Frame,
            HousingPhase::Roof,
            HousingPhase::FitOut,
            HousingPhase::MoveIn,
        ] {
            let mut world = LivingWorld::new_village_wall_ms(0);
            world.test_house_visual_stage(plan, stage);
            let home = world.housing_sites().next().unwrap();
            assert_eq!((home.kind, home.phase), (plan, stage));
            let (view, origin) = frame(&world, home.position, None, &[]);
            let key = format!("{}_{}", plan.asset_name(), stage.asset_name());
            let (x, y) = local(home.position, origin);
            let (matches, opaque) =
                art_matches(&view, view.art.as_ref().unwrap(), &key, (x, y - 4));
            assert!(
                opaque > 300 && matches * 4 > opaque * 3,
                "{key}: {matches}/{opaque}"
            );
            if stage != HousingPhase::MoveIn {
                let material = home
                    .material_position
                    .expect("local working material point");
                material_anchor = Some(material);
                assert!((material.x - home.position.x).abs() < 32.0);
                assert!((material.y - home.position.y).abs() < 24.0);
            }
            let (matching, opaque) = art_matches(
                &view,
                view.village_art.as_ref().unwrap(),
                "village_materials",
                local(material_anchor.unwrap(), origin),
            );
            assert!(
                matching * 4 < opaque,
                "{key} must not invent stored construction material: {matching}/{opaque}"
            );
            let tile = TilePos::new((home.position.x as u16) / 8, (home.position.y as u16) / 8);
            assert!(
                world.can_walk(tile.x.into(), tile.y.into()),
                "{key} exterior threshold obstructed"
            );
            save(&key, &view);
        }
    }
}

#[test]
fn mature_neighbor_roofs_exist_and_seasonal_corner_clears_road() {
    let mut world = LivingWorld::new_village_wall_ms(0);
    world.test_market_visual_stage(10);
    let duplex = WILLOW_VILLAGE.building("east_duplex_1").unwrap();
    assert!(world.village_building_active(duplex.id));
    let (view, origin) = frame(&world, point(duplex.door), None, &[]);
    let (matches, opaque) = art_matches(
        &view,
        view.art.as_ref().unwrap(),
        "housing_shared_move_in",
        local(point(duplex.door), origin),
    );
    assert!(opaque > 1000 && matches * 4 >= opaque * 3);
    save("q62-q63-east-duplex", &view);

    let hall = WILLOW_VILLAGE.building("commons_hall").unwrap();
    assert!(world.village_building_active(hall.id));
    let door = point(hall.door);
    let (baseline, origin) = frame(&world, door, None, &[]);
    let (x, y) = local(door, origin);
    let pigeon = WildlifeVisual {
        sprite: "wild_white_pigeon_idle_0",
        position: point(hall.door.offset(0, -4).unwrap()),
        facing_left: false,
    };
    let (perched, _) = frame_with_wildlife(&world, door, None, &[], &[pigeon]);
    let (in_roof, opaque) = art_matches(
        &perched,
        perched.wildlife_art.as_ref().unwrap(),
        pigeon.sprite,
        local(pigeon.position, origin),
    );
    assert!(
        opaque > 20 && in_roof * 5 >= opaque * 4,
        "hall covered the perched pigeon"
    );
    save("commons-hall-perch", &perched);
    let mut prior = None;
    for (season, name) in ["spring", "summer", "autumn", "winter"]
        .into_iter()
        .enumerate()
    {
        let (view, _) = frame(&world, door, Some((hall.door, season as u8)), &[]);
        let diff = changed(&baseline, &view);
        assert!(diff.len() >= 20);
        assert!(
            diff.iter()
                .all(|&(px, py)| px >= x * 2 + 3 && py < (y - 6) * 2),
            "corner covers part of the hall door or approach"
        );
        if let Some(prior) = prior.replace(view.pixels().to_vec()) {
            assert_ne!(view.pixels(), prior);
        }
        save(&format!("q69-corner-{name}"), &view);
    }
}

#[test]
fn actual_forestry_worker_render_keeps_some_body_visible() {
    let mut world = LivingWorld::new_village_wall_ms(0);
    let foot = world.test_forest_visual_worker();
    let (working, origin) = frame(&world, foot, None, &[]);
    let hide = FrameCommand {
        pack_id: PackId::parse("test.no-draw").unwrap(),
        frame_id: LocalId::parse("blank").unwrap(),
        point: foot,
        depth: 0,
        actor_override: Some(ActorKind::Ayu),
        flip: false,
    };
    let (background, _) = frame(&world, foot, None, &[hide]);
    let (x, y) = local(foot, origin);
    let body = changed(&background, &working)
        .iter()
        .filter(|&&(px, py)| {
            ((x - 8) * 2..(x + 8) * 2).contains(&px) && ((y - 25) * 2..(y - 4) * 2).contains(&py)
        })
        .count();
    save("forest-worker-real-work-foot", &working);
    assert!(
        body >= 120,
        "only {body} distinctive source pixels above the working feet"
    );
}

#[test]
fn indoor_awake_actor_is_visible_at_the_original_work_anchor() {
    let mut world = LivingWorld::new_village_wall_ms(0);
    let kitchen = world.buildings()[2].position;
    world.test_camera_actor(ActorKind::Xiaohe, kitchen, Pose::Work, true);
    world.test_camera_actor(ActorKind::Ayu, Point::at_tile(85, 51), Pose::Sleep, true);
    let (awake, origin) = frame(&world, kitchen, None, &[]);
    let hide = FrameCommand {
        pack_id: PackId::parse("test.no-draw").unwrap(),
        frame_id: LocalId::parse("blank").unwrap(),
        point: kitchen,
        depth: 0,
        actor_override: Some(ActorKind::Xiaohe),
        flip: false,
    };
    let (no_body, _) = frame(&world, kitchen, None, &[hide]);
    let (x, y) = local(kitchen, origin);
    let body = changed(&awake, &no_body);
    assert!(body.len() > 100, "cutaway does not reveal the protagonist");
    assert!(body.iter().all(|&(px, py)| {
        ((x - 18) * 2..(x + 18) * 2).contains(&px) && ((y - 30) * 2..(y + 7) * 2).contains(&py)
    }));
    save("awake-indoor-kitchen-cutaway", &awake);
    world.test_camera_actor(ActorKind::Xiaohe, kitchen, Pose::Sleep, true);
    let (asleep, _) = frame(&world, kitchen, None, &[]);
    assert!(changed(&asleep, &awake).len() > body.len());
}

#[test]
fn outside_awake_actor_is_visible_behind_foreground_roof() {
    let mut world = LivingWorld::new_village_wall_ms(0);
    let house = world.buildings()[0];
    let roof = Point {
        x: house.position.x,
        y: house.position.y - 16.0,
    };
    world.test_camera_actor(ActorKind::Xiaohe, roof, Pose::Walk, false);
    world.test_camera_actor(ActorKind::Ayu, Point::at_tile(85, 51), Pose::Sleep, true);
    let (awake, origin) = frame(&world, roof, None, &[]);
    let hide = FrameCommand {
        pack_id: PackId::parse("test.no-draw").unwrap(),
        frame_id: LocalId::parse("blank").unwrap(),
        point: roof,
        depth: 0,
        actor_override: Some(ActorKind::Xiaohe),
        flip: false,
    };
    let (no_body, _) = frame(&world, roof, None, &[hide]);
    let (x, y) = local(roof, origin);
    let body = changed(&awake, &no_body);
    assert!(
        body.len() > 100,
        "roof cutaway reveals too little of the actor"
    );
    assert!(body.iter().all(|&(px, py)| {
        ((x - 14) * 2..(x + 14) * 2).contains(&px) && ((y - 24) * 2..(y + 8) * 2).contains(&py)
    }));
    save("awake-behind-foreground-roof", &awake);
}
