use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use clap::Parser;
use investigator::crd::{AgentAuth, Investigation, InvestigationSpec, McpServer, SecretKeyRef};
use kube::{Api, Client, api::PostParams};
use serde::Deserialize;
use serde_json::json;

#[derive(Parser, Clone)]
struct Config {
    #[arg(long, env = "ALERTS_BIND_ADDRESS", default_value = "0.0.0.0:8080")]
    bind_address: String,
    #[arg(long, env = "INVESTIGATION_NAMESPACE", default_value = "default")]
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
}

#[derive(Clone)]
struct AppState {
    config: Config,
    client: Client,
    http: reqwest::Client,
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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "investigator_alerts=info".into()),
        )
        .init();
    let config = Config::parse();
    validate_auth(&config)?;
    let address: std::net::SocketAddr = config.bind_address.parse()?;
    let state = Arc::new(AppState {
        config,
        client: Client::try_default().await?,
        http: reqwest::Client::new(),
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
            node_selector: Default::default(),
            affinity: None,
            tolerations: vec![],
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
    loop {
        let current = api.get(&name).await?;
        if let Some(status) = current.status {
            if status.phase.as_deref() == Some("Succeeded") {
                if let (Some(webhook), Some(result)) =
                    (&state.config.slack_webhook_url, status.result)
                {
                    state
                        .http
                        .post(webhook)
                        .json(&json!({"text": format!("Alert investigation `{name}`\n{result}")}))
                        .send()
                        .await?
                        .error_for_status()?;
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
