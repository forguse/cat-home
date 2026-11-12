#![windows_subsystem = "windows"]

// Native window and tray lifecycle adapted from Nekodachi's window.rs and
// tray.rs, Copyright (c) 2026 Yannick Herrero, MIT.
mod activities;
mod cast;
mod clock;
mod ecology;
mod fishing;
mod projects;
mod renderer;
mod simulation;
mod world;

use std::{
    cell::RefCell,
    fmt::Write as _,
    fs::{self, File},
    io::{self, Read, Write},
    mem,
    path::PathBuf,
    ptr,
    time::{Duration, Instant},
};

use clock::{Anchor, Clock, Speed, WorldTime};
use projects::{BLUEPRINTS, Kind as ProjectKind, LOTS, Project};
use renderer::{DEFAULT_HEIGHT, DEFAULT_WIDTH, Renderer, dimensions};
use simulation::{Season, Snapshot, Visitor, WeatherMode};
use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Input::KeyboardAndMouse::{GetAsyncKeyState, ReleaseCapture, SetCapture, VK_LBUTTON},
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
                Shell_NotifyIconW,
            },
            WindowsAndMessaging::*,
        },
    },
    core::w,
};
use world::ManualActions;

const CLASS: *const u16 = w!("CodexCatHomeWorld");
const SMOKE_CLASS: *const u16 = w!("CodexCatHomeSmoke");
const TITLE: *const u16 = w!("猫咪小屋");
const TRAY_MESSAGE: u32 = WM_APP + 1;
const RESTORE_MESSAGE: u32 = WM_APP + 2;
const UPDATE_TIMER: usize = 1;
const MAX_SIDECAR_BYTES: usize = 4096;
const MENU_TREAT: usize = 1;
const MENU_TOGGLE: usize = 2;
const MENU_QUIT: usize = 3;
const MENU_SIZE: usize = 10;
const MENU_WEATHER: usize = 20;
const MENU_SEASON: usize = 30;
const MENU_WATER: usize = 40;
const MENU_SNACK: usize = 41;
const MENU_KEEPER: usize = 42;
const MENU_TAP: usize = 43;
const MENU_NOTEBOOK: usize = 44;
const MENU_LANTERN: usize = 50;
const MENU_INVITE: usize = 60;
const MENU_SPEED: usize = 70;
const MENU_PROJECT: usize = 100;
const MENU_CAST_INVITE: usize = 140;
const SIZES: [((i32, i32), &str); 4] = [
    ((360, 225), "360 × 225"),
    ((480, 300), "480 × 300"),
    ((600, 375), "600 × 375"),
    ((720, 450), "720 × 450"),
];
const WEATHER: [(WeatherMode, &str, &str); 4] = [
    (WeatherMode::Auto, "auto", "自动变化"),
    (WeatherMode::Clear, "clear", "晴天"),
    (WeatherMode::Rain, "rain", "下雨"),
    (WeatherMode::Snow, "snow", "下雪"),
];
const SEASONS: [(Season, &str, &str); 3] = [
    (Season::Spring, "spring", "春天"),
    (Season::Autumn, "autumn", "秋天"),
    (Season::Winter, "winter", "冬天"),
];
const LANTERNS: [(Option<bool>, &str, &str); 3] = [
    (None, "auto", "自动照明"),
    (Some(true), "on", "点亮"),
    (Some(false), "off", "熄灭"),
];
const VISITORS: [(Visitor, &str, &str); 7] = [
    (Visitor::Sparrow, "sparrow", "小麻雀"),
    (Visitor::GreyCat, "grey-cat", "灰猫"),
    (Visitor::BlackCat, "black-cat", "黑猫"),
    (Visitor::Squirrel, "squirrel", "松鼠"),
    (Visitor::Hedgehog, "hedgehog", "小刺猬"),
    (Visitor::Rabbit, "rabbit", "兔子"),
    (Visitor::Frog, "frog", "青蛙"),
];
const SPEEDS: [(Speed, &str); 4] = [
    (Speed::One, "1 倍（夜间 2 倍）"),
    (Speed::Two, "2 倍（夜间 4 倍）"),
    (Speed::Four, "4 倍（夜间 8 倍）"),
    (Speed::Eight, "8 倍（夜间 16 倍）"),
];

struct Settings {
    width: i32,
    height: i32,
    season: Season,
    weather: WeatherMode,
    started: u64,
    clock: Clock,
    treat_at: Option<u64>,
    manual: ManualActions,
    plot_unlocks: [u64; 16],
    projects: [Project; LOTS],
}

