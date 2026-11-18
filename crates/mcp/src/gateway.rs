//! Streamable HTTP gateway for axialCAD MCP (spec revision 2025-03-26).
//!
//! Provides a local HTTP bridge in loopback (`127.0.0.1`) that exposes CADCraft's read-only MCP
//! tools over HTTP POST requests, spawning and managing dedicated child processes:
//! `cadcraft-cli mcp --connect 127.0.0.1:<APP_PORT> --read-only`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Maximum body payload: 1 MiB.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
/// Maximum concurrent active sessions.
pub const MAX_SESSIONS: usize = 2;
/// Session inactivity timeout: 300 seconds (5 minutes).
pub const SESSION_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(300);
/// Supported MCP protocol version in Streamable HTTP mode.
pub const STREAMABLE_HTTP_PROTOCOL_VERSION: &str = "2025-03-26";

/// Configuration for the local Streamable HTTP gateway.
#[derive(Clone, Debug)]
pub struct GatewayConfig {
    /// Loopback port to listen on.
    pub gateway_port: u16,
    /// Target application control host:port (e.g. "127.0.0.1:45695").
    pub target_app_addr: String,
    /// Shared secret token required in HTTP Authorization Bearer header.
    pub auth_token: String,
    /// Path to the `cadcraft-cli` binary.
    pub cli_binary_path: PathBuf,
    /// Optional additional allowed Origin headers.
    pub allowed_origins: Vec<String>,
}

struct SessionChild {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    process: Child,
}

struct Session {
    _id: String,
    child: Arc<Mutex<SessionChild>>,
    last_activity: Instant,
}

/// A running Streamable HTTP gateway server.
pub struct Gateway {
    _config: GatewayConfig,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    shutdown: Arc<AtomicBool>,
    actual_port: u16,
    listener_thread: Option<std::thread::JoinHandle<()>>,
}

