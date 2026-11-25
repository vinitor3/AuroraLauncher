use super::{content_updates::ensure_no_symlink_components, launch_settings, recovery, Instance};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::ErrorKind, path::Path};
use uuid::Uuid;

const MAX_BYTES: u64 = 4 * 1024 * 1024;
const SAVED: &str = "options.txt";
const DEFAULTS: &[&str] = &[
    "config/defaultoptions/options.txt",
    "config/defaultoptions/keybindings.txt",
    "config/yosbr/options.txt",
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBinding {
    action: String,
    input: String,
    modifier: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyMap {
    bindings: Vec<KeyBinding>,
    source: &'static str,
    revision: Option<String>,
    modifier_support: ModifierSupport,
    input_backend: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum ModifierSupport {
    LegacyForge,
    ModernForge,
    BrokenForgeReader,
    Unavailable,
}

fn installed_forge(root: &Path, id: &str) -> Option<ModifierSupport> {
    let (mc, build) = id.split_once("-forge-")?;
    let (major, support) = match mc {
        "1.12.2" => ("14", ModifierSupport::LegacyForge),
        "1.16.5" => ("36", ModifierSupport::ModernForge),
        "1.19.2" => ("43", ModifierSupport::ModernForge),
        "1.20.1" => ("47", ModifierSupport::ModernForge),
        // This Forge branch calls InputConstants.getKey on the entire string before splitting the suffix.
        "1.21.1" => ("52", ModifierSupport::BrokenForgeReader),
        _ => return None,
    };
    if build.len() > 32
        || build.split('.').next() != Some(major)
        || !build
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let text = read_file(root, &format!("versions/{id}/{id}.json")).ok()??;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    if v["id"] != id || v["inheritsFrom"] != mc {
        return None;
    }
    let coordinate = format!("{mc}-{build}");
    let genuine = v["libraries"]
        .as_array()?
        .iter()
        .filter_map(|l| l["name"].as_str())
        .any(|name| {
            let mut fields = name.split(':');
            fields.next() == Some("net.minecraftforge")
                && matches!(fields.next(), Some("forge" | "fmlloader"))
                && fields.next() == Some(coordinate.as_str())
        });
    genuine.then_some(support)
}

fn modifier_support(root: &Path) -> ModifierSupport {
    let unsupported = ModifierSupport::Unavailable;
    for filename in ["aurora-modpack.json", "aurora-instance.json"] {
        match read_file(root, filename) {
            Ok(None) => continue,
            Ok(Some(text)) => {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
                    return unsupported;
                };
                let Some(id) = v["versionId"].as_str() else {
                    return unsupported;
                };
                let Some((mc, _)) = id.split_once("-forge-") else {
                    return unsupported;
                };
                if v["minecraft"].as_str().is_some_and(|m| m != mc)
                    || v["loader"].as_str().is_some_and(|l| l != "forge")
                    || matches!(v["source"].as_str(), Some("vanilla" | "fabric"))
                {
                    return unsupported;
                }
                return installed_forge(root, id).unwrap_or(unsupported);
            }
            Err(_) => return unsupported,
        }
    }
    // Old imported instances have only versions/. Accept one Forge profile plus its vanilla parent.
    let Ok(_pins) = recovery::pin_directory(&root.join("versions")) else {
        return unsupported;
    };
    let Ok(entries) = fs::read_dir(root.join("versions")) else {
        return unsupported;
    };
    let mut names = Vec::new();
    for entry in entries.take(513) {
        let Ok(entry) = entry else { return unsupported };
        if !entry.file_type().is_ok_and(|ty| ty.is_dir()) {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            return unsupported;
        };
        names.push(name);
        if names.len() > 2 {
            return unsupported;
        }
    }
    let Some(id) = names.iter().find(|n| n.contains("-forge-")) else {
        return unsupported;
    };
    let Some((base, _)) = id.split_once("-forge-") else {
        return unsupported;
    };
    if names.iter().any(|n| n != id && n != base) {
        return unsupported;
    }
    installed_forge(root, id).unwrap_or(unsupported)
}

fn valid_input(input: &str) -> bool {
    if let Ok(code) = input.parse::<i16>() {
        return (-100..=-81).contains(&code) || (0..=255).contains(&code);
    }
    let tail = ["key.keyboard.", "key.mouse.", "scancode."]
        .iter()
        .find_map(|prefix| input.strip_prefix(prefix));
    tail.is_some_and(|value| {
        !value.is_empty()
            && value.len() <= 54
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.')
    })
}

fn parse_binding(line: &str) -> Option<KeyBinding> {
    let entry = line.trim_start_matches('\u{feff}').strip_prefix("key_")?;
    let (entry, modifier) = match entry.rsplit_once(':') {
        Some((before, modifier))
            if modifier.is_empty()
                || (modifier.len() <= 30
                    && modifier
                        .bytes()
                        .all(|b| b.is_ascii_alphabetic() || b == b',' || b == b'_')
                    && before
                        .rsplit_once(':')
                        .is_some_and(|(_, input)| valid_input(input))) =>
        {
            (before, modifier)
        }
        _ => (entry, ""),
    };
    let (action, input) = entry.rsplit_once(':')?;
    if action.is_empty()
        || action.len() > 256
        || action.chars().any(char::is_control)
        || !valid_input(input)
    {
        return None;
    }
    Some(KeyBinding {
        action: action.to_owned(),
        input: input.to_owned(),
        modifier: modifier.to_owned(),
    })
}

fn read_file(root: &Path, relative: &str) -> Result<Option<String>, String> {
    ensure_no_symlink_components(root, relative).map_err(|e| e.to_string())?;
    let path = root.join(relative);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Não foi possível consultar o arquivo de controles.".into()),
        Ok(_) => {
            let _parents = recovery::pin_directory(path.parent().ok_or("Pasta inválida.")?)
                .map_err(|e| e.to_string())?;
            let bytes = recovery::read_bounded(&path, MAX_BYTES).map_err(|e| e.to_string())?;
            String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| "Arquivo de controles com codificação incompatível.".into())
        }
    }
}

