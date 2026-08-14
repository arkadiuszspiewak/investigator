use std::borrow::Cow;

use k8s_openapi::api::apps::v1::StatefulSet;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};

use crate::{error::AppError, server::KubernetesServer};

use super::common::{GetNamespacedArgs, ResourceOutput, read_only_annotations, to_resource};

pub struct GetStatefulSet;

impl ToolBase for GetStatefulSet {
    type Parameter = GetNamespacedArgs;
    type Output = ResourceOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "get_statefulset".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Get one Kubernetes StatefulSet by name".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for GetStatefulSet {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let stateful_sets: Api<StatefulSet> = Api::namespaced(server.client(), &args.namespace);
        to_resource(stateful_sets.get(&args.name).await?)
    }
}
