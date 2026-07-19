use super::*;

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("mcp-link-agent-plugin-{}", uuid::Uuid::new_v4()))
}

fn test_manifest() -> ExternalPluginManifest {
    ExternalPluginManifest {
        schema_version: 2,
        id: "test-agent".to_string(),
        name: "Test Agent".to_string(),
        version: "1.0.0".to_string(),
        description: String::new(),
        icon: None,
        capabilities: Vec::new(),
        instance_config: ManifestInstanceConfig::default(),
        config_files: Vec::new(),
        databases: Vec::new(),
        files: Vec::new(),
        skill_targets: Vec::new(),
        runtime: Some(PluginRuntime {
            kind: "wasm".to_string(),
            entry: "plugin.wasm".to_string(),
        }),
    }
}

#[test]
fn plugin_loads_from_valid_manifest() {
    let root = temp_dir();
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("plugin.wasm"), b"runtime").unwrap();
    fs::write(root.join(PLUGIN_MARKER), "test-agent").unwrap();
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&test_manifest()).unwrap(),
    )
    .unwrap();
    let plugin = load_plugin(root.clone()).unwrap();
    assert_eq!(plugin.manifest.id, "test-agent");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_paths_cannot_escape_plugin_root() {
    assert!(is_safe_relative_path("bin/agent.exe"));
    assert!(!is_safe_relative_path("../agent.exe"));
    assert!(!is_safe_relative_path("/agent.exe"));
    assert!(!is_safe_relative_path(""));
}

#[test]
#[ignore = "uses the locally configured AI CLI instances"]
fn configured_management_plugins_load_real_sections() {
    let local_app_data = std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA");
    let source = PathBuf::from(local_app_data)
        .join("MCP Link")
        .join("mcp.db");
    assert!(source.is_file(), "MCP Link state database is missing");
    let root = temp_dir();
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("mcp.db");
    fs::copy(&source, &state_path).unwrap();
    for suffix in ["-wal", "-shm"] {
        let source_sidecar = PathBuf::from(format!("{}{suffix}", source.display()));
        if source_sidecar.is_file() {
            fs::copy(
                &source_sidecar,
                PathBuf::from(format!("{}{suffix}", state_path.display())),
            )
            .unwrap();
        }
    }
    let state = DesktopState::load(state_path);
    let package_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("plugins")
        .join("agents")
        .join("dist");
    for entry in fs::read_dir(package_root).unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("mclagent") {
            install_plugin_package(&state, fs::read(path).unwrap()).unwrap();
        }
    }
    let plugins = load_plugins(&state);
    let instances = state.store.lock().unwrap().agent_instances.clone();
    let mut validated = 0;
    for plugin in plugins.iter().filter(|plugin| {
        plugin
            .manifest
            .capabilities
            .iter()
            .any(|capability| capability == "management.read")
    }) {
        let agent_id = plugin.manifest.id.as_str();
        let Some(instance_id) = instances.iter().find_map(|instance| {
            (instance.get("agentId").and_then(Value::as_str) == Some(agent_id))
                .then(|| instance.get("id").and_then(Value::as_str))
                .flatten()
        }) else {
            continue;
        };
        let descriptor = describe_management(&state, plugin, instance_id).unwrap();
        let sections = descriptor["sections"]
            .as_array()
            .expect("management sections");
        assert!(!sections.is_empty());
        for section in sections {
            let id = section["id"].as_str().expect("section id");
            if section["source"].as_str() == Some("host") {
                continue;
            }
            let loaded = load_management_section(&state, plugin, instance_id, id)
                .unwrap_or_else(|error| panic!("{agent_id}/{id}: {error}"));
            assert_eq!(loaded["id"], id);
            assert!(loaded["revision"].as_str().is_some());
        }
        validated += 1;
    }
    assert!(
        validated > 0,
        "expected at least one configured management plugin"
    );
    drop(state);
    fs::remove_dir_all(root).unwrap();
}
