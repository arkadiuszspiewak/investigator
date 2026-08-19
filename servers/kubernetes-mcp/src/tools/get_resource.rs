use std::borrow::Cow;

use kube::discovery::Scope;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{error::AppError, server::KubernetesServer};

use super::common::{
    GET, ResourceOutput, dynamic_api, read_only_annotations, resolve_resource, to_resource,
};

pub struct GetResource;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GetResourceArgs {
    /// Resource plural or Kind, such as `services` or `Service`.
    resource: String,

    /// Resource name.
    name: String,

    /// Namespace for namespaced resources. Defaults to `default`.
    #[serde(default)]
    namespace: Option<String>,

    /// Optional API version used to disambiguate resources, such as `apps/v1`.
    #[serde(default)]
    api_version: Option<String>,
}

impl ToolBase for GetResource {
    type Parameter = GetResourceArgs;
    type Output = ResourceOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "get_resource".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Get one complete Kubernetes object as a fallback when specialized summaries lack required evidence".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for GetResource {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let client = server.client();
        let (api_resource, scope) = resolve_resource(
            client.clone(),
            &args.resource,
            args.api_version.as_deref(),
            GET,
        )
        .await?;
        let default_namespace = "default".to_owned();
        let namespace = match scope {
            Scope::Namespaced => Some(
                args.namespace
                    .as_ref()
                    .unwrap_or(&default_namespace)
                    .as_str(),
            ),
            Scope::Cluster => None,
        };
        let api = dynamic_api(client, &api_resource, scope, namespace);
        to_resource(api.get(&args.name).await?)
    }
}
