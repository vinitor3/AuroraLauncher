//! Transaction adversarial checks; all bytes and instances are synthetic.
use super::*;
use crate::engine::{acquire_instance_recovery_lock, InstanceId, InstanceLayout};

struct Fixture {
    root: PathBuf,
    instance: Instance,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("aurora-recovery-journal-{}", Uuid::new_v4()));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("fixture").unwrap());
        instance.ensure_layout().unwrap();
        fs::create_dir(instance.root().join("config")).unwrap();
        fs::write(
            instance.root().join("config/video.json"),
            b"{\"distance\":8}",
        )
        .unwrap();
        fs::write(
            instance.root().join(PROFILE),
            br#"{"name":"Saved","projectId":"synthetic-link","source":"modrinth"}"#,
        )
        .unwrap();
        Self { root, instance }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn prepare(fixture: &Fixture, guard: &InstanceMutationGuard) -> (RecoveryPreview, Vec<u8>) {
    let point = create_point(&fixture.instance, guard, None).unwrap();
    fs::write(
        fixture.instance.root().join("config/video.json"),
        b"{\"distance\":24}",
    )
    .unwrap();
    let profile = br#"{ "name": "Current", "projectId": "synthetic-link", "source": "modrinth", "untouched": "synthetic-account-data" }"#.to_vec();
    fs::write(fixture.instance.root().join(PROFILE), &profile).unwrap();
    (
        preview_restore(&fixture.instance, guard, &point.id).unwrap(),
        profile,
    )
}

fn pending(store: &Store) -> Journal {
    let bytes: Vec<u8> = store
        .db
        .query_row(
            "SELECT data FROM journals WHERE instance_uuid=?1",
            [&store.uuid],
            |row| row.get(0),
        )
        .unwrap();
    parse(&bytes).unwrap()
}

#[test]
fn interrupted_first_identity_publication_does_not_strand_an_untouched_instance() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    FAULT.with(|fault| fault.set(Fault::IdentityPublish));
    let result = create_point(&fixture.instance, &guard, None);
    FAULT.with(|fault| fault.set(Fault::None));
    assert!(matches!(result, Err(RecoveryError::Io)));
    assert!(!fixture.instance.root().join(IDENTITY).exists());
    drop(guard);
    let guard = crate::engine::acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let point = create_point(&fixture.instance, &guard, None).unwrap();
    assert_eq!(
        list_points(&fixture.instance, &guard).unwrap().points[0].id,
        point.id
    );
    assert_eq!(
        fs::read(fixture.instance.root().join("config/video.json")).unwrap(),
        b"{\"distance\":8}"
    );
}

#[test]
fn completed_file_renames_still_rollback_after_restart_before_index_commit() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let (preview, original_profile) = prepare(&fixture, &guard);
    assert!(matches!(
        restore_impl(&fixture.instance, &guard, &preview.id, Some(2)),
        Err(RecoveryError::Pending)
    ));
    drop(guard);
    assert!(crate::engine::acquire_instance_mutation_lock(&fixture.instance).is_err());
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    assert!(recover_pending(&fixture.instance, &guard).unwrap());
    assert_eq!(
        fs::read(fixture.instance.root().join(PROFILE)).unwrap(),
        original_profile
    );
    assert_eq!(
        fs::read(fixture.instance.root().join("config/video.json")).unwrap(),
        b"{\"distance\":24}"
    );
    assert!(!recover_pending(&fixture.instance, &guard).unwrap());
}

#[test]
fn poisoned_in_scope_journal_cannot_remove_an_unapproved_file() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let (preview, _) = prepare(&fixture, &guard);
    assert!(restore_impl(&fixture.instance, &guard, &preview.id, Some(1)).is_err());
    let store = Store::open(&fixture.instance).unwrap();
    let mut journal = pending(&store);
    let file = fixture.instance.root().join("config/unrelated.json");
    fs::write(&file, b"unrelated external content").unwrap();
    journal.operations[0].path = "config/unrelated.json".into();
    let operation = &mut journal.operations[0];
    for record in [&mut operation.before, &mut operation.after]
        .into_iter()
        .flatten()
    {
        record.path = "config/unrelated.json".into();
    }
    store
        .db
        .execute(
            "UPDATE journals SET data=?1 WHERE instance_uuid=?2",
            params![json(&journal).unwrap(), store.uuid],
        )
        .unwrap();
    assert!(recover_pending(&fixture.instance, &guard).is_err());
    assert_eq!(fs::read(file).unwrap(), b"unrelated external content");
    assert!(has_pending(&fixture.instance).unwrap());
}

#[test]
fn profile_journal_cannot_restore_modified_internal_links() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let (preview, _) = prepare(&fixture, &guard);
    assert!(restore_impl(&fixture.instance, &guard, &preview.id, Some(2)).is_err());
    let store = Store::open(&fixture.instance).unwrap();
    let mut journal = pending(&store);
    let index = journal
        .operations
        .iter()
        .position(|op| op.path == PROFILE)
        .unwrap();
    let old = fixture.instance.root().join(format!(
        ".aurora/recovery-stage/{}/old/{index}",
        journal.transaction_id
    ));
    let forged = br#"{"name":"Current","projectId":"forged-link","source":"modrinth"}"#;
    fs::write(old, forged).unwrap();
    let record = journal.operations[index].before.as_mut().unwrap();
    record.sha256 = digest(forged);
    record.size = forged.len() as u64;
    store
        .db
        .execute(
            "UPDATE journals SET data=?1 WHERE instance_uuid=?2",
            params![json(&journal).unwrap(), store.uuid],
        )
        .unwrap();
    let current = fs::read(fixture.instance.root().join(PROFILE)).unwrap();
    assert!(recover_pending(&fixture.instance, &guard).is_err());
    assert_eq!(
        fs::read(fixture.instance.root().join(PROFILE)).unwrap(),
        current
    );
}

#[test]
fn rollback_preserves_external_edits_and_stays_blocked_instead_of_overwriting_them() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let (preview, original_profile) = prepare(&fixture, &guard);
    assert!(restore_impl(&fixture.instance, &guard, &preview.id, Some(1)).is_err());
    let changed = fixture.instance.root().join("config/video.json");
    fs::write(&changed, b"external edit after interruption").unwrap();
    assert!(matches!(
        recover_pending(&fixture.instance, &guard),
        Err(RecoveryError::Changed)
    ));
    assert_eq!(
        fs::read(changed).unwrap(),
        b"external edit after interruption"
    );
    assert_eq!(
        fs::read(fixture.instance.root().join(PROFILE)).unwrap(),
        original_profile
    );
    assert!(has_pending(&fixture.instance).unwrap());
}
