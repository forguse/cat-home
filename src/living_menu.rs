//! Native settings and plugin controls for the living world.

use super::*;
use cat_home::plugins::{Fact, PackId, ValidatedPack, VersionRange};
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
};
use windows_sys::Win32::UI::{
    Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
        OFN_PATHMUSTEXIST, OPENFILENAMEW,
    },
    Shell::ShellExecuteW,
};

const CAMERA_FIXED: usize = 300;
const CAMERA_HOME: usize = 301;
const CAMERA_FOLLOW: usize = 302;
const CAMERA_FOLLOW_LAST: usize = CAMERA_FOLLOW + 2;
const HELP: usize = 305;
const INSTALL: usize = 306;
const PLUGIN_USAGE: usize = 307;
const PLUGIN_FOLDER: usize = 308;
const CAMERA_AUTO: usize = 309;
const PLUGIN_BASE: usize = 320;
const PLUGIN_STRIDE: usize = 3;

struct MenuState {
    status: String,
    size: (i32, i32),
    follow: Option<u8>,
    auto_camera: bool,
    hidden: bool,
    packages: Vec<PackageMenu>,
}

struct PackageMenu {
    id: PackId,
    title: String,
    enabled: bool,
}

pub(super) fn show(hwnd: HWND) {
    show_native(hwnd, false);
}

pub(super) fn show_tray(hwnd: HWND) {
    show_native(hwnd, true);
}

fn read_app<T>(read: impl FnOnce(&mut App) -> Option<T>) -> Option<T> {
    let mut result = None;
    with_app(|app| result = read(app));
    result
}

fn show_native(hwnd: HWND, tray: bool) {
    let Some(point) = cursor() else { return };
    let Some(state) = read_app(|app| {
        let home = app.home.as_ref()?;
        Some(MenuState {
            status: home.status(),
            size: app.renderer.size(),
            follow: home.camera.follow,
            auto_camera: home.auto_camera,
            hidden: app.user_hidden,
            packages: home
                .store
                .packages()
                .map(|record| PackageMenu {
                    id: record.pack.manifest.id.clone(),
                    title: record.pack.manifest.title.clone(),
                    enabled: record.enabled,
                })
                .collect(),
        })
    }) else {
        return;
    };
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }

    append_label(menu, MF_STRING | MF_GRAYED, 0, &state.status);
    append_label(menu, MF_STRING, MENU_NOTEBOOK, "观察手记…");
    append_options(
        menu,
        "窗口大小",
        SIZES
            .iter()
            .enumerate()
            .map(|(i, &(size, name))| (MENU_SIZE + i, name, size == state.size)),
    );
    append_options(
        menu,
        "镜头",
        [
            (CAMERA_AUTO, "自动观察生活", state.auto_camera),
            (CAMERA_FIXED, "手动取景（保留人物）", !state.auto_camera && state.follow.is_none()),
            (CAMERA_FOLLOW, "跟随小禾", state.follow == Some(0)),
            (CAMERA_FOLLOW + 1, "跟随阿榆", state.follow == Some(1)),
            (CAMERA_FOLLOW + 2, "跟随橘子", state.follow == Some(2)),
            (CAMERA_HOME, "回到庭院", false),
        ],
    );
    append_plugins(menu, &state.packages);
    append_startup(menu);
    append_label(menu, MF_STRING, HELP, "操作帮助…");
    append_label(menu, MF_STRING, MENU_UNINSTALL_HELP, "卸载小屋的步骤…");
    separator(menu);
    if tray || state.hidden {
        append_label(menu, MF_STRING, MENU_SHOW, "显示小屋");
    }
    if !state.hidden {
        append_label(menu, MF_STRING, MENU_TOGGLE, "隐藏小屋");
    }
    append_label(menu, MF_STRING, MENU_QUIT, "退出猫咪小屋");

    let selected = pick_menu(hwnd, menu, point);
    match selected {
        MENU_NOTEBOOK => {
            if let Some(text) = read_app(|app| Some(app.home.as_ref()?.notebook())) {
                message(hwnd, "观察手记", &text, MB_ICONINFORMATION);
            }
        }
        id if (MENU_SIZE..MENU_SIZE + SIZES.len()).contains(&id) => {
            with_app(|app| {
                if app.change_size(SIZES[id - MENU_SIZE].0) {
                    app.save_position();
                    app.save_settings();
                    app.refresh(hwnd);
                }
            });
        }
        CAMERA_AUTO | CAMERA_FIXED | CAMERA_HOME | CAMERA_FOLLOW..=CAMERA_FOLLOW_LAST => {
            set_camera(hwnd, selected)
        }
        INSTALL => install(hwnd),
        PLUGIN_USAGE => show_usage(hwnd),
        PLUGIN_FOLDER => open_folder(hwnd),
        MENU_STARTUP => toggle_startup(hwnd),
        HELP => message(
            hwnd,
            "猫咪小屋 · 操作帮助",
            "左键拖动移动窗口；Shift 拖动平移镜头。\n拖动右下角改变可见范围，像素比例不变；缩小不会删除土地。\n镜头菜单可自动观察生活、跟随主角或固定视角。\n\n存档在同级 home-data-v2/world-v4.json，旧庭院原件保留。\n需要恢复旧庭院时使用 --recover-v3；更早桌宠版本使用 --legacy。\n退出入口在小屋右键和通知区域菜单。",
            MB_ICONINFORMATION,
        ),
        MENU_UNINSTALL_HELP => message(
            hwnd,
            "猫咪小屋 · 卸载步骤",
            "如已开启登录 Windows 时启动，请先在菜单取消勾选。\n选择退出猫咪小屋并等待托盘图标消失。\n备份需要保留的 home-data-v2，再用文件资源管理器手动删除小屋所在的安装文件夹。",
            MB_ICONINFORMATION,
        ),
        MENU_SHOW => reveal_home(hwnd),
        MENU_TOGGLE => toggle_home(hwnd),
        MENU_QUIT => unsafe {
            DestroyWindow(hwnd);
        },
        id if id >= PLUGIN_BASE => {
            let offset = id - PLUGIN_BASE;
            if let Some(package) = state.packages.get(offset / PLUGIN_STRIDE) {
                match offset % PLUGIN_STRIDE {
                    0 => toggle_plugin(hwnd, package),
                    1 => show_plugin(hwnd, &package.id),
                    _ => uninstall(hwnd, package),
                }
            }
        }
        _ => {}
    }
}

