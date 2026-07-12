use serde_json::{json, Value};

pub(crate) async fn fetch_registry_servers(options: Option<&Value>) -> Result<Value, String> {
    let limit = options
        .and_then(|value| value.get("limit"))
        .and_then(Value::as_u64)
        .unwrap_or(100)
        .clamp(1, 100);
    let mut next_cursor = options
        .and_then(|value| value.get("cursor"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let fetch_all = options
        .and_then(|value| value.get("fetchAll"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let max_pages = options
        .and_then(|value| value.get("maxPages"))
        .and_then(Value::as_u64)
        .unwrap_or(100)
        .clamp(1, 100);
    let search = options
        .and_then(|value| value.get("search"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_lowercase();

    let client = reqwest::Client::new();
    let mut pages = 0_u64;
    let mut last_metadata = json!({});
    let mut servers = Vec::new();

    loop {
        let mut query = vec![("limit", limit.to_string())];
        if !next_cursor.is_empty() {
            query.push(("cursor", next_cursor.clone()));
        }

        let body = client
            .get("https://registry.modelcontextprotocol.io/v0/servers")
            .query(&query)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json::<Value>()
            .await
            .map_err(|error| error.to_string())?;

        let page_servers = body
            .get("servers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let page_count = page_servers.len();
        servers.extend(page_servers);

        last_metadata = body.get("metadata").cloned().unwrap_or_else(|| json!({}));
        next_cursor = last_metadata
            .get("nextCursor")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        pages += 1;

        if !fetch_all || page_count == 0 || next_cursor.is_empty() || pages >= max_pages {
            break;
        }
    }

    let fetched_count = servers.len();

    if !search.is_empty() {
        servers.retain(|entry| {
            let server = entry.get("server").unwrap_or(&Value::Null);
            ["name", "title", "description", "websiteUrl"]
                .iter()
                .any(|key| {
                    server
                        .get(*key)
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.to_lowercase().contains(&search))
                })
                || server
                    .get("repository")
                    .and_then(|repository| repository.get("url"))
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.to_lowercase().contains(&search))
        });
    }

    Ok(json!({
        "servers": servers,
        "metadata": {
            "count": servers.len(),
            "fetchedCount": fetched_count,
            "hasMore": !next_cursor.is_empty(),
            "nextCursor": next_cursor,
            "pages": pages
        }
    }))
}
