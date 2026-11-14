use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};

use super::{
    HostConfig, MAX_COMPILED_BYTES, MAX_ENABLED_PACKS, MAX_FACILITY_TYPES, MAX_INSTALLED_PACKS,
    MAX_ITEM_KINDS, MAX_PLUGIN_ROOTS, MAX_PLUGIN_STATE_BYTES, PackId, PluginError,
    RESOURCE_DISK_LIMIT, TOTAL_DISK_LIMIT, ValidatedPack, inspect_pack,
};

const CANDIDATE: &str = ".candidate.catpack";
const LOCK: &str = ".store.lock";
const MAX_DIRECTORY_ENTRIES: usize = MAX_INSTALLED_PACKS * 4;

#[derive(Debug)]
pub struct InstalledPack {
    pub pack: ValidatedPack,
    pub enabled: bool,
}

#[derive(Debug)]
pub struct LoadIssue {
    pub file: PathBuf,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Usage {
    pub installed_packs: usize,
    pub enabled_packs: usize,
    pub plugin_roots: usize,
    pub item_kinds: usize,
    pub facility_types: usize,
    pub state_bytes: usize,
    pub compiled_bytes: usize,
    pub plugin_bytes: u64,
    pub resource_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug)]
pub struct PluginStore {
    root: PathBuf,
    host: HostConfig,
    installed: BTreeMap<PackId, InstalledPack>,
    issues: Vec<LoadIssue>,
    _lock: File,
}

