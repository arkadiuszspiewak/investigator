use std::borrow::Cow;

use k8s_openapi::api::apps::v1::Deployment;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};

use crate::{error::AppError, server::KubernetesServer};

use super::common::{GetNamespacedArgs, ResourceOutput, read_only_annotations, to_resource};

pub struct GetDeployment;

impl ToolBase for GetDeployment {
    type Parameter = GetNamespacedArgs;
    type Output = ResourceOutput;
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
        to_resource(deployments.get(&args.name).await?)
    }
}
