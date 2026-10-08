//! Explicit plan/invitation opt-in. No Java, mods, accounts or external I/O.
use aurora_connect_lab::{
    Capability, Error, Guest, Host, HostConfig, Invitation, Limits, PlanError, Protocol, SecretKey,
    Service, UdpReturnPolicy, generate_key, load_fragmented_plan, load_plan, unix_millis,
};
use serde_json::{Value, json};
use std::{fs, net::SocketAddr, path::PathBuf, time::Duration};

struct PlanFile {
    root: PathBuf,
    path: PathBuf,
}
impl PlanFile {
    fn new(value: &Value) -> Self {
        let root = std::env::temp_dir().join(format!(
            "aurora-connect-fragmented-profile-{}",
            generate_key().public()
        ));
        fs::create_dir(&root).unwrap();
        let fixture = Self {
            path: root.join("plan.json"),
            root,
        };
        fixture.write(value);
        fixture
    }
    fn write(&self, value: &Value) {
        fs::write(&self.path, serde_json::to_vec(value).unwrap()).unwrap();
    }
}
impl Drop for PlanFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.root);
    }
}

fn plan() -> Value {
    json!({"schemaVersion":1,"bind":"127.0.0.1:32000","durationSeconds":60,"services":[
        {"serviceId":"game","protocol":"tcp","endpoint":"127.0.0.1:32001","revision":1,
         "requiredCapability":"reliableStream","maxDatagramBytes":1400},
        {"serviceId":"aux","protocol":"udp","endpoint":"127.0.0.1:32002","revision":1,
         "requiredCapability":"fragmentedDatagram","maxDatagramBytes":4096}
    ]})
}
fn services(fragmented: bool) -> Vec<Service> {
    vec![
        Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: "127.0.0.1:32001".parse().unwrap(),
            revision: 1,
            required_capability: Capability::ReliableStream,
            max_datagram_bytes: 1400,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: "127.0.0.1:32002".parse().unwrap(),
            revision: 1,
            required_capability: if fragmented {
                Capability::FragmentedDatagram
            } else {
                Capability::Datagram
            },
            max_datagram_bytes: if fragmented { 4096 } else { 1400 },
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
    ]
}
fn issue(
    host: &SecretKey,
    guest: &SecretKey,
    services: &[Service],
) -> aurora_connect_lab::Result<Invitation> {
    Invitation::issue_fragmented(
        host,
        guest.public(),
        "fragmented-profile",
        "127.0.0.1:32000".parse().unwrap(),
        services,
        unix_millis() + 60_000,
    )
}

#[test]
fn fragmented_plan_requires_explicit_loader_and_exact_hash_without_mutation() {
    let fixture = PlanFile::new(&plan());
    let bytes = fs::read(&fixture.path).unwrap();
    assert!(matches!(
        load_plan(&fixture.path, None),
        Err(PlanError::InvalidPlan)
    ));
    let reviewed = load_fragmented_plan(&fixture.path, None).unwrap();
    assert_eq!(
        reviewed.services[1].required_capability,
        Capability::FragmentedDatagram
    );
    assert_eq!(reviewed.preview()["profile"], "fragmentedV2");
    assert_eq!(reviewed.preview()["udpEnvelopeVersion"], 2);
    assert_eq!(reviewed.preview()["maxFragmentFrameBytes"], 1264);
    assert_eq!(reviewed.preview()["filesChanged"], json!([]));
    assert_eq!(reviewed.preview()["relay"], false);
    assert_eq!(
        load_fragmented_plan(&fixture.path, Some(&reviewed.hash))
            .unwrap()
            .hash,
        reviewed.hash
    );
    assert_eq!(fs::read(&fixture.path).unwrap(), bytes);

    let mut changed = plan();
    changed["services"][1]["maxDatagramBytes"] = json!(4095);
    fixture.write(&changed);
    assert!(matches!(
        load_fragmented_plan(&fixture.path, Some(&reviewed.hash)),
        Err(PlanError::Changed)
    ));
    assert!(matches!(
        load_fragmented_plan(&fixture.path, Some("invalid")),
        Err(PlanError::InvalidApproval)
    ));
}

