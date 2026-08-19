use std::borrow::Cow;

use k8s_openapi::api::batch::v1::Job;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::{error::AppError, server::KubernetesServer};

use super::common::{
    GetNamespacedArgs, ListNamespacedArgs, paginated_params, read_only_annotations,
};

#[derive(Debug, Serialize, JsonSchema)]
pub struct JobSummary {
    name: Option<String>,
    namespace: Option<String>,
    generation: Option<i64>,
    active: i32,
    succeeded: i32,
    failed: i32,
    completions: Option<i32>,
    parallelism: Option<i32>,
    backoff_limit: Option<i32>,
    start_time: Option<String>,
    completion_time: Option<String>,
    images: Vec<String>,
    conditions: Vec<JobConditionSummary>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct JobConditionSummary {
    condition: String,
    status: String,
    reason: Option<String>,
    message: Option<String>,
}

impl From<Job> for JobSummary {
    fn from(job: Job) -> Self {
        let status = job.status.as_ref();
        let spec = job.spec.as_ref();
        Self {
            name: job.metadata.name,
            namespace: job.metadata.namespace,
            generation: job.metadata.generation,
            active: status.and_then(|status| status.active).unwrap_or(0),
            succeeded: status.and_then(|status| status.succeeded).unwrap_or(0),
            failed: status.and_then(|status| status.failed).unwrap_or(0),
            completions: spec.and_then(|spec| spec.completions),
            parallelism: spec.and_then(|spec| spec.parallelism),
            backoff_limit: spec.and_then(|spec| spec.backoff_limit),
            start_time: status
                .and_then(|status| status.start_time.as_ref())
                .map(|time| time.0.to_string()),
            completion_time: status
                .and_then(|status| status.completion_time.as_ref())
                .map(|time| time.0.to_string()),
            images: spec
                .and_then(|spec| spec.template.spec.as_ref())
                .map(|pod_spec| {
                    pod_spec
                        .containers
                        .iter()
                        .filter_map(|container| container.image.clone())
                        .collect()
                })
                .unwrap_or_default(),
            conditions: status
                .and_then(|status| status.conditions.as_ref())
                .into_iter()
                .flatten()
                .map(|condition| JobConditionSummary {
                    condition: condition.type_.clone(),
                    status: condition.status.clone(),
                    reason: condition.reason.clone(),
                    message: condition.message.clone(),
                })
                .collect(),
        }
    }
}

pub struct GetJob;

impl ToolBase for GetJob {
    type Parameter = GetNamespacedArgs;
    type Output = JobSummary;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "get_job".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Get a compact Kubernetes Job health summary; prefer over get_resource".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for GetJob {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let jobs: Api<Job> = Api::namespaced(server.client(), &args.namespace);
        Ok(JobSummary::from(jobs.get(&args.name).await?))
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListJobsOutput {
    jobs: Vec<JobSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_continue_token: Option<String>,
}

pub struct ListJobs;

impl ToolBase for ListJobs {
    type Parameter = ListNamespacedArgs;
    type Output = ListJobsOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_jobs".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "List compact Kubernetes Job summaries in one namespace; prefer over list_resource"
                .into(),
        )
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for ListJobs {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        let jobs: Api<Job> = Api::namespaced(server.client(), &args.namespace);
        let mut params = paginated_params(args.limit, args.continue_token);
        if let Some(selector) = args.label_selector.as_deref() {
            params = params.labels(selector);
        }
        let page = jobs.list(&params).await?;
        Ok(ListJobsOutput {
            next_continue_token: page.metadata.continue_,
            jobs: page.items.into_iter().map(JobSummary::from).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_omits_job_template() {
        let summary = serde_json::to_value(JobSummary::from(Job::default())).unwrap();
        assert!(summary.get("spec").is_none());
        assert!(summary.get("status").is_none());
        assert!(summary.get("images").is_some());
    }
}
