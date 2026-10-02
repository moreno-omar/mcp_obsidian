//! The Ollama client: constructing it, reaching the server, and turning a
//! reply into an [`AssistantTurn`].
//!
//! Most tests run against a tiny in-process HTTP server (plain `std` sockets),
//! so both wire dialects are covered without needing a real Ollama install. The
//! one test that does need a real server is `#[ignore]`d:
//!
//! ```text
//! cargo test -- --ignored live_client
//! ```

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::json;

use mcp_obsidian::ollama_client::{
    ApiFlavor, ChatMessage, DEFAULT_BASE_URL, DEFAULT_MODEL, OllamaClient, Role, ToolCall, ToolSpec,
};

// ---------------------------------------------------------------------------
// Client construction
// ---------------------------------------------------------------------------

#[test]
fn client_normalises_the_base_url_into_a_root() {
    let client = OllamaClient::new("http://localhost:11434/v1", ApiFlavor::Openai, "gemma4:e4b")
        .expect("client should build");

    assert_eq!(client.base_url(), "http://localhost:11434");
    assert_eq!(client.model(), "gemma4:e4b");
    assert_eq!(client.api(), ApiFlavor::Openai);
    assert_eq!(client.chat_url(), "http://localhost:11434/v1/chat/completions");
    assert_eq!(client.openai_url(), "http://localhost:11434/v1/chat/completions");
    assert_eq!(client.native_url(), "http://localhost:11434/api/chat");
    assert_eq!(client.version_url(), "http://localhost:11434/api/version");

    // Trailing slashes and surrounding whitespace are tolerated too.
    let native =
        OllamaClient::new("  http://localhost:11434/  ", ApiFlavor::Native, "m").unwrap();
    assert_eq!(native.base_url(), "http://localhost:11434");
    assert_eq!(native.chat_url(), "http://localhost:11434/api/chat");
    assert_eq!(native.api().as_str(), "native");
}

#[test]
fn client_rejects_an_empty_base_url() {
    let error = OllamaClient::new("   ", ApiFlavor::Openai, "m").unwrap_err();
    assert!(error.to_string().contains("must not be empty"));
}

#[test]
fn model_available_tolerates_the_implicit_latest_tag() {
    let models = vec!["gemma4:e4b".to_string(), "llama3:latest".to_string()];

    assert!(OllamaClient::model_available(&models, "gemma4:e4b"));
    assert!(OllamaClient::model_available(&models, "llama3"));
    assert!(OllamaClient::model_available(&models, "llama3:latest"));
    assert!(!OllamaClient::model_available(&models, "mistral"));
    assert!(!OllamaClient::model_available(&models, "  "));
}

#[test]
fn tool_spec_serialises_as_an_openai_function() {
    let spec = ToolSpec::new("search_by_tag", "desc", json!({"type": "object"}));
    let wire = spec.to_json();

    assert_eq!(wire["type"], "function");
    assert_eq!(wire["function"]["name"], "search_by_tag");
    assert_eq!(wire["function"]["description"], "desc");
    assert_eq!(wire["function"]["parameters"]["type"], "object");
}

#[test]
fn tool_result_messages_echo_the_call_identity() {
    let call = ToolCall {
        id: Some("call_7".to_string()),
        name: "get_note".to_string(),
        arguments: json!({"note": "index"}),
    };
    let message = ChatMessage::tool_result(&call, "note body");

    assert_eq!(message.role, Role::Tool);
    assert_eq!(message.content, "note body");
    assert_eq!(message.tool_name.as_deref(), Some("get_note"));
    assert_eq!(message.tool_call_id.as_deref(), Some("call_7"));
    assert!(message.tool_calls.is_empty());
}

// ---------------------------------------------------------------------------
// Connecting and receiving a response
// ---------------------------------------------------------------------------

