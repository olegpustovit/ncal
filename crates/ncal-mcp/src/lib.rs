use std::future::Future;

use ncal_api::{client::NotionCalendarClient, endpoints::IncrementalSyncRequest, error::ApiError};
use ncal_core::{events::*, Candidate, CoreError, Ctx, Defaults, ErrorCode, Hint, Resolved};
use rmcp::{model::*, service::RequestContext, ErrorData as McpError, RoleServer, ServerHandler};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

pub trait ClientSource: Send + Sync + 'static {
    fn client(
        &self,
        request: &RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<(NotionCalendarClient, Defaults), ToolFailure>> + Send;

    fn policy(&self, _request: &RequestContext<RoleServer>) -> ToolPolicy {
        ToolPolicy {
            write: true,
            destructive: true,
        }
    }

    fn finished(
        &self,
        _request: &RequestContext<RoleServer>,
        _tool: &str,
        _outcome: &'static str,
    ) -> impl Future<Output = ()> + Send {
        std::future::ready(())
    }

    /// Observe the outcome and structured response before it is returned to the client.
    fn completed(
        &self,
        request: &RequestContext<RoleServer>,
        tool: &str,
        result: Result<&Value, &ToolFailure>,
    ) -> impl Future<Output = ()> + Send {
        self.finished(
            request,
            tool,
            result.map_or_else(|error| error.code, |_| "ok"),
        )
    }

    /// Authorize a write against its resolved target before contacting the provider.
    fn authorize_write(
        &self,
        _request: &RequestContext<RoleServer>,
        _tool: &str,
        _target: ncal_core::CalendarTarget,
    ) -> impl Future<Output = Result<(), ToolFailure>> + Send {
        std::future::ready(Ok(()))
    }

    fn relink_hint(&self) -> Option<&str> {
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ToolPolicy {
    pub write: bool,
    pub destructive: bool,
}

impl ToolPolicy {
    fn permits(self, tool: &Tool) -> bool {
        let Some(annotations) = &tool.annotations else {
            return false;
        };
        annotations.read_only_hint == Some(true)
            || (self.write && (annotations.destructive_hint == Some(false) || self.destructive))
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            write: self.write && other.write,
            destructive: self.destructive && other.destructive,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ToolFailure {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
}

impl ToolFailure {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            candidates: vec![],
        }
    }
}

impl From<ApiError> for ToolFailure {
    fn from(error: ApiError) -> Self {
        match error {
            ApiError::InvalidToken => Self::new("relink_needed", "Calendar session has expired"),
            ApiError::RefreshCooldown => {
                Self::new("refresh_cooldown", "Wait 30 seconds before retrying")
            }
            _ => Self::new("calendar_api_error", "Calendar request failed"),
        }
    }
}

impl From<CoreError> for ToolFailure {
    fn from(error: CoreError) -> Self {
        if let Some(upstream) = error.upstream {
            return (*upstream).into();
        }
        let code = match error.code {
            ErrorCode::AccountNotFound => "account_not_found",
            ErrorCode::AccountRequired => "account_required",
            ErrorCode::CalendarRequired => "calendar_required",
            _ => "invalid_selection",
        };
        let hint = match error.hint {
            Some(Hint::SelectAccount) => Some("Call accounts_list and pass an account ID"),
            Some(Hint::SelectCalendar) => Some("Call calendars_list and pass a calendar ID"),
            _ => None,
        };
        Self {
            code,
            message: error.message,
            hint: hint.map(str::to_owned),
            candidates: error.candidates,
        }
    }
}

pub struct NcalMcp<S> {
    source: S,
    policy: ToolPolicy,
    tools: Vec<Tool>,
}

impl<S: ClientSource> NcalMcp<S> {
    pub fn new(source: S, policy: ToolPolicy) -> Self {
        Self {
            source,
            policy,
            tools: tools(),
        }
    }

    fn policy(&self, request: &RequestContext<RoleServer>) -> ToolPolicy {
        self.policy.intersect(self.source.policy(request))
    }

    async fn invoke(
        &self,
        request: CallToolRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<Value, ToolFailure> {
        let tool = self
            .tools
            .iter()
            .find(|tool| tool.name == request.name)
            .ok_or_else(|| ToolFailure::new("unknown_tool", "Unknown calendar tool"))?;

        if !self.policy(context).permits(tool) {
            return Err(ToolFailure::new(
                "insufficient_scope",
                "This request cannot call that tool",
            ));
        }

        let (client, defaults) = self.source.client(context).await?;
        let ctx = Ctx {
            client: &client,
            defaults: &defaults,
        };
        dispatch(
            tool.name.as_ref(),
            request.arguments.unwrap_or_default(),
            &ctx,
            |target| {
                self.source
                    .authorize_write(context, tool.name.as_ref(), target)
            },
        )
        .await
    }
}

impl<S: ClientSource> ServerHandler for NcalMcp<S> {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("Unofficial Notion Calendar tools")
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(
            self.tools
                .iter()
                .filter(|tool| self.policy(&context).permits(tool))
                .cloned()
                .collect(),
        ))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let name = request.name.clone();
        let result = self.invoke(request, &context).await;
        self.source
            .completed(&context, &name, result.as_ref())
            .await;
        Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(mut error) => {
                if error.code == "relink_needed" && error.hint.is_none() {
                    error.hint = self.source.relink_hint().map(str::to_owned);
                }
                CallToolResult::structured_error(json!(error))
            }
        }
        .into())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListCalendars {
    account: Option<String>,
    provider: Option<String>,
}

fn arguments<T: DeserializeOwned>(args: JsonObject) -> Result<T, ToolFailure> {
    serde_json::from_value(Value::Object(args)).map_err(|_| {
        ToolFailure::new(
            "invalid_arguments",
            "Arguments do not match the tool schema",
        )
    })
}

fn data<T: Serialize>(value: T) -> Result<Value, ToolFailure> {
    serde_json::to_value(value)
        .map_err(|_| ToolFailure::new("serialization_error", "Cannot serialize tool result"))
}

fn resolved<T: Serialize>(value: Resolved<T>) -> Result<Value, ToolFailure> {
    Ok(json!({ "data": data(value.value)?, "notes": value.notes }))
}

fn tool<T: JsonSchema + 'static>(
    name: &'static str,
    description: &'static str,
    read: bool,
    destructive: bool,
) -> Tool {
    Tool::new(name, description, JsonObject::new())
        .with_input_schema::<T>()
        .with_annotations(
            ToolAnnotations::new()
                .read_only(read)
                .destructive(destructive),
        )
}

