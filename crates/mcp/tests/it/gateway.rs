//! Integration tests for the Streamable HTTP gateway (MCP revision 2025-03-26).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cadcraft_mcp::gateway::{read_token_file, validate_token, Gateway, GatewayConfig};
use serde_json::json;

fn cli_path() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // deps
    p.pop(); // debug
    p.push("cadcraft-cli");
    p
}

fn http_exchange(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();

    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n", body.len());
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");

    stream.write_all(req.as_bytes()).unwrap();
    if !body.is_empty() {
        stream.write_all(body).unwrap();
    }
    stream.flush().unwrap();

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();

    let parts: Vec<&str> = status_line.split_whitespace().collect();
    let status_code: u16 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    let mut resp_headers = Vec::new();
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_lowercase();
            let val = v.trim().to_string();
            if key == "content-length" {
                content_length = val.parse().unwrap_or(0);
            }
            resp_headers.push((key, val));
        }
    }

    let mut resp_body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut resp_body).unwrap();
    }

    (status_code, resp_headers, resp_body)
}

fn header_val<'a>(headers: &'a [(String, String)], key: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

#[test]
fn gateway_unauthorized_requests_rejected_with_401() {
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: "127.0.0.1:45695".into(),
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    // 1. Missing Authorization header -> 401
    let (status, _, body) = http_exchange(port, "POST", "/mcp", &[], b"{}");
    assert_eq!(status, 401);
    assert!(String::from_utf8_lossy(&body).contains("unauthorized"));

    // 2. Wrong token -> 401
    let (status2, _, body2) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer wrong-token")],
        b"{}",
    );
    assert_eq!(status2, 401);
    assert!(String::from_utf8_lossy(&body2).contains("unauthorized"));

    // Verify 0 sessions or children were created
    assert_eq!(gateway.session_count(), 0);
    gateway.stop();
}

#[test]
fn gateway_unauthorized_host_and_origin_rejected_with_403() {
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: "127.0.0.1:45695".into(),
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    // 1. Evil Host -> 403
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let req = format!("POST /mcp HTTP/1.1\r\nHost: evil.attacker.com\r\nAuthorization: Bearer secret-token-12345\r\nContent-Length: 2\r\n\r\n{{}}");
    stream.write_all(req.as_bytes()).unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    assert!(line.contains("403 Forbidden"), "{line}");

    // 2. Evil Origin -> 403
    let (status, _, body) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[
            ("Authorization", "Bearer secret-token-12345"),
            ("Origin", "http://evil.attacker.com"),
        ],
        b"{}",
    );
    assert_eq!(status, 403);
    assert!(String::from_utf8_lossy(&body).contains("unauthorized origin"));

    gateway.stop();
}

#[test]
fn gateway_get_returns_405_method_not_allowed() {
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: "127.0.0.1:45695".into(),
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let (status, headers, body) = http_exchange(
        port,
        "GET",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        b"",
    );
    assert_eq!(status, 405);
    assert_eq!(header_val(&headers, "allow"), Some("POST, DELETE"));
    assert!(String::from_utf8_lossy(&body).contains("method not allowed"));

    gateway.stop();
}

#[test]
fn gateway_payload_exceeding_1mib_rejected_with_413() {
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: "127.0.0.1:45695".into(),
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer secret-token-12345\r\nContent-Length: {}\r\n\r\n",
        1024 * 1024 + 100
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    assert!(line.contains("413 Payload Too Large"), "{line}");

    gateway.stop();
}

#[test]
fn gateway_unknown_and_missing_session_rejected() {
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: "127.0.0.1:45695".into(),
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    // 1. Request without Mcp-Session-Id after initialize -> 400
    let call_json = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}).to_string();
    let (status, _, body) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        call_json.as_bytes(),
    );
    assert_eq!(status, 400);
    assert!(String::from_utf8_lossy(&body).contains("Mcp-Session-Id"));

    // 2. Request with unknown Mcp-Session-Id -> 404
    let (status2, _, body2) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[
            ("Authorization", "Bearer secret-token-12345"),
            ("Mcp-Session-Id", "unknown-session-123"),
        ],
        call_json.as_bytes(),
    );
    assert_eq!(status2, 404);
    assert!(String::from_utf8_lossy(&body2).contains("unknown session"));

    gateway.stop();
}

