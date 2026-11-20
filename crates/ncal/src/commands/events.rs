use clap::Subcommand;
use ncal_core::{events as core, Ctx};

use ncal_api::endpoints::GetEventsResult;

use super::context::{authenticated_client, defaults, parse_time_ms, render};
use super::do_sync;
use crate::cache;
use crate::config::AppConfig;
use crate::output::{
    event_sort_key, hint, print_event_detail, print_events, print_json, print_json_lines,
};
use crate::Cli;
use crate::CliError;

#[derive(Subcommand, Debug)]
pub enum EventsCmd {
    /// List events (defaults to upcoming; auto-resolves account and calendar).
    List {
        /// Account email or ID.
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        calendar: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        /// Start time — RFC3339 or epoch millis (default: now).
        #[arg(long)]
        from_time: Option<String>,
        /// End time — RFC3339 or epoch millis.
        #[arg(long)]
        to_time: Option<String>,
        /// Search query string.
        #[arg(long)]
        query: Option<String>,
        /// Max number of events (default: 25).
        #[arg(long, default_value = "25")]
        limit: Option<u32>,
        /// Show all events (not just upcoming).
        #[arg(long)]
        all: bool,
        /// Include deleted/cancelled events.
        #[arg(long)]
        include_deleted: bool,
        /// Force a fresh sync (bypass cache).
        #[arg(long)]
        refresh: bool,
    },
    /// Get a single event by ID.
    Get {
        /// Event ID.
        event_id: String,
        /// Account email or ID.
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        calendar: Option<String>,
        #[arg(long)]
        provider: Option<String>,
    },
    /// Create a new calendar event.
    Create {
        /// Account email or ID.
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        calendar: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        /// Event title.
        #[arg(long)]
        summary: String,
        /// Start time (RFC3339).
        #[arg(long)]
        start: String,
        /// End time (RFC3339).
        #[arg(long)]
        end: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        location: Option<String>,
    },
    /// Update fields on an existing event.
    Update {
        /// Event ID.
        event_id: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        calendar: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        end: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        location: Option<String>,
    },
    /// Delete or cancel an event (soft-cancel by default).
    Delete {
        /// Event ID.
        event_id: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        calendar: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        /// Hard-delete via `deleteEvents` instead of soft-cancel.
        #[arg(long)]
        hard: bool,
    },
}

pub async fn run(cli: &Cli, config: &AppConfig, cmd: &EventsCmd) -> Result<(), CliError> {
    let client = authenticated_client(config)?;
    match cmd {
        EventsCmd::List {
            account,
            calendar,
            provider,
            from_time,
            to_time,
            query,
            limit,
            all,
            include_deleted,
            refresh,
        } => {
            list(
                cli,
                config,
                &client,
                ListEvents {
                    account: account.as_deref(),
                    calendar: calendar.as_deref(),
                    provider: provider.as_deref(),
                    from_time: from_time.as_deref(),
                    to_time: to_time.as_deref(),
                    query: query.as_deref(),
                    limit: *limit,
                    all: *all,
                    include_deleted: *include_deleted,
                    refresh: *refresh,
                },
            )
            .await
        }
        EventsCmd::Get {
            event_id,
            account,
            calendar,
            provider,
        } => {
            get(
                cli,
                config,
                &client,
                event_id,
                account.as_deref(),
                calendar.as_deref(),
                provider.as_deref(),
            )
            .await
        }
        EventsCmd::Create {
            account,
            calendar,
            provider,
            summary,
            start,
            end,
            description,
            location,
        } => {
            create(
                cli,
                config,
                &client,
                CreateEventArgs {
                    account: account.as_deref(),
                    calendar: calendar.as_deref(),
                    provider: provider.as_deref(),
                    summary,
                    start,
                    end,
                    description: description.as_deref(),
                    location: location.as_deref(),
                },
            )
            .await
        }
        EventsCmd::Update {
            event_id,
            account,
            calendar,
            provider,
            summary,
            start,
            end,
            description,
            location,
        } => {
            update(
                cli,
                config,
                &client,
                UpdateEventArgs {
                    event_id,
                    account: account.as_deref(),
                    calendar: calendar.as_deref(),
                    provider: provider.as_deref(),
                    summary: summary.as_deref(),
                    start: start.as_deref(),
                    end: end.as_deref(),
                    description: description.as_deref(),
                    location: location.as_deref(),
                },
            )
            .await
        }
        EventsCmd::Delete {
            event_id,
            account,
            calendar,
            provider,
            hard,
        } => {
            delete(
                cli,
                config,
                &client,
                DeleteEventArgs {
                    event_id,
                    account: account.as_deref(),
                    calendar: calendar.as_deref(),
                    provider: provider.as_deref(),
                    hard: *hard,
                },
            )
            .await
        }
    }
}

