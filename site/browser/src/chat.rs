//! Chat protocol, identity, moderation and media controls implemented in Rust.
use crate::{
    dom::*,
    ui::{t, translation},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, Event, WebSocket};
const HOST: &str = "bdlminer-chat.bdlminer-bot.workers.dev";
thread_local! {static STATE:RefCell<Chat>=RefCell::new(Chat::default());}
#[derive(Default)]
struct Chat {
    ws: Option<WebSocket>,
    callbacks: Vec<Closure<dyn FnMut(Event)>>,
    messages: Vec<Value>,
    online: u64,
    unread: usize,
    open: bool,
    admin: bool,
    retry: i32,
    connected: bool,
    me: Value,
    presence: Value,
    view: String,
    busy: bool,
    attach: Option<Attachment>,
    recording: Option<JsValue>,
    record_callbacks: Vec<Closure<dyn FnMut(Event)>>,
    imported: bool,
    qr_timer: i32,
    voice: Option<JsValue>,
}
#[derive(Clone)]
struct Attachment {
    kind: String,
    blob: Blob,
    url: String,
}
fn with(f: impl FnOnce(&mut Chat)) {
    STATE.with(|s| f(&mut s.borrow_mut()));
}
fn note(value: &str) {
    text("chat-note", value);
}
fn error(key: &str, until: Option<f64>) {
    let errors = translation("chatErrors");
    let text = errors[key]
        .as_str()
        .unwrap_or(key)
        .replace("{d}", &until.map(chat_date).unwrap_or_default());
    note(&text);
}
fn code() -> String {
    if let Some(code) = read_storage("chatCode").filter(|s| {
        regex::Regex::new(r"^[A-Za-z0-9_-]{16,128}$")
            .unwrap()
            .is_match(s)
    }) {
        return code;
    }
    let mut random = [0_u8; 24];
    window()
        .crypto()
        .expect("secure browser randomness")
        .get_random_values_with_u8_array(&mut random)
        .expect("secure browser randomness");
    let value = URL_SAFE_NO_PAD.encode(random);
    store("chatCode", &value);
    value
}
fn chat_date(time: f64) -> String {
    let stamp = js_sys::Date::new(&time.into())
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    date(
        &json!(stamp),
        json!({"day":"numeric","month":"short","year":"numeric"}),
    )
}
fn chat_time(time: f64) -> String {
    let stamp = js_sys::Date::new(&time.into())
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    date(&json!(stamp), json!({"hour":"2-digit","minute":"2-digit"}))
}
fn send(s: &Chat, value: Value) -> bool {
    if let Some(ws) = &s.ws {
        if ws.ready_state() == 1 && ws.send_with_str(&value.to_string()).is_ok() {
            return true;
        }
    }
    note(&t("chatOffline"));
    false
}
fn seen(s: &Chat) {
    if let Some(id) = s.messages.iter().filter_map(|m| m["id"].as_u64()).max() {
        let previous = read_storage("chatSeen")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        store("chatSeen", &id.max(previous).to_string());
    }
}
fn unread(s: &mut Chat) {
    let seen = read_storage("chatSeen")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    s.unread = if s.open {
        0
    } else {
        s.messages
            .iter()
            .filter(|m| {
                m["own"] != true
                    && if seen > 0 {
                        m["id"].as_u64().unwrap_or(0) > seen
                    } else {
                        m["t"].as_f64().unwrap_or(0.0) > now() - 86400000.0
                    }
            })
            .count()
    };
    badge(s);
}
fn badge(s: &Chat) {
    hidden("chat-unread", s.unread == 0 || s.open);
    text(
        "chat-unread",
        &if s.unread > 9 {
            "9+".into()
        } else {
            s.unread.to_string()
        },
    );
    attr(
        "chat-bubble",
        "aria-label",
        &(t("chatOpen")
            + &if s.unread > 0 && !s.open {
                format!(" ({} {})", s.unread, t("chatUnread"))
            } else {
                String::new()
            }),
    );
}
fn me(s: &Chat) {
    let Some(input) = get("chat-name") else {
        return;
    };
    let named = s.me["name"].as_str().filter(|n| !n.is_empty());
    let free = named.is_none() || s.admin || now() >= s.me["changeAt"].as_f64().unwrap_or(0.0);
    if let Some(name) = named {
        if !document()
            .active_element()
            .map(|e| e == input)
            .unwrap_or(false)
        {
            set_value("chat-name", name);
        }
    } else if value("chat-name").is_empty() {
        set_value("chat-name", &read_storage("chatName").unwrap_or_default());
    }
    let _ = input.toggle_attribute_with_force("readonly", !free);
    hidden("chat-save", !free);
    text(
        "chat-when",
        &if named.is_none() {
            t("chatNameHint")
        } else if free {
            t("chatNow")
        } else {
            t("chatWhen").replace("{d}", &chat_date(s.me["changeAt"].as_f64().unwrap_or(0.0)))
        },
    );
}
fn members(s: &Chat) {
    let rows = s.presence["members"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut out = String::new();
    for member in rows {
        out += &format!(
            "<li><span class=\"dot on\" aria-hidden=\"true\"></span><span>{}</span>{}{}</li>",
            escape(&string(&member["name"])),
            if member["team"] == true {
                format!("<span class=\"team\">{}</span>", escape(&t("chatTeam")))
            } else {
                String::new()
            },
            if s.me["name"] == member["name"] {
                format!("<span class=\"me\">{}</span>", escape(&t("chatYou")))
            } else {
                String::new()
            }
        );
    }
    html("chat-member-list", &out);
    let guests = s.presence["guests"].as_u64().unwrap_or(0);
    text(
        "chat-guests",
        &(if rows.is_empty() {
            t("chatNobody") + " "
        } else {
            String::new()
        } + &if guests > 0 {
            t("chatGuests").replace("{n}", &guests.to_string())
        } else {
            String::new()
        }),
    );
}
fn qr_hide(s: &mut Chat) {
    window().clear_timeout_with_handle(s.qr_timer);
    hidden("chat-qr", true);
    html("chat-qr-img", "");
    text("chat-qr-show", &t("chatQrShow"));
    attr("chat-qr-show", "aria-expanded", "false");
}
fn view(s: &mut Chat, value: &str) {
    if value != "settings" {
        qr_hide(s);
    }
    s.view = value.into();
    hidden("chat-settings", value != "settings");
    hidden("chat-members", value != "members");
    for id in ["chat-log", "chat-form"] {
        hidden(id, value != "log");
    }
    if let Some(rules) = query("#chat .chat-rules") {
        let _ = rules.toggle_attribute_with_force("hidden", value != "log");
    }
    attr(
        "chat-gear",
        "aria-pressed",
        if value == "settings" { "true" } else { "false" },
    );
    attr(
        "chat-people",
        "aria-pressed",
        if value == "members" { "true" } else { "false" },
    );
    me(s);
    members(s);
    if !touch() {
        focus(match value {
            "settings" => "chat-name",
            "members" => "chat-members-back",
            _ => "chat-text",
        });
    }
}
fn touch() -> bool {
    window()
        .match_media("(pointer: coarse)")
        .ok()
        .flatten()
        .map(|m| m.matches())
        .unwrap_or(false)
}
fn fit(s: &Chat) {
    let Some(el) = get("chat").and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok()) else {
        return;
    };
    let viewport = property(&window(), "visualViewport");
    let width = window()
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(1000.0);
    let height = window()
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0);
    if viewport.is_null() || viewport.is_undefined() || !s.open || width > 520.0 {
        let _ = el.style().remove_property("height");
        let _ = el.style().remove_property("bottom");
        return;
    }
    let visible = property(&viewport, "height").as_f64().unwrap_or(height);
    let top = property(&viewport, "offsetTop").as_f64().unwrap_or(0.0);
    let _ = el.style().set_property(
        "bottom",
        &format!("{}px", (height - visible - top).max(0.0)),
    );
    let _ = el.style().set_property(
        "height",
        &format!("{}px", (visible * 0.92).min(height * 0.85).round()),
    );
}
fn toggle(s: &mut Chat, open: bool) {
    s.open = open;
    hidden("chat", !open);
    attr(
        "chat-bubble",
        "aria-expanded",
        if open { "true" } else { "false" },
    );
    fit(s);
    if open {
        s.unread = 0;
        seen(s);
        badge(s);
        if let Some(log) = get("chat-log") {
            log.set_scroll_top(log.scroll_height());
        }
        if s.me["name"].as_str().filter(|s| !s.is_empty()).is_none() {
            view(s, "settings");
        } else if touch() {
            focus("chat-close");
        } else {
            focus("chat-text");
        }
    } else {
        focus("chat-bubble");
    }
}
fn rich_text(text: &str) -> String {
    thread_local! { static LINKS: regex::Regex = regex::Regex::new(r"https?://[^\s]+").unwrap(); }
    LINKS.with(|re| {
        let mut out = String::new();
        let mut end = 0;
        for found in re.find_iter(text) {
            out += &escape(&text[end..found.start()]);
            out += &format!(
                "<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer nofollow ugc\">{}</a>",
                escape(found.as_str()),
                escape(found.as_str())
            );
            end = found.end();
        }
        out += &escape(&text[end..]);
        out
    })
}
fn confirm_button(selector: &str, label: &str, action: Value) {
    let Some(button) = query(selector) else {
        return;
    };
    let armed = Rc::new(Cell::new(0));
    let target = button.clone();
    let label = label.to_string();
    on(button.as_ref(), "click", move |_| {
        if armed.get() != 0 {
            window().clear_timeout_with_handle(armed.get());
            let action = action.clone();
            with(|s| {
                send(s, action);
            });
            return;
        }
        target.set_text_content(Some(&t("chatSure")));
        let target = target.clone();
        let label = label.clone();
        let state = armed.clone();
        armed.set(later(3000, move || {
            state.set(0);
            target.set_text_content(Some(&t(&label)));
        }));
    });
}
fn editing(message: Value) {
    let id = string(&message["id"]);
    let Some(item) = query(&format!("#chat-log li[data-id='{id}']")) else {
        return;
    };
    let editor=format!("<div class=\"editing\"><textarea maxlength=\"500\" rows=\"2\">{}</textarea><div class=\"row\"><button type=\"button\" class=\"btn-s ghost\" data-cancel>{}</button><button type=\"button\" class=\"btn-s\" data-save>{}</button></div></div>",escape(&string(&message["text"])),escape(&t("chatCancel")),escape(&t("chatSaveEdit")));
    if let Some(old) = item.query_selector(".text").ok().flatten() {
        old.set_outer_html(&editor);
    } else {
        let _ = item.insert_adjacent_html("beforeend", &editor);
    }
    prune_listeners();
    let selector = format!("#chat-log li[data-id='{id}']");
    if let Some(button) = query(&(selector.clone() + " [data-cancel]")) {
        on(button.as_ref(), "click", |_| with(render));
    }
    if let Some(button) = query(&(selector.clone() + " [data-save]")) {
        let area = query(&(selector.clone() + " textarea")).unwrap();
        let id = id.clone();
        on(button.as_ref(), "click", move |_| {
            let text = property(&area, "value").as_string().unwrap_or_default();
            with(|s| {
                send(s, json!({"type":"edit","id":id,"text":text}));
            });
        });
    }
    if let Some(area) = query(&(selector.clone() + " textarea")) {
        let selector = selector.clone();
        on(area.as_ref(), "keydown", move |event| {
            let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
                return;
            };
            let button = if key.key() == "Enter" && !key.shift_key() {
                Some("[data-save]")
            } else if key.key() == "Escape" {
                event.stop_propagation();
                Some("[data-cancel]")
            } else {
                None
            };
            if let Some(button) = button {
                event.prevent_default();
                if let Some(button) = query(&(selector.clone() + " " + button)) {
                    let _ = call(&button, "click", &[]);
                }
            }
        });
        let _ = call(&area, "focus", &[]);
    }
}
fn render(s: &mut Chat) {
    let near = get("chat-log")
        .map(|e| e.scroll_height() - e.scroll_top() - e.client_height() < 80)
        .unwrap_or(true);
    let mut out = String::new();
    if s.messages.is_empty() {
        out = format!(
            "<li class=\"empty\">{}</li>",
            escape(&t(if s.connected {
                "chatEmpty"
            } else {
                "chatOffline"
            }))
        );
    }
    for message in &s.messages {
        let id = string(&message["id"]);
        let name = string(&message["name"]);
        let hue = name
            .chars()
            .fold(7_u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32))
            % 360;
        let color = if message["team"] == true {
            "var(--accent)".into()
        } else {
            format!("hsl({hue} 70% 70%)")
        };
        let mut actions = String::new();
        if message["own"] == true {
            actions += &format!(
                "<button type=\"button\" data-edit>{}</button>",
                escape(&t("chatEdit"))
            );
        }
        if message["own"] == true || s.admin {
            actions += &format!(
                "<button type=\"button\" class=\"danger\" data-delete>{}</button>",
                escape(&t("chatDelete"))
            );
            if s.admin && message["own"] != true {
                actions += &format!(
                    "<button type=\"button\" class=\"danger\" data-ban>{}</button>",
                    escape(&t("chatBan"))
                );
            }
        }
        let time = message["t"].as_f64().unwrap_or(0.0);
        let iso = js_sys::Date::new(&time.into())
            .to_iso_string()
            .as_string()
            .unwrap_or_default();
        out+=&format!("<li data-id=\"{}\"><div class=\"meta\"><span class=\"who\" style=\"color:{}\">{}</span>{}<time datetime=\"{}\">{}</time>{}<span class=\"acts\">{actions}</span></div>{}{}</li>",escape(&id),escape(&color),escape(&name),if message["team"]==true{format!("<span class=\"team\">{}</span>",escape(&t("chatTeam")))}else{String::new()},escape(&iso),escape(&chat_time(time)),if message["edited"]==true{format!("<span class=\"edited\">({})</span>",escape(&t("chatEdited")))}else{String::new()},message["text"].as_str().filter(|s|!s.is_empty()).map(|text|format!("<div class=\"text\">{}</div>",rich_text(text))).unwrap_or_default(),if message["media"].is_object(){media_html(&message["media"])}else{String::new()});
    }
    html("chat-log", &out);
    for message in &s.messages {
        let id = string(&message["id"]);
        let selector = format!("#chat-log li[data-id='{}']", escape(&id));
        if let Some(button) = query(&(selector.clone() + " [data-edit]")) {
            let message = message.clone();
            on(button.as_ref(), "click", move |_| editing(message.clone()));
        }
        confirm_button(
            &(selector.clone() + " [data-delete]"),
            "chatDelete",
            json!({"type":"delete","id":message["id"]}),
        );
        confirm_button(
            &(selector + " [data-ban]"),
            "chatBan",
            json!({"type":"ban","id":message["id"]}),
        );
    }
    bind_voice(s);
    if near {
        if let Some(log) = get("chat-log") {
            log.set_scroll_top(log.scroll_height());
        }
    }
    text(
        "chat-online",
        &if s.connected {
            s.online.to_string()
        } else {
            "–".into()
        },
    );
    badge(s);
}
fn received(s: &mut Chat, message: Value) {
    match message["type"].as_str().unwrap_or_default() {
        "hello" => {
            s.messages = message["messages"].as_array().cloned().unwrap_or_default();
            s.messages.truncate(100);
            s.online = message["online"].as_u64().unwrap_or(0);
            render(s);
            if s.open {
                seen(s);
            }
            unread(s);
        }
        "mine" => {
            let ids = message["ids"].as_array().cloned().unwrap_or_default();
            for row in &mut s.messages {
                if ids.contains(&row["id"]) {
                    row["own"] = true.into();
                }
            }
            render(s);
            unread(s);
        }
        "online" => {
            s.online = message["count"].as_u64().unwrap_or(0);
            text("chat-online", &s.online.to_string());
        }
        "presence" => {
            s.presence = message;
            members(s);
        }
        "message" => {
            if s.messages
                .iter()
                .any(|m| m["id"] == message["message"]["id"])
            {
                return;
            }
            s.messages.push(message["message"].clone());
            if s.messages.len() > 100 {
                s.messages.remove(0);
            }
            if s.open {
                seen(s);
            }
            render(s);
            unread(s);
        }
        "edited" => {
            if let Some(item) = s
                .messages
                .iter_mut()
                .find(|m| m["id"] == message["message"]["id"])
            {
                *item = message["message"].clone();
            }
            render(s);
        }
        "removed" => {
            let ids = message["ids"].as_array().cloned().unwrap_or_default();
            s.messages.retain(|m| !ids.contains(&m["id"]));
            render(s);
        }
        "you" => {
            let previous = s.me["name"].clone();
            s.me = message;
            if let Some(name) = s.me["name"].as_str().filter(|s| !s.is_empty()) {
                store("chatName", name);
            }
            me(s);
            if s.me["name"].is_string() && s.view == "settings" && previous != s.me["name"] {
                note(&t("chatSaved"));
                view(s, "log");
            }
            if s.imported {
                s.imported = false;
                if !s.open {
                    toggle(s, true);
                }
                if s.me["name"].is_string() {
                    view(s, "log");
                    note(&t("chatCodeSet"));
                }
            }
        }
        "error" => error(
            message["error"].as_str().unwrap_or("invalid"),
            message["until"].as_f64(),
        ),
        "auth" => {
            s.admin = message["ok"] == true;
            if !s.admin && message["error"] != "slow" {
                forget("chatKey");
            }
            note(&t(if s.admin {
                "chatAdminOn"
            } else if message["error"] == "slow" {
                "chatAdminSlow"
            } else {
                "chatAdminOff"
            }));
            render(s);
            me(s);
        }
        _ => {}
    }
}
fn connect() {
    let Ok(ws) = WebSocket::new(&format!("wss://{HOST}/ws")) else {
        return;
    };
    let open = Closure::wrap(Box::new(|_: Event| {
        with(|s| {
            s.connected = true;
            s.retry = 1000;
            note("");
            if let Some(key) = read_storage("chatKey") {
                send(s, json!({"type":"auth","key":key}));
            }
            send(s, json!({"type":"identify","code":code()}));
        })
    }) as Box<dyn FnMut(Event)>);
    let message = Closure::wrap(Box::new(|event: Event| {
        if let Some(text) = property(&event, "data").as_string() {
            if let Ok(message) = serde_json::from_str::<Value>(&text) {
                with(|s| received(s, message));
            }
        }
    }) as Box<dyn FnMut(Event)>);
    let close = Closure::wrap(Box::new(|_: Event| {
        with(|s| {
            s.connected = false;
            s.ws = None;
            render(s);
            let retry = s.retry.max(1000);
            later(retry, connect);
            s.retry = (retry * 2).min(30000);
        })
    }) as Box<dyn FnMut(Event)>);
    ws.set_onopen(Some(open.as_ref().unchecked_ref()));
    ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
    ws.set_onclose(Some(close.as_ref().unchecked_ref()));
    with(|s| {
        s.ws = Some(ws);
        s.callbacks = vec![open, message, close];
    });
}

