use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ServiceExt};

#[tokio::test]
async fn stdio_read_only_hides_and_refuses_writes() {
    let config = tempfile_config();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ncal"));
    command
        .args(["mcp", "--read-only", "--config"])
        .arg(&config);
    command
        .env("NCAL_ACCESS_TOKEN", "test-access")
        .env("NCAL_USER_ID", "test-user");
    let child = TokioChildProcess::new(command).unwrap();
    let client = ().serve(child).await.unwrap();
    let tools = client.peer().list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 9);
    assert!(tools.iter().any(|tool| tool.name == "events_list"));
    assert!(tools.iter().all(|tool| !matches!(
        tool.name.as_ref(),
        "events_create" | "events_update" | "events_delete"
    )));
    let result = client
        .peer()
        .call_tool(CallToolRequestParams::new("events_delete"))
        .await
        .unwrap();
    assert_eq!(
        result.structured_content.unwrap()["code"],
        "insufficient_scope"
    );
    client.cancel().await.unwrap();
    std::fs::remove_file(config).unwrap();
}

fn tempfile_config() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("ncal-stdio-{}.toml", uuid::Uuid::new_v4()));
    std::fs::write(&path, "").unwrap();
    path
}
