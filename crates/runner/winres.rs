// The Windows version resource a shim carries (`2.2`).
//
// A generated `<alias>.exe` is a byte-for-byte copy of a runner, so whatever
// this writes is what a person sees when they right-click a shim and open
// Properties - the only place the tool can explain one of its own files to
// somebody who has never read the README.
//
// It is one file and not two because the two executables differ in exactly two
// lines of it: a build script belongs to its own package, so each runner crate
// needs one, and both of them include this and say which variant they are. A
// file that is included rather than compiled on its own has no crate to put a
// doc comment on, which is why these are comments.

/// The four numbers Windows shows, packed the way `VERSIONINFO` wants them.
fn packed(version: &str) -> u64 {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let mut parts = core.split('.').map(|part| part.parse::<u64>().unwrap_or(0));
    let mut packed = 0;
    for shift in [48, 32, 16] {
        packed |= parts.next().unwrap_or(0).min(0xffff) << shift;
    }
    packed | parts.next().unwrap_or(0).min(0xffff)
}

/// Writes and compiles the resource of one runner variant.
fn version_resource(description: &str, original_filename: &str) {
    // This file arrives at both scripts through one `include!`, so watching the
    // script is not enough - the watched path is relative to the including
    // package, next to which this file sits.
    println!("cargo:rerun-if-changed=../runner/winres.rs");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let mut resource = tauri_winres::WindowsResource::new();
    resource.set("FileDescription", description);
    resource.set("ProductName", "shimbox");
    resource.set("OriginalFilename", original_filename);
    resource.set(
        "Comments",
        "A shim: reads the .shim file of the same name beside it and launches the target that file names.",
    );
    resource.set("CompanyName", "Sergey Goloveshko");
    resource.set("LegalCopyright", "Copyright (c) 2026 Sergey Goloveshko");
    resource.set("FileVersion", &version);
    resource.set("ProductVersion", &version);
    resource.set_version_info(tauri_winres::VersionInfo::FILEVERSION, packed(&version));
    resource.set_version_info(tauri_winres::VersionInfo::PRODUCTVERSION, packed(&version));
    // What is left for `compile` to report is a resource file it could not write,
    // and a runner built without the version resource this describes is a wrong
    // artifact, so the build stops rather than ships one quietly.
    resource
        .compile()
        .expect("failed to write and compile the version resource");
}