#[tokio::test]
async fn version_and_models_read_the_native_endpoints() {
    let (base, server) = spawn_http_server(vec![
        Reply::json(200, r#"{"version":"0.33.1"}"#),
        Reply::json(200, r#"{"models":[{"model":"gemma4:e4b"},{"name":"llama3:latest"}]}"#),
    ]);

    let client = OllamaClient::new(&base, ApiFlavor::Openai, "gemma4:e4b").unwrap();
    assert_eq!(client.version().await.unwrap(), "0.33.1");
    let models = client.models().await.unwrap();
    assert_eq!(models, vec!["gemma4:e4b".to_string(), "llama3:latest".to_string()]);
    assert!(OllamaClient::model_available(&models, "gemma4:e4b"));

    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET /api/version "), "{}", requests[0]);
    assert!(requests[1].starts_with("GET /api/tags "), "{}", requests[1]);
}

#[tokio::test]
async fn chat_sends_the_prompt_and_parses_an_openai_reply() {
    let (base, server) = spawn_http_server(vec![Reply::json(
        200,
        r#"{"choices":[{"finish_reason":"tool_calls","message":{
             "content":"checking the vault",
             "tool_calls":[{"id":"call_abc","type":"function","function":{
                 "name":"search_by_tag","arguments":"{\"tags\":[\"project\"]}"}}]}}]}"#,
    )]);

    let client = OllamaClient::new(&base, ApiFlavor::Openai, "gemma4:e4b").unwrap();
    let turn = client
        .chat(&[ChatMessage::user("what is tagged project?")], &[])
        .await
        .unwrap();

    assert_eq!(turn.content, "checking the vault");
    assert_eq!(turn.finish_reason.as_deref(), Some("tool_calls"));
    assert!(turn.has_tool_calls());

    let call = &turn.tool_calls[0];
    assert_eq!(call.id.as_deref(), Some("call_abc"));
    assert_eq!(call.name, "search_by_tag");
    // OpenAI sends arguments JSON encoded; they must arrive parsed.
    assert_eq!(call.arguments["tags"][0], "project");

    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].starts_with("POST /v1/chat/completions "),
        "{}",
        requests[0]
    );
    let body = request_body(&requests[0]);
    assert!(body.contains("\"model\":\"gemma4:e4b\""), "{body}");
    assert!(body.contains("what is tagged project?"), "{body}");
    assert!(body.contains("\"stream\":false"), "{body}");
}

#[tokio::test]
async fn chat_parses_a_native_reply_and_synthesises_missing_ids() {
    let (base, server) = spawn_http_server(vec![Reply::json(
        200,
        r#"{"message":{"content":"reading the note","tool_calls":[
             {"function":{"name":"get_note","arguments":{"note":"index"}}}]},
             "done_reason":"stop"}"#,
    )]);

    let client = OllamaClient::new(&base, ApiFlavor::Native, "gemma4:e4b").unwrap();
    let tools = vec![ToolSpec::new("get_note", "read a note", json!({"type": "object"}))];
    let turn = client
        .chat(&[ChatMessage::user("read the index")], &tools)
        .await
        .unwrap();

    assert_eq!(turn.content, "reading the note");
    assert_eq!(turn.finish_reason.as_deref(), Some("stop"));
    assert_eq!(turn.tool_calls[0].id.as_deref(), Some("call_0"));
    assert_eq!(turn.tool_calls[0].name, "get_note");
    assert_eq!(turn.tool_calls[0].arguments["note"], "index");

    let requests = server.join().unwrap();
    assert!(requests[0].starts_with("POST /api/chat "), "{}", requests[0]);
    let body = request_body(&requests[0]);
    assert!(body.contains("\"tools\""), "{body}");
    assert!(body.contains("read a note"), "{body}");
}

