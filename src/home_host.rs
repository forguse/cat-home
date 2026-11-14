//! Native host for the household, village, plugins and bounded atomic saves.
use cat_home::{
    living_view::{Camera, LivingView, VillageCamera},
    living_world::{DAY_MS, Item, LivingWorld, Point, YardWeather},
    map_data::TilePos,
    map_data::WILLOW_VILLAGE,
    plugin_scene::{SceneRuntime, SceneState},
    plugins::{self, ContentId, HostConfig, PluginStore, SceneOp},
    residents::{VILLAGE_OCCUPANCY, VillageLife},
    visitors::VisitorState,
};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

const MAX_SNAPSHOT: usize = 256 * 1024;
const V4_FILE: &str = "world-v4.json";
const COMPAT_FILE: &str = "world-compat-v3.json";
const LIVE_PUMP_BUDGET: Duration = Duration::from_millis(6);
const OFFLINE_BACKLOG_MS: u64 = 10 * 60_000;
const MAX_PUMP_SLICES: usize = 128;

#[derive(Serialize, Deserialize)]
struct SavedLegacy {
    schema: u16,
    world: LivingWorld,
    scenes: SceneState,
    camera: Camera,
    #[serde(default = "default_size")]
    size: (i32, i32),
    #[serde(default)]
    visitors: VisitorState,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct MigrationReceipt {
    source_schema: u16,
    source_layout: String,
    digest_algorithm: String,
    source_digest: String,
    mapping_revision: u16,
}
#[derive(Serialize, Deserialize)]
struct SavedVillage {
    schema: u16,
    layout_id: String,
    map_revision: u16,
    source_migration: Option<MigrationReceipt>,
    world: LivingWorld,
    scenes: SceneState,
    camera: Camera,
    size: (i32, i32),
    life: VillageLife,
    auto_camera: bool,
    #[serde(default)]
    companionship_ms: u64,
}
fn default_size() -> (i32, i32) {
    (480, 300)
}
fn error(text: impl Into<String>) -> io::Error {
    io::Error::other(text.into())
}
fn valid_camera(c: Camera) -> bool {
    c.x.is_finite() && c.y.is_finite() && c.follow.is_none_or(|v| v < 3)
}
fn valid_size((w, h): (i32, i32)) -> bool {
    (240..=3840).contains(&w) && (180..=2160).contains(&h)
}

pub struct HomeHost {
    pub world: LivingWorld,
    pub scenes: SceneRuntime,
    pub camera: Camera,
    pub view: LivingView,
    pub store: PluginStore,
    pub visitors: VisitorState,
    pub life: VillageLife,
    pub auto_camera: bool,
    pub size: (i32, i32),
    pub last_error: Option<String>,
    root: PathBuf,
    preview: bool,
    compatibility: bool,
    source_migration: Option<MigrationReceipt>,
    last_save: u64,
    camera_clock: Instant,
    pixel_origin: Option<cat_home::viewport::PixelOrigin>,
    active_clock: Instant,
    companionship_ms: u64,
    village_camera: VillageCamera,
}

pub fn host_config() -> HostConfig {
    let capabilities = [
        SceneOp::PrepareFacility,
        SceneOp::OrderItem,
        SceneOp::ReserveSlots,
        SceneOp::Invite,
        SceneOp::MoveTo,
        SceneOp::TakeProp,
        SceneOp::Animate,
        SceneOp::PassArc,
        SceneOp::ThrowArc,
        SceneOp::CatchOrGround,
        SceneOp::RotateRoles,
        SceneOp::RetrieveProp,
        SceneOp::ReturnProp,
        SceneOp::Rest,
    ]
    .into_iter()
    .filter_map(|o| o.capability().map(str::to_owned))
    .collect();
    HostConfig {
        base_resource_bytes: 32_000_000,
        base_data_bytes: MAX_SNAPSHOT as u64,
        core_item_kinds: 8,
        core_facility_types: 5,
        capabilities,
        core_items: ["core:grain", "core:wood", "core:stone", "core:rope"]
            .into_iter()
            .map(|s| ContentId::parse(s).unwrap())
            .collect(),
    }
}

impl HomeHost {
    pub fn open(root: PathBuf, now: u64, preview: bool) -> io::Result<Self> {
        Self::open_mode(root, now, preview, false)
    }
    /// Explicit old-household recovery, distinct from the older --legacy pet.
    /// Recovery progress writes its own file; v2/v3 originals remain intact.
    pub fn open_compat(root: PathBuf, now: u64, preview: bool) -> io::Result<Self> {
        Self::open_mode(root, now, preview, true)
    }
    fn open_mode(root: PathBuf, now: u64, preview: bool, compatibility: bool) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        let mut receipt = None;
        let mut companionship_ms = 0;
        let (mut world, scenes, camera, size, visitors, mut life, auto_camera) = if !preview
            && !compatibility
            && root.join(V4_FILE).try_exists()?
        {
            let saved: SavedVillage = read_snapshot(&root.join(V4_FILE))?;
            if saved.schema != 4
                || saved.layout_id != WILLOW_VILLAGE.layout_id
                || saved.map_revision != WILLOW_VILLAGE.map_revision
                || !saved.world.is_village()
                || !saved.world.validate()
                || !saved.life.validate_for_world(&saved.world)
                || !saved
                    .life
                    .companionship_valid_for_ms(saved.companionship_ms)
                || !valid_camera(saved.camera)
                || !valid_size(saved.size)
            {
                return Err(error(
                    "村庄存档版本或状态无效。原文件已保留；可用 --recover-v3 打开旧庭院恢复副本。",
                ));
            }
            let scenes =
                SceneRuntime::from_state(saved.scenes).map_err(|e| error(e.to_string()))?;
            if !scenes.validate_for_world(&saved.world)
                || saved
                    .life
                    .reserved_actors()
                    .iter()
                    .any(|actor| scenes.state().reserved_actors().contains(actor))
            {
                return Err(error("村庄活动或角色预约无效，原存档已保留。"));
            }
            receipt = saved.source_migration;
            companionship_ms = saved.companionship_ms;
            (
                saved.world,
                scenes,
                saved.camera,
                saved.size,
                VisitorState::default(),
                saved.life,
                saved.auto_camera,
            )
        } else if !preview && let Some((path, schema)) = find_legacy(&root, compatibility)? {
            let bytes = read_bytes(&path)?;
            let saved: SavedLegacy = serde_json::from_slice(&bytes)
                .map_err(|e| error(format!("旧庭院存档无法读取，原件已保留：{e}")))?;
            if saved.schema != schema
                || saved.world.is_village()
                || !saved.world.validate()
                || !saved.visitors.validate_for_world(&saved.world)
                || !valid_camera(saved.camera)
                || !valid_size(saved.size)
            {
                return Err(error("旧庭院存档无效，未迁移或覆盖。"));
            }
            if saved
                .visitors
                .meeting_partner()
                .is_some_and(|a| saved.scenes.reserved_actors().contains(&a))
            {
                return Err(error("旧存档存在角色预约冲突，原件已保留。"));
            }
            let mut scenes =
                SceneRuntime::from_state(saved.scenes).map_err(|e| error(e.to_string()))?;
            let camera = if schema == 2 {
                saved.camera.migrate_legacy_view()
            } else {
                saved.camera
            };
            if compatibility {
                (
                    saved.world,
                    scenes,
                    camera,
                    saved.size,
                    saved.visitors,
                    VillageLife::default(),
                    false,
                )
            } else {
                let mut old_world = saved.world;
                let life = VillageLife::migrate_from_legacy(&saved.visitors, &mut old_world);
                scenes.migrate_to_village(&mut old_world).map_err(|e| {
                    error(format!(
                        "活动迁移未完成，原件已保留；可用 --recover-v3：{e}"
                    ))
                })?;
                let world = old_world.migrate_to_village().map_err(|e| {
                    error(format!(
                        "庭院迁移未完成，原件已保留；可用 --recover-v3：{e}"
                    ))
                })?;
                if !world.validate()
                    || !life.validate_for_world(&world)
                    || !scenes.validate_for_world(&world)
                {
                    return Err(error("迁移候选未通过完整校验，原存档未改变。"));
                }
                let mut mapped = camera;
                if let Some(p) = world.legacy_camera_anchor(Point {
                    x: camera.x,
                    y: camera.y,
                }) {
                    (mapped.x, mapped.y) = (p.x, p.y);
                }
                mapped.clamp_in_world(&world, (saved.size.0 / 3, saved.size.1 / 3));
                receipt = Some(MigrationReceipt {
                    source_schema: schema,
                    source_layout: "household-v3".into(),
                    digest_algorithm: "sha3-256".into(),
                    source_digest: format!("{:x}", Sha3_256::digest(&bytes)),
                    mapping_revision: 1,
                });
                (
                    world,
                    scenes,
                    mapped,
                    saved.size,
                    VisitorState::default(),
                    life,
                    false,
                )
            }
        } else {
            let world = if compatibility {
                LivingWorld::new_wall_ms(now)
            } else {
                LivingWorld::new_village_wall_ms(now)
            };
            (
                world,
                SceneRuntime::default(),
                Camera::default(),
                default_size(),
                VisitorState::default(),
                VillageLife::default(),
                !compatibility,
            )
        };
        let store = PluginStore::open(
            root.join(if preview {
                "preview-plugins"
            } else {
                "plugins"
            }),
            host_config(),
        )
        .map_err(|e| error(e.to_string()))?;
        if world.is_village() {
            world.note_first_village_observation(now);
            world.initialize_town_calendar(now);
            life.update_companionship(companionship_ms);
        }
        let mut view = LivingView::new();
        view.enable_builtin_premium()
            .map_err(|e| error(e.to_string()))?;
        Ok(Self {
            world,
            scenes,
            camera,
            view,
            store,
            visitors,
            life,
            auto_camera,
            size,
            last_error: None,
            root,
            preview,
            compatibility,
            source_migration: receipt,
            last_save: now,
            camera_clock: Instant::now(),
            pixel_origin: None,
            active_clock: Instant::now(),
            companionship_ms,
            village_camera: VillageCamera::new(),
        })
    }