struct ListEvents<'a> {
    account: Option<&'a str>,
    calendar: Option<&'a str>,
    provider: Option<&'a str>,
    from_time: Option<&'a str>,
    to_time: Option<&'a str>,
    query: Option<&'a str>,
    limit: Option<u32>,
    all: bool,
    include_deleted: bool,
    refresh: bool,
}

async fn list(
    cli: &Cli,
    config: &AppConfig,
    client: &ncal_api::client::NotionCalendarClient,
    args: ListEvents<'_>,
) -> Result<(), CliError> {
    let time_min = match args.from_time {
        Some(s) => Some(parse_time_ms("from-time", s)?),
        None if args.all => None,
        None => Some(chrono::Utc::now().timestamp_millis()),
    };
    let time_max = args
        .to_time
        .map(|s| parse_time_ms("to-time", s))
        .transpose()?;

    // Try cache first (all-accounts path only, no filters).
    let use_cache = !args.refresh
        && args.account.is_none()
        && args.calendar.is_none()
        && args.provider.is_none()
        && config.defaults.account.is_none()
        && config.defaults.calendar.is_none()
        && args.query.is_none();

    let events = if use_cache {
        if let Some(cached) = cache::read_fresh_cache(config) {
            eprintln!("(using cache)");
            cached.events
        } else {
            eprintln!("(syncing…)");
            do_sync(config, client).await?.events
        }
    } else {
        let ctx = Ctx {
            client,
            defaults: &defaults(config),
        };
        let results = render(
            core::list_events(
                &ctx,
                core::ListEvents {
                    account: args.account.map(str::to_owned),
                    calendar: args.calendar.map(str::to_owned),
                    provider: args.provider.map(str::to_owned),
                    time_min,
                    time_max,
                    query: args.query.map(str::to_owned),
                    limit: args.limit,
                    include_deleted: args.include_deleted,
                },
            )
            .await,
        )?;
        if cli.json {
            return print_json_lines(cli, &results);
        }
        let mut evts = Vec::new();
        for r in &results {
            match r {
                GetEventsResult::Ok(ok) => evts.extend_from_slice(&ok.events),
                GetEventsResult::Err(err) => eprintln!("warning: {}", err.error_message),
            }
        }
        evts
    };

    // Client-side filtering for cached events.
    let filtered: Vec<&ncal_api::types::Event> = events
        .iter()
        .filter(|e| {
            if !args.include_deleted && e.status == Some(ncal_api::types::EventStatus::Cancelled) {
                return false;
            }
            let key = event_sort_key(e);
            if let Some(min) = time_min {
                if key < min {
                    return false;
                }
            }
            if let Some(max) = time_max {
                if key > max {
                    return false;
                }
            }
            true
        })
        .collect();

    // Apply limit.
    let limited: Vec<ncal_api::types::Event> = if let Some(n) = args.limit {
        let mut sorted = filtered;
        sorted.sort_by_key(|e| event_sort_key(e));
        sorted.into_iter().take(n as usize).cloned().collect()
    } else {
        filtered.into_iter().cloned().collect()
    };

    if cli.json {
        return print_json(cli, &limited);
    }
    print_events(cli, &limited)?;
    if limited.is_empty() {
        hint(&[
            "ncal events list --from-time <RFC3339>  — try a different time range",
            "ncal events list --refresh              — force a fresh sync",
            "ncal calendars list                     — check available calendars",
        ]);
    }
    Ok(())
}

