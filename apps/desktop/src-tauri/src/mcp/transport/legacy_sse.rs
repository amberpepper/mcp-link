use std::{error::Error, fmt};

use futures::StreamExt;
use reqwest::{
    header::{HeaderMap, ACCEPT, CONTENT_TYPE},
    Url,
};
use rmcp::{
    service::RoleClient,
    transport::worker::{Worker, WorkerConfig, WorkerContext, WorkerQuitReason},
};

#[derive(Debug)]
pub enum LegacySseError {
    Closed,
    Join(tokio::task::JoinError),
    Http(reqwest::Error),
    Sse(sse_stream::Error),
    Json(serde_json::Error),
    InvalidUrl(String),
    MissingEndpoint,
}

impl fmt::Display for LegacySseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(formatter, "legacy SSE transport closed"),
            Self::Join(error) => write!(formatter, "legacy SSE transport join error: {error}"),
            Self::Http(error) => write!(formatter, "legacy SSE HTTP error: {error}"),
            Self::Sse(error) => write!(formatter, "legacy SSE stream error: {error}"),
            Self::Json(error) => write!(formatter, "legacy SSE JSON error: {error}"),
            Self::InvalidUrl(error) => write!(formatter, "legacy SSE invalid URL: {error}"),
            Self::MissingEndpoint => {
                write!(formatter, "legacy SSE endpoint event was not received")
            }
        }
    }
}

impl Error for LegacySseError {}

impl From<reqwest::Error> for LegacySseError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<sse_stream::Error> for LegacySseError {
    fn from(error: sse_stream::Error) -> Self {
        Self::Sse(error)
    }
}

impl From<serde_json::Error> for LegacySseError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone)]
pub struct LegacySseClientTransport {
    url: String,
    headers: HeaderMap,
}

impl LegacySseClientTransport {
    pub fn new(url: String, headers: HeaderMap) -> Self {
        Self { url, headers }
    }
}

impl Worker for LegacySseClientTransport {
    type Error = LegacySseError;
    type Role = RoleClient;

    fn err_closed() -> Self::Error {
        LegacySseError::Closed
    }

    fn err_join(error: tokio::task::JoinError) -> Self::Error {
        LegacySseError::Join(error)
    }

    fn config(&self) -> WorkerConfig {
        let mut config = WorkerConfig::default();
        config.name = Some("legacy-sse-client".to_string());
        config.channel_buffer_capacity = 16;
        config
    }

    async fn run(
        self,
        mut context: WorkerContext<Self>,
    ) -> Result<(), WorkerQuitReason<Self::Error>> {
        let client = reqwest::Client::new();
        let base_url = Url::parse(&self.url).map_err(|error| {
            WorkerQuitReason::fatal(
                LegacySseError::InvalidUrl(error.to_string()),
                "parse SSE URL",
            )
        })?;
        let response = client
            .get(base_url.clone())
            .headers(self.headers.clone())
            .header(ACCEPT, "text/event-stream")
            .send()
            .await
            .map_err(|error| {
                WorkerQuitReason::fatal(LegacySseError::Http(error), "connect SSE stream")
            })?
            .error_for_status()
            .map_err(|error| {
                WorkerQuitReason::fatal(LegacySseError::Http(error), "open SSE stream")
            })?;
        let mut stream = sse_stream::SseStream::from_byte_stream(response.bytes_stream());
        let cancellation_token = context.cancellation_token.clone();

        let endpoint = loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    return Err(WorkerQuitReason::Cancelled);
                }
                event = stream.next() => {
                    let Some(event) = event else {
                        return Err(WorkerQuitReason::TransportClosed);
                    };
                    let event = event.map_err(|error| {
                        WorkerQuitReason::fatal(LegacySseError::Sse(error), "read SSE endpoint")
                    })?;
                    if event.event.as_deref() == Some("endpoint") {
                        let endpoint = event.data.ok_or_else(|| {
                            WorkerQuitReason::fatal(LegacySseError::MissingEndpoint, "read SSE endpoint")
                        })?;
                        break resolve_legacy_sse_endpoint(&base_url, &endpoint)?;
                    }
                    if event.event.is_none() {
                        if let Some(data) = event.data {
                            let message = serde_json::from_str(&data).map_err(|error| {
                                WorkerQuitReason::fatal(LegacySseError::Json(error), "parse SSE message")
                            })?;
                            context.send_to_handler(message).await?;
                        }
                    }
                }
            }
        };

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    return Err(WorkerQuitReason::Cancelled);
                }
                event = stream.next() => {
                    let Some(event) = event else {
                        return Err(WorkerQuitReason::TransportClosed);
                    };
                    let event = event.map_err(|error| {
                        WorkerQuitReason::fatal(LegacySseError::Sse(error), "read SSE message")
                    })?;
                    if event.event.is_none() {
                        if let Some(data) = event.data {
                            let message = serde_json::from_str(&data).map_err(|error| {
                                WorkerQuitReason::fatal(LegacySseError::Json(error), "parse SSE message")
                            })?;
                            context.send_to_handler(message).await?;
                        }
                    }
                }
                request = context.recv_from_handler() => {
                    let request = request?;
                    let result = post_legacy_sse_message(
                        &client,
                        endpoint.clone(),
                        self.headers.clone(),
                        request.message,
                    )
                    .await;
                    let _ = request.responder.send(result);
                }
            }
        }
    }
}

pub fn resolve_legacy_sse_endpoint(
    base_url: &Url,
    endpoint: &str,
) -> Result<Url, WorkerQuitReason<LegacySseError>> {
    base_url
        .join(endpoint)
        .or_else(|_| Url::parse(endpoint))
        .map_err(|error| {
            WorkerQuitReason::fatal(
                LegacySseError::InvalidUrl(error.to_string()),
                "resolve SSE endpoint",
            )
        })
}

async fn post_legacy_sse_message(
    client: &reqwest::Client,
    endpoint: Url,
    headers: HeaderMap,
    message: rmcp::model::ClientJsonRpcMessage,
) -> Result<(), LegacySseError> {
    client
        .post(endpoint)
        .headers(headers)
        .header(CONTENT_TYPE, "application/json")
        .json(&message)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