    pub fn tick(&mut self, now: u64) {
        // Real active time excludes sleep/resume gaps; offline world age uses its
        // own persisted wall anchor. Never credit a clock jump as companionship.
        let active = self.active_clock.elapsed();
        if active.as_secs() < 2 {
            let whole_ms = active.as_millis() as u64;
            self.companionship_ms = self.companionship_ms.saturating_add(whole_ms);
            self.active_clock += std::time::Duration::from_millis(whole_ms);
        } else {
            self.active_clock = Instant::now();
        }
        if self.world.is_village() {
            self.life.update_companionship(self.companionship_ms);
            self.world
                .record_companionship_clock(now, self.companionship_ms);
        }
        self.world.queue_wall_time(now);
        let deadline = Instant::now() + LIVE_PUMP_BUDGET;
        let mut occupied = [TilePos::new(0, 0); VILLAGE_OCCUPANCY];
        // A higher presentation rate must not also multiply the amount of
        // catch-up work. Ordinary live ticks consume only elapsed world time.
        let slices = if self.offline_backlog() { MAX_PUMP_SLICES } else { 1 };
        for _ in 0..slices {
            self.pump_world_slice(deadline, &mut occupied);
            if Instant::now() >= deadline
                || !(self.world.is_catching_up()
                    || self.world.navigation_pending()
                    || self.life.navigation_pending())
            {
                break;
            }
        }
        if now.saturating_sub(self.last_save) >= 30_000 {
            self.last_save = now;
            self.last_error = self.save().err().map(|e| e.to_string());
        }
    }

