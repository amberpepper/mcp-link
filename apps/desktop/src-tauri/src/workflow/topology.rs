use std::collections::HashMap;

use serde_json::Value;

use crate::util::json::value_id;

pub fn validate_workflow(workflow: &Value) -> Result<(), String> {
    if workflow
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("Workflow name is required".to_string());
    }
    if workflow
        .get("workflowType")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("Workflow type is required".to_string());
    }
    if workflow
        .get("nodes")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
    {
        return Err("Workflow must have at least one node".to_string());
    }
    if workflow.get("edges").and_then(Value::as_array).is_none() {
        return Err("Workflow edges must be an array".to_string());
    }
    if !is_valid_workflow(workflow) {
        return Err(
            "Workflow must be acyclic and include a valid Start -> MCP Call -> End path"
                .to_string(),
        );
    }
    Ok(())
}

pub fn is_valid_workflow(workflow: &Value) -> bool {
    let nodes = workflow
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let edges = workflow
        .get("edges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let start_node = nodes
        .iter()
        .find(|node| node.get("type").and_then(Value::as_str) == Some("start"));
    let end_node = nodes
        .iter()
        .find(|node| node.get("type").and_then(Value::as_str) == Some("end"));
    let mcp_call_node = nodes
        .iter()
        .find(|node| node.get("type").and_then(Value::as_str) == Some("mcp-call"));

    let (Some(start_node), Some(end_node), Some(mcp_call_node)) =
        (start_node, end_node, mcp_call_node)
    else {
        return false;
    };
    let (Some(start_id), Some(end_id), Some(mcp_call_id)) = (
        value_id(start_node),
        value_id(end_node),
        value_id(mcp_call_node),
    ) else {
        return false;
    };

    if workflow_has_cycle(&nodes, &edges) {
        return false;
    }

    workflow_has_path(&edges, start_id, mcp_call_id)
        && workflow_has_path(&edges, mcp_call_id, end_id)
}

pub fn workflow_has_path(edges: &[Value], from_id: &str, to_id: &str) -> bool {
    let mut adjacency = HashMap::<String, Vec<String>>::new();
    for edge in edges {
        let Some(source) = edge.get("source").and_then(Value::as_str) else {
            continue;
        };
        let Some(target) = edge.get("target").and_then(Value::as_str) else {
            continue;
        };
        adjacency
            .entry(source.to_string())
            .or_default()
            .push(target.to_string());
    }

    let mut visited = HashMap::<String, bool>::new();
    let mut queue = vec![from_id.to_string()];
    visited.insert(from_id.to_string(), true);

    while let Some(current) = queue.pop() {
        if current == to_id {
            return true;
        }
        for next in adjacency.get(&current).into_iter().flatten() {
            if !visited.contains_key(next) {
                visited.insert(next.clone(), true);
                queue.push(next.clone());
            }
        }
    }

    false
}

pub fn workflow_has_cycle(nodes: &[Value], edges: &[Value]) -> bool {
    determine_workflow_execution_order_from_parts(nodes, edges).is_err()
}

pub fn determine_workflow_execution_order(workflow: &Value) -> Result<Vec<String>, String> {
    let nodes = workflow
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let edges = workflow
        .get("edges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    determine_workflow_execution_order_from_parts(&nodes, &edges)
}

pub fn determine_workflow_execution_order_from_parts(
    nodes: &[Value],
    edges: &[Value],
) -> Result<Vec<String>, String> {
    let mut adjacency = HashMap::<String, Vec<String>>::new();
    let mut in_degree = HashMap::<String, usize>::new();

    for node in nodes {
        let Some(id) = value_id(node) else {
            continue;
        };
        adjacency.insert(id.to_string(), Vec::new());
        in_degree.insert(id.to_string(), 0);
    }

    for edge in edges {
        let Some(source) = edge.get("source").and_then(Value::as_str) else {
            continue;
        };
        let Some(target) = edge.get("target").and_then(Value::as_str) else {
            continue;
        };
        if !adjacency.contains_key(source) || !in_degree.contains_key(target) {
            continue;
        }
        adjacency
            .entry(source.to_string())
            .or_default()
            .push(target.to_string());
        *in_degree.entry(target.to_string()).or_default() += 1;
    }

    let mut queue = in_degree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then(|| id.clone()))
        .collect::<Vec<_>>();
    let mut execution_order = Vec::new();

    while let Some(current) = queue.pop() {
        execution_order.push(current.clone());
        for next in adjacency.get(&current).into_iter().flatten() {
            let Some(next_degree) = in_degree.get_mut(next) else {
                continue;
            };
            *next_degree = next_degree.saturating_sub(1);
            if *next_degree == 0 {
                queue.push(next.clone());
            }
        }
    }

    if execution_order.len() != in_degree.len() {
        return Err("Workflow contains a cycle".to_string());
    }

    Ok(execution_order)
}
