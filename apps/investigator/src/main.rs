mod controller;
mod error;

use investigator::crd;

use error::AppError;
use kube::{Client, CustomResourceExt};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "investigator=info".into()),
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
    if std::env::args().nth(1).as_deref() == Some("crd") {
        print!(
            "{}",
            serde_json::to_string_pretty(&crd::Investigation::crd())?
        );
        return Ok(());
    }

    controller::run(
        Client::try_default().await?,
        controller::AgentJobConfig::from_env().map_err(AppError::Configuration)?,
    )
    .await;
    Ok(())
}
