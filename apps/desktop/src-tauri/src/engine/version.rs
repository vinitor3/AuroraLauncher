//! Resolve perfis Minecraft instalados (vanilla, Fabric e Forge) para um comando Java.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use thiserror::Error;

use super::{Instance, VersionLaunchSpec};

#[derive(Debug, Error)]
pub enum VersionResolveError {
    #[error("perfil instalado não encontrado: {0}")]
    MissingProfile(String),
    #[error("perfil Minecraft inválido: {0}")]
    InvalidProfile(String),
    #[error("cadeia de perfis contém um ciclo em {0}")]
    InheritanceCycle(String),
    #[error("falha de E/S: {0}")]
    Io(#[from] std::io::Error),
}

pub fn resolve_launch_spec(
    instance: &Instance,
    version_id: &str,
) -> Result<VersionLaunchSpec, VersionResolveError> {
    let mut seen = Vec::new();
    let profile = resolve_profile(instance, version_id, &mut seen)?;
    let client_jar = find_client_jar(instance, version_id, &mut Vec::new())?;
    let main_class = profile
        .main_class
        .ok_or_else(|| VersionResolveError::InvalidProfile("mainClass ausente".to_owned()))?;
    let asset_index = profile
        .asset_index
        .ok_or_else(|| VersionResolveError::InvalidProfile("assetIndex ausente".to_owned()))?;
    let mut libraries = Vec::new();
    for library in profile.libraries.into_values() {
        if !rules_allow_windows(&library) {
            continue;
        }
        if let Some(path) = library_path(&library)? {
            libraries.push(instance.root().join("libraries").join(path));
        }
    }
    if libraries.is_empty() {
        return Err(VersionResolveError::InvalidProfile(
            "nenhuma biblioteca encontrada".to_owned(),
        ));
    }

    let jvm_args = clean_jvm_arguments(&profile.jvm_args, instance);
    let game_args = clean_game_arguments(&profile.game_args);
    Ok(VersionLaunchSpec {
        version_name: version_id.to_owned(),
        main_class,
        client_jar,
        libraries,
        assets_dir: instance.root().join("assets"),
        asset_index,
        jvm_args,
        game_args,
        aurora_ipc_port: None,
        aurora_session_nonce: None,
    })
}

#[derive(Default)]
struct ResolvedProfile {
    main_class: Option<String>,
    asset_index: Option<String>,
    libraries: BTreeMap<String, Value>,
    jvm_args: Vec<String>,
    game_args: Vec<String>,
}

fn resolve_profile(
    instance: &Instance,
    version_id: &str,
    seen: &mut Vec<String>,
) -> Result<ResolvedProfile, VersionResolveError> {
    if seen.iter().any(|item| item == version_id) {
        return Err(VersionResolveError::InheritanceCycle(version_id.to_owned()));
    }
    seen.push(version_id.to_owned());
    let profile = load_profile(instance, version_id)?;
    let mut resolved = if let Some(parent) = profile["inheritsFrom"].as_str() {
        resolve_profile(instance, parent, seen)?
    } else {
        ResolvedProfile::default()
    };
    seen.pop();

    if let Some(main_class) = profile["mainClass"].as_str() {
        resolved.main_class = Some(main_class.to_owned());
    }
    if let Some(asset_index) = profile.pointer("/assetIndex/id").and_then(Value::as_str) {
        resolved.asset_index = Some(asset_index.to_owned());
    }
    if let Some(libraries) = profile["libraries"].as_array() {
        for library in libraries {
            let name = library["name"].as_str().ok_or_else(|| {
                VersionResolveError::InvalidProfile("biblioteca sem nome".to_owned())
            })?;
            resolved.libraries.insert(name.to_owned(), library.clone());
        }
    }
    let mut jvm_args = profile_arguments(&profile, "jvm");
    let mut game_args = profile_arguments(&profile, "game");
    if game_args.is_empty() {
        if let Some(arguments) = profile["minecraftArguments"].as_str() {
            game_args = arguments.split_whitespace().map(str::to_owned).collect();
        }
    }
    resolved.jvm_args.append(&mut jvm_args);
    resolved.game_args.append(&mut game_args);
    Ok(resolved)
}

fn load_profile(instance: &Instance, version_id: &str) -> Result<Value, VersionResolveError> {
    let path = instance
        .root()
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.json"));
    if !path.is_file() {
        return Err(VersionResolveError::MissingProfile(version_id.to_owned()));
    }
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| VersionResolveError::InvalidProfile(error.to_string()))
}

fn find_client_jar(
    instance: &Instance,
    version_id: &str,
    seen: &mut Vec<String>,
) -> Result<PathBuf, VersionResolveError> {
    if seen.iter().any(|item| item == version_id) {
        return Err(VersionResolveError::InheritanceCycle(version_id.to_owned()));
    }
    seen.push(version_id.to_owned());
    let path = instance
        .root()
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.jar"));
    if path.is_file() {
        return Ok(path);
    }
    let profile = load_profile(instance, version_id)?;
    let parent = profile["inheritsFrom"]
        .as_str()
        .ok_or_else(|| VersionResolveError::InvalidProfile("client.jar ausente".to_owned()))?;
    find_client_jar(instance, parent, seen)
}

