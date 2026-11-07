//! Real native source +71Host + two independently authenticated Guests. The
//! payloads below are fixture bytes, never voice/mod messages or recorded audio.
use super::voice_fixture::{VoiceFixture, PROFILES};
use super::voice_producer_fixture::{Action, Harness, WAIT};
use super::voice_runtime::{limits, prepare, ready, restored, Running};
use super::voice_transport_fixture::{TCP_MAGIC, UDP_MAGIC};
use super::*;
use crate::engine::connect::{ConnectManager, GroupParticipantRequest};
use aurora_connect_lab as lab;
use producer::TestHooks;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(WAIT, future)
        .await
        .expect("native86_host_fixture_deadline")
}
async fn work_finished(manager: &ConnectManager) {
    bounded(async {
        while !manager.native_voice_work_finished_for_test() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
}
async fn chunk(
    send: &mut (impl AsyncWrite + Unpin),
    recv: &mut (impl AsyncRead + Unpin),
    bytes: &[u8],
) {
    let length = (bytes.len() as u16).to_be_bytes();
    bounded(async {
        send.write_all(&length).await.unwrap();
        send.write_all(bytes).await.unwrap();
        let mut echo_length = [0; 2];
        recv.read_exact(&mut echo_length).await.unwrap();
        assert_eq!(echo_length, length);
        let mut echo = vec![0; bytes.len()];
        recv.read_exact(&mut echo).await.unwrap();
        assert_eq!(echo, bytes);
    })
    .await;
}
fn datagram(nonce: Uuid, guest: u8, source: u8, sequence: u8) -> Vec<u8> {
    let mut bytes = vec![sequence; 4096];
    bytes[..4].copy_from_slice(UDP_MAGIC);
    bytes[4..20].copy_from_slice(nonce.as_bytes());
    bytes[20] = guest;
    bytes[21] = source;
    bytes[22] = sequence;
    bytes[23] = 0;
    bytes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_host_two_guests_keep_tcp_during_udp_retirement_without_reactivation() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let mut runtime_limits = limits();
        runtime_limits.lifetime_ms = 20_000;
        let (plan, paths, _) = prepare(
            &fixture,
            &harness,
            Arc::new(TestHooks::default()),
            Hooks::default(),
            runtime_limits,
        );
        let running = Running::start(plan, CancellationToken::new());
        harness.ready();
        ready(running.control()).await;
        let manager = ConnectManager::default();
        let binding = running.control().native_voice_host_binding().unwrap();
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = held.local_addr().unwrap();
        let keys = [lab::generate_key(), lab::generate_key()];
        let roster = keys
            .iter()
            .map(|key| GroupParticipantRequest {
                guest_public_id: key.public().to_string(),
                admission_seconds: 15,
                session_seconds: 18,
            })
            .collect();
        let preparation = manager
            .prepare_native_voice_group("main", binding.clone(), bind, roster)
            .unwrap()
            .wait()
            .await
            .unwrap();
        work_finished(&manager).await;
        let reviewed = manager
            .review_native_voice_group("main", &preparation.operation_id, preparation.generation)
            .unwrap()
            .wait()
            .await
            .unwrap();
        work_finished(&manager).await;
        assert!(manager
            .confirm_native_voice_group(
                "main",
                &preparation.operation_id,
                reviewed.data.generation,
                "incorrect"
            )
            .is_err());
        assert!(manager
            .confirm_native_voice_group(
                "main",
                &preparation.operation_id,
                reviewed.data.generation + 1,
                &reviewed.digest
            )
            .is_err());
        drop(held);
        let bound = bounded(
            manager
                .confirm_native_voice_group(
                    "main",
                    &preparation.operation_id,
                    reviewed.data.generation,
                    &reviewed.digest,
                )
                .unwrap()
                .wait(),
        )
        .await
        .unwrap();
        assert_eq!(bound.operation_id, preparation.operation_id);
        assert_eq!(bound.participants, 2);
        assert!(manager
            .confirm_native_voice_group(
                "main",
                &preparation.operation_id,
                reviewed.data.generation,
                &reviewed.digest
            )
            .is_err());
        let mut guests = Vec::new();
        let mut authorities = Vec::new();
        for (index, key) in keys.iter().enumerate() {
            let exported = manager
                .export_native_voice_invitation(
                    "main",
                    &preparation.operation_id,
                    &reviewed.data.participants[index].participant_id,
                )
                .unwrap();
            let invitation = lab::SessionInvitation::verify_fragmented_loopback(
                &exported,
                preparation.host_public_id.parse().unwrap(),
                key.public(),
            )
            .unwrap();
            assert_eq!(
                invitation.digest(),
                reviewed.data.participants[index].invitation_digest
            );
            guests.push(
                bounded(lab::Guest::connect_invited_fragmented_session(
                    key.clone(),
                    &invitation,
                    lab::Limits::default(),
                ))
                .await
                .unwrap(),
            );
            authorities.push(invitation);
        }
        let mut streams = Vec::new();
        let mut bridges = Vec::new();
        let mut local_sources = Vec::new();
        let mut payloads = Vec::new();
        for (guest_index, guest) in guests.iter().enumerate() {
            let (mut send, mut recv) = bounded(guest.open_tcp("game", 1)).await.unwrap();
            send.write_all(&[TCP_MAGIC.as_slice(), harness.nonce().as_bytes()].concat())
                .await
                .unwrap();
            chunk(&mut send, &mut recv, &[guest_index as u8; 4096]).await;
            streams.push((send, recv));
            let bridge =
                bounded(guest.udp_bridge_fragmented("aux", 1, "127.0.0.1:0".parse().unwrap()))
                    .await
                    .unwrap();
            let address = bridge.local_addr();
            for source in 0..2 {
                local_sources.push((
                    tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap(),
                    address,
                ));
                payloads.push(datagram(harness.nonce(), guest_index as u8, source, 1));
            }
            bridges.push(bridge);
        }
        harness.command(Action::HoldUdpReturns);
        for ((socket, address), payload) in local_sources.iter().zip(&payloads) {
            assert_eq!(
                socket.send_to(payload, address).await.unwrap(),
                payload.len()
            );
        }
        // The SAME child acknowledges that all four marked requests reached
        // their real destination and holds their responses. TCP remains open.
        harness.udp_held(4);
        for (index, (send, recv)) in streams.iter_mut().enumerate() {
            chunk(send, recv, &[index as u8 + 17; 4096]).await;
        }
        harness.command(Action::ReleaseUdpReturns);
        for ((socket, address), payload) in local_sources.iter().zip(&payloads) {
            let mut echoed = vec![0; 4097];
            let (length, sender) = bounded(socket.recv_from(&mut echoed)).await.unwrap();
            assert_eq!(sender, *address);
            assert_eq!(&echoed[..length], payload);
        }
        harness.command(Action::RewriteInvalid);
        bounded(async {
            loop {
                let observed = manager
                    .native_voice_host_observation("main", &preparation.operation_id)
                    .unwrap();
                assert!(observed.bound);
                if observed.udp_retired() {
                    assert_eq!(observed.retirement_pending, 0);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        for (index, (send, recv)) in streams.iter_mut().enumerate() {
            chunk(send, recv, &[index as u8 + 29; 4096]).await;
        }
        for guest in &guests {
            assert!(matches!(
                bounded(guest.open_udp_fragmented("aux", 1)).await,
                Err(lab::Error::ServiceDenied)
            ));
            assert!(guest.connection().close_reason().is_none());
        }
        // All prior responses were drained above. A newly marked payload cannot
        // make a retired service receive/reopen authority after restoration.
        harness.command(Action::RestoreValid);
        assert!(binding.reserve_voice().unwrap().execute().is_ok());
        assert!(manager
            .native_voice_host_observation("main", &preparation.operation_id)
            .unwrap()
            .udp_retired());
        for invitation in &authorities {
            assert!(invitation.check_session_current().is_ok());
        }
        for (index, (socket, address)) in local_sources.iter().enumerate() {
            let payload = datagram(harness.nonce(), (index / 2) as u8, (index % 2) as u8, 2);
            // The existing protocol may retain this remote association. A
            // local send result alone is neither delivery nor retirement proof.
            assert_eq!(
                socket.send_to(&payload, address).await.unwrap(),
                payload.len()
            );
            let mut echoed = [0; 4097];
            assert!(!matches!(
                tokio::time::timeout(Duration::from_millis(100), socket.recv_from(&mut echoed))
                    .await,
                Ok(Ok(_))
            ));
        }
        for (index, guest) in guests.iter().enumerate() {
            assert!(matches!(
                bounded(guest.open_udp_fragmented("aux", 1)).await,
                Err(lab::Error::ServiceDenied)
            ));
            assert!(matches!(
                bounded(guest.udp_bridge_fragmented("aux", 1, "127.0.0.1:0".parse().unwrap()))
                    .await,
                Err(lab::Error::ServiceDenied)
            ));
            let (mut send, mut recv) = bounded(guest.open_tcp("game", 1)).await.unwrap();
            send.write_all(&[TCP_MAGIC.as_slice(), harness.nonce().as_bytes()].concat())
                .await
                .unwrap();
            chunk(&mut send, &mut recv, &[index as u8 + 41; 1024]).await;
            chunk(&mut send, &mut recv, &[]).await;
            send.finish().unwrap();
        }
        for (send, recv) in &mut streams {
            chunk(send, recv, &[]).await;
            send.finish().unwrap();
        }
        for bridge in &bridges {
            bridge.shutdown().await;
        }
        for guest in &guests {
            guest.shutdown().await;
        }
        drop(bridges);
        drop(local_sources);
        drop(guests);
        assert_eq!(
            manager
                .stop("main", &preparation.operation_id)
                .await
                .unwrap()
                .cleanup_state,
            "complete"
        );
        drop(std::net::UdpSocket::bind(bind).expect("native86_transport_rebind"));
        // Connect shutdown did not stop the borrowed DedicatedRun.
        assert!(binding.reserve_voice().unwrap().execute().is_ok());
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
    }
}

#[cfg(debug_assertions)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_lease_seals_before_first_owner_drains_and_stop_releases_transport_before_query() {
    use super::voice_runtime::{Gate, Release};
    use std::sync::atomic::AtomicBool;
    for profile in PROFILES {
        for stage in [
            lab::NativeVoiceRetirementStage::AfterConnect,
            lab::NativeVoiceRetirementStage::Completed,
        ] {
            let fixture = VoiceFixture::new(profile);
            let mut harness = Harness::new(fixture.stored_profile());
            let query_gate = Arc::new(Gate::default());
            let armed = Arc::new(AtomicBool::new(false));
            let called = query_gate.clone();
            let active = armed.clone();
            let hooks = Arc::new(TestHooks {
                point: Some(Arc::new(move |point| {
                    if point == producer::ProducerPoint::VoiceBeforeFinalGate
                        && active.load(Ordering::Acquire)
                    {
                        called.reach();
                    }
                })),
                ..TestHooks::default()
            });
            let mut runtime_limits = limits();
            runtime_limits.lifetime_ms = 20_000;
            let (plan, paths, _) =
                prepare(&fixture, &harness, hooks, Hooks::default(), runtime_limits);
            let running = Running::start(plan, CancellationToken::new());
            let _release_query = Release(query_gate.clone());
            harness.ready();
            ready(running.control()).await;
            let binding = running.control().native_voice_host_binding().unwrap();
            let manager = Arc::new(ConnectManager::default());
            let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            let bind = held.local_addr().unwrap();
            let keys = [lab::generate_key(), lab::generate_key()];
            let roster = keys
                .iter()
                .map(|key| GroupParticipantRequest {
                    guest_public_id: key.public().to_string(),
                    admission_seconds: 15,
                    session_seconds: 18,
                })
                .collect();
            let prepared = manager
                .prepare_native_voice_group("main", binding.clone(), bind, roster)
                .unwrap()
                .wait()
                .await
                .unwrap();
            work_finished(&manager).await;
            let reviewed = manager
                .review_native_voice_group("main", &prepared.operation_id, prepared.generation)
                .unwrap()
                .wait()
                .await
                .unwrap();
            work_finished(&manager).await;
            drop(held);
            bounded(
                manager
                    .confirm_native_voice_group(
                        "main",
                        &prepared.operation_id,
                        reviewed.data.generation,
                        &reviewed.digest,
                    )
                    .unwrap()
                    .wait(),
            )
            .await
            .unwrap();
            let mut guests = Vec::new();
            for (index, key) in keys.iter().enumerate() {
                let exported = manager
                    .export_native_voice_invitation(
                        "main",
                        &prepared.operation_id,
                        &reviewed.data.participants[index].participant_id,
                    )
                    .unwrap();
                let invitation = lab::SessionInvitation::verify_fragmented_loopback(
                    &exported,
                    prepared.host_public_id.parse().unwrap(),
                    key.public(),
                )
                .unwrap();
                guests.push(
                    bounded(lab::Guest::connect_invited_fragmented_session(
                        key.clone(),
                        &invitation,
                        lab::Limits::default(),
                    ))
                    .await
                    .unwrap(),
                );
            }
            let owner_gate = manager
                .native_voice_retirement_gate_for_test(
                    "main",
                    &prepared.operation_id,
                    &reviewed.data.participants[0].participant_id,
                    stage,
                )
                .unwrap();
            let mut pending_open = None;
            let mut retained = None;
            match stage {
                lab::NativeVoiceRetirementStage::AfterConnect => {
                    let guest = guests[0].clone();
                    pending_open = Some(tokio::spawn(async move {
                        guest.open_udp_fragmented("aux", 1).await
                    }));
                }
                lab::NativeVoiceRetirementStage::Completed => {
                    let association = bounded(guests[0].open_udp_fragmented("aux", 1))
                        .await
                        .unwrap();
                    bounded(association.send(&datagram(harness.nonce(), 0, 0, 5)))
                        .await
                        .unwrap();
                    retained = Some(association);
                }
            }
            bounded(owner_gate.entered()).await;
            let association_port = owner_gate
                .socket_addr()
                .expect("native86_real_association_socket");
            assert!(std::net::UdpSocket::bind(association_port).is_err());
            let (mut game_send, mut game_recv) =
                bounded(guests[1].open_tcp("game", 1)).await.unwrap();
            game_send
                .write_all(&[TCP_MAGIC.as_slice(), harness.nonce().as_bytes()].concat())
                .await
                .unwrap();
            chunk(&mut game_send, &mut game_recv, &[83; 1024]).await;
            let second_udp = bounded(guests[1].open_udp_fragmented("aux", 1))
                .await
                .unwrap();
            let marker = datagram(harness.nonce(), 1, 0, 7);
            bounded(second_udp.send(&marker)).await.unwrap();
            assert_eq!(bounded(second_udp.recv()).await.unwrap().as_ref(), &marker);
            armed.store(true, Ordering::Release);
            query_gate.reached();
            // Fixed2s window expires while the real query remains reserved.
            bounded(async {
                loop {
                    let observed = manager
                        .native_voice_host_observation("main", &prepared.operation_id)
                        .unwrap();
                    if observed.udp_retiring() && observed.retirement_pending > 0 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await;
            // First real resource is still bound, yet the second lease's
            // separate81 gate has already denied a NEW request.
            bounded(async {
                while !manager
                    .native_voice_service_disabled_for_test(
                        "main",
                        &prepared.operation_id,
                        &reviewed.data.participants[1].participant_id,
                    )
                    .unwrap()
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            assert!(std::net::UdpSocket::bind(association_port).is_err());
            assert!(matches!(
                bounded(guests[1].open_udp_fragmented("aux", 1)).await,
                Err(lab::Error::ServiceDenied)
            ));
            assert!(!owner_gate.timed_out());
            assert!(binding.reserve_tcp().is_err());
            chunk(&mut game_send, &mut game_recv, &[89; 1024]).await;
            owner_gate.release();
            if let Some(pending) = pending_open {
                assert!(bounded(pending).await.unwrap().is_err());
            }
            bounded(async {
                while !manager
                    .native_voice_host_observation("main", &prepared.operation_id)
                    .unwrap()
                    .udp_retired()
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            assert!(!owner_gate.timed_out());
            drop(
                std::net::UdpSocket::bind(association_port)
                    .expect("native86_original_association_rebind"),
            );
            let mut stopping = {
                let manager = manager.clone();
                let id = prepared.operation_id.clone();
                tokio::spawn(async move { manager.stop("main", &id).await })
            };
            bounded(async {
                loop {
                    if let Ok(socket) = std::net::UdpSocket::bind(bind) {
                        drop(socket);
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            assert!(
                tokio::time::timeout(Duration::from_millis(40), &mut stopping)
                    .await
                    .is_err()
            );
            assert_eq!(manager.snapshot("main").unwrap().cleanup_state, "pending");
            stopping.abort();
            assert!(stopping.await.unwrap_err().is_cancelled());
            assert!(manager
                .prepare_native_voice_group(
                    "main",
                    binding.clone(),
                    bind,
                    keys.iter()
                        .map(|key| GroupParticipantRequest {
                            guest_public_id: key.public().to_string(),
                            admission_seconds: 10,
                            session_seconds: 12
                        })
                        .collect()
                )
                .is_err());
            query_gate.release();
            assert_eq!(
                manager
                    .stop("main", &prepared.operation_id)
                    .await
                    .unwrap()
                    .cleanup_state,
                "complete"
            );
            if let Some(association) = retained {
                association.shutdown().await;
            }
            second_udp.shutdown().await;
            for guest in &guests {
                guest.shutdown().await;
            }
            drop(guests);
            assert!(binding.reserve_voice().unwrap().execute().is_ok());
            assert_eq!(bytes(&target(&paths)), EULA_TRUE);
            let report = running.finish().await;
            restored(&paths, &report);
            harness.ports_released();
            harness.finish();
        }
    }
}