fn append_plugins(parent: HMENU, packages: &[PackageMenu]) {
    append_group(parent, "插件与内容", |menu| {
        append_label(menu, MF_STRING, INSTALL, "安装或更新本地插件…");
        append_label(menu, MF_STRING, PLUGIN_USAGE, "资源用量与活动状态…");
        append_label(menu, MF_STRING, PLUGIN_FOLDER, "打开插件目录");
        separator(menu);
        if packages.is_empty() {
            append_label(menu, MF_STRING | MF_GRAYED, 0, "尚无已安装插件");
        }
        for (i, package) in packages.iter().enumerate() {
            let base = PLUGIN_BASE + i * PLUGIN_STRIDE;
            // An ampersand in the package title is plain text, not a menu shortcut.
            append_group(menu, &package.title.replace('&', "&&"), |item| {
                let check = if package.enabled { MF_CHECKED } else { 0 };
                append_label(item, MF_STRING | check, base, "启用");
                append_label(item, MF_STRING, base + 1, "查看详情…");
                append_label(item, MF_STRING, base + 2, "卸载…");
            });
        }
    });
}

fn separator(menu: HMENU) {
    unsafe {
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn message(hwnd: HWND, title: &str, text: &str, flags: u32) {
    let title = wide(title);
    let text = wide(text);
    unsafe {
        MessageBoxW(hwnd, text.as_ptr(), title.as_ptr(), MB_OK | flags);
    }
}

fn confirm(hwnd: HWND, title: &str, text: &str) -> bool {
    let title = wide(title);
    let text = wide(text);
    unsafe {
        MessageBoxW(
            hwnd,
            text.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_DEFBUTTON2 | MB_ICONQUESTION,
        ) == IDYES
    }
}

fn report<T>(hwnd: HWND, title: &str, result: Option<Result<T, String>>) -> Option<T> {
    match result? {
        Ok(value) => Some(value),
        Err(error) => {
            message(hwnd, title, &error, MB_ICONWARNING);
            None
        }
    }
}

fn set_camera(hwnd: HWND, selected: usize) {
    let result = read_app(|app| {
        let home = app.home.as_mut()?;
        match selected {
            CAMERA_AUTO => home.camera_auto(true),
            CAMERA_HOME => home.camera_home(),
            CAMERA_FIXED => home.camera_follow(None),
            follow => home.camera_follow(Some((follow - CAMERA_FOLLOW) as u8)),
        }
        let result = home.save().map_err(|error| error.to_string());
        app.refresh(hwnd);
        Some(result)
    });
    report(hwnd, "无法保存镜头设置", result);
}

fn choose_pack(hwnd: HWND) -> Result<Option<PathBuf>, String> {
    let mut file = vec![0u16; 32_768];
    let filter = wide("猫咪小屋插件 (*.catpack)\0*.catpack\0");
    let title = wide("选择庭院活动插件");
    // The dialog borrows all buffers only for the duration of the call.
    let mut dialog: OPENFILENAMEW = unsafe { mem::zeroed() };
    dialog.lStructSize = mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = hwnd;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.nFilterIndex = 1;
    dialog.lpstrFile = file.as_mut_ptr();
    dialog.nMaxFile = file.len() as u32;
    dialog.lpstrTitle = title.as_ptr();
    dialog.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        let error = unsafe { CommDlgExtendedError() };
        return if error == 0 {
            Ok(None)
        } else {
            Err(format!("文件选择器错误 {error:#x}"))
        };
    }
    let length = file
        .iter()
        .position(|&value| value == 0)
        .unwrap_or(file.len());
    Ok(Some(OsString::from_wide(&file[..length]).into()))
}

fn pack_details(pack: &ValidatedPack) -> String {
    let manifest = &pack.manifest;
    let size = pack.package_bytes;
    let version = match &manifest.core_api {
        VersionRange::Exact(version) => version.to_string(),
        VersionRange::Between { min, before } => format!(">= {min}，< {before}"),
    };
    let contents = format!(
        "{} 项活动，{} 种物品，{} 类设施，{} 页素材",
        pack.activities.len(),
        pack.items.len(),
        pack.facilities.len(),
        pack.assets.pages.len()
    );
    let items = pack
        .items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<Vec<_>>()
        .join("、");
    let facilities = pack
        .facilities
        .iter()
        .map(|facility| facility.title.as_str())
        .collect::<Vec<_>>()
        .join("、");
    let mut details = format!(
        "{}  {}\n{}\n\n作者  {}\n许可  {}\n核心 API  {version}\n大小  {size} 字节\n{}\n\n{}",
        manifest.title,
        manifest.version,
        manifest.id,
        manifest.author,
        manifest.license,
        contents,
        manifest.description
    );
    if !items.is_empty() {
        details.push_str(&format!("\n物品  {items}"));
    }
    if !facilities.is_empty() {
        details.push_str(&format!("\n设施  {facilities}"));
    }
    details
}

fn install(hwnd: HWND) {
    let Some(Some(path)) = report(hwnd, "无法选择插件", Some(choose_pack(hwnd))) else {
        return;
    };
    let preview = read_app(|app| {
        let home = app.home.as_ref()?;
        Some(
            home.store
                .inspect(&path)
                .map(|pack| {
                    let updating = home.store.package(&pack.manifest.id).is_some();
                    (pack.manifest.title.clone(), pack_details(&pack), updating)
                })
                .map_err(|error| error.to_string()),
        )
    });
    let Some((name, details, updating)) = report(hwnd, "插件校验失败", preview) else {
        return;
    };
    let title = if updating {
        "更新插件"
    } else {
        "安装插件"
    };
    if !confirm(hwnd, title, &format!("{details}\n\n是否{title}？")) {
        return;
    }
    let result = read_app(|app| Some(app.home.as_mut()?.install(&path)));
    if report(hwnd, "插件安装或更新失败", result).is_some() {
        message(
            hwnd,
            title,
            &format!("已{title}「{name}」。"),
            MB_ICONINFORMATION,
        );
    }
}

fn toggle_plugin(hwnd: HWND, package: &PackageMenu) {
    let result = read_app(|app| {
        let home = app.home.as_mut()?;
        Some(if package.enabled {
            home.stop_plugin(&package.id)
        } else {
            home.store
                .set_enabled(&package.id, true)
                .map_err(|error| error.to_string())
                .and_then(|()| home.save().map_err(|error| error.to_string()))
        })
    });
    report(hwnd, "无法更改插件状态", result);
}

fn show_plugin(hwnd: HWND, id: &PackId) {
    if let Some(text) = read_app(|app| {
        let home = app.home.as_ref()?;
        let record = home.store.package(id)?;
        let mut text = format!(
            "{}\n\n状态  {}\n{}",
            pack_details(&record.pack),
            if record.enabled {
                "已启用"
            } else {
                "已停用"
            },
            home.scenes.status()
        );
        for activity in &record.pack.activities {
            let requirements = activity.when.facts().map(fact_label).collect::<Vec<_>>();
            let chance = activity.chance;
            let cooldown_hours = activity.cooldown_world_ms / 3_600_000;
            text.push_str(&format!(
                "\n\n{}\n检查 {}\n触发概率 {}/{}，冷却 {} 世界小时",
                activity.id.local(),
                requirements.join("、"),
                chance.numerator,
                chance.denominator,
                cooldown_hours,
            ));
        }
        Some(text)
    }) {
        message(hwnd, "插件详情", &text, MB_ICONINFORMATION);
    }
}

fn fact_label(fact: &Fact) -> &'static str {
    match fact {
        Fact::WorldHour | Fact::WorldMinute => "时段",
        Fact::Season => "季节",
        Fact::Weather | Fact::SurfaceDry => "天气或地面状态",
        Fact::AllAvailable => "角色是否有空",
        Fact::UrgentNeeds => "紧急需要",
        Fact::Willing => "角色意愿",
        Fact::EssentialWorkDue => "重要工作",
        Fact::FacilityUsable(_) => "设施可用性",
        Fact::ItemObtainable(_) => "物品可用性",
        Fact::CooldownReady => "活动冷却",
        Fact::LastSucceeded => "上次活动结果",
        Fact::HasWish => "家庭愿望",
    }
}

