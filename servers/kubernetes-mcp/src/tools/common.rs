use kube::{
    Api, Client,
    api::{DynamicObject, ListParams},
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

    /// Maximum items to return. Defaults to 50 and is capped at 100.
    #[serde(default)]
    pub limit: Option<u32>,

    /// Opaque continuation token returned by a previous call.
    #[serde(default)]
    pub continue_token: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListNamespacedArgs {
    /// Kubernetes namespace to query.
    #[serde(default = "default_namespace")]
    pub namespace: String,

    /// Optional Kubernetes label selector, such as `app=nginx`.
    #[serde(default)]
    pub label_selector: Option<String>,

    /// Maximum items to return. Defaults to 50 and is capped at 100.
    #[serde(default)]
    pub limit: Option<u32>,

    /// Opaque continuation token returned by a previous call.
    #[serde(default)]
    pub continue_token: Option<String>,
}

impl Default for ListNamespacedArgs {
    fn default() -> Self {
        Self {
            namespace: default_namespace(),
            label_selector: None,
            limit: None,
            continue_token: None,
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
    #[schemars(with = "std::collections::BTreeMap<String, serde_json::Value>")]
    pub resource: Value,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResourceListOutput {
    #[schemars(with = "Vec<std::collections::BTreeMap<String, serde_json::Value>>")]
    pub resources: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continue_token: Option<String>,
}

pub fn paginated_params(limit: Option<u32>, continue_token: Option<String>) -> ListParams {
    ListParams {
        limit: Some(limit.unwrap_or(50).clamp(1, 100)),
        continue_token,
        ..Default::default()
    }
}

pub fn require_explicit_scope(
    tool: &'static str,
    namespace: Option<&str>,
    all_namespaces: bool,
) -> Result<(), AppError> {
    match (namespace, all_namespaces) {
        (None, false) => Err(AppError::ExplicitAllNamespacesRequired { tool }),
        (Some(_), true) => Err(AppError::AmbiguousNamespaceScope { tool }),
        _ => Ok(()),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_kubernetes_resources_have_object_output_schemas() {
        let resource = serde_json::to_value(schemars::schema_for!(ResourceOutput)).unwrap();
        let list = serde_json::to_value(schemars::schema_for!(ResourceListOutput)).unwrap();

        assert_eq!(resource["properties"]["resource"]["type"], "object");
        assert_eq!(list["properties"]["resources"]["items"]["type"], "object");
    }

    #[test]
    fn pagination_has_bounded_defaults() {
        assert_eq!(paginated_params(None, None).limit, Some(50));
        assert_eq!(paginated_params(Some(0), None).limit, Some(1));
        assert_eq!(paginated_params(Some(500), None).limit, Some(100));
        assert_eq!(
            paginated_params(Some(25), Some("next".to_owned())).continue_token,
            Some("next".to_owned())
        );
    }

    #[test]
    fn cluster_wide_namespace_scope_must_be_explicit() {
        assert!(require_explicit_scope("test", Some("default"), false).is_ok());
        assert!(require_explicit_scope("test", None, true).is_ok());
        assert!(require_explicit_scope("test", None, false).is_err());
        assert!(require_explicit_scope("test", Some("default"), true).is_err());
    }
}

pub fn read_only_annotations() -> Option<ToolAnnotations> {
    Some(ToolAnnotations::new().read_only(true).idempotent(true))
}
