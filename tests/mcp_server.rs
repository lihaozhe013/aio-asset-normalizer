use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn request(id: u64, method: &str, params: Value) -> String {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string()
}

#[test]
fn real_mcp_binary_negotiates_lists_typed_tools_and_returns_structured_errors() {
    let binary = env!("CARGO_BIN_EXE_aio-asset-normalizer-mcp");
    let missing_glb =
        std::env::temp_dir().join(format!("aio-mcp-missing-{}.glb", uuid::Uuid::new_v4()));
    let missing_fbx =
        std::env::temp_dir().join(format!("aio-mcp-missing-{}.fbx", uuid::Uuid::new_v4()));
    let output_glb =
        std::env::temp_dir().join(format!("aio-mcp-output-{}.glb", uuid::Uuid::new_v4()));
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("MCP server starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (output_sender, output_receiver) = mpsc::channel();
    let output_reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(message) = serde_json::from_str::<Value>(&line) {
                let _ = output_sender.send(message);
            }
        }
    });
    let stdin = child.stdin.as_mut().expect("stdin is piped");
    for message in [
        request(
            1,
            "initialize",
            json!({
                "protocolVersion":"2025-11-25",
                "capabilities":{},
                "clientInfo":{"name":"aio-mcp-smoke-test","version":"1"}
            }),
        ),
        json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}).to_string(),
        request(2, "tools/list", json!({})),
        request(
            3,
            "tools/call",
            json!({
                "name":"glb_inspect",
                "arguments":{"input":missing_glb}
            }),
        ),
    ] {
        writeln!(stdin, "{message}").expect("request is written");
    }
    let mut messages = std::collections::HashMap::new();
    let first_deadline = Instant::now() + Duration::from_secs(10);
    while !(messages.contains_key(&1) && messages.contains_key(&2) && messages.contains_key(&3)) {
        let remaining = first_deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "MCP server did not answer initial requests"
        );
        let message = output_receiver
            .recv_timeout(remaining)
            .expect("initial MCP response arrives");
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            messages.insert(id, message);
        }
    }
    for message in [
        request(
            4,
            "tools/call",
            json!({
                "name":"fbx_convert",
                "arguments":{
                    "input":missing_fbx,
                    "output":output_glb,
                    "blender":null,
                    "overwrite":false
                }
            }),
        ),
        request(
            5,
            "tools/call",
            json!({
                "name":"job_get",
                "arguments":{"job_id":"unknown-file-job"}
            }),
        ),
    ] {
        writeln!(stdin, "{message}").expect("request is written");
    }
    drop(child.stdin.take());

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child
            .try_wait()
            .expect("MCP process can be polled")
            .is_some()
        {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("MCP server did not exit after stdin closed");
        }
        thread::sleep(Duration::from_millis(25));
    }
    output_reader.join().expect("stdout reader finishes");
    messages.extend(output_receiver.into_iter().filter_map(|message| {
        message
            .get("id")
            .and_then(Value::as_u64)
            .map(|id| (id, message))
    }));
    let response_for = |id: u64| {
        messages
            .get(&id)
            .unwrap_or_else(|| panic!("missing JSON-RPC response {id}: {messages:?}"))
    };

    let initialized = response_for(1);
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(
        initialized["result"]["serverInfo"]["name"],
        "aio-asset-normalizer"
    );

    let listed = response_for(2);
    let tools = listed["result"]["tools"]
        .as_array()
        .expect("tools are listed");
    let glb_inspect = tools
        .iter()
        .find(|tool| tool["name"] == "glb_inspect")
        .expect("file inspection tool is discoverable");
    assert!(glb_inspect["inputSchema"]["properties"]["input"].is_object());
    assert!(tools.iter().any(|tool| tool["name"] == "session_capture"));
    assert!(tools.iter().any(|tool| tool["name"] == "job_get"));

    let failure = response_for(3);
    assert_eq!(failure["result"]["isError"], true);
    assert_eq!(failure["result"]["structuredContent"]["ok"], false);
    assert!(failure["result"]["structuredContent"]["error"]["message"]
        .as_str()
        .is_some_and(|message| !message.is_empty()));

    let file_job = response_for(4);
    assert!(file_job["result"]["structuredContent"]["job_id"]
        .as_str()
        .is_some_and(|id| !id.is_empty()));
    assert_eq!(file_job["result"]["structuredContent"]["status"], "running");

    let missing_job = response_for(5);
    assert_eq!(missing_job["result"]["isError"], true);
    assert_eq!(
        missing_job["result"]["structuredContent"]["error"]["kind"],
        "job_not_found"
    );
}
