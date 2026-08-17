use std::{collections::BTreeMap, sync::Arc, time::Duration};

use futures::StreamExt;
use k8s_openapi::api::{
    batch::v1::Job,
    core::v1::{
        Affinity, Container, EmptyDirVolumeSource, EnvVar, EnvVarSource, KeyToPath, Pod, PodSpec,
        PodTemplateSpec, SecretKeySelector, SecretVolumeSource, Toleration, Volume, VolumeMount,
    },
};
use kube::{
    Api, Client, Resource, ResourceExt,
    api::{DeleteParams, ListParams, ObjectMeta, Patch, PatchParams, PostParams},
    runtime::{
        controller::{Action, Controller},
        watcher,
    },
};
use serde_json::json;

use crate::error::ReconcileError;
use investigator::crd::{Investigation, InvestigationAnswer, InvestigationStatus};

#[derive(Clone, Debug, PartialEq)]
enum AgentProvider {
    OpenAi,
    Bedrock { region: String },
}

#[derive(Clone, Debug, PartialEq)]
enum AgentAuth {
    ApiKey {
        secret_name: String,
        secret_key: String,
    },
    AuthJson {
        secret_name: String,
        secret_key: String,
    },
    WorkloadIdentity,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct McpServer {
    name: String,
    url: String,
}

#[derive(Clone)]
struct Context {
    client: Client,
    agent_job: AgentJobConfig,
}

#[derive(Clone, Debug)]
pub struct AgentJobConfig {
    image: String,
    service_account_name: String,
    model: String,
    provider: AgentProvider,
    auth: AgentAuth,
    mcp_servers: Vec<McpServer>,
    node_selector: BTreeMap<String, String>,
    affinity: Option<Affinity>,
    tolerations: Vec<Toleration>,
}

impl AgentJobConfig {
    pub fn from_env() -> Result<Self, String> {
        let provider_name =
            std::env::var("INVESTIGATOR_AGENT_PROVIDER").unwrap_or_else(|_| "openai".to_owned());
        let provider = match provider_name.as_str() {
            "openai" => AgentProvider::OpenAi,
            "bedrock" => AgentProvider::Bedrock {
                region: required_env("INVESTIGATOR_AGENT_REGION")?,
            },
            _ => return Err(format!("unsupported agent provider: {provider_name}")),
        };
        let auth_name =
            std::env::var("INVESTIGATOR_AGENT_AUTH_TYPE").unwrap_or_else(|_| "apiKey".to_owned());
        let auth = match auth_name.as_str() {
            "apiKey" => AgentAuth::ApiKey {
                secret_name: required_env("INVESTIGATOR_AGENT_API_KEY_SECRET_NAME")?,
                secret_key: required_env("INVESTIGATOR_AGENT_API_KEY_SECRET_KEY")?,
            },
            "authJson" => AgentAuth::AuthJson {
                secret_name: required_env("INVESTIGATOR_AGENT_AUTH_JSON_SECRET_NAME")?,
                secret_key: required_env("INVESTIGATOR_AGENT_AUTH_JSON_SECRET_KEY")?,
            },
            "workloadIdentity" => AgentAuth::WorkloadIdentity,
            _ => return Err(format!("unsupported agent auth type: {auth_name}")),
        };
        match (&provider, &auth) {
            (AgentProvider::OpenAi, AgentAuth::WorkloadIdentity)
            | (AgentProvider::Bedrock { .. }, AgentAuth::AuthJson { .. }) => {
                return Err(format!(
                    "invalid provider/auth combination: {provider_name}/{auth_name}"
                ));
            }
            _ => {}
        }
        Ok(Self {
            image: std::env::var("INVESTIGATOR_AGENT_IMAGE").unwrap_or_else(|_| {
                "ghcr.io/arkadiuszspiewak/investigator-agent:latest".to_owned()
            }),
            service_account_name: std::env::var("INVESTIGATOR_AGENT_SERVICE_ACCOUNT")
                .unwrap_or_else(|_| "investigator-agent".to_owned()),
            model: required_env("INVESTIGATOR_AGENT_MODEL")?,
            provider,
            auth,
            mcp_servers: serde_json::from_str(
                &std::env::var("INVESTIGATOR_MCP_SERVERS").unwrap_or_else(|_| "[]".to_owned()),
            )
            .map_err(|error| error.to_string())?,
            node_selector: serde_json::from_str(
                &std::env::var("INVESTIGATOR_AGENT_JOB_NODE_SELECTOR")
                    .unwrap_or_else(|_| "{}".to_owned()),
            )
            .map_err(|error| error.to_string())?,
            affinity: serde_json::from_str(
                &std::env::var("INVESTIGATOR_AGENT_JOB_AFFINITY")
                    .unwrap_or_else(|_| "null".to_owned()),
            )
            .map_err(|error| error.to_string())?,
            tolerations: serde_json::from_str(
                &std::env::var("INVESTIGATOR_AGENT_JOB_TOLERATIONS")
                    .unwrap_or_else(|_| "[]".to_owned()),
            )
            .map_err(|error| error.to_string())?,
        })
    }
}

impl Default for AgentJobConfig {
    fn default() -> Self {
        Self {
            image: "ghcr.io/arkadiuszspiewak/investigator-agent:latest".to_owned(),
            service_account_name: "investigator-agent".to_owned(),
            model: "gpt-5.6-terra".to_owned(),
            provider: AgentProvider::OpenAi,
            auth: AgentAuth::ApiKey {
                secret_name: "openai-api-key".to_owned(),
                secret_key: "api-key".to_owned(),
            },
            mcp_servers: vec![],
            node_selector: BTreeMap::new(),
            affinity: None,
            tolerations: vec![],
        }
    }
}

fn required_env(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("missing required environment variable {name}"))
}

