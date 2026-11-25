use serde_json::Value;
use std::{fs, io::Read, path::Path};

fn presence_id(id: Option<&str>) -> bool {
    matches!(id, Some("craftpresence" | "simplerpc"))
}

fn declares_presence(name: &str, data: &str) -> bool {
    match name {
        "META-INF/mods.toml" => {
            #[cfg(feature = "connect-lab")]
            if let Ok(value) = toml::from_str::<toml::Value>(data) {
                return value
                    .get("mods")
                    .and_then(|v| v.as_array())
                    .is_some_and(|mods| {
                        mods.iter()
                            .any(|m| presence_id(m.get("modId").and_then(|v| v.as_str())))
                    });
            }
            false
        }
        "fabric.mod.json" => serde_json::from_str::<Value>(data)
            .ok()
            .is_some_and(|v| presence_id(v["id"].as_str())),
        _ => serde_json::from_str::<Value>(data).ok().is_some_and(|v| {
            v.as_array()
                .or_else(|| v["modList"].as_array())
                .is_some_and(|mods| mods.iter().any(|m| presence_id(m["modid"].as_str())))
        }),
    }
}

/// Reads declarations from enabled top-level JARs. Dependency IDs and filenames do not identify a mod.
pub(crate) fn has_presence(instance: &Path) -> Result<bool, &'static str> {
    const ERROR: &str = "discord_mods_unavailable";
    const LIMIT: u64 = 128 * 1024;
    let directory = instance.join("mods");
    if !directory.try_exists().map_err(|_| ERROR)? {
        return Ok(false);
    }
    if !fs::symlink_metadata(&directory).is_ok_and(|m| m.file_type().is_dir()) {
        return Err(ERROR);
    }
    for (index, entry) in fs::read_dir(directory).map_err(|_| ERROR)?.enumerate() {
        if index >= 2048 {
            return Err(ERROR);
        }
        let entry = entry.map_err(|_| ERROR)?;
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("jar"))
        {
            continue;
        }
        if !entry.file_type().map_err(|_| ERROR)?.is_file() {
            return Err(ERROR);
        }
        let mut archive =
            zip::ZipArchive::new(fs::File::open(path).map_err(|_| ERROR)?).map_err(|_| ERROR)?;
        for name in ["fabric.mod.json", "META-INF/mods.toml", "mcmod.info"] {
            let Ok(file) = archive.by_name(name) else {
                continue;
            };
            if file.size() > LIMIT {
                return Err(ERROR);
            }
            let mut data = String::new();
            file.take(LIMIT + 1)
                .read_to_string(&mut data)
                .map_err(|_| ERROR)?;
            if data.len() as u64 > LIMIT {
                return Err(ERROR);
            }
            if declares_presence(name, &data) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an isolated instance containing an original presence mod"]
    fn discord_recognizes_a_real_presence_mod() {
        let directory =
            std::env::var_os("AURORA_DISCORD_QA_MOD_DIRECTORY").expect("instância de teste");
        assert_eq!(has_presence(Path::new(&directory)), Ok(true));
    }
    #[test]
    fn discord_detects_enabled_mod_identity_and_ignores_names_dependencies_and_disabled_jars() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("aurora-discord-{}", uuid::Uuid::new_v4()));
        let mods = root.join("mods");
        fs::create_dir_all(&mods).unwrap();
        let jar = |file: &str, name: &str, data: &str| {
            let mut zip = zip::ZipWriter::new(fs::File::create(mods.join(file)).unwrap());
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data.as_bytes()).unwrap();
            zip.finish().unwrap();
        };
        jar(
            "craftpresence.jar",
            "fabric.mod.json",
            r#"{"id":"other","suggests":{"craftpresence":"*"}}"#,
        );
        jar(
            "rpc.jar.disabled",
            "fabric.mod.json",
            r#"{"id":"simplerpc"}"#,
        );
        assert_eq!(has_presence(&root), Ok(false));
        jar(
            "renamed.jar",
            "fabric.mod.json",
            r#"{"id":"craftpresence"}"#,
        );
        assert_eq!(has_presence(&root), Ok(true));
        assert!(declares_presence(
            "mcmod.info",
            r#"[{"modid":"simplerpc"}]"#
        ));
        #[cfg(feature = "connect-lab")]
        assert!(declares_presence(
            "META-INF/mods.toml",
            "[[mods]]\nmodId='simplerpc'"
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