#[test]
fn fragmented_plan_rejects_legacy_or_partial_grants_and_preserves_legacy_preview() {
    let mut legacy = plan();
    legacy["services"][1]["requiredCapability"] = json!("datagram");
    let fixture = PlanFile::new(&legacy);
    let old = load_plan(&fixture.path, None).unwrap();
    assert!(old.preview().get("profile").is_none());
    assert!(old.preview().get("udpEnvelopeVersion").is_none());
    assert!(matches!(
        load_fragmented_plan(&fixture.path, None),
        Err(PlanError::InvalidPlan)
    ));
    legacy["services"].as_array_mut().unwrap().pop();
    fixture.write(&legacy);
    assert!(load_plan(&fixture.path, None).is_ok());
    assert!(matches!(
        load_fragmented_plan(&fixture.path, None),
        Err(PlanError::InvalidPlan)
    ));
}

#[test]
fn fragmented_plan_rejects_invalid_shape_caps_and_unknown_fields() {
    let fixture = PlanFile::new(&plan());
    let mut invalid = Vec::new();
    for (field, value) in [
        ("maxDatagramBytes", json!(0)),
        ("maxDatagramBytes", json!(4097)),
        ("revision", json!(2)),
        ("serviceId", json!("game")),
        ("protocol", json!("tcp")),
        ("endpoint", json!("0.0.0.0:32002")),
        ("endpoint", json!("127.0.0.1:0")),
        ("extra", json!(true)),
    ] {
        let mut p = plan();
        p["services"][1][field] = value;
        invalid.push(p);
    }
    let mut p = plan();
    p["profile"] = json!("fragmentedV2");
    invalid.push(p);
    let mut p = plan();
    p["services"][0]["requiredCapability"] = json!("fragmentedDatagram");
    invalid.push(p);
    let mut p = plan();
    p["services"][0]["maxDatagramBytes"] = json!(0);
    invalid.push(p);
    let mut p = plan();
    p["services"].as_array_mut().unwrap().remove(0);
    invalid.push(p);
    for value in invalid {
        fixture.write(&value);
        assert!(load_fragmented_plan(&fixture.path, None).is_err());
    }
}

#[test]
fn fragmented_plan_downstream_budget_counts_its_32_byte_header() {
    let mut value = plan();
    value["services"][1]["udpReturnPolicy"] = json!({
        "type":"authorizedDownstream","bytesPerSecond":32,"datagramsPerSecond":1
    });
    let fixture = PlanFile::new(&value);
    assert!(load_fragmented_plan(&fixture.path, None).is_ok());
    for (field, bad) in [
        ("bytesPerSecond", 31),
        ("bytesPerSecond", 250001),
        ("datagramsPerSecond", 0),
        ("datagramsPerSecond", 201),
    ] {
        let mut rejected = value.clone();
        rejected["services"][1]["udpReturnPolicy"][field] = json!(bad);
        fixture.write(&rejected);
        assert!(matches!(
            load_fragmented_plan(&fixture.path, None),
            Err(PlanError::InvalidPlan)
        ));
    }
}

#[test]
fn fragmented_invitation_roundtrip_is_canonical_signed_and_legacy_denied() {
    let host = generate_key();
    let guest = generate_key();
    let invitation = issue(&host, &guest, &services(true)).unwrap();
    let encoded = invitation.encode();
    let verified = Invitation::verify_fragmented(&encoded, host.public(), guest.public()).unwrap();
    assert!(verified.encode() == encoded, "canonical invitation changed");
    assert_eq!(verified.digest(), invitation.digest());
    assert_eq!(
        verified.preview()["services"][1]["requiredCapability"],
        "fragmentedDatagram"
    );
    assert_eq!(verified.preview()["services"][1]["maxDatagramBytes"], 4096);
    assert_eq!(verified.preview()["schemaVersion"], 1);
    assert_eq!(verified.preview()["profile"], "fragmentedV2");
    assert_eq!(verified.preview()["udpEnvelopeVersion"], 2);
    assert_eq!(verified.preview()["maxFragmentFrameBytes"], 1264);
    assert_eq!(verified.preview()["singleUse"], true);
    assert!(Invitation::verify(&encoded, host.public(), guest.public()).is_err());
    assert!(
        Invitation::issue(
            &host,
            guest.public(),
            "session",
            "127.0.0.1:32000".parse().unwrap(),
            &services(true),
            unix_millis() + 60_000
        )
        .is_err()
    );
    let old = Invitation::issue(
        &host,
        guest.public(),
        "session",
        "127.0.0.1:32000".parse().unwrap(),
        &services(false),
        unix_millis() + 60_000,
    )
    .unwrap();
    assert!(Invitation::verify(&old.encode(), host.public(), guest.public()).is_ok());
    assert!(Invitation::verify_fragmented(&old.encode(), host.public(), guest.public()).is_err());
    assert!(issue(&host, &guest, &services(false)).is_err());
}

