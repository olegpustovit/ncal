use ncal_api::client::NotionCalendarClient;
use ncal_api::endpoints::{
    get_calendar_lists, get_user, CalendarListQuery, CalendarListResult, GetCalendarListsRequest,
};
use ncal_api::types::{Account, Calendar, Provider};

use ncal_api::error::ApiError;

#[derive(Clone, Debug, Default)]
pub struct Defaults {
    pub account: Option<String>,
    pub calendar: Option<String>,
    pub timezone: Option<String>,
}

pub struct Ctx<'a> {
    pub client: &'a NotionCalendarClient,
    pub defaults: &'a Defaults,
}

pub type CalendarTarget = (String, String, Provider);

pub struct Resolved<T> {
    pub value: T,
    pub notes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0}")]
    Selection(String),
    #[error(transparent)]
    Api(Box<ApiError>),
}

impl From<ApiError> for CoreError {
    fn from(error: ApiError) -> Self {
        Self::Api(Box::new(error))
    }
}

/// Extract successful calendars from batch results, collecting warnings for the caller.
pub fn flatten_calendar_results(
    results: Vec<CalendarListResult>,
    notes: &mut Vec<String>,
) -> Vec<Calendar> {
    results
        .into_iter()
        .flat_map(|r| match r {
            CalendarListResult::Ok(ok) => ok.calendars,
            CalendarListResult::Err(err) => {
                notes.push(format!(
                    "warning: calendar list error: {}",
                    err.error_message
                ));
                vec![]
            }
        })
        .collect()
}

/// Resolve account + calendar + provider from explicit selections and reusable defaults.
pub async fn resolve_context(
    ctx: &Ctx<'_>,
    account: Option<&str>,
    calendar: Option<&str>,
    provider: Option<&str>,
) -> Result<Resolved<CalendarTarget>, CoreError> {
    let mut notes = Vec::new();
    let account_hint = account.or(ctx.defaults.account.as_deref());
    let (account_id, resolved_provider) =
        resolve_account(ctx.client, account_hint, &mut notes).await?;
    let prov = provider
        .map(str::parse)
        .map(|result| result.map_err(CoreError::Selection))
        .transpose()?
        .unwrap_or(resolved_provider);
    let calendar_id = match calendar.or(ctx.defaults.calendar.as_deref()) {
        Some(c) => c.to_string(),
        None => auto_resolve_calendar(ctx.client, &account_id, prov, &mut notes).await?,
    };
    Ok(Resolved {
        value: (account_id, calendar_id, prov),
        notes,
    })
}

async fn resolve_account(
    client: &NotionCalendarClient,
    hint: Option<&str>,
    notes: &mut Vec<String>,
) -> Result<(String, Provider), CoreError> {
    let user = get_user(client).await.map_err(CoreError::from)?;
    let accounts = user.accounts.unwrap_or_default();

    if let Some(q) = hint {
        let q_lower = q.to_ascii_lowercase();
        if let Some(found) = accounts.iter().find(|a| {
            a.id == q
                || a.email
                    .as_deref()
                    .is_some_and(|e| e.to_ascii_lowercase() == q_lower)
        }) {
            return Ok((
                found.id.clone(),
                found.provider_name.unwrap_or(Provider::Google),
            ));
        }
        return Err(CoreError::Selection(format!(
            "no account matching {:?}; available accounts:\n{}",
            q,
            format_account_list(&accounts),
        )));
    }

    match accounts.as_slice() {
        [] => Err(CoreError::Selection(
            "no accounts connected; add one in the Notion Calendar app".into(),
        )),
        [one] => {
            notes.push(format!(
                "auto: using account {} ({})",
                one.email.as_deref().unwrap_or("?"),
                one.provider_name.map(|p| p.to_string()).unwrap_or_default()
            ));
            Ok((one.id.clone(), one.provider_name.unwrap_or(Provider::Google)))
        }
        many => Err(CoreError::Selection(format!(
            "multiple accounts found; specify --account <EMAIL>:\n{}\nor set defaults.account in your config file",
            format_account_list(many),
        ))),
    }
}

/// Build one (account, calendar, provider) tuple per calendar across all accounts.
/// Fetches all calendars in a single batch RPC. Used by `events list` when no
/// `--account` is specified — queries every calendar the user has.
pub async fn build_all_account_queries(
    client: &NotionCalendarClient,
) -> Result<Resolved<Vec<CalendarTarget>>, CoreError> {
    let user = get_user(client).await.map_err(CoreError::from)?;
    let accounts = user.accounts.unwrap_or_default();
    if accounts.is_empty() {
        return Err(CoreError::Selection(
            "no accounts connected; add one in the Notion Calendar app".into(),
        ));
    }
    let queries: Vec<CalendarListQuery> = accounts
        .iter()
        .filter_map(|a| {
            Some(CalendarListQuery {
                provider: a.provider_name?,
                account_id: a.id.clone(),
            })
        })
        .collect();
    let results = get_calendar_lists(client, &GetCalendarListsRequest { queries })
        .await
        .map_err(CoreError::from)?;
    let mut notes = Vec::new();
    let calendars = flatten_calendar_results(results, &mut notes);
    if calendars.is_empty() {
        return Err(CoreError::Selection(
            "no calendars found for any account".into(),
        ));
    }
    let value = calendars
        .into_iter()
        .filter_map(|c| Some((c.account_id?, c.id, c.provider?)))
        .collect();
    Ok(Resolved { value, notes })
}

fn format_account_list(accounts: &[Account]) -> String {
    accounts
        .iter()
        .map(|a| {
            format!(
                "  --account {}  # {} ({})",
                a.email.as_deref().unwrap_or(&a.id),
                a.display_name.as_deref().unwrap_or(""),
                a.provider_name.map(|p| p.to_string()).unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn auto_resolve_calendar(
    client: &NotionCalendarClient,
    account_id: &str,
    provider: Provider,
    notes: &mut Vec<String>,
) -> Result<String, CoreError> {
    let req = GetCalendarListsRequest {
        queries: vec![CalendarListQuery {
            provider,
            account_id: account_id.to_string(),
        }],
    };
    let results = get_calendar_lists(client, &req)
        .await
        .map_err(CoreError::from)?;
    let calendars = flatten_calendar_results(results, notes);

    if let Some(primary) = calendars.iter().find(|c| c.primary == Some(true)) {
        notes.push(format!(
            "auto: using calendar {:?} (primary)",
            primary.summary.as_deref().unwrap_or(&primary.id)
        ));
        return Ok(primary.id.clone());
    }

    match calendars.as_slice() {
        [] => Err(CoreError::Selection(
            "no calendars found for this account".into(),
        )),
        [one] => {
            notes.push(format!(
                "auto: using calendar {:?}",
                one.summary.as_deref().unwrap_or(&one.id)
            ));
            Ok(one.id.clone())
        }
        many => {
            let mut msg = String::from("multiple calendars found; specify --calendar <ID>:\n");
            for c in many {
                msg.push_str(&format!(
                    "  --calendar {}  # {}{}\n",
                    c.id,
                    c.summary.as_deref().unwrap_or(""),
                    if c.primary == Some(true) {
                        " (primary)"
                    } else {
                        ""
                    },
                ));
            }
            msg.push_str("\nor set defaults.calendar in your config file");
            Err(CoreError::Selection(msg))
        }
    }
}