impl Settings {
    fn load(now: u64, local_seconds: u64) -> Self {
        let mut settings = Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            season: Season::Spring,
            weather: WeatherMode::Auto,
            started: now,
            clock: Clock::load(now, local_seconds, None),
            treat_at: None,
            manual: ManualActions::default(),
            plot_unlocks: [0; 16],
            projects: [Project::default(); LOTS],
        };
        let Some(raw) = read_sidecar("cat-home-settings.txt") else {
            return settings;
        };
        let mut saved_height = None;
        let mut saved_started = None;
        let mut clock_anchor_real = 0;
        let mut clock_anchor_world = 0;
        let mut clock_anchor_local_seconds = u64::MAX;
        let mut clock_speed = 0;
        for (key, value) in raw.lines().filter_map(|line| line.split_once('=')) {
            let value = value.trim();
            match key.trim() {
                "width" => {
                    if let Ok(width) = value.parse() {
                        settings.width = width;
                    }
                }
                "height" => saved_height = value.parse().ok(),
                "season" => {
                    if let Some((season, _, _)) = SEASONS.iter().find(|(_, name, _)| *name == value)
                    {
                        settings.season = *season;
                    }
                }
                "weather" => {
                    if let Some((weather, _, _)) =
                        WEATHER.iter().find(|(_, name, _)| *name == value)
                    {
                        settings.weather = *weather;
                    }
                }
                "started" => saved_started = value.parse::<u64>().ok(),
                "clock_anchor_real" => clock_anchor_real = value.parse().unwrap_or_default(),
                "clock_anchor_world" => clock_anchor_world = value.parse().unwrap_or_default(),
                "clock_anchor_local_seconds" => {
                    clock_anchor_local_seconds = value.parse().unwrap_or(u64::MAX)
                }
                "clock_speed" => clock_speed = value.parse().unwrap_or_default(),
                "treat_at" => settings.treat_at = parse_timestamp(value, now),
                "water_bowl" => {
                    if let Ok(water) = value.parse() {
                        settings.manual.water_bowl = water;
                    }
                }
                "lantern" => {
                    settings.manual.lantern = LANTERNS
                        .iter()
                        .find(|(_, name, _)| *name == value)
                        .and_then(|&(lantern, _, _)| lantern)
                }
                "keeper_called_at" => {
                    settings.manual.keeper_called_at = parse_timestamp(value, now)
                }
                "visitor_snack_at" => {
                    settings.manual.visitor_snack_at = parse_timestamp(value, now)
                }
                "invited_visitor" => {
                    settings.manual.invited_visitor = VISITORS
                        .iter()
                        .find(|(_, name, _)| *name == value)
                        .map_or(Visitor::None, |&(visitor, _, _)| visitor)
                }
                "invited_role" => {
                    settings.manual.invited_role = value.parse().ok().and_then(cast_invite)
                }
                "visitor_invited_at" => {
                    settings.manual.visitor_invited_at = parse_timestamp(value, now)
                }
                "window_tapped_at" => {
                    settings.manual.window_tapped_at = parse_timestamp(value, now)
                }
                "plot_unlocks" => {
                    for (slot, encoded) in settings.plot_unlocks.iter_mut().zip(value.split(',')) {
                        *slot = encoded.trim().parse().unwrap_or_default();
                    }
                }
                "projects" => {
                    for (slot, encoded) in settings.projects.iter_mut().zip(value.split(',')) {
                        if let Some((style, age)) = encoded.trim().split_once(':') {
                            let kind = style.parse().ok().and_then(ProjectKind::from_style);
                            if let (Some(kind), Ok(start_age)) = (kind, age.parse()) {
                                *slot = Project { kind, start_age };
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        (settings.width, settings.height) = dimensions(
            settings.width,
            saved_height.unwrap_or_else(|| legacy_height(settings.width)),
        );
        let clock_anchor = Anchor::decode(
            clock_anchor_real,
            clock_anchor_world,
            clock_anchor_local_seconds,
            clock_speed,
        );
        settings.clock = Clock::load(now, local_seconds, clock_anchor);
        if let Some(started) =
            saved_started.filter(|&started| started > 0 && started <= settings.clock.time().seconds)
        {
            settings.started = started;
        }
        settings
    }

    fn save(&self) {
        let season = SEASONS
            .iter()
            .find(|(value, _, _)| *value == self.season)
            .map_or("spring", |(_, name, _)| *name);
        let weather = WEATHER
            .iter()
            .find(|(value, _, _)| *value == self.weather)
            .map_or("auto", |(_, name, _)| *name);
        let lantern = LANTERNS
            .iter()
            .find(|(value, _, _)| *value == self.manual.lantern)
            .map_or("auto", |(_, name, _)| *name);
        let visitor = VISITORS
            .iter()
            .find(|(value, _, _)| *value == self.manual.invited_visitor)
            .map_or("none", |(_, name, _)| *name);
        let invited_role = self.manual.invited_role.map_or(0, |role| role.0);
        let anchor = self.clock.anchor(world::unix_millis() / 1_000);
        let mut contents = format!(
            "width={}\nheight={}\nseason={season}\nweather={weather}\nstarted={}\n\
            clock_anchor_real={}\nclock_anchor_world={}\nclock_anchor_local_seconds={}\nclock_speed={}\n\
            treat_at={}\n\
            water_bowl={}\nlantern={lantern}\nkeeper_called_at={}\nvisitor_snack_at={}\n\
            invited_visitor={visitor}\ninvited_role={invited_role}\nvisitor_invited_at={}\nwindow_tapped_at={}\nplot_unlocks=",
            self.width,
            self.height,
            self.started,
            anchor.real,
            anchor.world.seconds,
            anchor.world.seconds_of_day,
            anchor.speed as u8,
            self.treat_at.unwrap_or(0),
            self.manual.water_bowl,
            self.manual.keeper_called_at.unwrap_or(0),
            self.manual.visitor_snack_at.unwrap_or(0),
            self.manual.visitor_invited_at.unwrap_or(0),
            self.manual.window_tapped_at.unwrap_or(0),
        );
        for (index, age) in self.plot_unlocks.iter().enumerate() {
            if index > 0 {
                contents.push(',');
            }
            let _ = write!(contents, "{age}");
        }
        contents.push_str("\nprojects=");
        for (index, project) in self.projects.iter().enumerate() {
            if index > 0 {
                contents.push(',');
            }
            let _ = write!(contents, "{}:{}", project.kind.style(), project.start_age);
        }
        contents.push('\n');
        write_sidecar("cat-home-settings.txt", contents);
    }
}

fn parse_timestamp(value: &str, now: u64) -> Option<u64> {
    value
        .parse::<u64>()
        .ok()
        .filter(|&time| time > 0 && time <= now)
}

fn cast_invite(index: u8) -> Option<cast::Role> {
    // The two residents stay home, and the seven familiar animals use their existing menu.
    matches!(index, 1..=9 | 18..=29).then_some(cast::Role(index))
}

enum YardAction {
    ToggleWater,
    LeaveSnack,
    CallKeeper,
    TapWindow,
    SetLantern(Option<bool>),
    Invite(Visitor),
    InviteRole(cast::Role),
}

#[derive(Clone, Copy)]
enum DragKind {
    Move,
    Resize,
}

struct Drag {
    kind: DragKind,
    pointer: POINT,
    origin: POINT,
    size: (i32, i32),
    moved: bool,
    last_resize: Option<Instant>,
}

struct App {
    renderer: Renderer,
    scene: world::Scene,
    tray: Tray,
    settings: Settings,
    position: POINT,
    timer_ms: u32,
    drag: Option<Drag>,
    user_hidden: bool,
    preview: bool,
    visible: bool,
}

thread_local! { static APP: RefCell<Option<App>> = const { RefCell::new(None) }; }

impl App {
    fn unlock_visible_plots(&mut self) {
        let (width, height) = self.renderer.logical_size();
        let visible = ecology::visible_sites(width as usize, height as usize);
        let age = self
            .settings
            .clock
            .time()
            .seconds
            .saturating_sub(self.settings.started)
            .saturating_add(1);
        for (index, slot) in self.settings.plot_unlocks.iter_mut().enumerate() {
            if *slot == 0 && visible & (1 << index) != 0 {
                *slot = age;
            }
        }
    }

    fn save_settings(&self) {
        if !self.preview {
            self.settings.save();
        }
    }

    fn save_position(&self) {
        if !self.preview {
            write_sidecar(
                "cat-home-position.txt",
                format!("{},{}", self.position.x, self.position.y),
            );
        }
    }

    fn snapshot(&self, wall_now: u64, time: WorldTime) -> Snapshot {
        let treat = self
            .settings
            .treat_at
            .filter(|&at| at <= wall_now && wall_now - at < 4)
            .map(|_| time.seconds);
        let base = simulation::snapshot(
            time.seconds,
            self.settings.started,
            time.minutes(),
            self.settings.season,
            self.settings.weather,
            treat,
        );
        world::observe(base, wall_now, &self.settings.manual)
    }

    fn world_time_label(&self, time: WorldTime) -> String {
        let minutes = time.minutes();
        let period = if time.is_night() { "夜间" } else { "日间" };
        let speed = self.settings.clock.speed().multiplier_at(time);
        format!(
            "小屋时间 {:02}:{:02}（{period} {speed} 倍）",
            minutes / 60,
            minutes % 60
        )
    }

    fn render(&mut self, hwnd: HWND) -> io::Result<u32> {
        let now = world::unix_millis();
        let snapshot = self.snapshot(now / 1_000, self.settings.clock.time());
        let (image, dirty) = self.scene.render_view(
            &snapshot,
            now,
            &self.settings.manual,
            self.renderer.logical_size(),
            &self.settings.plot_unlocks,
            &self.settings.projects,
        );
        self.renderer.draw(hwnd, self.position, image, dirty)?;
        Ok(if self.scene.in_transition() {
            180
        } else {
            world::redraw_ms(&snapshot)
        })
    }

    fn set_timer(&mut self, hwnd: HWND, interval: u32) {
        if interval == self.timer_ms {
            return;
        }
        if interval == 0 {
            unsafe { KillTimer(hwnd, UPDATE_TIMER) };
            self.timer_ms = 0;
        } else if unsafe { SetTimer(hwnd, UPDATE_TIMER, interval, None) } != 0 {
            self.timer_ms = interval;
        }
    }

    fn refresh(&mut self, hwnd: HWND) {
        let should_show =
            !self.user_hidden && (self.preview || self.drag.is_some() || desktop_foreground(hwnd));
        if !should_show {
            if self.visible {
                unsafe { ShowWindow(hwnd, SW_HIDE) };
                self.visible = false;
            }
            self.set_timer(hwnd, if self.user_hidden { 0 } else { 900 });
            return;
        }

        let needs_show = !self.visible || unsafe { IsWindowVisible(hwnd) } == 0;
        if needs_show {
            self.position = clamp_position(self.position, self.renderer.size());
        }
        let interval = match self.render(hwnd) {
            Ok(interval) => {
                if needs_show {
                    unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
                    self.visible = true;
                }
                interval
            }
            Err(_) => 900,
        };
        self.set_timer(hwnd, if self.drag.is_some() { 32 } else { interval });
    }

    fn tick(&mut self, hwnd: HWND) {
        if self.drag.is_some() {
            if unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } >= 0 {
                self.end_drag(hwnd);
            } else {
                self.apply_pointer(hwnd, false);
            }
        } else {
            self.refresh(hwnd);
        }
    }

    fn treat(&mut self, hwnd: HWND) {
        self.settings.treat_at = Some(world::unix_millis() / 1_000);
        self.save_settings();
        self.refresh(hwnd);
    }

    fn interact(&mut self, hwnd: HWND, action: YardAction) {
        let now = world::unix_millis() / 1_000;
        let manual = &mut self.settings.manual;
        match action {
            YardAction::ToggleWater => manual.water_bowl = !manual.water_bowl,
            YardAction::LeaveSnack => manual.visitor_snack_at = Some(now),
            YardAction::CallKeeper => manual.keeper_called_at = Some(now),
            YardAction::TapWindow => manual.window_tapped_at = Some(now),
            YardAction::SetLantern(lantern) => manual.lantern = lantern,
            YardAction::Invite(visitor) => {
                manual.invited_visitor = visitor;
                manual.invited_role = None;
                manual.visitor_invited_at = Some(now);
            }
            YardAction::InviteRole(role) => {
                manual.invited_visitor = Visitor::None;
                manual.invited_role = Some(role);
                manual.visitor_invited_at = Some(now);
            }
        }
        self.save_settings();
        self.refresh(hwnd);
    }

    fn start_project(&mut self, hwnd: HWND, kind: ProjectKind) {
        let Some(lot) =
            projects::first_available(self.renderer.logical_size(), &self.settings.projects)
        else {
            return;
        };
        let start_age = self
            .settings
            .clock
            .time()
            .seconds
            .saturating_sub(self.settings.started);
        self.settings.projects[lot] = Project { kind, start_age };
        self.save_settings();
        self.refresh(hwnd);
    }

    fn in_resize_corner(&self, pointer: POINT) -> bool {
        let (width, height) = self.renderer.size();
        let x = pointer.x.saturating_sub(self.position.x);
        let y = pointer.y.saturating_sub(self.position.y);
        (width - 48..width).contains(&x) && (height - 48..height).contains(&y)
    }

    fn start_drag(&mut self, hwnd: HWND) {
        if let Some(pointer) = cursor() {
            self.drag = Some(Drag {
                kind: if self.in_resize_corner(pointer) {
                    DragKind::Resize
                } else {
                    DragKind::Move
                },
                pointer,
                origin: self.position,
                size: self.renderer.size(),
                moved: false,
                last_resize: None,
            });
            unsafe { SetCapture(hwnd) };
            self.set_timer(hwnd, 32);
        }
    }

    fn change_size(&mut self, requested: (i32, i32)) -> bool {
        let (width, height) = fit_size_to_monitor(self.position, requested);
        if self.renderer.size() == (width, height) || self.renderer.resize(width, height).is_err() {
            return false;
        }
        (self.settings.width, self.settings.height) = self.renderer.size();
        self.position = clamp_position(self.position, self.renderer.size());
        true
    }

    fn apply_pointer(&mut self, hwnd: HWND, force: bool) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        let Some(pointer) = cursor() else {
            return;
        };
        let dx = pointer.x.saturating_sub(drag.pointer.x);
        let dy = pointer.y.saturating_sub(drag.pointer.y);
        if dx.unsigned_abs().max(dy.unsigned_abs()) >= 5 {
            drag.moved = true;
        }
        if !drag.moved {
            return;
        }
        match drag.kind {
            DragKind::Move => {
                let position = clamp_position(
                    POINT {
                        x: drag.origin.x.saturating_add(dx),
                        y: drag.origin.y.saturating_add(dy),
                    },
                    self.renderer.size(),
                );
                if (position.x, position.y) == (self.position.x, self.position.y) {
                    return;
                }
                self.position = position;
                unsafe {
                    SetWindowPos(
                        hwnd,
                        ptr::null_mut(),
                        self.position.x,
                        self.position.y,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
                    );
                }
            }
            DragKind::Resize => {
                if !force
                    && drag
                        .last_resize
                        .is_some_and(|last| last.elapsed() < Duration::from_millis(32))
                {
                    return;
                }
                drag.last_resize = Some(Instant::now());
                let available = monitor_area(drag.origin).map_or((i32::MAX, i32::MAX), |area| {
                    (
                        area.right.saturating_sub(drag.origin.x),
                        area.bottom.saturating_sub(drag.origin.y),
                    )
                });
                let requested = (
                    drag.size.0.saturating_add(dx).min(available.0),
                    drag.size.1.saturating_add(dy).min(available.1),
                );
                if self.change_size(requested) {
                    self.refresh(hwnd);
                }
            }
        }
    }

    fn end_drag(&mut self, hwnd: HWND) {
        self.apply_pointer(hwnd, true);
        if let Some(drag) = self.drag.take() {
            unsafe { ReleaseCapture() };
            if drag.moved {
                self.save_position();
                if matches!(drag.kind, DragKind::Resize) {
                    self.unlock_visible_plots();
                    self.save_settings();
                }
                self.refresh(hwnd);
            } else if matches!(drag.kind, DragKind::Move) {
                self.treat(hwnd);
            }
        }
    }
}

fn with_app(run: impl FnOnce(&mut App)) {
    APP.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            if let Some(app) = state.as_mut() {
                run(app);
            }
        }
    });
}

