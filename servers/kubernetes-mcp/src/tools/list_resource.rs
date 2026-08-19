use std::borrow::Cow;

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

    /// Namespace for namespaced resources.
    #[serde(default)]
    namespace: Option<String>,

    /// Must be true to explicitly list a namespaced resource across all namespaces.
    #[serde(default)]
    all_namespaces: bool,

    /// Optional API version used to disambiguate resources, such as `apps/v1`.
    #[serde(default)]
    api_version: Option<String>,

    #[serde(default)]
    label_selector: Option<String>,

    /// Maximum items to return. Defaults to 50 and is capped at 100.
    #[serde(default)]
    limit: Option<u32>,

    /// Opaque continuation token returned by a previous call.
    #[serde(default)]
    continue_token: Option<String>,
}

impl ToolBase for ListResource {
    type Parameter = ListResourceArgs;
    type Output = ResourceListOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_resource".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List a page of any Kubernetes resource, including custom resources".into())
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
        if scope == kube::discovery::Scope::Namespaced {
            super::common::require_explicit_scope(
                "list_resource",
                args.namespace.as_deref(),
                args.all_namespaces,
            )?;
        }
        let api = dynamic_api(client, &api_resource, scope, args.namespace.as_deref());
        let mut params = super::common::paginated_params(args.limit, args.continue_token);
        if let Some(selector) = args.label_selector.as_deref() {
            params = params.labels(selector);
        }
        let page = api.list(&params).await?;
        let next_continue_token = page.metadata.continue_;
        let resources = page
            .items
            .into_iter()
            .map(to_value)
            .collect::<Result<_, _>>()?;
        Ok(ResourceListOutput {
            resources,
            next_continue_token,
        })
    }
}