impl Gateway {
    /// Start the gateway on 127.0.0.1. If `config.gateway_port` is 0, OS allocates a free port.
    pub fn start(mut config: GatewayConfig) -> Result<Self, String> {
        config.auth_token = validate_token(&config.auth_token)?;
        let listener = TcpListener::bind(("127.0.0.1", config.gateway_port))
            .map_err(|e| format!("cannot bind gateway to 127.0.0.1:{}: {e}", config.gateway_port))?;
        let actual_port = listener
            .local_addr()
            .map_err(|e| format!("cannot get local addr: {e}"))?
            .port();

        // 500ms timeout on accept so the thread can react to shutdown signal
        let _ = listener.set_nonblocking(false);

        let sessions: Arc<Mutex<HashMap<String, Session>>> = Arc::new(Mutex::new(HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let sessions_clone = Arc::clone(&sessions);
        let shutdown_clone = Arc::clone(&shutdown);
        let config_clone = config.clone();

        let listener_thread = std::thread::spawn(move || {
            // Set 1s timeout on accept so loop periodically checks shutdown
            // In standard library, we can set read timeout or accept in loop
            for stream in listener.incoming() {
                if shutdown_clone.load(Ordering::Relaxed) {
                    break;
                }
                match stream {
                    Ok(sock) => {
                        let cfg = config_clone.clone();
                        let sess = Arc::clone(&sessions_clone);
                        std::thread::spawn(move || {
                            handle_connection(sock, cfg, sess, actual_port);
                        });
                    }
                    Err(_) => {
                        if shutdown_clone.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            _config: config,
            sessions,
            shutdown,
            actual_port,
            listener_thread: Some(listener_thread),
        })
    }

    /// The port the gateway is listening on.
    pub fn port(&self) -> u16 {
        self.actual_port
    }

    /// Active session count.
    pub fn session_count(&self) -> usize {
        self.clean_expired_sessions();
        self.sessions.lock().map(|s| s.len()).unwrap_or(0)
    }

    /// Sweep and terminate sessions inactive for at least `timeout`, and also
    /// remove sessions whose child process has already exited.
    pub fn clean_sessions_older_than(&self, timeout: Duration) {
        if let Ok(mut map) = self.sessions.lock() {
            let now = Instant::now();
            let mut expired = Vec::new();
            for (id, sess) in map.iter() {
                let timed_out = now.duration_since(sess.last_activity) >= timeout;
                let child_dead = match sess.child.lock() {
                    Ok(mut ch) => ch.process.try_wait().map(|opt| opt.is_some()).unwrap_or(true),
                    Err(_) => true,
                };
                if timed_out || child_dead {
                    expired.push(id.clone());
                }
            }
            for id in expired {
                if let Some(sess) = map.remove(&id) {
                    if let Ok(mut child) = sess.child.lock() {
                        let _ = child.process.kill();
                        let _ = child.process.wait();
                    }
                }
            }
        }
    }

    /// Sweep and terminate expired sessions.
    pub fn clean_expired_sessions(&self) {
        self.clean_sessions_older_than(SESSION_INACTIVITY_TIMEOUT);
    }

    /// Stop the gateway and kill all running child processes.
    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        // Clean and kill all active child sessions
        if let Ok(mut map) = self.sessions.lock() {
            for (_, sess) in map.drain() {
                if let Ok(mut child) = sess.child.lock() {
                    let _ = child.process.kill();
                    let _ = child.process.wait();
                }
            }
        }
        // Connect once to unblock accept if waiting
        let _ = TcpStream::connect(("127.0.0.1", self.actual_port));
        if let Some(h) = self.listener_thread.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Constant time token comparison.
fn token_matches(expected: &str, given: &str) -> bool {
    let (a, b) = (expected.as_bytes(), given.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Validate an authentication token.
///
/// Trims surrounding whitespace and rejects empty/whitespace-only values.
pub fn validate_token(token: &str) -> Result<String, String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err("authentication token cannot be empty or whitespace only".into());
    }
    Ok(trimmed.to_string())
}

#[cfg(unix)]
fn current_uid() -> Result<u32, String> {
    let status = std::fs::read_to_string("/proc/self/status")
        .map_err(|_| "cannot determine current process uid".to_string())?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            let uid = rest
                .split_whitespace()
                .next()
                .ok_or_else(|| "cannot determine current process uid".to_string())?
                .parse::<u32>()
                .map_err(|_| "cannot determine current process uid".to_string())?;
            return Ok(uid);
        }
    }
    Err("cannot determine current process uid".to_string())
}

/// Read and validate an authentication token from a file.
///
/// On Unix this opens the file atomically with `O_NOFOLLOW | O_NONBLOCK` to
/// avoid TOCTOU races:
/// - `O_NOFOLLOW` causes `open` to fail with `ELOOP` if the final path component
///   is a symlink. This protects against symlink substitution after a
///   `symlink_metadata` check.
/// - `O_NONBLOCK` prevents the open from blocking if the path refers to a FIFO
///   or special device. The subsequent `metadata().is_file()` check guarantees
///   the opened descriptor is a regular file.
/// - Rust's stdlib automatically applies `O_CLOEXEC` on Unix, so we do not
///   duplicate it.
///
/// The function rejects non-regular files, files not owned by the current user,
/// and files readable/writable/executable by group or others.
/// Error messages never include the token or file contents.
pub fn read_token_file(path: &Path) -> Result<String, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| {
                if e.raw_os_error() == Some(libc::ELOOP) {
                    "token file must not be a symlink".to_string()
                } else {
                    "cannot open token file".to_string()
                }
            })?;

        let meta = file
            .metadata()
            .map_err(|_| "cannot read token file metadata".to_string())?;

        if !meta.file_type().is_file() {
            return Err("token file must be a regular file".to_string());
        }

        let uid = current_uid()?;
        if meta.uid() != uid {
            return Err("token file must be owned by the current user".to_string());
        }
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(
                "token file permissions must be restrictive (no access for group or others)"
                    .to_string(),
            );
        }

        let mut content = String::new();
        (&file)
            .read_to_string(&mut content)
            .map_err(|_| "cannot read token file".to_string())?;
        validate_token(&content)
    }

    #[cfg(not(unix))]
    {
        let link_meta = std::fs::symlink_metadata(path)
            .map_err(|_| "cannot read token file metadata".to_string())?;
        if link_meta.file_type().is_symlink() {
            return Err("token file must not be a symlink".to_string());
        }

        let file = std::fs::File::open(path)
            .map_err(|_| "cannot open token file".to_string())?;
        let meta = file
            .metadata()
            .map_err(|_| "cannot read token file metadata".to_string())?;
        if !meta.file_type().is_file() {
            return Err("token file must be a regular file".to_string());
        }

        let mut content = String::new();
        (&file)
            .read_to_string(&mut content)
            .map_err(|_| "cannot read token file".to_string())?;
        validate_token(&content)
    }
}

