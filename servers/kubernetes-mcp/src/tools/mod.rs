mod common;
mod get_crd;
mod get_deployment;
mod get_pod;
mod get_pod_logs;
mod get_resource;
mod get_resource_usage;
mod get_statefulset;
mod list_events;
mod list_namespaces;
mod list_pods;
mod list_resource;

use rmcp::handler::server::router::tool::ToolRouter;

use crate::server::KubernetesServer;
use get_crd::GetCrd;
use get_deployment::GetDeployment;
use get_pod::GetPod;
use get_pod_logs::GetPodLogs;
use get_resource::GetResource;
use get_resource_usage::GetResourceUsage;
use get_statefulset::GetStatefulSet;
use list_events::ListEvents;
use list_namespaces::ListNamespaces;
use list_pods::ListPods;
use list_resource::ListResource;

pub fn router() -> ToolRouter<KubernetesServer> {
    ToolRouter::new()
        .with_async_tool::<ListNamespaces>()
        .with_async_tool::<ListPods>()
        .with_async_tool::<GetPod>()
        .with_async_tool::<GetPodLogs>()
        .with_async_tool::<ListEvents>()
        .with_async_tool::<GetDeployment>()
        .with_async_tool::<GetStatefulSet>()
        .with_async_tool::<GetResource>()
        .with_async_tool::<ListResource>()
        .with_async_tool::<GetCrd>()
        .with_async_tool::<GetResourceUsage>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_all_read_only_tools() {
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
                "get_crd",
                "get_deployment",
                "get_pod",
                "get_pod_logs",
                "get_resource",
                "get_resource_usage",
                "get_statefulset",
                "list_events",
                "list_namespaces",
                "list_pods",
                "list_resource",
            ]
        );
    }
}