#[test]
fn fragmented_invitation_rejects_changed_signed_cap_limit_and_policy() {
    let host = generate_key();
    let guest = generate_key();
    let mut grants = services(true);
    grants[1].udp_return_policy = UdpReturnPolicy::AuthorizedDownstream {
        bytes_per_second: 2000,
        datagrams_per_second: 10,
    };
    let invitation = issue(&host, &guest, &grants).unwrap();
    for (before, after) in [
        ("\"maxDatagramBytes\":4096", "\"maxDatagramBytes\":4095"),
        ("\"bytesPerSecond\":2000", "\"bytesPerSecond\":2001"),
        ("\"datagramsPerSecond\":10", "\"datagramsPerSecond\":11"),
        ("\"fragmentedDatagram\"", "\"datagram\""),
    ] {
        let encoded = invitation.encode();
        assert!(encoded.contains(before));
        let changed = encoded.replace(before, after);
        assert!(Invitation::verify_fragmented(&changed, host.public(), guest.public()).is_err());
        // Switching to a legacy verifier cannot turn a signed V2 grant into V1.
        if before == "\"fragmentedDatagram\"" {
            assert!(matches!(
                Invitation::verify(&changed, host.public(), guest.public()),
                Err(Error::Unauthorized)
            ));
        } else {
            assert!(matches!(
                Invitation::verify_fragmented(&changed, host.public(), guest.public()),
                Err(Error::Unauthorized)
            ));
        }
    }
    assert!(
        Invitation::verify_fragmented(
            &invitation.encode(),
            generate_key().public(),
            guest.public()
        )
        .is_err()
    );
    assert!(
        Invitation::verify_fragmented(&invitation.encode(), host.public(), generate_key().public())
            .is_err()
    );
    assert!(
        Invitation::verify_fragmented(&(invitation.encode() + " "), host.public(), guest.public())
            .is_err()
    );
}

#[test]
fn fragmented_invitation_requires_both_exact_services_valid_caps_and_budgets() {
    let host = generate_key();
    let guest = generate_key();
    let mut invalid = vec![
        vec![services(true).remove(0)],
        vec![services(true).remove(1)],
    ];
    for max in [0, 4097, 65507] {
        let mut s = services(true);
        s[1].max_datagram_bytes = max;
        invalid.push(s);
    }
    let mut s = services(true);
    s[1].revision = 2;
    invalid.push(s);
    let mut s = services(true);
    s[1].endpoint = "0.0.0.0:32002".parse().unwrap();
    invalid.push(s);
    let mut s = services(true);
    s[1].service_id = "game".into();
    invalid.push(s);
    let mut s = services(true);
    s[1].udp_return_policy = UdpReturnPolicy::AuthorizedDownstream {
        bytes_per_second: 31,
        datagrams_per_second: 1,
    };
    invalid.push(s);
    for s in invalid {
        assert!(issue(&host, &guest, &s).is_err());
    }
    let mut minimum = services(true);
    minimum[1].max_datagram_bytes = 1;
    minimum[1].udp_return_policy = UdpReturnPolicy::AuthorizedDownstream {
        bytes_per_second: 32,
        datagrams_per_second: 1,
    };
    assert!(issue(&host, &guest, &minimum).is_ok());
}

