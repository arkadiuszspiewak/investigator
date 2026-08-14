use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use k8s_openapi::api::{
    batch::v1::Job,
    core::v1::{Container, EnvVar, PodSpec, PodTemplateSpec},
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

use crate::crd::{Investigation, InvestigationStatus};

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
                        env: Some(vec![EnvVar {
                            name: "INVESTIGATOR_MCP_SERVERS".to_owned(),
                            value: Some(mcp_servers),
                            ..Default::default()
                        }]),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    })
}

fn error_policy(_: Arc<Investigation>, error: &ReconcileError, _: Arc<Context>) -> Action {
    tracing::warn!(%error, "retrying investigation");
    Action::requeue(Duration::from_secs(30))
}
