//! Local sketch compiler + static demo server.

use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use avr8rs::{compile_source, UNO_FQBN};

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_SKETCH_BYTES: usize = 512 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct Options {
    port: u16,
    web_dir: PathBuf,
}

fn usage() -> &'static str {
    "usage: avr8rs-server [--port 8080] [--web-dir web] [--board arduino:avr:uno]"
}

fn parse_args() -> Result<Options, String> {
    let mut port = 8080u16;
    let mut web_dir = PathBuf::from("web");
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "--port" => {
                port = args
                    .next()
                    .ok_or("--port requires a number")?
                    .parse()
                    .map_err(|_| "--port must be an integer")?
            }
            "--web-dir" => web_dir = PathBuf::from(args.next().ok_or("--web-dir requires a path")?),
            "--board" => {
                let board = args.next().ok_or("--board requires an FQBN")?;
                if board != UNO_FQBN {
                    return Err(format!("unsupported board {board}; expected {UNO_FQBN}"));
                }
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Options { port, web_dir })
}

fn main() -> ExitCode {
    let mut options = match parse_args() {
        Ok(options) => options,
        Err(message) if message.is_empty() => {
            println!("{}", usage());
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}\n{}", usage());
            return ExitCode::from(2);
        }
    };
    let listener = match TcpListener::bind(("127.0.0.1", options.port)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("cannot bind port {}: {error}", options.port);
            return ExitCode::FAILURE;
        }
    };
    match listener.local_addr() {
        Ok(addr) => options.port = addr.port(),
        Err(error) => {
            eprintln!("cannot determine listen address: {error}");
            return ExitCode::FAILURE;
        }
    }
    println!(
        "avr8rs server: http://127.0.0.1:{} (board {UNO_FQBN})",
        options.port
    );
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle(Connection::new(stream), &options) {
                    eprintln!("request failed: {error}");
                }
            }
            Err(error) => eprintln!("accept failed: {error}"),
        }
    }
    ExitCode::SUCCESS
}

struct Connection {
    stream: TcpStream,
    read_deadline: Instant,
    write_deadline: Option<Instant>,
}

impl Connection {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            read_deadline: Instant::now() + IO_TIMEOUT,
            write_deadline: None,
        }
    }
}

fn time_remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "HTTP connection timed out"))
}

impl Read for Connection {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.stream
            .set_read_timeout(Some(time_remaining(self.read_deadline)?))?;
        self.stream.read(buffer)
    }
}

impl Write for Connection {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let deadline = *self
            .write_deadline
            .get_or_insert_with(|| Instant::now() + IO_TIMEOUT);
        self.stream
            .set_write_timeout(Some(time_remaining(deadline)?))?;
        self.stream.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

struct Request<'a> {
    method: &'a str,
    path: &'a str,
    host: Option<&'a str>,
    origin: Option<&'a str>,
    content_length: Option<usize>,
}

fn parse_request(header: &[u8]) -> Option<Request<'_>> {
    let mut lines = std::str::from_utf8(header).ok()?.split("\r\n");
    let mut request_line = lines.next()?.split(' ');
    let method = request_line.next()?;
    let target = request_line.next()?;
    let version = request_line.next()?;
    if method.is_empty()
        || !target.starts_with('/')
        || target.starts_with("//")
        || target.chars().any(char::is_control)
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || request_line.next().is_some()
    {
        return None;
    }
    let path = target.split_once('?').map_or(target, |(path, _)| path);
    let mut request = Request {
        method,
        path,
        host: None,
        origin: None,
        content_length: None,
    };
    for line in lines {
        let (name, value) = line.split_once(':')?;
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return None;
        }
        let value = value.trim_matches([' ', '\t']);
        let duplicate = if name.eq_ignore_ascii_case("Host") {
            request.host.replace(value).is_some()
        } else if name.eq_ignore_ascii_case("Origin") {
            request.origin.replace(value).is_some()
        } else if name.eq_ignore_ascii_case("Content-Length") {
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            request
                .content_length
                .replace(value.parse().ok()?)
                .is_some()
        } else {
            name.eq_ignore_ascii_case("Transfer-Encoding")
        };
        if duplicate {
            return None;
        }
    }
    Some(request)
}

fn loopback_authority(authority: &str, port: u16) -> Option<&str> {
    let (host, request_port) = match authority.split_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, port.parse().ok()?)
        }
        Some(_) => return None,
        None => (authority, 80),
    };
    if request_port == port && (host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1") {
        Some(host)
    } else {
        None
    }
}