fn cursor() -> Option<POINT> {
    let mut point = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut point) } != 0).then_some(point)
}

fn desktop_foreground(hwnd: HWND) -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null()
        || foreground == hwnd
        || foreground == unsafe { GetShellWindow() }
        || unsafe { IsIconic(foreground) } != 0
    {
        return true;
    }
    let mut class = [0u16; 96];
    let len = unsafe { GetClassNameW(foreground, class.as_mut_ptr(), class.len() as i32) };
    if len <= 0 {
        return false;
    }
    let name = &class[..len as usize];
    ["Progman", "WorkerW", "SHELLDLL_DefView", "SysListView32"]
        .into_iter()
        .any(|known| name.iter().copied().eq(known.encode_utf16()))
        || name
            .get(.."PecoFence".len())
            .is_some_and(|start| start.iter().copied().eq("PecoFence".encode_utf16()))
}

fn monitor_area(point: POINT) -> Option<RECT> {
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut info: MONITORINFO = unsafe { mem::zeroed() };
    info.cbSize = mem::size_of::<MONITORINFO>() as u32;
    (unsafe { GetMonitorInfoW(monitor, &mut info) } != 0).then_some(info.rcWork)
}

fn default_position(width: i32) -> POINT {
    let mut work: RECT = unsafe { mem::zeroed() };
    if unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0) } == 0
    {
        return POINT { x: 24, y: 24 };
    }
    let desktop_width = work.right - work.left;
    let desktop_height = work.bottom - work.top;
    POINT {
        x: (work.right - width - desktop_width * 95 / 1_000).max(work.left),
        y: work.top + desktop_height * 296 / 1_000,
    }
}

