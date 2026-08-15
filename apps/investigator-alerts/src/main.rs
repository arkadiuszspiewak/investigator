use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use clap::Parser;
use investigator::crd::{AgentAuth, Investigation, InvestigationSpec, McpServer, SecretKeyRef};
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
    #[arg(
        long,
        env = "INVESTIGATION_AGENT_IMAGE",
        default_value = "ghcr.io/arkadiuszspiewak/investigator-agent:latest"
    )]
    agent_image: String,
    #[arg(
        long,
        env = "INVESTIGATION_SERVICE_ACCOUNT",
        default_value = "investigator-agent"
    )]
    service_account: String,
    #[arg(long, env = "INVESTIGATION_API_KEY_SECRET")]
    api_key_secret: Option<String>,
    #[arg(long, env = "INVESTIGATION_AUTH_JSON_SECRET")]
    auth_json_secret: Option<String>,
    #[arg(long, env = "INVESTIGATION_MCP_SERVERS", default_value = "[]")]
    mcp_servers: String,
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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // kube uses rustls with ring while reqwest enables aws-lc-rs. Feature
    // unification therefore leaves rustls unable to choose automatically.
    // Select one provider before either client constructs a TLS configuration.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "investigator_alerts=info".into()),
        )
        .init();
    let config = Config::parse();
    validate_auth(&config)?;
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
    tracing::info!(%address, "listening for Alertmanager webhooks");
    axum::serve(tokio::net::TcpListener::bind(address).await?, app).await?;
    Ok(())
}

async fn receive_alerts(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AlertWebhook>,
) -> StatusCode {
    for alert in payload.alerts.into_iter().filter(|a| a.status == "firing") {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = investigate(state, alert).await {
                tracing::error!(%error, "alert investigation failed");
            }
        });
    }
    StatusCode::ACCEPTED
}

async fn investigate(
    state: Arc<AppState>,
    alert: Alert,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let api = Api::<Investigation>::namespaced(state.client.clone(), &state.config.namespace);
    let suffix = if alert.fingerprint.is_empty() {
        stable_alert_key(&alert)
    } else {
        alert.fingerprint.clone()
    };
    let name = format!("alert-{}", dns_label(&suffix));
    let serialized = serde_json::to_string_pretty(&alert)?;
    let investigation = Investigation::new(
        &name,
        InvestigationSpec {
            query: state.config.query.replace("{{alert}}", &serialized),
            questions: vec![],
            agent_image: state.config.agent_image.clone(),
            auth: configured_auth(&state.config)?,
            mcp_servers: serde_json::from_str::<Vec<McpServer>>(&state.config.mcp_servers)?,
            service_account_name: state.config.service_account.clone(),
        },
    );
    match api.create(&PostParams::default(), &investigation).await {
        Ok(_) => tracing::info!(%name, "created alert investigation"),
        Err(kube::Error::Api(response)) if response.code == 409 => {
            tracing::info!(%name, "alert investigation already exists");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    }
    let relay_thread = if state.config.relay_mode {
        let notification = state.notification.as_ref().expect("validated relay target");
        let message = send_notification(&state.http, notification, &relay_alert_text(&alert), None)
            .await?
            .expect("Slack App returns a message reference");
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
            tracing::warn!(%name, %error, "could not persist Slack thread metadata");
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
                return Err(format!("investigation {name} failed").into());
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn validate_relay_mode(
    config: &Config,
    target: Option<&NotificationTarget>,
) -> Result<(), &'static str> {
    if config.relay_mode && !matches!(target, Some(NotificationTarget::SlackApp { .. })) {
        return Err(
            "RELAY_MODE requires SLACK_BOT_TOKEN and SLACK_CHANNEL; Slack webhooks cannot create reliable threads",
        );
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

fn notification_target(config: &Config) -> Result<Option<NotificationTarget>, &'static str> {
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
            Err("configure either SLACK_WEBHOOK_URL or the Slack App settings, not both")
        }
        _ => Err("Slack App delivery requires both SLACK_BOT_TOKEN and SLACK_CHANNEL"),
    }
}

async fn send_notification(
    http: &reqwest::Client,
    target: &NotificationTarget,
    text: &str,
    thread_ts: Option<&str>,
) -> Result<Option<SlackMessage>, Box<dyn std::error::Error + Send + Sync>> {
    match target {
        NotificationTarget::SlackWebhook { url } => {
            http.post(url)
                .json(&json!({"text": text}))
                .send()
                .await?
                .error_for_status()?;
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
                return Err(format!(
                    "Slack chat.postMessage failed: {}",
                    response.error.as_deref().unwrap_or("unknown_error")
                )
                .into());
            }
            Ok(Some(SlackMessage {
                channel: response.channel.unwrap_or_else(|| channel.clone()),
                ts: response
                    .ts
                    .ok_or("Slack chat.postMessage response did not contain ts")?,
            }))
        }
    }
}

fn validate_auth(config: &Config) -> Result<(), &'static str> {
    configured_auth(config).map(|_| ())
}
fn configured_auth(config: &Config) -> Result<AgentAuth, &'static str> {
    match (&config.api_key_secret, &config.auth_json_secret) {
        (Some(value), None) => Ok(AgentAuth {
            api_key_secret_ref: Some(secret_ref(value)?),
            auth_json_secret_ref: None,
        }),
        (None, Some(value)) => Ok(AgentAuth {
            api_key_secret_ref: None,
            auth_json_secret_ref: Some(secret_ref(value)?),
        }),
        _ => Err("exactly one investigation credential env var must contain NAME:KEY"),
    }
}
fn secret_ref(value: &str) -> Result<SecretKeyRef, &'static str> {
    let (name, key) = value
        .split_once(':')
        .ok_or("credential must use NAME:KEY")?;
    Ok(SecretKeyRef {
        name: name.into(),
        key: key.into(),
    })
}
fn stable_alert_key(alert: &Alert) -> String {
    format!(
        "{}-{}",
        alert
            .labels
            .get("alertname")
            .map(String::as_str)
            .unwrap_or("unknown"),
        alert.starts_at
    )
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
            agent_image: "agent:test".into(),
            service_account: "agent".into(),
            api_key_secret: Some("openai:api-key".into()),
            auth_json_secret: None,
            mcp_servers: "[]".into(),
            slack_webhook_url: None,
            slack_bot_token: None,
            slack_channel: None,
            slack_api_url: "https://slack.com/api".into(),
            relay_mode: false,
        }
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