pub async fn run(client: Client, agent_job: AgentJobConfig) {
    let investigations = Api::<Investigation>::all(client.clone());
    Controller::new(investigations, watcher::Config::default())
        .owns(Api::<Job>::all(client.clone()), watcher::Config::default())
        .run(
            reconcile,
            error_policy,
            Arc::new(Context { client, agent_job }),
        )
        .for_each(|result| async move {
            match result {
                Ok(object) => tracing::info!(
                    event = "investigation_reconciled",
                    object = ?object,
                ),
                Err(error) => tracing::error!(
                    event = "investigation_reconciliation_failed",
                    error = %error,
                ),
            }
        })
        .await;
}

async fn reconcile(
    investigation: Arc<Investigation>,
    context: Arc<Context>,
) -> Result<Action, ReconcileError> {
    let namespace = investigation
        .namespace()
        .ok_or(ReconcileError::MissingNamespace)?;
    let previous = investigation.status.clone().unwrap_or_default();
    let turn = next_turn(&investigation, &previous);
    if turn.is_none() {
        let status = InvestigationStatus {
            phase: Some("Succeeded".to_owned()),
            observed_generation: investigation.metadata.generation,
            ..previous
        };
        patch_status(&context.client, &namespace, &investigation, status).await?;
        return Ok(Action::requeue(Duration::from_secs(30)));
    }
    let turn = turn.unwrap();
    let job_name = if turn == 0 {
        format!("{}-agent", investigation.name_any())
    } else {
        format!("{}-agent-q{turn}", investigation.name_any())
    };
    let jobs = Api::<Job>::namespaced(context.client.clone(), &namespace);

    let existing_job = jobs.get_opt(&job_name).await?;
    if let Some(job) = existing_job.as_ref()
        && !job_is_owned_by(job, &investigation)
    {
        jobs.delete(&job_name, &DeleteParams::default()).await?;
        return Ok(Action::requeue(Duration::from_secs(1)));
    }

    let phase = match existing_job {
        Some(job) if job.status.as_ref().and_then(|s| s.succeeded).unwrap_or(0) > 0 => "Succeeded",
        Some(job) if job.status.as_ref().and_then(|s| s.failed).unwrap_or(0) > 0 => "Failed",
        Some(_) => "Running",
        None => {
            jobs.create(
                &PostParams::default(),
                &agent_job(
                    &investigation,
                    job_name.clone(),
                    conversation_prompt(&investigation, &previous, turn),
                    &context.agent_job,
                )?,
            )
            .await?;
            "Pending"
        }
    };
    let completed_result = if matches!(phase, "Succeeded" | "Failed") {
        agent_result(&context.client, &namespace, &job_name).await?
    } else {
        None
    };

    let mut status = InvestigationStatus {
        phase: Some(phase.to_owned()),
        job_name: Some(job_name),
        message: None,
        result: previous.result,
        answers: previous.answers,
        observed_generation: investigation.metadata.generation,
    };
    if phase == "Succeeded" {
        if turn == 0 {
            status.result = completed_result;
        } else if let Some(result) = completed_result {
            status.answers.push(InvestigationAnswer {
                question_id: investigation.spec.questions[turn - 1].id.clone(),
                result,
            });
        }
    }
    patch_status(&context.client, &namespace, &investigation, status).await?;

    Ok(Action::requeue(Duration::from_secs(10)))
}

