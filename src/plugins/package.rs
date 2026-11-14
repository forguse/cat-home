use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use serde::{Deserialize, de::DeserializeOwned};
use sha3::{Digest, Sha3_256};
use zip::{CompressionMethod, ZipArchive, read::Config, unstable::read::ZipIterable};

use super::{
    Activity, ActorSelector, Body, CompiledCondition, ContentId, Facility, Fact, Frame, HostConfig,
    Item, LocalId, Manifest, MarketSource, Material, Page, PluginError, Scene, SceneOp,
    manifest::{ActivityDeclaration, validate_label},
};

pub(super) const MAX_PACK_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;
const MAX_JSON_BYTES: u64 = 128 * 1024;
const PAGE_WIDTH: u16 = 128;
const PAGE_BYTES: usize = 128 * 128 * 4;
const MAX_SCENE_STEPS: usize = 128;
const MAX_SCENE_EXPANDED_STEPS: usize = 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetIndex {
    pub pages: Vec<Page>,
    pub frames: Vec<Frame>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemIndex {
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FacilityIndex {
    facilities: Vec<Facility>,
}

#[derive(Debug)]
pub struct ValidatedPack {
    pub manifest: Manifest,
    pub assets: AssetIndex,
    pub items: Vec<Item>,
    pub facilities: Vec<Facility>,
    pub activities: Vec<Activity>,
    pub package_bytes: u64,
    pub compiled_bytes: usize,
    pub(super) path: PathBuf,
    pub(super) digest: [u8; 32],
    page_offsets: Vec<u64>,
}

impl ValidatedPack {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn item(&self, id: &ContentId) -> Option<&Item> {
        (id.namespace() == self.manifest.id.as_str())
            .then(|| {
                self.items
                    .iter()
                    .find(|item| item.id.as_str() == id.local())
            })
            .flatten()
    }

    pub fn facility(&self, id: &ContentId) -> Option<&Facility> {
        (id.namespace() == self.manifest.id.as_str())
            .then(|| {
                self.facilities
                    .iter()
                    .find(|facility| facility.id.as_str() == id.local())
            })
            .flatten()
    }

    pub fn frame(&self, id: &LocalId) -> Option<&Frame> {
        self.assets.frames.iter().find(|frame| frame.id == *id)
    }

    /// Read one validated page on demand. The host owns the shared page cache.
    pub fn read_page(&self, id: &LocalId) -> Result<Vec<u8>, PluginError> {
        let index = self
            .assets
            .pages
            .iter()
            .position(|page| page.id == *id)
            .ok_or_else(|| PluginError::Missing(id.to_string()))?;
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(self.page_offsets[index]))?;
        let mut bytes = vec![0; PAGE_BYTES];
        file.read_exact(&mut bytes)?;
        validate_page_hash(&self.assets.pages[index], &bytes)?;
        Ok(bytes)
    }
}

pub fn inspect_pack(path: &Path, host: &HostConfig) -> Result<ValidatedPack, PluginError> {
    let mut archive = PackageReader::open(path)?;
    let package_bytes = archive.file_bytes;
    let mut manifest: Manifest = archive.json("manifest.json")?;
    manifest.validate(host, package_bytes)?;
    let license_bytes = archive.read_once(&manifest.license_file, 64 * 1024)?;
    if std::str::from_utf8(&license_bytes).is_err() || license_bytes.is_empty() {
        return Err(PluginError::invalid("许可文件必须是非空 UTF-8 文本"));
    }
    let assets: AssetIndex = archive.json(&manifest.assets)?;
    let ItemIndex { items } = archive.json(&manifest.items)?;
    let FacilityIndex { facilities } = archive.json(&manifest.facilities)?;
    if items.len() != manifest.budget.new_item_kinds
        || facilities.len() != manifest.budget.new_facility_types
    {
        return Err(PluginError::invalid("物品或设施实际数量与清单声明不符"));
    }
    let page_offsets = validate_assets(&mut archive, &assets)?;
    let frames: BTreeMap<_, _> = assets
        .frames
        .iter()
        .map(|frame| (&frame.id, frame))
        .collect();
    validate_catalogue(&items, &facilities, &frames, host)?;

    let mut roots = BTreeSet::new();
    let mut activities = Vec::with_capacity(manifest.raw_activities.len());
    for declaration in std::mem::take(&mut manifest.raw_activities) {
        declaration.validate()?;
        if !roots.insert(declaration.id.clone()) {
            return Err(PluginError::invalid(format!(
                "活动 ID 重复：{}",
                declaration.id
            )));
        }
        let scene: Scene = archive.json(&declaration.scene)?;
        validate_scene(
            &manifest,
            &declaration,
            &scene,
            &frames,
            &items,
            &facilities,
        )?;
        let condition = declaration.when.compile()?;
        validate_condition(&manifest, &condition, &items, &facilities, host)?;
        activities.push(Activity {
            id: ContentId::new(&manifest.id, &declaration.id),
            subscribe: declaration.subscribe,
            participants: declaration.participants,
            when: condition,
            chance: declaration.chance,
            cooldown_world_ms: declaration.cooldown_world_ms,
            scene,
            max_rounds: declaration.max_rounds,
            max_duration_world_ms: declaration.max_duration_world_ms,
            priority_band: declaration.priority_band,
            allow_user_suggestion: declaration.allow_user_suggestion,
        });
    }
    if !archive.entries.is_empty() {
        return Err(PluginError::invalid("包包含未引用或不支持的文件"));
    }
    let digest = archive.digest()?;
    let mut pack = ValidatedPack {
        manifest,
        assets,
        items,
        facilities,
        activities,
        package_bytes,
        compiled_bytes: 0,
        path: path.to_path_buf(),
        digest,
        page_offsets,
    };
    pack.compiled_bytes = retained_bytes(&pack);
    if pack.compiled_bytes > pack.manifest.budget.compiled_bytes {
        return Err(PluginError::Capacity("包的常驻定义与索引"));
    }
    Ok(pack)
}

struct PackageReader {
    archive: ZipArchive<BufReader<File>>,
    entries: BTreeMap<String, (usize, u64)>,
    file_bytes: u64,
}

impl PackageReader {
    fn open(path: &Path) -> Result<Self, PluginError> {
        if !path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("catpack"))
        {
            return Err(PluginError::invalid("文件扩展名必须为 .catpack"));
        }
        if !fs::symlink_metadata(path)?.is_file() {
            return Err(PluginError::invalid("包路径必须是普通文件"));
        }
        let file = open_for_validation(path)?;
        let file_bytes = file.metadata()?.len();
        if file_bytes > MAX_PACK_BYTES {
            return Err(PluginError::Capacity("单包文件大小"));
        }
        // Preflight the central directory before ZIP builds its in-memory file index.
        let directory =
            ZipIterable::try_new(BufReader::new(&file), Config::default()).map_err(zip_error)?;
        if directory.len() > MAX_ENTRIES {
            return Err(PluginError::Capacity("ZIP 条目数量"));
        }
        let mut archive = ZipArchive::new(BufReader::new(file)).map_err(zip_error)?;
        if archive.offset() != 0 {
            return Err(PluginError::invalid("ZIP 含有前置数据"));
        }
        archive.validate_header_consistency().map_err(zip_error)?;
        let mut entries = BTreeMap::new();
        let mut folded_names = BTreeSet::new();
        let mut expanded = 0u64;
        for index in 0..archive.len() {
            let entry = archive.by_index_raw(index).map_err(zip_error)?;
            let name = entry.name().map_err(zip_error)?;
            if name.as_bytes() != entry.name_raw()
                || !valid_entry_name(&name)
                || !folded_names.insert(name.to_ascii_lowercase())
            {
                return Err(PluginError::invalid(format!(
                    "ZIP 路径无效或重复：{name:?}"
                )));
            }
            if !entry.is_file()
                || entry
                    .unix_mode()
                    .is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o100000))
                || entry.encrypted()
                || entry.compression() != CompressionMethod::Stored
                || entry.size() != entry.compressed_size()
            {
                return Err(PluginError::invalid(format!(
                    "ZIP 条目类型、加密或压缩方法无效：{name:?}"
                )));
            }
            expanded = expanded
                .checked_add(entry.size())
                .filter(|&total| total <= file_bytes)
                .ok_or(PluginError::Capacity("ZIP 展开大小"))?;
            entries.insert(name.into_owned(), (index, entry.size()));
        }
        Ok(Self {
            archive,
            entries,
            file_bytes,
        })
    }

    fn json<T: DeserializeOwned>(&mut self, name: &str) -> Result<T, PluginError> {
        if !name.ends_with(".json") {
            return Err(PluginError::invalid(format!("JSON 引用扩展名无效：{name}")));
        }
        let bytes = self.read_once(name, MAX_JSON_BYTES)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| PluginError::invalid(format!("{name}：{error}")))
    }

    fn read_once(&mut self, name: &str, limit: u64) -> Result<Vec<u8>, PluginError> {
        self.read_entry(name, limit).map(|(bytes, _)| bytes)
    }

    fn read_entry(&mut self, name: &str, limit: u64) -> Result<(Vec<u8>, u64), PluginError> {
        let (index, size) = self
            .entries
            .remove(name)
            .ok_or_else(|| PluginError::invalid(format!("缺少文件或重复引用：{name:?}")))?;
        if size > limit {
            return Err(PluginError::Capacity("单个 ZIP 条目大小"));
        }
        let mut entry = self.archive.by_index(index).map_err(zip_error)?;
        let mut bytes = Vec::with_capacity(size as usize);
        (&mut entry).take(limit + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != size {
            return Err(PluginError::invalid(format!("ZIP 条目长度不符：{name}")));
        }
        let offset = entry
            .data_start()
            .ok_or_else(|| PluginError::invalid(format!("ZIP 条目没有有效数据偏移：{name}")))?;
        Ok((bytes, offset))
    }

    fn digest(self) -> Result<[u8; 32], PluginError> {
        let mut file = self.archive.into_inner();
        file.seek(SeekFrom::Start(0))?;
        let mut hash = Sha3_256::new();
        let count = std::io::copy(&mut file.take(self.file_bytes + 1), &mut hash)?;
        if count != self.file_bytes {
            return Err(PluginError::invalid("校验期间包文件长度发生变化"));
        }
        Ok(hash.finalize().into())
    }
}