impl PluginStore {
    pub fn open(root: impl AsRef<Path>, host: HostConfig) -> Result<Self, PluginError> {
        let root = root.as_ref();
        fs::create_dir_all(root)?;
        if !fs::symlink_metadata(root)?.is_dir() {
            return Err(PluginError::invalid("插件目录必须是普通目录"));
        }
        let root = root.canonicalize()?;
        let lock_path = root.join(LOCK);
        if fs::symlink_metadata(&lock_path).is_ok_and(|metadata| !metadata.is_file()) {
            return Err(PluginError::invalid("插件目录锁不是普通文件"));
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock.try_lock().map_err(|error| match error {
            fs::TryLockError::WouldBlock => {
                PluginError::Conflict("插件目录已由另一个程序实例使用".into())
            }
            fs::TryLockError::Error(error) => PluginError::Io(error),
        })?;
        remove_candidate(&root.join(CANDIDATE))?;
        let mut store = Self {
            root,
            host,
            installed: BTreeMap::new(),
            issues: Vec::new(),
            _lock: lock,
        };
        store.load_directory()?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn packages(&self) -> impl Iterator<Item = &InstalledPack> {
        self.installed.values()
    }

    pub fn package(&self, id: &PackId) -> Option<&InstalledPack> {
        self.installed.get(id)
    }

    pub fn enabled_packs(&self) -> impl Iterator<Item = &ValidatedPack> {
        self.packages()
            .filter(|record| record.enabled)
            .map(|record| &record.pack)
    }

    pub fn load_issues(&self) -> &[LoadIssue] {
        &self.issues
    }

    pub fn usage(&self) -> Result<Usage, PluginError> {
        let mut usage =
            self.usage_for(self.packages().map(|record| (&record.pack, record.enabled)));
        self.measure_disk(&mut usage)?;
        Ok(usage)
    }

    pub fn inspect(&self, source: &Path) -> Result<ValidatedPack, PluginError> {
        inspect_pack(source, &self.host)
    }

    pub fn install(&mut self, source: &Path) -> Result<PackId, PluginError> {
        let preview = self.inspect(source)?;
        self.install_or_update(&preview, false, false)
    }

    /// Install exactly the contents previously displayed in a package preview.
    pub fn install_inspected(&mut self, preview: &ValidatedPack) -> Result<PackId, PluginError> {
        self.install_or_update(preview, false, false)
    }

    /// Update after the host has released active jobs and asset references to the old version.
    pub fn update(&mut self, source: &Path, in_use: bool) -> Result<PackId, PluginError> {
        let preview = self.inspect(source)?;
        self.install_or_update(&preview, true, in_use)
    }

    /// Replace exactly the package previously checked against saved world references.
    pub fn update_inspected(&mut self, preview: &ValidatedPack) -> Result<PackId, PluginError> {
        self.install_or_update(preview, true, false)
    }

    /// Disabling stops new proposals. Active jobs may continue against the retained pack.
    pub fn set_enabled(&mut self, id: &PackId, enabled: bool) -> Result<(), PluginError> {
        let record = self
            .installed
            .get(id)
            .ok_or_else(|| PluginError::Missing(id.to_string()))?;
        let destination = self.root.join(pack_name(id, enabled));
        if record.enabled == enabled && record.pack.path() == destination {
            return Ok(());
        }
        let mut view = self.current_view();
        view.insert(id, (&record.pack, enabled));
        check_registry(&view)?;
        let mut usage = self.usage_for(view.values().copied());
        if enabled {
            self.measure_disk(&mut usage)?;
            check_usage(usage)?;
        }
        if record.pack.path() != destination {
            expect_absent(&destination)?;
            commit_file(record.pack.path(), &destination, false)?;
        }
        let record = self.installed.get_mut(id).expect("previously checked");
        record.enabled = enabled;
        record.pack.path = destination;
        self.issues.retain(|issue| issue.file != record.pack.path);
        Ok(())
    }

    /// Mark the pack disabled first so the host can retire its active content before retrying.
    pub fn uninstall(&mut self, id: &PackId, in_use: bool) -> Result<(), PluginError> {
        let record = self
            .installed
            .get(id)
            .ok_or_else(|| PluginError::Missing(id.to_string()))?;
        let dependent = self.packages().find(|record| {
            record
                .pack
                .manifest
                .dependencies
                .iter()
                .any(|dependency| dependency.id == *id)
        });
        if let Some(dependent) = dependent {
            return Err(PluginError::InUse(format!(
                "{} 依赖 {id}",
                dependent.pack.manifest.id
            )));
        }
        if record.enabled {
            self.set_enabled(id, false)?;
        }
        if in_use {
            return Err(PluginError::InUse(id.to_string()));
        }
        let record = self.installed.get(id).expect("previously checked");
        fs::remove_file(record.pack.path())?;
        self.issues.retain(|issue| issue.file != record.pack.path);
        self.installed.remove(id);
        Ok(())
    }

    fn install_or_update(
        &mut self,
        preview: &ValidatedPack,
        update: bool,
        in_use: bool,
    ) -> Result<PackId, PluginError> {
        let id = preview.manifest.id.clone();
        let current = self.installed.get(&id);
        let enabled = match (update, current) {
            (false, None) => true,
            (false, Some(_)) => return Err(PluginError::Conflict(format!("{id} 已安装"))),
            (true, None) => return Err(PluginError::Missing(id.to_string())),
            (true, Some(_)) if in_use => return Err(PluginError::InUse(id.to_string())),
            (true, Some(current)) => {
                if current.pack.manifest.version == preview.manifest.version {
                    return Err(PluginError::Conflict(format!(
                        "{id} 已是版本 {}",
                        preview.manifest.version
                    )));
                }
                if current.pack.manifest.state_schema != preview.manifest.state_schema {
                    return Err(PluginError::Conflict(format!("{id} 的存档架构需要迁移")));
                }
                current.enabled
            }
        };
        let mut view = self.current_view();
        view.insert(&id, (preview, enabled));
        check_registry(&view)?;
        check_dependencies(&view, &id, &mut BTreeSet::new(), &mut BTreeSet::new())?;
        let mut usage = self.usage_for(view.values().copied());
        self.measure_disk(&mut usage)?;
        usage.resource_bytes = usage.resource_bytes.saturating_add(preview.package_bytes);
        usage.total_bytes = usage.total_bytes.saturating_add(preview.package_bytes);
        check_usage(usage)?;
        let destination = current.map_or_else(
            || self.root.join(pack_name(&id, enabled)),
            |record| record.pack.path.clone(),
        );
        if !update {
            expect_absent(&destination)?;
            expect_absent(&self.root.join(pack_name(&id, false)))?;
        }
        let candidate = self.root.join(CANDIDATE);
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)?;
        let result =
            self.stage_and_commit(output, &candidate, &destination, preview, enabled, update);
        match result {
            Ok(pack) => {
                self.issues.retain(|issue| issue.file != pack.path);
                self.installed
                    .insert(id.clone(), InstalledPack { pack, enabled });
                Ok(id)
            }
            Err(error) => {
                if let Err(cleanup_error) = remove_candidate(&candidate) {
                    self.issues.push(LoadIssue {
                        file: candidate,
                        reason: cleanup_error.to_string(),
                    });
                }
                Err(error)
            }
        }
    }

    fn stage_and_commit(
        &self,
        mut output: File,
        candidate: &Path,
        destination: &Path,
        preview: &ValidatedPack,
        enabled: bool,
        update: bool,
    ) -> Result<ValidatedPack, PluginError> {
        let mut source = File::open(preview.path())?.take(preview.package_bytes);
        if io::copy(&mut source, &mut output)? != preview.package_bytes
            || source.into_inner().read(&mut [0])? != 0
        {
            return Err(PluginError::Conflict("源文件在安装时发生变化".into()));
        }
        output.sync_all()?;
        drop(output);
        let mut staged = self.inspect(candidate)?;
        if staged.digest != preview.digest {
            return Err(PluginError::Conflict("源文件在安装时发生变化".into()));
        }
        let mut view = self.current_view();
        view.insert(&staged.manifest.id, (&staged, enabled));
        check_registry(&view)?;
        check_dependencies(
            &view,
            &staged.manifest.id,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
        )?;
        let mut usage = self.usage_for(view.values().copied());
        self.measure_disk(&mut usage)?;
        check_usage(usage)?;
        if !update {
            expect_absent(destination)?;
        }
        commit_file(candidate, destination, update)?;
        staged.path = destination.to_path_buf();
        Ok(staged)
    }

    fn current_view(&self) -> BTreeMap<&PackId, (&ValidatedPack, bool)> {
        self.packages()
            .map(|record| (&record.pack.manifest.id, (&record.pack, record.enabled)))
            .collect()
    }

    fn usage_for<'a>(&self, packs: impl Iterator<Item = (&'a ValidatedPack, bool)>) -> Usage {
        let mut usage = Usage {
            item_kinds: self.host.core_item_kinds.max(self.host.core_items.len()),
            facility_types: self.host.core_facility_types,
            ..Usage::default()
        };
        for (pack, enabled) in packs {
            usage.installed_packs += 1;
            usage.compiled_bytes += pack.compiled_bytes;
            usage.state_bytes += pack.manifest.budget.state_bytes;
            usage.item_kinds = usage.item_kinds.saturating_add(pack.items.len());
            usage.facility_types = usage.facility_types.saturating_add(pack.facilities.len());
            if enabled {
                usage.enabled_packs += 1;
                usage.plugin_roots += pack.activities.len();
            }
        }
        usage
    }

    fn measure_disk(&self, usage: &mut Usage) -> Result<(), PluginError> {
        let mut plugin_bytes = 0u64;
        for (index, file) in fs::read_dir(&self.root)?.enumerate() {
            if index >= MAX_DIRECTORY_ENTRIES {
                return Err(PluginError::Capacity("插件目录条目数量"));
            }
            let metadata = fs::symlink_metadata(file?.path())?;
            if metadata.is_file() {
                plugin_bytes = plugin_bytes.saturating_add(metadata.len());
            }
        }
        usage.plugin_bytes = plugin_bytes;
        usage.resource_bytes = self.host.base_resource_bytes.saturating_add(plugin_bytes);
        usage.total_bytes = usage
            .resource_bytes
            .saturating_add(self.host.base_data_bytes);
        Ok(())
    }

    fn load_directory(&mut self) -> Result<(), PluginError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            if entries.len() >= MAX_DIRECTORY_ENTRIES {
                return Err(PluginError::Capacity("插件目录条目数量"));
            }
            entries.push(entry?.path());
        }
        entries.sort();
        for file in entries {
            let Some((id, enabled)) = file
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(parse_pack_name)
            else {
                continue;
            };
            let result = self.inspect(&file).and_then(|pack| {
                if pack.manifest.id != id || self.installed.contains_key(&id) {
                    return Err(PluginError::Conflict(format!(
                        "插件文件名与清单不符或重复：{id}"
                    )));
                }
                let mut usage = self.usage_for(
                    self.packages()
                        .map(|record| (&record.pack, false))
                        .chain(std::iter::once((&pack, false))),
                );
                self.measure_disk(&mut usage)?;
                check_catalogue_usage(usage)?;
                Ok(pack)
            });
            match result {
                Ok(pack) => {
                    self.installed.insert(id, InstalledPack { pack, enabled });
                }
                Err(error) => self.issues.push(LoadIssue {
                    file,
                    reason: error.to_string(),
                }),
            }
        }
        let invalid_dependencies: Vec<_> = {
            let view = self.current_view();
            view.keys()
                .filter_map(|&id| {
                    check_dependencies(&view, id, &mut BTreeSet::new(), &mut BTreeSet::new())
                        .err()
                        .map(|error| (id.clone(), error))
                })
                .collect()
        };
        for (id, error) in invalid_dependencies {
            let record = self.installed.get_mut(&id).expect("loaded record");
            record.enabled = false;
            self.issues.push(LoadIssue {
                file: record.pack.path.clone(),
                reason: error.to_string(),
            });
        }
        if let Err(error) = check_usage(self.usage()?) {
            let reason = error.to_string();
            for record in self.installed.values_mut().filter(|record| record.enabled) {
                record.enabled = false;
                self.issues.push(LoadIssue {
                    file: record.pack.path.clone(),
                    reason: reason.clone(),
                });
            }
        }
        Ok(())
    }
}

