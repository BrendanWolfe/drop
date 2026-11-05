fn main() {
    println!("cargo:rerun-if-env-changed=DROP_UPDATER_PUBLIC_KEY");
    println!("cargo:rerun-if-env-changed=DROP_RELEASE_TAG");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Emit libarchive's static transitive dependencies as well as archive.lib.
        // libarchive3-sys locates archive itself through LIBARCHIVE_* below.
        vcpkg::find_package("libarchive")
            .expect("Install libarchive with vcpkg; see desktop/LOCAL_GAMES.md");
    }
    tauri_build::build();
}
