//! Isolated demos: typed local state; no API, SSE or LAN requests are made.
use crate::{chart, dom::*};
use serde_json::{json, Value};
use std::{cell::RefCell, sync::LazyLock};
use wasm_bindgen::JsCast;
static FIXTURES: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("demo.json")).unwrap());
thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
#[derive(Default)]
struct State {
    mode: String,
    gamma: bool,
    values: Value,
    saved: Value,
    scene: String,
    powers: [f64; 2],
    channels: [String; 2],
    results: [String; 2],
    range: usize,
    paused: bool,
    log_query: String,
    log_tag: String,
    inspect: usize,
    benchmark: Option<f64>,
}
fn with(f: impl FnOnce(&mut State)) {
    STATE.with(|s| f(&mut s.borrow_mut()));
}
pub fn is_demo() -> bool {
    query("meta[name=bdl-demo]").is_some()
}
fn t(key: &str) -> String {
    if !lang() {
        FIXTURES["FR"][key]
            .as_str()
            .or_else(|| FIXTURES["hub"][key].as_str())
            .unwrap_or(key)
            .into()
    } else {
        key.into()
    }
}
fn storage_key(gamma: bool) -> &'static str {
    if gamma {
        "bdl-demo-values-gamma"
    } else {
        "bdl-demo-values-m5"
    }
}
fn save(s: &State) {
    if let Ok(Some(storage)) = window().session_storage() {
        let _ = storage.set_item(storage_key(s.gamma), &s.values.to_string());
    }
}
fn fixture(gamma: bool) -> &'static Value {
    &FIXTURES[if gamma { "gamma" } else { "m5" }]
}
fn si(value: f64) -> String {
    let mut v = value;
    let mut unit = 0;
    let units = ["H/s", "kH/s", "MH/s", "GH/s", "TH/s", "PH/s"];
    while v >= 1000.0 && unit < 5 {
        v /= 1000.0;
        unit += 1;
    }
    format!(
        "{} {}",
        format(
            v,
            json!({"maximumFractionDigits":if v>=100.0{0}else if v>=10.0{1}else{2}})
        ),
        units[unit]
    )
}
fn tile(label: &str, value: &str, sub: &str, hero: bool) -> String {
    format!("<div class=\"t{}\"><span class=\"l\">{}</span><strong class=\"v\">{}</strong><span class=\"s\">{}</span></div>",if hero{" big hero"}else{""},escape(&t(label)),escape(value),escape(sub))
}
fn rate(s: &State) -> f64 {
    if s.scene == "pool" {
        0.0
    } else {
        (if s.gamma { 1.07e12 } else { 1005000.0 })
            * if s.scene == "cooling" {
                0.25
            } else {
                s.values["power"].as_f64().unwrap_or(100.0) / 100.0
            }
    }
}
fn dashboard(s: &State) {
    if get("m").is_none() {
        return;
    }
    let sample = &fixture(s.gamma)["stats"];
    let rate = rate(s);
    let board = if s.gamma {
        "Bitaxe Gamma 601"
    } else {
        "M5StickC Plus2"
    };
    let version = query("meta[name=bdl-demo-version]")
        .and_then(|m| m.get_attribute("content"))
        .unwrap_or_default();
    text(
        "miner-identity",
        &format!(
            "{board} · {version} · {}",
            if lang() { "Demo" } else { "Démo" }
        ),
    );
    text(
        "mining-state",
        &t(if s.scene == "pool" {
            "Waiting for pool"
        } else if s.scene == "cooling" {
            "Cooling down"
        } else {
            "Mining"
        }),
    );
    let main = tile("Hashrate", &si(rate), &t("1-minute average"), true)
        + &tile(
            "Shares",
            if s.gamma { "301 / 1" } else { "20 / 0" },
            &t("Accepted / rejected"),
            false,
        )
        + &tile(
            "Best difficulty",
            if s.gamma { "6984.81" } else { "7.82" },
            &t("best ever"),
            false,
        )
        + &tile("uptime", "1d 0h 0m", &t("Since restart"), false);
    html("m", &main);
    html(
        "overview-strip",
        &(tile(
            "Temperature",
            if s.scene == "cooling" {
                "68 °C"
            } else if s.gamma {
                "51 °C"
            } else {
                "47 °C"
            },
            "",
            false,
        ) + &tile(
            "Effective power",
            &format!(
                "{} %",
                if s.scene == "cooling" {
                    25.0
                } else {
                    s.values["power"].as_f64().unwrap_or(100.0)
                }
            ),
            "",
            false,
        )),
    );
    html(
        "pool-grid",
        &(tile(
            "Pool",
            &string(&s.values["pool"]),
            &t(if s.scene == "pool" {
                "Disconnected"
            } else {
                "Connected"
            }),
            false,
        ) + &tile("Payout address", &string(&s.values["addr"]), "", false)
            + &tile(
                "Protocol",
                if s.values["coin"] == 0 && s.values["proto"] == 1 {
                    "Stratum V2 (Noise)"
                } else {
                    "Stratum V1"
                },
                "",
                false,
            )),
    );
    html(
        "records-grid",
        &(tile("Best difficulty", "7.82", &t("best ever"), false)
            + &tile("Total hashes", &number(&sample["total_hashes"]), "", false)),
    );
    let mut device = tile("Device", board, &t("Demo"), false)
        + &tile("WiFi", "Demo Wi-Fi · −45 dBm", "192.0.2.40", false)
        + &tile(
            "Temperature",
            if s.gamma { "51 °C" } else { "47 °C" },
            "",
            false,
        );
    if s.gamma {
        device += &tile("ASIC", "BM1370 · 525 MHz", "8500 RPM", false);
    }
    html("device-grid", &device);
    html(
        "market-grid",
        &if s.values["coin"] == 0 {
            tile("Block height", "974 232", "", false)
                + &tile("BTC price", "78 000 USD", "", false)
                + &tile("Network hashrate", "1.04 ZH/s", "", false)
        } else {
            tile(
                "Cryptocurrency",
                ["BTC", "BCH", "BSV", "XEC", "DGB", "PPC"]
                    .get(s.values["coin"].as_u64().unwrap_or(0) as usize)
                    .unwrap_or(&"BTC"),
                &t("Demo"),
                false,
            )
        },
    );
    html(
        "updates",
        &tile(
            "Firmware",
            &version,
            if s.values["channel"] == 1 {
                "Beta"
            } else {
                "Stable"
            },
            false,
        ),
    );
    html(
        "profile-grid",
        &(tile("TLS", "3 handshakes · 0 failures", "800 ms", false)
            + &tile(
                "Mining workers",
                "hw 97 % · sw 2.5 % · fill 0.5 %",
                "",
                false,
            )),
    );
    hidden("health", s.scene != "cooling");
    text("health", &t("Cooling down"));
    text(
        "pool-diagnostic-summary",
        &t(if s.scene == "pool" {
            "Disconnected"
        } else {
            "Connected"
        }),
    );
}
fn history(s: &State) {
    if get("hashrate-history").is_none() {
        return;
    }
    let history = &fixture(s.gamma)["history"]["hashrate"];
    let values = history.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let selected = &values[values.len().saturating_sub(s.range)..];
    if get("demo-chart").is_none() {
        html(
            "hashrate-history",
            "<canvas id=\"demo-chart\" tabindex=\"-1\" role=\"img\"></canvas>",
        );
    }
    let end = now();
    let points = selected
        .iter()
        .enumerate()
        .map(|(i, v)| (end - (selected.len() - 1 - i) as f64 * 300000.0, v.as_f64()))
        .collect::<Vec<_>>();
    chart::render("demo-chart", &points, 750000.0);
    let sum = selected.iter().filter_map(Value::as_f64).sum::<f64>();
    let min = selected
        .iter()
        .filter_map(Value::as_f64)
        .fold(f64::INFINITY, f64::min);
    let max = selected
        .iter()
        .filter_map(Value::as_f64)
        .fold(0.0_f64, f64::max);
    text("chart-average", &si(sum / selected.len().max(1) as f64));
    text("chart-min", &si(min));
    text("chart-max", &si(max));
    text("chart-count", &selected.len().to_string());
    text("history-caption",&format!("{} · {}",selected.len(),t("Five-minute averages. Time is relative to the latest recorded point. Use arrow keys to inspect.")));
    disabled("download-history", false);
    let point = selected
        .get(s.inspect.min(selected.len().saturating_sub(1)))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    text(
        "chart-inspection",
        &format!("{} · {}", s.inspect + 1, si(point)),
    );
}
fn dirty(s: &State) {
    let mut changed = false;
    for element in all("#f input,#f select") {
        let key = element.id().trim_start_matches("k_").to_string();
        let current = if element.get_attribute("type").as_deref() == Some("checkbox") {
            json!(property(&element, "checked").as_bool().unwrap_or(false))
        } else if s.values[&key].is_number() {
            json!(property(&element, "value")
                .as_string()
                .unwrap_or_default()
                .parse::<f64>()
                .unwrap_or(0.0))
        } else {
            json!(property(&element, "value").as_string().unwrap_or_default())
        };
        if current != s.saved[&key]
            && !(current.as_f64().is_some() && current.as_f64() == s.saved[&key].as_f64())
        {
            changed = true;
        }
    }
    disabled("save", !changed);
    hidden("discard", !changed);
    text(
        "dirty",
        &t(if changed {
            "Changes take effect after restart."
        } else {
            "Nothing to save."
        }),
    );
}
fn settings(s: &State) {
    if get("settings-nav").is_none() {
        return;
    }
    let schema = fixture(s.gamma)["settings"]["schema"].as_array().unwrap();
    let definitions = &FIXTURES["fields"];
    let mut nav = String::new();
    let mut fields = String::new();
    let mut groups = Vec::<String>::new();
    for row in schema {
        let group = row[2].as_str().unwrap_or("Network");
        if !groups.iter().any(|g| g == group) {
            groups.push(group.into());
        }
    }
    for group in groups {
        let section = if group == "Safety" {
            "performance".into()
        } else {
            format!("group-{}", group.to_lowercase())
        };
        nav += &format!("<a href=\"#{section}\">{}</a>", escape(&t(&group)));
        fields += &format!(
            "<fieldset id=\"{section}\"><legend>{}</legend><div class=\"subsection\">",
            escape(&t(&group))
        );
        for row in schema.iter().filter(|r| r[2] == group) {
            let key = row[0].as_str().unwrap();
            let kind = row[1].as_str().unwrap();
            let definition = &definitions["FIELDS"][key];
            let title = definition[0]
                .as_str()
                .unwrap_or_else(|| row[3].as_str().unwrap_or(key));
            let label = t(title);
            let hint = definition[1].as_str().map(t).unwrap_or_default();
            let options = definitions["CHOICES"][key].as_array();
            let current = &s.values[key];
            let input = if let Some(options) = options {
                format!(
                    "<select id=\"k_{key}\">{}</select>",
                    options
                        .iter()
                        .map(|option| {
                            let option_value = string(&option[0]);
                            format!(
                                "<option value=\"{}\"{}>{}</option>",
                                escape(&option_value),
                                if option_value == string(current) {
                                    " selected"
                                } else {
                                    ""
                                },
                                escape(&t(option[1].as_str().unwrap_or_default()))
                            )
                        })
                        .collect::<String>()
                )
            } else if kind == "bool" {
                format!(
                    "<input id=\"k_{key}\" type=\"checkbox\" class=\"switch\" role=\"switch\"{}>",
                    if current == true { " checked" } else { "" }
                )
            } else {
                let numeric = matches!(kind, "number" | "decimal");
                let limits = &row[4];
                format!(
                    "<input id=\"k_{key}\" type=\"{}\" value=\"{}\"{}{}>",
                    if numeric { "number" } else { "text" },
                    escape(&string(current)),
                    if numeric || key == "pool" || key == "addr" {
                        " required"
                    } else {
                        ""
                    },
                    if numeric {
                        format!(
                            " min=\"{}\"{} step=\"{}\"",
                            limits["min"].as_f64().unwrap_or(0.0),
                            limits["max"]
                                .as_f64()
                                .map(|max| format!(" max=\"{max}\""))
                                .unwrap_or_default(),
                            if kind == "decimal" { "any" } else { "1" }
                        )
                    } else {
                        String::new()
                    }
                )
            };
            fields+=&format!("<div class=\"field\"><label for=\"k_{key}\">{}</label><div class=\"control\">{input}</div><p class=\"field-help\">{}</p>{}</div>",escape(&label),escape(&hint),if key=="power"{format!("<div class=\"presets\">{}</div>",[(100,"Full performance"),(75,"Balanced"),(50,"Quiet")].iter().map(|(power,label)|format!("<button type=\"button\" id=\"preset-{power}\" class=\"secondary\">{} · {power} %</button>",escape(&t(label)))).collect::<String>())}else{String::new()});
        }
        fields += "</div></fieldset>";
    }
    html("f", &fields);
    html("settings-nav", &nav);
    remove_attr("f", "onsubmit");
    hidden("maintenance", true);
    for input in all("#f input,#f select") {
        for event in ["input", "change"] {
            on(input.as_ref(), event, |_| with(|s| dirty(s)));
        }
    }
    for power in [100, 75, 50] {
        click(&format!("preset-{power}"), move || {
            set_value("k_power", &power.to_string());
            with(|s| dirty(s));
        });
    }
    for key in ["pool", "pool2"] {
        on_id(&format!("k_{key}"), "change", move |_| {
            let value = value(&format!("k_{key}"));
            if value.starts_with("stratum+tcp://") || value.starts_with("stratum2+tcp://") {
                if let Ok(url) = web_sys::Url::new(&value) {
                    let suffix = if key == "pool2" { "2" } else { "" };
                    set_value(&format!("k_{key}"), &url.hostname());
                    set_value(&format!("k_port{suffix}"), &url.port());
                    set_value(
                        &format!("k_proto{suffix}"),
                        if url.protocol() == "stratum2+tcp:" {
                            "1"
                        } else {
                            "0"
                        },
                    );
                    set_value(
                        &format!("k_sv2key{suffix}"),
                        url.pathname().trim_start_matches('/'),
                    );
                }
            }
            with(|s| dirty(s));
        });
    }
    on_id("k_coin", "change", |_| {
        if value("k_coin") != "0" {
            set_value("k_proto", "0");
            set_value("k_proto2", "0");
        }
        with(|s| dirty(s));
    });
    dirty(s);
}
fn save_settings(s: &mut State) {
    if let Some(form) = get("f") {
        if call(&form, "reportValidity", &[])
            .ok()
            .and_then(|v| v.as_bool())
            == Some(false)
        {
            return;
        }
    }
    for element in all("#f input,#f select") {
        let key = element.id().trim_start_matches("k_").to_string();
        let value = if element.get_attribute("type").as_deref() == Some("checkbox") {
            json!(property(&element, "checked").as_bool().unwrap_or(false))
        } else if s.values[&key].is_number() {
            json!(property(&element, "value")
                .as_string()
                .unwrap_or_default()
                .parse::<f64>()
                .unwrap_or(0.0))
        } else {
            json!(property(&element, "value").as_string().unwrap_or_default())
        };
        s.values[key] = value;
    }
    save(s);
    s.saved = s.values.clone();
    text("msg", &t("Saved and applied."));
    dirty(s);
}
fn hub(s: &State) {
    if s.mode != "hub" {
        return;
    }
    let total = 1005000.0 * s.powers[0] / 100.0 + 1.07e12 * s.powers[1] / 100.0;
    text("total", &hash(&json!(total)));
    text("freshness", &t("Updated"));
    let mut cards = String::new();
    for (index, key, title) in [
        (0, "m5", "M5StickC Plus2"),
        (1, "gamma", "Gamma 601"),
        (2, "spare", "M5StickC Plus2"),
    ] {
        let online = index < 2;
        let power = s.powers[index.min(1)];
        let rate = if index == 0 {
            1005000.0 * power / 100.0
        } else if index == 1 {
            1.07e12 * power / 100.0
        } else {
            0.0
        };
        let href = format!(
            "../miner/index.html?lang={}{}",
            if lang() { "en" } else { "fr" },
            if index == 1 { "&device=gamma" } else { "" }
        );
        let href = web_sys::Url::new_with_base(&href, &current_url().href())
            .map(|url| url.href())
            .unwrap_or(href);
        let metric = |label: &str, value: &str| {
            format!(
                "<div class=\"metric\"><span>{}</span><strong>{}</strong></div>",
                escape(&t(label)),
                escape(value)
            )
        };
        cards+=&format!("<article class=\"miner{}\" data-id=\"{key}\"><div class=\"heading\"><div><h2>{title}</h2><small>{}</small></div><span>{}</span></div><div class=\"rate\">{}</div><div class=\"metrics\">{}{}{}</div><div class=\"controls\"><label>{}<input type=\"number\" min=\"0\" max=\"100\" step=\"1\" value=\"{power}\" aria-label=\"{}\"{}></label><button type=\"button\" data-apply{}>{}</button><a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a></div><div class=\"controls\"><button type=\"button\" data-diagnose{}>{}</button><select aria-label=\"{}\" data-channel{}><option value=\"stable\"{}>{}</option><option value=\"beta\"{}>{}</option></select><button type=\"button\" data-update{}>{}</button>{}</div><div class=\"result\">{}</div></article>",if online{""}else{" offline"},if index==1{"BM1370 · 525 MHz"}else{"ESP32 · Demo"},escape(&t(if online{"Online"}else{"Offline"})),escape(&hash(&json!(rate))),metric("Temperature",if index==1{"51 °C"}else{"47 °C"}),metric("Accepted / rejected",if index==1{"301 / 1"}else{"20 / 0"}),metric("Power limit",&format!("{power} %")),escape(&t("Power limit %")),escape(&t("Power limit %")),if online{""}else{" disabled"},if online{""}else{" disabled"},escape(&t("Apply")),escape(&href),escape(&t("Open miner")),if online{""}else{" disabled"},escape(&t("Diagnose pool")),escape(&t("Channel")),if online{""}else{" disabled"},if s.channels[index.min(1)]=="stable"{" selected"}else{""},escape(&t("Stable")),if s.channels[index.min(1)]=="beta"{" selected"}else{""},escape(&t("Beta")),if online{""}else{" disabled"},escape(&t("Check update")),if index==1{format!("<button type=\"button\" data-calibrate>{}</button>",escape(&t("Calibrate 525–600 MHz")))}else{String::new()},escape(&s.results[index.min(1)]));
    }
    html("miners", &cards);
    for (index, key) in [(0, "m5"), (1, "gamma")] {
        let selector = format!("[data-id={key}]");
        if let Some(button) = query(&(selector.clone() + " [data-apply]")) {
            let selector = selector.clone();
            on(button.as_ref(), "click", move |_| {
                let Some(input) = query(&(selector.clone() + " input")) else {
                    return;
                };
                if call(&input, "reportValidity", &[])
                    .ok()
                    .and_then(|v| v.as_bool())
                    == Some(false)
                {
                    return;
                }
                let power = property(&input, "value")
                    .as_string()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(100.0);
                with(|s| {
                    s.powers[index] = power.clamp(0.0, 100.0);
                    s.results[index] = t("Operation requested");
                    hub(s);
                });
            });
        }
        for (attr, diagnose) in [
            ("data-diagnose", true),
            ("data-update", false),
            ("data-calibrate", false),
        ] {
            if let Some(button) = query(&(selector.clone() + " [" + attr + "]")) {
                on(button.as_ref(), "click", move |_| {
                    with(|s| {
                        s.results[index] = if diagnose {
                            format!("Stratum V2 (Noise)\n{}: 72 min", t("Mean share interval"))
                        } else if attr == "data-calibrate" {
                            tr(
                                "Cette opération nécessite un vrai miner.",
                                "This operation requires a real miner.",
                            )
                            .into()
                        } else {
                            t("Operation requested")
                        };
                        hub(s);
                    });
                });
            }
        }
        if let Some(select) = query(&(selector.clone() + " [data-channel]")) {
            let target = select.clone();
            on(select.as_ref(), "change", move |_| {
                let value = property(&target, "value")
                    .as_string()
                    .unwrap_or_else(|| "stable".into());
                with(|s| {
                    s.channels[index] = value;
                    s.results[index] = t("Operation requested");
                    hub(s);
                });
            });
        }
    }
    if get("history-miner")
        .map(|e| e.child_element_count() == 0)
        .unwrap_or(false)
    {
        html("history-miner","<option value=\"m5\">M5StickC Plus2</option><option value=\"gamma\">Gamma 601</option><option value=\"spare\">Spare</option>");
    }
    hub_history();
}
fn hub_history() {
    let count = value("hours").parse::<usize>().unwrap_or(24).min(168) * 12;
    let gamma = value("history-miner") == "gamma";
    let base = if gamma { 1.07e12 } else { 1005000.0 };
    let end = now();
    let points = (0..count)
        .map(|i| {
            (
                end - (count - 1 - i) as f64 * 300000.0,
                Some(base * (1.0 + 0.018 * (i as f64 / 8.0).sin())),
            )
        })
        .collect::<Vec<_>>();
    chart::render("chart", &points, 750000.0);
    text("history-info", &t("Gaps mark missing readings or reboots."));
}
const LOG:&str="12:00:00 [boot] BDLMiner V2 · DEMO\n12:00:01 [wifi] Demo Wi-Fi connected\n12:00:02 [pool] Stratum V2 (Noise) connected\n12:00:30 [stats] 1005 kH/s · fictional demo readings\n";
fn log(s: &State) {
    let lines = LOG
        .lines()
        .filter(|line| {
            line.to_lowercase().contains(&s.log_query.to_lowercase())
                && (s.log_tag.is_empty() || line.contains(&format!("[{}]", s.log_tag)))
        })
        .map(|line| format!("<div class=\"log-line\">{}</div>", escape(line)))
        .collect::<String>();
    html("l", &lines);
    text("sum-lines", "4");
    text("sum-alerts", "0");
    text("sum-span", "30 s");
    text("sum-reset", "Demo");
    text("log-state", &t(if s.paused { "Pause" } else { "LIVE" }));
}
pub fn init() {
    let mode = query("meta[name=bdl-demo]")
        .and_then(|m| m.get_attribute("content"))
        .unwrap_or_else(|| "miner".into());
    let gamma = current_url().search_params().get("device").as_deref() == Some("gamma");
    let language = current_url()
        .search_params()
        .get("lang")
        .filter(|s| s == "fr" || s == "en")
        .unwrap_or_else(|| "en".into());
    if let Some(root) = document().document_element() {
        let _ = root.set_attribute("lang", &language);
    }
    for selector in ["[data-t]", "[data-i]"] {
        for element in all(selector) {
            let key = element
                .get_attribute(if selector == "[data-t]" {
                    "data-t"
                } else {
                    "data-i"
                })
                .unwrap_or_default();
            element.set_text_content(Some(&t(&key)));
        }
    }
    let version = query("meta[name=bdl-demo-version]")
        .and_then(|m| m.get_attribute("content"))
        .unwrap_or_default();
    let mut values = fixture(gamma)["settings"]["values"].clone();
    values["ua"] = format!("BDLMinerV2/{version}").into();
    if let Ok(Some(storage)) = window().session_storage() {
        if let Ok(Some(saved)) = storage.get_item(storage_key(gamma)) {
            if let Ok(Value::Object(saved)) = serde_json::from_str::<Value>(&saved) {
                for (key, value) in saved {
                    if values.get(&key).is_some() {
                        values[key] = value;
                    }
                }
            }
        }
    }
    with(|s| {
        s.mode = mode.clone();
        s.gamma = gamma;
        s.saved = values.clone();
        s.values = values;
        s.scene = "mining".into();
        s.powers = [100.0, 100.0];
        s.channels = ["stable".into(), "stable".into()];
        s.range = 72;
    });
    let banner = format!(
        "<aside class=\"demo-notice\"><strong>{}</strong>{}</aside>",
        tr(
            "DÉMO INTERACTIVE · Données fictives · Aucun miner connecté",
            "INTERACTIVE DEMO · Fictional data · No miner connected"
        ),
        if mode == "miner" {
            format!("<select id=\"demo-scene\" aria-label=\"{}\"><option value=\"mining\">{}</option><option value=\"pool\">{}</option><option value=\"cooling\">{}</option></select>",tr("Scénario de démonstration","Demo scenario"),tr("Minage","Mining"),tr("Pool déconnecté","Pool disconnected"),tr("Limite de température","Temperature limit"))
        } else {
            String::new()
        }
    );
    if let Some(body) = document().body() {
        let _ = body.insert_adjacent_html("afterbegin", &banner);
    }
    let resize = || {
        if let (Some(root), Some(banner)) = (document().document_element(), query(".demo-notice")) {
            if let Some(root) = root.dyn_ref::<web_sys::HtmlElement>() {
                let _ = root.style().set_property(
                    "--demo-notice-height",
                    &format!("{}px", banner.get_bounding_client_rect().height()),
                );
            }
        }
    };
    resize();
    on(window().as_ref(), "resize", move |_| resize());
    for element in all("[onclick],[onsubmit],[onchange]") {
        for attribute in ["onclick", "onsubmit", "onchange"] {
            let _ = element.remove_attribute(attribute);
        }
    }
    on(document().as_ref(), "click", move |event| {
        let Some(link) = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|t| t.closest("a[href]").ok().flatten())
        else {
            return;
        };
        if let Ok(url) = web_sys::Url::new_with_base(
            &link.get_attribute("href").unwrap_or_default(),
            &current_url().href(),
        ) {
            if url.origin() == current_url().origin() && url.pathname().ends_with(".html") {
                url.search_params().set("lang", &language);
                if gamma {
                    url.search_params().set("device", "gamma");
                }
                let _ = link.set_attribute("href", &url.href());
            }
        }
    });
    on_id("demo-scene", "change", |_| {
        with(|s| {
            s.scene = value("demo-scene");
            dashboard(s);
        })
    });
    on_id("f", "submit", |event| {
        event.prevent_default();
        with(save_settings);
    });
    click("discard", || {
        with(|s| {
            s.values = s.saved.clone();
            settings(s);
        })
    });
    on_id("settings-search", "input", |_| {
        let query = value("settings-search").to_lowercase();
        let mut shown = 0;
        for field in all("#f .field") {
            let matches = field
                .text_content()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&query);
            let _ = field.toggle_attribute_with_force("hidden", !matches);
            if matches {
                shown += 1;
            }
        }
        hidden("settings-empty", shown > 0);
    });
    for button in all("[data-range]") {
        let range = button
            .get_attribute("data-range")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(21600)
            / 300;
        on(button.as_ref(), "click", move |_| {
            for button in all("[data-range]") {
                let selected = button
                    .get_attribute("data-range")
                    .and_then(|v| v.parse::<usize>().ok())
                    .map(|v| v / 300 == range)
                    .unwrap_or(false);
                let _ =
                    button.set_attribute("aria-pressed", if selected { "true" } else { "false" });
            }
            with(|s| {
                s.range = range;
                history(s);
            });
        });
    }
    on_id("hashrate-history", "keydown", |event| {
        let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>().map(|e| e.key()) else {
            return;
        };
        if ["ArrowLeft", "ArrowRight", "Home", "End"].contains(&key.as_str()) {
            event.prevent_default();
            with(|s| {
                s.inspect = match key.as_str() {
                    "Home" => 0,
                    "End" => s.range - 1,
                    "ArrowLeft" => s.inspect.saturating_sub(1),
                    _ => (s.inspect + 1).min(s.range - 1),
                };
                history(s);
            });
        }
    });
    click("download-history", || {
        with(|s| {
            let values = fixture(s.gamma)["history"]["hashrate"].as_array().unwrap();
            let csv = "sample,hashrate_hs\r\n".to_string()
                + &values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| format!("{i},{v}\r\n"))
                    .collect::<String>();
            download(&csv, "bdlminer-demo-history.csv", "text/csv");
        })
    });
    click("check-pool", || {
        with(|s| {
            text(
                "pool-diagnostic-status",
                &t(if s.scene == "pool" {
                    "Waiting for pool"
                } else {
                    "Connected"
                }),
            );
            html(
                "pool-diagnostic-values",
                &(tile("Protocol", "Stratum V2 (Noise)", "", false)
                    + &tile("Mean share interval", "72 min", &t("on average"), false)
                    + &tile("Share-based estimate", "994 kH/s", "20 shares", false)),
            );
        })
    });
    disabled("start-benchmark", false);
    click("start-benchmark", || {
        with(|s| {
            s.benchmark = Some(now());
            disabled("start-benchmark", true);
            disabled("stop-benchmark", false);
            text("benchmark-status", &t("Running"));
        })
    });
    click("stop-benchmark", || {
        with(|s| {
            let elapsed = s.benchmark.map(|t| (now() - t) / 1000.0).unwrap_or(0.0);
            text(
                "benchmark-status",
                &format!("{} · {elapsed:.1} s · {}", t("Demo"), si(rate(s))),
            );
            disabled("download-benchmark", false);
            disabled("stop-benchmark", true);
            disabled("start-benchmark", false);
        })
    });
    click("download-benchmark", || {
        with(|s| {
            download(&json!({"demo":true,"hashrate_hs":rate(s),"duration_s":s.benchmark.map(|t|(now()-t)/1000.0)}).to_string(),"bdlminer-demo-report.json","application/json")
        })
    });
    on_id("history-miner", "change", |_| hub_history());
    on_id("hours", "change", |_| hub_history());
    set_value("lang", if lang() { "en" } else { "fr" });
    on_id("lang", "change", |_| {
        let url = current_url();
        url.search_params().set("lang", &value("lang"));
        let _ = window().location().assign(&url.href());
    });
    click("pause-log", || {
        with(|s| {
            s.paused = !s.paused;
            log(s);
        })
    });
    on_id("log-search", "input", |_| {
        with(|s| {
            s.log_query = value("log-search");
            log(s);
        })
    });
    click("download-log", || {
        download(LOG, "bdlminer-demo.log", "text/plain")
    });
    with(|s| {
        dashboard(s);
        settings(s);
        if get("hashrate-history").is_some() {
            history(s);
        }
        if mode == "hub" {
            hub(s);
        }
        if get("l").is_some() {
            log(s);
        }
    });
    interval(2000, || {
        with(|s| {
            if !document().hidden() {
                dashboard(s);
            }
        })
    });
}
