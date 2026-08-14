use std::borrow::Cow;

use k8s_openapi::api::core::v1::Namespace;
use kube::{Api, api::ListParams};
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use serde_json::to_value;

use crate::{error::AppError, server::KubernetesServer};

use super::common::{LabelSelectorArgs, ResourceListOutput, read_only_annotations};

pub struct ListNamespaces;

impl ToolBase for ListNamespaces {
    type Parameter = LabelSelectorArgs;
    type Output = ResourceListOutput;
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
        let mut params = ListParams::default();
        if let Some(selector) = args.label_selector.as_deref() {
            params = params.labels(selector);
        }
        let resources = namespaces
            .list(&params)
            .await?
            .items
            .into_iter()
            .map(to_value)
            .collect::<Result<_, _>>()?;
        Ok(ResourceListOutput { resources })
    }
}
