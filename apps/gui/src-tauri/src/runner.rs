//! Staging of the prebuilt runner binaries into the GUI executable
//! (docs/techspec.md 2.2). The actual bytes are placed in `OUT_DIR` by
//! `build.rs`, which is what keeps the GUI from embedding a stale runner.

pub enum Variant {
    Cli,
    Gui,
}

impl Variant {
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Variant::Cli => include_bytes!(concat!(env!("OUT_DIR"), "/runner-cli.exe")),
            Variant::Gui => include_bytes!(concat!(env!("OUT_DIR"), "/runner-gui.exe")),
        }
    }
}
