use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

    /// MCP Streamable HTTP endpoints exposed to this investigation.
    #[serde(default)]
    pub mcp_servers: Vec<McpServer>,

    /// ServiceAccount used by the agent Job. Its RBAC defines the blast radius.
    #[serde(default = "default_service_account")]
    pub service_account_name: String,
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
    "ghcr.io/example/investigator-agent:latest".to_owned()
}

fn default_service_account() -> String {
    "investigator-agent".to_owned()
}