macro_rules! define_tools {
    ($($name:literal, $label:literal, $read:literal, $destructive:literal, $ty:ty, |$ctx:ident, $args:ident, $authorize:ident| $call:expr;)*) => {
        fn tools() -> Vec<Tool> {
            vec![$(tool::<$ty>($name, $label, $read, $destructive)),*]
        }

        async fn dispatch<F, Fut>(name: &str, args: JsonObject, ctx: &Ctx<'_>, authorize: F) -> Result<Value, ToolFailure>
        where
            F: FnOnce(ncal_core::CalendarTarget) -> Fut,
            Fut: Future<Output = Result<(), ToolFailure>>,
        {
            match name {
                $($name => {
                    let $args: $ty = arguments(args)?;
                    let $ctx = ctx;
                    let $authorize = authorize;
                    $call
                },)*
                _ => Err(ToolFailure::new("unknown_tool", "Unknown calendar tool")),
            }
        }
    };
}

define_tools! {
    "whoami", "Show the linked calendar identity", true, false, Empty, |ctx, _args, _authorize| {
        let user = ncal_core::get_user(ctx.client).await?;
        Ok(json!({ "id": user.id, "email": user.email, "displayName": user.display_name, "notionUserId": user.notion_user_id }))
    };
    "accounts_list", "List connected calendar accounts", true, false, Empty, |ctx, _args, _authorize| {
        let accounts = ncal_core::accounts_list(ctx.client).await?;
        let accounts: Vec<_> = accounts.into_iter().map(|account| json!({
            "id": account.id, "email": account.email, "displayName": account.display_name,
            "providerName": account.provider_name, "primary": account.primary
        })).collect();
        Ok(json!({ "accounts": accounts }))
    };
    "calendars_list", "Discover calendars and their IDs", true, false, ListCalendars, |ctx, args, _authorize| {
        let results = ncal_core::list_calendars(ctx.client, args.account.as_deref(), args.provider.as_deref()).await?;
        Ok(json!({ "results": data(results)? }))
    };
    "calendar_colors", "List the available calendar colors", true, false, Empty, |ctx, _args, _authorize| {
        data(ncal_core::calendar_colors(ctx.client).await?)
    };
    "events_list", "List events across calendars or filter by calendar and time range in epoch milliseconds", true, false, ListEvents, |ctx, args, _authorize| {
        resolved(list_events(ctx, args).await?)
    };
    "events_get", "Fetch one calendar event by ID", true, false, GetEvent, |ctx, args, _authorize| {
        resolved(get_event(ctx, args).await?)
    };
    "events_create", "Create a calendar event with RFC3339 start and end times", false, false, CreateEvent, |ctx, args, authorize| {
        resolved(create_event_guarded(ctx, args, authorize).await?)
    };
    "events_update", "Update specified fields on an existing calendar event", false, false, UpdateEvent, |ctx, args, authorize| {
        let result = update_event_guarded(ctx, args, authorize).await?;
        if result.value.len() != 1 || result.value[0].event.is_none()
            || result.value[0].error_message.is_some()
        {
            return Err(ToolFailure::new("calendar_api_error", "Calendar rejected the event update"));
        }
        resolved(result)
    };
    "events_delete", "Cancel or permanently delete a calendar event", false, true, DeleteEvent, |ctx, args, authorize| {
        let result = delete_event_guarded(ctx, args, authorize).await?;
        if let DeletedEvent::Cancelled(entries) = &result.value {
            if entries.len() != 1 || entries[0].event.is_none() || entries[0].error_message.is_some() {
                return Err(ToolFailure::new("calendar_api_error", "Calendar rejected the event cancellation"));
            }
        }
        resolved(result)
    };
    "sync", "Fetch an initial calendar sync", true, false, Empty, |ctx, _args, _authorize| {
        data(ncal_core::sync(ctx.client, &IncrementalSyncRequest { sync_tokens: vec![], metadata: None }).await?)
    };
    "contacts_list", "List contacts from connected calendar accounts", true, false, Empty, |ctx, _args, _authorize| {
        Ok(json!({ "contacts": data(ncal_core::contacts_list(ctx.client).await?)? }))
    };
    "preferences_get", "Show calendar preferences", true, false, Empty, |ctx, _args, _authorize| {
        Ok(json!({ "preferences": ncal_core::preferences_get(ctx.client).await? }))
    };
}
