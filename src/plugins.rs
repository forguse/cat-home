//! Bounded, declarative activity content and local package lifecycle.

mod manifest;
mod package;
mod rules;
mod store;
#[cfg(test)]
mod tests;

pub use manifest::{
    Activity, ActorKind, ActorSelector, Body, Budget, ContentId, Decoration, Dependency, Facility,
    Frame, HostConfig, Item, LocalId, Manifest, MarketSource, Material, PackId, Page, Participants,
    PriorityBand, Repeat, Scene, SceneOp, SceneStep, Slot, Subscription, Version, VersionRange,
};
pub use package::{AssetIndex, ValidatedPack, inspect_pack};
pub use rules::{Chance, CompiledCondition, Fact, FactSource, FactValue, Season, Weather, Window};
pub use store::{InstalledPack, LoadIssue, PluginStore, Usage};

use std::{error::Error, fmt, io};

pub const MAX_ENABLED_PACKS: usize = 8;
pub const MAX_INSTALLED_PACKS: usize = 16;
pub const MAX_PLUGIN_ROOTS: usize = 32;
pub const MAX_COMPILED_BYTES: usize = 512 * 1024;
pub const MAX_ITEM_KINDS: usize = 64;
pub const MAX_FACILITY_TYPES: usize = 32;
pub const MAX_PLUGIN_STATE_BYTES: usize = 32 * 1024;
pub const MAX_ACTIVITY_DURATION_MS: u64 = 90 * 60 * 1000;
pub const RESOURCE_DISK_LIMIT: u64 = 190_000_000;
pub const TOTAL_DISK_LIMIT: u64 = 200_000_000;

#[derive(Debug)]
pub enum PluginError {
    Io(io::Error),
    Invalid(String),
    Capacity(&'static str),
    Conflict(String),
    Missing(String),
    InUse(String),
}

impl PluginError {
    fn invalid(reason: impl Into<String>) -> Self {
        Self::Invalid(reason.into())
    }
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "插件文件操作失败：{error}"),
            Self::Invalid(reason) => write!(f, "插件包无效：{reason}"),
            Self::Capacity(resource) => write!(f, "插件超过容量限制：{resource}"),
            Self::Conflict(reason) => write!(f, "插件冲突：{reason}"),
            Self::Missing(reason) => write!(f, "缺少插件或能力：{reason}"),
            Self::InUse(reason) => write!(f, "插件仍在使用：{reason}"),
        }
    }
}

impl Error for PluginError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for PluginError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
