use boa_engine::{Context as JsContext, JsValue, Source};
use serde_json::{json, Value};

pub fn execute_hook_script(script: &str, context: &Value) -> Result<Value, String> {
    let context_json = serde_json::to_string(context).map_err(|error| error.to_string())?;
    let mut js_context = JsContext::default();
    js_context
        .runtime_limits_mut()
        .set_loop_iteration_limit(1_000_000);
    js_context.runtime_limits_mut().set_recursion_limit(256);

    let wrapped_script = format!(
        r#"
globalThis.__hookResult = undefined;
globalThis.__hookError = undefined;
const context = {context_json};
const console = {{
  log: function() {{}},
  warn: function() {{}},
  error: function() {{}}
}};
Promise.resolve((async function() {{
{script}
}})()).then(
  function(value) {{ globalThis.__hookResult = value === undefined ? null : value; }},
  function(error) {{ globalThis.__hookError = error && error.message ? error.message : String(error); }}
);
"#
    );

    js_context
        .eval(Source::from_bytes(wrapped_script.as_bytes()))
        .map_err(|error| format!("Hook execution failed: {error}"))?;
    js_context
        .run_jobs()
        .map_err(|error| format!("Hook execution failed: {error}"))?;

    let hook_error = js_context
        .eval(Source::from_bytes(
            "globalThis.__hookError === undefined ? null : globalThis.__hookError",
        ))
        .map_err(|error| format!("Hook execution failed: {error}"))?;
    if !hook_error.is_null() && !hook_error.is_undefined() {
        return Err(format!(
            "Hook execution failed: {}",
            js_value_to_string(&hook_error)
        ));
    }

    let hook_result = js_context
        .eval(Source::from_bytes(
            "globalThis.__hookResult === undefined ? null : globalThis.__hookResult",
        ))
        .map_err(|error| format!("Hook execution failed: {error}"))?;
    Ok(hook_result
        .to_json(&mut js_context)
        .map_err(|error| format!("Hook execution failed: {error}"))?
        .unwrap_or(Value::Null))
}

pub fn validate_hook_script(script: &str) -> Value {
    let mut js_context = JsContext::default();
    let wrapped_script = format!("(async function() {{\n{script}\n}})");
    match js_context.eval(Source::from_bytes(wrapped_script.as_bytes())) {
        Ok(_) => json!({ "valid": true }),
        Err(error) => json!({
            "valid": false,
            "error": error.to_string()
        }),
    }
}

fn js_value_to_string(value: &JsValue) -> String {
    if let Some(string) = value.as_string() {
        return string.to_std_string_lossy();
    }
    value.display().to_string()
}

pub fn validate_hook_module(hook: &Value) -> Result<(), String> {
    if hook
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("Hook module name is required".to_string());
    }
    let script = hook
        .get("script")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if script.trim().is_empty() {
        return Err("Hook module script is required".to_string());
    }
    let validation = validate_hook_script(script);
    if validation.get("valid").and_then(Value::as_bool) != Some(true) {
        return Err(format!(
            "Invalid hook script: {}",
            validation
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Invalid JavaScript syntax")
        ));
    }
    Ok(())
}
