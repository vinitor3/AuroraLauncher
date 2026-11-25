use super::*;
use serde_json::json;
use std::io::Write;
use zip::write::SimpleFileOptions;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("aurora-mrpack-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn archive(&self, index: &Value, entries: &[(&str, &[u8])]) -> ZipArchive<File> {
        let path = self.0.join("test.mrpack");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("modrinth.index.json", SimpleFileOptions::default())
            .unwrap();
        serde_json::to_writer(&mut zip, index).unwrap();
        for &(name, data) in entries {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        ZipArchive::new(File::open(path).unwrap()).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn indexed_file(path: &str, client: Option<&str>) -> Value {
    let mut file = json!({"path":path,"hashes":{"sha1":"0".repeat(40)},"fileSize":3,
        "downloads":["https://unknown.example/other.jar", "https://cdn.modrinth.com/data/test/versions/fixture/file.jar"]});
    if let Some(client) = client {
        file["env"] = json!({"client": client, "server":"required"});
    }
    file
}

#[test]
fn plans_only_client_files_and_extracts_the_winning_layer_independent_of_zip_order() {
    let fixture = Fixture::new();
    let index = json!({"formatVersion":1,"files":[
        indexed_file("mods/both.jar",None), indexed_file("mods/client.jar",Some("required")),
        indexed_file("mods/optional.jar",Some("optional")), indexed_file("mods/server.jar",Some("unsupported")),
        indexed_file("config\\shared.cfg",None)
    ]});
    let mut zip = fixture.archive(
        &index,
        &[
            ("client-overrides/config/shared.cfg", b"client"),
            ("overrides/config//shared.cfg", b"common"),
            ("overrides/config/common.cfg", b"both"),
            ("server-overrides/config/exclusive.cfg", b"server"),
            ("client-overrides/options.txt", b"options"),
        ],
    );
    let index = read_pack_index(&mut zip).unwrap();
    let plan = client_pack_plan(&mut zip, &index, &fixture.0).unwrap();
    let names: Vec<_> = plan
        .downloads
        .iter()
        .map(|request| request.destination.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, ["both.jar", "client.jar", "optional.jar"]);
    assert_eq!(plan.managed_paths.len(), 7);
    assert!(plan
        .managed_paths
        .contains(&PathBuf::from("aurora-modpack.json")));
    assert!(!plan
        .managed_paths
        .contains(&PathBuf::from("mods/server.jar")));
    fs::create_dir(fixture.0.join("config")).unwrap();
    fs::write(fixture.0.join("config/shared.cfg"), b"older pack").unwrap();
    assert_eq!(
        extract_overrides(&mut zip, &fixture.0, &plan.overrides).unwrap(),
        3
    );
    assert_eq!(
        fs::read(fixture.0.join("config/shared.cfg")).unwrap(),
        b"client"
    );
    assert_eq!(
        fs::read(fixture.0.join("config/common.cfg")).unwrap(),
        b"both"
    );
    assert_eq!(fs::read(fixture.0.join("options.txt")).unwrap(), b"options");
    assert!(!fixture.0.join("config/exclusive.cfg").exists());
}

#[test]
fn rejects_invalid_client_destinations_and_ambiguous_metadata_before_writes() {
    let fixture = Fixture::new();
    let index = json!({"formatVersion":1,"files":[indexed_file("config/base.cfg", None)]});
    for path in [
        "client-overrides/../escaped.cfg",
        "client-overrides/saves/world.dat",
        "client-overrides/aurora-modpack.json",
        "client-overrides/config/base.cfg/nested",
        "client-overrides/config/BASE.cfg",
    ] {
        let mut zip = fixture.archive(&index, &[(path, b"invalid")]);
        assert!(
            client_pack_plan(&mut zip, &index, &fixture.0).is_err(),
            "{path}"
        );
    }
    for env in [
        json!({"client":"maybe"}),
        json!("optional"),
        json!({"client":true}),
    ] {
        let mut index = index.clone();
        index["files"][0]["env"] = env;
        let mut zip = fixture.archive(&index, &[]);
        assert!(client_pack_plan(&mut zip, &index, &fixture.0).is_err());
    }
    assert!(!fixture.0.join("config").exists());
}

#[test]
fn a_short_override_preserves_the_previous_contents_and_removes_staging() {
    let fixture = Fixture::new();
    let previous = fixture.0.join("options.txt");
    fs::write(&previous, b"existing options").unwrap();
    assert!(write_pack_file(&fixture.0, Path::new("options.txt"), &b"short"[..], 6).is_err());
    assert_eq!(fs::read(previous).unwrap(), b"existing options");
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
}
