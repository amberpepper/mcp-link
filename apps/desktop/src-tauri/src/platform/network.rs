use std::{collections::BTreeSet, net::IpAddr};

use serde_json::{json, Value};

pub(crate) fn list_network_interfaces() -> Result<Value, String> {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();

    push_interface(
        &mut entries,
        &mut seen,
        "Loopback",
        IpAddr::from([127, 0, 0, 1]),
    );

    let interfaces = if_addrs::get_if_addrs().map_err(|error| error.to_string())?;
    for interface in interfaces {
        let ip = interface.ip();
        if !ip.is_ipv4() {
            continue;
        }
        push_interface(&mut entries, &mut seen, &interface.name, ip);
    }

    entries.sort_by(|left, right| {
        let left_loopback = left
            .get("isLoopback")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let right_loopback = right
            .get("isLoopback")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        right_loopback
            .cmp(&left_loopback)
            .then_with(|| {
                left.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
            .then_with(|| {
                left.get("address")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("address")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
    });

    Ok(Value::Array(entries))
}

fn push_interface(entries: &mut Vec<Value>, seen: &mut BTreeSet<String>, name: &str, ip: IpAddr) {
    let address = ip.to_string();
    if !seen.insert(address.clone()) {
        return;
    }
    entries.push(json!({
        "name": name,
        "address": address,
        "family": if ip.is_ipv4() { "ipv4" } else { "ipv6" },
        "isLoopback": ip.is_loopback(),
        "label": format!("{address} ({name})")
    }));
}