    pub fn offline_backlog(&self) -> bool {
        self.world.is_village()
            && self.world.pending_world_target_ms().saturating_sub(self.world.world_ms())
                >= OFFLINE_BACKLOG_MS
    }

    fn pump_world_slice(
        &mut self,
        deadline: Instant,
        occupied: &mut [TilePos; VILLAGE_OCCUPANCY],
    ) {
        if self.world.is_village() {
            let count = self.life.write_occupied_tiles(occupied);
            self.world.set_external_occupied(&occupied[..count]);
            self.world
                .set_social_harvest_plot(self.life.reserved_harvest_plot());
            self.life.apply_supply_requests(&mut self.world);
        }
        for _ in 0..512 {
            if !self.world.advance_pending_to(self.world.world_ms())
                || self.world.navigation_pending()
            {
                break;
            }
            if self.world.is_village() {
                self.life.advance(&mut self.world);
                self.world
                    .record_village_social(self.life.social_progress());
            } else {
                self.visitors.advance(&mut self.world);
            }
            if self.world.is_village() {
                let count = self.life.write_occupied_tiles(occupied);
                self.world.set_external_occupied(&occupied[..count]);
            }
            if self.world.is_village() && self.life.navigation_pending() {
                break;
            }
            self.scenes.advance(&mut self.world, &self.store);
            if self.world.is_village() {
                self.life.observe_basketball(
                    self.scenes.state().basketball_completed(),
                    self.scenes.basketball_active(),
                    &self.world,
                );
            }
            let target = self.world.pending_world_target_ms();
            if self.world.world_ms() >= target || Instant::now() >= deadline {
                break;
            }
            let people = if self.world.is_village() {
                self.life.next_update_ms(&self.world)
            } else {
                self.visitors.next_update_ms(&self.world)
            };
            let next = self
                .scenes
                .next_update_ms(&self.world)
                .unwrap_or(target)
                .min(people.unwrap_or(target))
                .min(self.world.next_update_ms())
                .min(target)
                .max(self.world.world_ms().saturating_add(1));
            if !self.world.advance_pending_to(next) || Instant::now() >= deadline {
                break;
            }
        }
    }