async fn get(
    cli: &Cli,
    config: &AppConfig,
    client: &ncal_api::client::NotionCalendarClient,
    event_id: &str,
    account: Option<&str>,
    calendar: Option<&str>,
    provider: Option<&str>,
) -> Result<(), CliError> {
    let ctx = Ctx {
        client,
        defaults: &defaults(config),
    };
    let event = render(
        core::get_event(
            &ctx,
            core::GetEvent {
                event_id: event_id.into(),
                account: account.map(str::to_owned),
                calendar: calendar.map(str::to_owned),
                provider: provider.map(str::to_owned),
            },
        )
        .await,
    )?;
    print_event_detail(cli, &event)?;
    if !cli.json {
        hint(&[
            &format!("ncal events update {event_id} --summary <TEXT>  — update this event"),
            &format!("ncal events delete {event_id}                  — cancel this event"),
        ]);
    }
    Ok(())
}

struct CreateEventArgs<'a> {
    account: Option<&'a str>,
    calendar: Option<&'a str>,
    provider: Option<&'a str>,
    summary: &'a str,
    start: &'a str,
    end: &'a str,
    description: Option<&'a str>,
    location: Option<&'a str>,
}

async fn create(
    cli: &Cli,
    config: &AppConfig,
    client: &ncal_api::client::NotionCalendarClient,
    args: CreateEventArgs<'_>,
) -> Result<(), CliError> {
    let ctx = Ctx {
        client,
        defaults: &defaults(config),
    };
    let event = render(
        core::create_event(
            &ctx,
            core::CreateEvent {
                account: args.account.map(str::to_owned),
                calendar: args.calendar.map(str::to_owned),
                provider: args.provider.map(str::to_owned),
                summary: args.summary.into(),
                start: args.start.into(),
                end: args.end.into(),
                description: args.description.map(str::to_owned),
                location: args.location.map(str::to_owned),
            },
        )
        .await,
    )?;
    print_event_detail(cli, &event)?;
    if !cli.json {
        eprintln!("\nCreated.");
        hint(&[
            &format!("ncal events get {}  — view this event", event.id),
            "ncal events list                  — list events",
        ]);
    }
    Ok(())
}

struct UpdateEventArgs<'a> {
    event_id: &'a str,
    account: Option<&'a str>,
    calendar: Option<&'a str>,
    provider: Option<&'a str>,
    summary: Option<&'a str>,
    start: Option<&'a str>,
    end: Option<&'a str>,
    description: Option<&'a str>,
    location: Option<&'a str>,
}

async fn update(
    cli: &Cli,
    config: &AppConfig,
    client: &ncal_api::client::NotionCalendarClient,
    args: UpdateEventArgs<'_>,
) -> Result<(), CliError> {
    let ctx = Ctx {
        client,
        defaults: &defaults(config),
    };
    let resp = render(
        core::update_event(
            &ctx,
            core::UpdateEvent {
                event_id: args.event_id.into(),
                account: args.account.map(str::to_owned),
                calendar: args.calendar.map(str::to_owned),
                provider: args.provider.map(str::to_owned),
                summary: args.summary.map(str::to_owned),
                start: args.start.map(str::to_owned),
                end: args.end.map(str::to_owned),
                description: args.description.map(str::to_owned),
                location: args.location.map(str::to_owned),
            },
        )
        .await,
    )?;
    if cli.json {
        return print_json(cli, &resp);
    }
    for entry in &resp {
        if let Some(e) = &entry.event {
            print_event_detail(cli, e)?;
        }
        if let Some(err) = &entry.error_message {
            eprintln!("error: {err}");
        }
    }
    eprintln!("\nUpdated.");
    Ok(())
}

struct DeleteEventArgs<'a> {
    event_id: &'a str,
    account: Option<&'a str>,
    calendar: Option<&'a str>,
    provider: Option<&'a str>,
    hard: bool,
}

async fn delete(
    cli: &Cli,
    config: &AppConfig,
    client: &ncal_api::client::NotionCalendarClient,
    args: DeleteEventArgs<'_>,
) -> Result<(), CliError> {
    let ctx = Ctx {
        client,
        defaults: &defaults(config),
    };
    let resp = render(
        core::delete_event(
            &ctx,
            core::DeleteEvent {
                event_id: args.event_id.into(),
                account: args.account.map(str::to_owned),
                calendar: args.calendar.map(str::to_owned),
                provider: args.provider.map(str::to_owned),
                hard: args.hard,
            },
        )
        .await,
    )?;
    if cli.json {
        return print_json(cli, &resp);
    }
    if args.hard {
        eprintln!("Deleted (hard).");
        return Ok(());
    }
    eprintln!("Cancelled.");
    hint(&["ncal events list  — verify the cancellation"]);
    Ok(())
}
