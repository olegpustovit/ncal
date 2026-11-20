use ncal_api::client::NotionCalendarClient;
use ncal_core::Defaults;
use ncal_mcp::{ClientSource, NcalMcp, ToolFailure, ToolPolicy};
use rmcp::{service::RequestContext, RoleServer, ServiceExt};

use super::{auth, context};
use crate::{config::AppConfig, CliError};

struct LocalSource {
    client: NotionCalendarClient,
    defaults: Defaults,
}

impl ClientSource for LocalSource {
    async fn client(
        &self,
        _request: &RequestContext<RoleServer>,
    ) -> Result<(NotionCalendarClient, Defaults), ToolFailure> {
        Ok((self.client.clone(), self.defaults.clone()))
    }

    fn relink_hint(&self) -> Option<&str> {
        Some("ncal auth refresh")
    }
}

pub async fn run(config: &AppConfig, read_only: bool) -> Result<(), CliError> {
    let client = auth::build_client(config, auth::resolve_credentials(config)?)?;
    let source = LocalSource {
        client,
        defaults: context::defaults(config),
    };
    let policy = ToolPolicy {
        write: !read_only,
        destructive: !read_only,
    };
    let server = NcalMcp::new(source, policy)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(std::io::Error::other)?;
    server.waiting().await.map_err(std::io::Error::other)?;
    Ok(())
}
