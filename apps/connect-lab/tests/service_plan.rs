use aurora_connect_lab::{MAX_PLAN_BYTES, PlanError, UdpReturnPolicy, load_plan};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "aurora-connect-plan-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self {
            path: root.join("plan.json"),
            root,
        }
    }
    fn bytes(&self, bytes: &[u8]) {
        fs::write(&self.path, bytes).unwrap();
    }
    fn json(&self, value: &Value) {
        self.bytes(&serde_json::to_vec(value).unwrap());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(self.root.join("linked.json"));
        let _ = fs::remove_dir(self.root.join("linked"));
        let _ = fs::remove_file(self.root.join("target").join("plan.json"));
        let _ = fs::remove_dir(self.root.join("target"));
        let _ = fs::remove_dir(&self.root);
    }
}
fn valid() -> Value {
    json!({"schemaVersion":1,"bind":"127.0.0.1:32000","durationSeconds":60,"services":[
        {"serviceId":"game","protocol":"tcp","endpoint":"127.0.0.1:25565","revision":1,"requiredCapability":"reliableStream","maxDatagramBytes":1400},
        {"serviceId":"aux","protocol":"udp","endpoint":"[::1]:24454","revision":1,"requiredCapability":"datagram","maxDatagramBytes":1400}
    ]})
}
fn downstream() -> Value {
    json!({"type":"authorizedDownstream","bytesPerSecond":100_000,"datagramsPerSecond":100})
}

#[test]
fn valid_plan_defaults_to_reply_budget_and_preview_has_no_mutation_or_secret() {
    let fixture = Fixture::new();
    fixture.json(&valid());
    let before = fs::read(&fixture.path).unwrap();
    let plan = load_plan(&fixture.path, None).unwrap();
    assert_eq!(plan.seconds, 60);
    assert_eq!(plan.services.len(), 2);
    assert_eq!(plan.hash.len(), 64);
    assert!(
        plan.services
            .iter()
            .all(|s| s.udp_return_policy == UdpReturnPolicy::ReplyBudget)
    );
    let preview = plan.preview();
    assert_eq!(preview["filesChanged"], json!([]));
    assert_eq!(preview["modConfigurationChanges"], false);
    assert_eq!(preview["relay"], false);
    assert_eq!(preview["before"]["session"], "notStarted");
    assert!(preview.get("secretKey").is_none());
    assert_eq!(fs::read(&fixture.path).unwrap(), before);
    assert_eq!(
        load_plan(&fixture.path, Some(&plan.hash)).unwrap().hash,
        plan.hash
    );
}

#[test]
fn explicit_downstream_is_local_host_configuration_with_exact_limits() {
    let fixture = Fixture::new();
    let mut value = valid();
    value["services"][1]["udpReturnPolicy"] = downstream();
    fixture.json(&value);
    let plan = load_plan(&fixture.path, None).unwrap();
    assert_eq!(
        plan.services[1].udp_return_policy,
        UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second: 100_000,
            datagrams_per_second: 100
        }
    );
    assert_eq!(
        plan.preview()["after"]["services"][1]["udpReturnPolicy"],
        downstream()
    );
}

#[test]
fn every_change_including_whitespace_invalidates_exact_byte_approval() {
    let fixture = Fixture::new();
    fixture.json(&valid());
    let hash = load_plan(&fixture.path, None).unwrap().hash;
    let mut bytes = fs::read(&fixture.path).unwrap();
    bytes.push(b'\n');
    fixture.bytes(&bytes);
    assert!(matches!(
        load_plan(&fixture.path, Some(&hash)),
        Err(PlanError::Changed)
    ));
    let changed = load_plan(&fixture.path, None).unwrap();
    assert_ne!(changed.hash, hash);
    assert!(load_plan(&fixture.path, Some(&changed.hash.to_uppercase())).is_ok());
    assert!(matches!(
        load_plan(&fixture.path, Some("not-a-hash")),
        Err(PlanError::InvalidApproval)
    ));
}

#[test]
fn utf8_bom_is_accepted_but_remains_part_of_the_reviewed_bytes() {
    let fixture = Fixture::new();
    fixture.json(&valid());
    let hash = load_plan(&fixture.path, None).unwrap().hash;
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(&fs::read(&fixture.path).unwrap());
    fixture.bytes(&bytes);
    let plan = load_plan(&fixture.path, None).unwrap();
    assert_ne!(plan.hash, hash);
    assert!(matches!(
        load_plan(&fixture.path, Some(&hash)),
        Err(PlanError::Changed)
    ));
    assert!(load_plan(&fixture.path, Some(&plan.hash)).is_ok());
}

#[test]
fn unknown_fields_at_every_level_are_rejected() {
    let fixture = Fixture::new();
    let mut outer = valid();
    outer["relay"] = json!(true);
    let mut service = valid();
    service["services"][0]["ipOverride"] = json!("192.0.2.1");
    let mut policy = valid();
    policy["services"][1]["udpReturnPolicy"] = downstream();
    policy["services"][1]["udpReturnPolicy"]["unlimited"] = json!(true);
    let mut unit_policy = valid();
    unit_policy["services"][1]["udpReturnPolicy"] = json!({"type":"replyBudget","unlimited":true});
    for value in [outer, service, policy, unit_policy] {
        fixture.json(&value);
        assert!(matches!(
            load_plan(&fixture.path, None),
            Err(PlanError::InvalidJson)
        ));
    }
}

