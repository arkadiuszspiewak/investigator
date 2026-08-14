use std::borrow::Cow;

use k8s_openapi::api::core::v1::Event;
use kube::{Api, api::ListParams};
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
    /// Namespace to query. Omit to list events across all namespaces.
    #[serde(default)]
    namespace: Option<String>,

    #[serde(default)]
    field_selector: Option<String>,
}

impl ToolBase for ListEvents {
    type Parameter = ListEventsArgs;
    type Output = ResourceListOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_events".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List Kubernetes events, optionally within one namespace".into())
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
        let events: Api<Event> = match args.namespace.as_deref() {
            Some(namespace) => Api::namespaced(server.client(), namespace),
            None => Api::all(server.client()),
        };
        let mut params = ListParams::default();
        if let Some(selector) = args.field_selector.as_deref() {
            params = params.fields(selector);
        }
        let resources = events
            .list(&params)
            .await?
            .items
            .into_iter()
            .map(to_value)
            .collect::<Result<_, _>>()?;
        Ok(ResourceListOutput { resources })
    }
}
