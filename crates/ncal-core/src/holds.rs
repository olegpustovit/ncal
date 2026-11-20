use ncal_api::{
    client::NotionCalendarClient,
    endpoints::{self as rpc, CreateHoldRequest, CreateHoldResponse},
    types::{HoldGroup, TimeRange},
};

use crate::{CoreError, Ctx};
pub use rpc::{delete_hold, get_hold_alias_available, get_holds};

pub async fn find_hold(
    client: &NotionCalendarClient,
    hold_id: &str,
) -> Result<HoldGroup, CoreError> {
    get_holds(client)
        .await?
        .holds
        .into_iter()
        .find(|hold| hold.id == hold_id)
        .ok_or_else(|| {
            CoreError::selection(format!(
                "no hold with ID {hold_id:?}; use `ncal holds list` to see all"
            ))
        })
}

pub struct CreateHoldArgs<'a> {
    pub title: &'a str,
    pub alias: &'a str,
    pub duration: u32,
    pub timezone: Option<&'a str>,
    pub account: &'a str,
    pub calendar: &'a str,
    pub hold_type: &'a str,
    pub description: Option<&'a str>,
    pub min_lead_time: Option<u64>,
    pub max_lead_time: Option<u64>,
}

pub async fn create_hold(
    ctx: &Ctx<'_>,
    args: CreateHoldArgs<'_>,
) -> Result<(String, CreateHoldResponse), CoreError> {
    let tz = args
        .timezone
        .map(String::from)
        .or_else(|| ctx.defaults.timezone.clone())
        .unwrap_or_else(|| "UTC".to_string());
    let id = uuid::Uuid::new_v4().to_string();
    let req = CreateHoldRequest {
        id: id.clone(),
        alias: args.alias.to_string(),
        hold_type: args.hold_type.to_string(),
        status: "activeBookable".to_string(),
        user_primary_time_zone: tz.clone(),
        time_zone: tz,
        duration: Some(args.duration),
        time_ranges: Some(Vec::<TimeRange>::new()),
        title: args.title.to_string(),
        description: args.description.map(String::from),
        conferencing_provider_name: None,
        conferencing_account_id: None,
        google_account_id: args.account.to_string(),
        google_calendar_id: args.calendar.to_string(),
        conflict_free_resources: None,
        min_lead_time: args.min_lead_time,
        max_lead_time: args.max_lead_time,
        expiration_date: None,
    };
    let resp = rpc::create_hold(ctx.client, &req).await?;
    Ok((id, resp))
}