#[test]
fn gateway_unsupported_methods_return_405() {
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: "127.0.0.1:45695".into(),
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let auth = &[("Authorization", "Bearer secret-token-12345")];

    for method in ["PUT", "PATCH", "HEAD", "OPTIONS", "TRACE", "CONNECT"] {
        let (status, headers, body) = http_exchange(port, method, "/mcp", auth, b"{}");
        assert_eq!(status, 405, "method {method} should return 405");
        assert_eq!(
            header_val(&headers, "allow"),
            Some("POST, DELETE"),
            "method {method} should have Allow header"
        );
        let body_str = String::from_utf8_lossy(&body);
        assert!(
            body_str.contains("unsupported HTTP method"),
            "method {method} should contain error message: {body_str}"
        );
    }

    gateway.stop();
}

#[test]
fn gateway_malformed_json_and_empty_body_return_400() {
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: "127.0.0.1:45695".into(),
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let auth = &[("Authorization", "Bearer secret-token-12345")];

    let (status1, _, body1) = http_exchange(port, "POST", "/mcp", auth, b"");
    assert_eq!(status1, 400);
    let body1_str = String::from_utf8_lossy(&body1);
    assert!(
        body1_str.contains("invalid") || body1_str.contains("error"),
        "empty body: {body1_str}"
    );

    let (status2, _, body2) = http_exchange(port, "POST", "/mcp", auth, b"not-a-json");
    assert_eq!(status2, 400);
    let body2_str = String::from_utf8_lossy(&body2);
    assert!(
        body2_str.contains("invalid") || body2_str.contains("error"),
        "malformed json: {body2_str}"
    );

    let (status3, _, body3) = http_exchange(port, "POST", "/mcp", auth, b"{invalid}");
    assert_eq!(status3, 400);
    let body3_str = String::from_utf8_lossy(&body3);
    assert!(
        body3_str.contains("invalid") || body3_str.contains("error"),
        "invalid json: {body3_str}"
    );

    gateway.stop();
}

#[test]
fn gateway_invalid_content_length_returns_400() {
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: "127.0.0.1:45695".into(),
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();

    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer secret-token-12345\r\nContent-Length: not-a-number\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();

    assert!(
        status_line.contains("400 Bad Request"),
        "expected 400, got: {status_line}"
    );

    gateway.stop();
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_token_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "cadcraft-mcp-gw-{name}-{}-{nanos}-{seq}",
        std::process::id()
    ))
}

#[test]
fn validate_token_rejects_empty_or_whitespace_only() {
    assert!(validate_token("").is_err());
    assert!(validate_token("   ").is_err());
    assert!(validate_token("\n\t  ").is_err());
    assert_eq!(
        validate_token("").err().as_deref(),
        Some("authentication token cannot be empty or whitespace only")
    );
}

#[test]
fn gateway_start_fails_for_empty_or_whitespace_token() {
    for bad in ["", "   ", "\n\t"] {
        let config = GatewayConfig {
            gateway_port: 0,
            target_app_addr: "127.0.0.1:45695".into(),
            auth_token: bad.into(),
            cli_binary_path: cli_path(),
            allowed_origins: vec![],
        };
        let err = match Gateway::start(config) {
            Ok(_) => panic!("expected Gateway::start to fail for invalid token"),
            Err(e) => e,
        };
        assert_eq!(err, "authentication token cannot be empty or whitespace only");
    }
}

#[test]
fn read_token_file_fails_for_nonexistent_file() {
    let path = temp_token_path("missing");
    let err = read_token_file(&path).expect_err("expected missing file error");
    assert!(!err.is_empty());
}

#[test]
fn read_token_file_rejects_empty_or_whitespace_content() {
    for (name, content) in [("empty", ""), ("spaces", "   \n\t")] {
        let path = temp_token_path(name);
        std::fs::write(&path, content).expect("write temp token file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("set permissions");
        }
        let err = read_token_file(&path).expect_err("expected empty/whitespace rejection");
        assert_eq!(err, "authentication token cannot be empty or whitespace only");
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            assert!(!err.contains(trimmed));
        }
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
#[cfg(unix)]
fn read_token_file_rejects_symlink() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let target = temp_token_path("symlink-target");
    std::fs::write(&target, "secret-symlink-token").expect("write target token file");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600))
        .expect("set target permissions");
    let link = temp_token_path("symlink-link");
    symlink(&target, &link).expect("create symlink");

    let err = read_token_file(&link).expect_err("expected symlink rejection");
    assert!(err.contains("symlink"), "unexpected error: {err}");
    assert!(!err.contains("secret-symlink-token"));

    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_file(&target);
}

