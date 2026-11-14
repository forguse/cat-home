#![allow(dead_code)]
#[path = "../src/activities.rs"]
mod activities;
#[path = "../src/cast.rs"]
mod cast;
#[path = "../src/clock.rs"]
mod clock;
#[path = "../src/ecology.rs"]
mod ecology;
#[path = "../src/effects.rs"]
mod effects;
#[path = "../src/fishing.rs"]
mod fishing;
#[path = "../src/projects.rs"]
mod projects;
#[path = "../src/renderer.rs"]
mod renderer;
#[path = "../src/simulation.rs"]
mod simulation;
#[path = "../src/world.rs"]
mod world;

fn main() {
    use simulation::{Season, WeatherMode, snapshot};
    use world::{ManualActions, Scene};
    let destination = std::path::PathBuf::from(std::env::args_os().nth(1).unwrap());
    match std::env::args().nth(2).as_deref() {
        Some("detail") => return detailed_demo(&destination),
        Some("art") => return art_demo(&destination),
        _ => {}
    }
    let start = 1_790_000_000;
    for (name, size, age, minute, season) in [
        ("default", (160, 100), 128, 730, Season::Spring),
        ("wide", (272, 116), 201_000, 1_092, Season::Autumn),
        ("tall", (174, 176), 306_000, 530, Season::Spring),
    ] {
        let now = start + age;
        let state = snapshot(now, start, minute, season, WeatherMode::Clear, None);
        let mut scene = Scene::new();
        let projects = [projects::Project::default(); projects::LOTS];
        let _ = scene.render_view(
            &state,
            now * 1000,
            &ManualActions::default(),
            size,
            &[1; 16],
            &projects,
            &effects::Effects::default(),
        );
        let (pixels, _) = scene.render_view(
            &state,
            now * 1000 + 3_500,
            &ManualActions::default(),
            size,
            &[1; 16],
            &projects,
            &effects::Effects::default(),
        );
        std::fs::write(
            destination.join(format!("{name}-{}x{}.bgra", size.0, size.1)),
            pixels,
        )
        .unwrap();
    }
    let now = start + 90 * 86_400;
    let state = snapshot(now, start, 730, Season::Spring, WeatherMode::Clear, None);
    for kind in projects::BLUEPRINTS {
        let mut projects = [projects::Project::default(); projects::LOTS];
        projects[4].kind = kind;
        let mut scene = Scene::new();
        let (pixels, _) = scene.render_view(
            &state,
            now * 1000,
            &ManualActions::default(),
            (400, 180),
            &[1; 16],
            &projects,
            &effects::Effects::default(),
        );
        let mut crop = Vec::new();
        for row in pixels.chunks_exact(400 * 4).skip(31).take(42) {
            crop.extend_from_slice(&row[198 * 4..232 * 4]);
        }
        std::fs::write(
            destination.join(format!("structure-{:02}.bgra", kind.style())),
            crop,
        )
        .unwrap();
    }
}

fn art_demo(destination: &std::path::Path) {
    use simulation::{CareAction, CatAction, Season, Visitor, WeatherMode, snapshot};
    use world::{ManualActions, Scene};

    let start = 1_790_000_000;
    for (name, size, age, minute) in [
        ("art-day-480x300", (160, 100), 80 * 3_600, 660),
        ("art-evening-480x300", (160, 100), 81 * 3_600, 1115),
        ("art-yard-708x369", (236, 123), 25 * 86_400 + 31_200, 700),
        ("art-night-480x300", (160, 100), 82 * 3_600, 1300),
    ] {
        let now = start + age;
        let mut state = snapshot(now, start, minute, Season::Spring, WeatherMode::Clear, None);
        let mut effects = effects::Effects::default();
        let mut projects = [projects::Project::default(); projects::LOTS];
        let mut scene = Scene::new();
        let actions = ManualActions::default();
        let unlocks = [1; 16];
        if name.starts_with("art-yard") {
            projects[0].kind = projects::Kind::Cabin;
            projects[0].start_age = 10 * 86_400;
            projects[1].kind = projects::Kind::Treehouse;
        }
        if !name.starts_with("art-night") {
            state.visitor = Visitor::Sparrow;
            state.cat = CatAction::Walk;
            state.caretaker = CareAction::Tending;
        }
        for offset in [0, 1350, 2700, 4050, 5400] {
            let frame_name = if offset == 5400 {
                name.to_owned()
            } else {
                format!("{name}-{offset}")
            };
            detailed_frame(
                destination,
                &frame_name,
                &mut scene,
                &state,
                now * 1000 + offset,
                &actions,
                size,
                &unlocks,
                &projects,
                &mut effects,
            );
        }
    }
}

