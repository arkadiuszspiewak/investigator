use std::borrow::Cow;

use kube::{
    Api,
    api::DynamicObject,
    core::{ApiResource, GroupVersionKind},
};
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::to_value;

use crate::{error::AppError, server::KubernetesServer};

use super::common::read_only_annotations;

pub struct GetResourceUsage;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum UsageResource {
    #[default]
    Pods,
    Nodes,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GetResourceUsageArgs {
    /// Metrics resource to query: `pods` or `nodes`.
    #[serde(default)]
    resource: UsageResource,

    /// Namespace for pod metrics.
    #[serde(default)]
    namespace: Option<String>,

    /// Must be true to explicitly query pod metrics across all namespaces.
    #[serde(default)]
    all_namespaces: bool,

    #[serde(default)]
    label_selector: Option<String>,

    /// Maximum items to return. Defaults to 50 and is capped at 100.
    #[serde(default)]
    limit: Option<u32>,

    /// Opaque continuation token returned by a previous call.
    #[serde(default)]
    continue_token: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResourceUsageOutput {
    resource: String,
    metrics: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_continue_token: Option<String>,
}

impl ToolBase for GetResourceUsage {
    type Parameter = GetResourceUsageArgs;
    type Output = ResourceUsageOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "get_resource_usage".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Get pod or node CPU and memory usage from metrics.k8s.io".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for GetResourceUsage {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let (kind, plural) = match args.resource {
            UsageResource::Pods => ("PodMetrics", "pods"),
            UsageResource::Nodes => ("NodeMetrics", "nodes"),
        };
        if matches!(args.resource, UsageResource::Pods) {
            super::common::require_explicit_scope(
                "get_resource_usage",
                args.namespace.as_deref(),
                args.all_namespaces,
            )?;
        }
        let gvk = GroupVersionKind::gvk("metrics.k8s.io", "v1beta1", kind);
        let api_resource = ApiResource::from_gvk_with_plural(&gvk, plural);
        let metrics: Api<DynamicObject> = match (&args.resource, args.namespace.as_deref()) {
            (UsageResource::Pods, Some(namespace)) => {
                Api::namespaced_with(server.client(), namespace, &api_resource)
            }
            _ => Api::all_with(server.client(), &api_resource),
        };
        let mut params = super::common::paginated_params(args.limit, args.continue_token);
        if let Some(selector) = args.label_selector.as_deref() {
            params = params.labels(selector);
        }
        let page = metrics.list(&params).await?;
        let next_continue_token = page.metadata.continue_;
        let metrics = page
            .items
            .into_iter()
            .map(to_value)
            .collect::<Result<_, _>>()?;
        Ok(ResourceUsageOutput {
            resource: plural.to_owned(),
            metrics,
            next_continue_token,
        })
    }
}
