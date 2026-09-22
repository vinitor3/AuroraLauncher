//! Public Java skin lookup from the user's network when a cloud provider is unavailable.
//! No account token or provider key participates in this read-only command.

use std::{
    io::Read,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{blocking::Client, redirect::Policy, StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_JSON_BYTES: u64 = 128 * 1024;
const MAX_PNG_BYTES: u64 = 256 * 1024;
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePlayerSkin {
    name: String,
    uuid: String,
    model: &'static str,
    skin_url: String,
    texture_url: String,
    content_hash: String,
    source: &'static str,
    stale: bool,
}

#[derive(Deserialize)]
struct PublicIdentity {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct SessionProfile {
    id: String,
    name: String,
    properties: Vec<ProfileProperty>,
}

#[derive(Deserialize)]
struct ProfileProperty {
    name: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TexturesProperty {
    profile_id: String,
    profile_name: Option<String>,
    textures: PublicTextures,
}

#[derive(Deserialize)]
struct PublicTextures {
    #[serde(rename = "SKIN")]
    skin: Option<PublicSkin>,
}

#[derive(Deserialize)]
struct PublicSkin {
    url: String,
    metadata: Option<SkinMetadata>,
}

#[derive(Deserialize)]
struct SkinMetadata {
    model: Option<String>,
}

fn valid_name(value: &str) -> bool {
    (1..=16).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn normalized_uuid(value: &str) -> Option<String> {
    let valid = match value.len() {
        32 => value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        36 => value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        }),
        _ => false,
    };
    valid.then(|| value.replace('-', "").to_ascii_lowercase())
}

fn normalized_identifier(identifier: &str) -> Result<(String, Option<String>), String> {
    let identifier = identifier.trim();
    if let Some(uuid) = normalized_uuid(identifier) {
        return Ok((uuid, None));
    }
    if valid_name(identifier) {
        return Ok((identifier.to_owned(), Some(identifier.to_owned())));
    }
    Err("Informe o nome ou UUID da conta Minecraft Java.".to_owned())
}

fn official_texture_url(value: &str) -> Result<String, String> {
    let invalid = || "O perfil oficial retornou uma origem de skin inválida.".to_owned();
    if value.len() > 256 {
        return Err(invalid());
    }
    let mut url = Url::parse(value).map_err(|_| invalid())?;
    let hash = url.path().strip_prefix("/texture/").ok_or_else(invalid)?;
    if url.host_str() != Some("textures.minecraft.net")
        || !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !(32..=64).contains(&hash.len())
        || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid());
    }
    url.set_scheme("https").map_err(|_| invalid())?;
    Ok(url.to_string())
}

fn identity_from_json(bytes: &[u8], requested_name: &str) -> Result<PublicIdentity, String> {
    let identity: PublicIdentity = serde_json::from_slice(bytes)
        .map_err(|_| "A fonte oficial retornou um perfil inválido.".to_owned())?;
    if !valid_name(&identity.name) || !identity.name.eq_ignore_ascii_case(requested_name) {
        return Err("O perfil oficial não corresponde ao jogador solicitado.".to_owned());
    }
    let id = normalized_uuid(&identity.id)
        .ok_or_else(|| "A fonte oficial retornou um UUID inválido.".to_owned())?;
    Ok(PublicIdentity {
        id,
        name: identity.name,
    })
}

fn skin_from_session(
    bytes: &[u8],
    uuid: &str,
    requested_name: Option<&str>,
) -> Result<NativePlayerSkin, String> {
    let profile: SessionProfile = serde_json::from_slice(bytes)
        .map_err(|_| "A fonte oficial retornou um perfil inválido.".to_owned())?;
    if normalized_uuid(&profile.id).as_deref() != Some(uuid)
        || !valid_name(&profile.name)
        || requested_name.is_some_and(|name| !profile.name.eq_ignore_ascii_case(name))
    {
        return Err("O perfil oficial não corresponde ao jogador solicitado.".to_owned());
    }
    let mut properties = profile
        .properties
        .iter()
        .filter(|property| property.name == "textures");
    let property = properties
        .next()
        .ok_or_else(|| "O perfil oficial não retornou uma skin pública.".to_owned())?;
    if properties.next().is_some() || property.value.len() > 32_768 {
        return Err("A textura do perfil oficial é inválida.".to_owned());
    }
    let decoded = STANDARD
        .decode(&property.value)
        .map_err(|_| "A textura do perfil oficial é inválida.".to_owned())?;
    let texture: TexturesProperty = serde_json::from_slice(&decoded)
        .map_err(|_| "A textura do perfil oficial é inválida.".to_owned())?;
    if normalized_uuid(&texture.profile_id).as_deref() != Some(uuid)
        || texture
            .profile_name
            .is_some_and(|name| !name.eq_ignore_ascii_case(&profile.name))
    {
        return Err("A textura retornada pertence a outro jogador.".to_owned());
    }
    let skin = texture
        .textures
        .skin
        .ok_or_else(|| "Este jogador não possui uma skin pública personalizada.".to_owned())?;
    let texture_url = official_texture_url(&skin.url)?;
    let model = match skin.metadata.and_then(|metadata| metadata.model).as_deref() {
        None | Some("classic") => "classic",
        Some("slim") => "slim",
        Some(_) => return Err("O perfil oficial retornou um modelo de skin inválido.".to_owned()),
    };
    Ok(NativePlayerSkin {
        name: profile.name,
        uuid: uuid.to_owned(),
        model,
        skin_url: texture_url.clone(),
        texture_url,
        content_hash: String::new(),
        source: "mojang-local",
        stale: false,
    })
}

fn read_bounded(mut reader: impl Read, maximum: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Não foi possível receber a resposta oficial. Tente novamente.".to_owned())?;
    if bytes.len() as u64 > maximum {
        return Err("A resposta oficial excedeu o tamanho permitido.".to_owned());
    }
    Ok(bytes)
}

fn status_error(status: StatusCode, is_texture: bool) -> Option<String> {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Some(
            "Muitas consultas à fonte oficial. Aguarde um minuto e tente novamente.".to_owned(),
        );
    }
    if status == StatusCode::NOT_FOUND || status == StatusCode::NO_CONTENT {
        return Some(
            if is_texture {
                "A skin pública não está disponível agora."
            } else {
                "Jogador não encontrado."
            }
            .to_owned(),
        );
    }
    (!status.is_success())
        .then(|| "A fonte oficial de jogadores está temporariamente indisponível.".to_owned())
}

