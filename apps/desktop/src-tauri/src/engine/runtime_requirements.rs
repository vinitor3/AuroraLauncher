//! Requisitos de runtime declarados pelo conteúdo instalado na instância.

use std::fs::{self, File};
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use serde_json::Value;
use zip::ZipArchive;

use super::{required_java_major, Instance};

const MAX_MODS: usize = 512;
const MAX_NESTED_JARS: usize = 32;
const MAX_NESTED_DEPTH: u8 = 3;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_NESTED_JAR_BYTES: u64 = 16 * 1024 * 1024;

pub fn required_java_major_for_instance(instance: &Instance, minecraft_version: &str) -> u32 {
    required_java_major_for_root(instance.root(), minecraft_version)
}

fn required_java_major_for_root(root: &Path, minecraft_version: &str) -> u32 {
    let mut required = required_java_major(minecraft_version);
    let Ok(entries) = fs::read_dir(root.join("mods")) else {
        return required;
    };
    for entry in entries.flatten().take(MAX_MODS) {
        let path = entry.path();
        if !path.is_file()
            || !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
        {
            continue;
        }
        let Ok(file) = File::open(path) else {
            continue;
        };
        let Ok(mut archive) = ZipArchive::new(file) else {
            continue;
        };
        required = required.max(required_java_from_archive(&mut archive, 0));
    }
    required
}

fn required_java_from_archive<R: Read + Seek>(archive: &mut ZipArchive<R>, depth: u8) -> u32 {
    if depth > MAX_NESTED_DEPTH {
        return 0;
    }
    let (mut required, nested_paths) = {
        let Ok(mut entry) = archive.by_name("fabric.mod.json") else {
            return 0;
        };
        if entry.size() > MAX_MANIFEST_BYTES {
            return 0;
        }
        let mut contents = String::new();
        if entry
            .by_ref()
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_string(&mut contents)
            .is_err()
            || contents.len() as u64 > MAX_MANIFEST_BYTES
        {
            return 0;
        }
        let Ok(manifest) = serde_json::from_str::<Value>(&contents) else {
            return 0;
        };
        let required = manifest
            .pointer("/depends/java")
            .and_then(minimum_java_from_value)
            .unwrap_or(0);
        let nested_paths = manifest["jars"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|nested| nested["file"].as_str())
            .filter(|path| path.ends_with(".jar") && path.len() <= 240)
            .take(MAX_NESTED_JARS)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        (required, nested_paths)
    };

    if depth == MAX_NESTED_DEPTH {
        return required;
    }
    for path in nested_paths {
        let nested_bytes = {
            let Ok(mut nested) = archive.by_name(&path) else {
                continue;
            };
            if nested.size() == 0 || nested.size() > MAX_NESTED_JAR_BYTES {
                continue;
            }
            let mut bytes = Vec::with_capacity(nested.size() as usize);
            if nested
                .by_ref()
                .take(MAX_NESTED_JAR_BYTES + 1)
                .read_to_end(&mut bytes)
                .is_err()
                || bytes.len() as u64 > MAX_NESTED_JAR_BYTES
            {
                continue;
            }
            bytes
        };
        let Ok(mut nested_archive) = ZipArchive::new(Cursor::new(nested_bytes)) else {
            continue;
        };
        required = required.max(required_java_from_archive(&mut nested_archive, depth + 1));
    }
    required
}

fn minimum_java_from_value(value: &Value) -> Option<u32> {
    match value {
        Value::String(predicate) => minimum_java_from_predicate(predicate),
        Value::Array(predicates) => predicates
            .iter()
            .filter_map(Value::as_str)
            .filter_map(minimum_java_from_predicate)
            .max(),
        _ => None,
    }
}

fn minimum_java_from_predicate(predicate: &str) -> Option<u32> {
    predicate
        .split(|character: char| character.is_whitespace() || matches!(character, ',' | '|'))
        .filter_map(|token| {
            let token = token.trim();
            if token.is_empty() || token.starts_with('<') || token == "*" {
                return None;
            }
            let strict = token.starts_with('>') && !token.starts_with(">=");
            let major = token
                .trim_start_matches(['>', '=', '^', '~'])
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<u32>()
                .ok()?;
            Some(if strict {
                major.saturating_add(1)
            } else {
                major
            })
        })
        .max()
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;
    use zip::{ZipArchive, ZipWriter};

    use super::{minimum_java_from_predicate, required_java_from_archive};

    fn jar(manifest: &str, nested: Option<&[u8]>) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("fabric.mod.json", SimpleFileOptions::default())
            .expect("manifesto deve iniciar");
        writer
            .write_all(manifest.as_bytes())
            .expect("manifesto deve ser escrito");
        if let Some(nested) = nested {
            writer
                .start_file(
                    "META-INF/jars/CaffeineConfig.jar",
                    SimpleFileOptions::default(),
                )
                .expect("jar aninhado deve iniciar");
            writer
                .write_all(nested)
                .expect("jar aninhado deve ser escrito");
        }
        writer.finish().expect("jar deve finalizar").into_inner()
    }

    #[test]
    fn reads_common_fabric_java_predicates() {
        assert_eq!(minimum_java_from_predicate(">=16"), Some(16));
        assert_eq!(minimum_java_from_predicate(">=8 <17"), Some(8));
        assert_eq!(minimum_java_from_predicate(">16"), Some(17));
        assert_eq!(minimum_java_from_predicate("*"), None);
    }

    #[test]
    fn finds_java_requirement_inside_a_nested_fabric_jar() {
        let nested = jar(r#"{"id":"caffeineconfig","depends":{"java":">=16"}}"#, None);
        let parent = jar(
            r#"{"id":"sodium-extra","jars":[{"file":"META-INF/jars/CaffeineConfig.jar"}]}"#,
            Some(&nested),
        );
        let mut archive = ZipArchive::new(Cursor::new(parent)).expect("jar externo deve abrir");
        assert_eq!(required_java_from_archive(&mut archive, 0), 16);
    }
}
