use std::borrow::Cow;

use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};

use crate::{error::AppError, server::KubernetesServer};

use super::common::{ListNamespacedArgs, paginated_params, read_only_annotations};
use schemars::JsonSchema;
use serde::Serialize;

pub struct ListPods;

#[derive(Debug, Serialize, JsonSchema)]
pub struct PodSummary {
    name: Option<String>,
    namespace: Option<String>,
    phase: Option<String>,
    pod_ip: Option<String>,
    node: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListPodsOutput {
    pods: Vec<PodSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_continue_token: Option<String>,
}

impl ToolBase for ListPods {
    type Parameter = ListNamespacedArgs;
    type Output = ListPodsOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_pods".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List Kubernetes pods in a namespace".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for ListPods {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let pods: Api<Pod> = Api::namespaced(server.client(), &args.namespace);
        let mut params = paginated_params(args.limit, args.continue_token);
        if let Some(selector) = args.label_selector.as_deref() {
            params = params.labels(selector);
        }
        let page = pods.list(&params).await?;
        let next_continue_token = page.metadata.continue_;
        let pods = page
            .items
            .into_iter()
            .map(|pod| PodSummary {
                name: pod.metadata.name,
                namespace: pod.metadata.namespace,
                phase: pod.status.as_ref().and_then(|status| status.phase.clone()),
                pod_ip: pod.status.as_ref().and_then(|status| status.pod_ip.clone()),
                node: pod.spec.and_then(|spec| spec.node_name),
            })
            .collect();
        Ok(ListPodsOutput {
            pods,
            next_continue_token,
        })
    }
}