async fn patch_status(
    client: &Client,
    namespace: &str,
    investigation: &Investigation,
    status: InvestigationStatus,
) -> Result<(), ReconcileError> {
    if investigation.status.as_ref() != Some(&status) {
        Api::<Investigation>::namespaced(client.clone(), namespace)
            .patch_status(
                &investigation.name_any(),
                &PatchParams::default(),
                &Patch::Merge(json!({ "status": status })),
            )
            .await?;
    }
    Ok(())
}

fn next_turn(investigation: &Investigation, status: &InvestigationStatus) -> Option<usize> {
    if status.result.is_none() {
        return Some(0);
    }
    investigation
        .spec
        .questions
        .iter()
        .position(|question| !status.answers.iter().any(|a| a.question_id == question.id))
        .map(|index| index + 1)
}

fn conversation_prompt(
    investigation: &Investigation,
    status: &InvestigationStatus,
    turn: usize,
) -> String {
    if turn == 0 {
        return investigation.spec.query.clone();
    }
    let mut prompt = format!(
        "Continue this investigation using the prior conversation as context.\n\nInitial question:\n{}\n\nInitial answer:\n{}",
        investigation.spec.query,
        status.result.as_deref().unwrap_or_default()
    );
    for question in investigation.spec.questions.iter().take(turn - 1) {
        if let Some(answer) = status.answers.iter().find(|a| a.question_id == question.id) {
            prompt.push_str(&format!(
                "\n\nFollow-up question:\n{}\n\nAnswer:\n{}",
                question.query, answer.result
            ));
        }
    }
    prompt.push_str(&format!(
        "\n\nNew follow-up question:\n{}",
        investigation.spec.questions[turn - 1].query
    ));
    prompt
}

async fn agent_result(
    client: &Client,
    namespace: &str,
    job_name: &str,
) -> Result<Option<String>, ReconcileError> {
    let pods = Api::<Pod>::namespaced(client.clone(), namespace);
    let pods = pods
        .list(&ListParams::default().labels(&format!("job-name={job_name}")))
        .await?;
    Ok(pods.items.iter().find_map(pod_result))
}

fn pod_result(pod: &Pod) -> Option<String> {
    pod.status
        .as_ref()?
        .container_statuses
        .as_ref()?
        .iter()
        .find(|status| status.name == "codex")?
        .state
        .as_ref()?
        .terminated
        .as_ref()?
        .message
        .clone()
        .filter(|message| !message.is_empty())
}

fn job_is_owned_by(job: &Job, investigation: &Investigation) -> bool {
    investigation.metadata.uid.as_ref().is_some_and(|uid| {
        job.metadata
            .owner_references
            .as_ref()
            .is_some_and(|owners| owners.iter().any(|owner| owner.uid == *uid))
    })
}

