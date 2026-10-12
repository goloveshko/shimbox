// Both runners carry the same resource and differ in the two lines that name
// which one of them this is; the description of it lives beside the library.
include!("../runner/winres.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    version_resource("shimbox shim (detached)", "runner-gui.exe");
}
