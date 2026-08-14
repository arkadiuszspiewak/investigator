use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(CustomResource, Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[kube(
    group = "investigator.openai.com",
    version = "v1alpha1",
    kind = "Investigation",
    plural = "investigations",
    namespaced,
    status = "InvestigationStatus",
    shortname = "inv"
)]
#[serde(rename_all = "camelCase")]
pub struct InvestigationSpec {
    /// The task given to the Codex agent.
    pub query: String,

    /// Container image containing Codex. The controller never runs the LLM itself.
    #[serde(default = "default_agent_image")]
    pub agent_image: String,

    /// Credentials used by Codex. Exactly one credential source must be set.
    pub auth: AgentAuth,

    /// MCP Streamable HTTP endpoints exposed to this investigation.
    #[serde(default)]
    pub mcp_servers: Vec<McpServer>,

    /// ServiceAccount used by the agent Job. Its RBAC defines the blast radius.
    #[serde(default = "default_service_account")]
    pub service_account_name: String,

    /// Labels that the agent Job's node must have.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub node_selector: BTreeMap<String, String>,

    /// Kubernetes affinity rules applied to the agent Job pod.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affinity: Option<serde_json::Value>,

    /// Kubernetes taint tolerations applied to the agent Job pod.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tolerations: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuth {
    /// A Secret key containing an OpenAI API key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_secret_ref: Option<SecretKeyRef>,

    /// A Secret key containing a Codex auth.json file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_json_secret_ref: Option<SecretKeyRef>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretKeyRef {
    pub name: String,
    pub key: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestigationStatus {
    pub phase: Option<String>,
    pub job_name: Option<String>,
    pub message: Option<String>,
    pub observed_generation: Option<i64>,
}

fn default_agent_image() -> String {
    "ghcr.io/arkadiuszspiewak/investigator-agent:latest".to_owned()
}

fn default_service_account() -> String {
    "investigator-agent".to_owned()
}
