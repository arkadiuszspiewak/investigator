use std::borrow::Cow;

use kube::api::ListParams;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::to_value;

use crate::{error::AppError, server::KubernetesServer};

use super::common::{
    LIST, ResourceListOutput, dynamic_api, read_only_annotations, resolve_resource,
};

pub struct ListResource;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListResourceArgs {
    /// Resource plural or Kind, such as `services` or `Service`.
    resource: String,

    /// Namespace for namespaced resources. Omit to list across all namespaces.
    #[serde(default)]
    namespace: Option<String>,

    /// Optional API version used to disambiguate resources, such as `apps/v1`.
    #[serde(default)]
    api_version: Option<String>,

    #[serde(default)]
    label_selector: Option<String>,
}

impl ToolBase for ListResource {
    type Parameter = ListResourceArgs;
    type Output = ResourceListOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_resource".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List any Kubernetes resource, including custom resources".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for ListResource {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let client = server.client();
        let (api_resource, scope) = resolve_resource(
            client.clone(),
            &args.resource,
            args.api_version.as_deref(),
            LIST,
        )
        .await?;
        let api = dynamic_api(client, &api_resource, scope, args.namespace.as_deref());
        let mut params = ListParams::default();
        if let Some(selector) = args.label_selector.as_deref() {
            params = params.labels(selector);
        }
        let resources = api
            .list(&params)
            .await?
            .items
            .into_iter()
            .map(to_value)
            .collect::<Result<_, _>>()?;
        Ok(ResourceListOutput { resources })
    }
}
