//! Local sketch compiler + static demo server.

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use avr8rs::{compile_source, UNO_FQBN};

#[derive(Debug)]
struct Options {
    port: u16,
    web_dir: PathBuf,
    board: String,
}

fn usage() -> &'static str {
    "usage: avr8rs-server [--port 8080] [--web-dir web] [--board arduino:avr:uno]"
}

fn parse_args() -> Result<Options, String> {
    let mut port = 8080u16;
    let mut web_dir = PathBuf::from("web");
    let mut board = UNO_FQBN.to_string();
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
            "--board" => board = args.next().ok_or("--board requires an FQBN")?,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Options {
        port,
        web_dir,
        board,
    })
}

fn main() -> ExitCode {
    let options = match parse_args() {
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
    println!(
        "avr8rs server: http://127.0.0.1:{} (board {})",
        options.port, options.board
    );
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle(stream, &options) {
                    eprintln!("request failed: {error}");
                }
            }
            Err(error) => eprintln!("accept failed: {error}"),
        }
    }
    ExitCode::SUCCESS
}

fn handle(mut stream: TcpStream, options: &Options) -> std::io::Result<()> {
    let mut head = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        head.extend_from_slice(&buffer[..read]);
        if head.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if head.len() > 64 * 1024 {
            return respond(&mut stream, 431, "text/plain", b"request too large");
        }
    }
    let split = head
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    let head_text = String::from_utf8_lossy(&head[..split]);
    let mut lines = head_text.lines();
    let request = lines.next().unwrap_or_default();
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or("/");
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = head[split + 4..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..read]);
    }

    if method == "POST" && path == "/compile" {
        if body.len() > 512 * 1024 {
            return respond(&mut stream, 413, "text/plain", b"sketch too large");
        }
        let source = match std::str::from_utf8(&body) {
            Ok(source) => source,
            Err(_) => return respond(&mut stream, 400, "text/plain", b"sketch must be UTF-8"),
        };
        match compile_source("sketch", source, &options.board) {
            Ok(bytes) => return respond(&mut stream, 200, "application/octet-stream", &bytes),
            Err(error) => {
                let message = format!("compile failed: {error}\n");
                return respond(&mut stream, 400, "text/plain", message.as_bytes());
            }
        }
    }
    if method == "GET" {
        return serve_static(&mut stream, &options.web_dir, path);
    }
    respond(&mut stream, 405, "text/plain", b"method not allowed")
}

fn serve_static(stream: &mut TcpStream, web_dir: &Path, path: &str) -> std::io::Result<()> {
    let relative = if path == "/" {
        "index.html"
    } else {
        &path[1..]
    };
    let relative = Path::new(relative);
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return respond(stream, 403, "text/plain", b"forbidden");
    }
    let file = web_dir.join(relative);
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
    stream: &mut TcpStream,
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
