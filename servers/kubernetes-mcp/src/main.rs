mod error;
mod server;
mod tools;

use error::AppError;
use kube::Client;
use rmcp::ServiceExt;
use server::KubernetesServer;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let client = Client::try_default().await?;
    let server = KubernetesServer::new(client);
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