fn fetch_bounded(
    client: &Client,
    url: &str,
    maximum: u64,
    deadline: Instant,
    is_texture: bool,
) -> Result<Vec<u8>, String> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| "A consulta oficial demorou para responder. Tente novamente.".to_owned())?;
    let response = client
        .get(url)
        .timeout(remaining.min(REQUEST_TIMEOUT))
        .header(
            "Accept",
            if is_texture {
                "image/png"
            } else {
                "application/json"
            },
        )
        .send()
        .map_err(|error| {
            if error.is_timeout() {
                "A consulta oficial demorou para responder. Tente novamente.".to_owned()
            } else {
                "Não foi possível conectar à fonte oficial de jogadores.".to_owned()
            }
        })?;
    if let Some(error) = status_error(response.status(), is_texture) {
        return Err(error);
    }
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err("A resposta oficial excedeu o tamanho permitido.".to_owned());
    }
    if !is_texture
        && !response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
            })
    {
        return Err("A fonte oficial retornou uma resposta inválida.".to_owned());
    }
    read_bounded(response, maximum)
}

fn validate_skin_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 33
        || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || bytes[8..12] != [0, 0, 0, 13]
        || &bytes[12..16] != b"IHDR"
    {
        return Err("A fonte oficial não retornou uma imagem PNG válida.".to_owned());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("verified PNG width"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("verified PNG height"));
    if width != 64 || ![32, 64].contains(&height) {
        return Err("A skin pública precisa ter 64x64 ou 64x32 pixels.".to_owned());
    }
    Ok(())
}

