use kube::Client;
use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool_handler,
};

use crate::tools;

#[derive(Clone)]
pub struct KubernetesServer {
    client: Client,
    tool_router: ToolRouter<Self>,
}

impl KubernetesServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            tool_router: tools::router(),
        }
    }

    pub(crate) fn client(&self) -> Client {
        self.client.clone()
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for KubernetesServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions("Use the available tools to inspect Kubernetes resources.")
    }
}
