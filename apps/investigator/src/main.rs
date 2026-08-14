mod controller;
mod crd;

use kube::{Client, CustomResourceExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("crd") {
        print!(
            "{}",
            serde_json::to_string_pretty(&crd::Investigation::crd())?
        );
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "investigator=info".into()),
        )
        .init();
    controller::run(Client::try_default().await?).await;
    Ok(())
}