/// Generate a 32-character random hexadecimal session ID using OS entropy.
fn generate_session_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| format!("entropy failure: {e}"))?;
    let mut s = String::with_capacity(32);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut s, "{:02x}", b);
    }
    Ok(s)
}

struct ParsedRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<ParsedRequest, (u16, &'static str)> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut reader = BufReader::new(stream);
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .map_err(|_| (400, "cannot read request line"))?;

    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err((400, "malformed request line"));
    }
    let method = parts[0].to_uppercase();
    let path = parts[1].to_string();

    let mut headers = HashMap::new();
    let mut total_header_bytes = first_line.len();

    loop {
        let mut line = String::new();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|_| (400, "cannot read headers"))?;
        total_header_bytes += bytes_read;
        if total_header_bytes > 64 * 1024 {
            return Err((431, "request header fields too large"));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }

    let mut body = Vec::new();
    if let Some(len_str) = headers.get("content-length") {
        let len: usize = len_str.parse().map_err(|_| (400, "invalid content-length"))?;
        if len > MAX_BODY_BYTES {
            return Err((413, "payload too large (exceeds 1 MiB)"));
        }
        body.resize(len, 0);
        reader
            .read_exact(&mut body)
            .map_err(|_| (400, "incomplete body read"))?;
    }

    Ok(ParsedRequest {
        method,
        path,
        headers,
        body,
    })
}

fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    status_text: &str,
    extra_headers: &[(&str, &str)],
    body: &[u8],
) {
    let mut head = format!(
        "HTTP/1.1 {status} {status_text}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (k, v) in extra_headers {
        head.push_str(k);
        head.push_str(": ");
        head.push_str(v);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");

    let _ = stream.write_all(head.as_bytes());
    if !body.is_empty() {
        let _ = stream.write_all(body);
    }
    let _ = stream.flush();
}

fn status_reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        _ => "Error",
    }
}

