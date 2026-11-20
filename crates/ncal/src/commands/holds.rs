use clap::Subcommand;

use super::context::{authenticated_client, defaults};
use crate::config::AppConfig;
use crate::output::{hint, print_hold_detail, print_holds, print_json};
use crate::Cli;
use crate::CliError;
use ncal_api::endpoints::{CheckAliasRequest, DeleteHoldRequest};
use ncal_core::holds::{
    create_hold, delete_hold, find_hold, get_hold_alias_available, get_holds, CreateHoldArgs,
};

#[derive(Subcommand, Debug)]
pub enum HoldsCmd {
    /// List all scheduling holds.
    List,
    /// Show details of a hold by ID.
    Get {
        /// Hold ID.
        hold_id: String,
    },
    /// Create a new scheduling hold (availability block).
    Create {
        /// Hold title.
        #[arg(long)]
        title: String,
        /// URL-safe alias for the scheduling link.
        #[arg(long)]
        alias: String,
        /// Duration in minutes.
        #[arg(long)]
        duration: u32,
        /// IANA timezone (default: from config or UTC).
        #[arg(long)]
        timezone: Option<String>,
        /// Destination account ID (googleAccountId field).
        #[arg(long)]
        account: String,
        /// Destination calendar ID (googleCalendarId field).
        #[arg(long)]
        calendar: String,
        /// Hold type (default: recurring).
        #[arg(long, default_value = "recurring")]
        hold_type: String,
        /// Description.
        #[arg(long)]
        description: Option<String>,
        /// Minimum lead time in minutes.
        #[arg(long)]
        min_lead_time: Option<u64>,
        /// Maximum lead time in minutes.
        #[arg(long)]
        max_lead_time: Option<u64>,
    },
    /// Delete a scheduling hold.
    Delete {
        /// Hold ID.
        hold_id: String,
    },
    /// Check if a scheduling link alias is available.
    CheckAlias {
        /// The alias to check.
        alias: String,
    },
}

pub async fn run(cli: &Cli, config: &AppConfig, cmd: &HoldsCmd) -> Result<(), CliError> {
    let client = authenticated_client(config)?;
    match cmd {
        HoldsCmd::List => list(cli, &client).await,
        HoldsCmd::Get { hold_id } => get(cli, &client, hold_id).await,
        HoldsCmd::Create {
            title,
            alias,
            duration,
            timezone,
            account,
            calendar,
            hold_type,
            description,
            min_lead_time,
            max_lead_time,
        } => {
            create(
                cli,
                config,
                &client,
                CreateHoldArgs {
                    title,
                    alias,
                    duration: *duration,
                    timezone: timezone.as_deref(),
                    account,
                    calendar,
                    hold_type,
                    description: description.as_deref(),
                    min_lead_time: *min_lead_time,
                    max_lead_time: *max_lead_time,
                },
            )
            .await
        }
        HoldsCmd::Delete { hold_id } => delete(cli, &client, hold_id).await,
        HoldsCmd::CheckAlias { alias } => check_alias(cli, &client, alias).await,
    }
}

async fn list(cli: &Cli, client: &ncal_api::client::NotionCalendarClient) -> Result<(), CliError> {
    let resp = get_holds(client).await.map_err(CliError::Api)?;
    if cli.json {
        return print_json(cli, &resp.holds);
    }
    print_holds(cli, &resp.holds)?;
    hint(&[
        "ncal holds get <ID>              — view hold details",
        "ncal holds create --title <T> …  — create a new hold",
    ]);
    Ok(())
}

async fn get(
    cli: &Cli,
    client: &ncal_api::client::NotionCalendarClient,
    hold_id: &str,
) -> Result<(), CliError> {
    let hold = find_hold(client, hold_id).await?;
    print_hold_detail(cli, &hold)?;
    if !cli.json {
        hint(&[
            &format!("ncal holds delete {hold_id}  — delete this hold"),
            "ncal holds list              — list all holds",
        ]);
    }
    Ok(())
}

async fn create(
    cli: &Cli,
    config: &AppConfig,
    client: &ncal_api::client::NotionCalendarClient,
    args: CreateHoldArgs<'_>,
) -> Result<(), CliError> {
    let context = ncal_core::Ctx {
        client,
        defaults: &defaults(config),
    };
    let (id, resp) = create_hold(&context, args).await?;
    if cli.json {
        return print_json(
            cli,
            &serde_json::json!({ "id": id, "success": resp.success }),
        );
    }
    eprintln!("Created hold {id}.");
    hint(&[
        &format!("ncal holds get {id}  — view the hold"),
        "ncal holds list           — list all holds",
    ]);
    Ok(())
}

async fn delete(
    cli: &Cli,
    client: &ncal_api::client::NotionCalendarClient,
    hold_id: &str,
) -> Result<(), CliError> {
    let req = DeleteHoldRequest {
        hold_id: hold_id.to_string(),
    };
    let resp = delete_hold(client, &req).await.map_err(CliError::Api)?;
    if cli.json {
        return print_json(cli, &resp);
    }
    eprintln!("Deleted hold {hold_id}.");
    Ok(())
}

async fn check_alias(
    cli: &Cli,
    client: &ncal_api::client::NotionCalendarClient,
    alias: &str,
) -> Result<(), CliError> {
    let req = CheckAliasRequest {
        alias: alias.to_string(),
    };
    let resp = get_hold_alias_available(client, &req)
        .await
        .map_err(CliError::Api)?;
    if cli.json {
        return print_json(cli, &resp);
    }
    if resp.available {
        eprintln!("Alias {alias:?} is available.");
    } else {
        eprintln!("Alias {alias:?} is taken.");
    }
    Ok(())
}