const PLAY: &str =
    "<svg viewBox=\"0 0 24 24\" fill=\"currentColor\"><path d=\"M7 4.5v15l13-7.5z\"/></svg>";
const PAUSE:&str="<svg viewBox=\"0 0 24 24\" fill=\"currentColor\"><path d=\"M6 4h4v16H6zm8 0h4v16h-4z\"/></svg>";
fn voice_html(src: &str) -> String {
    format!("<div class=\"voice\"><button type=\"button\" class=\"vplay\" aria-label=\"{}\">{PLAY}</button><div class=\"vwave\" tabindex=\"0\" role=\"slider\" aria-label=\"{}\">{}</div><span class=\"vtime\">0:00</span><audio hidden preload=\"metadata\" src=\"{}\"></audio></div>",escape(&t("chatPlay")),escape(&t("chatPosition")),"<i style=\"height:30%\"></i>".repeat(36),escape(src))
}
fn media_html(media: &Value) -> String {
    let src = format!(
        "https://{HOST}/media/{}",
        js_sys::encode_uri_component(&string(&media["id"]))
    );
    let content=match media["kind"].as_str(){Some("image")=>format!("<a href=\"{}\" target=\"_blank\" rel=\"noopener\"><img src=\"{}\" loading=\"lazy\" alt=\"{}\"></a>",escape(&src),escape(&src),escape(&t("chatPhoto"))),Some("video")=>format!("<video src=\"{}\" controls preload=\"metadata\" playsinline></video>",escape(&src)),Some("audio")=>voice_html(&src),_=>String::new()};
    format!("<div class=\"media\">{content}</div>")
}
fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
fn update_voice(player: &web_sys::Element, audio: &JsValue) {
    let duration = property(audio, "duration")
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(0.0);
    let current = property(audio, "currentTime").as_f64().unwrap_or(0.0);
    let paused = property(audio, "paused").as_bool().unwrap_or(true);
    class(player, "playing", !paused);
    if let Some(button) = player.query_selector(".vplay").ok().flatten() {
        button.set_inner_html(if paused { PLAY } else { PAUSE });
        let _ = button.set_attribute(
            "aria-label",
            &t(if paused { "chatPlay" } else { "chatPause" }),
        );
    }
    if let Some(time) = player.query_selector(".vtime").ok().flatten() {
        time.set_text_content(Some(&clock(if paused && current == 0.0 {
            duration
        } else {
            current
        })));
    }
    if let Some(wave) = player.query_selector(".vwave").ok().flatten() {
        for (name, value) in [
            ("aria-valuemin", "0".into()),
            ("aria-valuemax", duration.round().to_string()),
            ("aria-valuenow", current.round().to_string()),
            ("aria-valuetext", clock(current) + " / " + &clock(duration)),
        ] {
            let _ = wave.set_attribute(name, &value);
        }
        if let Ok(bars) = wave.query_selector_all("i") {
            for i in 0..bars.length() {
                if let Some(bar) = bars
                    .item(i)
                    .and_then(|b| b.dyn_into::<web_sys::Element>().ok())
                {
                    class(
                        &bar,
                        "on",
                        duration > 0.0 && (f64::from(i) + 0.5) / 36.0 <= current / duration,
                    );
                }
            }
        }
    }
}
fn bind_voice(s: &mut Chat) {
    for player in all("#chat .voice") {
        let Some(mut audio) = player.query_selector("audio").ok().flatten() else {
            continue;
        };
        if audio.has_attribute("data-bound") {
            continue;
        }
        let src = audio.get_attribute("src").unwrap_or_default();
        if let Some(previous) = s
            .voice
            .as_ref()
            .and_then(|v| v.dyn_ref::<web_sys::Element>())
        {
            if previous.get_attribute("src").as_deref() == Some(&src) {
                let _ = audio.replace_with_with_node_1(previous);
                audio = previous.clone();
            }
        }
        let _ = audio.set_attribute("data-bound", "");
        for name in [
            "loadedmetadata",
            "durationchange",
            "timeupdate",
            "play",
            "pause",
            "ended",
        ] {
            let player = player.clone();
            let audio = audio.clone();
            let target = audio.clone();
            on(target.as_ref(), name, move |_| {
                if name == "ended" {
                    set(&audio, "currentTime", &0.into());
                }
                update_voice(&player, &audio);
            });
        }
        if let Some(button) = player.query_selector(".vplay").ok().flatten() {
            let audio = audio.clone();
            on(button.as_ref(), "click", move |_| {
                if property(&audio, "paused").as_bool().unwrap_or(true) {
                    with(|s| {
                        if let Some(previous) = &s.voice {
                            if previous != &JsValue::from(audio.clone()) {
                                let _ = call(previous, "pause", &[]);
                            }
                        }
                        s.voice = Some(audio.clone().into());
                    });
                    let _ = call(&audio, "play", &[]);
                } else {
                    let _ = call(&audio, "pause", &[]);
                }
            });
        }
        if let Some(wave) = player.query_selector(".vwave").ok().flatten() {
            let audio = audio.clone();
            let area = wave.clone();
            on(wave.as_ref(), "click", move |event| {
                if let Some(mouse) = event.dyn_ref::<web_sys::MouseEvent>() {
                    let rect = area.get_bounding_client_rect();
                    let duration = property(&audio, "duration")
                        .as_f64()
                        .filter(|v| v.is_finite())
                        .unwrap_or(0.0);
                    set(
                        &audio,
                        "currentTime",
                        &(((f64::from(mouse.client_x()) - rect.left()) / rect.width())
                            .clamp(0.0, 1.0)
                            * duration)
                            .into(),
                    );
                }
            });
            let audio = player.query_selector("audio").ok().flatten().unwrap();
            let button = player.query_selector(".vplay").ok().flatten().unwrap();
            on(wave.as_ref(), "keydown", move |event| {
                let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>().map(|e| e.key()) else {
                    return;
                };
                if ["ArrowLeft", "ArrowRight"].contains(&key.as_str()) {
                    event.prevent_default();
                    let current = property(&audio, "currentTime").as_f64().unwrap_or(0.0);
                    let duration = property(&audio, "duration")
                        .as_f64()
                        .filter(|v| v.is_finite())
                        .unwrap_or(0.0);
                    set(
                        &audio,
                        "currentTime",
                        &((current + if key == "ArrowRight" { 2.0 } else { -2.0 })
                            .clamp(0.0, duration))
                        .into(),
                    );
                } else if key == " " || key == "Enter" {
                    event.prevent_default();
                    let _ = call(&button, "click", &[]);
                }
            });
        }
        let bars = player.query_selector_all(".vwave i").ok();
        run(async move {
            let mut hash = src
                .bytes()
                .fold(7_u32, |h, c| h.wrapping_mul(31).wrapping_add(c.into()));
            let mut peaks = (0..36)
                .map(|_| {
                    hash = hash.wrapping_mul(1103515245).wrapping_add(12345);
                    0.25 + 0.75 * f64::from((hash >> 16) % 100) / 100.0
                })
                .collect::<Vec<_>>();
            if let Ok(response) = JsFuture::from(window().fetch_with_str(&src)).await {
                if let Ok(response) = response.dyn_into::<web_sys::Response>() {
                    if let Ok(buffer) = response.array_buffer() {
                        if let Ok(buffer) = JsFuture::from(buffer).await {
                            if let Ok(ctx) = web_sys::AudioContext::new() {
                                if let Ok(buffer) = buffer.dyn_into::<js_sys::ArrayBuffer>() {
                                    if let Ok(decoded) = ctx.decode_audio_data(&buffer) {
                                        if let Ok(decoded) = JsFuture::from(decoded).await {
                                            if let Ok(decoded) =
                                                decoded.dyn_into::<web_sys::AudioBuffer>()
                                            {
                                                if let Ok(samples) = decoded.get_channel_data(0) {
                                                    let step = (samples.len() / 36).max(1);
                                                    let raw = (0..36)
                                                        .map(|i| {
                                                            samples
                                                                .iter()
                                                                .skip(i * step)
                                                                .take(step)
                                                                .step_by(16)
                                                                .map(|s| f64::from(s.abs()))
                                                                .fold(0.0_f64, f64::max)
                                                        })
                                                        .collect::<Vec<_>>();
                                                    let top = raw
                                                        .iter()
                                                        .copied()
                                                        .fold(0.01_f64, f64::max);
                                                    peaks = raw
                                                        .iter()
                                                        .map(|p| 0.15 + 0.85 * p / top)
                                                        .collect();
                                                }
                                            }
                                        }
                                    }
                                }
                                let _ = ctx.close();
                            }
                        }
                    }
                }
            }
            if let Some(bars) = bars {
                for (i, height) in peaks.iter().enumerate() {
                    if let Some(bar) = bars
                        .item(i as u32)
                        .and_then(|b| b.dyn_into::<web_sys::HtmlElement>().ok())
                    {
                        let _ = bar
                            .style()
                            .set_property("height", &format!("{}%", (height * 100.0).round()));
                    }
                }
            }
        });
    }
}
fn clear_attach(s: &mut Chat) {
    if let Some(attach) = s.attach.take() {
        let _ = web_sys::Url::revoke_object_url(&attach.url);
    }
    hidden("chat-attach", true);
    for el in all("#chat-attach img,#chat-attach video,#chat-attach .voice") {
        el.remove();
    }
    prune_listeners();
}
fn attach(s: &mut Chat, kind: &str, blob: Blob, label: String) {
    clear_attach(s);
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    let media = if kind == "audio" {
        voice_html(&url)
    } else if kind == "image" {
        format!("<img src=\"{}\" alt=\"\">", escape(&url))
    } else {
        format!("<video src=\"{}\" muted playsinline></video>", escape(&url))
    };
    if let Some(box_) = get("chat-attach") {
        let _ = box_.insert_adjacent_html("afterbegin", &media);
    }
    text(
        "chat-attach-name",
        if kind == "audio" { "" } else { &label },
    );
    s.attach = Some(Attachment {
        kind: kind.into(),
        blob,
        url,
    });
    hidden("chat-attach", false);
    bind_voice(s);
    focus("chat-text");
}
async fn shrink(blob: Blob) -> Blob {
    if blob.type_() == "image/gif" {
        return blob;
    }
    let Ok(promise) = window().create_image_bitmap_with_blob(&blob) else {
        return blob;
    };
    let Ok(bitmap) = JsFuture::from(promise)
        .await
        .and_then(|v| v.dyn_into::<web_sys::ImageBitmap>())
    else {
        return blob;
    };
    let ratio = (1600.0 / f64::from(bitmap.width().max(bitmap.height()))).min(1.0);
    let Some(canvas) = document()
        .create_element("canvas")
        .ok()
        .and_then(|v| v.dyn_into::<web_sys::HtmlCanvasElement>().ok())
    else {
        return blob;
    };
    canvas.set_width((f64::from(bitmap.width()) * ratio).round() as u32);
    canvas.set_height((f64::from(bitmap.height()) * ratio).round() as u32);
    if let Some(ctx) = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into::<web_sys::CanvasRenderingContext2d>().ok())
    {
        let _ = ctx.draw_image_with_image_bitmap_and_dw_and_dh(
            &bitmap,
            0.0,
            0.0,
            canvas.width().into(),
            canvas.height().into(),
        );
    }
    bitmap.close();
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let cb = Closure::once_into_js(move |value: JsValue| {
            let _ = resolve.call1(&JsValue::UNDEFINED, &value);
        });
        let _ = canvas.to_blob_with_type_and_encoder_options(
            cb.unchecked_ref(),
            "image/jpeg",
            &0.85.into(),
        );
    });
    if let Ok(compressed) = JsFuture::from(promise)
        .await
        .and_then(|v| v.dyn_into::<Blob>())
    {
        if compressed.size() < blob.size() {
            return compressed;
        }
    }
    blob
}
async fn upload(attach: Attachment) -> Result<Value, String> {
    let options = web_sys::RequestInit::new();
    options.set_method("POST");
    options.set_body(&attach.blob);
    let headers = web_sys::Headers::new().map_err(|_| "upload")?;
    let _ = headers.set("X-Chat-Code", &code());
    let _ = headers.set("Content-Type", &attach.blob.type_());
    options.set_headers(&headers);
    options.set_signal(Some(&web_sys::AbortSignal::timeout_with_u32(30000)));
    let response = JsFuture::from(window().fetch_with_str_and_init(
        &format!("https://{HOST}/api/media?kind={}", attach.kind),
        &options,
    ))
    .await
    .map_err(|_| "upload")?
    .dyn_into::<web_sys::Response>()
    .map_err(|_| "upload")?;
    let value = JsFuture::from(response.json().map_err(|_| "upload")?)
        .await
        .map_err(|_| "upload")?;
    let value = json(&value);
    if !response.ok() || !value["media"].is_object() {
        return Err(value["error"].as_str().unwrap_or("upload").into());
    }
    Ok(value["media"].clone())
}
fn save_name() {
    let name = value("chat-name").trim().to_string();
    if name.is_empty() {
        error("name", None);
        return;
    }
    if let Some(key) = name
        .strip_prefix("/login ")
        .filter(|s| !s.is_empty() && !s.chars().any(char::is_whitespace))
    {
        store("chatKey", key);
        with(|s| {
            send(s, json!({"type":"auth","key":key}));
        });
        set_value("chat-name", "");
        return;
    }
    with(|s| {
        if s.me["name"] == name {
            view(s, "log");
        } else {
            send(s, json!({"type":"register","name":name}));
        }
    });
}
fn link_to() -> String {
    format!(
        "{}{}#chat-code={}",
        current_url().origin(),
        current_url().pathname(),
        code()
    )
}
fn show_qr(s: &mut Chat) {
    if get("chat-qr")
        .map(|e| !e.has_attribute("hidden"))
        .unwrap_or(false)
    {
        qr_hide(s);
        return;
    }
    let Ok(qr) =
        qrcode::QrCode::with_error_correction_level(link_to().as_bytes(), qrcode::EcLevel::M)
    else {
        note(&t("chatQrFailed"));
        return;
    };
    let n = qr.width();
    let mut path = String::new();
    for y in 0..n {
        for x in 0..n {
            if qr[(x, y)] == qrcode::Color::Dark {
                path += &format!("M{} {}h1v1h-1z", x + 4, y + 4);
            }
        }
    }
    html("chat-qr-img",&format!("<svg viewBox=\"0 0 {} {}\" shape-rendering=\"crispEdges\" aria-hidden=\"true\"><rect width=\"100%\" height=\"100%\" fill=\"#fff\"/><path d=\"{path}\" fill=\"#0b0d12\"/></svg>",n+8,n+8));
    hidden("chat-qr", false);
    text("chat-qr-show", &t("chatQrHide"));
    attr("chat-qr-show", "aria-expanded", "true");
    window().clear_timeout_with_handle(s.qr_timer);
    s.qr_timer = later(90000, || with(qr_hide));
}
async fn submit() {
    let text = value("chat-text").trim().to_string();
    let mut state = None;
    with(|s| {
        if !s.busy {
            state = Some((s.attach.clone(), s.me["name"].is_string()));
        }
    });
    let Some((attachment, named)) = state else {
        return;
    };
    if text.is_empty() && attachment.is_none() {
        return;
    }
    if let Some(key) = text
        .strip_prefix("/login ")
        .filter(|s| !s.is_empty() && !s.chars().any(char::is_whitespace))
    {
        store("chatKey", key);
        with(|s| {
            send(s, json!({"type":"auth","key":key}));
        });
        set_value("chat-text", "");
        return;
    }
    if text == "/logout" {
        forget("chatKey");
        with(|s| {
            s.admin = false;
            render(s);
            me(s);
        });
        set_value("chat-text", "");
        return;
    }
    if !named {
        note(&t("chatNeedName"));
        with(|s| view(s, "settings"));
        return;
    }
    let mut say = json!({"type":"say","text":text});
    if let Some(attachment) = attachment {
        with(|s| s.busy = true);
        disabled("chat-send", true);
        note(&t("chatUploading"));
        let result = upload(attachment).await;
        with(|s| s.busy = false);
        disabled("chat-send", false);
        match result {
            Ok(media) => {
                say["media"] = json!({"id":media["id"]});
                note("");
            }
            Err(key) => {
                error(&key, None);
                return;
            }
        }
    }
    with(|s| {
        if send(s, say) {
            set_value("chat-text", "");
            clear_attach(s);
        }
    });
}
fn stop_tracks(stream: &JsValue) {
    if let Ok(tracks) = call(stream, "getTracks", &[]) {
        for track in js_sys::Array::from(&tracks) {
            let _ = call(&track, "stop", &[]);
        }
    }
}
fn record_tick(start: f64) {
    let mut recording = None;
    with(|s| recording = s.recording.clone());
    if let Some(recorder) = recording {
        let seconds = ((now() - start) / 1000.0).round();
        note(&t("chatRecording").replace("{s}", &seconds.to_string()));
        if seconds >= 60.0 {
            let _ = call(&recorder, "stop", &[]);
        } else {
            later(500, move || record_tick(start));
        }
    }
}
async fn record() {
    let mut current = None;
    with(|s| current = s.recording.clone());
    if let Some(recorder) = current {
        let _ = call(&recorder, "stop", &[]);
        return;
    }
    let constraints = web_sys::MediaStreamConstraints::new();
    constraints.set_audio(&true.into());
    let stream = match window()
        .navigator()
        .media_devices()
        .ok()
        .and_then(|m| m.get_user_media_with_constraints(&constraints).ok())
    {
        Some(p) => JsFuture::from(p).await,
        None => Err(JsValue::NULL),
    };
    let Ok(stream) = stream else {
        note(&t("chatNoMic"));
        return;
    };
    let constructor = property(&js_sys::global(), "MediaRecorder");
    let options = js_sys::Object::new();
    for mime in [
        "audio/webm;codecs=opus",
        "audio/ogg;codecs=opus",
        "audio/mp4",
    ] {
        if call(&constructor, "isTypeSupported", &[mime.into()])
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            set(&options, "mimeType", &mime.into());
            set(&options, "audioBitsPerSecond", &32000.into());
            break;
        }
    }
    let recorder = constructor
        .dyn_into::<js_sys::Function>()
        .ok()
        .and_then(|c| js_sys::Reflect::construct(&c, &js_sys::Array::of2(&stream, &options)).ok());
    let Some(recorder) = recorder else {
        stop_tracks(&stream);
        note(&t("chatNoMic"));
        return;
    };
    let parts = Rc::new(RefCell::new(Vec::<Blob>::new()));
    let output = parts.clone();
    let data = Closure::wrap(Box::new(move |event: Event| {
        if let Ok(blob) = property(&event, "data").dyn_into::<Blob>() {
            if blob.size() > 0.0 {
                output.borrow_mut().push(blob);
            }
        }
    }) as Box<dyn FnMut(Event)>);
    let start = now();
    let current = recorder.clone();
    let stopped = Closure::wrap(Box::new(move |_: Event| {
        stop_tracks(&stream);
        let mime = property(&current, "mimeType")
            .as_string()
            .unwrap_or_else(|| "audio/webm".into());
        let options = web_sys::BlobPropertyBag::new();
        options.set_type(mime.split(';').next().unwrap_or("audio/webm"));
        let blobs: js_sys::Array = parts
            .borrow()
            .iter()
            .map(|b| JsValue::from(b.clone()))
            .collect();
        if let Ok(blob) = Blob::new_with_blob_sequence_and_options(&blobs, &options) {
            with(|s| {
                s.recording = None;
                attr("chat-rec", "aria-pressed", "false");
                note("");
                if blob.size() > 0.0 && blob.size() <= 2.0 * 1048576.0 {
                    attach(
                        s,
                        "audio",
                        blob,
                        t("chatVoice").replace(
                            "{s}",
                            &((now() - start) / 1000.0).round().max(1.0).to_string(),
                        ),
                    );
                } else if blob.size() > 2.0 * 1048576.0 {
                    error("media_size", None);
                }
            });
        }
    }) as Box<dyn FnMut(Event)>);
    set(&recorder, "ondataavailable", data.as_ref());
    set(&recorder, "onstop", stopped.as_ref());
    with(|s| {
        s.recording = Some(recorder.clone());
        s.record_callbacks = vec![data, stopped];
    });
    attr("chat-rec", "aria-pressed", "true");
    if call(&recorder, "start", &[]).is_err() {
        with(|s| s.recording = None);
        note(&t("chatNoMic"));
        return;
    }
    record_tick(start);
}
pub fn init() {
    if get("chat").is_none() {
        return;
    }
    with(|s| {
        s.view = "log".into();
        s.retry = 1000;
    });
    let hash = current_url().hash();
    if let Some(code) = hash.strip_prefix("#chat-code=").filter(|s| {
        regex::Regex::new(r"^[A-Za-z0-9_-]{16,128}$")
            .unwrap()
            .is_match(s)
    }) {
        store("chatCode", code);
        let url = current_url();
        url.set_hash("");
        let _ = window()
            .history()
            .and_then(|h| h.replace_state_with_url(&JsValue::NULL, "", Some(&url.href())));
        with(|s| s.imported = true);
    }
    click("chat-bubble", || with(|s| toggle(s, !s.open)));
    click("chat-close", || with(|s| toggle(s, false)));
    click("chat-gear", || {
        with(|s| {
            view(
                s,
                if s.view == "settings" {
                    "log"
                } else {
                    "settings"
                },
            )
        })
    });
    click("chat-back", || with(|s| view(s, "log")));
    click("chat-people", || {
        with(|s| {
            view(
                s,
                if s.view == "members" {
                    "log"
                } else {
                    "members"
                },
            )
        })
    });
    click("chat-members-back", || with(|s| view(s, "log")));
    on_id("chat", "keydown", |event| {
        if event
            .dyn_ref::<web_sys::KeyboardEvent>()
            .map(|e| e.key() == "Escape")
            .unwrap_or(false)
        {
            with(|s| {
                if s.view != "log" {
                    view(s, "log");
                } else {
                    toggle(s, false);
                }
            });
        }
    });
    click("chat-save", save_name);
    on_id("chat-name", "keydown", |event| {
        if event
            .dyn_ref::<web_sys::KeyboardEvent>()
            .map(|e| e.key() == "Enter")
            .unwrap_or(false)
        {
            event.prevent_default();
            if get("chat-name")
                .map(|e| !e.has_attribute("readonly"))
                .unwrap_or(false)
            {
                save_name();
            }
        }
    });
    click("chat-qr-show", || with(show_qr));
    click("chat-link", || {
        run(async {
            let link = link_to();
            if copy(&link).await {
                note(&t("chatLinkCopied"));
            } else {
                note(&link);
            }
        })
    });
    click("chat-file", || {
        if let Some(input) = get("chat-file-input") {
            let _ = call(&input, "click", &[]);
        }
    });
    on_id("chat-file-input", "change", |_| {
        let file = get("chat-file-input").and_then(|e| {
            property(&property(&e, "files"), "0")
                .dyn_into::<web_sys::File>()
                .ok()
        });
        set_value("chat-file-input", "");
        let Some(file) = file else {
            return;
        };
        let kind = if file.type_().starts_with("image/") {
            "image"
        } else if file.type_().starts_with("video/") {
            "video"
        } else {
            error("media_type", None);
            return;
        };
        let label = file.name();
        let blob: Blob = file.into();
        run(async move {
            let blob = if kind == "image" {
                shrink(blob).await
            } else {
                blob
            };
            if blob.size()
                > if kind == "image" {
                    3.0 * 1048576.0
                } else {
                    8.0 * 1048576.0
                }
            {
                error("media_size", None);
                return;
            }
            with(|s| attach(s, kind, blob, label));
        });
    });
    click("chat-attach-drop", || with(clear_attach));
    click("chat-rec", || run(record()));
    on_id("chat-text", "keydown", |event| {
        if event
            .dyn_ref::<web_sys::KeyboardEvent>()
            .map(|e| e.key() == "Enter" && !e.shift_key())
            .unwrap_or(false)
        {
            event.prevent_default();
            run(submit());
        }
    });
    on_id("chat-text", "input", |_| {
        if let Some(input) =
            get("chat-text").and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = input.style().set_property("height", "auto");
            let _ = input
                .style()
                .set_property("height", &format!("{}px", input.scroll_height()));
        }
    });
    on_id("chat-form", "submit", |event| {
        event.prevent_default();
        run(submit());
    });
    on(window().as_ref(), "resize", |_| with(|s| fit(s)));
    if let Ok(viewport) = property(&window(), "visualViewport").dyn_into::<web_sys::EventTarget>() {
        for name in ["resize", "scroll"] {
            on(&viewport, name, |_| with(|s| fit(s)));
        }
    }
    on(document().as_ref(), "bdl-language-change", |_| {
        with(|s| {
            render(s);
            me(s);
            members(s);
        })
    });
    connect();
}
