//! Synthetic broker fixtures. No game, model, shell or fixture JAR is executed.
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use aurora_launcher_core::engine::ai_tools::{
    AiToolAdvice, AiToolBroker, AiToolContext, AiToolProposal,
};
use aurora_launcher_core::engine::{
    acquire_instance_mutation_lock, Instance, InstanceId, InstanceLayout,
};
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;

struct Fixture {
    root: PathBuf,
    instance: Instance,
    broker: AiToolBroker,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("aurora-ai-broker-tests-{}", uuid::Uuid::new_v4()));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("fixture").unwrap());
        instance.ensure_layout().unwrap();
        fs::create_dir(instance.root().join("config")).unwrap();
        fs::write(
            instance.root().join("mods/example.jar"),
            jar("Synthetic Example", false),
        )
        .unwrap();
        fs::write(instance.root().join("config/render.json"),b"{\n  \"renderDistance\": 16, \"showFps\": false, \"secretToken\": \"SYNTHETIC_SECRET\", \"security\": {\"showFps\": true}\n}").unwrap();
        Self {
            root,
            instance,
            broker: AiToolBroker::default(),
        }
    }
    fn context(&self) -> AiToolContext {
        self.broker
            .context(self.instance.clone(), "main", Vec::new())
            .unwrap()
    }
    fn remove(&self, c: &AiToolContext) -> AiToolProposal {
        AiToolProposal::RemoveMod {
            target_id: c.mods.iter().find(|item| !item.managed).unwrap().id.clone(),
            reason: "Conflito observado; revisar antes de alterar.".into(),
        }
    }
    fn setting(&self, c: &AiToolContext, value: Value) -> AiToolProposal {
        AiToolProposal::SetSetting {
            target_id: c
                .settings
                .iter()
                .find(|s| s.kind == "number")
                .unwrap()
                .id
                .clone(),
            value,
            reason: "Reduzir a distância de renderização para teste.".into(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn jar(name: &str, managed: bool) -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    archive
        .write_all(
            serde_json::to_string(&json!({"id":"synthetic","name":name,"version":"1.0.0"}))
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
    if managed {
        archive
            .start_file(
                "com/aurora/skins/Synthetic.class",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(b"not-a-class").unwrap();
    }
    archive.finish().unwrap().into_inner()
}
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                result.insert(
                    entry.path().strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    result
}

#[test]
fn context_and_preparation_are_read_only_and_model_inventory_is_minimal() {
    let f = Fixture::new();
    let before = files(f.instance.root());
    let c = f.context();
    assert_eq!(c.mods.len(), 1);
    assert_eq!(c.settings.len(), 2);
    let serialized = serde_json::to_string(&c).unwrap();
    for private in [
        "SYNTHETIC_SECRET",
        "secretToken",
        "render.json",
        "example.jar",
        "fabric.mod.json",
    ] {
        assert!(!serialized.contains(private), "{private}");
    }
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.setting(&c, json!(8)))
        .unwrap();
    assert_eq!(plan.changes[0].before, "16");
    assert_eq!(plan.changes[0].after, "8");
    assert_eq!(files(f.instance.root()), before);
}

#[test]
fn wrong_window_digest_and_replay_cannot_apply() {
    let f = Fixture::new();
    let c = f.context();
    let proposal = f.remove(&c);
    assert!(f
        .broker
        .prepare(&f.instance, "other", &c.context_id, proposal.clone())
        .is_err());
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, proposal)
        .unwrap();
    let before = files(f.instance.root());
    assert!(f
        .broker
        .confirm("other", &plan.plan_id, &plan.digest, |_| Ok(()))
        .is_err());
    assert!(f
        .broker
        .confirm("main", &plan.plan_id, "bad-digest", |_| Ok(()))
        .is_err());
    assert_eq!(files(f.instance.root()), before);
    let applied = f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .unwrap();
    assert_eq!(applied.receipt.status, "applied");
    assert!(applied.receipt.undo_available);
    assert!(f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .is_err());
}

#[test]
fn remove_and_undo_preserve_exact_jar_bytes() {
    let f = Fixture::new();
    let original = fs::read(f.instance.root().join("mods/example.jar")).unwrap();
    let c = f.context();
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.remove(&c))
        .unwrap();
    let applied = f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .unwrap();
    assert!(!f.instance.root().join("mods/example.jar").exists());
    let undo = f
        .broker
        .prepare_undo(
            &f.instance,
            "main",
            applied.receipt.action_id.as_deref().unwrap(),
        )
        .unwrap();
    let result = f
        .broker
        .confirm("main", &undo.plan_id, &undo.digest, |_| Ok(()))
        .unwrap();
    assert_eq!(result.receipt.status, "undone");
    assert!(!result.receipt.undo_available);
    assert_eq!(
        fs::read(f.instance.root().join("mods/example.jar")).unwrap(),
        original
    );
}

#[test]
fn setting_changes_only_selected_scalar_and_undo_restores_original_format() {
    let f = Fixture::new();
    let path = f.instance.root().join("config/render.json");
    let original = fs::read(&path).unwrap();
    let c = f.context();
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.setting(&c, json!(8)))
        .unwrap();
    let applied = f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .unwrap();
    let changed: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(changed["renderDistance"], 8);
    assert_eq!(changed["secretToken"], "SYNTHETIC_SECRET");
    assert_eq!(changed["security"]["showFps"], true);
    let broker = AiToolBroker::default(); // persisted signed journal works after restart
    let undo = broker
        .prepare_undo(
            &f.instance,
            "main",
            applied.receipt.action_id.as_deref().unwrap(),
        )
        .unwrap();
    broker
        .confirm("main", &undo.plan_id, &undo.digest, |_| Ok(()))
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn stale_context_and_plan_never_overwrite_external_changes() {
    let f = Fixture::new();
    let c = f.context();
    let proposal = f.setting(&c, json!(8));
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, proposal.clone())
        .unwrap();
    let path = f.instance.root().join("config/render.json");
    fs::write(&path, b"{\"renderDistance\":12}").unwrap();
    let before = files(f.instance.root());
    assert!(f
        .broker
        .prepare(&f.instance, "main", &c.context_id, proposal)
        .is_err());
    assert!(f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .is_err());
    assert_eq!(files(f.instance.root()), before);
}

