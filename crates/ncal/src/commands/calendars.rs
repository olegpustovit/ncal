use clap::Subcommand;

use ncal_core::calendar_colors;

use super::context::{authenticated_client, flatten_calendar_results};
use crate::config::AppConfig;
use crate::output::{hint, print_calendars, print_json};
use crate::Cli;
use crate::CliError;

#[derive(Subcommand, Debug)]
pub enum CalCmd {
    /// List calendars for all connected accounts (or filter with --account).
    List {
        /// Filter to a specific account ID.
        #[arg(long)]
        account: Option<String>,
        /// Override provider (default: from account).
        #[arg(long)]
        provider: Option<String>,
    },
    /// Show the named color palette.
    Colors,
}

pub async fn run(cli: &Cli, config: &AppConfig, cmd: &CalCmd) -> Result<(), CliError> {
    let client = authenticated_client(config)?;
    match cmd {
        CalCmd::List { account, provider } => {
            let results =
                ncal_core::list_calendars(&client, account.as_deref(), provider.as_deref()).await?;
            if cli.json {
                return print_json(cli, &results);
            }

            let calendars = flatten_calendar_results(results);
            print_calendars(cli, &calendars)?;
            hint(&[
                "ncal events list --calendar <ID>  — list events in a calendar",
                "ncal calendars colors             — show the color palette",
            ]);
            Ok(())
        }
        CalCmd::Colors => print_json(cli, &calendar_colors(&client).await?),
    }
}
