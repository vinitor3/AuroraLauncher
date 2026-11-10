use super::*;
use aurora_launcher_core::engine::connect::{ConnectManager, ConnectMode, HostRequest};
fn fixture() -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("aurora-connect-terminal-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    root
}
fn cleanup(root: &Path) {
    assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
    assert!(root
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("aurora-connect-terminal-"));
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn connect_terminal_writer_failure_preserves_normal_cleanup_and_latest_order() {
    let root = fixture();
    let blocked = root.join("blocked");
    fs::write(&blocked, b"preserve").unwrap();
    let writer = ConnectTerminalRecorder::new(blocked.clone());
    let records = Arc::new(Mutex::new(Vec::new()));
    let received = records.clone();
    let sink = writer.clone();
    let manager = ConnectManager::default().with_terminal_recorder(move |record| {
        received.lock().unwrap().push(record.clone());
        sink.submit(record);
    });
    for _ in 0..2 {
        let id = manager
            .prepare_host(
                "main",
                HostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: "127.0.0.1:43000".into(),
                    game_target: "127.0.0.1:43001".into(),
                    lifetime_seconds: 30,
                },
            )
            .unwrap()
            .operation_id;
        let snapshot = manager.stop("main", &id).await.unwrap();
        assert_eq!(snapshot.phase, "closed");
        assert_eq!(snapshot.cleanup_state, "complete");
        assert!(snapshot.error_code.is_none());
    }
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while writer.queue.lock().unwrap().running {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fs::read(&blocked).unwrap(), b"preserve");
    let records = records.lock().unwrap().clone();
    assert_eq!(records.len(), 2);
    let working = ConnectTerminalRecorder::new(root.join("working"));
    working.submit(records[1].clone());
    working.submit(records[0].clone());
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while working.queue.lock().unwrap().running {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(working.queue.lock().unwrap().latest, records[1].sequence());
    assert_eq!(
        fs::read(root.join("working/last-terminal.json")).unwrap(),
        serde_json::to_vec(&records[1]).unwrap()
    );
    cleanup(&root);
}