#[test]
fn running_game_or_existing_mutation_lock_blocks_commit() {
    let f = Fixture::new();
    let c = f.context();
    let before = files(f.instance.root());
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.remove(&c))
        .unwrap();
    assert!(f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Err(
            "Jogo em execução.".into()
        ))
        .is_err());
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.remove(&c))
        .unwrap();
    let _guard = acquire_instance_mutation_lock(&f.instance).unwrap();
    assert!(f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| panic!(
            "idle callback must not bypass lock"
        ))
        .is_err());
    assert_eq!(files(f.instance.root()), before);
}

#[test]
fn managed_renamed_jar_and_invalid_requests_are_refused() {
    let f = Fixture::new();
    fs::write(
        f.instance.root().join("mods/renamed.jar"),
        jar("Renamed", true),
    )
    .unwrap();
    let c = f.context();
    let before = files(f.instance.root());
    let managed = c.mods.iter().find(|m| m.managed).unwrap();
    assert!(f
        .broker
        .prepare(
            &f.instance,
            "main",
            &c.context_id,
            AiToolProposal::RemoveMod {
                target_id: managed.id.clone(),
                reason: "teste".into()
            }
        )
        .is_err());
    for value in [
        json!("text"),
        json!(true),
        json!(99999),
        json!(8.5),
        json!(16),
    ] {
        assert!(f
            .broker
            .prepare(&f.instance, "main", &c.context_id, f.setting(&c, value))
            .is_err());
    }
    assert!(f
        .broker
        .prepare(
            &f.instance,
            "main",
            &c.context_id,
            AiToolProposal::RemoveMod {
                target_id: "../../secret".into(),
                reason: "teste".into()
            }
        )
        .is_err());
    assert_eq!(files(f.instance.root()), before);
}

#[test]
fn revoke_and_cross_instance_context_are_enforced() {
    let f = Fixture::new();
    let c = f.context();
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.remove(&c))
        .unwrap();
    f.broker.revoke("main", &plan.plan_id).unwrap();
    assert!(f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .is_err());
    let other = InstanceLayout::new(&f.root).open(InstanceId::parse("other").unwrap());
    other.ensure_layout().unwrap();
    assert!(f
        .broker
        .prepare(&other, "main", &c.context_id, f.remove(&c))
        .is_err());
}

