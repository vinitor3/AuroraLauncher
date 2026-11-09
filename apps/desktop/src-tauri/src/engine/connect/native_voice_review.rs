//! Exact86 preimage and retained schema3 authorities. None can be exported or
//! confirmed until the separate Host/terminal-withdrawal consumer is complete.
use super::*;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Participant {
    pub(crate) participant_id: String,
    pub(crate) guest_public_id: String,
    pub(crate) invitation_digest: String,
    pub(crate) admission_expires_at_ms: u64,
    pub(crate) session_expires_at_ms: u64,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Service {
    pub(crate) service_id: String,
    pub(crate) protocol: lab::Protocol,
    pub(crate) revision: u32,
    pub(crate) required_capability: lab::Capability,
    pub(crate) max_datagram_bytes: usize,
    pub(crate) return_policy: &'static str,
    pub(crate) return_bytes_per_second: u32,
    pub(crate) return_frames_per_second: u32,
}
/// Field order is part of native86's new exclusive compact JSON preimage.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReviewData {
    pub(crate) operation_id: String,
    pub(crate) generation: u64,
    pub(crate) review_id: String,
    pub(crate) host_public_id: String,
    pub(crate) mode: ConnectMode,
    pub(crate) direct_address: String,
    pub(crate) game_target: String,
    pub(crate) udp_target: String,
    pub(crate) announcement_policy: &'static str,
    pub(crate) services: Vec<Service>,
    pub(crate) participants: Vec<Participant>,
    pub(crate) expires_at_ms: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct NativeReview {
    pub(crate) deadline: Instant,
    pub(crate) digest: String,
    pub(crate) data: ReviewData,
    pub(crate) original_copy_id: String,
    pub(crate) profile: &'static str,
    pub(crate) minecraft: &'static str,
    pub(crate) loader: &'static str,
    pub(crate) loader_version: &'static str,
    pub(crate) artifact_version: &'static str,
    pub(crate) binding_policy: &'static str,
    pub(crate) withdrawal: &'static str,
}
pub(super) struct RetainedReview {
    pub(super) review: NativeReview,
    pub(super) invitations: Vec<lab::SessionInvitation>,
    pub(super) deadline: Instant,
}
fn preimage(owner: &str, identity: (uuid::Uuid, &str), data: &ReviewData) -> Result<Vec<u8>> {
    serde_json::to_vec(&(
        "aurora.connect.desktop.native-voice-static-group/v1",
        owner,
        identity,
        data,
    ))
    .map_err(|_| ConnectError::Internal.into())
}
pub(super) fn digest(
    owner: &str,
    binding: &NativeVoiceHostBinding,
    data: &ReviewData,
) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(preimage(owner, binding.identity_preimage(), data)?)
    ))
}

