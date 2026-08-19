use std::borrow::Cow;

use k8s_openapi::api::apps::v1::StatefulSet;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::{error::AppError, server::KubernetesServer};

use super::common::{GetNamespacedArgs, read_only_annotations};

pub struct GetStatefulSet;

#[derive(Debug, Serialize, JsonSchema)]
pub struct StatefulSetSummary {
    name: Option<String>,
    namespace: Option<String>,
    generation: Option<i64>,
    observed_generation: Option<i64>,
    desired_replicas: i32,
    replicas: i32,
    ready_replicas: i32,
    available_replicas: i32,
    current_replicas: i32,
    updated_replicas: i32,
    current_revision: Option<String>,
    update_revision: Option<String>,
    conditions: Vec<StatefulSetConditionSummary>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct StatefulSetConditionSummary {
    condition: String,
    status: String,
    reason: Option<String>,
    message: Option<String>,
}

impl From<StatefulSet> for StatefulSetSummary {
    fn from(stateful_set: StatefulSet) -> Self {
        let status = stateful_set.status.as_ref();
        Self {
            name: stateful_set.metadata.name,
            namespace: stateful_set.metadata.namespace,
            generation: stateful_set.metadata.generation,
            observed_generation: status.and_then(|status| status.observed_generation),
            desired_replicas: stateful_set
                .spec
                .and_then(|spec| spec.replicas)
                .unwrap_or(1),
            replicas: status.map(|status| status.replicas).unwrap_or(0),
            ready_replicas: status.and_then(|status| status.ready_replicas).unwrap_or(0),
            available_replicas: status
                .and_then(|status| status.available_replicas)
                .unwrap_or(0),
            current_replicas: status
                .and_then(|status| status.current_replicas)
                .unwrap_or(0),
            updated_replicas: status
                .and_then(|status| status.updated_replicas)
                .unwrap_or(0),
            current_revision: status.and_then(|status| status.current_revision.clone()),
            update_revision: status.and_then(|status| status.update_revision.clone()),
            conditions: status
                .and_then(|status| status.conditions.as_ref())
                .into_iter()
                .flatten()
                .map(|condition| StatefulSetConditionSummary {
                    condition: condition.type_.clone(),
                    status: condition.status.clone(),
                    reason: condition.reason.clone(),
                    message: condition.message.clone(),
                })
                .collect(),
        }
    }
}

impl ToolBase for GetStatefulSet {
    type Parameter = GetNamespacedArgs;
    type Output = StatefulSetSummary;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "get_statefulset".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Get a compact StatefulSet health summary; prefer over get_resource".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for GetStatefulSet {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let stateful_sets: Api<StatefulSet> = Api::namespaced(server.client(), &args.namespace);
        Ok(StatefulSetSummary::from(
            stateful_sets.get(&args.name).await?,
        ))
    }
}
