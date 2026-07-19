fn main() {
    generate_bundled_agent_plugins();

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

fn generate_bundled_agent_plugins() {
    use std::{env, fs, path::PathBuf};

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let agents_root = manifest_dir.join("../../../plugins/agents");
    let list_path = agents_root.join("bundled.txt");
    println!("cargo:rerun-if-changed={}", list_path.display());
    let list = fs::read_to_string(&list_path)
        .unwrap_or_else(|error| panic!("Failed to read {}: {error}", list_path.display()));
    let mut entries = String::new();
    for id in list.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        {
            panic!("Invalid bundled Agent plugin id: {id}");
        }
        let package = agents_root.join("dist").join(format!("{id}.mclagent"));
        if !package.is_file() {
            panic!(
                "Bundled Agent plugin package is missing: {}",
                package.display()
            );
        }
        println!("cargo:rerun-if-changed={}", package.display());
        entries.push_str(&format!(
            "    ({id:?}, include_bytes!({path:?}) as &[u8]),\n",
            path = package.to_string_lossy(),
        ));
    }
    let generated =
        format!("pub(crate) static BUNDLED_AGENT_PLUGINS: &[(&str, &[u8])] = &[\n{entries}];\n");
    let output =
        PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("bundled_agent_plugins.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("Failed to write {}: {error}", output.display()));
}