fn revision(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn profile_input_backend(root: &Path, id: &str) -> Option<&'static str> {
    if id.is_empty()
        || id.len() > 96
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        || id == "."
        || id == ".."
    {
        return None;
    }
    let text = read_file(root, &format!("versions/{id}/{id}.json")).ok()??;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    if value["id"] != id || value.get("inheritsFrom").is_some() {
        return None;
    }
    let libraries = value["libraries"].as_array()?;
    let has = |prefix: &str| {
        libraries
            .iter()
            .filter_map(|lib| lib["name"].as_str())
            .any(|name| name.starts_with(prefix))
    };
    match (has("org.lwjgl:lwjgl-glfw:"), has("org.lwjgl:lwjgl-sdl:")) {
        (true, false) => Some("glfw"),
        (false, true) => Some("sdl"),
        _ => None,
    }
}

fn input_backend(root: &Path) -> Option<&'static str> {
    for filename in ["aurora-modpack.json", "aurora-instance.json"] {
        if let Some(text) = read_file(root, filename).ok()? {
            let value: serde_json::Value = serde_json::from_str(&text).ok()?;
            if let Some(base) = value["minecraft"].as_str() {
                return profile_input_backend(root, base);
            }
        }
    }
    // Imported instances may only retain the vanilla profile and a loader child.
    let dir = root.join("versions");
    let _pins = recovery::pin_directory(&dir).ok()?;
    let mut backend = None;
    for (index, entry) in fs::read_dir(&dir).ok()?.enumerate() {
        if index >= 64 {
            return None;
        }
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_dir() {
            continue;
        }
        if let Some(found) = profile_input_backend(root, entry.file_name().to_str()?) {
            if backend.is_some_and(|previous| previous != found) {
                return None;
            }
            backend = Some(found);
        }
    }
    backend
}