fn profile_arguments(profile: &Value, kind: &str) -> Vec<String> {
    let Some(arguments) = profile
        .pointer(&format!("/arguments/{kind}"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    arguments.iter().flat_map(argument_values).collect()
}

fn argument_values(argument: &Value) -> Vec<String> {
    if let Some(value) = argument.as_str() {
        return vec![value.to_owned()];
    }
    if !rules_allow_windows(argument) {
        return Vec::new();
    }
    match &argument["value"] {
        Value::String(value) => vec![value.to_owned()],
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn rules_allow_windows(value: &Value) -> bool {
    let Some(rules) = value["rules"].as_array() else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        let os_applies = rule
            .pointer("/os/name")
            .and_then(Value::as_str)
            .map(|name| name == "windows")
            .unwrap_or(true);
        let features_apply = rule["features"]
            .as_object()
            .map(|features| features.values().all(|value| value == &Value::Bool(false)))
            .unwrap_or(true);
        if os_applies && features_apply {
            allowed = rule["action"].as_str() == Some("allow");
        }
    }
    allowed
}

fn library_path(library: &Value) -> Result<Option<PathBuf>, VersionResolveError> {
    if let Some(path) = library
        .pointer("/downloads/artifact/path")
        .and_then(Value::as_str)
    {
        return Ok(Some(PathBuf::from(path)));
    }
    // Alguns manifests antigos declaram contêineres exclusivamente nativos
    // sem um artefato Java principal (por exemplo jinput-platform 2.0.5).
    // Esses JARs são extraídos em `natives` e não pertencem ao classpath.
    if library.get("natives").is_some() && library.pointer("/downloads/artifact").is_none() {
        return Ok(None);
    }
    let Some(name) = library["name"].as_str() else {
        return Ok(None);
    };
    let (coordinate, extension) = name.split_once('@').unwrap_or((name, "jar"));
    let pieces: Vec<&str> = coordinate.split(':').collect();
    if pieces.len() < 3 {
        return Err(VersionResolveError::InvalidProfile(format!(
            "coordenada Maven inválida: {name}"
        )));
    }
    let group = pieces[0].replace('.', "/");
    let artifact = pieces[1];
    let version = pieces[2];
    let classifier = pieces.get(3).copied().filter(|value| !value.is_empty());
    let filename = match classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{extension}"),
        None => format!("{artifact}-{version}.{extension}"),
    };
    Ok(Some(
        PathBuf::from(group)
            .join(artifact)
            .join(version)
            .join(filename),
    ))
}

fn clean_jvm_arguments(args: &[String], instance: &Instance) -> Vec<String> {
    let mut cleaned = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let value = &args[index];
        if value == "-cp" || value == "-classpath" {
            index += 2;
            continue;
        }
        if value == "${classpath}" {
            index += 1;
            continue;
        }
        cleaned.push(
            value
                .replace(
                    "${natives_directory}",
                    &instance.natives_dir().display().to_string(),
                )
                .replace("${launcher_name}", "Aurora")
                .replace("${launcher_version}", "0.1.0")
                .replace(
                    "${library_directory}",
                    &instance.root().join("libraries").display().to_string(),
                )
                .replace(
                    "${classpath_separator}",
                    if cfg!(windows) { ";" } else { ":" },
                ),
        );
        index += 1;
    }
    cleaned
}

fn clean_game_arguments(args: &[String]) -> Vec<String> {
    let mut cleaned = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let value = &args[index];
        let next = args.get(index + 1);
        // Os manifests modernos têm pares opcionais como --clientId,
        // --quickPlayPath e --xuid. Deixar somente a flag quando o valor é
        // uma variável não resolvida desloca todos os argumentos seguintes.
        if value.starts_with("--") && next.is_some_and(|value| value.contains("${")) {
            index += 2;
            continue;
        }
        if !value.contains("${") {
            cleaned.push(value.clone());
        }
        index += 1;
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::{clean_game_arguments, library_path};
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn skips_native_only_library_without_main_artifact() {
        let library = json!({
            "name": "net.java.jinput:jinput-platform:2.0.5",
            "natives": { "windows": "natives-windows" },
            "downloads": { "classifiers": { "natives-windows": { "path": "native.jar" } } }
        });
        assert_eq!(library_path(&library).unwrap(), None);
    }

    #[test]
    fn keeps_only_modpack_safe_game_arguments() {
        assert_eq!(
            clean_game_arguments(&[
                "--username".into(),
                "${auth_player_name}".into(),
                "--demo".into(),
            ]),
            vec!["--demo"]
        );
    }

    #[test]
    fn removes_unresolved_optional_argument_pairs() {
        assert_eq!(
            clean_game_arguments(&[
                "--clientId".into(),
                "${clientid}".into(),
                "--xuid".into(),
                "${auth_xuid}".into(),
                "--demo".into(),
            ]),
            vec!["--demo"]
        );
    }

    #[test]
    fn resolves_maven_coordinate_to_library_path() {
        assert_eq!(
            library_path(&json!({ "name": "net.fabricmc:fabric-loader:0.16.10" }))
                .unwrap()
                .unwrap(),
            PathBuf::from("net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar")
        );
    }
}
