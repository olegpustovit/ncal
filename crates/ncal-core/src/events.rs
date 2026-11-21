use ncal_api::endpoints::{
    self as rpc, CreateEventMutation, CreateEventRequest, DeleteEventMutation, DeleteEventsRequest,
    GetEventQuery, GetEventRequest, UpdateEventEntry, UpdateEventMutation, UpdateEventsRequest,
};
use ncal_api::types::Event;
use rpc::{EventQuery, GetEventsRequest, GetEventsResult};

use crate::{build_all_account_queries, resolve_context, CoreError, Ctx, Resolved};

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetEvent {
    pub event_id: String,
    pub account: Option<String>,
    pub calendar: Option<String>,
    pub provider: Option<String>,
}

pub async fn get_event(ctx: &Ctx<'_>, args: GetEvent) -> Result<Resolved<Event>, CoreError> {
    let Resolved {
        value: (account_id, calendar_id, provider),
        notes,
    } = resolve_context(
        ctx,
        args.account.as_deref(),
        args.calendar.as_deref(),
        args.provider.as_deref(),
    )
    .await?;
    let event = get_event_at(
        ctx.client,
        (account_id, calendar_id, provider),
        args.event_id,
        ctx.defaults.timezone.clone(),
    )
    .await?;
    Ok(Resolved {
        value: event,
        notes,
    })
}