#[test]
#[cfg(unix)]
fn read_token_file_rejects_permissive_and_accepts_restrictive_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let path = temp_token_path("perms");
    let secret = "secret-perms-token";
    std::fs::write(&path, format!("{secret}\n")).expect("write temp token file");

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
        .expect("set permissive permissions");
    let err = read_token_file(&path).expect_err("expected permissive permission rejection");
    assert!(
        err.contains("permissions"),
        "unexpected error for permissive file: {err}"
    );
    assert!(!err.contains(secret));

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .expect("set restrictive permissions");
    let token = read_token_file(&path).expect("expected restrictive permissions to be accepted");
    assert_eq!(token, secret);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn token_file_and_validation_errors_do_not_leak_secret() {
    let secret = "super-secret-token-value";
    let err = validate_token("   ").expect_err("expected validation error");
    assert!(!err.contains(secret));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_token_path("no-leak");
        std::fs::write(&path, format!("{secret}\n")).expect("write temp token file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("set permissive permissions");
        let err = read_token_file(&path).expect_err("expected permission error");
        assert!(!err.contains(secret), "error leaked secret: {err}");
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
#[cfg(unix)]
fn read_token_file_rejects_fifo_without_blocking() {
    let path = temp_token_path("fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo command execution");
    assert!(status.success(), "mkfifo command failed");

    let (tx, rx) = std::sync::mpsc::channel();
    let path_clone = path.clone();
    std::thread::spawn(move || {
        let res = read_token_file(&path_clone);
        let _ = tx.send(res);
    });

    let res = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("read_token_file blocked on FIFO");
    let err = res.expect_err("FIFO must not be accepted as a token file");
    assert!(
        err.contains("regular file") || err.contains("cannot open"),
        "unexpected error: {err}"
    );

    let _ = std::fs::remove_file(&path);
}

fn spawn_dummy_app() -> (String, std::sync::mpsc::Sender<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = listener.set_nonblocking(true);
        while rx.try_recv().is_err() {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 128];
                let _ = stream.read(&mut buf);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    (format!("127.0.0.1:{port}"), tx)
}

#[test]
fn gateway_protocol_negotiation_accepts_2025_03_26() {
    let (target_addr, _stop_target) = spawn_dummy_app();
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: target_addr,
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });

    let (status, headers, body) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        init_req.to_string().as_bytes(),
    );
    assert_eq!(status, 200);
    assert!(header_val(&headers, "mcp-session-id").is_some());
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["result"]["protocolVersion"], "2025-03-26");

    gateway.stop();
}

#[test]
fn gateway_protocol_negotiation_negotiates_down_from_2025_11_25() {
    let (target_addr, _stop_target) = spawn_dummy_app();
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: target_addr,
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });

    let (status, headers, body) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        init_req.to_string().as_bytes(),
    );
    assert_eq!(status, 200);
    assert!(header_val(&headers, "mcp-session-id").is_some());
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["result"]["protocolVersion"], "2025-03-26");

    gateway.stop();
}

#[test]
fn gateway_protocol_negotiation_rejects_legacy_or_invalid_version() {
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: "127.0.0.1:45695".into(),
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    // 1. Legacy version 2024-11-05
    let legacy_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });
    let (status, _, body) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        legacy_req.to_string().as_bytes(),
    );
    assert_eq!(status, 400);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"]["code"], -32602);
    assert!(v["error"]["message"].as_str().unwrap().contains("unsupported protocolVersion"));
    assert_eq!(gateway.session_count(), 0);

    // 2. Malformed / invalid version
    let bad_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "initialize",
        "params": {
            "protocolVersion": "invalid-spec-version",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });
    let (status2, _, body2) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        bad_req.to_string().as_bytes(),
    );
    assert_eq!(status2, 400);
    let v2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert_eq!(v2["error"]["code"], -32602);
    assert_eq!(gateway.session_count(), 0);

    gateway.stop();
}

#[test]
fn gateway_uninitialized_session_cleanup_without_leak() {
    let (target_addr, _stop_target) = spawn_dummy_app();
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: target_addr,
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });

    let (status, headers, _) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        init_req.to_string().as_bytes(),
    );
    assert_eq!(status, 200);
    assert!(header_val(&headers, "mcp-session-id").is_some());
    assert_eq!(gateway.session_count(), 1);

    // Client abandons without sending notifications/initialized.
    // Simulate inactivity expiration via clean_sessions_older_than:
    std::thread::sleep(std::time::Duration::from_millis(50));
    gateway.clean_sessions_older_than(std::time::Duration::from_millis(20));
    assert_eq!(gateway.session_count(), 0);

    gateway.stop();
}