fn zip_error(error: zip::result::ZipError) -> PluginError {
    PluginError::invalid(format!("ZIP：{error}"))
}

#[cfg(windows)]
fn open_for_validation(path: &Path) -> std::io::Result<File> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
}

#[cfg(not(windows))]
fn open_for_validation(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

fn valid_entry_name(name: &str) -> bool {
    name.len() <= 160
        && name.split('/').all(|part| {
            !part.is_empty()
                && part.as_bytes()[0].is_ascii_alphanumeric()
                && part
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        })
}

fn validate_assets(
    reader: &mut PackageReader,
    assets: &AssetIndex,
) -> Result<Vec<u64>, PluginError> {
    if assets.pages.len() > MAX_ENTRIES || assets.frames.len() > 512 {
        return Err(PluginError::Capacity("图页或帧数量"));
    }
    let mut pages = BTreeSet::new();
    let mut offsets = Vec::with_capacity(assets.pages.len());
    for page in &assets.pages {
        if !pages.insert(&page.id)
            || page.width != PAGE_WIDTH
            || page.height != PAGE_WIDTH
            || !page.file.starts_with("assets/")
            || !page.file.ends_with(".bgra")
        {
            return Err(PluginError::invalid(format!(
                "图页 ID、路径或尺寸无效：{}",
                page.id
            )));
        }
        let (bytes, offset) = reader.read_entry(&page.file, PAGE_BYTES as u64)?;
        if bytes.len() != PAGE_BYTES
            || bytes
                .chunks_exact(4)
                .any(|p| p[..3].iter().any(|&c| c > p[3]))
        {
            return Err(PluginError::invalid(format!(
                "图页字节数或预乘 BGRA 无效：{}",
                page.id
            )));
        }
        validate_page_hash(page, &bytes)?;
        offsets.push(offset);
    }
    let mut frames = BTreeSet::new();
    for frame in &assets.frames {
        let [x, y, width, height] = frame.rect;
        if !frames.insert(&frame.id)
            || !pages.contains(&frame.page)
            || width == 0
            || height == 0
            || x.saturating_add(width) > PAGE_WIDTH
            || y.saturating_add(height) > PAGE_WIDTH
            || frame.anchor[0] > width
            || frame.anchor[1] > height
            || frame
                .hand_anchor
                .is_some_and(|[x, y]| x > width || y > height)
        {
            return Err(PluginError::invalid(format!(
                "帧重复、越界或找不到图页：{}",
                frame.id
            )));
        }
    }
    Ok(offsets)
}

fn validate_page_hash(page: &Page, bytes: &[u8]) -> Result<(), PluginError> {
    if page.sha3_256 != format!("{:x}", Sha3_256::digest(bytes)) {
        return Err(PluginError::invalid(format!("图页校验值不符：{}", page.id)));
    }
    Ok(())
}

fn validate_catalogue(
    items: &[Item],
    facilities: &[Facility],
    frames: &BTreeMap<&LocalId, &Frame>,
    host: &HostConfig,
) -> Result<(), PluginError> {
    let mut ids = BTreeSet::new();
    for item in items {
        validate_label(&item.title, "物品名称", 128)?;
        if !ids.insert(&item.id)
            || frames
                .get(&item.frame)
                .is_none_or(|frame| frame.body != Body::Prop)
        {
            return Err(PluginError::invalid(format!(
                "物品 ID 重复或缺少道具素材：{}",
                item.id
            )));
        }
        let MarketSource::MarketOrder { materials } = &item.source;
        validate_materials(materials, host)?;
    }
    ids.clear();
    for facility in facilities {
        validate_label(&facility.title, "设施名称", 128)?;
        if !ids.insert(&facility.id)
            || !(1..=12).contains(&facility.width)
            || !(1..=8).contains(&facility.height)
            || frames
                .get(&facility.frame)
                .is_none_or(|frame| frame.body != Body::Prop)
            || facility.decorations.len() > 8
            || facility.target_offset.is_some() != facility.target_radius.is_some()
            || facility.target_offset.is_some_and(|[x, y]| {
                !(0..=i16::try_from(facility.width).unwrap_or(0) * 8).contains(&x)
                    || !(-64..=i16::try_from(facility.height).unwrap_or(0) * 8).contains(&y)
            })
            || facility
                .target_radius
                .is_some_and(|radius| !(1..=16).contains(&radius))
            || facility.decorations.iter().any(|decoration| {
                frames
                    .get(&decoration.frame)
                    .is_none_or(|frame| frame.body != Body::Prop)
                    || decoration
                        .offset
                        .into_iter()
                        .chain([decoration.depth_bias])
                        .any(|coordinate| !(-128..=128).contains(&coordinate))
            })
        {
            return Err(PluginError::invalid(format!(
                "设施 ID、尺寸或素材无效：{}",
                facility.id
            )));
        }
        validate_materials(&facility.materials, host)?;
    }
    Ok(())
}

fn validate_materials(materials: &[Material], host: &HostConfig) -> Result<(), PluginError> {
    let mut kinds = BTreeSet::new();
    if !(1..=8).contains(&materials.len()) {
        return Err(PluginError::invalid("配方需要 1 至 8 种材料"));
    }
    for material in materials {
        if !(1..=128).contains(&material.quantity)
            || !kinds.insert(&material.item)
            || !host.core_items.contains(&material.item)
        {
            return Err(PluginError::invalid(format!(
                "配方材料未知、数量无效或重复：{}",
                material.item
            )));
        }
    }
    Ok(())
}

fn validate_condition(
    manifest: &Manifest,
    condition: &CompiledCondition,
    items: &[Item],
    facilities: &[Facility],
    host: &HostConfig,
) -> Result<(), PluginError> {
    for fact in condition.facts() {
        let unresolved = match fact {
            Fact::FacilityUsable(id) if !own_facility(manifest, facilities, id) => Some(id),
            Fact::ItemObtainable(id)
                if !own_item(manifest, items, id) && !host.core_items.contains(id) =>
            {
                Some(id)
            }
            _ => None,
        };
        if let Some(id) = unresolved {
            return Err(PluginError::invalid(format!(
                "条件引用找不到物品或设施：{id}"
            )));
        }
    }
    Ok(())
}

fn own_item(manifest: &Manifest, items: &[Item], id: &ContentId) -> bool {
    id.namespace() == manifest.id.as_str()
        && items.iter().any(|item| item.id.as_str() == id.local())
}

fn own_facility(manifest: &Manifest, facilities: &[Facility], id: &ContentId) -> bool {
    id.namespace() == manifest.id.as_str()
        && facilities
            .iter()
            .any(|facility| facility.id.as_str() == id.local())
}

fn validate_scene(
    manifest: &Manifest,
    activity: &ActivityDeclaration,
    scene: &Scene,
    frames: &BTreeMap<&LocalId, &Frame>,
    items: &[Item],
    facilities: &[Facility],
) -> Result<(), PluginError> {
    if scene.steps.is_empty() || scene.steps.len() > MAX_SCENE_STEPS {
        return Err(PluginError::Capacity("场景步骤"));
    }
    let indexes: BTreeMap<_, _> = scene
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| (&step.id, i))
        .collect();
    if indexes.len() != scene.steps.len()
        || scene
            .steps
            .first()
            .is_none_or(|step| step.id != scene.start)
    {
        return Err(PluginError::invalid("场景起点须位于第一步且步骤 ID 须唯一"));
    }
    let mut reachable = vec![false; scene.steps.len()];
    reachable[0] = true;
    let mut repetitions = 0;
    let mut expanded_steps = scene.steps.len();
    for (index, step) in scene.steps.iter().enumerate() {
        use SceneOp::*;
        if !reachable[index] {
            return Err(PluginError::invalid(format!(
                "无法到达场景步骤：{}",
                step.id
            )));
        }
        if step.op.capability().is_some_and(|capability| {
            !manifest
                .requires_capabilities
                .iter()
                .any(|declared| declared == capability)
        }) {
            return Err(PluginError::Missing(format!(
                "{} 未声明能力 {:?}",
                step.id,
                step.op.capability()
            )));
        }
        if step.duration_world_ms > activity.max_duration_world_ms
            || (step.op == End) != step.next.is_none()
            || (step.op == End && (step.on_failure.is_some() || step.duration_world_ms != 0))
            || (step.op == MoveTo && (step.actor.is_none() || step.slot.is_none()))
            || (matches!(
                step.op,
                TakeProp
                    | OrderItem
                    | PassArc
                    | ThrowArc
                    | CatchOrGround
                    | RetrieveProp
                    | ReturnProp
            ) && step.prop.is_none())
            || (matches!(step.op, PrepareFacility | ReserveSlots) && step.facility.is_none())
            || (matches!(step.op, PassArc | ThrowArc) && manifest.budget.active_flying_props == 0)
            || (step.bounce_height != 0
                && (step.op != Animate
                    || step.prop.is_none()
                    || step.duration_world_ms < 1_000
                    || step.bounce_height > 24))
            || (activity.participants.count == 1
                && (matches!(
                    step.actor,
                    Some(ActorSelector::Partner | ActorSelector::Both | ActorSelector::Other)
                ) || matches!(step.op, Invite | PassArc | RotateRoles)))
            || (matches!(step.actor, Some(ActorSelector::Both))
                && matches!(
                    step.op,
                    TakeProp | PassArc | ThrowArc | CatchOrGround | RetrieveProp | ReturnProp
                ))
            || (step.op == PassArc
                && !matches!(step.slot, Some(super::Slot::CourtA | super::Slot::CourtB)))
        {
            return Err(PluginError::invalid(format!(
                "场景步骤参数无效：{}",
                step.id
            )));
        }
        if step
            .prop
            .as_ref()
            .is_some_and(|id| !own_item(manifest, items, id))
            || step
                .facility
                .as_ref()
                .is_some_and(|id| !own_facility(manifest, facilities, id))
        {
            return Err(PluginError::invalid(format!(
                "步骤引用找不到本包道具或设施：{}",
                step.id
            )));
        }
        if step.frames.len() > 32
            || (!step.alternate_frames.is_empty()
                && step.alternate_frames.len() != step.frames.len())
            || step.frames.iter().any(|id| {
                frames.get(id).is_none_or(|f| {
                    !(f.body == Body::Human
                        || f.body == Body::HumanFemale && !step.alternate_frames.is_empty())
                })
            })
            || step.alternate_frames.iter().any(|id| {
                frames
                    .get(id)
                    .is_none_or(|f| !matches!(f.body, Body::Human | Body::HumanMale))
            })
            || (step.op == Animate && step.frames.is_empty())
        {
            return Err(PluginError::invalid(format!(
                "步骤引用了缺失或不兼容的动画：{}",
                step.id
            )));
        }
        for target in [&step.next, &step.on_failure].into_iter().flatten() {
            let target = indexes
                .get(target)
                .copied()
                .ok_or_else(|| PluginError::invalid(format!("找不到场景转移目标：{target}")))?;
            if target <= index {
                return Err(PluginError::invalid("场景转移须向前，重复需明确声明"));
            }
            reachable[target] = true;
        }
        if let Some(repeat) = &step.repeat {
            let target = indexes.get(&repeat.target).copied().ok_or_else(|| {
                PluginError::invalid(format!("找不到场景重复目标：{}", repeat.target))
            })?;
            repetitions += 1;
            if repetitions > 1
                || step.op != RotateRoles
                || target >= index
                || repeat.count == 0
                || repeat.count >= activity.max_rounds
            {
                return Err(PluginError::invalid("重复须为单个有界的轮换回边"));
            }
            expanded_steps += (index - target + 1) * usize::from(repeat.count);
        }
    }
    if expanded_steps > MAX_SCENE_EXPANDED_STEPS {
        return Err(PluginError::Capacity("场景展开后的步骤数量"));
    }
    Ok(())
}

