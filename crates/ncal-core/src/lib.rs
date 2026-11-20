mod context;
pub mod events;

pub use context::{
    build_all_account_queries, flatten_calendar_results, resolve_context, CalendarTarget,
    CoreError, Ctx, Defaults, Resolved,
};

mod calendar;
pub use calendar::list_calendars;
pub use ncal_api::endpoints::{
    get_colors as calendar_colors, get_contacts as contacts_list, get_user,
    get_user_preferences as preferences_get, incremental_sync as sync, remove_account,
    update_primary_account,
};

pub async fn accounts_list(
    client: &ncal_api::client::NotionCalendarClient,
) -> Result<Vec<ncal_api::types::Account>, CoreError> {
    get_user(client)
        .await?
        .accounts
        .ok_or_else(|| CoreError::Selection("no accounts on user object".into()))
}
