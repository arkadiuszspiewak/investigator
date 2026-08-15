mod client;
mod error;
mod server;
mod tools;

use client::AlertmanagerClient;
use error::AppError;
use rmcp::ServiceExt;
use server::AlertmanagerServer;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let server = AlertmanagerServer::new(AlertmanagerClient::from_env()?);
    if std::env::args().any(|argument| argument == "--http") {
        mcp_runtime::serve_http(server).await?;
        Ok(())
    } else {
        let service = server
            .serve((tokio::io::stdin(), tokio::io::stdout()))
            .await?;
        service.waiting().await?;
        Ok(())
    }
}