fn clamp_position(point: POINT, (width, height): (i32, i32)) -> POINT {
    let center = POINT {
        x: point.x.saturating_add(width / 2),
        y: point.y.saturating_add(height / 2),
    };
    let Some(area) = monitor_area(center) else {
        return point;
    };
    POINT {
        x: point
            .x
            .clamp(area.left, (area.right - width).max(area.left)),
        y: point
            .y
            .clamp(area.top, (area.bottom - height).max(area.top)),
    }
}

fn fit_size_to_monitor(point: POINT, (width, height): (i32, i32)) -> (i32, i32) {
    let (width, height) = dimensions(width, height);
    match monitor_area(point) {
        Some(area) => dimensions(
            width.min(area.right - area.left),
            height.min(area.bottom - area.top),
        ),
        None => (width, height),
    }
}

struct Tray {
    hwnd: HWND,
}

impl Tray {
    fn data(hwnd: HWND) -> NOTIFYICONDATAW {
        let mut data: NOTIFYICONDATAW = unsafe { mem::zeroed() };
        data.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = 1;
        data
    }

    fn add(hwnd: HWND) -> Self {
        let tray = Self { hwnd };
        tray.register();
        tray
    }

    fn register(&self) {
        let mut data = Self::data(self.hwnd);
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = TRAY_MESSAGE;
        // LoadIconW returns a shared icon, which the app must not destroy.
        data.hIcon = unsafe { LoadIconW(ptr::null_mut(), IDI_APPLICATION) };
        for (output, unit) in data.szTip.iter_mut().zip("猫咪小屋".encode_utf16()) {
            *output = unit;
        }
        unsafe { Shell_NotifyIconW(NIM_ADD, &data) };
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe { Shell_NotifyIconW(NIM_DELETE, &Self::data(self.hwnd)) };
    }
}