#[test]
fn gateway_tools_list_preserves_read_only_annotations() {
    let (target_addr, _stop_target) = spawn_dummy_app();
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: target_addr,
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    // 1. Initialize session
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });
    let (status, headers, _) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        init_req.to_string().as_bytes(),
    );
    assert_eq!(status, 200);
    let session_id = header_val(&headers, "mcp-session-id").unwrap();

    // 2. Request tools/list
    let list_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    let (list_status, _, list_body) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[
            ("Authorization", "Bearer secret-token-12345"),
            ("Mcp-Session-Id", session_id),
        ],
        list_req.to_string().as_bytes(),
    );
    assert_eq!(list_status, 200);
    let v: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
    let tools = v["result"]["tools"].as_array().expect("expected tools array");
    assert!(!tools.is_empty(), "tools array must not be empty");

    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        let ann = &tool["annotations"];
        assert_eq!(
            ann["readOnlyHint"],
            serde_json::json!(true),
            "tool `{name}` must have readOnlyHint: true through gateway"
        );
        assert_eq!(
            ann["destructiveHint"],
            serde_json::json!(false),
            "tool `{name}` must have destructiveHint: false through gateway"
        );
        assert_eq!(
            ann["openWorldHint"],
            serde_json::json!(false),
            "tool `{name}` must have openWorldHint: false through gateway"
        );
    }

    gateway.stop();
}

#[test]
fn gateway_explicit_delete_frees_session_slot() {
    let (target_addr, _stop_target) = spawn_dummy_app();
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: target_addr,
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });

    let (status, headers, _) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        init_req.to_string().as_bytes(),
    );
    assert_eq!(status, 200);
    let session_id = header_val(&headers, "mcp-session-id").unwrap();
    assert_eq!(gateway.session_count(), 1);

    // Send explicit DELETE
    let (del_status, _, del_body) = http_exchange(
        port,
        "DELETE",
        "/mcp",
        &[
            ("Authorization", "Bearer secret-token-12345"),
            ("Mcp-Session-Id", session_id),
        ],
        b"",
    );
    assert_eq!(del_status, 200);
    assert!(String::from_utf8_lossy(&del_body).contains("\"ok\":true"));
    assert_eq!(gateway.session_count(), 0);

    gateway.stop();
}

#[test]
fn gateway_dead_child_reaped_automatically() {
    // When target app does not exist, child process exits after connection attempt fails
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: "127.0.0.1:45695".into(),
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });

    let (status, headers, _) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        init_req.to_string().as_bytes(),
    );
    assert_eq!(status, 200);
    assert!(header_val(&headers, "mcp-session-id").is_some());

    // Give child process moment to exit upon connection refusal to 45695
    std::thread::sleep(Duration::from_millis(100));

    // session_count() runs clean_expired_sessions() which checks try_wait()
    assert_eq!(gateway.session_count(), 0);

    gateway.stop();
}

#[test]
fn gateway_max_sessions_limit_and_recovery() {
    let (target_addr, _stop_target) = spawn_dummy_app();
    let config = GatewayConfig {
        gateway_port: 0,
        target_app_addr: target_addr,
        auth_token: "secret-token-12345".into(),
        cli_binary_path: cli_path(),
        allowed_origins: vec![],
    };
    let mut gateway = Gateway::start(config).unwrap();
    let port = gateway.port();

    let init_req = |id: u64| {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test-client", "version": "1.0.0"}
            }
        }).to_string()
    };

    // 1. Session 1
    let (s1, h1, _) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        init_req(1).as_bytes(),
    );
    assert_eq!(s1, 200);
    let session_id_1 = header_val(&h1, "mcp-session-id").unwrap().to_string();

    // 2. Session 2 (now at MAX_SESSIONS = 2)
    let (s2, h2, _) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        init_req(2).as_bytes(),
    );
    assert_eq!(s2, 200);
    let _session_id_2 = header_val(&h2, "mcp-session-id").unwrap().to_string();
    assert_eq!(gateway.session_count(), 2);

    // 3. Session 3 must receive 429 Too Many Requests
    let (s3, _, b3) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        init_req(3).as_bytes(),
    );
    assert_eq!(s3, 429);
    assert!(String::from_utf8_lossy(&b3).contains("maximum concurrent sessions reached"));

    // 4. Send DELETE for session 1
    let (del_s, _, _) = http_exchange(
        port,
        "DELETE",
        "/mcp",
        &[
            ("Authorization", "Bearer secret-token-12345"),
            ("Mcp-Session-Id", &session_id_1),
        ],
        b"",
    );
    assert_eq!(del_s, 200);
    assert_eq!(gateway.session_count(), 1);

    // 5. Now Session 3 can initialize successfully
    let (s3_retry, h3_retry, _) = http_exchange(
        port,
        "POST",
        "/mcp",
        &[("Authorization", "Bearer secret-token-12345")],
        init_req(3).as_bytes(),
    );
    assert_eq!(s3_retry, 200);
    assert!(header_val(&h3_retry, "mcp-session-id").is_some());
    assert_eq!(gateway.session_count(), 2);

    gateway.stop();
}