#[test]
fn invalid_schema_duration_service_set_protocol_and_revision_fail_closed() {
    let fixture = Fixture::new();
    let cases: Vec<(&str, Value)> = vec![
        ("/schemaVersion", json!(2)),
        ("/durationSeconds", json!(0)),
        ("/durationSeconds", json!(3601)),
        ("/services/0/serviceId", json!("rcon")),
        ("/services/1/serviceId", json!("game")),
        ("/services/0/protocol", json!("udp")),
        ("/services/1/requiredCapability", json!("reliableStream")),
        ("/services/0/revision", json!(0)),
        ("/services/0/revision", json!(2)),
        ("/services/1/maxDatagramBytes", json!(65508)),
    ];
    for (pointer, data) in cases {
        let mut value = valid();
        *value.pointer_mut(pointer).unwrap() = data;
        fixture.json(&value);
        assert!(
            matches!(load_plan(&fixture.path, None), Err(PlanError::InvalidPlan)),
            "{pointer}"
        );
    }
    let mut value = valid();
    value["services"].as_array_mut().unwrap().pop();
    fixture.json(&value);
    assert!(matches!(
        load_plan(&fixture.path, None),
        Err(PlanError::InvalidPlan)
    ));
}

#[test]
fn private_destinations_cannot_escape_loopback_and_transport_cannot_multicast() {
    let fixture = Fixture::new();
    for address in [
        "192.0.2.1:25565",
        "0.0.0.0:25565",
        "[::]:24454",
        "127.0.0.1:0",
    ] {
        let mut value = valid();
        value["services"][1]["endpoint"] = json!(address);
        fixture.json(&value);
        assert!(matches!(
            load_plan(&fixture.path, None),
            Err(PlanError::InvalidPlan)
        ));
    }
    for address in [
        "127.0.0.1:0",
        "224.0.0.1:32000",
        "[ff02::1]:32000",
        "255.255.255.255:32000",
    ] {
        let mut value = valid();
        value["bind"] = json!(address);
        fixture.json(&value);
        assert!(matches!(
            load_plan(&fixture.path, None),
            Err(PlanError::InvalidPlan)
        ));
    }
}

#[test]
fn downstream_limits_are_bounded_and_cannot_be_applied_to_tcp() {
    let fixture = Fixture::new();
    for (field, amount) in [
        ("bytesPerSecond", 0),
        ("bytesPerSecond", 23),
        ("bytesPerSecond", 250001),
        ("datagramsPerSecond", 0),
        ("datagramsPerSecond", 201),
    ] {
        let mut value = valid();
        let mut policy = downstream();
        policy[field] = json!(amount);
        value["services"][1]["udpReturnPolicy"] = policy;
        fixture.json(&value);
        assert!(matches!(
            load_plan(&fixture.path, None),
            Err(PlanError::InvalidPlan)
        ));
    }
    let mut value = valid();
    value["services"][0]["udpReturnPolicy"] = downstream();
    fixture.json(&value);
    assert!(matches!(
        load_plan(&fixture.path, None),
        Err(PlanError::InvalidPlan)
    ));
}

#[test]
fn malformed_truncated_oversize_non_file_and_missing_inputs_are_rejected() {
    let fixture = Fixture::new();
    for bytes in [b"".as_slice(), b"{\"schemaVersion\":1", &[0xff], b"{}"][..].iter() {
        fixture.bytes(bytes);
        assert!(load_plan(&fixture.path, None).is_err());
    }
    fixture.bytes(&vec![b' '; MAX_PLAN_BYTES + 1]);
    assert!(matches!(
        load_plan(&fixture.path, None),
        Err(PlanError::TooLarge)
    ));
    assert!(matches!(
        load_plan(&fixture.root, None),
        Err(PlanError::NotRegular)
    ));
    assert!(matches!(
        load_plan(fixture.root.join("missing.json"), None),
        Err(PlanError::Read)
    ));
}

#[test]
fn linked_file_or_directory_is_rejected_without_following_to_plan() {
    let fixture = Fixture::new();
    fixture.json(&valid());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&fixture.path, fixture.root.join("linked.json")).unwrap();
        assert!(matches!(
            load_plan(fixture.root.join("linked.json"), None),
            Err(PlanError::LinkedPath)
        ));
    }
    #[cfg(windows)]
    {
        let target = fixture.root.join("target");
        fs::create_dir(&target).unwrap();
        fs::copy(&fixture.path, target.join("plan.json")).unwrap();
        let link = fixture.root.join("linked");
        // A directory junction needs no administrator/developer-mode privilege.
        let result = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "test could not create its temporary junction"
        );
        assert!(matches!(
            load_plan(link.join("plan.json"), None),
            Err(PlanError::LinkedPath)
        ));
        assert!(load_plan(target.join("plan.json"), None).is_ok());
    }
}

#[cfg(windows)]
#[test]
fn network_device_ambiguous_and_alternate_stream_paths_fail_lexically() {
    for path in [
        r"\\server.invalid\share\plan.json",
        r"\\?\UNC\server.invalid\share\plan.json",
        r"\\.\pipe\aurora-connect-plan",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1\plan.json",
        r"C:relative-plan.json",
        r"\relative-plan.json",
        r"plan.json:secret",
        r"C:\plan.json::$DATA",
        r"NUL",
        r"con.json",
        r"COM1.json",
        r"LPT².json",
        r"CONIN$",
        r"plan.json.",
        r"plan.json ",
    ] {
        assert!(
            matches!(load_plan(path, None), Err(PlanError::InvalidPlan)),
            "{path} must fail before filesystem access"
        );
    }
}