fn uninstall(hwnd: HWND, package: &PackageMenu) {
    if !confirm(
        hwnd,
        "卸载插件",
        &format!("是否卸载「{}」？", package.title),
    ) {
        return;
    }
    let id = &package.id;
    let result = read_app(|app| {
        let home = app.home.as_mut()?;
        Some((|| {
            if home.scenes.is_in_use(id) {
                home.stop_plugin(id)?;
                if home.scenes.active_pack() == Some(id) {
                    return Ok(UninstallOutcome::Finishing);
                }
                if home.scenes.is_in_use(id) {
                    return Ok(UninstallOutcome::Retained);
                }
            }
            home.store
                .uninstall(id, false)
                .map_err(|error| error.to_string())?;
            home.view.clear_plugin_cache();
            home.save().map_err(|error| error.to_string())?;
            Ok(UninstallOutcome::Removed)
        })())
    });
    if let Some(outcome) = report(hwnd, "无法卸载插件", result) {
        let text = match outcome {
            UninstallOutcome::Removed => "插件已卸载。",
            UninstallOutcome::Retained => {
                "庭院里仍有此插件提供的设施或物品。插件已停用，已有资产和插件文件已保留。"
            }
            UninstallOutcome::Finishing => {
                "插件已停用，居民正在收尾。活动结束后可以再次尝试卸载；庭院资产和插件文件已保留。"
            }
        };
        message(hwnd, "卸载插件", text, MB_ICONINFORMATION);
    }
}

