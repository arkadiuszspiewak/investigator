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

    /// Follow-up questions, appended by interactive clients after the initial result.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<InvestigationQuestion>,

    /// Credentials used by Codex. Exactly one credential source must be set.
    pub auth: AgentAuth,

    /// ServiceAccount used by the agent Job. Its RBAC defines the blast radius.
    #[serde(default = "default_service_account")]
    pub service_account_name: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestigationQuestion {
    /// Client-generated stable identifier used to correlate the answer.
    pub id: String,
    pub query: String,
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

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestigationStatus {
    pub phase: Option<String>,
    pub job_name: Option<String>,
    pub message: Option<String>,
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<InvestigationAnswer>,
    pub observed_generation: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestigationAnswer {
    pub question_id: String,
    pub result: String,
}

fn default_service_account() -> String {
    "investigator-agent".to_owned()
}