fn agent_job(
    investigation: &Investigation,
    name: String,
    prompt: String,
    config: &AgentJobConfig,
) -> Result<Job, ReconcileError> {
    let owner_reference = investigation
        .controller_owner_ref(&())
        .ok_or(ReconcileError::MissingOwnerReference)?;
    let mcp_servers = serde_json::to_string(&config.mcp_servers)?;
    let (mut auth_env, init_containers, volumes, volume_mounts) =
        agent_auth(config, &config.image)?;
    auth_env.push(EnvVar {
        name: "INVESTIGATOR_AGENT_MODEL".to_owned(),
        value: Some(config.model.clone()),
        ..Default::default()
    });
    match &config.provider {
        AgentProvider::OpenAi => auth_env.push(EnvVar {
            name: "INVESTIGATOR_AGENT_PROVIDER".to_owned(),
            value: Some("openai".to_owned()),
            ..Default::default()
        }),
        AgentProvider::Bedrock { region } => {
            auth_env.push(EnvVar {
                name: "INVESTIGATOR_AGENT_PROVIDER".to_owned(),
                value: Some("bedrock".to_owned()),
                ..Default::default()
            });
            auth_env.push(EnvVar {
                name: "AWS_REGION".to_owned(),
                value: Some(region.clone()),
                ..Default::default()
            });
        }
    }
    Ok(Job {
        metadata: ObjectMeta {
            name: Some(name),
            owner_references: Some(vec![owner_reference]),
            ..Default::default()
        },
        spec: Some(k8s_openapi::api::batch::v1::JobSpec {
            backoff_limit: Some(0),
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(
                        [(
                            "app.kubernetes.io/name".to_owned(),
                            "investigator-agent".to_owned(),
                        )]
                        .into(),
                    ),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    restart_policy: Some("Never".to_owned()),
                    service_account_name: Some(config.service_account_name.clone()),
                    node_selector: (!config.node_selector.is_empty())
                        .then(|| config.node_selector.clone()),
                    affinity: config.affinity.clone(),
                    tolerations: (!config.tolerations.is_empty())
                        .then(|| config.tolerations.clone()),
                    containers: vec![Container {
                        name: "codex".to_owned(),
                        image: Some(config.image.clone()),
                        args: Some(codex_args(config, prompt)),
                        env: Some({
                            let mut env = vec![EnvVar {
                                name: "INVESTIGATOR_MCP_SERVERS".to_owned(),
                                value: Some(mcp_servers),
                                ..Default::default()
                            }];
                            env.append(&mut auth_env);
                            env
                        }),
                        volume_mounts: Some(volume_mounts),
                        ..Default::default()
                    }],
                    init_containers,
                    volumes: Some(volumes),
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    })
}

fn codex_args(config: &AgentJobConfig, prompt: String) -> Vec<String> {
    let mut args = vec![
        "exec".to_owned(),
        "--approve-for-me".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "--output-last-message".to_owned(),
        "/dev/termination-log".to_owned(),
        "--model".to_owned(),
        config.model.clone(),
    ];
    if let AgentProvider::Bedrock { region } = &config.provider {
        args.extend([
            "-c".to_owned(),
            "model_provider=\"bedrock\"".to_owned(),
            "-c".to_owned(),
            "model_providers.bedrock.name=\"Amazon Bedrock\"".to_owned(),
            "-c".to_owned(),
            format!("model_providers.bedrock.base_url=\"https://bedrock-mantle.{region}.api.aws/openai/v1\""),
            "-c".to_owned(),
            "model_providers.bedrock.env_key=\"BEDROCK_API_KEY\"".to_owned(),
            "-c".to_owned(),
            "model_providers.bedrock.wire_api=\"responses\"".to_owned(),
        ]);
    }
    args.push(prompt);
    args
}

type AgentAuthResources = (
    Vec<EnvVar>,
    Option<Vec<Container>>,
    Vec<Volume>,
    Vec<VolumeMount>,
);

fn agent_auth(
    config: &AgentJobConfig,
    agent_image: &str,
) -> Result<AgentAuthResources, ReconcileError> {
    let codex_home = Volume {
        name: "codex-home".to_owned(),
        empty_dir: Some(EmptyDirVolumeSource::default()),
        ..Default::default()
    };
    let codex_home_mount = VolumeMount {
        name: "codex-home".to_owned(),
        mount_path: "/home/codex/.codex".to_owned(),
        ..Default::default()
    };

    match &config.auth {
        AgentAuth::ApiKey {
            secret_name,
            secret_key,
        } => Ok((
            vec![EnvVar {
                name: match config.provider {
                    AgentProvider::OpenAi => "OPENAI_API_KEY",
                    AgentProvider::Bedrock { .. } => "BEDROCK_API_KEY",
                }
                .to_owned(),
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: secret_name.clone(),
                        key: secret_key.clone(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            None,
            vec![codex_home],
            vec![codex_home_mount],
        )),
        AgentAuth::AuthJson {
            secret_name,
            secret_key,
        } => {
            let auth_input = Volume {
                name: "codex-auth".to_owned(),
                secret: Some(SecretVolumeSource {
                    secret_name: Some(secret_name.clone()),
                    items: Some(vec![KeyToPath {
                        key: secret_key.clone(),
                        path: "auth.json".to_owned(),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let init = Container {
                name: "install-codex-auth".to_owned(),
                image: Some(agent_image.to_owned()),
                command: Some(vec!["/bin/sh".to_owned(), "-c".to_owned()]),
                args: Some(vec![
                    "cp /var/run/codex-auth/auth.json /home/codex/.codex/auth.json && chmod 0600 /home/codex/.codex/auth.json"
                        .to_owned(),
                ]),
                volume_mounts: Some(vec![
                    VolumeMount {
                        name: "codex-auth".to_owned(),
                        mount_path: "/var/run/codex-auth".to_owned(),
                        read_only: Some(true),
                        ..Default::default()
                    },
                    codex_home_mount.clone(),
                ]),
                ..Default::default()
            };
            Ok((
                vec![EnvVar {
                    name: "CODEX_HOME".to_owned(),
                    value: Some("/home/codex/.codex".to_owned()),
                    ..Default::default()
                }],
                Some(vec![init]),
                vec![codex_home, auth_input],
                vec![codex_home_mount],
            ))
        }
        AgentAuth::WorkloadIdentity => Ok((
            vec![EnvVar {
                name: "INVESTIGATOR_BEDROCK_WORKLOAD_IDENTITY".to_owned(),
                value: Some("true".to_owned()),
                ..Default::default()
            }],
            None,
            vec![codex_home],
            vec![codex_home_mount],
        )),
    }
}

fn error_policy(_: Arc<Investigation>, error: &ReconcileError, _: Arc<Context>) -> Action {
    tracing::warn!(
        event = "investigation_reconciliation_retry_scheduled",
        error = %error,
    );
    Action::requeue(Duration::from_secs(30))
}

#[cfg(test)]
mod tests {
    use super::*;
    use investigator::crd::InvestigationSpec;
    use std::collections::BTreeMap;

    fn config_with_auth(auth: AgentAuth) -> AgentJobConfig {
        AgentJobConfig {
            auth,
            ..AgentJobConfig::default()
        }
    }

    #[test]
    fn api_key_auth_uses_secret_key_ref() {
        let config = config_with_auth(AgentAuth::ApiKey {
            secret_name: "openai".into(),
            secret_key: "api-key".into(),
        });

        let (env, init, volumes, mounts) = agent_auth(&config, "agent:test").unwrap();
        let selector = env[0].value_from.clone().unwrap().secret_key_ref.unwrap();
        assert_eq!(env[0].name, "OPENAI_API_KEY");
        assert_eq!(selector.name, "openai");
        assert_eq!(selector.key, "api-key");
        assert!(init.is_none());
        assert_eq!(volumes.len(), 1);
        assert_eq!(mounts[0].mount_path, "/home/codex/.codex");
    }

    #[test]
    fn auth_json_uses_init_container_and_writable_codex_home() {
        let config = config_with_auth(AgentAuth::AuthJson {
            secret_name: "codex-auth".into(),
            secret_key: "credentials.json".into(),
        });

        let (env, init, volumes, mounts) = agent_auth(&config, "agent:test").unwrap();
        let init = init.unwrap();
        assert_eq!(env[0].name, "CODEX_HOME");
        assert_eq!(env[0].value.as_deref(), Some("/home/codex/.codex"));
        assert_eq!(init[0].image.as_deref(), Some("agent:test"));
        assert_eq!(volumes.len(), 2);
        assert_eq!(mounts[0].name, "codex-home");
    }

    #[test]
    fn bedrock_workload_identity_configures_token_generation_and_provider() {
        let config = AgentJobConfig {
            provider: AgentProvider::Bedrock {
                region: "us-east-1".into(),
            },
            model: "openai.gpt-5.6-terra".into(),
            auth: AgentAuth::WorkloadIdentity,
            ..AgentJobConfig::default()
        };
        let (env, init, _, _) = agent_auth(&config, "agent:test").unwrap();
        assert!(init.is_none());
        assert_eq!(env[0].name, "INVESTIGATOR_BEDROCK_WORKLOAD_IDENTITY");
        let args = codex_args(&config, "investigate".into());
        assert!(args.iter().any(|arg| arg == "model_provider=\"bedrock\""));
        assert!(
            args.iter()
                .any(|arg| arg.contains("bedrock-mantle.us-east-1"))
        );
    }

    #[test]
    fn agent_job_uses_global_scheduling() {
        let mut investigation = Investigation::new(
            "test",
            InvestigationSpec {
                query: "investigate".to_owned(),
                questions: vec![],
            },
        );
        investigation.metadata.uid = Some("test-uid".to_owned());
        let config = AgentJobConfig {
            image: "agent:test".to_owned(),
            service_account_name: "agent".to_owned(),
            model: "gpt-test".to_owned(),
            provider: AgentProvider::OpenAi,
            auth: AgentAuth::ApiKey {
                secret_name: "openai".into(),
                secret_key: "api-key".into(),
            },
            mcp_servers: vec![McpServer {
                name: "kubernetes".to_owned(),
                url: "http://kubernetes-mcp:8080/mcp".to_owned(),
            }],
            node_selector: BTreeMap::from([("workload".to_owned(), "agent".to_owned())]),
            affinity: Some(serde_json::from_value(json!({"nodeAffinity": {}})).unwrap()),
            tolerations: vec![
                serde_json::from_value(json!({"key": "agent", "operator": "Exists"})).unwrap(),
            ],
        };

        let pod = agent_job(
            &investigation,
            "test-agent".to_owned(),
            "investigate".to_owned(),
            &config,
        )
        .unwrap()
        .spec
        .unwrap()
        .template
        .spec
        .unwrap();
        assert_eq!(pod.node_selector.unwrap()["workload"], "agent");
        assert_eq!(pod.containers[0].image.as_deref(), Some("agent:test"));
        assert!(pod.affinity.unwrap().node_affinity.is_some());
        assert_eq!(pod.tolerations.unwrap()[0].key.as_deref(), Some("agent"));
        let mcp_servers = pod.containers[0]
            .env
            .as_ref()
            .unwrap()
            .iter()
            .find(|env| env.name == "INVESTIGATOR_MCP_SERVERS")
            .unwrap();
        assert_eq!(
            mcp_servers.value.as_deref(),
            Some("[{\"name\":\"kubernetes\",\"url\":\"http://kubernetes-mcp:8080/mcp\"}]")
        );
        assert_eq!(
            pod.containers[0].args.as_deref(),
            Some(
                [
                    "exec",
                    "--approve-for-me",
                    "--skip-git-repo-check",
                    "--output-last-message",
                    "/dev/termination-log",
                    "--model",
                    "gpt-test",
                    "investigate",
                ]
                .map(str::to_owned)
                .as_slice()
            )
        );
    }

    #[test]
    fn job_owner_must_match_current_investigation_uid() {
        let mut investigation = Investigation::new(
            "test",
            InvestigationSpec {
                query: "investigate".to_owned(),
                questions: vec![],
            },
        );
        investigation.metadata.uid = Some("old-uid".to_owned());
        let job = agent_job(
            &investigation,
            "test-agent".to_owned(),
            "investigate".to_owned(),
            &AgentJobConfig::default(),
        )
        .unwrap();
        assert!(job_is_owned_by(&job, &investigation));

        investigation.metadata.uid = Some("new-uid".to_owned());
        assert!(!job_is_owned_by(&job, &investigation));
    }

    #[test]
    fn selects_initial_then_follow_up_turns() {
        let investigation = Investigation::new(
            "test",
            InvestigationSpec {
                query: "investigate".to_owned(),
                questions: vec![],
            },
        );
        assert_eq!(
            next_turn(&investigation, &InvestigationStatus::default()),
            Some(0)
        );
        let status = InvestigationStatus {
            result: Some("done".to_owned()),
            ..Default::default()
        };
        assert_eq!(next_turn(&investigation, &status), None);
    }

    #[test]
    fn pod_result_reads_codex_termination_message() {
        let pod: Pod = serde_json::from_value(json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {"name": "test-agent"},
            "spec": {"containers": [{"name": "codex", "image": "agent:test"}]},
            "status": {
                "containerStatuses": [{
                    "name": "codex",
                    "image": "agent:test",
                    "imageID": "agent:test",
                    "ready": false,
                    "restartCount": 0,
                    "state": {"terminated": {
                        "exitCode": 0,
                        "message": "Investigation complete.",
                        "reason": "Completed"
                    }}
                }]
            }
        }))
        .unwrap();

        assert_eq!(pod_result(&pod).as_deref(), Some("Investigation complete."));
    }
}