fn handle(mut stream: impl Read + Write, options: &Options) -> io::Result<()> {
    let mut head = Vec::new();
    let mut buffer = [0u8; 8192];
    let split = loop {
        if head.len() == MAX_HEADER_BYTES {
            return respond(&mut stream, 431, "text/plain", b"request too large");
        }
        let scan_start = head.len().saturating_sub(3);
        let max_read = buffer.len().min(MAX_HEADER_BYTES - head.len());
        let read = stream.read(&mut buffer[..max_read])?;
        if read == 0 {
            return if head.is_empty() {
                Ok(())
            } else {
                respond(&mut stream, 400, "text/plain", b"incomplete request")
            };
        }
        head.extend_from_slice(&buffer[..read]);
        if let Some(split) = head[scan_start..]
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
        {
            break scan_start + split;
        }
    };
    let Some(request) = parse_request(&head[..split]) else {
        return respond(&mut stream, 400, "text/plain", b"invalid HTTP request");
    };
    let Some(host) = request
        .host
        .and_then(|host| loopback_authority(host, options.port))
    else {
        return respond(&mut stream, 403, "text/plain", b"invalid request host");
    };
    if request.method == "POST" && request.path == "/compile" {
        if request.origin.is_some_and(|origin| {
            !origin
                .strip_prefix("http://")
                .and_then(|origin| loopback_authority(origin, options.port))
                .is_some_and(|origin_host| origin_host.eq_ignore_ascii_case(host))
        }) {
            return respond(&mut stream, 403, "text/plain", b"invalid request origin");
        }
        let Some(content_length) = request.content_length else {
            return respond(
                &mut stream,
                411,
                "text/plain",
                b"Content-Length is required",
            );
        };
        if content_length > MAX_SKETCH_BYTES {
            return respond(&mut stream, 413, "text/plain", b"sketch too large");
        }
        let received = &head[split + 4..];
        let received = &received[..received.len().min(content_length)];
        let mut body = vec![0; content_length];
        body[..received.len()].copy_from_slice(received);
        if let Err(error) = stream.read_exact(&mut body[received.len()..]) {
            return if error.kind() == io::ErrorKind::UnexpectedEof {
                respond(&mut stream, 400, "text/plain", b"incomplete request body")
            } else {
                Err(error)
            };
        }
        let source = match std::str::from_utf8(&body) {
            Ok(source) => source,
            Err(_) => return respond(&mut stream, 400, "text/plain", b"sketch must be UTF-8"),
        };
        match compile_source("sketch", source, UNO_FQBN) {
            Ok(bytes) => return respond(&mut stream, 200, "application/octet-stream", &bytes),
            Err(error) => {
                let message = format!("compile failed: {error}\n");
                return respond(&mut stream, 400, "text/plain", message.as_bytes());
            }
        }
    }
    if request.method == "GET" {
        return serve_static(&mut stream, &options.web_dir, request.path);
    }
    respond(&mut stream, 405, "text/plain", b"method not allowed")
}

fn serve_static(stream: &mut impl Write, web_dir: &Path, path: &str) -> std::io::Result<()> {
    let Some(relative) = path.strip_prefix('/') else {
        return respond(stream, 400, "text/plain", b"invalid request path");
    };
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    if relative.contains(['\\', ':']) {
        return respond(stream, 403, "text/plain", b"forbidden");
    }
    let relative = Path::new(relative);
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return respond(stream, 403, "text/plain", b"forbidden");
    }
    let Ok(root) = web_dir.canonicalize() else {
        return respond(stream, 404, "text/plain", b"not found");
    };
    let Ok(file) = root.join(relative).canonicalize() else {
        return respond(stream, 404, "text/plain", b"not found");
    };
    if !file.starts_with(&root) {
        return respond(stream, 403, "text/plain", b"forbidden");
    }
    if !file.is_file() {
        return respond(stream, 404, "text/plain", b"not found");
    }
    match fs::read(&file) {
        Ok(bytes) => {
            let mime = match file.extension().and_then(|ext| ext.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("js") => "text/javascript; charset=utf-8",
                Some("css") => "text/css; charset=utf-8",
                Some("wasm") => "application/wasm",
                Some("json") => "application/json",
                _ => "application/octet-stream",
            };
            respond(stream, 200, mime, &bytes)
        }
        Err(_) => respond(stream, 404, "text/plain", b"not found"),
    }
}

fn respond(
    stream: &mut impl Write,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Result};

    #[derive(Default)]
    struct Stream {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl Read for &mut Stream {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for &mut Stream {
        fn write(&mut self, buf: &[u8]) -> Result<usize> {
            self.output.write(buf)
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }
    }

    fn check_request(request: &str, expected_status: u16) {
        let mut stream = Stream {
            input: Cursor::new(request.as_bytes().to_vec()),
            ..Stream::default()
        };
        let options = Options {
            port: 8080,
            web_dir: PathBuf::from("web"),
        };
        handle(&mut stream, &options).unwrap();
        let response = String::from_utf8_lossy(&stream.output);
        assert!(
            response.starts_with(&format!("HTTP/1.1 {expected_status} ")),
            "{response}"
        );
    }

    #[test]
    fn rejects_foreign_host_and_origin() {
        check_request(
            "POST /compile HTTP/1.1\r\nHost: foreign.invalid:8080\r\nContent-Length: 0\r\n\r\n",
            403,
        );
        check_request(
            "POST /compile HTTP/1.1\r\nHost: localhost:8080\r\nOrigin: http://localhost:8081\r\nContent-Length: 0\r\n\r\n",
            403,
        );
    }

    #[test]
    fn rejects_oversized_declared_body_before_reading_it() {
        check_request(
            "POST /compile HTTP/1.1\r\nHost: localhost:8080\r\nContent-Length: 524289\r\n\r\n",
            413,
        );
    }

    #[test]
    fn rejects_request_with_non_path_target() {
        check_request("GET é HTTP/1.1\r\nHost: localhost:8080\r\n\r\n", 400);
    }

    #[test]
    fn rejects_ambiguous_request_framing() {
        check_request(
            "GET / HTTP/1.1\r\nHost: localhost:8080\r\nContent-Length: 0\r\nContent-Length: 1\r\n\r\n",
            400,
        );
        check_request(
            "GET / HTTP/1.1\r\nHost: localhost:8080\r\nTransfer-Encoding: chunked\r\n\r\n",
            400,
        );
    }

    #[test]
    fn accepts_local_static_requests_with_query_strings() {
        check_request("GET /?reload HTTP/1.1\r\nHost: localhost:8080\r\n\r\n", 200);
    }
}