fn append_label(menu: HMENU, flags: u32, id: usize, label: &str) -> bool {
    let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
    (unsafe { AppendMenuW(menu, flags, id, wide.as_ptr()) }) != 0
}

fn append_options<I>(parent: HMENU, label: &str, options: I)
where
    I: IntoIterator<Item = (usize, &'static str, bool)>,
{
    append_group(parent, label, |child| {
        for (id, caption, checked) in options {
            append_label(
                child,
                MF_STRING | if checked { MF_CHECKED } else { 0 },
                id,
                caption,
            );
        }
    });
}

fn append_group(parent: HMENU, label: &str, append_items: impl FnOnce(HMENU)) {
    let child = unsafe { CreatePopupMenu() };
    if child.is_null() {
        return;
    }
    append_items(child);
    if !append_label(parent, MF_POPUP, child as usize, label) {
        unsafe { DestroyMenu(child) };
    }
}

fn toggle_home(hwnd: HWND) {
    with_app(|app| {
        app.user_hidden = !app.user_hidden;
        app.refresh(hwnd);
    });
}

fn show_notebook(hwnd: HWND) {
    let mut text = None;
    with_app(|app| {
        let time = app.settings.clock.time();
        let now = world::unix_millis() / 1_000;
        let snapshot = app.snapshot(now, time);
        let mut notebook = format!(
            "{}\n{}",
            app.world_time_label(time),
            world::notebook(
                &snapshot,
                app.renderer.logical_size(),
                &app.settings.plot_unlocks,
                &app.settings.projects,
            ),
        );
        let manual = &app.settings.manual;
        if let Some(role) = manual.invited_role.filter(|_| {
            manual
                .visitor_invited_at
                .and_then(|at| now.checked_sub(at))
                .is_some_and(|elapsed| elapsed < 90)
        }) {
            let _ = write!(notebook, "\n\n邀请的访客 {}", role.label());
        }
        text = Some(notebook);
    });
    if let Some(text) = text {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            MessageBoxW(
                hwnd,
                wide.as_ptr(),
                w!("观察手记"),
                MB_OK | MB_ICONINFORMATION,
            )
        };
    }
}

