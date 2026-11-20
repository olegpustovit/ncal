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

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidSelection,
    AccountNotFound,
    AccountRequired,
    CalendarRequired,
    InvalidToken,
    RefreshCooldown,
    CalendarApiError,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hint {
    SelectAccount,
    SelectCalendar,
    RefreshSession,
    RetryLater,
}

#[derive(Debug, serde::Serialize)]
pub struct Candidate {
    pub id: String,
    pub label: Option<String>,
    pub email: Option<String>,
    pub provider: Option<Provider>,
    pub primary: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct CoreError {
    pub code: ErrorCode,
    pub message: String,
    pub hint: Option<Hint>,
    pub candidates: Vec<Candidate>,
    #[source]
    pub upstream: Option<Box<ApiError>>,
}

impl CoreError {
    pub fn selection(message: String) -> Self {
        Self {
            code: ErrorCode::InvalidSelection,
            message,
            hint: None,
            candidates: vec![],
            upstream: None,
        }
    }

    fn accounts(code: ErrorCode, message: String, accounts: &[Account]) -> Self {
        Self {
            code,
            message,
            hint: Some(Hint::SelectAccount),
            upstream: None,
            candidates: accounts
                .iter()
                .map(|account| Candidate {
                    id: account.id.clone(),
                    label: account.display_name.clone(),
                    email: account.email.clone(),
                    provider: account.provider_name,
                    primary: account.primary.unwrap_or(false),
                })
                .collect(),
        }
    }

    fn calendars(calendars: &[Calendar]) -> Self {
        Self {
            code: ErrorCode::CalendarRequired,
            message: "multiple calendars found".into(),
            hint: Some(Hint::SelectCalendar),
            upstream: None,
            candidates: calendars
                .iter()
                .map(|calendar| Candidate {
                    id: calendar.id.clone(),
                    label: calendar.summary.clone(),
                    email: None,
                    provider: calendar.provider,
                    primary: calendar.primary.unwrap_or(false),
                })
                .collect(),
        }
    }
}

impl From<ApiError> for CoreError {
    fn from(error: ApiError) -> Self {
        let (code, hint, message) = match error {
            ApiError::InvalidToken => (
                ErrorCode::InvalidToken,
                Some(Hint::RefreshSession),
                "Calendar session has expired",
            ),
            ApiError::RefreshCooldown => (
                ErrorCode::RefreshCooldown,
                Some(Hint::RetryLater),
                "Wait 30 seconds before retrying",
            ),
            _ => (ErrorCode::CalendarApiError, None, "Calendar request failed"),
        };
        Self {
            code,
            message: message.into(),
            hint,
            candidates: vec![],
            upstream: Some(Box::new(error)),
        }
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
        .map(|result| result.map_err(CoreError::selection))
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
        return Err(CoreError::accounts(
            ErrorCode::AccountNotFound,
            format!("no account matching {q:?}"),
            &accounts,
        ));
    }

    match accounts.as_slice() {
        [] => Err(CoreError::selection(
            "no accounts connected; add one in the Notion Calendar app".into(),
        )),
        [one] => {
            notes.push(format!(
                "auto: using account {} ({})",
                one.email.as_deref().unwrap_or("?"),
                one.provider_name.map(|p| p.to_string()).unwrap_or_default()
            ));
            Ok((
                one.id.clone(),
                one.provider_name.unwrap_or(Provider::Google),
            ))
        }
        many => Err(CoreError::accounts(
            ErrorCode::AccountRequired,
            "multiple accounts found".into(),
            many,
        )),
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
        return Err(CoreError::selection(
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
        return Err(CoreError::selection(
            "no calendars found for any account".into(),
        ));
    }
    let value = calendars
        .into_iter()
        .filter_map(|c| Some((c.account_id?, c.id, c.provider?)))
        .collect();
    Ok(Resolved { value, notes })
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
        [] => Err(CoreError::selection(
            "no calendars found for this account".into(),
        )),
        [one] => {
            notes.push(format!(
                "auto: using calendar {:?}",
                one.summary.as_deref().unwrap_or(&one.id)
            ));
            Ok(one.id.clone())
        }
        many => Err(CoreError::calendars(many)),
    }
}
