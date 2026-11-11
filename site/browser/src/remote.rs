//! Read-only private monitoring. Access values remain in memory, not URLs/storage.
use crate::dom::*;
use serde_json::{json, Value};
use std::{cell::RefCell, sync::LazyLock};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};
static FR: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("remote.json")).unwrap());
thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
#[derive(Default)]
struct State {
    access: Option<Access>,
    version: u64,
    history_version: u64,
    timer: i32,
    fleet: Option<Value>,
    series: Option<Value>,
    metric_temp: bool,
    hover: Option<usize>,
    plot: Option<Plot>,
}
#[derive(Clone)]
struct Access {
    key: bool,
    value: String,
}
#[derive(Clone)]
struct Point {
    index: usize,
    t: f64,
    value: f64,
    row: Value,
}
struct Plot {
    points: Vec<Point>,
    left: f64,
    right: f64,
    start: f64,
    end: f64,
}
fn with(f: impl FnOnce(&mut State)) {
    STATE.with(|s| f(&mut s.borrow_mut()));
}
fn t(value: &str) -> String {
    if !lang() {
        FR[value].as_str().unwrap_or(value).into()
    } else {
        value.into()
    }
}
fn id(value: &str) -> String {
    format!("rm-{value}")
}
fn out(target: &str, value: &str) {
    text(&id(target), value);
}
fn hide(target: &str, value: bool) {
    hidden(&id(target), value);
}
fn val(target: &str) -> String {
    value(&id(target))
}
fn translate() {
    for el in all("#remote [data-i]") {
        el.set_text_content(Some(&t(&el.get_attribute("data-i").unwrap_or_default())));
    }
}
fn address(value: &str) -> Option<String> {
    let a = value.trim().split('.').next().unwrap_or_default();
    regex::Regex::new(r"^[A-Za-z0-9:_-]{4,100}$")
        .unwrap()
        .is_match(a)
        .then(|| a.into())
}
fn duration(value: &Value) -> String {
    let n = finite(value).unwrap_or(0.0);
    if n < 60.0 {
        format!("{} s", n.round())
    } else if n < 3600.0 {
        format!("{} min", (n / 60.0).round())
    } else if n < 86400.0 {
        format!("{:.1} h", n / 3600.0)
    } else {
        format!("{:.1} d", n / 86400.0)
    }
}
fn temperature(value: &Value) -> String {
    value
        .as_f64()
        .filter(|n| n.is_finite())
        .map(|n| format(n, json!({"maximumFractionDigits":1})) + " °C")
        .unwrap_or_else(|| "—".into())
}
fn stamp(value: &Value) -> String {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .map(|v| {
            let stamp = js_sys::Date::new(&v.into())
                .to_iso_string()
                .as_string()
                .unwrap_or_default();
            date(
                &json!(stamp),
                json!({"day":"2-digit","month":"short","hour":"2-digit","minute":"2-digit"}),
            )
        })
        .unwrap_or_else(|| "—".into())
}
fn clear_history(s: &mut State) {
    s.series = None;
    s.hover = None;
    s.plot = None;
    hide("chart-tooltip", true);
    for key in [
        "stat-average",
        "stat-maximum",
        "stat-minimum",
        "stat-temperature",
        "stat-temperature-max",
        "stat-reports",
        "stat-restarts",
        "stat-variation",
    ] {
        out(key, "—");
    }
    out("history-range", "");
    html(&id("history-rows"), "");
    disabled(&id("export-history"), true);
    if let Some(canvas) = get(&id("chart")).and_then(|e| e.dyn_into::<HtmlCanvasElement>().ok()) {
        if let Some(ctx) = canvas
            .get_context("2d")
            .ok()
            .flatten()
            .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok())
        {
            ctx.clear_rect(0.0, 0.0, canvas.width().into(), canvas.height().into());
        }
    }
}
fn lock_state(s: &mut State) {
    if let Some(root) = get("remote") {
        class(&root, "on", false);
    }
    s.version += 1;
    s.history_version += 1;
    s.access = None;
    s.fleet = None;
    clear_history(s);
    window().clear_timeout_with_handle(s.timer);
    set_value(&id("key"), "");
    hide("login", false);
    for key in ["logout", "history", "manage", "empty"] {
        hide(key, true);
    }
    html(&id("miners"), "");
    html(&id("history-miner"), "");
    out("total", "—");
    out("updated", "");
    out("notice", "");
    translate();
}
async fn api(path: &str, method: &str) -> Result<Value, String> {
    let mut credentials = None;
    with(|s| credentials = s.access.clone().map(|a| (s.version, a)));
    let Some((version, access)) = credentials else {
        return Err(t("Connection closed"));
    };
    let origin = query("meta[name=bdl-remote-api]")
        .and_then(|e| e.get_attribute("content"))
        .unwrap_or_else(|| current_url().origin());
    let options = web_sys::RequestInit::new();
    options.set_method(method);
    options.set_credentials(web_sys::RequestCredentials::Omit);
    options.set_redirect(web_sys::RequestRedirect::Error);
    options.set_cache(web_sys::RequestCache::NoStore);
    let signal = web_sys::AbortSignal::timeout_with_u32(15000);
    options.set_signal(Some(&signal));
    let headers = web_sys::Headers::new().map_err(|_| t("Connection closed"))?;
    let _ = headers.set(
        "Authorization",
        &format!(
            "{} {}",
            if access.key { "Bearer" } else { "Address" },
            access.value
        ),
    );
    options.set_headers(&headers);
    let response = JsFuture::from(window().fetch_with_str_and_init(&(origin + path), &options))
        .await
        .map_err(|_| t("Connection failed. Last readings may be out of date."))?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| t("Connection closed"))?;
    let data = if response.status() == 204 {
        Value::Null
    } else {
        json(
            &JsFuture::from(response.json().map_err(|_| t("Connection closed"))?)
                .await
                .map_err(|_| t("Connection closed"))?,
        )
    };
    let mut current = false;
    with(|s| current = s.version == version);
    if !current {
        return Err(t("Connection closed"));
    }
    if !response.ok() {
        if matches!(response.status(), 401 | 410) {
            with(lock_state);
        }
        return Err(t(data["error"]
            .as_str()
            .unwrap_or(&format!("HTTP {}", response.status()))));
    }
    Ok(data)
}
fn render(s: &mut State, data: Value) {
    if let Some(root) = get("remote") {
        class(&root, "on", true);
    }
    out("total", &hash(&data["hashrate"]));
    out(
        "updated",
        &(t("Updated") + " " + &stamp(&data["recorded_at"])),
    );
    hide("login", true);
    hide("logout", false);
    hide("manage", !s.access.as_ref().map(|a| a.key).unwrap_or(false));
    let miners = data["miners"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    hide("empty", !miners.is_empty());
    hide("history", miners.is_empty());
    let mut cards = String::new();
    let mut options = String::new();
    for miner in miners {
        let metric = |label: &str, value: String| {
            format!(
                "<div class=\"metric\"><span>{}</span><strong>{}</strong></div>",
                escape(&t(label)),
                escape(&value)
            )
        };
        let mut metrics = metric("Temperature", temperature(&miner["temp_c"]));
        metrics += &metric(
            "Accepted / rejected",
            format!(
                "{} / {}",
                string(&miner["accepted"]),
                string(&miner["rejected"])
            ),
        );
        metrics += &metric("Uptime", duration(&miner["uptime_s"]));
        metrics += &metric("Power", string(&miner["power_percent"]) + " %");
        metrics += &metric(
            "Best difficulty",
            finite(&miner["best"])
                .map(|v| format(v, json!({"maximumFractionDigits":0})))
                .unwrap_or_else(|| "—".into()),
        );
        metrics += &metric(
            "Pool",
            t(if miner["pool_connected"] == true {
                "Connected"
            } else {
                "Disconnected"
            }) + " · "
                + &string(&miner["pool"]),
        );
        cards+=&format!("<article class=\"miner{}\" data-id=\"{}\"><div class=\"heading\"><h2>{}</h2><span class=\"state\">{}</span></div><small>{}</small><div class=\"rate\">{}</div><small>{} · {}</small><div class=\"metrics\">{metrics}</div></article>",if miner["online"]==true{""}else{" offline"},escape(&string(&miner["id"])),escape(&string(&miner["name"])),escape(&t(if miner["online"]==true{"Online"}else{"Offline"})),escape(&format!("{} · {} · {}",string(&miner["board"]),string(&miner["v"]),string(&miner["coin"]))),escape(&hash(&miner["hashrate"])),escape(&t("Last report")),escape(&duration(&miner["age_s"])));
        options += &format!(
            "<option value=\"{}\">{}</option>",
            escape(&string(&miner["id"])),
            escape(&string(&miner["name"]))
        );
    }
    let chosen = val("history-miner");
    html(&id("miners"), &cards);
    html(&id("history-miner"), &options);
    if miners.iter().any(|m| m["id"] == chosen) {
        set_value(&id("history-miner"), &chosen);
    }
    s.fleet = Some(data);
}
fn history_stats(s: &State) {
    let data = s.series.as_ref().unwrap_or(&Value::Null);
    let summary = &data["summary"];
    out("stat-average", &hash(&summary["average"]));
    out("stat-maximum", &hash(&summary["maximum"]));
    out(
        "stat-minimum",
        &(t("Minimum") + " · " + &hash(&summary["minimum"])),
    );
    out(
        "stat-temperature",
        &temperature(&summary["temperature_average"]),
    );
    out(
        "stat-temperature-max",
        &(t("Peak") + " · " + &temperature(&summary["temperature_maximum"])),
    );
    out("stat-reports", &number(&summary["reports"]));
    out(
        "stat-restarts",
        &(t("Observed restarts") + " · " + &number(&summary["restarts"])),
    );
    out(
        "stat-variation",
        &(t("Variation")
            + " · "
            + &summary["variation_percent"]
                .as_f64()
                .map(|v| format(v, json!({"maximumFractionDigits":1})) + " %")
                .unwrap_or_else(|| "—".into())),
    );
    out(
        "history-range",
        &if whole(&summary["reports"]) > 0 {
            stamp(&summary["first_at"]) + " → " + &stamp(&summary["last_at"])
        } else {
            t("No history yet.")
        },
    );
    let points = data["points"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    disabled(&id("export-history"), points.is_empty());
    let rows = points
        .iter()
        .rev()
        .take(8)
        .map(|p| {
            let values = [
                stamp(&p["t"]),
                hash(&p["rate"]),
                temperature(&p["temp"]),
                number(p.get("samples").unwrap_or(&json!(1))),
            ];
            format!(
                "<tr>{}</tr>",
                values
                    .iter()
                    .map(|v| format!("<td>{}</td>", escape(v)))
                    .collect::<String>()
            )
        })
        .collect::<String>();
    html(&id("history-rows"), &rows);
    if let Some(button) = query("#remote [data-metric=temp]") {
        let _ = button
            .toggle_attribute_with_force("disabled", !points.iter().any(|p| p["temp"].is_number()));
    }
}
fn whole(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
}
async fn history() {
    let miner = val("history-miner");
    let hours = val("hours").parse::<u64>().unwrap_or(24);
    if miner.is_empty() {
        return;
    }
    let mut request = (0, 0);
    with(|s| {
        s.history_version += 1;
        request = (s.version, s.history_version);
        if s.series
            .as_ref()
            .map(|v| v["miner"] != miner || v["hours"] != hours)
            .unwrap_or(true)
        {
            clear_history(s);
            out("history-note", &t("Loading readings…"));
            hide("chart-empty", false);
            out("chart-empty", &t("Loading readings…"));
        }
    });
    let result = api(
        &format!(
            "/api/remote/history?miner={}&hours={hours}",
            js_sys::encode_uri_component(&miner)
        ),
        "GET",
    )
    .await;
    with(|s| {
        if (s.version, s.history_version) != request {
            return;
        }
        match result {
            Ok(mut data) => {
                data["miner"] = miner.into();
                data["hours"] = hours.into();
                s.series = Some(data);
                s.hover = None;
                history_stats(s);
                draw(s);
            }
            Err(error) => {
                clear_history(s);
                hide("chart-empty", false);
                out("chart-empty", &t("History unavailable"));
                out("history-note", &error);
            }
        }
    });
}
fn draw(s: &mut State) {
    let Some(series) = s.series.as_ref() else {
        return;
    };
    let Some(canvas) = get(&id("chart")).and_then(|e| e.dyn_into::<HtmlCanvasElement>().ok())
    else {
        return;
    };
    let width = f64::from(canvas.client_width());
    let height = f64::from(canvas.client_height());
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let ratio = window().device_pixel_ratio().clamp(1.0, 3.0);
    canvas.set_width((width * ratio) as u32);
    canvas.set_height((height * ratio) as u32);
    let Some(ctx) = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok())
    else {
        return;
    };
    let _ = ctx.scale(ratio, ratio);
    let rows = series["points"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let points: Vec<_> = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            Some(Point {
                index,
                t: row["t"].as_f64()?,
                value: row[if s.metric_temp { "temp" } else { "rate" }]
                    .as_f64()
                    .filter(|n| n.is_finite())?,
                row: row.clone(),
            })
        })
        .collect();
    out(
        "history-note",
        &t(if rows.is_empty() {
            "No history yet."
        } else if rows.len() == 1 {
            "Only one reading in this period. Reports arrive every five minutes."
        } else {
            "Statistics use received reports. Missing periods are left empty."
        }),
    );
    hide("chart-empty", !points.is_empty());
    if points.is_empty() {
        out(
            "chart-empty",
            &t(if rows.is_empty() {
                "No history yet."
            } else {
                "No temperature readings."
            }),
        );
        hide("chart-tooltip", true);
        s.plot = None;
        return;
    }
    let end = series["window_end"].as_f64().unwrap_or_else(now);
    let window_start = series["window_start"]
        .as_f64()
        .unwrap_or(end - series["hours"].as_f64().unwrap_or(24.0) * 3600000.0);
    let start = window_start.max(
        rows.first()
            .and_then(|r| r["t"].as_f64())
            .unwrap_or(window_start),
    );
    let end = end.max(start + 1.0);
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for point in &points {
        let min = point.row[if s.metric_temp { "temp" } else { "min_rate" }]
            .as_f64()
            .unwrap_or(point.value);
        let max = point.row[if s.metric_temp {
            "max_temp"
        } else {
            "max_rate"
        }]
        .as_f64()
        .unwrap_or(point.value);
        low = low.min(min);
        high = high.max(max);
    }
    let padding = ((high - low) * 0.15).max(high.abs() * 0.035).max(0.01);
    let min = if s.metric_temp {
        low - padding
    } else {
        (low - padding).max(0.0)
    };
    let max = high + padding;
    let left = if width < 500.0 { 68.0 } else { 86.0 };
    let right = width - 12.0;
    let top = 16.0;
    let bottom = height - 34.0;
    let x = |time: f64| left + (right - left) * (time - start) / (end - start);
    let y = |value: f64| bottom - (bottom - top) * (value - min) / (max - min);
    let dark = document()
        .document_element()
        .and_then(|r| r.get_attribute("data-theme"))
        .as_deref()
        == Some("dark");
    let foreground = if dark { "#999999" } else { "#666666" };
    ctx.set_font("11px system-ui");
    ctx.set_fill_style_str(foreground);
    ctx.set_stroke_style_str(if dark { "#303030" } else { "#dddddd" });
    ctx.set_line_width(1.0);
    for i in 0..5 {
        let value = min + (max - min) * f64::from(i) / 4.0;
        let py = y(value);
        ctx.begin_path();
        ctx.move_to(left, py);
        ctx.line_to(right, py);
        ctx.stroke();
        let label = if s.metric_temp {
            temperature(&json!(value))
        } else {
            hash(&json!(value))
        };
        let _ = ctx.fill_text(&label, 0.0, py + 4.0);
    }
    let ticks = if width < 500.0 { 3 } else { 6 };
    for i in 0..ticks {
        let time = start + (end - start) * f64::from(i) / f64::from(ticks - 1);
        ctx.set_text_align(if i == 0 {
            "left"
        } else if i == ticks - 1 {
            "right"
        } else {
            "center"
        });
        let _ = ctx.fill_text(&stamp(&json!(time)), x(time), height - 9.0);
    }
    ctx.set_text_align("left");
    let color = if s.metric_temp { "#5d9eff" } else { "#f7931a" };
    ctx.set_stroke_style_str(color);
    ctx.set_fill_style_str(color);
    ctx.set_line_width(2.0);
    let interval = series["interval_ms"].as_f64().unwrap_or(300000.0);
    let mut previous: Option<&Point> = None;
    ctx.begin_path();
    for point in &points {
        let continuous = previous
            .map(|p| {
                point.index == p.index + 1
                    && point.row["mixed_boots"] != true
                    && p.row["mixed_boots"] != true
                    && point.row["boot"] == p.row["boot"]
                    && point.t - p.t <= interval * 2.5
            })
            .unwrap_or(false);
        if continuous {
            ctx.line_to(x(point.t), y(point.value));
        } else {
            ctx.move_to(x(point.t), y(point.value));
            ctx.fill_rect(x(point.t) - 2.0, y(point.value) - 2.0, 4.0, 4.0);
        }
        previous = Some(point);
    }
    ctx.stroke();
    if let Some(average) = series["summary"][if s.metric_temp {
        "temperature_average"
    } else {
        "average"
    }]
    .as_f64()
    {
        let _ = ctx.set_line_dash(&js_sys::Array::of2(&5.into(), &5.into()));
        ctx.set_stroke_style_str(foreground);
        ctx.begin_path();
        ctx.move_to(left, y(average));
        ctx.line_to(right, y(average));
        ctx.stroke();
        let _ = ctx.set_line_dash(&js_sys::Array::new());
    }
    let chosen = s.hover.and_then(|i| points.iter().find(|p| p.index == i));
    hide("chart-tooltip", chosen.is_none());
    if let Some(point) = chosen {
        let px = x(point.t);
        let py = y(point.value);
        ctx.set_stroke_style_str(foreground);
        ctx.begin_path();
        ctx.move_to(px, top);
        ctx.line_to(px, bottom);
        ctx.stroke();
        ctx.set_fill_style_str(color);
        ctx.begin_path();
        let _ = ctx.arc(px, py, 4.0, 0.0, std::f64::consts::TAU);
        ctx.fill();
        if let Some(tip) = get(&id("chart-tooltip")) {
            for (selector, label) in [
                ("time", stamp(&json!(point.t))),
                (
                    "strong",
                    if s.metric_temp {
                        temperature(&point.row["temp"])
                    } else {
                        hash(&point.row["rate"])
                    },
                ),
                (
                    "small",
                    number(point.row.get("samples").unwrap_or(&json!(1)))
                        + " "
                        + &t("reports")
                        + " · "
                        + &if s.metric_temp {
                            hash(&point.row["rate"])
                        } else {
                            temperature(&point.row["temp"])
                        },
                ),
            ] {
                if let Some(el) = tip.query_selector(selector).ok().flatten() {
                    el.set_text_content(Some(&label));
                }
            }
            if let Some(el) = tip.dyn_ref::<web_sys::HtmlElement>() {
                let _ = el.style().set_property(
                    "left",
                    &format!(
                        "{}px",
                        (px + 12.0)
                            .min(width - f64::from(el.offset_width()))
                            .max(0.0)
                    ),
                );
                let _ = el.style().set_property(
                    "top",
                    &format!("{}px", (py - f64::from(el.offset_height()) - 12.0).max(0.0)),
                );
            }
        }
    }
    attr(
        &id("chart"),
        "aria-label",
        &(t(if s.metric_temp {
            "Temperature"
        } else {
            "Hashrate"
        }) + " · "
            + &points.len().to_string()
            + " "
            + &t("readings")
            + ". "
            + &t("Use left and right arrows to inspect readings.")),
    );
    s.plot = Some(Plot {
        points,
        left,
        right,
        start,
        end,
    });
}
async fn refresh() {
    let mut version = 0;
    with(|s| version = s.version);
    let result = api("/api/remote", "GET").await;
    match result {
        Ok(data) => {
            let mut current = false;
            with(|s| {
                if s.version == version {
                    out("notice", "");
                    render(s, data);
                    current = true;
                }
            });
            if current {
                history().await;
            }
        }
        Err(error) => with(|s| {
            if s.version == version || s.access.is_none() {
                out("notice", &error);
            }
            if s.fleet.is_some() && s.version == version {
                out(
                    "updated",
                    &t("Connection failed. Last readings may be out of date."),
                );
                out("total", "—");
                for card in all("#rm-miners .miner") {
                    class(&card, "offline", true);
                    if let Some(state) = card.query_selector(".state").ok().flatten() {
                        state.set_text_content(Some(&t("Offline")));
                    }
                }
            }
        }),
    }
    with(|s| {
        if s.access.is_some() && s.version == version {
            window().clear_timeout_with_handle(s.timer);
            s.timer = later(30000, || {
                run(async {
                    refresh().await;
                })
            });
        }
    });
}
fn open(access: Access) {
    with(|s| {
        s.version += 1;
        s.access = Some(access);
        set_value(&id("key"), "");
    });
    run(async {
        refresh().await;
    });
}
pub fn init() {
    if get("remote").is_none() {
        return;
    }
    translate();
    on_id(&id("login-form"), "submit", |event| {
        event.prevent_default();
        let value = val("key").trim().to_string();
        let key = value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if key {
            forget("bdl-remote-address");
            open(Access { key: true, value });
            return;
        }
        let Some(address) = address(&value) else {
            out("notice", &t("Enter the payout address set on your miners."));
            return;
        };
        if checked(&id("remember")) {
            store("bdl-remote-address", &address);
        } else {
            forget("bdl-remote-address");
        }
        open(Access {
            key: false,
            value: address,
        });
    });
    click(&id("logout"), || {
        forget("bdl-remote-address");
        with(lock_state);
    });
    for key in ["history-miner", "hours"] {
        on_id(&id(key), "change", |_| {
            run(async {
                history().await;
            })
        });
    }
    for button in all("#remote [data-metric]") {
        let temp = button.get_attribute("data-metric").as_deref() == Some("temp");
        on(button.as_ref(), "click", move |_| {
            with(|s| {
                s.metric_temp = temp;
                s.hover = None;
                for el in all("#remote [data-metric]") {
                    let _ = el.set_attribute(
                        "aria-pressed",
                        if (el.get_attribute("data-metric").as_deref() == Some("temp")) == temp {
                            "true"
                        } else {
                            "false"
                        },
                    );
                }
                draw(s);
            })
        });
    }
    click(&id("revoke"), || {
        let mut key = false;
        with(|s| key = s.access.as_ref().map(|a| a.key).unwrap_or(false));
        if !key
            || !window()
                .confirm_with_message(&t(
                    "Erase all private history and permanently revoke this key?",
                ))
                .unwrap_or(false)
        {
            return;
        }
        run(async {
            match api("/api/remote", "DELETE").await {
                Ok(_) => {
                    with(lock_state);
                    out("notice", &t("Data erased and key revoked."));
                }
                Err(e) => out("notice", &e),
            }
        });
    });
    for name in ["bdl-language-change", "bdl-theme-change"] {
        on(document().as_ref(), name, |_| {
            translate();
            with(|s| {
                if let Some(fleet) = s.fleet.clone() {
                    render(s, fleet);
                }
                if s.series.is_some() {
                    history_stats(s);
                    draw(s);
                }
            });
        });
    }
    on(window().as_ref(), "resize", |_| with(draw));
    for name in ["pointermove", "pointerdown"] {
        on_id(&id("chart"), name, |event| {
            let Some(mouse) = event.dyn_ref::<web_sys::MouseEvent>() else {
                return;
            };
            if let Some(canvas) = get(&id("chart")) {
                let x = f64::from(mouse.client_x()) - canvas.get_bounding_client_rect().left();
                with(|s| {
                    let selected = s.plot.as_ref().and_then(|plot| {
                        if x < plot.left || x > plot.right {
                            return None;
                        }
                        plot.points
                            .iter()
                            .min_by(|a, b| {
                                let map = |p: &Point| {
                                    plot.left
                                        + (plot.right - plot.left) * (p.t - plot.start)
                                            / (plot.end - plot.start)
                                };
                                (map(a) - x).abs().total_cmp(&(map(b) - x).abs())
                            })
                            .map(|p| p.index)
                    });
                    s.hover = selected;
                    draw(s);
                });
            }
        });
    }
    for name in ["pointerleave", "blur"] {
        on_id(&id("chart"), name, |_| {
            with(|s| {
                s.hover = None;
                draw(s);
            })
        });
    }
    on_id(&id("chart"), "keydown", |event| {
        let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>().map(|e| e.key()) else {
            return;
        };
        if !["ArrowLeft", "ArrowRight", "Home", "End"].contains(&key.as_str()) {
            return;
        }
        event.prevent_default();
        with(|s| {
            if let Some(plot) = &s.plot {
                let count = plot.points.len();
                if count == 0 {
                    return;
                }
                let index = s
                    .hover
                    .and_then(|i| plot.points.iter().position(|p| p.index == i));
                let next = match key.as_str() {
                    "Home" => 0,
                    "End" => count - 1,
                    _ => index
                        .map(|i| {
                            if key == "ArrowRight" {
                                (i + 1).min(count - 1)
                            } else {
                                i.saturating_sub(1)
                            }
                        })
                        .unwrap_or(count - 1),
                };
                s.hover = Some(plot.points[next].index);
            }
            draw(s);
        });
    });
    click(&id("export-history"), || {
        with(|s| {
            let Some(series) = &s.series else {
                return;
            };
            let mut csv="time_utc,hashrate_average_hs,hashrate_minimum_hs,hashrate_maximum_hs,temperature_average_c,temperature_maximum_c,reports,mixed_boots\r\n".to_string();
            for p in series["points"].as_array().into_iter().flatten() {
                let stamp = js_sys::Date::new(&p["t"].as_f64().unwrap_or(0.0).into())
                    .to_iso_string()
                    .as_string()
                    .unwrap_or_default();
                let fields = [
                    stamp,
                    string(&p["rate"]),
                    string(p.get("min_rate").unwrap_or(&p["rate"])),
                    string(p.get("max_rate").unwrap_or(&p["rate"])),
                    string(&p["temp"]),
                    string(&p["max_temp"]),
                    string(p.get("samples").unwrap_or(&json!(1))),
                    (p["mixed_boots"] == true).to_string(),
                ];
                csv += &fields.join(",");
                csv += "\r\n";
            }
            download(
                &csv,
                &format!("bdlminer-history-{}h.csv", series["hours"]),
                "text/csv;charset=utf-8",
            );
        })
    });
    if let Some(saved) = read_storage("bdl-remote-address").and_then(|v| address(&v)) {
        open(Access {
            key: false,
            value: saved,
        });
    }
}
