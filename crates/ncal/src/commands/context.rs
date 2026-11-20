use ncal_api::client::NotionCalendarClient;
use ncal_api::endpoints::CalendarListResult;
use ncal_api::types::{Calendar, Provider};
use ncal_core::{CalendarTarget, CoreError, Ctx, Defaults, Resolved};

use super::auth::{build_client, resolve_credentials};
use crate::config::AppConfig;
use crate::CliError;

/// Build an authenticated client (resolve credentials + construct client) in one step.
pub fn authenticated_client(config: &AppConfig) -> Result<NotionCalendarClient, CliError> {
    build_client(config, resolve_credentials(config)?)
}

/// Parse a provider string (case-insensitive) into a `Provider`.
pub(super) fn parse_provider(s: &str) -> Result<Provider, CliError> {
    s.parse().map_err(CliError::Usage)
}

/// Parse an RFC3339 or epoch-millis string into epoch millis.
pub fn parse_time_ms(label: &str, s: &str) -> Result<i64, CliError> {
    if let Ok(ms) = s.parse::<i64>() {
        return Ok(ms);
    }
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.timestamp_millis())
        .map_err(|_| {
            CliError::Usage(format!(
                "--{label}: expected epoch millis or RFC3339, got {s:?}"
            ))
        })
}

pub fn flatten_calendar_results(results: Vec<CalendarListResult>) -> Vec<Calendar> {
    let mut notes = Vec::new();
    let calendars = ncal_core::flatten_calendar_results(results, &mut notes);
    render_notes(Resolved {
        value: calendars,
        notes,
    })
}

pub async fn resolve_context(
    config: &AppConfig,
    client: &NotionCalendarClient,
    account: Option<&str>,
    calendar: Option<&str>,
    provider: Option<&str>,
) -> Result<CalendarTarget, CliError> {
    let defaults = Defaults {
        account: config.defaults.account.clone(),
        calendar: config.defaults.calendar.clone(),
    };
    render(
        ncal_core::resolve_context(
            &Ctx {
                client,
                defaults: &defaults,
            },
            account,
            calendar,
            provider,
        )
        .await,
    )
}

pub async fn build_all_account_queries(
    client: &NotionCalendarClient,
) -> Result<Vec<CalendarTarget>, CliError> {
    render(ncal_core::build_all_account_queries(client).await)
}

fn render<T>(result: Result<Resolved<T>, CoreError>) -> Result<T, CliError> {
    let resolved = result.map_err(|error| match error {
        CoreError::Selection(message) => CliError::Usage(message),
        CoreError::Api(error) => CliError::Api(error),
    })?;
    Ok(render_notes(resolved))
}

fn render_notes<T>(resolved: Resolved<T>) -> T {
    for note in resolved.notes {
        eprintln!("{note}");
    }
    resolved.value
}
