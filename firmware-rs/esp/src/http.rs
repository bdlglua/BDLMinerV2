//! The ESP-IDF HTTP server in front of the portable web handler.

use std::sync::Arc;

use bdlminer_app::web::{self, Method, Request};
use bdlminer_app::App;
use esp_idf_svc::http::server::{Configuration, EspHttpConnection, EspHttpServer};
use esp_idf_svc::http::Method as HttpMethod;
use esp_idf_svc::io::Write;

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
