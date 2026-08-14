use std::borrow::Cow;

use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase, ToolRouter},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{error::AppError, server::PrometheusServer};

#[derive(Debug, Serialize, JsonSchema)]
pub struct PrometheusOutput {
    /// Unmodified `data` field returned by the Prometheus HTTP API.
    data: Value,
}

fn read_only_annotations() -> Option<ToolAnnotations> {
    Some(ToolAnnotations::new().read_only(true).idempotent(true))
}

pub struct QueryPromql;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct QueryPromqlArgs {
    /// PromQL expression to execute.
    query: String,
    /// Optional evaluation timestamp, as RFC3339 or Unix seconds.
    #[serde(default)]
    time: Option<String>,
    /// Optional Prometheus query timeout, such as `30s`.
    #[serde(default)]
    timeout: Option<String>,
}

impl ToolBase for QueryPromql {
    type Parameter = QueryPromqlArgs;
    type Output = PrometheusOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "query_promql".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Execute a read-only instant PromQL query".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<PrometheusServer> for QueryPromql {
    async fn invoke(
        server: &PrometheusServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let mut params = vec![("query", args.query)];
        push_optional(&mut params, "time", args.time);
        push_optional(&mut params, "timeout", args.timeout);
        Ok(PrometheusOutput {
            data: server.client().get("/api/v1/query", &params).await?,
        })
    }
}

pub struct QueryPromqlRange;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct QueryPromqlRangeArgs {
    /// PromQL expression to execute.
    query: String,
    /// Start timestamp, as RFC3339 or Unix seconds.
    start: String,
    /// End timestamp, as RFC3339 or Unix seconds.
    end: String,
    /// Query resolution step, such as `30s`, `5m`, or seconds as a number.
    step: String,
    /// Optional Prometheus query timeout, such as `30s`.
    #[serde(default)]
    timeout: Option<String>,
}

impl ToolBase for QueryPromqlRange {
    type Parameter = QueryPromqlRangeArgs;
    type Output = PrometheusOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "query_promql_range".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Execute a read-only PromQL query over a time range".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<PrometheusServer> for QueryPromqlRange {
    async fn invoke(
        server: &PrometheusServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let mut params = vec![
            ("query", args.query),
            ("start", args.start),
            ("end", args.end),
            ("step", args.step),
        ];
        push_optional(&mut params, "timeout", args.timeout);
        Ok(PrometheusOutput {
            data: server.client().get("/api/v1/query_range", &params).await?,
        })
    }
}

pub struct ListMetricNames;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListMetricNamesArgs {
    /// Optional series selectors used to limit metric names, such as `{job="api"}`.
    #[serde(default)]
    matchers: Vec<String>,
    /// Optional start timestamp.
    #[serde(default)]
    start: Option<String>,
    /// Optional end timestamp.
    #[serde(default)]
    end: Option<String>,
}

impl ToolBase for ListMetricNames {
    type Parameter = ListMetricNamesArgs;
    type Output = PrometheusOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_metric_names".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Discover metric names, optionally restricted by series selectors and time".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<PrometheusServer> for ListMetricNames {
    async fn invoke(
        server: &PrometheusServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let mut params = matcher_params(args.matchers);
        push_optional(&mut params, "start", args.start);
        push_optional(&mut params, "end", args.end);
        Ok(PrometheusOutput {
            data: server
                .client()
                .get("/api/v1/label/__name__/values", &params)
                .await?,
        })
    }
}

pub struct ListLabelValues;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListLabelValuesArgs {
    /// Prometheus label name, such as `namespace`, `job`, or `pod`.
    label: String,
    /// Optional series selectors used to limit returned values.
    #[serde(default)]
    matchers: Vec<String>,
    /// Optional start timestamp.
    #[serde(default)]
    start: Option<String>,
    /// Optional end timestamp.
    #[serde(default)]
    end: Option<String>,
}

impl ToolBase for ListLabelValues {
    type Parameter = ListLabelValuesArgs;
    type Output = PrometheusOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_label_values".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List known values for a Prometheus label".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<PrometheusServer> for ListLabelValues {
    async fn invoke(
        server: &PrometheusServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        validate_label_name(&args.label)?;
        let mut params = matcher_params(args.matchers);
        push_optional(&mut params, "start", args.start);
        push_optional(&mut params, "end", args.end);
        Ok(PrometheusOutput {
            data: server
                .client()
                .get(&format!("/api/v1/label/{}/values", args.label), &params)
                .await?,
        })
    }
}

pub struct ListTargets;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListTargetsArgs {
    /// Optional target state: `active`, `dropped`, or `any`.
    #[serde(default)]
    state: Option<String>,
}

impl ToolBase for ListTargets {
    type Parameter = ListTargetsArgs;
    type Output = PrometheusOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_targets".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Inspect Prometheus scrape targets and their health".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<PrometheusServer> for ListTargets {
    async fn invoke(
        server: &PrometheusServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let mut params = Vec::new();
        push_optional(&mut params, "state", args.state);
        Ok(PrometheusOutput {
            data: server.client().get("/api/v1/targets", &params).await?,
        })
    }
}

fn matcher_params(matchers: Vec<String>) -> Vec<(&'static str, String)> {
    matchers
        .into_iter()
        .map(|matcher| ("match[]", matcher))
        .collect()
}

fn push_optional(
    params: &mut Vec<(&'static str, String)>,
    name: &'static str,
    value: Option<String>,
) {
    if let Some(value) = value {
        params.push((name, value));
    }
}

fn validate_label_name(label: &str) -> Result<(), AppError> {
    let mut characters = label.chars();
    let valid_first = characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic());
    if valid_first
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        Ok(())
    } else {
        Err(AppError::Prometheus(format!(
            "invalid Prometheus label name {label:?}"
        )))
    }
}

pub fn router() -> ToolRouter<PrometheusServer> {
    ToolRouter::new()
        .with_async_tool::<QueryPromql>()
        .with_async_tool::<QueryPromqlRange>()
        .with_async_tool::<ListMetricNames>()
        .with_async_tool::<ListLabelValues>()
        .with_async_tool::<ListTargets>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_only_read_only_tools() {
        let tools = router().list_all();
        assert!(tools.iter().all(|tool| {
            tool.annotations
                .as_ref()
                .and_then(|annotations| annotations.read_only_hint)
                == Some(true)
        }));

        let mut names: Vec<_> = tools
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "list_label_values",
                "list_metric_names",
                "list_targets",
                "query_promql",
                "query_promql_range",
            ]
        );
    }

    #[test]
    fn validates_prometheus_label_names() {
        assert!(validate_label_name("namespace").is_ok());
        assert!(validate_label_name("__name__").is_ok());
        assert!(validate_label_name("bad/label").is_err());
        assert!(validate_label_name("9bad").is_err());
    }
}
