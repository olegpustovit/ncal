use ncal_core::contacts_list;

use super::context::authenticated_client;
use crate::config::AppConfig;
use crate::output::print_contacts;
use crate::Cli;
use crate::CliError;

pub async fn run(cli: &Cli, config: &AppConfig) -> Result<(), CliError> {
    let client = authenticated_client(config)?;
    let rows = contacts_list(&client).await.map_err(CliError::Api)?;
    print_contacts(cli, &rows)
}
