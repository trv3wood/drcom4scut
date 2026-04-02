use std::env;
use std::path::PathBuf;

fn main() {
    if env::var_os("CARGO_CFG_WINDOWS").is_some() {
        if let Some(path) = npcab_sdk_lib_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
        println!("cargo:rustc-link-lib=Packet");
    }
}

fn npcab_sdk_lib_dir() -> Option<PathBuf> {
    env::var_os("NPCAP_SDK_LIB")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("NPCAP_SDK_DIR").map(|dir| {
                let mut path = PathBuf::from(dir);
                path.push("Lib");
                path.push("x64");
                path
            })
        })
}
