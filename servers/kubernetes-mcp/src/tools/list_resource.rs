use std::borrow::Cow;

use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::AppError, server::KubernetesServer};

use super::common::{LIST, dynamic_api, read_only_annotations, resolve_resource};

pub struct ListResource;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum OutputMode {
    #[default]
    Summary,
    Full,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListResourceArgs {
    /// Resource plural or Kind, such as `services` or `Service`.
    resource: String,

    /// Namespace for namespaced resources.
    #[serde(default)]
    namespace: Option<String>,

    /// Must be true to explicitly list a namespaced resource across all namespaces.
    #[serde(default)]
    all_namespaces: bool,

    /// Optional API version used to disambiguate resources, such as `apps/v1`.
    #[serde(default)]
    api_version: Option<String>,

    #[serde(default)]
    label_selector: Option<String>,

    /// Maximum items to return. Defaults to 50 and is capped at 100.
    #[serde(default)]
    limit: Option<u32>,

    /// Opaque continuation token returned by a previous call.
    #[serde(default)]
    continue_token: Option<String>,

    /// `summary` returns compact identity metadata; `full` explicitly returns complete objects.
    #[serde(default)]
    output_mode: OutputMode,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct GenericResourceSummary {
    api_version: String,
    kind: String,
    namespace: Option<String>,
    name: Option<String>,
    created_at: Option<String>,
    generation: Option<i64>,
    deleting: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ListedResource {
    Summary(GenericResourceSummary),
    #[schemars(with = "std::collections::BTreeMap<String, serde_json::Value>")]
    Full(serde_json::Value),
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListResourceOutput {
    output_mode: String,
    resources: Vec<ListedResource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_continue_token: Option<String>,
}

impl ToolBase for ListResource {
    type Parameter = ListResourceArgs;
    type Output = ListResourceOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_resource".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Generic fallback listing compact resource identities; full objects require output_mode=full".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for ListResource {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let client = server.client();
        let (api_resource, scope) = resolve_resource(
            client.clone(),
            &args.resource,
            args.api_version.as_deref(),
            LIST,
        )
        .await?;
        if scope == kube::discovery::Scope::Namespaced {
            super::common::require_explicit_scope(
                "list_resource",
                args.namespace.as_deref(),
                args.all_namespaces,
            )?;
        }
        let api = dynamic_api(client, &api_resource, scope, args.namespace.as_deref());
        let limit = match args.output_mode {
            OutputMode::Summary => args.limit.unwrap_or(50).clamp(1, 100),
            OutputMode::Full => args.limit.unwrap_or(5).clamp(1, 20),
        };
        let mut params = kube::api::ListParams {
            limit: Some(limit),
            continue_token: args.continue_token,
            ..Default::default()
        };
        if let Some(selector) = args.label_selector.as_deref() {
            params = params.labels(selector);
        }
        let page = api.list(&params).await?;
        let next_continue_token = page.metadata.continue_;
        let output_mode = match args.output_mode {
            OutputMode::Summary => "summary",
            OutputMode::Full => "full",
        };
        let resources = page
            .items
            .into_iter()
            .map(|resource| match args.output_mode {
                OutputMode::Summary => Ok(ListedResource::Summary(GenericResourceSummary {
                    api_version: api_resource.api_version.clone(),
                    kind: api_resource.kind.clone(),
                    namespace: resource.metadata.namespace,
                    name: resource.metadata.name,
                    created_at: resource
                        .metadata
                        .creation_timestamp
                        .map(|time| time.0.to_string()),
                    generation: resource.metadata.generation,
                    deleting: resource.metadata.deletion_timestamp.is_some(),
                })),
                OutputMode::Full => serde_json::to_value(resource)
                    .map(ListedResource::Full)
                    .map_err(AppError::from),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListResourceOutput {
            output_mode: output_mode.to_owned(),
            resources,
            next_continue_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_mode_defaults_to_summary() {
        assert!(matches!(OutputMode::default(), OutputMode::Summary));
    }

    #[test]
    fn summary_has_no_spec_or_status() {
        let value = serde_json::to_value(ListedResource::Summary(GenericResourceSummary {
            api_version: "v1".to_owned(),
            kind: "Service".to_owned(),
            namespace: Some("default".to_owned()),
            name: Some("api".to_owned()),
            created_at: None,
            generation: None,
            deleting: false,
        }))
        .unwrap();
        assert!(value.get("spec").is_none());
        assert!(value.get("status").is_none());
    }
}
