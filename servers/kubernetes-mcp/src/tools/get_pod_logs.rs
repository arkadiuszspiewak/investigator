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
}

impl Default for GetPodLogsArgs {
    fn default() -> Self {
        Self {
            name: String::new(),
            namespace: default_namespace(),
            container: None,
            previous: false,
            tail_lines: None,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PodLogsOutput {
    logs: String,
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
        let pods: Api<Pod> = Api::namespaced(server.client(), &args.namespace);
        let logs = pods
            .logs(
                &args.name,
                &LogParams {
                    container: args.container,
                    previous: args.previous,
                    tail_lines: args.tail_lines,
                    ..LogParams::default()
                },
            )
            .await?;
        Ok(PodLogsOutput { logs })
    }
}
