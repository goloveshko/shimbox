//! The Windows version resource the runner carries into every shim (`2.2`).
//!
//! A generated `<alias>.exe` is a byte-for-byte copy of the runner, so whatever
//! this script writes is what a person sees when they right-click a shim and
//! open Properties - the only place the tool can explain one of its own files to
//! somebody who has never seen the README.

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

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    // The two variants differ by nothing but the subsystem, so they share every
    // line of this resource except the one that says which one it is.
    let detached = std::env::var_os("CARGO_FEATURE_GUI").is_some();

    let mut resource = tauri_winres::WindowsResource::new();
    let description = if detached {
        "shimbox shim (detached)"
    } else {
        "shimbox shim (console)"
    };
    resource.set("FileDescription", description);
    resource.set("ProductName", "shimbox");
    resource.set("OriginalFilename", "shim-runner.exe");
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
    resource.compile();
}
