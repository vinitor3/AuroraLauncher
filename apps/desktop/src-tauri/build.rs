use serde_json::Value;
use std::{env, fmt::Write, fs, path::PathBuf};

fn main() {
    const MATRIX: &str = "../../../scripts/active-mod-matrix.json";
    println!("cargo:rerun-if-changed={MATRIX}");
    let matrix: Value =
        serde_json::from_str(include_str!("../../../scripts/active-mod-matrix.json"))
            .expect("invalid mod matrix");
    let targets = matrix["targets"].as_array().expect("missing mod targets");
    let mut bindings = String::new();
    for module in matrix["modules"].as_array().expect("missing mod modules") {
        let name = module["name"].as_str().expect("missing module name");
        let version = module["version"].as_str().expect("missing module version");
        assert!(matches!(name, "core" | "skins"));
        if name == "core" {
            writeln!(bindings, "pub(super) const CORE_MANIFEST: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../../apps/aurora-core/compatibility-manifest-{version}.json\"));").unwrap();
            writeln!(bindings, "pub(super) const CORE_PUBLIC_KEY: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../../apps/aurora-core/trusted-public-key-{version}.txt\"));").unwrap();
            writeln!(
                bindings,
                "pub(super) const CORE_KEY_ID: &str = \"aurora-core-{version}-release\";"
            )
            .unwrap();
        }
        writeln!(bindings, "pub(super) fn {name}(minecraft: &str, loader: &str) -> Option<&'static [u8]> {{ match (minecraft, loader) {{").unwrap();
        for target in targets {
            let mc = target["minecraft"]
                .as_str()
                .expect("missing Minecraft version");
            let loader = target["loader"].as_str().expect("missing mod loader");
            let jar = format!("/../../../releases/{name}/{version}/{mc}/{loader}/aurora-{name}-{loader}-{mc}-{version}.jar");
            writeln!(bindings, "({mc:?}, {loader:?}) => Some(include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {jar:?}))),").unwrap();
        }
        bindings.push_str("_ => None, } }\n");
    }
    bindings.push_str(
        "pub(super) fn fabric_minimum(minecraft: &str) -> [u32; 3] { match minecraft {\n",
    );
    for target in targets.iter().filter(|target| target["loader"] == "fabric") {
        let mc = target["minecraft"]
            .as_str()
            .expect("missing Minecraft version");
        let minimum: Vec<u32> = target["loaderVersionRange"]
            .as_str()
            .and_then(|range| range.strip_prefix(">="))
            .expect("missing minimum Fabric version")
            .split('.')
            .map(|part| part.parse().expect("invalid Fabric version"))
            .collect();
        assert_eq!(minimum.len(), 3, "invalid Fabric minimum in matrix");
        writeln!(bindings, "{mc:?} => {minimum:?},").unwrap();
    }
    bindings.push_str("_ => [0, 0, 0], } }\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    fs::write(out.join("aurora_modules.rs"), bindings).expect("write embedded mod bindings");
    tauri_build::build()
}
