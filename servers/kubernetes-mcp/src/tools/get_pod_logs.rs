use std::borrow::Cow;

use k8s_openapi::api::core::v1::Pod;
use kube::{Api, api::LogParams};
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::AppError, server::KubernetesServer};

use super::common::{default_namespace, read_only_annotations};

pub struct GetPodLogs;

const LOG_TAIL_LINES_ENV: &str = "KUBERNETES_MCP_LOG_TAIL_LINES";
const LOG_SINCE_SECONDS_ENV: &str = "KUBERNETES_MCP_LOG_SINCE_SECONDS";
const LOG_MAX_BYTES_ENV: &str = "KUBERNETES_MCP_LOG_MAX_BYTES";

#[derive(Clone, Copy)]
pub(crate) struct LogLimits {
    tail_lines: i64,
    since_seconds: i64,
    max_bytes: usize,
}

impl LogLimits {
    pub(crate) fn from_env() -> Result<Self, AppError> {
        let max_bytes = positive_env(LOG_MAX_BYTES_ENV, 32 * 1024)?;
        Ok(Self {
            tail_lines: positive_env(LOG_TAIL_LINES_ENV, 100)?,
            since_seconds: positive_env(LOG_SINCE_SECONDS_ENV, 15 * 60)?,
            max_bytes: usize::try_from(max_bytes).map_err(|_| {
                AppError::InvalidPositiveInteger {
                    name: LOG_MAX_BYTES_ENV,
                    value: max_bytes.to_string(),
                }
            })?,
        })
    }
}

fn positive_env(name: &'static str, default: i64) -> Result<i64, AppError> {
    let Ok(value) = std::env::var(name) else {
        return Ok(default);
    };
    value
        .parse::<i64>()
        .ok()
        .filter(|parsed| *parsed > 0)
        .ok_or(AppError::InvalidPositiveInteger { name, value })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPodLogsArgs {
    pub name: String,

    #[serde(default = "default_namespace")]
    pub namespace: String,

    #[serde(default)]
    pub container: Option<String>,

    #[serde(default)]
    pub previous: bool,

    #[serde(default)]
    pub tail_lines: Option<i64>,

    /// Lookback window in seconds. Defaults to the configured maximum.
    #[serde(default)]
    pub since_seconds: Option<i64>,
}

impl Default for GetPodLogsArgs {
    fn default() -> Self {
        Self {
            name: String::new(),
            namespace: default_namespace(),
            container: None,
            previous: false,
            tail_lines: None,
            since_seconds: None,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PodLogsOutput {
    logs: String,
    lines: usize,
    bytes: usize,
    truncated: bool,
    tail_lines: i64,
    since_seconds: i64,
}

impl ToolBase for GetPodLogs {
    type Parameter = GetPodLogsArgs;
    type Output = PodLogsOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "get_pod_logs".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Get logs from a Kubernetes pod container".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for GetPodLogs {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let limits = LogLimits::from_env()?;
        let tail_lines = args
            .tail_lines
            .unwrap_or(limits.tail_lines)
            .clamp(1, limits.tail_lines);
        let since_seconds = args
            .since_seconds
            .unwrap_or(limits.since_seconds)
            .clamp(1, limits.since_seconds);
        let pods: Api<Pod> = Api::namespaced(server.client(), &args.namespace);
        let raw_logs = pods
            .logs(
                &args.name,
                &LogParams {
                    container: args.container,
                    previous: args.previous,
                    tail_lines: Some(tail_lines),
                    since_seconds: Some(since_seconds),
                    limit_bytes: Some(limits.max_bytes as i64),
                    ..LogParams::default()
                },
            )
            .await?;
        let truncated = raw_logs.len() >= limits.max_bytes;
        let logs = truncate_utf8(raw_logs, limits.max_bytes);
        Ok(PodLogsOutput {
            lines: logs.lines().count(),
            bytes: logs.len(),
            logs,
            truncated,
            tail_lines,
            since_seconds,
        })
    }
}

fn truncate_utf8(mut value: String, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value;
    }
    let mut boundary = max_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_cap_preserves_utf8() {
        let max_bytes = 32 * 1024;
        let logs = "aé".repeat(max_bytes);
        let truncated = truncate_utf8(logs, max_bytes);
        assert!(truncated.len() <= max_bytes);
        assert!(truncated.is_char_boundary(truncated.len()));
    }
}
