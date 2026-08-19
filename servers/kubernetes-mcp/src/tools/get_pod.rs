use std::borrow::Cow;

use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};

use crate::{error::AppError, server::KubernetesServer};

use super::{
    common::{GetNamespacedArgs, read_only_annotations},
    list_pods::PodSummary,
};

pub struct GetPod;

impl ToolBase for GetPod {
    type Parameter = GetNamespacedArgs;
    type Output = PodSummary;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "get_pod".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Get one Kubernetes pod by name".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for GetPod {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let pods: Api<Pod> = Api::namespaced(server.client(), &args.namespace);
        Ok(PodSummary::from(pods.get(&args.name).await?))
    }
}