fn pack_name(id: &PackId, enabled: bool) -> String {
    let suffix = if enabled {
        ".catpack"
    } else {
        "~disabled.catpack"
    };
    format!("{id}{suffix}")
}

fn parse_pack_name(file: &str) -> Option<(PackId, bool)> {
    let name = file.strip_suffix(".catpack")?;
    if let Some(disabled) = name.strip_suffix("~disabled") {
        return Some((PackId::parse(disabled).ok()?, false));
    }
    Some((PackId::parse(name).ok()?, true))
}

fn expect_absent(path: &Path) -> Result<(), PluginError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(PluginError::Conflict(format!(
            "文件已存在：{}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(windows)]
fn commit_file(from: &Path, to: &Path, replace: bool) -> Result<(), PluginError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let from: Vec<_> = from.as_os_str().encode_wide().chain([0]).collect();
    let to: Vec<_> = to.as_os_str().encode_wide().chain([0]).collect();
    let flags = MOVEFILE_WRITE_THROUGH
        | if replace {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    // The paths are owned, null-terminated UTF-16 strings. Both files are in the same directory.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn commit_file(from: &Path, to: &Path, replace: bool) -> Result<(), PluginError> {
    if !replace {
        expect_absent(to)?;
    }
    fs::rename(from, to)?;
    Ok(())
}

fn remove_candidate(path: &Path) -> Result<(), PluginError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(fs::remove_file(path)?),
        Ok(_) => Err(PluginError::invalid("安装临时文件不是普通文件")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn check_catalogue_usage(usage: Usage) -> Result<(), PluginError> {
    for (amount, limit, name) in [
        (usage.installed_packs, MAX_INSTALLED_PACKS, "已安装包数"),
        (usage.item_kinds, MAX_ITEM_KINDS, "共享物品类型"),
        (usage.facility_types, MAX_FACILITY_TYPES, "共享设施类型"),
        (usage.state_bytes, MAX_PLUGIN_STATE_BYTES, "插件存档字节"),
        (
            usage.compiled_bytes,
            MAX_COMPILED_BYTES,
            "常驻规则和素材索引",
        ),
    ] {
        if amount > limit {
            return Err(PluginError::Capacity(name));
        }
    }
    Ok(())
}

fn check_usage(usage: Usage) -> Result<(), PluginError> {
    check_catalogue_usage(usage)?;
    if usage.resource_bytes > RESOURCE_DISK_LIMIT || usage.total_bytes > TOTAL_DISK_LIMIT {
        return Err(PluginError::Capacity("磁盘总量，包含候选安装包"));
    }
    if usage.enabled_packs > MAX_ENABLED_PACKS || usage.plugin_roots > MAX_PLUGIN_ROOTS {
        return Err(PluginError::Capacity("已启用包数或自主活动根数"));
    }
    Ok(())
}

fn check_registry(view: &BTreeMap<&PackId, (&ValidatedPack, bool)>) -> Result<(), PluginError> {
    let mut verified = BTreeSet::new();
    for (id, _) in view.iter().filter(|(_, (_, enabled))| *enabled) {
        check_dependencies(view, id, &mut BTreeSet::new(), &mut verified)?;
    }
    Ok(())
}

fn check_dependencies<'a>(
    view: &BTreeMap<&'a PackId, (&ValidatedPack, bool)>,
    id: &'a PackId,
    visiting: &mut BTreeSet<&'a PackId>,
    verified: &mut BTreeSet<&'a PackId>,
) -> Result<(), PluginError> {
    if verified.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(PluginError::Conflict(format!("依赖成环：{id}")));
    }
    let (pack, enabled) = view.get(id).expect("checked dependency entry");
    for dependency in &pack.manifest.dependencies {
        let Some((&target, &(target_pack, target_enabled))) = view.get_key_value(&dependency.id)
        else {
            return Err(PluginError::Missing(format!("{id} 需要 {}", dependency.id)));
        };
        if !dependency.version.includes(&target_pack.manifest.version) {
            return Err(PluginError::Missing(format!(
                "{id} 需要兼容版本的 {target}"
            )));
        }
        if *enabled && !target_enabled {
            return Err(PluginError::Missing(format!("{id} 需要启用 {target}")));
        }
        check_dependencies(view, target, visiting, verified)?;
    }
    visiting.remove(id);
    verified.insert(id);
    Ok(())
}
