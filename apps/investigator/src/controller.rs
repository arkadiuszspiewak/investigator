use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use k8s_openapi::api::{
    batch::v1::Job,
    core::v1::{
        Container, EmptyDirVolumeSource, EnvVar, EnvVarSource, KeyToPath, PodSpec, PodTemplateSpec,
        SecretKeySelector, SecretVolumeSource, Volume, VolumeMount,
    },
};
use kube::{
    Api, Client, Resource, ResourceExt,
    api::{ObjectMeta, Patch, PatchParams, PostParams},
    runtime::{
        controller::{Action, Controller},
        watcher,
    },
};
use serde_json::json;
use thiserror::Error;

use crate::crd::{AgentAuth, Investigation, InvestigationStatus};

#[derive(Clone)]
struct Context {
    client: Client,
}

#[derive(Debug, Error)]
enum ReconcileError {
    #[error("Kubernetes API request failed: {0}")]
    Kubernetes(#[from] kube::Error),
    #[error("Investigation is missing its namespace")]
    MissingNamespace,
    #[error("Investigation is missing its owner reference data")]
    MissingOwnerReference,
    #[error("could not serialize MCP server configuration: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("exactly one of auth.apiKeySecretRef or auth.authJsonSecretRef must be set")]
    InvalidAuth,
}

pub async fn run(client: Client) {
    let investigations = Api::<Investigation>::all(client.clone());
    Controller::new(investigations, watcher::Config::default())
        .owns(Api::<Job>::all(client.clone()), watcher::Config::default())
        .run(reconcile, error_policy, Arc::new(Context { client }))
        .for_each(|result| async move {
            match result {
                Ok(object) => tracing::info!(?object, "reconciled investigation"),
                Err(error) => tracing::error!(%error, "reconciliation failed"),
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
    let job_name = format!("{}-agent", investigation.name_any());
    let jobs = Api::<Job>::namespaced(context.client.clone(), &namespace);

    let phase = match jobs.get_opt(&job_name).await? {
        Some(job) if job.status.as_ref().and_then(|s| s.succeeded).unwrap_or(0) > 0 => "Succeeded",
        Some(job) if job.status.as_ref().and_then(|s| s.failed).unwrap_or(0) > 0 => "Failed",
        Some(_) => "Running",
        None => {
            jobs.create(
                &PostParams::default(),
                &agent_job(&investigation, job_name.clone())?,
            )
            .await?;
            "Pending"
        }
    };

    let status = InvestigationStatus {
        phase: Some(phase.to_owned()),
        job_name: Some(job_name),
        message: None,
        observed_generation: investigation.metadata.generation,
    };
    if investigation.status.as_ref() != Some(&status) {
        let investigations = Api::<Investigation>::namespaced(context.client.clone(), &namespace);
        investigations
            .patch_status(
                &investigation.name_any(),
                &PatchParams::default(),
                &Patch::Merge(json!({ "status": status })),
            )
            .await?;
    }

    Ok(Action::requeue(Duration::from_secs(10)))
}

fn agent_job(investigation: &Investigation, name: String) -> Result<Job, ReconcileError> {
    let owner_reference = investigation
        .controller_owner_ref(&())
        .ok_or(ReconcileError::MissingOwnerReference)?;
    let mcp_servers = serde_json::to_string(&investigation.spec.mcp_servers)?;
    let (auth_env, init_containers, volumes, volume_mounts) =
        agent_auth(&investigation.spec.auth, &investigation.spec.agent_image)?;
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
                    service_account_name: Some(investigation.spec.service_account_name.clone()),
                    containers: vec![Container {
                        name: "codex".to_owned(),
                        image: Some(investigation.spec.agent_image.clone()),
                        args: Some(vec![
                            "exec".to_owned(),
                            "--full-auto".to_owned(),
                            investigation.spec.query.clone(),
                        ]),
                        env: Some(vec![
                            EnvVar {
                                name: "INVESTIGATOR_MCP_SERVERS".to_owned(),
                                value: Some(mcp_servers),
                                ..Default::default()
                            },
                            auth_env,
                        ]),
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

fn agent_auth(
    auth: &AgentAuth,
    agent_image: &str,
) -> Result<
    (
        EnvVar,
        Option<Vec<Container>>,
        Vec<Volume>,
        Vec<VolumeMount>,
    ),
    ReconcileError,
> {
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

    match (&auth.api_key_secret_ref, &auth.auth_json_secret_ref) {
        (Some(secret), None) => Ok((
            EnvVar {
                name: "OPENAI_API_KEY".to_owned(),
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: secret.name.clone(),
                        key: secret.key.clone(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            None,
            vec![codex_home],
            vec![codex_home_mount],
        )),
        (None, Some(secret)) => {
            let auth_input = Volume {
                name: "codex-auth".to_owned(),
                secret: Some(SecretVolumeSource {
                    secret_name: Some(secret.name.clone()),
                    items: Some(vec![KeyToPath {
                        key: secret.key.clone(),
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
                EnvVar {
                    name: "CODEX_HOME".to_owned(),
                    value: Some("/home/codex/.codex".to_owned()),
                    ..Default::default()
                },
                Some(vec![init]),
                vec![codex_home, auth_input],
                vec![codex_home_mount],
            ))
        }
        _ => Err(ReconcileError::InvalidAuth),
    }
}

fn error_policy(_: Arc<Investigation>, error: &ReconcileError, _: Arc<Context>) -> Action {
    tracing::warn!(%error, "retrying investigation");
    Action::requeue(Duration::from_secs(30))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::SecretKeyRef;

    fn secret(name: &str, key: &str) -> SecretKeyRef {
        SecretKeyRef {
            name: name.to_owned(),
            key: key.to_owned(),
        }
    }

    #[test]
    fn api_key_auth_uses_secret_key_ref() {
        let auth = AgentAuth {
            api_key_secret_ref: Some(secret("openai", "api-key")),
            auth_json_secret_ref: None,
        };

        let (env, init, volumes, mounts) = agent_auth(&auth, "agent:test").unwrap();
        let selector = env.value_from.unwrap().secret_key_ref.unwrap();
        assert_eq!(env.name, "OPENAI_API_KEY");
        assert_eq!(selector.name, "openai");
        assert_eq!(selector.key, "api-key");
        assert!(init.is_none());
        assert_eq!(volumes.len(), 1);
        assert_eq!(mounts[0].mount_path, "/home/codex/.codex");
    }

    #[test]
    fn auth_json_uses_init_container_and_writable_codex_home() {
        let auth = AgentAuth {
            api_key_secret_ref: None,
            auth_json_secret_ref: Some(secret("codex-auth", "credentials.json")),
        };

        let (env, init, volumes, mounts) = agent_auth(&auth, "agent:test").unwrap();
        let init = init.unwrap();
        assert_eq!(env.name, "CODEX_HOME");
        assert_eq!(env.value.as_deref(), Some("/home/codex/.codex"));
        assert_eq!(init[0].image.as_deref(), Some("agent:test"));
        assert_eq!(volumes.len(), 2);
        assert_eq!(mounts[0].name, "codex-home");
    }

    #[test]
    fn auth_requires_exactly_one_source() {
        let neither = AgentAuth {
            api_key_secret_ref: None,
            auth_json_secret_ref: None,
        };
        assert!(matches!(
            agent_auth(&neither, "agent:test"),
            Err(ReconcileError::InvalidAuth)
        ));

        let both = AgentAuth {
            api_key_secret_ref: Some(secret("openai", "api-key")),
            auth_json_secret_ref: Some(secret("codex-auth", "auth.json")),
        };
        assert!(matches!(
            agent_auth(&both, "agent:test"),
            Err(ReconcileError::InvalidAuth)
        ));
    }
}
