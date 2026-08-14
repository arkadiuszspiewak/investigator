use std::borrow::Cow;

use kube::{
    Api,
    api::{DynamicObject, ListParams},
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

    /// Namespace for pod metrics. Omit to query all namespaces.
    #[serde(default)]
    namespace: Option<String>,

    #[serde(default)]
    label_selector: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResourceUsageOutput {
    resource: String,
    metrics: Vec<serde_json::Value>,
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
        let gvk = GroupVersionKind::gvk("metrics.k8s.io", "v1beta1", kind);
        let api_resource = ApiResource::from_gvk_with_plural(&gvk, plural);
        let metrics: Api<DynamicObject> = match (&args.resource, args.namespace.as_deref()) {
            (UsageResource::Pods, Some(namespace)) => {
                Api::namespaced_with(server.client(), namespace, &api_resource)
            }
            _ => Api::all_with(server.client(), &api_resource),
        };
        let mut params = ListParams::default();
        if let Some(selector) = args.label_selector.as_deref() {
            params = params.labels(selector);
        }
        let metrics = metrics
            .list(&params)
            .await?
            .items
            .into_iter()
            .map(to_value)
            .collect::<Result<_, _>>()?;
        Ok(ResourceUsageOutput {
            resource: plural.to_owned(),
            metrics,
        })
    }
}
