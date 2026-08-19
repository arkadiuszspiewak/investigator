use std::borrow::Cow;

use k8s_openapi::api::core::v1::{ContainerState, Pod};
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
    ready: bool,
    restart_count: i32,
    containers: Vec<ContainerSummary>,
    conditions: Vec<PodConditionSummary>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct ContainerSummary {
    name: String,
    image: String,
    ready: bool,
    restart_count: i32,
    state: String,
    reason: Option<String>,
    message: Option<String>,
    last_termination_reason: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct PodConditionSummary {
    condition: String,
    status: String,
    reason: Option<String>,
    message: Option<String>,
}

impl From<Pod> for PodSummary {
    fn from(pod: Pod) -> Self {
        let status = pod.status.as_ref();
        let containers = status
            .and_then(|status| status.container_statuses.as_ref())
            .into_iter()
            .flatten()
            .map(|container| {
                let (state, reason, message) = container_state(container.state.as_ref());
                ContainerSummary {
                    name: container.name.clone(),
                    image: container.image.clone(),
                    ready: container.ready,
                    restart_count: container.restart_count,
                    state,
                    reason,
                    message,
                    last_termination_reason: container
                        .last_state
                        .as_ref()
                        .and_then(|state| state.terminated.as_ref())
                        .and_then(|state| state.reason.clone()),
                }
            })
            .collect::<Vec<_>>();
        let conditions = status
            .and_then(|status| status.conditions.as_ref())
            .into_iter()
            .flatten()
            .map(|condition| PodConditionSummary {
                condition: condition.type_.clone(),
                status: condition.status.clone(),
                reason: condition.reason.clone(),
                message: condition.message.clone(),
            })
            .collect();
        Self {
            name: pod.metadata.name,
            namespace: pod.metadata.namespace,
            phase: status.and_then(|status| status.phase.clone()),
            pod_ip: status.and_then(|status| status.pod_ip.clone()),
            node: pod.spec.and_then(|spec| spec.node_name),
            ready: !containers.is_empty() && containers.iter().all(|container| container.ready),
            restart_count: containers
                .iter()
                .map(|container| container.restart_count)
                .sum(),
            containers,
            conditions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_omits_raw_pod_spec_and_metadata() {
        let summary = serde_json::to_value(PodSummary::from(Pod::default())).unwrap();
        assert!(summary.get("spec").is_none());
        assert!(summary.get("status").is_none());
        assert!(summary.get("metadata").is_none());
        assert_eq!(summary["ready"], false);
    }
}

fn container_state(state: Option<&ContainerState>) -> (String, Option<String>, Option<String>) {
    if let Some(waiting) = state.and_then(|state| state.waiting.as_ref()) {
        return (
            "waiting".to_owned(),
            waiting.reason.clone(),
            waiting.message.clone(),
        );
    }
    if let Some(terminated) = state.and_then(|state| state.terminated.as_ref()) {
        return (
            "terminated".to_owned(),
            terminated.reason.clone(),
            terminated.message.clone(),
        );
    }
    if state.and_then(|state| state.running.as_ref()).is_some() {
        return ("running".to_owned(), None, None);
    }
    ("unknown".to_owned(), None, None)
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
        Some("List compact Pod summaries in a namespace; prefer over list_resource for Pods".into())
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
        let pods = page.items.into_iter().map(PodSummary::from).collect();
        Ok(ListPodsOutput {
            pods,
            next_continue_token,
        })
    }
}