pub fn inspect(instance: &Instance) -> Result<KeyMap, String> {
    let root = instance.root();
    let _pinned = recovery::pin_directory(root).map_err(|e| e.to_string())?;
    let mut commands = BTreeMap::new();
    let mut collect = |text: &str| {
        for binding in text.lines().filter_map(parse_binding) {
            commands.insert(binding.action.clone(), binding);
        }
    };
    let (source, revision) = if let Some(saved) = read_file(root, SAVED)? {
        collect(&saved);
        ("saved", Some(revision(&saved)))
    } else {
        for filename in DEFAULTS {
            if let Some(text) = read_file(root, filename)? {
                collect(&text);
            }
        }
        (
            if commands.is_empty() {
                "empty"
            } else {
                "defaults"
            },
            None,
        )
    };
    Ok(KeyMap {
        bindings: commands.into_values().collect(),
        source,
        revision,
        modifier_support: modifier_support(root),
        input_backend: input_backend(root),
    })
}

fn with_replacement(
    text: &str,
    action: &str,
    input: &str,
    modifier: Option<&str>,
    support: ModifierSupport,
) -> Result<String, String> {
    if !valid_input(input) {
        return Err("Tecla inválida.".into());
    }
    let mut output = String::with_capacity(text.len());
    let mut found = false;
    for raw in text.split_inclusive('\n') {
        let body = raw.trim_end_matches(['\r', '\n']);
        if let Some(binding) = parse_binding(body).filter(|binding| binding.action == action) {
            if binding.input.parse::<i16>().is_ok() != input.parse::<i16>().is_ok() {
                return Err("A tecla pertence a outra geração do Minecraft.".into());
            }
            // A key-only update can also turn an existing Alt+C into Alt+Alt.
            let retained = (binding.input != input
                && support != ModifierSupport::Unavailable
                && matches!(binding.modifier.as_str(), "ALT" | "SHIFT" | "CONTROL"))
            .then_some(binding.modifier.as_str());
            if let Some(m) = modifier.or(retained) {
                if !matches!(
                    binding.modifier.as_str(),
                    "" | "NONE" | "SHIFT" | "CONTROL" | "ALT"
                ) || !matches!(m, "NONE" | "SHIFT" | "CONTROL" | "ALT")
                {
                    return Err("O formato deste modificador pertence ao jogo ou ao mod.".into());
                }
                let supported = match support {
                    ModifierSupport::LegacyForge => input.parse::<i16>().is_ok(),
                    ModifierSupport::ModernForge => input.parse::<i16>().is_err(),
                    ModifierSupport::BrokenForgeReader => {
                        m == "NONE" && !binding.modifier.is_empty()
                    }
                    ModifierSupport::Unavailable => false,
                };
                if !supported {
                    return Err("Este perfil instalado não permite editar esse modificador.".into());
                }
                let self_modifier = match m {
                    "ALT" => matches!(
                        input,
                        "56" | "184" | "key.keyboard.left.alt" | "key.keyboard.right.alt"
                    ),
                    "CONTROL" => matches!(
                        input,
                        "29" | "157" | "key.keyboard.left.control" | "key.keyboard.right.control"
                    ),
                    "SHIFT" => matches!(
                        input,
                        "42" | "54" | "key.keyboard.left.shift" | "key.keyboard.right.shift"
                    ),
                    _ => false,
                };
                if m != "NONE" && (self_modifier || matches!(input, "0" | "key.keyboard.unknown")) {
                    return Err("Escolha uma tecla diferente do próprio modificador.".into());
                }
            }
            if body.starts_with('\u{feff}') {
                output.push('\u{feff}');
            }
            output.push_str("key_");
            output.push_str(&binding.action);
            output.push(':');
            output.push_str(input);
            let suffix = modifier.unwrap_or(&binding.modifier);
            if (modifier.is_some() && suffix != "NONE")
                || (modifier.is_none() && (!suffix.is_empty() || body.ends_with(':')))
            {
                output.push(':');
                output.push_str(suffix);
            }
            output.push_str(&raw[body.len()..]);
            found = true;
        } else {
            output.push_str(raw);
        }
    }
    if !found {
        return Err("Esse comando mudou. Atualize o mapa.".into());
    }
    Ok(output)
}