    /// The native renderer consumes two-texel source pixels at physical size.
    pub fn render_physical(&mut self, physical: (i32, i32)) -> Option<[i32; 4]> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.camera_clock);
        self.camera_clock = now;
        let logical = cat_home::viewport::logical_size(physical);
        if self.world.is_village() {
            self.view
                .set_companionship_rewards(self.life.companionship_received());
            self.view
                .set_village_public_corner(self.life.social_public_corner());
            self.view.set_resident_presence(self.life.residents());
            let trip = self.life.trip_visual(&self.world);
            self.view.set_village_household_payload([
                self.life.household_held(cat_home::living_world::ActorKind::Xiaohe),
                self.life.household_held(cat_home::living_world::ActorKind::Ayu),
            ]);
            if self.auto_camera {
                self.village_camera.update_physical(
                    &mut self.camera,
                    &self.world,
                    physical,
                    elapsed,
                    trip,
                );
            } else if self.camera.follow.is_some() {
                self.village_camera.update_follow_physical(
                    &mut self.camera,
                    &self.world,
                    physical,
                    elapsed,
                );
            } else {
                self.village_camera
                    .retain_awake_physical(&mut self.camera, &self.world, physical);
            }
            let commands = self.scenes.visuals(&self.store, &self.world);
            let map = self.world.map_extent();
            let origin = cat_home::viewport::PixelOrigin::new(
                (self.camera.x, self.camera.y),
                physical,
                (
                    i32::from(map.0) * cat_home::living_world::TILE_PX,
                    i32::from(map.1) * cat_home::living_world::TILE_PX,
                ),
            );
            self.pixel_origin = Some(origin);
            self.view.render_village_overscan(
                &self.world,
                origin,
                physical,
                &commands,
                Some(&self.store),
                &self.life.visuals(&self.world),
                &self.life.wildlife_visuals(&self.world),
                &self.life.stall_visuals(&self.world),
            )
        } else {
            self.pixel_origin = None;
            let mut viewport = self.camera;
            if viewport.follow.is_some() {
                viewport.update(&self.world, logical);
                self.camera = viewport;
            } else {
                viewport.fit_viewport(logical);
            }
            let commands = self.scenes.visuals(&self.store, &self.world);
            self.view.render_premium(
                &self.world,
                &viewport,
                physical,
                &commands,
                Some(&self.store),
                &self.visitors.visuals(&self.world),
            )
        }
    }

    pub fn pixel_origin(&self) -> Option<cat_home::viewport::PixelOrigin> {
        self.pixel_origin
    }

    pub fn visible_motion(&self) -> bool {
        self.world.is_village() && (self.village_camera.moving() || self.view.visible_motion())
    }
    /// Compatibility for simulation probes that use logical dimensions.
    pub fn render(&mut self, logical: (i32, i32)) -> Option<[i32; 4]> {
        self.render_physical((logical.0.saturating_mul(cat_home::viewport::WORLD_PIXEL), logical.1.saturating_mul(cat_home::viewport::WORLD_PIXEL)))
    }
    pub fn camera_origin(&self, size: (i32, i32)) -> (f32, f32) {
        let mut c = self.camera;
        if self.world.is_village() {
            c.clamp_in_world(&self.world, size);
        } else {
            c.fit_viewport(size);
        }
        (c.x, c.y)
    }
    pub fn pan_camera(&mut self, x: f32, y: f32, size: (i32, i32)) {
        self.auto_camera = false;
        self.camera.pan_to_in_world(x, y, &self.world, size);
        self.village_camera.reset();
    }
    pub fn camera_home(&mut self) {
        self.camera.set_follow(None);
        self.auto_camera = self.world.is_village();
        self.village_camera.reset();
        if !self.world.is_village() {
            self.camera = Camera::default();
        }
    }
    pub fn camera_auto(&mut self, enabled: bool) {
        self.auto_camera = enabled && self.world.is_village();
        self.camera.set_follow(None);
        self.village_camera.reset();
    }
    pub fn camera_follow(&mut self, actor: Option<u8>) {
        self.auto_camera = false;
        self.camera.set_follow(actor);
        self.village_camera.reset();
    }
    pub fn save(&self) -> io::Result<()> {
        self.save_with_size(self.size)
    }
    pub fn save_with_size(&self, size: (i32, i32)) -> io::Result<()> {
        if self.preview {
            return Ok(());
        }
        if !valid_size(size) || !valid_camera(self.camera) {
            return Err(error("窗口或镜头参数无效，保留原存档。"));
        }
        let people_valid = if self.world.is_village() {
            self.life.validate_for_world(&self.world)
                && self.life.companionship_valid_for_ms(self.companionship_ms)
                && !self
                    .life
                    .reserved_actors()
                    .iter()
                    .any(|actor| self.scenes.state().reserved_actors().contains(actor))
        } else {
            self.visitors.validate_for_world(&self.world)
                && !self
                    .visitors
                    .meeting_partner()
                    .is_some_and(|actor| self.scenes.state().reserved_actors().contains(&actor))
        };
        if !self.world.validate() || !self.scenes.validate_for_world(&self.world) || !people_valid {
            return Err(error("生活状态未通过保存检查，保留上一份存档。"));
        }
        let (file, data) = if self.world.is_village() {
            (
                V4_FILE,
                serde_json::to_vec(&SavedVillage {
                    schema: 4,
                    layout_id: WILLOW_VILLAGE.layout_id.into(),
                    map_revision: WILLOW_VILLAGE.map_revision,
                    source_migration: self.source_migration.clone(),
                    world: self.world.clone(),
                    scenes: self.scenes.state().clone(),
                    camera: self.camera,
                    size,
                    life: self.life.clone(),
                    auto_camera: self.auto_camera,
                    companionship_ms: self.companionship_ms,
                }),
            )
        } else {
            (
                if self.compatibility {
                    COMPAT_FILE
                } else {
                    "world-v3.json"
                },
                serde_json::to_vec(&SavedLegacy {
                    schema: 3,
                    world: self.world.clone(),
                    scenes: self.scenes.state().clone(),
                    camera: self.camera,
                    size,
                    visitors: self.visitors.clone(),
                }),
            )
        };
        let data = data.map_err(|e| error(e.to_string()))?;
        if data.len() > MAX_SNAPSHOT {
            return Err(error("存档超过容量上限，旧存档保留。"));
        }
        atomic_write(&self.root.join(file), &data)
    }
    pub fn status(&self) -> String {
        let min = self.world.minute_of_day();
        let day = self.world.world_ms() / DAY_MS + 1;
        let state = if self.world.is_catching_up() {
            " · 正在补算离线生活"
        } else {
            ""
        };
        format!(
            "第{day}天 · {:02}:{:02} · {}{state}",
            min / 60,
            min % 60,
            match self.world.weather() {
                YardWeather::Clear => "晴",
                YardWeather::Clouds => "阴",
                YardWeather::Rain => "雨",
                YardWeather::Snow => "雪",
            }
        )
    }
    pub fn notebook(&self) -> String {
        let mut s = format!(
            "{}\n白天48倍，夜间96倍；离线生活按相同规则补算。\n\n",
            self.status()
        );
        for a in self.world.actors() {
            s.push_str(&format!(
                "{}：{}（体力{}，饥饿{}）\n",
                a.kind.label(),
                a.activity.label(),
                a.stamina(),
                a.hunger()
            ));
        }
        s.push_str("\n家中库存：\n");
        for (item, name) in [
            (Item::Seeds, "种子"),
            (Item::Crops, "收获"),
            (Item::Food, "熟食"),
            (Item::Water, "水"),
            (Item::Wood, "木材"),
            (Item::Stone, "石料"),
            (Item::Rope, "绳索"),
            (Item::Fish, "鲜鱼"),
        ] {
            s.push_str(&format!("{name}：{}  ", self.world.stock().count(item)));
        }
        if self.world.is_village() {
            s.push_str(&format!(
                "\n\n城镇成长：已过 {} 个真实日 · {}",
                self.world.town_real_days(),
                self.world.town_era().label()
            ));
            if let Some(day) = self.world.town_next_construction_day() {
                if day > self.world.town_real_days() {
                    s.push_str(&format!(
                        "\n下一项工程满 {} 个真实日可开工；仍需备料和施工。",
                        day
                    ));
                } else {
                    s.push_str("\n下一项工程已到开放日期，按物资与工班安排推进。");
                }
            }
            s.push_str(&format!(
                "\n\n村庄出行 {} 次 · 真实交易 {} 次 · 日常事件 {} 次",
                self.life.trips(),
                self.life.trades(),
                self.life.events()
            ));
            let completed = |id: &str| {
                self.world.village_chapter_completed(id) || self.life.village_chapter_completed(id)
            };
            let chapters = &cat_home::chapter_catalog::CHAPTERS;
            s.push_str(&format!(
                "\n共同经历：{}/72",
                chapters.iter().filter(|c| completed(c.id)).count()
            ));
            let ready: Vec<_> = chapters
                .iter()
                .filter(|c| {
                    !completed(c.id)
                        && self.world.town_chapter_allows(c.id)
                        && c.earliest_day <= self.world.world_ms() / DAY_MS
                        && c.after.iter().all(|id| completed(id))
                })
                .take(3)
                .collect();
            if !ready.is_empty() {
                s.push_str("\n接下来可以做的事（按物资、季节和作息安排）：");
                for c in ready {
                    s.push_str(&format!("\n· {}", c.title));
                }
            }
            if let Some((_, label, _)) = self.life.last_event() {
                s.push_str(&format!("\n最近：{label}"));
            }
        }
        if let Some(reward) = cat_home::companionship::latest(self.life.companionship_received()) {
            s.push_str(&format!(
                "\n\n陪伴纪念：{}\n{}",
                reward.title, reward.description
            ));
        }
        s.push_str(&format!("\n\n建设 {}/24 阶段 · 粮食损耗 {}\n陪伴累计 {} 小时\n插件：{}\n\n左键拖动窗口；Shift 拖动镜头；右下角调整视野大小。\n右键可选择自动观察、跟随人物、回家及退出。\n缩小窗口保留地图，画面外的生活继续。",self.world.story().completed(),self.world.story().waste(),self.companionship_ms/3_600_000,self.scenes.status()));
        if let Some(e) = &self.last_error {
            s.push_str(&format!("\n\n需要处理：{e}"));
        }
        for issue in self.store.load_issues() {
            s.push_str(&format!("\n插件未加载：{}", issue.reason));
        }
        s
    }
    pub fn install(&mut self, path: &Path) -> Result<String, String> {
        let candidate = self.store.inspect(path).map_err(|e| e.to_string())?;
        let id = candidate.manifest.id.clone();
        if self.store.package(&id).is_some() {
            self.scenes
                .validate_update(&candidate)
                .map_err(|e| e.to_string())?;
            self.store
                .update_inspected(&candidate)
                .map_err(|e| e.to_string())?;
        } else {
            self.store
                .install_inspected(&candidate)
                .map_err(|e| e.to_string())?;
        }
        self.view.clear_plugin_cache();
        self.save().map_err(|e| e.to_string())?;
        Ok(id.to_string())
    }
    pub fn stop_plugin(&mut self, id: &plugins::PackId) -> Result<(), String> {
        self.store
            .set_enabled(id, false)
            .map_err(|e| e.to_string())?;
        self.scenes.advance(&mut self.world, &self.store);
        self.view.clear_plugin_cache();
        self.save().map_err(|e| e.to_string())
    }
}