fn show_menu(hwnd: HWND) {
    let Some(point) = cursor() else {
        return;
    };
    let mut menu_state = None;
    with_app(|app| {
        let time = app.settings.clock.time();
        let status = world::status_line(&app.snapshot(world::unix_millis() / 1_000, time));
        let project_lot =
            projects::first_available(app.renderer.logical_size(), &app.settings.projects);
        let project_count = app
            .settings
            .projects
            .iter()
            .filter(|project| project.kind != ProjectKind::Empty)
            .count();
        menu_state = Some((
            (app.settings.width, app.settings.height),
            app.settings.weather,
            app.settings.season,
            app.settings.clock.speed(),
            app.settings.manual.water_bowl,
            app.settings.manual.lantern,
            app.user_hidden,
            app.world_time_label(time),
            status,
            project_lot,
            project_count,
        ));
    });
    let Some((
        size,
        weather,
        season,
        speed,
        water_bowl,
        lantern,
        hidden,
        clock,
        status,
        project_lot,
        project_count,
    )) = menu_state
    else {
        return;
    };
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        append_label(menu, MF_STRING | MF_GRAYED, 0, &clock);
        append_label(menu, MF_STRING | MF_GRAYED, 0, &status);
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        AppendMenuW(menu, MF_STRING, MENU_TREAT, w!("喂点零食"));
        append_group(menu, "院子互动", |yard| {
            append_label(
                yard,
                MF_STRING | if water_bowl { MF_CHECKED } else { 0 },
                MENU_WATER,
                "摆上水碗",
            );
            append_label(yard, MF_STRING, MENU_SNACK, "给访客留点心");
            append_label(yard, MF_STRING, MENU_KEEPER, "呼唤管理员");
            append_label(yard, MF_STRING, MENU_TAP, "轻敲窗户");
            append_options(
                yard,
                "门廊灯",
                LANTERNS
                    .into_iter()
                    .enumerate()
                    .map(|(index, (value, _, text))| {
                        (MENU_LANTERN + index, text, value == lantern)
                    }),
            );
            append_options(
                yard,
                "邀请客人",
                VISITORS
                    .into_iter()
                    .enumerate()
                    .map(|(index, (_, _, text))| (MENU_INVITE + index, text, false)),
            );
            append_group(yard, "邀请其他角色", |cast_menu| {
                for (title, range) in [("人物", 1..=9), ("动物", 18..=29)] {
                    append_options(
                        cast_menu,
                        title,
                        range.filter_map(cast_invite).map(|role| {
                            (MENU_CAST_INVITE + usize::from(role.0), role.label(), false)
                        }),
                    );
                }
            });
        });
        append_group(menu, "扩建工程", |projects_menu| {
            let lot_status = match project_lot {
                Some(lot) => format!("第 {} 号地块可开工", lot + 1),
                None if project_count == LOTS => "8 个地块均已使用".to_owned(),
                None => "向右或向下拉大庭院可露出空地".to_owned(),
            };
            append_label(projects_menu, MF_STRING | MF_GRAYED, 0, &lot_status);
            AppendMenuW(projects_menu, MF_SEPARATOR, 0, ptr::null());
            for (index, kind) in BLUEPRINTS.into_iter().enumerate() {
                let flags = MF_STRING | if project_lot.is_none() { MF_GRAYED } else { 0 };
                let label = format!("{} · {} 个小屋日", kind.label(), kind.days());
                append_label(projects_menu, flags, MENU_PROJECT + index, &label);
            }
        });
        AppendMenuW(menu, MF_STRING, MENU_NOTEBOOK, w!("观察手记"));
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        append_options(
            menu,
            "小屋大小",
            SIZES
                .into_iter()
                .enumerate()
                .map(|(index, (value, text))| (MENU_SIZE + index, text, value == size)),
        );
        append_options(
            menu,
            "小屋天气",
            WEATHER
                .into_iter()
                .enumerate()
                .map(|(index, (value, _, text))| (MENU_WEATHER + index, text, value == weather)),
        );
        append_options(
            menu,
            "小屋季节",
            SEASONS
                .into_iter()
                .enumerate()
                .map(|(index, (value, _, text))| (MENU_SEASON + index, text, value == season)),
        );
        append_options(
            menu,
            "日间时间流速",
            SPEEDS
                .into_iter()
                .enumerate()
                .map(|(index, (value, text))| (MENU_SPEED + index, text, value == speed)),
        );
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        AppendMenuW(
            menu,
            MF_STRING,
            MENU_TOGGLE,
            if hidden {
                w!("显示小屋")
            } else {
                w!("隐藏小屋")
            },
        );
        AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("退出"));
        let previous_foreground = GetForegroundWindow();
        SetForegroundWindow(hwnd);
        let selected = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            ptr::null(),
        ) as usize;
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        if GetForegroundWindow() == hwnd
            && previous_foreground != hwnd
            && !previous_foreground.is_null()
        {
            SetForegroundWindow(previous_foreground);
        }
        match selected {
            MENU_TREAT => with_app(|app| app.treat(hwnd)),
            MENU_TOGGLE => toggle_home(hwnd),
            MENU_WATER => with_app(|app| app.interact(hwnd, YardAction::ToggleWater)),
            MENU_SNACK => with_app(|app| app.interact(hwnd, YardAction::LeaveSnack)),
            MENU_KEEPER => with_app(|app| app.interact(hwnd, YardAction::CallKeeper)),
            MENU_TAP => with_app(|app| app.interact(hwnd, YardAction::TapWindow)),
            MENU_NOTEBOOK => show_notebook(hwnd),
            MENU_QUIT => {
                DestroyWindow(hwnd);
            }
            id if (MENU_SIZE..MENU_SIZE + SIZES.len()).contains(&id) => with_app(|app| {
                if app.change_size(SIZES[id - MENU_SIZE].0) {
                    app.unlock_visible_plots();
                    app.save_settings();
                    app.save_position();
                    app.refresh(hwnd);
                }
            }),
            id if (MENU_WEATHER..MENU_WEATHER + WEATHER.len()).contains(&id) => with_app(|app| {
                app.settings.weather = WEATHER[id - MENU_WEATHER].0;
                app.save_settings();
                app.refresh(hwnd);
            }),
            id if (MENU_SEASON..MENU_SEASON + SEASONS.len()).contains(&id) => with_app(|app| {
                app.settings.season = SEASONS[id - MENU_SEASON].0;
                app.save_settings();
                app.refresh(hwnd);
            }),
            id if (MENU_SPEED..MENU_SPEED + SPEEDS.len()).contains(&id) => with_app(|app| {
                if app.settings.clock.set_speed(SPEEDS[id - MENU_SPEED].0) {
                    app.save_settings();
                    app.refresh(hwnd);
                }
            }),
            id if (MENU_LANTERN..MENU_LANTERN + LANTERNS.len()).contains(&id) => with_app(|app| {
                app.interact(hwnd, YardAction::SetLantern(LANTERNS[id - MENU_LANTERN].0));
            }),
            id if (MENU_INVITE..MENU_INVITE + VISITORS.len()).contains(&id) => with_app(|app| {
                app.interact(hwnd, YardAction::Invite(VISITORS[id - MENU_INVITE].0));
            }),
            id if (MENU_PROJECT..MENU_PROJECT + BLUEPRINTS.len()).contains(&id) => {
                with_app(|app| {
                    app.start_project(hwnd, BLUEPRINTS[id - MENU_PROJECT]);
                })
            }
            id if (MENU_CAST_INVITE..MENU_CAST_INVITE + 30).contains(&id) => {
                if let Some(role) = cast_invite((id - MENU_CAST_INVITE) as u8) {
                    with_app(|app| app.interact(hwnd, YardAction::InviteRole(role)));
                }
            }
            _ => {}
        }
    }
}