pub fn rebind(
    instance: &Instance,
    expected: &str,
    action: &str,
    input: &str,
    modifier: Option<&str>,
) -> Result<KeyMap, String> {
    let root = instance.root();
    let _pinned = recovery::pin_directory(root).map_err(|e| e.to_string())?;
    let read_saved = || {
        read_file(root, SAVED)?
            .ok_or_else(|| "Abra o jogo para criar os controles da instância.".to_string())
    };
    let original = read_saved()?;
    if revision(&original) != expected {
        return Err("Os controles mudaram. Atualize o mapa antes de salvar.".into());
    }
    let support = modifier_support(root);
    let updated = with_replacement(&original, action, input, modifier, support)?;
    let tmp = root.join(format!(".aurora-controls-{}.tmp", Uuid::new_v4()));
    recovery::write_new(&tmp, updated.as_bytes()).map_err(|e| e.to_string())?;
    let result = (|| {
        if read_saved()? != original || (modifier.is_some() && modifier_support(root) != support) {
            return Err("Os controles foram alterados. Atualize o mapa.".into());
        }
        launch_settings::atomic_replace(&tmp, &root.join(SAVED))
            .map_err(|_| "Não foi possível salvar options.txt.".to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    inspect(instance)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_bom_line_endings_namespaces_modifier_and_foreign_options() {
        let text = "\u{feff}key_mod:open:key.keyboard.r:SHIFT\r\nserver:private\r\nkey_key.jump:key.keyboard.space";
        assert_eq!(
            with_replacement(
                text,
                "mod:open",
                "key.keyboard.g",
                None,
                ModifierSupport::Unavailable
            )
            .unwrap(),
            text.replace("keyboard.r", "keyboard.g")
        );
        assert_eq!(
            parse_binding("key_test:key.keyboard.l:CONTROL,SHIFT")
                .unwrap()
                .modifier,
            "CONTROL,SHIFT"
        );
        assert_eq!(
            with_replacement(
                "key_test:key.keyboard.l:",
                "test",
                "key.keyboard.k",
                None,
                ModifierSupport::Unavailable
            )
            .unwrap(),
            "key_test:key.keyboard.k:"
        );
        assert!(parse_binding("lastServer:example.org").is_none());
        assert!(parse_binding("key_secret:invalid").is_none());
        assert!(with_replacement(
            "key_key.attack:-100",
            "key.attack",
            "key.mouse.left",
            None,
            ModifierSupport::Unavailable
        )
        .is_err());
    }
    #[test]
    fn input_names_follow_installed_runtime_and_ambiguous_imports_remain_unknown() {
        let root = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let child = root.join("versions").join("fabric-test");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("fabric-test.json"), r#"{"id":"fabric-test","inheritsFrom":"1.21.11","libraries":[{"name":"org.lwjgl:lwjgl-sdl:3.4.3"}]}"#).unwrap();
        assert_eq!(input_backend(&root), None);
        for (id, backend) in [("1.21.11", "glfw"), ("26.3", "sdl")] {
            let directory = root.join("versions").join(id);
            fs::create_dir_all(&directory).unwrap();
            let profile = serde_json::json!({"id":id, "libraries":[{"name":format!("org.lwjgl:lwjgl-{backend}:3.4.3")}]});
            fs::write(directory.join(format!("{id}.json")), profile.to_string()).unwrap();
            assert_eq!(profile_input_backend(&root, id), Some(backend));
        }
        assert_eq!(input_backend(&root), None);
        fs::write(root.join("aurora-instance.json"), r#"{"minecraft":"26.3"}"#).unwrap();
        assert_eq!(input_backend(&root), Some("sdl"));
        fs::write(
            root.join("aurora-instance.json"),
            r#"{"minecraft":"../other"}"#,
        )
        .unwrap();
        assert_eq!(input_backend(&root), None);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn saved_options_override_pack_defaults_and_stale_edits_fail() {
        use super::super::{InstanceId, InstanceLayout};
        let root = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("map-test").unwrap());
        fs::create_dir_all(instance.root().join("config/yosbr")).unwrap();
        fs::write(
            instance.root().join(DEFAULTS[2]),
            "key_key.test:key.keyboard.k",
        )
        .unwrap();
        assert_eq!(inspect(&instance).unwrap().source, "defaults");
        fs::write(
            instance.root().join(SAVED),
            "key_key.jump:57\nkey_key.inventory:18",
        )
        .unwrap();
        let saved = inspect(&instance).unwrap();
        assert_eq!(saved.bindings.len(), 2);
        let hash = saved.revision.unwrap();
        rebind(&instance, &hash, "key.jump", "19", None).unwrap();
        assert!(rebind(&instance, &hash, "key.jump", "20", None).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn forge_modifier_roundtrip_keeps_c_and_alt_c_distinct_across_supported_profiles() {
        use super::super::{InstanceId, InstanceLayout};
        let root = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("keys-forge").unwrap());
        for (mc, build, input) in [
            ("1.12.2", "14.23.5.2860", "46"),
            ("1.16.5", "36.2.42", "key.keyboard.c"),
            ("1.19.2", "43.4.16", "key.keyboard.c"),
            ("1.20.1", "47.4.28", "key.keyboard.c"),
            ("1.21.1", "52.1.12", "key.keyboard.c"),
        ] {
            let id = format!("{mc}-forge-{build}");
            let version = instance.root().join("versions").join(&id);
            fs::create_dir_all(&version).unwrap();
            fs::write(version.join(format!("{id}.json")),serde_json::json!({"id":id,"inheritsFrom":mc,"libraries":[{"name":format!("net.minecraftforge:fmlloader:{mc}-{build}")}]}).to_string()).unwrap();
            fs::write(
                instance.root().join("aurora-instance.json"),
                serde_json::json!({"versionId":id,"minecraft":mc,"source":"forge"}).to_string(),
            )
            .unwrap();
            let original = format!("key_mod.plain:{input}\r\nkey_mod.alternate:{input}\r\nkey_other:{input}:CUSTOM\r\n");
            let file = instance.root().join(SAVED);
            fs::write(&file, &original).unwrap();
            let hash = revision(&original);
            let edit = |revision: &str, modifier| {
                rebind(&instance, revision, "mod.alternate", input, Some(modifier))
            };
            if mc == "1.21.1" {
                assert!(edit(&hash, "ALT").is_err());
                assert_eq!(fs::read_to_string(&file).unwrap(), original);
                let modified = original.replace(
                    &format!("alternate:{input}"),
                    &format!("alternate:{input}:ALT"),
                );
                fs::write(&file, &modified).unwrap();
                edit(&revision(&modified), "NONE").unwrap();
                assert_eq!(fs::read_to_string(&file).unwrap(), original);
                continue;
            }
            for modifier in ["ALT", "CONTROL", "SHIFT"] {
                edit(&hash, modifier).unwrap();
                let map = inspect(&instance).unwrap();
                assert_eq!(
                    map.bindings
                        .iter()
                        .find(|b| b.action == "mod.plain")
                        .unwrap()
                        .modifier,
                    ""
                );
                assert_eq!(
                    map.bindings
                        .iter()
                        .find(|b| b.action == "mod.alternate")
                        .unwrap()
                        .modifier,
                    modifier
                );
                assert_eq!(
                    fs::read_to_string(&file).unwrap(),
                    original.replace(
                        &format!("alternate:{input}"),
                        &format!("alternate:{input}:{modifier}")
                    )
                );
                edit(&map.revision.unwrap(), "NONE").unwrap();
                assert_eq!(fs::read_to_string(&file).unwrap(), original);
            }
            assert!(rebind(&instance, &hash, "other", input, Some("ALT")).is_err());
            assert!(edit(&hash, "SUPER").is_err());
            fs::write(
                instance.root().join("aurora-modpack.json"),
                r#"{"versionId":"fabric-loader-0.16.14-1.20.1","loader":"fabric"}"#,
            )
            .unwrap();
            assert!(edit(&hash, "ALT").is_err());
            fs::remove_file(instance.root().join("aurora-modpack.json")).unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }
}
