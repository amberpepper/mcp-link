use serde_json::{Map, Value};

pub fn sanitize_for_security_boundary(value: &Value) -> Value {
    match value {
        Value::Array(items) => {
            Value::Array(items.iter().map(sanitize_for_security_boundary).collect())
        }
        Value::Object(object) => {
            let mut output = Map::new();
            for (key, value) in object {
                if is_sensitive_key(key) {
                    output.insert(key.clone(), Value::String("[REDACTED]".to_string()));
                } else {
                    output.insert(key.clone(), sanitize_for_security_boundary(value));
                }
            }
            Value::Object(output)
        }
        _ => value.clone(),
    }
}

pub fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_lowercase();
    key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("authorization")
        || key.contains("api_key")
        || key.contains("apikey")
}