#[tokio::test]
async fn chat_surfaces_the_server_error_message() {
    let (base, server) = spawn_http_server(vec![Reply::json(
        404,
        r#"{"error":"model 'gemma4:e4b' not found"}"#,
    )]);

    let client = OllamaClient::new(&base, ApiFlavor::Openai, "gemma4:e4b").unwrap();
    let error = client
        .chat(&[ChatMessage::user("hi")], &[])
        .await
        .expect_err("a 404 must be an error");

    let message = error.to_string();
    assert!(message.contains("404"), "{message}");
    assert!(message.contains("not found"), "{message}");
    assert_eq!(server.join().unwrap().len(), 1);
}

#[tokio::test]
async fn chat_reports_an_unreachable_server() {
    // Bind then drop, so the port is almost certainly closed.
    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let client = OllamaClient::new(format!("http://{addr}"), ApiFlavor::Openai, "m").unwrap();
    let error = client
        .chat(&[ChatMessage::user("hi")], &[])
        .await
        .expect_err("a closed port must be an error");

    assert!(error.to_string().contains("cannot reach Ollama"), "{error}");
}

/// Full round trip against a real server. Ignored by default; run with
/// `cargo test -- --ignored live_client`.
#[tokio::test]
#[ignore = "requires a running Ollama server (ollama serve)"]
async fn live_client_connects_and_answers() {
    let base = std::env::var("OLLAMA_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let client = OllamaClient::new(&base, ApiFlavor::Openai, &model).unwrap();

    let version = client.version().await.expect("Ollama should be reachable");
    assert!(!version.is_empty());

    let models = client.models().await.expect("model list should be readable");
    assert!(
        OllamaClient::model_available(&models, &model),
        "{model} is not installed (found {} models)",
        models.len()
    );

    let turn = client
        .chat(
            &[ChatMessage::user("Reply with the single word: pong")],
            &[],
        )
        .await
        .expect("chat should answer");
    assert!(!turn.content.trim().is_empty(), "the model answered nothing");
}

// ---------------------------------------------------------------------------
// Minimal in-process HTTP server
// ---------------------------------------------------------------------------

/// A canned HTTP response.
struct Reply {
    status: u16,
    body: String,
}

impl Reply {
    fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_string(),
        }
    }
}

/// Serve `replies` in order on a loopback port.
///
/// Returns the base URL to point a client at, plus a handle that yields the raw
/// requests that were received. The loop gives up after five seconds so a
/// failing test can never wedge the suite.
fn spawn_http_server(replies: Vec<Reply>) -> (String, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();

    let handle = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut requests = Vec::new();
        for reply in replies {
            let Some(mut stream) = accept_before(&listener, deadline) else {
                break;
            };
            requests.push(read_request(&mut stream));
            let _ = stream.write_all(render(reply).as_bytes());
            let _ = stream.flush();
        }
        requests
    });

    (format!("http://{addr}"), handle)
}

/// Accept one connection, polling until `deadline` elapses.
fn accept_before(listener: &TcpListener, deadline: Instant) -> Option<TcpStream> {
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => return Some(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
    None
}

/// Read a whole request: headers plus the body announced by `Content-Length`.
fn read_request(stream: &mut TcpStream) -> String {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    while let Ok(read) = stream.read(&mut chunk) {
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(end) = header_end(&buffer) {
            let head = String::from_utf8_lossy(&buffer[..end]).to_string();
            if buffer.len() >= end + 4 + content_length(&head) {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buffer).to_string()
}

/// Offset of the blank line separating headers from body.
fn header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

/// `Content-Length` of a request head, or 0 when absent.
fn content_length(head: &str) -> usize {
    for line in head.lines() {
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
            && let Ok(length) = value.trim().parse()
        {
            return length;
        }
    }
    0
}

/// The body of a raw request, for asserting what the client actually sent.
fn request_body(request: &str) -> String {
    match header_end(request.as_bytes()) {
        Some(end) => request[end + 4..].to_string(),
        None => String::new(),
    }
}

/// Render a reply as an HTTP/1.1 message that closes the connection.
fn render(reply: Reply) -> String {
    let reason = if reply.status == 200 { "OK" } else { "Error" };
    format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.status,
        reply.body.len(),
        reply.body
    )
}