fn lookup_blocking(identifier: &str) -> Result<NativePlayerSkin, String> {
    let (identifier, name) = normalized_identifier(identifier)?;
    let client = Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(REQUEST_TIMEOUT)
        .user_agent("AuroraLauncher/0.1 PublicSkinLookup")
        .build()
        .map_err(|_| "Não foi possível iniciar a consulta oficial.".to_owned())?;
    let deadline = Instant::now() + LOOKUP_TIMEOUT;
    let uuid = if let Some(name) = name.as_deref() {
        let bytes = fetch_bounded(
            &client,
            &format!("https://api.minecraftservices.com/minecraft/profile/lookup/name/{name}"),
            MAX_JSON_BYTES,
            deadline,
            false,
        )?;
        identity_from_json(&bytes, name)?.id
    } else {
        identifier
    };
    let bytes = fetch_bounded(
        &client,
        &format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"),
        MAX_JSON_BYTES,
        deadline,
        false,
    )?;
    let mut skin = skin_from_session(&bytes, &uuid, name.as_deref())?;
    let image = fetch_bounded(&client, &skin.texture_url, MAX_PNG_BYTES, deadline, true)?;
    validate_skin_png(&image)?;
    skin.content_hash = format!("{:x}", Sha256::digest(&image));
    Ok(skin)
}

