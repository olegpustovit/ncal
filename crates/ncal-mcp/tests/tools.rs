use ncal_api::{
    auth::Credentials,
    client::{ClientConfig, NotionCalendarClient},
};
use ncal_core::Defaults;
use ncal_mcp::{ClientSource, NcalMcp, ToolFailure, ToolPolicy};
use rmcp::{model::CallToolRequestParams, service::RequestContext, RoleServer, ServiceExt};
use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

struct Source(NotionCalendarClient, bool);

impl ClientSource for Source {
    async fn client(
        &self,
        _: &RequestContext<RoleServer>,
    ) -> Result<(NotionCalendarClient, Defaults), ToolFailure> {
        Ok((
            self.0.clone(),
            Defaults {
                account: Some("account".into()),
                calendar: Some("calendar".into()),
                timezone: None,
            },
        ))
    }
    async fn authorize_write(
        &self,
        _: &RequestContext<RoleServer>,
        _: &str,
        (account, calendar, _): ncal_core::CalendarTarget,
    ) -> Result<(), ToolFailure> {
        assert_eq!(account, "account");
        assert_eq!(calendar, "calendar");
        if self.1 {
            return Err(ToolFailure::new(
                "calendar_read_only",
                "Fixture denies calendar writes",
            ));
        }
        Ok(())
    }
}

#[tokio::test]
async fn every_tool_dispatches_and_destructive_gate_is_enforced() {
    let upstream = MockServer::start().await;
    let user = json!({"id":"me", "refreshToken":"must-not-appear", "accounts":[{"id":"account","providerName":"google"}]});
    let event = json!({"id":"event","summary":"Fixture"});
    for (operation, body) in [
        ("getUser", user),
        (
            "getCalendarLists",
            json!([{"accountId":"account","calendars":[{"id":"calendar"}]}]),
        ),
        ("getColors", json!({})),
        (
            "getEvents",
            json!([{"accountId":"account","calendarId":"calendar","events":[event]}]),
        ),
        ("getEvent", json!({"event":event})),
        ("createEvent", event.clone()),
        ("updateEvents", json!([{"event":event}])),
        ("deleteEvents", json!({"success":true})),
        ("incrementalSync", json!({})),
        ("getContacts", json!([])),
        ("getUserPreferences", json!({})),
    ] {
        Mock::given(method("POST"))
            .and(path(format!("/v2/{operation}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&upstream)
            .await;
    }
    let mut config = ClientConfig::production_defaults().unwrap();
    config.base_url = upstream.uri().parse().unwrap();
    let client = NotionCalendarClient::builder(reqwest::Client::new())
        .config(config)
        .credentials(Credentials {
            user_id: "me".into(),
            access_token: "fixture".into(),
            refresh_token: "fixture".into(),
            access_token_expires_at: "2099-01-01T00:00:00Z".into(),
        })
        .build()
        .unwrap();
    for (destructive, blocked) in [(true, false), (false, false), (true, true)] {
        let (transport, peer) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(
            NcalMcp::new(
                Source(client.clone(), blocked),
                ToolPolicy {
                    write: true,
                    destructive,
                },
            )
            .serve(transport),
        );
        let connection = ().serve(peer).await.unwrap();
        let tools = connection.peer().list_all_tools().await.unwrap();
        assert_eq!(tools.len(), if destructive { 12 } else { 11 });
        for (name, args) in [
            ("whoami", json!({})),
            ("accounts_list", json!({})),
            ("calendars_list", json!({})),
            ("calendar_colors", json!({})),
            ("events_list", json!({})),
            ("events_get", json!({"event_id":"event"})),
            (
                "events_create",
                json!({"summary":"Fixture","start":"2026-11-20T12:00:00Z","end":"2026-11-20T13:00:00Z"}),
            ),
            (
                "events_update",
                json!({"event_id":"event","summary":"Updated"}),
            ),
            ("events_delete", json!({"event_id":"event","hard":true})),
            ("sync", json!({})),
            ("contacts_list", json!({})),
            ("preferences_get", json!({})),
        ] {
            let Value::Object(args) = args else {
                unreachable!()
            };
            let result = connection
                .peer()
                .call_tool(CallToolRequestParams::new(name).with_arguments(args))
                .await
                .unwrap();
            let content = result.structured_content.unwrap();
            if !destructive && name == "events_delete" {
                assert_eq!(content["code"], "insufficient_scope");
            } else if blocked && ["events_create", "events_update", "events_delete"].contains(&name)
            {
                assert_eq!(content["code"], "calendar_read_only");
            } else {
                assert_ne!(result.is_error, Some(true), "{name}: {content}");
                assert!(!content.to_string().contains("must-not-appear"));
            }
        }
        let _ = connection.cancel().await;
        server.await.unwrap().unwrap();
    }
    let requests = upstream.received_requests().await.unwrap();
    for (operation, expected) in [("createEvent", 2), ("updateEvents", 2), ("deleteEvents", 1)] {
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.url.path() == format!("/v2/{operation}"))
                .count(),
            expected
        );
    }
}
