use std::borrow::Cow;

use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{error::AppError, server::KubernetesServer};

use super::common::{ResourceOutput, read_only_annotations, to_resource};

pub struct GetCrd;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GetCrdArgs {
    /// Full CRD name, such as `certificates.cert-manager.io`.
    name: String,
}

impl ToolBase for GetCrd {
    type Parameter = GetCrdArgs;
    type Output = ResourceOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "get_crd".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Get a Kubernetes CustomResourceDefinition".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for GetCrd {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let crds: Api<CustomResourceDefinition> = Api::all(server.client());
        to_resource(crds.get(&args.name).await?)
    }
}
