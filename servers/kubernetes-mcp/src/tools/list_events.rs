use std::borrow::Cow;

use k8s_openapi::api::core::v1::Event;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::to_value;

use crate::{error::AppError, server::KubernetesServer};

use super::common::{ResourceListOutput, read_only_annotations};

pub struct ListEvents;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListEventsArgs {
    /// Namespace to query.
    #[serde(default)]
    namespace: Option<String>,

    /// Must be true to explicitly query all namespaces.
    #[serde(default)]
    all_namespaces: bool,

    #[serde(default)]
    field_selector: Option<String>,

    /// Maximum items to return. Defaults to 50 and is capped at 100.
    #[serde(default)]
    limit: Option<u32>,

    /// Opaque continuation token returned by a previous call.
    #[serde(default)]
    continue_token: Option<String>,
}

impl ToolBase for ListEvents {
    type Parameter = ListEventsArgs;
    type Output = ResourceListOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_events".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List a page of Kubernetes events in one namespace or explicitly across all".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for ListEvents {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        super::common::require_explicit_scope(
            "list_events",
            args.namespace.as_deref(),
            args.all_namespaces,
        )?;
        let events: Api<Event> = match args.namespace.as_deref() {
            Some(namespace) => Api::namespaced(server.client(), namespace),
            None => Api::all(server.client()),
        };
        let mut params = super::common::paginated_params(args.limit, args.continue_token);
        if let Some(selector) = args.field_selector.as_deref() {
            params = params.fields(selector);
        }
        let page = events.list(&params).await?;
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
