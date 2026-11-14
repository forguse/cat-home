use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use super::*;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

fn test_root() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "cat-home-plugin-tests-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
        ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn test_manifest(id: &str, version: &str) -> Value {
    json!({
        "schema_version": 1, "state_schema": 1, "id": id, "version": version,
        "title": "Test pack", "author": "Cat Home", "license": "MIT",
        "license_file": "LICENSE.txt", "description": "Plugin lifecycle fixture",
        "core_api": ">=1.0.0 <2.0.0", "dependencies": [], "requires_capabilities": [],
        "budget": { "autonomous_roots": 1, "new_item_kinds": 0, "new_facility_types": 0,
            "active_instances": 1, "active_flying_props": 0, "state_bytes": 64,
            "compiled_bytes": 16384, "package_bytes": 65536 },
        "assets": "assets/index.json", "items": "rules/items.json",
        "facilities": "rules/facilities.json",
        "activities": [{
            "id": "sample", "subscribe": ["actor.available"],
            "participants": {"count": 1, "kind": "human", "requires_acceptance": false},
            "when": {"fact": "world.minute", "in_range": [900, 1080]},
            "chance": {"numerator": 1, "denominator": 5, "window": "world_day_afternoon",
                "draw_at_first_eligible_opportunity_only": true},
            "cooldown_world_ms": 1000, "scene": "scenes/sample.json", "max_rounds": 1,
            "max_duration_world_ms": 60000, "priority_band": "leisure", "allow_user_suggestion": true
        }]
    })
}