#[test]
fn undo_refuses_a_new_file_and_tampered_journal() {
    let f = Fixture::new();
    let c = f.context();
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.remove(&c))
        .unwrap();
    let applied = f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .unwrap();
    let action = applied.receipt.action_id.unwrap();
    fs::write(
        f.instance.root().join("mods/example.jar"),
        b"new user content",
    )
    .unwrap();
    assert!(f.broker.prepare_undo(&f.instance, "main", &action).is_err());
    let journal = f
        .instance
        .root()
        .join(format!(".aurora/ai-tools/{action}/plan.json"));
    let mut value: Value = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
    value["target"] = json!("config/render.json");
    fs::write(&journal, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(f.broker.prepare_undo(&f.instance, "main", &action).is_err());
    assert_eq!(
        fs::read(f.instance.root().join("mods/example.jar")).unwrap(),
        b"new user content"
    );
}

#[test]
fn partial_journal_keeps_recovery_available() {
    let f = Fixture::new();
    let c = f.context();
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.remove(&c))
        .unwrap();
    let applied = f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .unwrap();
    let action = applied.receipt.action_id.unwrap();
    fs::remove_file(
        f.instance
            .root()
            .join(format!(".aurora/ai-tools/{action}/applied")),
    )
    .unwrap();
    let history = f.broker.history(&f.instance).unwrap();
    assert_eq!(history[0].status, "pending");
    assert!(history[0].undo_available);
    let undo = f.broker.prepare_undo(&f.instance, "main", &action).unwrap();
    f.broker
        .confirm("main", &undo.plan_id, &undo.digest, |_| Ok(()))
        .unwrap();
    assert!(f.instance.root().join("mods/example.jar").exists());
}

#[test]
fn hardlinked_inputs_are_never_capabilities() {
    let f = Fixture::new();
    let outside = f.root.join("private.json");
    fs::write(&outside, b"{\"renderDistance\":16}").unwrap();
    fs::hard_link(&outside, f.instance.root().join("config/linked.json")).unwrap();
    let c = f.context();
    assert_eq!(c.settings.len(), 2);
    assert!(c.truncated);
    assert_eq!(fs::read(outside).unwrap(), b"{\"renderDistance\":16}");
}

#[test]
fn advice_is_validated_without_applied_action() {
    let f = Fixture::new();
    let c = f.context();
    let before = files(f.instance.root());
    let advice = AiToolAdvice {
        summary: "Sugestão sujeita à revisão do jogador.".into(),
        proposals: vec![f.setting(&c, json!(8))],
    };
    let targets = f
        .broker
        .validate_advice(&f.instance, "main", &c.context_id, &advice)
        .unwrap();
    assert_eq!(targets[0].0, "set_setting");
    assert!(!targets[0].1.contains(".json"));
    assert!(f
        .broker
        .validate_advice(&f.instance, "other", &c.context_id, &advice)
        .is_err());
    assert_eq!(files(f.instance.root()), before);
}

#[test]
fn interrupted_config_replacement_can_restore_missing_target() {
    let f = Fixture::new();
    let path = f.instance.root().join("config/render.json");
    let original = fs::read(&path).unwrap();
    let c = f.context();
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.setting(&c, json!(8)))
        .unwrap();
    let applied = f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .unwrap();
    let action = applied.receipt.action_id.unwrap();
    let directory = f.instance.root().join(format!(".aurora/ai-tools/{action}"));
    // Reproduce a crash after quarantining the original and before publishing the stage.
    fs::remove_file(directory.join("applied")).unwrap();
    fs::rename(&path, directory.join("after.bin")).unwrap();
    let broker = AiToolBroker::default();
    let entry = broker.history(&f.instance).unwrap().remove(0);
    assert_eq!(entry.status, "pending");
    assert!(entry.undo_available);
    let undo = broker.prepare_undo(&f.instance, "main", &action).unwrap();
    let result = broker
        .confirm("main", &undo.plan_id, &undo.digest, |_| Ok(()))
        .unwrap();
    assert_eq!(result.receipt.status, "undone");
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn copied_signed_journal_cannot_restore_into_another_instance() {
    let f = Fixture::new();
    let c = f.context();
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.remove(&c))
        .unwrap();
    let applied = f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .unwrap();
    let action = applied.receipt.action_id.unwrap();
    let other = InstanceLayout::new(&f.root).open(InstanceId::parse("other").unwrap());
    other.ensure_layout().unwrap();
    let relative = format!(".aurora/ai-tools/{action}");
    let destination = other.root().join(&relative);
    fs::create_dir_all(&destination).unwrap();
    for (name, bytes) in files(&f.instance.root().join(relative)) {
        fs::write(destination.join(name), bytes).unwrap();
    }
    let before = files(other.root());
    assert!(f.broker.prepare_undo(&other, "main", &action).is_err());
    assert!(f.broker.history(&other).unwrap().is_empty());
    assert_eq!(files(other.root()), before);
}

