use std::borrow::Cow;

use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase, ToolRouter},
    model::ToolAnnotations,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{error::AppError, server::AlertmanagerServer};

#[derive(Debug, Serialize, JsonSchema)]
pub struct AlertmanagerOutput {
    /// Unmodified response returned by the Alertmanager v2 API.
    data: Value,
}

fn read_only_annotations() -> Option<ToolAnnotations> {
    Some(ToolAnnotations::new().read_only(true).idempotent(true))
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AlertQueryArgs {
    /// Alertmanager label matchers, for example `severity="critical"` or `namespace=~"prod-.*"`.
    #[serde(default)]
    filters: Vec<String>,
    /// Optional regular expression matching receiver names.
    #[serde(default)]
    receiver: Option<String>,
    /// Include silenced alerts. Omit to use Alertmanager's default.
    #[serde(default)]
    silenced: Option<bool>,
    /// Include inhibited alerts. Omit to use Alertmanager's default.
    #[serde(default)]
    inhibited: Option<bool>,
    /// Include alerts that have not yet been processed. Omit to use Alertmanager's default.
    #[serde(default)]
    unprocessed: Option<bool>,
}

pub struct ListActiveAlerts;

impl ToolBase for ListActiveAlerts {
    type Parameter = AlertQueryArgs;
    type Output = AlertmanagerOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_active_alerts".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "List active alerts, optionally filtered by labels, receiver, and suppression state"
                .into(),
        )
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<AlertmanagerServer> for ListActiveAlerts {
    async fn invoke(
        server: &AlertmanagerServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        Ok(AlertmanagerOutput {
            data: server
                .client()
                .get("/api/v2/alerts", &alert_params(args))
                .await?,
        })
    }
}

pub struct ListAlertGroups;

impl ToolBase for ListAlertGroups {
    type Parameter = AlertQueryArgs;
    type Output = AlertmanagerOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_alert_groups".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List active alerts grouped as Alertmanager routes and displays them".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<AlertmanagerServer> for ListAlertGroups {
    async fn invoke(
        server: &AlertmanagerServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        Ok(AlertmanagerOutput {
            data: server
                .client()
                .get("/api/v2/alerts/groups", &alert_params(args))
                .await?,
        })
    }
}

pub struct ListSilences;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListSilencesArgs {
    /// Alertmanager label matchers used to limit returned silences.
    #[serde(default)]
    filters: Vec<String>,
}

impl ToolBase for ListSilences {
    type Parameter = ListSilencesArgs;
    type Output = AlertmanagerOutput;
    type Error = AppError;

    fn name() -> Cow<'static, str> {
        "list_silences".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List silences and their current state, optionally filtered by label matchers".into())
    }

    fn annotations() -> Option<ToolAnnotations> {
        read_only_annotations()
    }
}

impl AsyncTool<AlertmanagerServer> for ListSilences {
    async fn invoke(
        server: &AlertmanagerServer,
        args: Self::Parameter,
    ) -> Result<Self::Output, Self::Error> {
        Ok(AlertmanagerOutput {
            data: server
                .client()
                .get("/api/v2/silences", &filter_params(args.filters))
                .await?,
        })
    }
}

macro_rules! no_arg_tool {
    ($tool:ident, $name:literal, $description:literal, $path:literal) => {
        pub struct $tool;

        impl ToolBase for $tool {
            type Parameter = EmptyArgs;
            type Output = AlertmanagerOutput;
            type Error = AppError;

            fn name() -> Cow<'static, str> {
                $name.into()
            }
            fn description() -> Option<Cow<'static, str>> {
                Some($description.into())
            }
            fn annotations() -> Option<ToolAnnotations> {
                read_only_annotations()
            }
        }

        impl AsyncTool<AlertmanagerServer> for $tool {
            async fn invoke(
                server: &AlertmanagerServer,
                _args: Self::Parameter,
            ) -> Result<Self::Output, Self::Error> {
                Ok(AlertmanagerOutput {
                    data: server.client().get($path, &[]).await?,
                })
            }
        }
    };
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct EmptyArgs {}

no_arg_tool!(
    ListReceivers,
    "list_receivers",
    "List configured receiver names",
    "/api/v2/receivers"
);
no_arg_tool!(
    GetStatus,
    "get_status",
    "Get Alertmanager cluster, version, and configuration status",
    "/api/v2/status"
);

fn alert_params(args: AlertQueryArgs) -> Vec<(&'static str, String)> {
    let mut params = filter_params(args.filters);
    params.push(("active", "true".to_owned()));
    push_optional(&mut params, "receiver", args.receiver);
    push_optional_bool(&mut params, "silenced", args.silenced);
    push_optional_bool(&mut params, "inhibited", args.inhibited);
    push_optional_bool(&mut params, "unprocessed", args.unprocessed);
    params
}

fn filter_params(filters: Vec<String>) -> Vec<(&'static str, String)> {
    filters
        .into_iter()
        .map(|filter| ("filter", filter))
        .collect()
}

fn push_optional(
    params: &mut Vec<(&'static str, String)>,
    name: &'static str,
    value: Option<String>,
) {
    if let Some(value) = value {
        params.push((name, value));
    }
}

fn push_optional_bool(
    params: &mut Vec<(&'static str, String)>,
    name: &'static str,
    value: Option<bool>,
) {
    if let Some(value) = value {
        params.push((name, value.to_string()));
    }
}

pub fn router() -> ToolRouter<AlertmanagerServer> {
    ToolRouter::new()
        .with_async_tool::<ListActiveAlerts>()
        .with_async_tool::<ListAlertGroups>()
        .with_async_tool::<ListSilences>()
        .with_async_tool::<ListReceivers>()
        .with_async_tool::<GetStatus>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_only_the_focused_read_only_tools() {
        let tools = router().list_all();
        assert!(tools.iter().all(|tool| {
            tool.annotations
                .as_ref()
                .and_then(|value| value.read_only_hint)
                == Some(true)
        }));
        let mut names: Vec<_> = tools
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "get_status",
                "list_active_alerts",
                "list_alert_groups",
                "list_receivers",
                "list_silences"
            ]
        );
    }

    #[test]
    fn active_alert_params_preserve_repeated_filters() {
        let params = alert_params(AlertQueryArgs {
            filters: vec![
                "severity=\"critical\"".to_owned(),
                "namespace=\"prod\"".to_owned(),
            ],
            receiver: Some("pager.*".to_owned()),
            silenced: Some(false),
            ..Default::default()
        });
        assert_eq!(
            params.iter().filter(|(name, _)| *name == "filter").count(),
            2
        );
        assert!(params.contains(&("active", "true".to_owned())));
        assert!(params.contains(&("silenced", "false".to_owned())));
    }
}