thread_local! { static TASKBAR_CREATED: u32 = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }; }

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_TIMER if wparam == UPDATE_TIMER => with_app(|app| app.tick(hwnd)),
        WM_LBUTTONDOWN => with_app(|app| app.start_drag(hwnd)),
        WM_MOUSEMOVE => with_app(|app| app.apply_pointer(hwnd, false)),
        WM_LBUTTONUP => with_app(|app| app.end_drag(hwnd)),
        WM_RBUTTONUP => show_menu(hwnd),
        WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
        WM_SETCURSOR => {
            let mut resize = false;
            with_app(|app| {
                resize = app
                    .drag
                    .as_ref()
                    .is_some_and(|drag| matches!(drag.kind, DragKind::Resize))
                    || cursor().is_some_and(|point| app.in_resize_corner(point));
            });
            if resize {
                unsafe { SetCursor(LoadCursorW(ptr::null_mut(), IDC_SIZENWSE)) };
                return 1;
            }
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => with_app(|app| {
            if !app.change_size(app.renderer.size()) {
                app.position = clamp_position(app.position, app.renderer.size());
            }
            app.refresh(hwnd);
        }),
        TRAY_MESSAGE => match lparam as u32 {
            WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
            WM_LBUTTONDBLCLK => toggle_home(hwnd),
            _ => {}
        },
        RESTORE_MESSAGE => with_app(|app| {
            app.user_hidden = false;
            app.refresh(hwnd);
        }),
        WM_DESTROY => unsafe {
            KillTimer(hwnd, UPDATE_TIMER);
            PostQuitMessage(0);
        },
        other if other != 0 && TASKBAR_CREATED.with(|id| other == *id) => {
            with_app(|app| app.tray.register())
        }
        _ => return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
    0
}