fn fixture(path: &Path, manifest: Value, scene: Value, extra: Option<(&str, &[u8])>) {
    let output = File::create(path).unwrap();
    let mut zip = ZipWriter::new(output);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut entries = vec![
        ("manifest.json", serde_json::to_vec(&manifest).unwrap()),
        ("LICENSE.txt", b"MIT license".to_vec()),
        ("assets/index.json", br#"{"pages":[],"frames":[]}"#.to_vec()),
        ("rules/items.json", br#"{"items":[]}"#.to_vec()),
        ("rules/facilities.json", br#"{"facilities":[]}"#.to_vec()),
        ("scenes/sample.json", serde_json::to_vec(&scene).unwrap()),
    ];
    if let Some((name, bytes)) = extra {
        entries.push((name, bytes.to_vec()));
    }
    for (name, bytes) in entries {
        zip.start_file(name, options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
}

fn end_scene() -> Value {
    json!({"start": "done", "steps": [{"id": "done", "op": "end"}]})
}

#[test]
fn bundled_basketball_pack_passes_manifest_rules_scene_and_page_validation() {
    let host = HostConfig {
        capabilities: [
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
            SceneOp::Rest,
        ]
        .iter()
        .filter_map(|op| op.capability())
        .map(str::to_owned)
        .collect(),
        core_items: ["core:grain", "core:wood", "core:stone", "core:rope"]
            .iter()
            .map(|id| id.parse().unwrap())
            .collect(),
        ..HostConfig::default()
    };
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("content-packs/basketball/庭院篮球.catpack");
    let pack = inspect_pack(&path, &host).unwrap();
    assert_eq!(pack.activities.len(), 2);
    assert_eq!(pack.items.len(), 1);
    assert_eq!(pack.facilities.len(), 1);
    for page in &pack.assets.pages {
        assert_eq!(pack.read_page(&page.id).unwrap().len(), 65536);
    }
}

#[test]
fn predicates_are_typed_bounded_and_missing_facts_stay_unknown() {
    let predicate = CompiledCondition::from_json(
        br#"{"all":[
        {"fact":"world.minute","in_range":[900,1080]},
        {"not":{"fact":"participants.urgent_needs","eq":true}}
    ]}"#,
    )
    .unwrap();
    let facts = |fact: &Fact| match fact {
        Fact::WorldMinute => Some(FactValue::Number(900)),
        Fact::UrgentNeeds => Some(FactValue::Bool(false)),
        _ => None,
    };
    assert!(predicate.evaluate(&facts));
    assert!(!predicate.evaluate(&|fact: &Fact| (*fact == Fact::WorldMinute).then_some(FactValue::Number(900))));
    assert!(!predicate.evaluate(&|fact: &Fact| match fact {
        Fact::WorldMinute => Some(FactValue::Number(1080)),
        Fact::UrgentNeeds => Some(FactValue::Bool(false)),
        _ => None,
    }));
    for invalid in [
        json!({"fact":"world.minute","eq":true}),
        json!({"fact":"world.hour","in_range":[24,25]}),
        json!({"fact":"weather.surface_dry","ref":"core:grain","eq":true}),
        json!({"fact":"facility.usable","eq":true}),
        json!({"fact":"camera.visible","eq":true}),
        json!({"all":[]}),
        json!({"all":vec![json!({"fact":"weather.surface_dry","eq":true}); 128]}),
    ] {
        assert!(CompiledCondition::from_json(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    let mut too_deep = json!({"fact":"world.hour","eq":15});
    for _ in 0..16 {
        too_deep = json!({"not": too_deep});
    }
    assert!(matches!(
        CompiledCondition::from_json(&serde_json::to_vec(&too_deep).unwrap()),
        Err(PluginError::Capacity(_)),
    ));
}

#[test]
fn chance_replays_by_stable_content_and_window() {
    let a = "org.test.alpha:sample".parse().unwrap();
    let b = "org.test.beta:sample".parse().unwrap();
    let chance: Chance = serde_json::from_value(
        test_manifest("org.test.alpha", "1.0.0")["activities"][0]["chance"].clone(),
    )
    .unwrap();
    let rolls: Vec<_> = (0..64).map(|day| chance.allows(42, &a, day)).collect();
    assert!(rolls.iter().any(|&roll| roll));
    assert!(rolls.iter().any(|&roll| !roll));
    assert!((0..64).any(|day| chance.allows(42, &b, day) != rolls[day as usize]));
    assert!((0..64).all(|day| chance.allows(42, &a, day) == rolls[day as usize]));
}

#[test]
fn install_reopen_disable_uninstall_and_replace_keep_stable_identity() {
    let dir = test_root();
    let root = dir.join("installed");
    let first = dir.join("first.catpack");
    let second = dir.join("second.catpack");
    let id = "org.test.lifecycle";
    fixture(&first, test_manifest(id, "1.0.0"), end_scene(), None);
    fixture(&second, test_manifest(id, "1.1.0"), end_scene(), None);
    let host = HostConfig::default();
    let mut store = PluginStore::open(&root, host.clone()).unwrap();
    assert!(matches!(
        PluginStore::open(&root, host.clone()),
        Err(PluginError::Conflict(_))
    ));
    let id = store.install(&first).unwrap();
    assert!(matches!(
        store.install(&first),
        Err(PluginError::Conflict(_))
    ));
    let activity_id = store.enabled_packs().next().unwrap().activities[0]
        .id
        .clone();
    let installed_path = store.package(&id).unwrap().pack.path().to_owned();
    let original_bytes = fs::read(&installed_path).unwrap();
    assert!(matches!(
        store.update(&second, true),
        Err(PluginError::InUse(_))
    ));
    assert_eq!(fs::read(&installed_path).unwrap(), original_bytes);
    assert_eq!(store.update(&second, false).unwrap(), id);
    assert_eq!(
        store.package(&id).unwrap().pack.manifest.version,
        Version::new(1, 1, 0)
    );
    assert_eq!(
        store.enabled_packs().next().unwrap().activities[0].id,
        activity_id
    );
    assert!(matches!(
        store.uninstall(&id, true),
        Err(PluginError::InUse(_))
    ));
    assert_eq!(store.enabled_packs().count(), 0);
    drop(store);
    let mut store = PluginStore::open(&root, host).unwrap();
    assert!(!store.package(&id).unwrap().enabled);
    assert_eq!(
        store.package(&id).unwrap().pack.manifest.version,
        Version::new(1, 1, 0)
    );
    store.set_enabled(&id, true).unwrap();
    assert_eq!(store.enabled_packs().count(), 1);
    store.uninstall(&id, false).unwrap();
    assert_eq!(store.packages().count(), 0);
    assert!(first.is_file() && second.is_file());
}

#[test]
fn rejected_candidates_and_crash_residue_preserve_old_package() {
    let dir = test_root();
    let root = dir.join("installed");
    let first = dir.join("first.catpack");
    let bad = dir.join("bad.catpack");
    let large = dir.join("large.catpack");
    let name = "org.test.atomic";
    fixture(&first, test_manifest(name, "1.0.0"), end_scene(), None);
    fixture(
        &bad,
        test_manifest(name, "1.1.0"),
        end_scene(),
        Some(("../escape", b"unsafe")),
    );
    fixture(&large, test_manifest(name, "1.1.0"), end_scene(), None);
    let size = fs::metadata(&first).unwrap().len();
    let host = HostConfig {
        base_resource_bytes: RESOURCE_DISK_LIMIT - size,
        ..HostConfig::default()
    };
    let mut store = PluginStore::open(&root, host.clone()).unwrap();
    let id = store.install(&first).unwrap();
    let installed = store.package(&id).unwrap().pack.path().to_owned();
    let original_bytes = fs::read(&installed).unwrap();
    assert!(matches!(
        store.update(&bad, false),
        Err(PluginError::Invalid(_))
    ));
    assert!(matches!(
        store.update(&large, false),
        Err(PluginError::Capacity(_))
    ));
    assert_eq!(fs::read(&installed).unwrap(), original_bytes);
    drop(store);
    fs::write(root.join(".candidate.catpack"), b"interrupted").unwrap();
    let store = PluginStore::open(&root, host).unwrap();
    assert!(!root.join(".candidate.catpack").exists());
    assert_eq!(
        store.package(&id).unwrap().pack.manifest.version,
        Version::new(1, 0, 0)
    );
    assert_eq!(fs::read(&installed).unwrap(), original_bytes);
}

#[test]
fn bytes_changed_after_preview_are_never_committed() {
    let dir = test_root();
    let root = dir.join("installed");
    let file = dir.join("sample.catpack");
    let name = "org.test.preview";
    fixture(&file, test_manifest(name, "1.0.0"), end_scene(), None);
    let mut store = PluginStore::open(root, HostConfig::default()).unwrap();
    let preview = store.inspect(&file).unwrap();
    let mut modified = test_manifest(name, "1.0.0");
    modified["title"] = json!("Diff pack");
    fixture(&file, modified, end_scene(), None);
    assert_eq!(fs::metadata(&file).unwrap().len(), preview.package_bytes);
    assert!(matches!(
        store.install_inspected(&preview),
        Err(PluginError::Conflict(_))
    ));
    assert_eq!(store.packages().count(), 0);
    assert!(!store.root().join(".candidate.catpack").exists());
}

#[test]
fn dependencies_require_enabled_compatible_acyclic_packages() {
    let dir = test_root();
    let root = dir.join("installed");
    let parent = dir.join("parent.catpack");
    let child = dir.join("child.catpack");
    let cyclic_update = dir.join("update.catpack");
    let parent_id = "org.test.parent";
    let child_id = "org.test.child";
    let mut dependent = test_manifest(child_id, "1.0.0");
    dependent["dependencies"] = json!([{"id": parent_id, "version": ">=1.0.0 <2.0.0"}]);
    let mut cyclic = test_manifest(parent_id, "1.1.0");
    cyclic["dependencies"] = json!([{"id": child_id, "version": "1.0.0"}]);
    fixture(
        &parent,
        test_manifest(parent_id, "1.0.0"),
        end_scene(),
        None,
    );
    fixture(&child, dependent, end_scene(), None);
    fixture(&cyclic_update, cyclic, end_scene(), None);
    let mut store = PluginStore::open(root, HostConfig::default()).unwrap();
    assert!(matches!(
        store.install(&child),
        Err(PluginError::Missing(_))
    ));
    let parent_id = store.install(&parent).unwrap();
    store.set_enabled(&parent_id, false).unwrap();
    assert!(matches!(
        store.install(&child),
        Err(PluginError::Missing(_))
    ));
    store.set_enabled(&parent_id, true).unwrap();
    let child_id = store.install(&child).unwrap();
    assert!(matches!(
        store.set_enabled(&parent_id, false),
        Err(PluginError::Missing(_))
    ));
    assert!(matches!(
        store.uninstall(&parent_id, false),
        Err(PluginError::InUse(_))
    ));
    assert!(matches!(
        store.update(&cyclic_update, false),
        Err(PluginError::Conflict(_))
    ));
    assert_eq!(
        store.package(&parent_id).unwrap().pack.manifest.version,
        Version::new(1, 0, 0)
    );
    assert!(store.package(&child_id).unwrap().enabled);
}

#[test]
fn duplicate_roots_unsupported_capability_unbounded_cycle_and_invalid_zip_are_rejected() {
    let dir = test_root();
    let path = dir.join("bad.catpack");
    let host = HostConfig::default();
    let mut manifest = test_manifest("org.test.invalid", "1.0.0");
    manifest["activities"] = json!([
        manifest["activities"][0].clone(),
        manifest["activities"][0].clone()
    ]);
    manifest["budget"]["autonomous_roots"] = json!(2);
    fixture(&path, manifest, end_scene(), None);
    assert!(matches!(
        inspect_pack(&path, &host),
        Err(PluginError::Invalid(_))
    ));

    let mut manifest = test_manifest("org.test.invalid", "1.0.0");
    manifest["requires_capabilities"] = json!(["needs.activity_cost.v1"]);
    let cycle = json!({"start":"loop","steps":[{"id":"loop","op":"rest","next":"loop"}]});
    fixture(&path, manifest, cycle, None);
    assert!(matches!(
        inspect_pack(&path, &host),
        Err(PluginError::Missing(_))
    ));
    let host = HostConfig {
        capabilities: ["needs.activity_cost.v1".into()].into(),
        ..host
    };
    assert!(matches!(
        inspect_pack(&path, &host),
        Err(PluginError::Invalid(_))
    ));

    fs::write(&path, b"not a zip").unwrap();
    assert!(matches!(
        inspect_pack(&path, &host),
        Err(PluginError::Invalid(_))
    ));
}