fn detailed_demo(destination: &std::path::Path) {
    use cast::{Context, Role};
    use effects::Effects;
    use simulation::{CatAction, Season, Visitor, WeatherMode, snapshot};
    use world::{ManualActions, Scene};

    let start = 1_790_000_000;
    let size = (272, 145);
    let unlocks = [1; 16];
    let mut projects = [projects::Project::default(); projects::LOTS];
    projects[0].kind = projects::Kind::Cabin;
    projects[1].kind = projects::Kind::Treehouse;
    let mut effects = Effects::default();
    let mut scene = Scene::new();
    let duck = Role(19);
    let frog = Role(17);
    let first = (2_000..604_800)
        .step_by(3)
        .find(|&age| {
            let mut state = snapshot(
                start + age,
                start,
                870,
                Season::Spring,
                WeatherMode::Rain,
                None,
            );
            state.visitor = Visitor::Frog;
            state.cat = CatAction::Idle;
            let garden = ecology::plots_with_care(
                start + age,
                start,
                size.0 as usize,
                size.1 as usize,
                Season::Spring,
                state.weather,
                &unlocks,
                &effects.plots,
            );
            let mut context = Context::new(&state, &garden);
            context.guest = Some(frog);
            context.companion = Some(duck);
            cast::action(frog, &context).is_some_and(|action| {
                action.partner == Some(duck)
                    && action.phase == cast::Phase::Work
                    && (60..=65).contains(&action.phase_progress)
            })
        })
        .expect("find a rain pond encounter");
    for index in 0..30 {
        let age = first + index;
        let now_ms = (start + first) * 1000 + index * 160;
        let mut state = snapshot(
            start + age,
            start,
            870,
            Season::Spring,
            WeatherMode::Rain,
            None,
        );
        state.visitor = Visitor::Frog;
        state.cat = CatAction::Idle;
        let actions = ManualActions {
            invited_role: Some(duck),
            visitor_invited_at: Some((start + first) - 3),
            ..Default::default()
        };
        detailed_frame(
            destination,
            &format!("observe-pair-{index:02}"),
            &mut scene,
            &state,
            now_ms,
            &actions,
            size,
            &unlocks,
            &projects,
            &mut effects,
        );
    }

    projects[1].kind = projects::Kind::Empty;
    let mut seen = [false; 12];
    let mut scene = Scene::new();
    for age in (0..22 * 86_400).step_by(240) {
        let local = (6 * 3_600 + age) % 86_400;
        let Some(site) = projects::visible_with_help(age, local, size, &projects, &effects.work)
            .find(|site| site.lot == 0)
        else {
            continue;
        };
        let frame = site.stage as usize * 2 + usize::from(site.stage_progress >= 50);
        if seen[frame] {
            continue;
        }
        seen[frame] = true;
        let mut state = snapshot(
            start + age,
            start,
            (local / 60) as u16,
            Season::Spring,
            WeatherMode::Clear,
            None,
        );
        // Show the same daylight for side-by-side inspection of the actual build.
        state.day_part = simulation::DayPart::Day;
        state.cat = CatAction::Idle;
        detailed_frame(
            destination,
            &format!("observe-build-{frame:02}"),
            &mut scene,
            &state,
            (start + age) * 1000,
            &ManualActions::default(),
            size,
            &unlocks,
            &projects,
            &mut effects,
        );
        if seen.into_iter().all(|seen| seen) {
            break;
        }
    }
    assert!(seen.into_iter().all(|seen| seen));
}

fn detailed_frame(
    destination: &std::path::Path,
    name: &str,
    scene: &mut world::Scene,
    state: &simulation::Snapshot,
    now_ms: u64,
    actions: &world::ManualActions,
    size: (i32, i32),
    unlocks: &[u64; 16],
    projects: &[projects::Project; projects::LOTS],
    effects: &mut effects::Effects,
) {
    let (logical, _, sprites) =
        scene.render_detailed(state, now_ms, actions, size, unlocks, projects, effects);
    let (width, height) = (size.0 * 3, size.1 * 3);
    let mut physical = vec![0; width as usize * height as usize * 4];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let source = (y / 3 * size.0 as usize + x / 3) * 4;
            let target = (y * width as usize + x) * 4;
            physical[target..target + 4].copy_from_slice(&logical[source..source + 4]);
        }
    }
    for sprite in sprites.iter().flatten() {
        renderer::composite_detail(
            &mut physical,
            width,
            height,
            renderer::Damage::full((width, height)),
            *sprite,
        );
    }
    std::fs::write(destination.join(format!("{name}.bgra")), physical).unwrap();
    let results = scene.take_results();
    world::apply_results(state, size, unlocks, projects, effects, results);
}
