use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool_handler,
};

use crate::{client::AlertmanagerClient, tools};

#[derive(Clone)]
pub struct AlertmanagerServer {
    client: AlertmanagerClient,
    tool_router: ToolRouter<Self>,
}

impl AlertmanagerServer {
    pub fn new(client: AlertmanagerClient) -> Self {
        Self {
            client,
            tool_router: tools::router(),
        }
    }

    pub(crate) fn client(&self) -> &AlertmanagerClient {
        &self.client
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AlertmanagerServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Use read-only tools to inspect active alerts, groups, silences, receivers, and Alertmanager status.",
            )
    }
}