fn retained_bytes(pack: &ValidatedPack) -> usize {
    use std::mem::{size_of, size_of_val};

    // Conservative allowance for individual string allocations and the installed entry.
    let mut bytes = 1024 + size_of::<ValidatedPack>();
    let manifest = &pack.manifest;
    bytes += manifest.id.as_str().len()
        + manifest.title.capacity()
        + manifest.author.capacity()
        + manifest.license.capacity()
        + manifest.description.capacity()
        + manifest.license_file.capacity()
        + manifest.assets.capacity()
        + manifest.items.capacity()
        + manifest.facilities.capacity();
    bytes += manifest.dependencies.capacity() * size_of::<super::Dependency>()
        + manifest
            .dependencies
            .iter()
            .map(|dep| dep.id.as_str().len() + 32)
            .sum::<usize>();
    bytes += manifest.requires_capabilities.capacity() * size_of::<String>()
        + manifest
            .requires_capabilities
            .iter()
            .map(|s| s.capacity() + 32)
            .sum::<usize>();
    bytes += pack.assets.pages.capacity() * size_of::<Page>()
        + pack.page_offsets.capacity() * size_of::<u64>()
        + pack
            .assets
            .pages
            .iter()
            .map(|page| {
                page.id.as_str().len() + page.file.capacity() + page.sha3_256.capacity() + 96
            })
            .sum::<usize>();
    bytes += pack.assets.frames.capacity() * size_of::<Frame>()
        + pack
            .assets
            .frames
            .iter()
            .map(|f| f.id.as_str().len() + f.page.as_str().len() + 64)
            .sum::<usize>();
    let material_bytes = |materials: &Vec<Material>| {
        materials.capacity() * size_of::<Material>()
            + materials
                .iter()
                .map(|m| m.item.as_str().len() + 32)
                .sum::<usize>()
    };
    bytes += pack.items.capacity() * size_of::<Item>();
    for item in &pack.items {
        let MarketSource::MarketOrder { materials } = &item.source;
        bytes += item.id.as_str().len()
            + item.title.capacity()
            + item.frame.as_str().len()
            + material_bytes(materials)
            + 96;
    }
    bytes += pack.facilities.capacity() * size_of::<Facility>();
    for facility in &pack.facilities {
        bytes += facility.id.as_str().len()
            + facility.title.capacity()
            + facility.frame.as_str().len()
            + material_bytes(&facility.materials)
            + 96;
        bytes += facility.decorations.capacity() * size_of::<super::Decoration>()
            + facility
                .decorations
                .iter()
                .map(|layer| layer.frame.as_str().len() + 32)
                .sum::<usize>();
    }
    bytes += pack.activities.capacity() * size_of::<Activity>();
    for activity in &pack.activities {
        bytes += activity.id.as_str().len()
            + activity.scene.start.as_str().len()
            + size_of_val(activity.subscribe.as_slice())
            + activity.when.retained_bytes()
            + 256
            + activity.scene.steps.capacity() * size_of::<super::SceneStep>();
        for step in &activity.scene.steps {
            bytes += step.id.as_str().len() + 256;
            bytes += [&step.next, &step.on_failure]
                .into_iter()
                .flatten()
                .map(|id| id.as_str().len())
                .sum::<usize>();
            bytes += [&step.prop, &step.facility]
                .into_iter()
                .flatten()
                .map(|id| id.as_str().len())
                .sum::<usize>();
            if let Some(repeat) = &step.repeat {
                bytes += repeat.target.as_str().len();
            }
            for frames in [&step.frames, &step.alternate_frames] {
                bytes += frames.capacity() * size_of::<LocalId>()
                    + frames
                        .iter()
                        .map(|frame| frame.as_str().len() + 32)
                        .sum::<usize>();
            }
        }
    }
    bytes
}
