mod error;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use clap::Parser;
use error::{AppError, ConfigurationError};
use investigator::crd::{Investigation, InvestigationSpec};
use kube::{
    Api, Client,
    api::{Patch, PatchParams, PostParams},
};
use serde::Deserialize;
use serde_json::json;

#[derive(Parser, Clone)]
struct Config {
    #[arg(long, env = "ALERTS_BIND_ADDRESS", default_value = "0.0.0.0:8080")]
    bind_address: String,
    #[arg(long, env = "INVESTIGATION_NAMESPACE", default_value_t = default_investigation_namespace())]
    namespace: String,
    #[arg(
        long,
        env = "INVESTIGATION_QUERY",
        default_value = "Investigate this active alert. Explain what happened, why it happened, and how to fix it. Alert: {{alert}}"
    )]
    query: String,
    #[arg(long, env = "SLACK_WEBHOOK_URL")]
    slack_webhook_url: Option<String>,
    /// Slack App bot token (xoxb-...). Must be used with SLACK_CHANNEL.
    #[arg(long, env = "SLACK_BOT_TOKEN")]
    slack_bot_token: Option<String>,
    /// Channel ID or name used by Slack App delivery.
    #[arg(long, env = "SLACK_CHANNEL")]
    slack_channel: Option<String>,
    #[arg(long, env = "SLACK_API_URL", default_value = "https://slack.com/api")]
    slack_api_url: String,
    /// Post the original alert and put the investigation result in its thread.
    /// Requires Slack App delivery; disabled keeps the independent notification flow.
    #[arg(long, env = "RELAY_MODE", default_value_t = false)]
    relay_mode: bool,
}

#[derive(Clone)]
struct AppState {
    config: Config,
    client: Client,
    http: reqwest::Client,
    notification: Option<NotificationTarget>,
}

#[derive(Clone)]
enum NotificationTarget {
    SlackWebhook {
        url: String,
    },
    SlackApp {
        api_url: String,
        bot_token: String,
        channel: String,
    },
}

#[derive(Deserialize)]
struct SlackResponse {
    ok: bool,
    error: Option<String>,
    ts: Option<String>,
    channel: Option<String>,
}

struct SlackMessage {
    channel: String,
    ts: String,
}

fn default_investigation_namespace() -> String {
    std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
        .ok()
        .map(|namespace| namespace.trim().to_owned())
        .filter(|namespace| !namespace.is_empty())
        .unwrap_or_else(|| "default".to_owned())
}

#[derive(Clone, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Alert {
    status: String,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    #[serde(default)]
    annotations: BTreeMap<String, String>,
    #[serde(default)]
    fingerprint: String,
    #[serde(default)]
    starts_at: String,
    #[serde(default)]
    generator_url: String,
}

#[derive(Deserialize)]
struct AlertWebhook {
    alerts: Vec<Alert>,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // kube uses rustls with ring while reqwest enables aws-lc-rs. Feature
    // unification therefore leaves rustls unable to choose automatically.
    // Select one provider before either client constructs a TLS configuration.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "investigator_alerts=info".into()),
        )
        .init();

    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(event = "application_failed", error = %error);
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), AppError> {
    let config = Config::parse();
    let notification = notification_target(&config)?;
    validate_relay_mode(&config, notification.as_ref())?;
    let address: std::net::SocketAddr = config.bind_address.parse()?;
    let state = Arc::new(AppState {
        config,
        client: Client::try_default().await?,
        http: reqwest::Client::new(),
        notification,
    });
    let app = Router::new()
        .route("/healthz", get(|| async { StatusCode::NO_CONTENT }))
        .route("/alerts", post(receive_alerts))
        .with_state(state);
    tracing::info!(
        event = "http_server_started",
        transport = "http",
        address = %address,
    );
    axum::serve(tokio::net::TcpListener::bind(address).await?, app).await?;
    Ok(())
}

async fn receive_alerts(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AlertWebhook>,
) -> StatusCode {
    let alert_count = payload.alerts.len();
    let firing_count = payload
        .alerts
        .iter()
        .filter(|alert| alert.status == "firing")
        .count();
    tracing::info!(event = "alert_webhook_accepted", alert_count, firing_count,);
    for alert in payload
        .alerts
        .into_iter()
        .filter(|alert| alert.status == "firing")
    {
        let state = state.clone();
        let investigation_name = alert_investigation_name(&alert);
        tokio::spawn(async move {
            if let Err(error) = investigate(state, alert).await {
                tracing::error!(
                    event = "alert_investigation_failed",
                    investigation_name,
                    error = %error,
                );
            }
        });
    }
    StatusCode::ACCEPTED
}