#[tauri::command]
pub async fn lookup_player_skin(identifier: String) -> Result<NativePlayerSkin, String> {
    tauri::async_runtime::spawn_blocking(move || lookup_blocking(&identifier))
        .await
        .map_err(|_| "Não foi possível concluir a consulta oficial de skin.".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const UUID: &str = "069a79f444e94726a5befca90e38aaf5";
    fn texture() -> String {
        format!("http://textures.minecraft.net/texture/{}", "a".repeat(64))
    }
    fn session(texture: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({ "id": UUID, "name": "Notch", "properties": [{
            "name": "textures", "value": STANDARD.encode(serde_json::to_vec(&texture).unwrap())
        }] }))
        .unwrap()
    }
    fn textures() -> Value {
        json!({ "profileId": UUID, "profileName": "Notch",
        "textures": { "SKIN": { "url": texture(), "metadata": { "model": "slim" } } } })
    }

    #[test]
    fn identifiers_are_confined_to_names_or_normalized_uuid() {
        assert_eq!(
            normalized_identifier(" Notch ").unwrap(),
            ("Notch".into(), Some("Notch".into()))
        );
        assert_eq!(
            normalized_identifier("069A79F4-44E9-4726-A5BE-FCA90E38AAF5").unwrap(),
            (UUID.into(), None)
        );
        for bad in [
            "",
            "../Notch",
            "Notch?key=x",
            "a".repeat(17).as_str(),
            "SãoPaulo",
            "069a79f4--4e9-4726-a5be-fca90e38aaf5",
        ] {
            assert!(normalized_identifier(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn name_lookup_rejects_wrong_identity_and_malformed_uuid() {
        assert_eq!(
            identity_from_json(
                &serde_json::to_vec(&json!({"id":UUID,"name":"Notch"})).unwrap(),
                "notch"
            )
            .unwrap()
            .id,
            UUID
        );
        for value in [
            json!({"id": UUID, "name": "Other"}),
            json!({"id": "invalid", "name": "Notch"}),
        ] {
            assert!(identity_from_json(&serde_json::to_vec(&value).unwrap(), "Notch").is_err());
        }
    }

    #[test]
    fn session_preserves_model_and_verifies_both_profile_and_texture_owner() {
        let skin = skin_from_session(&session(textures()), UUID, Some("notch")).unwrap();
        assert_eq!(skin.model, "slim");
        assert_eq!(skin.skin_url, texture().replace("http:", "https:"));
        assert_eq!(skin.source, "mojang-local");
        assert!(skin_from_session(&session(textures()), &"b".repeat(32), None).is_err());
        assert!(skin_from_session(&session(textures()), UUID, Some("Other")).is_err());
        for key in ["profileId", "profileName"] {
            let mut invalid = textures();
            invalid[key] = json!("different-owner");
            assert!(skin_from_session(&session(invalid), UUID, None)
                .unwrap_err()
                .contains("outro jogador"));
        }
    }

    #[test]
    fn missing_custom_skin_is_reported_without_inventing_a_default() {
        let mut value = textures();
        value["textures"] = json!({});
        assert!(skin_from_session(&session(value), UUID, None)
            .unwrap_err()
            .contains("não possui"));
        let invalid = json!({"id":UUID,"name":"Notch","properties":[{"name":"textures","value":"not base64"}]});
        assert!(skin_from_session(&serde_json::to_vec(&invalid).unwrap(), UUID, None).is_err());
    }

    #[test]
    fn textures_allow_only_official_cdn_and_never_credentials_queries_or_redirect_hosts() {
        assert!(official_texture_url(&texture())
            .unwrap()
            .starts_with("https://"));
        for value in [
            texture().replace("textures.minecraft.net", "evil.invalid"),
            texture().replace(
                "textures.minecraft.net",
                "textures.minecraft.net.evil.invalid",
            ),
            texture().replace("http://", "https://user:pass@"),
            format!("{}?token=x", texture()),
            format!("{}#x", texture()),
            texture().replace(".net/", ".net:8443/"),
            "https://textures.minecraft.net/texture/../secret".into(),
        ] {
            assert!(official_texture_url(&value).is_err(), "{value}");
        }
    }

    #[test]
    fn response_bytes_and_png_dimensions_are_bounded() {
        assert!(read_bounded(std::io::repeat(b'x'), 10).is_err());
        assert_eq!(read_bounded(&b"ok"[..], 10).unwrap(), b"ok");
        let mut png = vec![0; 33];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[8..12].copy_from_slice(&13_u32.to_be_bytes());
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&64_u32.to_be_bytes());
        for height in [32_u32, 64] {
            png[20..24].copy_from_slice(&height.to_be_bytes());
            assert!(validate_skin_png(&png).is_ok());
        }
        png[20..24].copy_from_slice(&128_u32.to_be_bytes());
        assert!(validate_skin_png(&png).is_err());
        assert!(validate_skin_png(&png[..24]).is_err());
    }

    #[test]
    fn quota_outage_and_missing_skin_are_distinct() {
        assert!(status_error(StatusCode::TOO_MANY_REQUESTS, false)
            .unwrap()
            .contains("Muitas consultas"));
        assert_eq!(
            status_error(StatusCode::NOT_FOUND, false).unwrap(),
            "Jogador não encontrado."
        );
        assert!(status_error(StatusCode::SERVICE_UNAVAILABLE, false)
            .unwrap()
            .contains("indisponível"));
        assert!(!status_error(StatusCode::NOT_FOUND, true)
            .unwrap()
            .contains("Jogador"));
        assert!(status_error(StatusCode::FOUND, true).is_some());
        assert!(status_error(StatusCode::OK, false).is_none());
    }

    #[test]
    #[ignore = "Consulta APIs públicas reais; executar explicitamente para homologar a rede local."]
    fn live_public_profiles_return_verified_png_hashes() {
        for name in ["Notch", "jeb_", "Dinnerbone"] {
            let skin = lookup_blocking(name).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(skin.name.eq_ignore_ascii_case(name));
            assert_eq!(skin.content_hash.len(), 64);
            assert!(skin
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()));
            assert!(skin
                .skin_url
                .starts_with("https://textures.minecraft.net/texture/"));
        }
    }
}