#[test]
fn ordinary_instance_rename_preserves_authenticated_undo() {
    let f = Fixture::new();
    let original = fs::read(f.instance.root().join("mods/example.jar")).unwrap();
    let c = f.context();
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.remove(&c))
        .unwrap();
    let applied = f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .unwrap();
    let renamed = InstanceLayout::new(&f.root).open(InstanceId::parse("renamed").unwrap());
    fs::rename(f.instance.root(), renamed.root()).unwrap();
    let broker = AiToolBroker::default();
    let undo = broker
        .prepare_undo(
            &renamed,
            "main",
            applied.receipt.action_id.as_deref().unwrap(),
        )
        .unwrap();
    broker
        .confirm("main", &undo.plan_id, &undo.digest, |_| Ok(()))
        .unwrap();
    assert_eq!(
        fs::read(renamed.root().join("mods/example.jar")).unwrap(),
        original
    );
}

#[test]
fn damaged_journal_does_not_hide_other_verified_recovery() {
    let f = Fixture::new();
    let c = f.context();
    let plan = f
        .broker
        .prepare(&f.instance, "main", &c.context_id, f.remove(&c))
        .unwrap();
    let applied = f
        .broker
        .confirm("main", &plan.plan_id, &plan.digest, |_| Ok(()))
        .unwrap();
    let bad = uuid::Uuid::new_v4().to_string();
    let directory = f.instance.root().join(format!(".aurora/ai-tools/{bad}"));
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("plan.json"), b"{truncated").unwrap();
    let history = f.broker.history(&f.instance).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].action_id, applied.receipt.action_id);
    assert!(f.broker.prepare_undo(&f.instance, "main", &bad).is_err());
}

#[test]
fn unidentified_archive_never_shares_filename_and_is_protected() {
    let f = Fixture::new();
    let archive = zip::ZipWriter::new(Cursor::new(Vec::new()))
        .finish()
        .unwrap()
        .into_inner();
    fs::write(
        f.instance
            .root()
            .join("mods/SYNTHETIC_PERSONAL_FILENAME.jar"),
        archive,
    )
    .unwrap();
    let c = f.context();
    let unknown = c.mods.iter().find(|item| item.name == "Mod local").unwrap();
    assert!(unknown.managed);
    assert!(!serde_json::to_string(&c)
        .unwrap()
        .contains("SYNTHETIC_PERSONAL_FILENAME"));
}

#[cfg(windows)]
#[test]
fn junction_config_directory_is_not_scanned_or_modified() {
    use std::os::windows::process::CommandExt;
    struct Junction(PathBuf);
    impl Drop for Junction {
        fn drop(&mut self) {
            fs::remove_dir(&self.0).unwrap();
        }
    }
    let f = Fixture::new();
    let outside = f.root.join("outside");
    let linked = f.instance.root().join("config");
    fs::rename(&linked, &outside).unwrap();
    let result = std::process::Command::new("powershell.exe")
        .creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:AURORA_AI_TEST_LINK -Target $env:AURORA_AI_TEST_TARGET | Out-Null"])
        .env("AURORA_AI_TEST_LINK", &linked).env("AURORA_AI_TEST_TARGET", &outside).output().unwrap();
    assert!(result.status.success());
    let _junction = Junction(linked);
    let before = files(&outside);
    assert!(f
        .broker
        .context(f.instance.clone(), "main", Vec::new())
        .is_err());
    assert_eq!(files(&outside), before);
}
