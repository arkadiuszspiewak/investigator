use std::borrow::Cow;

use k8s_openapi::api::apps::v1::Deployment;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::{error::AppError, server::KubernetesServer};

use super::common::{GetNamespacedArgs, read_only_annotations};

pub struct GetDeployment;

#[derive(Debug, Serialize, JsonSchema)]
pub struct DeploymentSummary {
    name: Option<String>,
    namespace: Option<String>,
    generation: Option<i64>,
    observed_generation: Option<i64>,
    desired_replicas: i32,
    replicas: i32,
    ready_replicas: i32,
    available_replicas: i32,
    updated_replicas: i32,
    unavailable_replicas: i32,
    conditions: Vec<WorkloadConditionSummary>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct WorkloadConditionSummary {
    condition: String,
    status: String,
    reason: Option<String>,
    message: Option<String>,
}

impl From<Deployment> for DeploymentSummary {
    fn from(deployment: Deployment) -> Self {
        let status = deployment.status.as_ref();
        Self {
            name: deployment.metadata.name,
            namespace: deployment.metadata.namespace,
            generation: deployment.metadata.generation,
            observed_generation: status.and_then(|status| status.observed_generation),
            desired_replicas: deployment.spec.and_then(|spec| spec.replicas).unwrap_or(1),
            replicas: status.and_then(|status| status.replicas).unwrap_or(0),
            ready_replicas: status.and_then(|status| status.ready_replicas).unwrap_or(0),
            available_replicas: status
                .and_then(|status| status.available_replicas)
                .unwrap_or(0),
            updated_replicas: status
                .and_then(|status| status.updated_replicas)
                .unwrap_or(0),
            unavailable_replicas: status
                .and_then(|status| status.unavailable_replicas)
                .unwrap_or(0),
            conditions: status
                .and_then(|status| status.conditions.as_ref())
                .into_iter()
                .flatten()
                .map(|condition| WorkloadConditionSummary {
                    condition: condition.type_.clone(),
                    status: condition.status.clone(),
                    reason: condition.reason.clone(),
                    message: condition.message.clone(),
                })
                .collect(),
        }
    }
}

impl ToolBase for GetDeployment {
    type Parameter = GetNamespacedArgs;
    type Output = DeploymentSummary;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "get_deployment".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Get one Kubernetes deployment by name".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for GetDeployment {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let deployments: Api<Deployment> = Api::namespaced(server.client(), &args.namespace);
        Ok(DeploymentSummary::from(deployments.get(&args.name).await?))
    }
}
