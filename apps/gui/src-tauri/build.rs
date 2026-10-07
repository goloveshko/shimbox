use std::path::PathBuf;

/// The runner must be built before the GUI (`scripts\build.cmd` guarantees the
/// order). Copying it into `OUT_DIR` and watching it is what keeps a rebuilt
/// runner from hiding behind an already-cached GUI binary.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest
        .ancestors()
        .nth(3)
        .expect("unexpected manifest depth")
        .to_path_buf();
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));

    // `tauri.conf.json` is the only way to put a version into the file's own
    // Windows properties, and the workspace is the only way to put it anywhere
    // else, so the number is written twice and this is where the two are held
    // level: a release that disagrees with itself fails here rather than in
    // somebody's Properties dialog.
    println!("cargo:rerun-if-changed=tauri.conf.json");
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(manifest.join("tauri.conf.json")).expect("tauri.conf.json"),
    )
    .expect("tauri.conf.json does not read as JSON");
    let declared = config
        .get("version")
        .and_then(|version| version.as_str())
        .unwrap_or_default();
    let built = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    assert_eq!(
        declared, built,
        "tauri.conf.json says {declared} and the workspace says {built}"
    );

    for name in ["runner-cli.exe", "runner-gui.exe"] {
        let source = root.join("bin").join(name);
        if !source.is_file() {
            panic!(
                "missing {}\nbuild the runner first: scripts\\build.cmd",
                source.display()
            );
        }
        println!("cargo:rerun-if-changed={}", source.display());
        std::fs::copy(&source, out_dir.join(name))
            .unwrap_or_else(|error| panic!("cannot stage {}: {error}", source.display()));
    }

    tauri_build::build()
}