enum UninstallOutcome {
    Removed,
    Retained,
    Finishing,
}

fn show_usage(hwnd: HWND) {
    let result = read_app(|app| {
        let home = app.home.as_ref()?;
        Some(home.store.usage().map(|usage| format!(
            "已安装 {}，已启用 {}\n启用活动 {}\n插件文件 {} 字节\n常驻定义和索引 {} 字节\n素材页缓存 {} 字节\n\n{}",
            usage.installed_packs, usage.enabled_packs, usage.plugin_roots,
            usage.plugin_bytes, usage.compiled_bytes, home.view.cache_bytes(), home.scenes.status()
        )).map_err(|error| error.to_string()))
    });
    if let Some(text) = report(hwnd, "无法读取资源用量", result) {
        message(hwnd, "插件与内容 · 资源用量", &text, MB_ICONINFORMATION);
    }
}

fn open_folder(hwnd: HWND) {
    let Some(folder) = read_app(|app| Some(app.home.as_ref()?.store.root().to_path_buf())) else {
        return;
    };
    let folder = folder
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            hwnd,
            w!("open"),
            folder.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result <= 32 {
        message(
            hwnd,
            "无法打开插件目录",
            &format!("Windows 错误 {result}"),
            MB_ICONWARNING,
        );
    }
}
