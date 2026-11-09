//! The V3 entry point consumes the native installer's original capability.
//! Paths and executable arguments are never accepted as caller authority here.
use super::super::{manifest::ServerRuntimeV3, server_preparation::PreparedServerResources};
use super::*;

#[cfg(all(test, feature = "connect-voice-lab"))]
#[path = "tests/actual_server_v3.rs"]
mod actual_tests;

pub(in super::super) fn runtime_bytes(server: Option<&ServerRuntimeV3>) -> Result<Vec<u8>> {
    let Some(server) = server else {
        return Ok(RUNTIME.to_vec());
    };
    let runtime = server.runtime()?;
    let descriptor = runtime.descriptor();
    encode(
        &serde_json::json!({ "minecraft": descriptor.minecraft_version, "loader": descriptor.loader, "versionId": runtime.version_id() }),
    )
}

pub(in super::super) fn plan_voice_server_factory_at(
    lab: &Path,
    prepared: PreparedServerResources,
) -> Result<FactoryPlan> {
    prepared.verify()?;
    explicit_path(lab)?;
    let profile = prepared.voice_profile();
    let runtime = prepared.runtime();
    let descriptor = runtime.descriptor();
    let server = ServerRuntimeV3 {
        runtime_profile: runtime.version_id().into(),
        java_major: descriptor.java_major,
        recipe_args: prepared.recipe_args().to_vec(),
        provenance: prepared.provenance().clone(),
    };
    let stage = prepared.stage().to_path_buf();
    let java = prepared.java_root().to_path_buf();
    let pins = recovery::pin_directory(lab)?;
    let resources = resources::Resources::inspect_v3(lab, prepared)?;
    let id = BundleId(Uuid::new_v4());
    let review_id = Uuid::new_v4();
    let destination = lab.join(format!("native-status-{}", id.0));
    absent(&destination)?;
    let stored = profile.stored();
    let policy = ObservationPolicyV2::LiteralLoopback35554Durable;
    let record_bytes = serde_json::to_vec(&resources.records).map_err(|_| Error::Identity)?;
    if record_bytes.len() as u64 > super::super::manifest::MAX_V3_MANIFEST_BYTES {
        return Err(Error::Limit);
    }
    let data = encode(&(
        3,
        "native_voice_server_factory_v3",
        review_id,
        id,
        directory_identity(lab)?,
        destination.to_str().ok_or(Error::UnsafePath)?,
        stored,
        &server,
        digest(&record_bytes),
        stored.configuration_name(),
        stored.configuration_sha256(),
        policy,
        policy.configuration_sha256(stored),
        digest(resources::BRIDGE_PROPERTIES),
        digest(EULA_FALSE),
    ))?;
    #[cfg(test)]
    let hooks = Hooks::default();
    #[cfg(not(test))]
    let hooks = Hooks;
    Ok(FactoryPlan { lab: lab.into(), id, resources, deadline: Instant::now() + REVIEW_TTL, hooks,
        kind: ManifestKind::VoiceV3(stored), voice_policy: policy, server: Some(server),
        #[cfg(test)] test_expected_pins: None, _pins: pins,
        review: Review { review_id, digest: digest(&data), purpose: "native_voice_server_factory_v3", bundle_id: id,
            before: String::from_utf8(stored.configuration().to_vec()).map_err(|_| Error::Identity)?,
            after: String::from_utf8(policy.configuration(stored).to_vec()).map_err(|_| Error::Identity)?,
            details: vec![destination.display().to_string(), stage.display().to_string(), java.display().to_string(),
                format!("Minecraft {}; {} {}; Java {}; {:?}. Arquivos verificados pelo preparo nativo original.", descriptor.minecraft_version, descriptor.loader, descriptor.loader_version, descriptor.java_major, profile),
                "Criar uma cópia nova; EULA false; nenhum servidor iniciado nesta etapa.".into(),
                "Jogo loopback43001; autenticação de transporte Aurora; até8 jogadores; anúncio de voz127.0.0.1:35554 com recibo durável de restauração.".into()] } })
}