async fn investigate(state: Arc<AppState>, alert: Alert) -> Result<(), AppError> {
    let api = Api::<Investigation>::namespaced(state.client.clone(), &state.config.namespace);
    let name = alert_investigation_name(&alert);
    let serialized = serde_json::to_string_pretty(&alert)?;
    let investigation = Investigation::new(
        &name,
        InvestigationSpec {
            query: state.config.query.replace("{{alert}}", &serialized),
            questions: vec![],
        },
    );
    match api.create(&PostParams::default(), &investigation).await {
        Ok(_) => tracing::info!(
            event = "alert_investigation_created",
            investigation_name = name,
        ),
        Err(kube::Error::Api(response)) if response.code == 409 => {
            tracing::info!(
                event = "alert_investigation_already_exists",
                investigation_name = name,
            );
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    }
    let relay_thread = if state.config.relay_mode {
        let notification = state
            .notification
            .as_ref()
            .ok_or(AppError::MissingRelayTarget)?;
        let message = send_notification(&state.http, notification, &relay_alert_text(&alert), None)
            .await?
            .ok_or(AppError::MissingSlackMessage)?;
        if let Err(error) = api
            .patch(
                &name,
                &PatchParams::default(),
                &Patch::Merge(json!({"metadata": {"annotations": {
                    "investigator.openai.com/slack-channel": message.channel,
                    "investigator.openai.com/slack-thread-ts": message.ts,
                }}})),
            )
            .await
        {
            tracing::warn!(
                event = "slack_thread_metadata_persist_failed",
                investigation_name = name,
                error = %error,
            );
        }
        Some(message)
    } else {
        None
    };
    loop {
        let current = api.get(&name).await?;
        if let Some(status) = current.status {
            if status.phase.as_deref() == Some("Succeeded") {
                if let (Some(notification), Some(result)) = (&state.notification, status.result) {
                    let text = if state.config.relay_mode {
                        format!("*Investigation complete*\n{result}")
                    } else {
                        format!("Alert investigation `{name}`\n{result}")
                    };
                    send_notification(
                        &state.http,
                        notification,
                        &text,
                        relay_thread.as_ref().map(|message| message.ts.as_str()),
                    )
                    .await?;
                }
                return Ok(());
            }
            if status.phase.as_deref() == Some("Failed") {
                return Err(AppError::InvestigationFailed { name });
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn validate_relay_mode(
    config: &Config,
    target: Option<&NotificationTarget>,
) -> Result<(), ConfigurationError> {
    if config.relay_mode && !matches!(target, Some(NotificationTarget::SlackApp { .. })) {
        return Err(ConfigurationError::RelayRequiresSlackApp);
    }
    Ok(())
}

fn relay_alert_text(alert: &Alert) -> String {
    let alertname = alert
        .labels
        .get("alertname")
        .map(String::as_str)
        .unwrap_or("Alert");
    let severity = alert
        .labels
        .get("severity")
        .map(String::as_str)
        .unwrap_or("unknown");
    let namespace = alert
        .labels
        .get("namespace")
        .map(String::as_str)
        .unwrap_or("unknown");
    let summary = alert
        .annotations
        .get("summary")
        .or_else(|| alert.annotations.get("message"))
        .or_else(|| alert.annotations.get("description"))
        .map(String::as_str)
        .unwrap_or("No summary provided");
    format!(
        "*[FIRING] {alertname}*\nSeverity: `{severity}` · Namespace: `{namespace}`\n{summary}\n_Investigation started · fingerprint `{}`_",
        alert.fingerprint
    )
}

fn notification_target(config: &Config) -> Result<Option<NotificationTarget>, ConfigurationError> {
    match (
        &config.slack_webhook_url,
        &config.slack_bot_token,
        &config.slack_channel,
    ) {
        (None, None, None) => Ok(None),
        (Some(url), None, None) => Ok(Some(NotificationTarget::SlackWebhook { url: url.clone() })),
        (None, Some(bot_token), Some(channel)) => Ok(Some(NotificationTarget::SlackApp {
            api_url: config.slack_api_url.trim_end_matches('/').to_owned(),
            bot_token: bot_token.clone(),
            channel: channel.clone(),
        })),
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            Err(ConfigurationError::ConflictingSlackSettings)
        }
        _ => Err(ConfigurationError::IncompleteSlackAppSettings),
    }
}

async fn send_notification(
    http: &reqwest::Client,
    target: &NotificationTarget,
    text: &str,
    thread_ts: Option<&str>,
) -> Result<Option<SlackMessage>, AppError> {
    match target {
        NotificationTarget::SlackWebhook { url } => {
            http.post(url)
                .json(&json!({"text": text}))
                .send()
                .await?
                .error_for_status()?;
            tracing::info!(
                event = "slack_message_sent",
                delivery_method = "webhook",
                threaded = false,
            );
            Ok(None)
        }
        NotificationTarget::SlackApp {
            api_url,
            bot_token,
            channel,
        } => {
            let mut body = json!({"channel": channel, "text": text});
            if let Some(thread_ts) = thread_ts {
                body["thread_ts"] = json!(thread_ts);
            }
            let response = http
                .post(format!("{api_url}/chat.postMessage"))
                .bearer_auth(bot_token)
                .json(&body)
                .send()
                .await?
                .error_for_status()?
                .json::<SlackResponse>()
                .await?;
            if !response.ok {
                return Err(AppError::SlackApi {
                    code: response.error.unwrap_or_else(|| "unknown_error".to_owned()),
                });
            }
            let message = SlackMessage {
                channel: response.channel.unwrap_or_else(|| channel.clone()),
                ts: response.ts.ok_or(AppError::MissingSlackTimestamp)?,
            };
            tracing::info!(
                event = "slack_message_sent",
                delivery_method = "slack_app",
                channel = message.channel,
                message_ts = message.ts,
                threaded = thread_ts.is_some(),
            );
            Ok(Some(message))
        }
    }
}

fn alert_investigation_name(alert: &Alert) -> String {
    let identity = if alert.fingerprint.is_empty() {
        alert
            .labels
            .get("alertname")
            .map(String::as_str)
            .unwrap_or("unknown")
    } else {
        &alert.fingerprint
    };

    // Alertmanager keeps startsAt stable while an alert continuously fires, but
    // changes it when the same label set fires again after resolving. Put it
    // first so dns_label's length limit preserves the episode discriminator.
    let episode = if alert.starts_at.is_empty() {
        identity.to_owned()
    } else {
        format!("{}-{identity}", alert.starts_at)
    };
    format!("alert-{}", dns_label(&episode))
}
fn dns_label(value: &str) -> String {
    let value: String = value
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    value.trim_matches('-').chars().take(50).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            bind_address: "127.0.0.1:8080".into(),
            namespace: "default".into(),
            query: "investigate {{alert}}".into(),
            slack_webhook_url: None,
            slack_bot_token: None,
            slack_channel: None,
            slack_api_url: "https://slack.com/api".into(),
            relay_mode: false,
        }
    }

    fn alert(fingerprint: &str, starts_at: &str) -> Alert {
        Alert {
            status: "firing".into(),
            labels: BTreeMap::from([("alertname".into(), "HighErrorRate".into())]),
            annotations: BTreeMap::new(),
            fingerprint: fingerprint.into(),
            starts_at: starts_at.into(),
            generator_url: String::new(),
        }
    }

    #[test]
    fn repeated_delivery_for_same_firing_episode_has_same_name() {
        let first = alert("0625dac3f3a7ec87", "2026-08-15T17:25:00Z");
        let repeat = alert("0625dac3f3a7ec87", "2026-08-15T17:25:00Z");

        assert_eq!(
            alert_investigation_name(&first),
            alert_investigation_name(&repeat)
        );
    }

    #[test]
    fn later_firing_episode_has_different_name() {
        let first = alert("0625dac3f3a7ec87", "2026-08-15T17:25:00Z");
        let refire = alert("0625dac3f3a7ec87", "2026-08-16T09:10:00Z");

        assert_ne!(
            alert_investigation_name(&first),
            alert_investigation_name(&refire)
        );
    }

    #[test]
    fn accepts_slack_app_token_and_channel() {
        let mut config = config();
        config.slack_bot_token = Some("xoxb-test".into());
        config.slack_channel = Some("C123".into());
        assert!(matches!(
            notification_target(&config),
            Ok(Some(NotificationTarget::SlackApp { .. }))
        ));
    }

    #[test]
    fn rejects_partial_or_mixed_slack_configuration() {
        let mut partial = config();
        partial.slack_bot_token = Some("xoxb-test".into());
        assert!(notification_target(&partial).is_err());

        let mut mixed = config();
        mixed.slack_webhook_url = Some("https://hooks.slack.test".into());
        mixed.slack_bot_token = Some("xoxb-test".into());
        mixed.slack_channel = Some("C123".into());
        assert!(notification_target(&mixed).is_err());
    }

    #[test]
    fn relay_mode_requires_a_slack_app() {
        let mut config = config();
        config.relay_mode = true;
        assert!(validate_relay_mode(&config, None).is_err());

        config.slack_bot_token = Some("xoxb-test".into());
        config.slack_channel = Some("C123".into());
        let target = notification_target(&config).unwrap();
        assert!(validate_relay_mode(&config, target.as_ref()).is_ok());
    }
}
