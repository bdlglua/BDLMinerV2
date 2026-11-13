//! The ESP-IDF HTTP server in front of the portable web handler.

use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use bdlminer_app::multipart::{self, Event};
use bdlminer_app::update::FirmwareWriter;
use bdlminer_app::web::{self, Method, Request};
use bdlminer_app::App;
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::http::server::{Configuration, EspHttpConnection, EspHttpServer};
use esp_idf_svc::http::Method as HttpMethod;
use esp_idf_svc::io::{Read, Write};
use esp_idf_svc::sys::{
    esp_err_t, http_method_HTTP_GET, httpd_register_uri_handler, httpd_req_async_handler_begin,
    httpd_req_async_handler_complete, httpd_req_get_hdr_value_len, httpd_req_get_hdr_value_str, httpd_req_t,
    httpd_resp_send, httpd_resp_send_chunk, httpd_resp_set_hdr, httpd_resp_set_status, httpd_resp_set_type,
    httpd_uri_t, ESP_FAIL, ESP_OK,
};

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
    if method == Method::Get && path == "/api/coredump" && app.crashes.present() {
        let request = Request {
            method,
            path,
            query,
            host: host.as_deref(),
            origin: origin.as_deref(),
            authorization: authorization.as_deref(),
            body: &[],
        };
        // The crash report is a memory image, which can hold passwords.
        if let Err(refused) = web::owner_allowed(app, &request) {
            return reply(req, &refused);
        }
        let size = app.crashes.size();
        let length = size.to_string();
        let headers = [
            ("Content-Type", "application/octet-stream"),
            ("Content-Disposition", "attachment; filename=\"bdlminer-coredump.elf\""),
            ("Cache-Control", "no-store"),
            ("Content-Length", length.as_str()),
        ];
        let mut response = req.into_response(200, None, &headers)?;
        let mut chunk = [0u8; 1024];
        let mut offset = 0;
        while offset < size {
            let n = chunk.len().min(size - offset);
            if !app.crashes.read(offset, &mut chunk[..n]) {
                break;
            }
            response.write_all(&chunk[..n])?;
            offset += n;
        }
        return Ok(());
    }
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

/// Live dashboards (/api/events): requests handed over by the server
/// (httpd_req_async_handler_begin), written to every 2 s by their own task.
struct Stream(*mut httpd_req_t);
// The copied request belongs to the events task once handed over.
unsafe impl Send for Stream {}

/// One live dashboard: each holds a socket and a stats answer every 2 s.
const MAX_STREAMS: usize = 1;
const STREAM_MS: u64 = 2000;
static STREAMS: Mutex<Vec<Stream>> = Mutex::new(Vec::new());
static STREAM_DUE: AtomicBool = AtomicBool::new(false);
static EVENTS_APP: OnceLock<Arc<App>> = OnceLock::new();

unsafe fn send_text(req: *mut httpd_req_t, status: &CStr, text: &str) -> esp_err_t {
    httpd_resp_set_status(req, status.as_ptr());
    httpd_resp_set_type(req, c"text/plain".as_ptr());
    httpd_resp_send(req, text.as_ptr().cast(), text.len() as _)
}

unsafe extern "C" fn events_handler(req: *mut httpd_req_t) -> esp_err_t {
    let Some(app) = EVENTS_APP.get() else { return ESP_FAIL };
    let length = httpd_req_get_hdr_value_len(req, c"Host".as_ptr());
    let mut host = vec![0u8; length + 1];
    let host = (length > 0
        && httpd_req_get_hdr_value_str(req, c"Host".as_ptr(), host.as_mut_ptr().cast(), host.len()) == ESP_OK)
        .then(|| String::from_utf8_lossy(&host[..length]).into_owned());
    if !web::events_allowed(app, host.as_deref()) {
        return send_text(req, c"403 Forbidden", "Refused: open the miner's own page (http://its address/) to do this");
    }
    let mut streams = STREAMS.lock().unwrap();
    if streams.len() >= MAX_STREAMS {
        return send_text(req, c"503 Service Unavailable", "Too many live dashboards; polling instead");
    }
    let mut copy: *mut httpd_req_t = std::ptr::null_mut();
    if httpd_req_async_handler_begin(req, &mut copy) != ESP_OK {
        return send_text(req, c"503 Service Unavailable", "Too many live dashboards; polling instead");
    }
    httpd_resp_set_type(copy, c"text/event-stream".as_ptr());
    httpd_resp_set_hdr(copy, c"Cache-Control".as_ptr(), c"no-store".as_ptr());
    let first = "retry: 5000\n\n";
    if httpd_resp_send_chunk(copy, first.as_ptr().cast(), first.len() as _) != ESP_OK {
        httpd_req_async_handler_complete(copy);
        return ESP_OK;
    }
    streams.push(Stream(copy));
    // The pushing task starts with the first live dashboard, not at boot.
    static PUSHER: std::sync::Once = std::sync::Once::new();
    PUSHER.call_once(|| {
        let app = app.clone();
        if std::thread::Builder::new().stack_size(6 * 1024).spawn(move || push_events(&app)).is_err() {
            log::warn!("[web] live dashboard task could not start");
        }
    });
    // A first snapshot right away.
    STREAM_DUE.store(true, Ordering::Relaxed);
    ESP_OK
}

/// Writes the readings to every live dashboard; one that fails is dropped
/// (its browser reconnects).
fn push_events(app: &App) {
    let mut last = std::time::Instant::now();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !STREAM_DUE.swap(false, Ordering::Relaxed) && last.elapsed().as_millis() < STREAM_MS as u128 {
            continue;
        }
        last = std::time::Instant::now();
        if STREAMS.lock().unwrap().is_empty() {
            continue;
        }
        let message = web::event_message(app);
        let mut streams = STREAMS.lock().unwrap();
        streams.retain(|s| unsafe {
            let ok = httpd_resp_send_chunk(s.0, message.as_ptr().cast(), message.len() as _) == ESP_OK;
            if !ok {
                httpd_req_async_handler_complete(s.0);
            }
            ok
        });
    }
}

pub fn start(app: Arc<App>) -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&Configuration {
        uri_match_wildcard: true,
        stack_size: 12 * 1024,
        // One more than the default for the live dashboard.
        max_open_sockets: 7,
        lru_purge_enable: true,
        ..Default::default()
    })?;
    // Before the catch-all handlers: the first match serves a request.
    if EVENTS_APP.set(app.clone()).is_ok() {
        let uri = httpd_uri_t {
            uri: c"/api/events".as_ptr(),
            method: http_method_HTTP_GET as _,
            handler: Some(events_handler),
            user_ctx: std::ptr::null_mut(),
            ..unsafe { std::mem::zeroed() }
        };
        if unsafe { httpd_register_uri_handler(server.handle(), &uri) } != ESP_OK {
            log::warn!("[web] live dashboard unavailable");
        }
    }
    let get = app.clone();
    server.fn_handler("/*", HttpMethod::Get, move |req| serve(&get, req, Method::Get))?;
    let post = app;
    server.fn_handler("/*", HttpMethod::Post, move |req| serve(&post, req, Method::Post))?;
    Ok(server)
}