fn position_arg(value: &str) -> Option<POINT> {
    let (x, y) = value.trim().split_once(',')?;
    Some(POINT {
        x: x.parse().ok()?,
        y: y.parse().ok()?,
    })
}

fn legacy_height(width: i32) -> i32 {
    (i64::from(width) * 5 / 8) as i32
}

fn size_arg(value: &str) -> Option<(i32, i32)> {
    let value = value.trim();
    if let Some((width, height)) = value.split_once([',', 'x', 'X', '×']) {
        Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
    } else {
        let width = value.parse().ok()?;
        Some((width, legacy_height(width)))
    }
}

fn sidecar(name: &str) -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.with_file_name(name))
}

fn read_sidecar(name: &str) -> Option<String> {
    let mut contents = String::new();
    File::open(sidecar(name)?)
        .ok()?
        .take((MAX_SIDECAR_BYTES + 1) as u64)
        .read_to_string(&mut contents)
        .ok()?;
    (contents.len() <= MAX_SIDECAR_BYTES).then_some(contents)
}

fn write_sidecar(name: &str, contents: String) {
    let Some(path) = sidecar(name) else {
        return;
    };
    let temporary = path.with_extension("tmp");
    let stage = || -> io::Result<()> {
        let mut file = File::create(&temporary)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    };
    if stage().is_ok() {
        let _ = fs::rename(temporary, path);
    }
}

fn run() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut position = None;
    let mut requested_size = None;
    let mut preview = false;
    let mut window_class = CLASS;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--position" => position = args.next().as_deref().and_then(position_arg),
            "--size" => requested_size = args.next().as_deref().and_then(size_arg),
            "--preview" => preview = true,
            "--isolated-preview" => {
                preview = true;
                window_class = SMOKE_CLASS;
            }
            "--check-assets" => {
                return if world::check_assets() {
                    Ok(())
                } else {
                    Err(io::Error::other("invalid art"))
                };
            }
            _ => {}
        }
    }

    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let previous = FindWindowW(window_class, ptr::null());
        if !previous.is_null() {
            PostMessageW(previous, RESTORE_MESSAGE, 0, 0);
            return Ok(());
        }
        let instance = GetModuleHandleW(ptr::null());
        if instance.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut class: WNDCLASSW = mem::zeroed();
        class.hInstance = instance;
        class.lpszClassName = window_class;
        class.lpfnWndProc = Some(window_proc);
        class.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
        if RegisterClassW(&class) == 0 {
            return Err(io::Error::last_os_error());
        }

        let mut settings = Settings::load(world::unix_millis() / 1_000, world::local_seconds());
        if let Some((width, height)) = requested_size {
            (settings.width, settings.height) = dimensions(width, height);
        }
        let requested_position = position
            .or_else(|| position_arg(&read_sidecar("cat-home-position.txt")?))
            .unwrap_or_else(|| default_position(settings.width));
        let (width, height) =
            fit_size_to_monitor(requested_position, (settings.width, settings.height));
        let renderer = Renderer::new(width, height)?;
        let (width, height) = renderer.size();
        (settings.width, settings.height) = (width, height);
        let position = clamp_position(requested_position, (width, height));
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            window_class,
            TITLE,
            WS_POPUP,
            position.x,
            position.y,
            width,
            height,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null(),
        );
        if hwnd.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut app = App {
            renderer,
            scene: world::Scene::new(),
            tray: Tray::add(hwnd),
            settings,
            position,
            timer_ms: 0,
            drag: None,
            user_hidden: false,
            preview,
            visible: false,
        };
        app.unlock_visible_plots();
        app.save_settings();
        APP.with(|cell| {
            *cell.borrow_mut() = Some(app);
        });
        with_app(|app| app.refresh(hwnd));
        let mut message: MSG = mem::zeroed();
        let result = loop {
            match GetMessageW(&mut message, ptr::null_mut(), 0, 0) {
                -1 => break Err(io::Error::last_os_error()),
                0 => break Ok(()),
                _ => {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        };
        APP.with(|cell| {
            if let Some(app) = cell.borrow_mut().take() {
                app.save_settings();
            }
        });
        result
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
