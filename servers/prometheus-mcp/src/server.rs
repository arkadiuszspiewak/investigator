use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool_handler,
};

use crate::{client::PrometheusClient, tools};

#[derive(Clone)]
pub struct PrometheusServer {
    client: PrometheusClient,
    tool_router: ToolRouter<Self>,
}

impl PrometheusServer {
    pub fn new(client: PrometheusClient) -> Self {
        Self {
            client,
            tool_router: tools::router(),
        }
    }

    pub(crate) fn client(&self) -> &PrometheusClient {
        &self.client
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PrometheusServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Use read-only PromQL and discovery tools to investigate metrics and Prometheus target health.",
            )
    }
}
