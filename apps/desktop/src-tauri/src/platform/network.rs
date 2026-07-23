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
    // Bind on all interfaces (not a real NIC; clients should still use a concrete host IP).
    push_interface(
        &mut entries,
        &mut seen,
        "All interfaces",
        IpAddr::from([0, 0, 0, 0]),
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
        address_sort_rank(left)
            .cmp(&address_sort_rank(right))
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

fn address_sort_rank(item: &Value) -> u8 {
    let address = item
        .get("address")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let is_loopback = item
        .get("isLoopback")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if is_loopback {
        0
    } else if address == "0.0.0.0" {
        1
    } else {
        2
    }
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
