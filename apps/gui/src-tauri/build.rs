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