/// Fetch an event using a fully resolved calendar target.
pub async fn get_event_at(
    client: &ncal_api::client::NotionCalendarClient,
    (account_id, calendar_id, provider): crate::CalendarTarget,
    event_id: String,
    user_time_zone: Option<String>,
) -> Result<Event, CoreError> {
    let req = GetEventRequest {
        query: GetEventQuery {
            provider,
            account_id,
            calendar_id,
            event_id,
            user_time_zone,
        },
    };
    Ok(rpc::get_event(client, &req).await?.event)
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateEvent {
    pub account: Option<String>,
    pub calendar: Option<String>,
    pub provider: Option<String>,
    pub summary: String,
    pub start: String,
    pub end: String,
    pub description: Option<String>,
    pub location: Option<String>,
}

pub async fn create_event(ctx: &Ctx<'_>, args: CreateEvent) -> Result<Resolved<Event>, CoreError> {
    create_event_guarded(ctx, args, |_| std::future::ready(Ok(()))).await
}

pub async fn create_event_guarded<F, Fut, E>(
    ctx: &Ctx<'_>,
    args: CreateEvent,
    authorize: F,
) -> Result<Resolved<Event>, E>
where
    F: FnOnce(crate::CalendarTarget) -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
    E: From<CoreError>,
{
    create_event_checked(ctx, args, |target, _| authorize(target)).await
}

pub async fn create_event_checked<F, Fut, E>(
    ctx: &Ctx<'_>,
    args: CreateEvent,
    authorize: F,
) -> Result<Resolved<Event>, E>
where
    F: FnOnce(crate::CalendarTarget, Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
    E: From<CoreError>,
{
    let Resolved {
        value: (acct, cal, prov),
        notes,
    } = resolve_context(
        ctx,
        args.account.as_deref(),
        args.calendar.as_deref(),
        args.provider.as_deref(),
    )
    .await?;
    authorize((acct.clone(), cal.clone(), prov), None).await?;
    let tz = ctx.defaults.timezone.as_deref();
    let mut event_data = serde_json::Map::from_iter([
        ("summary".into(), serde_json::json!(args.summary)),
        ("start".into(), event_date_json(&args.start, tz)),
        ("end".into(), event_date_json(&args.end, tz)),
    ]);
    if let Some(d) = args.description {
        event_data.insert("description".into(), serde_json::json!(d));
    }
    if let Some(l) = args.location {
        event_data.insert("location".into(), serde_json::json!(l));
    }
    let req = CreateEventRequest {
        mutation: CreateEventMutation {
            provider: prov,
            account_id: acct,
            calendar_id: cal,
            event_data: serde_json::Value::Object(event_data),
            send_updates: None,
        },
    };
    let mut event = rpc::create_event(ctx.client, &req)
        .await
        .map_err(CoreError::from)?;
    event.account_id.get_or_insert(req.mutation.account_id);
    event.calendar_id.get_or_insert(req.mutation.calendar_id);
    Ok(Resolved {
        value: event,
        notes,
    })
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateEvent {
    pub event_id: String,
    pub account: Option<String>,
    pub calendar: Option<String>,
    pub provider: Option<String>,
    pub summary: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
}

pub async fn update_event(
    ctx: &Ctx<'_>,
    args: UpdateEvent,
) -> Result<Resolved<Vec<UpdateEventEntry>>, CoreError> {
    update_event_guarded(ctx, args, |_| std::future::ready(Ok(()))).await
}

pub async fn update_event_guarded<F, Fut, E>(
    ctx: &Ctx<'_>,
    args: UpdateEvent,
    authorize: F,
) -> Result<Resolved<Vec<UpdateEventEntry>>, E>
where
    F: FnOnce(crate::CalendarTarget) -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
    E: From<CoreError>,
{
    update_event_checked(ctx, args, |target, _| authorize(target)).await
}

pub async fn update_event_checked<F, Fut, E>(
    ctx: &Ctx<'_>,
    args: UpdateEvent,
    authorize: F,
) -> Result<Resolved<Vec<UpdateEventEntry>>, E>
where
    F: FnOnce(crate::CalendarTarget, Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
    E: From<CoreError>,
{
    let Resolved {
        value: (acct, cal, prov),
        notes,
    } = resolve_context(
        ctx,
        args.account.as_deref(),
        args.calendar.as_deref(),
        args.provider.as_deref(),
    )
    .await?;
    authorize(
        (acct.clone(), cal.clone(), prov),
        Some(args.event_id.clone()),
    )
    .await?;
    let tz = ctx.defaults.timezone.as_deref();
    let mut partial = serde_json::Map::new();
    if let Some(s) = args.summary {
        partial.insert("summary".into(), serde_json::json!(s));
    }
    if let Some(s) = args.start {
        partial.insert("start".into(), event_date_json(&s, tz));
    }
    if let Some(s) = args.end {
        partial.insert("end".into(), event_date_json(&s, tz));
    }
    if let Some(d) = args.description {
        partial.insert("description".into(), serde_json::json!(d));
    }
    if let Some(l) = args.location {
        partial.insert("location".into(), serde_json::json!(l));
    }
    if partial.is_empty() {
        return Err(CoreError::selection(
            "provide at least one of --summary --start --end --description --location".into(),
        )
        .into());
    }
    let req = UpdateEventsRequest {
        mutations: vec![UpdateEventMutation {
            provider: prov,
            account_id: acct,
            event_id: args.event_id,
            calendar_id: cal,
            event_data: serde_json::Value::Object(partial),
            user_time_zone: ctx.defaults.timezone.clone(),
            send_updates: None,
        }],
    };
    let mut resp = rpc::update_events(ctx.client, &req)
        .await
        .map_err(CoreError::from)?;
    if let [entry] = resp.as_mut_slice() {
        if let Some(event) = &mut entry.event {
            event
                .account_id
                .get_or_insert_with(|| req.mutations[0].account_id.clone());
            event
                .calendar_id
                .get_or_insert_with(|| req.mutations[0].calendar_id.clone());
        }
    }
    Ok(Resolved { value: resp, notes })
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteEvent {
    pub event_id: String,
    pub account: Option<String>,
    pub calendar: Option<String>,
    pub provider: Option<String>,
    #[serde(default)]
    pub hard: bool,
}

#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum DeletedEvent {
    Removed(serde_json::Value),
    Cancelled(Vec<UpdateEventEntry>),
}

pub async fn delete_event(
    ctx: &Ctx<'_>,
    args: DeleteEvent,
) -> Result<Resolved<DeletedEvent>, CoreError> {
    delete_event_guarded(ctx, args, |_| std::future::ready(Ok(()))).await
}

pub async fn delete_event_guarded<F, Fut, E>(
    ctx: &Ctx<'_>,
    args: DeleteEvent,
    authorize: F,
) -> Result<Resolved<DeletedEvent>, E>
where
    F: FnOnce(crate::CalendarTarget) -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
    E: From<CoreError>,
{
    delete_event_checked(ctx, args, |target, _| authorize(target)).await
}

pub async fn delete_event_checked<F, Fut, E>(
    ctx: &Ctx<'_>,
    args: DeleteEvent,
    authorize: F,
) -> Result<Resolved<DeletedEvent>, E>
where
    F: FnOnce(crate::CalendarTarget, Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
    E: From<CoreError>,
{
    let Resolved {
        value: (acct, cal, prov),
        notes,
    } = resolve_context(
        ctx,
        args.account.as_deref(),
        args.calendar.as_deref(),
        args.provider.as_deref(),
    )
    .await?;
    authorize(
        (acct.clone(), cal.clone(), prov),
        Some(args.event_id.clone()),
    )
    .await?;
    if args.hard {
        let req = DeleteEventsRequest {
            mutations: vec![DeleteEventMutation {
                provider: prov,
                account_id: acct,
                calendar_id: cal,
                event_id: args.event_id,
                send_updates: None,
            }],
        };
        let resp = rpc::delete_events(ctx.client, &req)
            .await
            .map_err(CoreError::from)?;
        return Ok(Resolved {
            value: DeletedEvent::Removed(resp),
            notes,
        });
    }
    let req = UpdateEventsRequest {
        mutations: vec![UpdateEventMutation {
            provider: prov,
            account_id: acct,
            event_id: args.event_id,
            calendar_id: cal,
            event_data: serde_json::json!({ "status": "cancelled", "responseStatus": "declined" }),
            user_time_zone: ctx.defaults.timezone.clone(),
            send_updates: None,
        }],
    };
    let mut resp = rpc::update_events(ctx.client, &req)
        .await
        .map_err(CoreError::from)?;
    if let [entry] = resp.as_mut_slice() {
        if let Some(event) = &mut entry.event {
            event
                .account_id
                .get_or_insert_with(|| req.mutations[0].account_id.clone());
            event
                .calendar_id
                .get_or_insert_with(|| req.mutations[0].calendar_id.clone());
        }
    }
    Ok(Resolved {
        value: DeletedEvent::Cancelled(resp),
        notes,
    })
}

fn event_date_json(datetime_rfc3339: &str, tz: Option<&str>) -> serde_json::Value {
    match tz {
        Some(z) => serde_json::json!({ "dateTime": datetime_rfc3339, "timeZone": z }),
        None => serde_json::json!({ "dateTime": datetime_rfc3339 }),
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListEvents {
    pub account: Option<String>,
    pub calendar: Option<String>,
    pub provider: Option<String>,
    pub time_min: Option<i64>,
    pub time_max: Option<i64>,
    pub query: Option<String>,
    pub limit: Option<u32>,
    #[serde(default)]
    pub include_deleted: bool,
}

pub type EventPage = Vec<GetEventsResult>;

pub async fn list_events(
    ctx: &Ctx<'_>,
    args: ListEvents,
) -> Result<Resolved<EventPage>, CoreError> {
    let time_min = args.time_min;
    let time_max = args.time_max;
    let targets = if args.account.is_some() || args.calendar.is_some() || args.provider.is_some() {
        let ctx = resolve_context(
            ctx,
            args.account.as_deref(),
            args.calendar.as_deref(),
            args.provider.as_deref(),
        )
        .await?;
        Resolved {
            value: vec![ctx.value],
            notes: ctx.notes,
        }
    } else if ctx.defaults.account.is_some() || ctx.defaults.calendar.is_some() {
        let ctx = resolve_context(ctx, None, None, None).await?;
        Resolved {
            value: vec![ctx.value],
            notes: ctx.notes,
        }
    } else {
        build_all_account_queries(ctx.client).await?
    };

    let queries = targets
        .value
        .into_iter()
        .map(|(acct, cal, prov)| EventQuery {
            provider: prov,
            account_id: acct,
            calendar_id: cal,
            time_min,
            time_max,
            single_events: Some(true),
            show_deleted: Some(args.include_deleted),
            query: args.query.clone(),
            user_time_zone: ctx.defaults.timezone.clone(),
            limit: args.limit,
        })
        .collect();

    let req = GetEventsRequest { queries };
    let results = rpc::get_events(ctx.client, &req).await?;
    Ok(Resolved {
        value: results,
        notes: targets.notes,
    })
}