#[test]
fn fragmented_invitation_external_ipv6_requires_both_explicit_opt_ins_without_network() {
    let host = generate_key();
    let guest = generate_key();
    // Classification only: this fixture is never bound or contacted.
    let external: SocketAddr = "[2606:4700::1111]:32000".parse().unwrap();
    let invitation = Invitation::issue_fragmented_external_ipv6(
        &host,
        guest.public(),
        "external-v2",
        external,
        &services(true),
        unix_millis() + 60_000,
    )
    .unwrap();
    assert!(
        Invitation::verify_fragmented_external_ipv6(
            &invitation.encode(),
            host.public(),
            guest.public()
        )
        .is_ok()
    );
    assert!(
        Invitation::verify_external_ipv6(&invitation.encode(), host.public(), guest.public())
            .is_err()
    );
    assert!(
        Invitation::verify_fragmented(&invitation.encode(), host.public(), guest.public()).is_err()
    );
    assert!(
        Invitation::issue_fragmented(
            &host,
            guest.public(),
            "external-v2",
            external,
            &services(true),
            unix_millis() + 60_000
        )
        .is_err()
    );
    let local = issue(&host, &guest, &services(true)).unwrap();
    assert!(
        Invitation::verify_fragmented_external_ipv6(&local.encode(), host.public(), guest.public())
            .is_err()
    );
    for rejected in [
        "[::1]:32000",
        "[2001:db8::1]:32000",
        "[fe80::1]:32000",
        "[2606:4700::1111]:0",
    ] {
        assert!(
            Invitation::issue_fragmented_external_ipv6(
                &host,
                guest.public(),
                "external-v2",
                rejected.parse().unwrap(),
                &services(true),
                unix_millis() + 60_000
            )
            .is_err()
        );
    }
}

#[test]
fn fragmented_invitation_preserves_lifetime_and_input_bounds() {
    let host = generate_key();
    let guest = generate_key();
    let now = unix_millis();
    for expires in [now, now.saturating_sub(1), now + 601_000] {
        assert!(
            Invitation::issue_fragmented(
                &host,
                guest.public(),
                "ttl-v2",
                "127.0.0.1:32000".parse().unwrap(),
                &services(true),
                expires
            )
            .is_err()
        );
    }
    let invitation = issue(&host, &guest, &services(true)).unwrap();
    assert!(
        Invitation::verify_fragmented(&"x".repeat(4097), host.public(), guest.public()).is_err()
    );
    let unknown = invitation.encode().replace(
        "\"schemaVersion\":1",
        "\"schemaVersion\":1,\"profile\":\"fragmentedV2\"",
    );
    assert!(Invitation::verify_fragmented(&unknown, host.public(), guest.public()).is_err());
    let duplicate = invitation.encode().replace(
        "\"schemaVersion\":1",
        "\"schemaVersion\":1,\"schemaVersion\":1",
    );
    assert!(Invitation::verify_fragmented(&duplicate, host.public(), guest.public()).is_err());
}

#[tokio::test]
async fn legacy_entrypoints_reject_an_already_verified_fragmented_object_before_transport() {
    let host = generate_key();
    let guest = generate_key();
    // Keep ownership of the transport destination even if a gate regresses.
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    reservation.set_nonblocking(true).unwrap();
    let invitation = Invitation::issue_fragmented(
        &host,
        guest.public(),
        "fragmented-profile",
        reservation.local_addr().unwrap(),
        &services(true),
        unix_millis() + 60_000,
    )
    .unwrap();
    let limits = Limits {
        handshake_timeout: Duration::from_millis(200),
        ..Limits::default()
    };
    let config = HostConfig {
        secret_key: host,
        bind_addr: invitation.direct_addr(),
        session_id: invitation.session_id().into(),
        services: services(true),
        leases: vec![invitation.lease()],
        limits: limits.clone(),
    };
    match Host::start_invited(config, &invitation).await {
        Err(Error::Configuration) => {}
        Ok(host) => {
            host.shutdown().await;
            panic!("legacy host accepted V2");
        }
        Err(_) => panic!("legacy host failed after profile gate"),
    }
    match Guest::connect_invited(guest, &invitation, limits).await {
        Err(Error::Configuration) => {}
        Ok(guest) => {
            guest.shutdown().await;
            panic!("legacy guest accepted V2");
        }
        Err(_) => panic!("legacy guest failed after profile gate"),
    }
    let mut packet = [0; 2048];
    assert_eq!(
        reservation.recv(&mut packet).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