pub(super) fn publish(
    shared: &Mutex<State>,
    owner: &str,
    id: &str,
    generation: u64,
    window: Instant,
    result: Observation,
) -> Result<NativeReview> {
    let mut state = lock(shared)?;
    let checked = check(&mut state, owner, id, generation, "nativeReviewing");
    let a = selected(&mut state, owner, id)?;
    let result = checked
        .and_then(|()| final_observation(a, window, result))
        .and_then(|sample| issue(a, owner, id, sample, window));
    if let Err(error) = result {
        if a.snapshot.phase == "nativeReviewing" {
            fail(a, error);
        }
    }
    result
}
fn issue(
    a: &mut Attempt,
    owner: &str,
    id: &str,
    sample: VoiceRuntimeSample,
    window: Instant,
) -> Result<NativeReview> {
    let operation = a
        .native_voice
        .as_ref()
        .ok_or(ConnectError::StaleOperation)?;
    if operation.review.is_some() {
        return Err(ConnectError::StaleOperation.into());
    }
    let now = Instant::now();
    let mut deadline = (now + Duration::from_secs(60))
        .min(a.prepared_deadline)
        .min(operation.binding.deadline());
    let mut expires = a
        .snapshot
        .expires_at_ms
        .ok_or(ConnectError::Internal)?
        .min(lab::unix_millis().saturating_add(60_000));
    let mut invitations = Vec::with_capacity(operation.roster.len());
    let mut participants = Vec::with_capacity(operation.roster.len());
    for request in &operation.roster {
        let invitation = lab::SessionInvitation::issue_fragmented_loopback_for_lifetimes(
            &operation.key,
            peer(&request.guest_public_id)?,
            id,
            operation.bind,
            &operation.services,
            Duration::from_secs(request.admission_seconds.into()),
            Duration::from_secs(request.session_seconds.into()),
        )
        .map_err(|_| ConnectError::InvitationRejected)?;
        expires = expires.min(invitation.admission_expires_at_unix_ms());
        deadline = deadline.min(
            Instant::now()
                + invitation
                    .admission_remaining()
                    .map_err(|_| ConnectError::PlanExpired)?,
        );
        participants.push(Participant {
            participant_id: uuid::Uuid::new_v4().to_string(),
            guest_public_id: request.guest_public_id.clone(),
            invitation_digest: invitation.digest(),
            admission_expires_at_ms: invitation.admission_expires_at_unix_ms(),
            session_expires_at_ms: invitation.session_expires_at_unix_ms(),
        });
        invitations.push(invitation);
    }
    let (game, udp) = operation.binding.targets();
    let services = operation
        .services
        .iter()
        .map(|service| {
            let (return_policy, bytes, frames) = match service.udp_return_policy {
                lab::UdpReturnPolicy::ReplyBudget => ("replyBudget", 0, 0),
                lab::UdpReturnPolicy::AuthorizedDownstream {
                    bytes_per_second,
                    datagrams_per_second,
                } => (
                    "authorizedDownstream",
                    bytes_per_second,
                    datagrams_per_second,
                ),
            };
            Service {
                service_id: service.service_id.clone(),
                protocol: service.protocol,
                revision: service.revision,
                required_capability: service.required_capability,
                max_datagram_bytes: service.max_datagram_bytes,
                return_policy,
                return_bytes_per_second: bytes,
                return_frames_per_second: frames,
            }
        })
        .collect();
    let data = ReviewData {
        operation_id: id.into(),
        generation: generation_after(a),
        review_id: uuid::Uuid::new_v4().to_string(),
        host_public_id: operation.key.public().to_string(),
        mode: ConnectMode::Loopback,
        direct_address: operation.bind.to_string(),
        game_target: game.to_string(),
        udp_target: udp.to_string(),
        announcement_policy: match sample.announcement {
            crate::engine::connect_voice::dedicated::VoiceRuntimeAnnouncement::DefaultUnchanged => "defaultUnchanged",
            crate::engine::connect_voice::dedicated::VoiceRuntimeAnnouncement::Loopback35554Durable => "loopback35554Durable",
        },
        services,
        participants,
        expires_at_ms: expires,
    };
    let result = NativeReview {
        deadline,
        digest: digest(owner, &operation.binding, &data)?,
        data,
        original_copy_id: operation.binding.copy_id()?,
        profile: operation.binding.identity_preimage().1,
        minecraft: sample.minecraft,
        loader: sample.loader,
        loader_version: sample.loader_version,
        artifact_version: sample.artifact_version,
        binding_policy: "ipv4Literal",
        withdrawal:
            "aux/revision1 is terminally withdrawn on observation failure; audio is unverified",
    };
    // Issuance and hashing never renew any gate. Recheck the original native
    // authority and every original invitation before publishing the full roster.
    operation.binding.check_current()?;
    if Instant::now() >= window {
        return Err(NativeError::ObservationWindowElapsed);
    }
    if a.cancel.is_cancelled() {
        return Err(ConnectError::Cancelled.into());
    }
    if Instant::now() >= deadline
        || lab::unix_millis() >= expires
        || invitations
            .iter()
            .any(|i| i.check_admission_current().is_err())
    {
        return Err(ConnectError::PlanExpired.into());
    }
    a.native_voice.as_mut().unwrap().review = Some(RetainedReview {
        review: result.clone(),
        invitations,
        deadline,
    });
    a.snapshot.phase = "nativeReviewed";
    a.snapshot.generation = result.data.generation;
    a.snapshot.expires_at_ms = Some(expires);
    Ok(result)
}
fn generation_after(a: &Attempt) -> u64 {
    a.snapshot.generation.saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_preimage_is_ordered_exclusive_and_every_authority_field_changes_it() {
        let identity = (uuid::Uuid::nil(), "svc");
        let data = ReviewData {
            operation_id: "operation".into(),
            generation: 2,
            review_id: "review".into(),
            host_public_id: "host".into(),
            mode: ConnectMode::Loopback,
            direct_address: "127.0.0.1:40000".into(),
            game_target: "127.0.0.1:43001".into(),
            udp_target: "127.0.0.1:43002".into(),
            announcement_policy: "defaultUnchanged",
            services: vec![],
            participants: vec![],
            expires_at_ms: 12,
        };
        let original = preimage("owner", identity, &data).unwrap();
        assert_eq!(String::from_utf8(original.clone()).unwrap(), concat!(
            "[\"aurora.connect.desktop.native-voice-static-group/v1\",\"owner\",[\"00000000-0000-0000-0000-000000000000\",\"svc\"],",
            "{\"operation_id\":\"operation\",\"generation\":2,\"review_id\":\"review\",\"host_public_id\":\"host\",\"mode\":\"loopback\",",
            "\"direct_address\":\"127.0.0.1:40000\",\"game_target\":\"127.0.0.1:43001\",\"udp_target\":\"127.0.0.1:43002\",",
            "\"announcement_policy\":\"defaultUnchanged\",\"services\":[],\"participants\":[],\"expires_at_ms\":12}]"));
        assert_ne!(original, preimage("other", identity, &data).unwrap());
        assert_ne!(
            original,
            preimage("owner", (uuid::Uuid::new_v4(), "svc"), &data).unwrap()
        );
        assert_ne!(
            original,
            preimage("owner", (identity.0, "plasmo"), &data).unwrap()
        );
        // Compare each differential against the SAME Value encoder baseline;
        // Value's key ordering alone must not make this proof pass.
        let encoded_value = |value: serde_json::Value| {
            serde_json::to_vec(&(
                "aurora.connect.desktop.native-voice-static-group/v1",
                "owner",
                identity,
                value,
            ))
            .unwrap()
        };
        let value_baseline = encoded_value(serde_json::to_value(&data).unwrap());
        for field in [
            "operation_id",
            "generation",
            "review_id",
            "host_public_id",
            "mode",
            "direct_address",
            "game_target",
            "udp_target",
            "announcement_policy",
            "services",
            "participants",
            "expires_at_ms",
        ] {
            let mut value = serde_json::to_value(&data).unwrap();
            value[field] = serde_json::json!("different");
            assert_ne!(value_baseline, encoded_value(value), "{field}");
        }
        for legacy in [
            "aurora.connect.desktop.static-group/v1",
            "aurora.connect.desktop.owned-static-group/v1",
        ] {
            assert_ne!(
                original,
                serde_json::to_vec(&(legacy, "owner", identity, &data)).unwrap()
            );
        }
    }
}