fn find_legacy(root: &Path, compatibility: bool) -> io::Result<Option<(PathBuf, u16)>> {
    let mut names = Vec::new();
    if compatibility {
        names.push((COMPAT_FILE, 3));
    }
    names.extend([("world-v3.json", 3), ("world-v2.json", 2)]);
    for (name, schema) in names {
        let path = root.join(name);
        if path.try_exists()? {
            return Ok(Some((path, schema)));
        }
    }
    Ok(None)
}
fn read_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let len = fs::metadata(path)?.len();
    if len > MAX_SNAPSHOT as u64 {
        return Err(error("存档过大，原文件已保留。"));
    }
    let mut data = Vec::with_capacity(len as usize);
    File::open(path)?
        .take(MAX_SNAPSHOT as u64 + 1)
        .read_to_end(&mut data)?;
    if data.len() > MAX_SNAPSHOT {
        return Err(error("读取时存档超过上限，原文件已保留。"));
    }
    Ok(data)
}
fn read_snapshot<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<T> {
    serde_json::from_slice(&read_bytes(path)?)
        .map_err(|e| error(format!("存档无法读取，原文件已保留：{e}")))
}
fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("pending");
    {
        let mut f = File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    use std::os::windows::ffi::OsStrExt;
    let a: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
    let b: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        MoveFileExW(
            a.as_ptr(),
            b.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
