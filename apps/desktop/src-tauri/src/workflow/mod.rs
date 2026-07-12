use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

pub mod executor;
pub mod topology;

pub type McpValueFuture<'a> = Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>>;
pub type McpValueHandler<'a> = dyn Fn() -> McpValueFuture<'a> + Send + Sync + 'a;
