//! The ESP-IDF HTTP server in front of the portable web handler.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use bdlminer_app::multipart::{self, Event};
use bdlminer_app::update::FirmwareWriter;
use bdlminer_app::web::{self, Method, Request};
use bdlminer_app::App;
use esp_idf_svc::http::server::{Configuration, EspHttpConnection, EspHttpServer};
use esp_idf_svc::http::Method as HttpMethod;
use esp_idf_svc::io::{Read, Write};

use crate::ota;

const MAX_BODY: usize = 16 * 1024;

fn serve(
    app: &App,
    mut req: esp_idf_svc::http::server::Request<&mut EspHttpConnection>,
    method: Method,
) -> anyhow::Result<()> {
    let uri = req.uri().to_owned();
    let (path, query) = uri.split_once('?').unwrap_or((&uri, ""));
    let host = req.header("Host").map(str::to_owned);
    let origin = req.header("Origin").map(str::to_owned);
    let authorization = req.header("Authorization").map(str::to_owned);
    if method == Method::Post && path == "/update" {
        let content_type = req.header("Content-Type").unwrap_or("").to_owned();
        let request = Request {
            method,
            path,
            query,
            host: host.as_deref(),
            origin: origin.as_deref(),
            authorization: authorization.as_deref(),
            body: &[],
        };
        return upload(app, req, &request, &content_type);
    }
    let length = req.header("Content-Length").and_then(|l| l.parse::<usize>().ok()).unwrap_or(0);
    if length > MAX_BODY {
        req.into_response(413, None, &[("Content-Type", "text/plain")])?.write_all(b"Request too large")?;
        return Ok(());
    }
    let mut body = vec![0u8; length];
    let mut got = 0;
    while got < length {
        let n = req.read(&mut body[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    body.truncate(got);
    let request = Request {
        method,
        path,
        query,
        host: host.as_deref(),
        origin: origin.as_deref(),
        authorization: authorization.as_deref(),
        body: &body,
    };
    let response = web::handle(app, &request);
    let mut headers: Vec<(&str, &str)> = vec![("Content-Type", response.content_type)];
    for (name, value) in &response.headers {
        headers.push((name, value.as_str()));
    }
    req.into_response(response.status, None, &headers)?.write_all(&response.body)?;
    Ok(())
}

fn reply(
    req: esp_idf_svc::http::server::Request<&mut EspHttpConnection>,
    response: &web::Response,
) -> anyhow::Result<()> {
    let mut headers: Vec<(&str, &str)> = vec![("Content-Type", response.content_type)];
    for (name, value) in &response.headers {
        headers.push((name, value.as_str()));
    }
    req.into_response(response.status, None, &headers)?.write_all(&response.body)?;
    Ok(())
}

/// POST /update (multipart): one firmware .bin built for this board, written
/// into the other slot as it arrives. The new image is selected only once the
/// whole request was read and the image checked.
fn upload(
    app: &App,
    mut req: esp_idf_svc::http::server::Request<&mut EspHttpConnection>,
    request: &Request,
    content_type: &str,
) -> anyhow::Result<()> {
    let text = |status: u16, body: &str| web::Response::new(status, "text/plain", body);
    if let Err(refused) = web::update_allowed(app, request) {
        return reply(req, &refused);
    }
    let Some(mut parser) = multipart::Parser::new(content_type) else {
        return reply(req, &text(400, "No firmware file uploaded"));
    };
    app.ota_busy.store(true, Ordering::Relaxed);
    app.miner.set_ota_busy(true);
    let mut writer: Option<Box<dyn FirmwareWriter>> = None;
    let (mut files, mut size, mut status) = (0, 0usize, 400u16);
    let mut writing = false;
    let mut failure: Option<String> = None;
    let mut buf = vec![0u8; 4096];
    loop {
        let n = match req.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                status = 500;
                failure = Some(format!("Update failed: {e}"));
                break;
            }
        };
        if n == 0 {
            break;
        }
        let fed = parser.feed(&buf[..n], &mut |event| match event {
            Event::Start { filename } => {
                writing = !filename.is_empty();
                if !writing {
                    return Ok(());
                }
                files += 1;
                if files > 1 {
                    return Err("Only one firmware file is allowed".into());
                }
                let name = filename.to_lowercase();
                if !name.ends_with(".bin") || name.ends_with("factory.bin") {
                    return Err("Use a firmware .bin file, not a factory image".into());
                }
                app.log(&format!("[web] firmware update: {filename}"));
                status = 500;
                writer = Some(ota::begin().map_err(|e| format!("Update failed: {e}"))?);
                Ok(())
            }
            Event::Data(data) if writing => {
                if let Some(w) = writer.as_mut() {
                    w.write(data).map_err(|e| format!("Update failed: {e}"))?;
                    size += data.len();
                }
                Ok(())
            }
            Event::Data(_) => Ok(()),
            Event::End => {
                writing = false;
                Ok(())
            }
        });
        if let Err(e) = fed {
            failure = Some(e);
            break;
        }
        if parser.finished() {
            break;
        }
        // The screen and the miners keep running.
        std::thread::yield_now();
    }
    let outcome = match (failure, writer) {
        (Some(why), _) => Err((status, why)),
        (None, None) => Err((400, "No firmware file uploaded".into())),
        (None, Some(_)) if size == 0 => Err((400, "Empty firmware file".into())),
        (None, Some(_)) if !parser.finished() => Err((500, "Update failed: Incomplete firmware upload".into())),
        (None, Some(w)) => {
            w.finish().map_err(|e| (500, format!("Update failed: {e} (this board may require a USB update)")))
        }
    };
    match outcome {
        Ok(()) => {
            app.log("[web] firmware validated, restarting");
            ota::confirm(app, "a newer firmware is installed");
            app.request_restart("firmware updated from the web page");
            // ota_busy stays set: nothing else touches the new image before the restart.
            reply(req, &text(200, "Firmware updated, restarting\u{2026} reload in 20 s"))
        }
        Err((status, why)) => {
            app.log(&format!("[web] {why}"));
            app.miner.set_ota_busy(false);
            app.ota_busy.store(false, Ordering::Relaxed);
            reply(req, &text(status, &why))
        }
    }
}

pub fn start(app: Arc<App>) -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&Configuration {
        uri_match_wildcard: true,
        stack_size: 12 * 1024,
        max_open_sockets: 6,
        lru_purge_enable: true,
        ..Default::default()
    })?;
    let get = app.clone();
    server.fn_handler("/*", HttpMethod::Get, move |req| serve(&get, req, Method::Get))?;
    let post = app;
    server.fn_handler("/*", HttpMethod::Post, move |req| serve(&post, req, Method::Post))?;
    Ok(server)
}
