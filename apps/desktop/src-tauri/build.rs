fn main() {
    if std::env::var_os("CARGO_FEATURE_SERVER").is_some() {
        let dist = std::path::Path::new("../../web/dist");
        if !dist.exists() {
            panic!(
                "apps/web/dist not found. Run `pnpm build:web` before building the server binary."
            );
        }
        println!("cargo:rerun-if-changed=../../web/dist");
    }

    #[cfg(feature = "desktop")]
    tauri_build::build();
}
