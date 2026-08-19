use std::borrow::Cow;

use k8s_openapi::api::core::v1::Namespace;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::{error::AppError, server::KubernetesServer};

use super::common::{LabelSelectorArgs, paginated_params, read_only_annotations};

pub struct ListNamespaces;

#[derive(Debug, Serialize, JsonSchema)]
pub struct NamespaceSummary {
    name: Option<String>,
    phase: Option<String>,
    created_at: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListNamespacesOutput {
    namespaces: Vec<NamespaceSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_continue_token: Option<String>,
}

impl ToolBase for ListNamespaces {
    type Parameter = LabelSelectorArgs;
    type Output = ListNamespacesOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_namespaces".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List Kubernetes namespaces".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for ListNamespaces {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let namespaces: Api<Namespace> = Api::all(server.client());
        let mut params = paginated_params(args.limit, args.continue_token);
        if let Some(selector) = args.label_selector.as_deref() {
            params = params.labels(selector);
        }
        let page = namespaces.list(&params).await?;
        let next_continue_token = page.metadata.continue_;
        let namespaces = page
            .items
            .into_iter()
            .map(|namespace| NamespaceSummary {
                name: namespace.metadata.name,
                phase: namespace.status.and_then(|status| status.phase),
                created_at: namespace
                    .metadata
                    .creation_timestamp
                    .map(|time| time.0.to_string()),
            })
            .collect();
        Ok(ListNamespacesOutput {
            namespaces,
            next_continue_token,
        })
    }
}