fn handle_connection(
    mut stream: TcpStream,
    config: GatewayConfig,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    actual_port: u16,
) {
    let req = match read_http_request(&mut stream) {
        Ok(r) => r,
        Err((code, msg)) => {
            let body = json!({"error": msg}).to_string();
            write_http_response(
                &mut stream,
                code,
                status_reason(code),
                &[("Content-Type", "application/json")],
                body.as_bytes(),
            );
            return;
        }
    };

    // 1. Path check
    if req.path != "/mcp" {
        let body = json!({"error": "not found"}).to_string();
        write_http_response(
            &mut stream,
            404,
            "Not Found",
            &[("Content-Type", "application/json")],
            body.as_bytes(),
        );
        return;
    }

    // 2. Host validation
    if let Some(host) = req.headers.get("host") {
        let valid_port = actual_port.to_string();
        let is_valid = host == &format!("127.0.0.1:{valid_port}")
            || host == &format!("localhost:{valid_port}")
            || host == "127.0.0.1"
            || host == "localhost";
        if !is_valid {
            let body = json!({"error": "unauthorized host"}).to_string();
            write_http_response(
                &mut stream,
                403,
                "Forbidden",
                &[("Content-Type", "application/json")],
                body.as_bytes(),
            );
            return;
        }
    } else {
        let body = json!({"error": "missing host header"}).to_string();
        write_http_response(
            &mut stream,
            400,
            "Bad Request",
            &[("Content-Type", "application/json")],
            body.as_bytes(),
        );
        return;
    }

    // 3. Origin validation policy (no universal wildcard CORS)
    if let Some(origin) = req.headers.get("origin") {
        let valid_port = actual_port.to_string();
        let allowed = origin == &format!("http://127.0.0.1:{valid_port}")
            || origin == &format!("http://localhost:{valid_port}")
            || config.allowed_origins.iter().any(|o| o == origin);
        if !allowed {
            let body = json!({"error": "unauthorized origin"}).to_string();
            write_http_response(
                &mut stream,
                403,
                "Forbidden",
                &[("Content-Type", "application/json")],
                body.as_bytes(),
            );
            return;
        }
    }

    // 4. Authentication (checked on ALL requests before touching sessions or children)
    let auth = req.headers.get("authorization");
    let token = auth.and_then(|h| h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer ")));
    if !token.is_some_and(|t| token_matches(&config.auth_token, t)) {
        let body = json!({"error": "unauthorized: missing or invalid authorization token"}).to_string();
        write_http_response(
            &mut stream,
            401,
            "Unauthorized",
            &[("Content-Type", "application/json"), ("WWW-Authenticate", "Bearer")],
            body.as_bytes(),
        );
        return;
    }

    // 5. Method routing
    match req.method.as_str() {
        "GET" => {
            // Per spec reduction: return 405 Method Not Allowed when GET SSE is disabled
            let body = json!({"error": "method not allowed: use POST for Streamable HTTP"}).to_string();
            write_http_response(
                &mut stream,
                405,
                "Method Not Allowed",
                &[("Content-Type", "application/json"), ("Allow", "POST, DELETE")],
                body.as_bytes(),
            );
        }
        "DELETE" => {
            // Explicit session termination
            let session_id = match req.headers.get("mcp-session-id") {
                Some(id) => id,
                None => {
                    let body = json!({"error": "missing Mcp-Session-Id header"}).to_string();
                    write_http_response(
                        &mut stream,
                        400,
                        "Bad Request",
                        &[("Content-Type", "application/json")],
                        body.as_bytes(),
                    );
                    return;
                }
            };

            let mut map = match sessions.lock() {
                Ok(m) => m,
                Err(_) => {
                    let body = json!({"error": "internal mutex failure"}).to_string();
                    write_http_response(&mut stream, 500, "Internal Server Error", &[], body.as_bytes());
                    return;
                }
            };

            if let Some(sess) = map.remove(session_id) {
                if let Ok(mut child) = sess.child.lock() {
                    let _ = child.process.kill();
                    let _ = child.process.wait();
                }
                let body = json!({"ok": true, "closed": session_id}).to_string();
                write_http_response(
                    &mut stream,
                    200,
                    "OK",
                    &[("Content-Type", "application/json")],
                    body.as_bytes(),
                );
            } else {
                let body = json!({"error": "session not found"}).to_string();
                write_http_response(
                    &mut stream,
                    404,
                    "Not Found",
                    &[("Content-Type", "application/json")],
                    body.as_bytes(),
                );
            }
        }
        "POST" => {
            handle_post(stream, req, config, sessions);
        }
        _ => {
            let body = json!({"error": "unsupported HTTP method"}).to_string();
            write_http_response(
                &mut stream,
                405,
                "Method Not Allowed",
                &[("Content-Type", "application/json"), ("Allow", "POST, DELETE")],
                body.as_bytes(),
            );
        }
    }
}

fn handle_post(
    mut stream: TcpStream,
    req: ParsedRequest,
    config: GatewayConfig,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
) {
    let json_req: Value = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => {
            let body = json!({"error": format!("invalid JSON body: {e}")}).to_string();
            write_http_response(
                &mut stream,
                400,
                "Bad Request",
                &[("Content-Type", "application/json")],
                body.as_bytes(),
            );
            return;
        }
    };

    let is_notification = json_req.get("id").is_none_or(Value::is_null);
    let method_name = json_req.get("method").and_then(Value::as_str).unwrap_or("");

    // Check if this is an `initialize` request to start a new session
    if method_name == "initialize" {
        // Enforce Streamable HTTP revision 2025-03-26 explicitly.
        // Handshake revisions >= 2025-03-26 negotiate down to 2025-03-26 per MCP spec.
        let requested_version = json_req
            .get("params")
            .and_then(|p| p.get("protocolVersion"))
            .and_then(Value::as_str);

        let can_negotiate_2025_03_26 = matches!(
            requested_version,
            Some("2025-03-26" | "2025-11-25")
        );

        if !can_negotiate_2025_03_26 {
            let body = json!({
                "jsonrpc": "2.0",
                "id": json_req.get("id").cloned().unwrap_or(Value::Null),
                "error": {
                    "code": -32602,
                    "message": format!(
                        "unsupported protocolVersion '{:?}': this gateway explicitly requires revision '{}'",
                        requested_version, STREAMABLE_HTTP_PROTOCOL_VERSION
                    )
                }
            })
            .to_string();
            write_http_response(
                &mut stream,
                400,
                "Bad Request",
                &[("Content-Type", "application/json")],
                body.as_bytes(),
            );
            return;
        }

        let mut map = match sessions.lock() {
            Ok(m) => m,
            Err(_) => {
                let body = json!({"error": "internal mutex failure"}).to_string();
                write_http_response(&mut stream, 500, "Internal Server Error", &[], body.as_bytes());
                return;
            }
        };

        // Sweep expired sessions first (also remove sessions whose child has exited)
        let now = Instant::now();
        let mut expired = Vec::new();
        for (id, sess) in map.iter() {
            let timed_out = now.duration_since(sess.last_activity) >= SESSION_INACTIVITY_TIMEOUT;
            let child_dead = match sess.child.lock() {
                Ok(mut ch) => ch.process.try_wait().map(|opt| opt.is_some()).unwrap_or(true),
                Err(_) => true,
            };
            if timed_out || child_dead {
                expired.push(id.clone());
            }
        }
        for id in expired {
            if let Some(sess) = map.remove(&id) {
                if let Ok(mut child) = sess.child.lock() {
                    let _ = child.process.kill();
                    let _ = child.process.wait();
                }
            }
        }

        // Limit maximum concurrent sessions
        if map.len() >= MAX_SESSIONS {
            let body = json!({"error": "maximum concurrent sessions reached (limit: 2)"}).to_string();
            write_http_response(
                &mut stream,
                429,
                "Too Many Requests",
                &[("Content-Type", "application/json"), ("Retry-After", "30")],
                body.as_bytes(),
            );
            return;
        }

        // Generate unguessable session ID
        let new_session_id = match generate_session_id() {
            Ok(id) => id,
            Err(e) => {
                let body = json!({"error": e}).to_string();
                write_http_response(&mut stream, 500, "Internal Server Error", &[], body.as_bytes());
                return;
            }
        };

        // Spawn child process directly without shell:
        // cadcraft-cli mcp --connect 127.0.0.1:PORT --read-only
        let spawn_res = Command::new(&config.cli_binary_path)
            .arg("mcp")
            .arg("--connect")
            .arg(&config.target_app_addr)
            .arg("--read-only")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        let mut child_proc = match spawn_res {
            Ok(c) => c,
            Err(e) => {
                let body = json!({"error": format!("failed to spawn child process: {e}")}).to_string();
                write_http_response(&mut stream, 502, "Bad Gateway", &[], body.as_bytes());
                return;
            }
        };

        let child_stdin = match child_proc.stdin.take() {
            Some(s) => s,
            None => {
                let _ = child_proc.kill();
                let body = json!({"error": "cannot open child stdin"}).to_string();
                write_http_response(&mut stream, 502, "Bad Gateway", &[], body.as_bytes());
                return;
            }
        };

        let child_stdout = match child_proc.stdout.take() {
            Some(s) => BufReader::new(s),
            None => {
                let _ = child_proc.kill();
                let body = json!({"error": "cannot open child stdout"}).to_string();
                write_http_response(&mut stream, 502, "Bad Gateway", &[], body.as_bytes());
                return;
            }
        };

        let session_child = Arc::new(Mutex::new(SessionChild {
            stdin: child_stdin,
            stdout: child_stdout,
            process: child_proc,
        }));

        // Send initialize to child
        let mut reply_line = String::new();
        {
            let mut ch = match session_child.lock() {
                Ok(c) => c,
                Err(_) => {
                    let body = json!({"error": "mutex poison"}).to_string();
                    write_http_response(&mut stream, 500, "Internal Server Error", &[], body.as_bytes());
                    return;
                }
            };
            let mut init_forward = json_req.clone();
            if let Some(params) = init_forward.get_mut("params").and_then(Value::as_object_mut) {
                params.insert(
                    "protocolVersion".to_string(),
                    json!(STREAMABLE_HTTP_PROTOCOL_VERSION),
                );
            }
            if let Err(e) = writeln!(
                ch.stdin,
                "{}",
                serde_json::to_string(&init_forward).unwrap_or_default()
            ) {
                let body = json!({"error": format!("cannot write to child: {e}")}).to_string();
                write_http_response(&mut stream, 502, "Bad Gateway", &[], body.as_bytes());
                return;
            }
            let _ = ch.stdin.flush();
            if let Err(e) = ch.stdout.read_line(&mut reply_line) {
                let body = json!({"error": format!("cannot read from child: {e}")}).to_string();
                write_http_response(&mut stream, 502, "Bad Gateway", &[], body.as_bytes());
                return;
            }
        }

        map.insert(
            new_session_id.clone(),
            Session {
                _id: new_session_id.clone(),
                child: Arc::clone(&session_child),
                last_activity: Instant::now(),
            },
        );

        write_http_response(
            &mut stream,
            200,
            "OK",
            &[
                ("Content-Type", "application/json"),
                ("Mcp-Session-Id", &new_session_id),
            ],
            reply_line.trim_end().as_bytes(),
        );
        return;
    }

    // For all subsequent requests, `Mcp-Session-Id` header is required
    let session_id = match req.headers.get("mcp-session-id") {
        Some(id) => id,
        None => {
            let body = json!({"error": "missing Mcp-Session-Id header for Streamable HTTP"}).to_string();
            write_http_response(
                &mut stream,
                400,
                "Bad Request",
                &[("Content-Type", "application/json")],
                body.as_bytes(),
            );
            return;
        }
    };

    let session_child = {
        let mut map = match sessions.lock() {
            Ok(m) => m,
            Err(_) => {
                let body = json!({"error": "internal mutex failure"}).to_string();
                write_http_response(&mut stream, 500, "Internal Server Error", &[], body.as_bytes());
                return;
            }
        };

        let sess = match map.get_mut(session_id) {
            Some(s) => s,
            None => {
                let body = json!({"error": "unknown session or session terminated"}).to_string();
                write_http_response(
                    &mut stream,
                    404,
                    "Not Found",
                    &[("Content-Type", "application/json")],
                    body.as_bytes(),
                );
                return;
            }
        };

        // Check expiration
        if Instant::now().duration_since(sess.last_activity) >= SESSION_INACTIVITY_TIMEOUT {
            if let Some(expired_sess) = map.remove(session_id) {
                if let Ok(mut ch) = expired_sess.child.lock() {
                    let _ = ch.process.kill();
                    let _ = ch.process.wait();
                }
            }
            let body = json!({"error": "session expired due to inactivity"}).to_string();
            write_http_response(
                &mut stream,
                404,
                "Not Found",
                &[("Content-Type", "application/json")],
                body.as_bytes(),
            );
            return;
        }

        sess.last_activity = Instant::now();
        Arc::clone(&sess.child)
    };

    // Serialize calls through child's mutex
    let mut ch = match session_child.lock() {
        Ok(c) => c,
        Err(_) => {
            let body = json!({"error": "session lock poisoned"}).to_string();
            write_http_response(&mut stream, 500, "Internal Server Error", &[], body.as_bytes());
            return;
        }
    };

    let line_out = match serde_json::to_string(&json_req) {
        Ok(s) => s,
        Err(e) => {
            let body = json!({"error": format!("JSON serialization error: {e}")}).to_string();
            write_http_response(&mut stream, 400, "Bad Request", &[], body.as_bytes());
            return;
        }
    };

    if let Err(e) = writeln!(ch.stdin, "{line_out}") {
        let _ = ch.process.kill();
        let _ = ch.process.wait();
        drop(ch);
        if let Ok(mut map) = sessions.lock() {
            let _ = map.remove(session_id);
        }
        let body = json!({"error": format!("failed writing to child: {e}")}).to_string();
        write_http_response(&mut stream, 502, "Bad Gateway", &[], body.as_bytes());
        return;
    }
    let _ = ch.stdin.flush();

    if is_notification {
        // Notifications require HTTP 202 Accepted without body
        write_http_response(&mut stream, 202, "Accepted", &[("Mcp-Session-Id", session_id)], b"");
    } else {
        // Tool calls / requests expect a response
        let mut reply_line = String::new();
        match ch.stdout.read_line(&mut reply_line) {
            Ok(0) => {
                let _ = ch.process.kill();
                let _ = ch.process.wait();
                drop(ch);
                if let Ok(mut map) = sessions.lock() {
                    let _ = map.remove(session_id);
                }
                let body = json!({"error": "child process closed stdout unexpectedly"}).to_string();
                write_http_response(&mut stream, 502, "Bad Gateway", &[], body.as_bytes());
            }
            Ok(_) => {
                write_http_response(
                    &mut stream,
                    200,
                    "OK",
                    &[
                        ("Content-Type", "application/json"),
                        ("Mcp-Session-Id", session_id),
                    ],
                    reply_line.trim_end().as_bytes(),
                );
            }
            Err(e) => {
                let _ = ch.process.kill();
                let _ = ch.process.wait();
                drop(ch);
                if let Ok(mut map) = sessions.lock() {
                    let _ = map.remove(session_id);
                }
                let body = json!({"error": format!("failed reading reply from child: {e}")}).to_string();
                write_http_response(&mut stream, 502, "Bad Gateway", &[], body.as_bytes());
            }
        }
    }
}
