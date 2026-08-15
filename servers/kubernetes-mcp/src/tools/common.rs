use kube::{
    Api, Client,
    api::DynamicObject,
    discovery::{ApiResource, Discovery, Scope, verbs},
};
use rmcp::model::ToolAnnotations;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LabelSelectorArgs {
    /// Optional Kubernetes label selector, such as `app=nginx`.
    #[serde(default)]
    pub label_selector: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListNamespacedArgs {
    /// Kubernetes namespace to query.
    #[serde(default = "default_namespace")]
    pub namespace: String,

    /// Optional Kubernetes label selector, such as `app=nginx`.
    #[serde(default)]
    pub label_selector: Option<String>,
}

impl Default for ListNamespacedArgs {
    fn default() -> Self {
        Self {
            namespace: default_namespace(),
            label_selector: None,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetNamespacedArgs {
    /// Resource name.
    pub name: String,

    /// Kubernetes namespace containing the resource.
    #[serde(default = "default_namespace")]
    pub namespace: String,
}

impl Default for GetNamespacedArgs {
    fn default() -> Self {
        Self {
            name: String::new(),
            namespace: default_namespace(),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResourceOutput {
    pub resource: Value,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResourceListOutput {
    pub resources: Vec<Value>,
}

pub fn default_namespace() -> String {
    "default".to_owned()
}

pub fn to_resource<T: Serialize>(resource: T) -> Result<ResourceOutput, AppError> {
    Ok(ResourceOutput {
        resource: serde_json::to_value(resource)?,
    })
}

pub async fn resolve_resource(
    client: Client,
    resource: &str,
    api_version: Option<&str>,
    operation: &str,
) -> Result<(ApiResource, Scope), AppError> {
    let discovery = Discovery::new(client).run().await?;
    for group in discovery.groups() {
        for (api_resource, capabilities) in group.recommended_resources() {
            let name_matches = api_resource.plural.eq_ignore_ascii_case(resource)
                || api_resource.kind.eq_ignore_ascii_case(resource);
            let version_matches = api_version
                .map(|version| api_resource.api_version == version)
                .unwrap_or(true);
            if name_matches && version_matches && capabilities.supports_operation(operation) {
                return Ok((api_resource, capabilities.scope));
            }
        }
    }

    Err(AppError::ResourceNotFound {
        resource: resource.to_owned(),
        api_version: api_version.map(str::to_owned),
        operation: operation.to_owned(),
    })
}

pub fn dynamic_api(
    client: Client,
    api_resource: &ApiResource,
    scope: Scope,
    namespace: Option<&str>,
) -> Api<DynamicObject> {
    match (scope, namespace) {
        (Scope::Namespaced, Some(namespace)) => {
            Api::namespaced_with(client, namespace, api_resource)
        }
        _ => Api::all_with(client, api_resource),
    }
}

pub const GET: &str = verbs::GET;
pub const LIST: &str = verbs::LIST;

pub fn read_only_annotations() -> Option<ToolAnnotations> {
    Some(ToolAnnotations::new().read_only(true).idempotent(true))
}
