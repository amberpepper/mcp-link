use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime},
};

use serde_json::{json, Value};
use wasmtime::{Caller, Config, Engine, Extern, Linker, Module, Store};

use crate::util::time::now_millis;

use super::super::model::AgentInstance;
use super::{host, ExternalPlugin};

const MAX_WASM_RESPONSE_SIZE: usize = 128 * 1024 * 1024;
const WASM_FUEL: u64 = 2_000_000_000;
const WASM_EPOCH_INTERVAL: Duration = Duration::from_millis(100);
const WASM_TIMEOUT_TICKS: u64 = 100;

static WASM_ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
static WASM_MODULES: OnceLock<Mutex<HashMap<PathBuf, CachedModule>>> = OnceLock::new();

#[derive(Clone)]
struct CachedModule {
    len: u64,
    modified: Option<SystemTime>,
    module: Module,
}

struct HostState {
    plugin: ExternalPlugin,
    instance: Option<AgentInstance>,
}

pub(crate) fn invoke(
    plugin: &ExternalPlugin,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let entry = plugin
        .manifest
        .runtime
        .as_ref()
        .map(|runtime| runtime.entry.as_str())
        .ok_or_else(|| "WASM plugin entry is missing".to_string())?;
    let engine = shared_engine()?;
    let module = cached_module(engine, &plugin.root.join(entry))?;
    let mut linker = Linker::new(engine);
    linker
        .func_wrap(
            "mcp_link",
            "host_call",
            |mut caller: Caller<'_, HostState>,
             request_ptr: i32,
             request_len: i32,
             output_ptr: i32,
             output_capacity: i32|
             -> i32 {
                host_call(
                    &mut caller,
                    request_ptr,
                    request_len,
                    output_ptr,
                    output_capacity,
                )
            },
        )
        .map_err(|error| error.to_string())?;

    let instance_value = params
        .get("instance")
        .cloned()
        .map(serde_json::from_value::<AgentInstance>)
        .transpose()
        .map_err(|error| error.to_string())?;
    let mut store = Store::new(
        engine,
        HostState {
            plugin: plugin.clone(),
            instance: instance_value,
        },
    );
    store
        .set_fuel(WASM_FUEL)
        .map_err(|error| error.to_string())?;
    store.set_epoch_deadline(WASM_TIMEOUT_TICKS);
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|error| error.to_string())?;
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| "WASM plugin does not export memory".to_string())?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut store, "mcp_link_alloc")
        .map_err(|error| error.to_string())?;
    let dealloc = instance
        .get_typed_func::<(i32, i32), ()>(&mut store, "mcp_link_dealloc")
        .map_err(|error| error.to_string())?;
    let call = instance
        .get_typed_func::<(i32, i32), i64>(&mut store, "mcp_link_call")
        .map_err(|error| error.to_string())?;
    let request = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": now_millis(),
        "method": method,
        "params": params,
    }))
    .map_err(|error| error.to_string())?;
    let request_len =
        i32::try_from(request.len()).map_err(|_| "WASM plugin request is too large".to_string())?;
    let request_ptr = alloc
        .call(&mut store, request_len)
        .map_err(|error| error.to_string())?;
    memory
        .write(&mut store, request_ptr as usize, &request)
        .map_err(|error| error.to_string())?;
    let packed = call
        .call(&mut store, (request_ptr, request_len))
        .map_err(|error| error.to_string())? as u64;
    let response_ptr = (packed >> 32) as u32 as usize;
    let response_len = (packed & 0xffff_ffff) as u32 as usize;
    if response_len > MAX_WASM_RESPONSE_SIZE {
        return Err("WASM plugin response is too large".to_string());
    }
    let mut response = vec![0_u8; response_len];
    memory
        .read(&store, response_ptr, &mut response)
        .map_err(|error| error.to_string())?;
    dealloc
        .call(&mut store, (request_ptr, request_len))
        .map_err(|error| error.to_string())?;
    dealloc
        .call(
            &mut store,
            (
                response_ptr as i32,
                i32::try_from(response_len).unwrap_or(i32::MAX),
            ),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&response).map_err(|error| error.to_string())
}

fn shared_engine() -> Result<&'static Engine, String> {
    let result = WASM_ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|error| error.to_string())?;
        let ticker = engine.clone();
        std::thread::Builder::new()
            .name("mcp-link-wasm-epoch".to_string())
            .spawn(move || loop {
                std::thread::sleep(WASM_EPOCH_INTERVAL);
                ticker.increment_epoch();
            })
            .map_err(|error| error.to_string())?;
        Ok(engine)
    });
    result.as_ref().map_err(Clone::clone)
}

fn cached_module(engine: &Engine, path: &Path) -> Result<Module, String> {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    let len = metadata.len();
    let modified = metadata.modified().ok();
    let modules = WASM_MODULES.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some(module) = modules
        .lock()
        .map_err(|_| "Failed to lock WASM module cache".to_string())?
        .get(&path)
        .filter(|cached| cached.len == len && cached.modified == modified)
        .map(|cached| cached.module.clone())
    {
        return Ok(module);
    }

    let module = Module::from_file(engine, &path).map_err(|error| error.to_string())?;
    modules
        .lock()
        .map_err(|_| "Failed to lock WASM module cache".to_string())?
        .insert(
            path,
            CachedModule {
                len,
                modified,
                module: module.clone(),
            },
        );
    Ok(module)
}

fn host_call(
    caller: &mut Caller<'_, HostState>,
    request_ptr: i32,
    request_len: i32,
    output_ptr: i32,
    output_capacity: i32,
) -> i32 {
    let result = (|| {
        let memory = caller
            .get_export("memory")
            .and_then(Extern::into_memory)
            .ok_or_else(|| "WASM plugin memory is unavailable".to_string())?;
        let request = read_memory(caller, &memory, request_ptr, request_len)?;
        let request: Value = serde_json::from_slice(&request).map_err(|error| error.to_string())?;
        let instance = caller
            .data()
            .instance
            .as_ref()
            .ok_or_else(|| "WASM host call requires an Agent instance".to_string())?;
        let response = match host::dispatch(&caller.data().plugin, instance, &request) {
            Ok(value) => json!({ "result": value }),
            Err(error) => json!({ "error": error }),
        };
        let bytes = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_WASM_RESPONSE_SIZE {
            return Err("WASM host response is too large".to_string());
        }
        if output_capacity < 0 || (output_capacity as usize) < bytes.len() {
            return i32::try_from(bytes.len())
                .map_err(|_| "WASM host response is too large".to_string());
        }
        memory
            .write(caller, output_ptr as usize, &bytes)
            .map_err(|error| error.to_string())?;
        i32::try_from(bytes.len()).map_err(|_| "WASM host response is too large".to_string())
    })();
    result.unwrap_or(-1)
}

fn read_memory(
    caller: &Caller<'_, HostState>,
    memory: &wasmtime::Memory,
    pointer: i32,
    length: i32,
) -> Result<Vec<u8>, String> {
    if pointer < 0 || length < 0 {
        return Err("WASM memory range is invalid".to_string());
    }
    let start = pointer as usize;
    let end = start
        .checked_add(length as usize)
        .ok_or_else(|| "WASM memory range overflowed".to_string())?;
    let data = memory.data(caller);
    data.get(start..end)
        .map(ToOwned::to_owned)
        .ok_or_else(|| "WASM memory range is outside the module memory".to_string())
}
