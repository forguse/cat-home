use std::{collections::BTreeSet, fmt, str::FromStr};

pub use semver::Version;
use serde::{Deserialize, Serialize};

use super::{
    Chance, CompiledCondition, MAX_ACTIVITY_DURATION_MS, MAX_COMPILED_BYTES, MAX_FACILITY_TYPES,
    MAX_ITEM_KINDS, MAX_PLUGIN_ROOTS, PluginError, rules::Expression,
};

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct PackId(Box<str>);

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct LocalId(Box<str>);

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct ContentId(Box<str>);

impl PackId {
    pub fn parse(value: &str) -> Result<Self, PluginError> {
        if !valid_pack_id(value) {
            return Err(PluginError::invalid(format!("无效的包 ID {value:?}")));
        }
        Ok(Self(value.into()))
    }
}

fn valid_pack_id(value: &str) -> bool {
    value.len() <= 96
        && value.contains('.')
        && value.split('.').all(|part| {
            part.len() <= 32
                && part.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && part
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

impl LocalId {
    pub fn parse(value: &str) -> Result<Self, PluginError> {
        if !valid_local_id(value) {
            return Err(PluginError::invalid(format!("无效的内容 ID {value:?}")));
        }
        Ok(Self(value.into()))
    }
}

fn valid_local_id(value: &str) -> bool {
    value.len() <= 48
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

impl ContentId {
    pub fn parse(value: &str) -> Result<Self, PluginError> {
        let Some((namespace, local)) = value.split_once(':') else {
            return Err(PluginError::invalid(format!(
                "内容 ID 缺少命名空间 {value:?}"
            )));
        };
        if (namespace != "core" && !valid_pack_id(namespace)) || !valid_local_id(local) {
            return Err(PluginError::invalid(format!("无效的内容 ID {value:?}")));
        }
        Ok(Self(value.into()))
    }

    pub fn new(pack: &PackId, local: &LocalId) -> Self {
        Self(format!("{pack}:{local}").into_boxed_str())
    }

    pub fn namespace(&self) -> &str {
        self.0.split_once(':').expect("validated content ID").0
    }

    pub fn local(&self) -> &str {
        self.0.split_once(':').expect("validated content ID").1
    }
}

macro_rules! id_conversions {
    ($ty:ty) => {
        impl $ty {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $ty {
            type Err = PluginError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }

        impl TryFrom<String> for $ty {
            type Error = PluginError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(&value)
            }
        }

        impl From<$ty> for String {
            fn from(value: $ty) -> Self {
                value.0.into()
            }
        }
    };
}

id_conversions!(PackId);
id_conversions!(LocalId);
id_conversions!(ContentId);

/// v1 accepts an exact stable version or a half-open range like >=1.0.0 <2.0.0.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(try_from = "String")]
pub enum VersionRange {
    Exact(Version),
    Between { min: Version, before: Version },
}

impl VersionRange {
    pub fn includes(&self, version: &Version) -> bool {
        match self {
            Self::Exact(expected) => version.cmp_precedence(expected).is_eq(),
            Self::Between { min, before } => {
                version.cmp_precedence(min).is_ge() && version.cmp_precedence(before).is_lt()
            }
        }
    }
}

impl TryFrom<String> for VersionRange {
    type Error = PluginError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let fields: Vec<_> = value.split_ascii_whitespace().take(3).collect();
        match fields.as_slice() {
            [exact] => Ok(Self::Exact(parse_bound(
                exact.strip_prefix('=').unwrap_or(exact),
            )?)),
            [minimum, maximum] => {
                let Some(minimum) = minimum.strip_prefix(">=") else {
                    return Err(PluginError::invalid("版本区间下限必须使用 >="));
                };
                let Some(maximum) = maximum.strip_prefix('<') else {
                    return Err(PluginError::invalid("版本区间上限必须使用 <"));
                };
                let min = parse_bound(minimum)?;
                let before = parse_bound(maximum)?;
                if min >= before {
                    return Err(PluginError::invalid("版本区间为空"));
                }
                Ok(Self::Between { min, before })
            }
            _ => Err(PluginError::invalid("版本区间格式无效")),
        }
    }
}

fn parse_bound(value: &str) -> Result<Version, PluginError> {
    let version: Version = value
        .parse()
        .map_err(|error| PluginError::invalid(format!("无效的版本区间边界 {value:?}：{error}")))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(PluginError::invalid("版本区间边界必须使用稳定发布版本"));
    }
    Ok(version)
}

#[derive(Clone, Debug, Default)]
pub struct HostConfig {
    pub base_resource_bytes: u64,
    pub base_data_bytes: u64,
    pub core_item_kinds: usize,
    pub core_facility_types: usize,
    pub capabilities: BTreeSet<String>,
    pub core_items: BTreeSet<ContentId>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u16,
    pub id: PackId,
    pub version: Version,
    pub title: String,
    pub core_api: VersionRange,
    pub author: String,
    pub license: String,
    pub description: String,
    pub license_file: String,
    pub dependencies: Vec<Dependency>,
    pub requires_capabilities: Vec<String>,
    pub state_schema: u16,
    pub budget: Budget,
    pub assets: String,
    pub items: String,
    pub facilities: String,
    #[serde(rename = "activities")]
    pub(super) raw_activities: Vec<ActivityDeclaration>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub id: PackId,
    pub version: VersionRange,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub autonomous_roots: usize,
    pub new_item_kinds: usize,
    pub new_facility_types: usize,
    pub active_instances: usize,
    pub active_flying_props: usize,
    pub state_bytes: usize,
    pub compiled_bytes: usize,
    pub package_bytes: u64,
}

impl Manifest {
    pub(super) fn validate(
        &self,
        host: &HostConfig,
        package_bytes: u64,
    ) -> Result<(), PluginError> {
        if self.schema_version != 1 || self.state_schema != 1 {
            return Err(PluginError::invalid("不支持的清单或存档架构版本"));
        }
        if !self.core_api.includes(&Version::new(1, 0, 0)) {
            return Err(PluginError::Missing(
                "核心 API 1.0.0 不在包声明的范围内".into(),
            ));
        }
        if self.version.to_string().len() > 96 {
            return Err(PluginError::Capacity("包版本字符串长度"));
        }
        validate_label(&self.title, "包名称", 128)?;
        validate_label(&self.author, "作者", 128)?;
        validate_label(&self.license, "许可", 128)?;
        validate_label(&self.description, "说明", 1024)?;
        let mut dependencies = BTreeSet::new();
        if self.dependencies.len() > super::MAX_INSTALLED_PACKS {
            return Err(PluginError::Capacity("依赖数量"));
        }
        for dependency in &self.dependencies {
            if dependency.id == self.id || !dependencies.insert(&dependency.id) {
                return Err(PluginError::invalid(format!(
                    "包依赖重复或指向自身：{}",
                    dependency.id
                )));
            }
        }
        let mut capabilities = BTreeSet::new();
        if self.requires_capabilities.len() > 32 {
            return Err(PluginError::Capacity("能力数量"));
        }
        for capability in &self.requires_capabilities {
            if !host.capabilities.contains(capability) {
                return Err(PluginError::Missing(capability.clone()));
            }
            if !capabilities.insert(capability) {
                return Err(PluginError::invalid(format!("能力声明重复：{capability}")));
            }
        }
        let budget = self.budget;
        if budget.autonomous_roots != self.raw_activities.len()
            || budget.autonomous_roots > MAX_PLUGIN_ROOTS
            || budget.new_item_kinds > MAX_ITEM_KINDS
            || budget.new_facility_types > MAX_FACILITY_TYPES
            || !(1..=2).contains(&budget.active_instances)
            || budget.active_flying_props > 4
            || budget.state_bytes > 4096
            || budget.compiled_bytes > MAX_COMPILED_BYTES
            || budget.package_bytes < package_bytes
            || budget.package_bytes > super::RESOURCE_DISK_LIMIT
        {
            return Err(PluginError::Capacity("清单资源声明"));
        }
        Ok(())
    }
}

pub(super) fn validate_label(value: &str, name: &str, max_len: usize) -> Result<(), PluginError> {
    if value.trim().is_empty() || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(PluginError::invalid(format!(
            "{name}为空、过长或含控制字符"
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActivityDeclaration {
    pub id: LocalId,
    pub subscribe: Vec<Subscription>,
    pub participants: Participants,
    pub when: Expression,
    pub chance: Chance,
    pub cooldown_world_ms: u64,
    pub scene: String,
    pub max_rounds: u8,
    pub max_duration_world_ms: u64,
    pub priority_band: PriorityBand,
    pub allow_user_suggestion: bool,
}

#[derive(Clone, Debug)]
pub struct Activity {
    pub id: ContentId,
    pub subscribe: Vec<Subscription>,
    pub participants: Participants,
    pub when: CompiledCondition,
    pub chance: Chance,
    pub cooldown_world_ms: u64,
    pub scene: Scene,
    pub max_rounds: u8,
    pub max_duration_world_ms: u64,
    pub priority_band: PriorityBand,
    pub allow_user_suggestion: bool,
}

impl ActivityDeclaration {
    pub(super) fn validate(&self) -> Result<(), PluginError> {
        let distinct: BTreeSet<_> = self.subscribe.iter().collect();
        if distinct.is_empty() || distinct.len() != self.subscribe.len() {
            return Err(PluginError::invalid(format!(
                "活动 {} 的事件订阅缺失或重复",
                self.id
            )));
        }
        if !(1..=2).contains(&self.participants.count)
            || (self.participants.count == 2 && !self.participants.requires_acceptance)
            || !(1..=8).contains(&self.max_rounds)
            || self.cooldown_world_ms > 31 * 24 * 60 * 60 * 1000
            || !(1..=MAX_ACTIVITY_DURATION_MS).contains(&self.max_duration_world_ms)
        {
            return Err(PluginError::invalid(format!(
                "活动 {} 的参与人数、轮次或时限无效",
                self.id
            )));
        }
        self.chance.validate()?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
pub enum Subscription {
    #[serde(rename = "actor.available")]
    ActorAvailable,
    #[serde(rename = "schedule.leisure_window")]
    LeisureWindow,
    #[serde(rename = "weather.changed")]
    WeatherChanged,
    #[serde(rename = "facility.changed")]
    FacilityChanged,
    #[serde(rename = "cooldown.expired")]
    CooldownExpired,
    #[serde(rename = "wish.changed")]
    WishChanged,
    #[serde(rename = "stock.changed")]
    StockChanged,
    #[serde(rename = "project.completed")]
    ProjectCompleted,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Participants {
    pub count: u8,
    pub kind: ActorKind,
    pub requires_acceptance: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Human,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PriorityBand {
    Leisure,
    Preparation,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub start: LocalId,
    pub steps: Vec<SceneStep>,
}

impl Scene {
    pub fn step(&self, id: &LocalId) -> Option<&SceneStep> {
        self.steps.iter().find(|step| step.id == *id)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneStep {
    pub id: LocalId,
    pub op: SceneOp,
    pub actor: Option<ActorSelector>,
    pub slot: Option<Slot>,
    pub prop: Option<ContentId>,
    pub facility: Option<ContentId>,
    #[serde(default)]
    pub frames: Vec<LocalId>,
    #[serde(default)]
    pub alternate_frames: Vec<LocalId>,
    #[serde(default)]
    pub duration_world_ms: u64,
    #[serde(default)]
    pub bounce_height: u8,
    pub next: Option<LocalId>,
    pub on_failure: Option<LocalId>,
    pub repeat: Option<Repeat>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repeat {
    pub target: LocalId,
    /// Number of additional executions of the repeated span.
    pub count: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActorSelector {
    Initiator,
    Partner,
    Holder,
    Other,
    Both,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Origin,
    CourtA,
    CourtB,
    Target,
    PropLocation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SceneOp {
    PrepareFacility,
    OrderItem,
    ReserveSlots,
    Invite,
    MoveTo,
    TakeProp,
    Animate,
    PassArc,
    ThrowArc,
    CatchOrGround,
    RotateRoles,
    RetrieveProp,
    ReturnProp,
    Rest,
    End,
}

impl SceneOp {
    pub const fn capability(self) -> Option<&'static str> {
        Some(match self {
            Self::PrepareFacility => "facility.prepare.v1",
            Self::OrderItem => "market.order.v1",
            Self::ReserveSlots => "scene.reserve_slots.v1",
            Self::Invite => "scene.invite.v1",
            Self::MoveTo => "actor.move_to_slot.v1",
            Self::TakeProp | Self::RetrieveProp | Self::ReturnProp => "prop.acquire_return.v1",
            Self::Animate => "actor.animate.v1",
            Self::PassArc | Self::ThrowArc => "prop.arc_throw.v1",
            Self::CatchOrGround => "prop.catch_or_ground.v1",
            Self::RotateRoles => "scene.rotate_roles.v1",
            Self::Rest => "needs.activity_cost.v1",
            Self::End => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    Human,
    HumanFemale,
    HumanMale,
    Prop,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: LocalId,
    pub file: String,
    pub sha3_256: String,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub id: LocalId,
    pub page: LocalId,
    pub rect: [u16; 4],
    pub anchor: [u16; 2],
    pub hand_anchor: Option<[u16; 2]>,
    pub body: Body,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: LocalId,
    pub title: String,
    pub frame: LocalId,
    pub source: MarketSource,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MarketSource {
    MarketOrder { materials: Vec<Material> },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub item: ContentId,
    pub quantity: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facility {
    pub id: LocalId,
    pub title: String,
    pub frame: LocalId,
    pub width: u16,
    pub height: u16,
    pub materials: Vec<Material>,
    #[serde(default)]
    pub decorations: Vec<Decoration>,
    pub target_offset: Option<[i16; 2]>,
    pub target_radius: Option<u8>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decoration {
    pub frame: LocalId,
    pub offset: [i16; 2],
    pub depth_bias: i16,
}
