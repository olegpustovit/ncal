use crate::CoreError;
use ncal_api::{
    client::NotionCalendarClient,
    endpoints::{
        get_calendar_lists, get_user, CalendarListQuery, CalendarListResult,
        GetCalendarListsRequest,
    },
};

pub async fn list_calendars(
    client: &NotionCalendarClient,
    account: Option<&str>,
    provider: Option<&str>,
) -> Result<Vec<CalendarListResult>, CoreError> {
    let user = get_user(client).await?;
    let accounts = user.accounts.as_ref().ok_or_else(|| {
        CoreError::Selection("no accounts connected; add one in the Notion Calendar app".into())
    })?;

    let mut queries = Vec::new();
    for acct in accounts {
        if let Some(f) = account {
            if acct.id != f {
                continue;
            }
        }
        let pn = acct
            .provider_name
            .ok_or_else(|| CoreError::Selection(format!("account {} has no provider", acct.id)))?;
        let prov = provider
            .map(|value| value.parse().map_err(CoreError::Selection))
            .transpose()?
            .unwrap_or(pn);
        queries.push(CalendarListQuery {
            provider: prov,
            account_id: acct.id.clone(),
        });
    }

    if queries.is_empty() {
        return Err(CoreError::Selection(
            "no calendar accounts matched (check --account or sign in)".into(),
        ));
    }

    Ok(get_calendar_lists(client, &GetCalendarListsRequest { queries }).await?)
}
