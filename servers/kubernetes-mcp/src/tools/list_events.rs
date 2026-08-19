use std::borrow::Cow;

use k8s_openapi::api::core::v1::Event;
use kube::Api;
use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::AppError, server::KubernetesServer};

use super::common::read_only_annotations;

pub struct ListEvents;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListEventsArgs {
    /// Namespace to query.
    #[serde(default)]
    namespace: Option<String>,

    /// Must be true to explicitly query all namespaces.
    #[serde(default)]
    all_namespaces: bool,

    #[serde(default)]
    field_selector: Option<String>,

    /// Maximum items to return. Defaults to 50 and is capped at 100.
    #[serde(default)]
    limit: Option<u32>,

    /// Opaque continuation token returned by a previous call.
    #[serde(default)]
    continue_token: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct EventSummary {
    namespace: Option<String>,
    event_type: Option<String>,
    reason: Option<String>,
    action: Option<String>,
    message: Option<String>,
    count: Option<i32>,
    first_seen: Option<String>,
    last_seen: Option<String>,
    involved_object_kind: Option<String>,
    involved_object_namespace: Option<String>,
    involved_object_name: Option<String>,
    reporting_component: Option<String>,
    reporting_instance: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListEventsOutput {
    events: Vec<EventSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_continue_token: Option<String>,
}

impl From<Event> for EventSummary {
    fn from(event: Event) -> Self {
        let series_last_seen = event
            .series
            .as_ref()
            .and_then(|series| series.last_observed_time.as_ref())
            .map(|time| time.0.to_string());
        Self {
            namespace: event.metadata.namespace,
            event_type: event.type_,
            reason: event.reason,
            action: event.action,
            message: event.message,
            count: event
                .count
                .or_else(|| event.series.as_ref().and_then(|series| series.count)),
            first_seen: event.first_timestamp.map(|time| time.0.to_string()),
            last_seen: series_last_seen
                .or_else(|| event.last_timestamp.map(|time| time.0.to_string()))
                .or_else(|| event.event_time.map(|time| time.0.to_string())),
            involved_object_kind: event.involved_object.kind,
            involved_object_namespace: event.involved_object.namespace,
            involved_object_name: event.involved_object.name,
            reporting_component: event.reporting_component.or_else(|| {
                event
                    .source
                    .as_ref()
                    .and_then(|source| source.component.clone())
            }),
            reporting_instance: event
                .reporting_instance
                .or_else(|| event.source.and_then(|source| source.host)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_omits_raw_event_metadata() {
        let summary = serde_json::to_value(EventSummary::from(Event::default())).unwrap();
        assert!(summary.get("metadata").is_none());
        assert!(summary.get("involved_object").is_none());
        assert!(summary.get("reason").is_some());
        assert!(summary.get("message").is_some());
    }
}

impl ToolBase for ListEvents {
    type Parameter = ListEventsArgs;
    type Output = ListEventsOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_events".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List compact Event summaries; prefer over list_resource for Events".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<KubernetesServer> for ListEvents {
    async fn invoke(
        server: &KubernetesServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        super::common::require_explicit_scope(
            "list_events",
            args.namespace.as_deref(),
            args.all_namespaces,
        )?;
        let events: Api<Event> = match args.namespace.as_deref() {
            Some(namespace) => Api::namespaced(server.client(), namespace),
            None => Api::all(server.client()),
        };
        let mut params = super::common::paginated_params(args.limit, args.continue_token);
        if let Some(selector) = args.field_selector.as_deref() {
            params = params.fields(selector);
        }
        let page = events.list(&params).await?;
        let next_continue_token = page.metadata.continue_;
        let events = page.items.into_iter().map(EventSummary::from).collect();
        Ok(ListEventsOutput {
            events,
            next_continue_token,
        })
    }
}
