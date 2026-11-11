use crate::dom::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use wasm_bindgen::JsCast;
thread_local! {static UI:RefCell<Ui>=RefCell::new(Ui::default());}
#[derive(Default)]
struct Ui {
    fr: Value,
    en: Value,
    data: Value,
    boards: Vec<Value>,
    language: String,
    screen: String,
    rotating: bool,
}
pub fn t(key: &str) -> String {
    UI.with(|u| {
        let u = u.borrow();
        let dict = if u.language == "fr" { &u.fr } else { &u.en };
        dict[key]
            .as_str()
            .or_else(|| u.en[key].as_str())
            .unwrap_or(key)
            .to_string()
    })
}
pub fn translation(key: &str) -> Value {
    UI.with(|u| {
        let u = u.borrow();
        if u.language == "fr" {
            u.fr[key].clone()
        } else {
            u.en[key].clone()
        }
    })
}
pub fn data() -> Value {
    UI.with(|u| u.borrow().data.clone())
}
pub fn boards() -> Vec<Value> {
    UI.with(|u| u.borrow().boards.clone())
}
pub fn language() -> String {
    UI.with(|u| u.borrow().language.clone())
}
pub fn short(env: &str) -> String {
    match env {
        "m5stickc-plus2" => "M5StickC Plus2".into(),
        "m5stack-esp32" => "M5Stack (ESP32)".into(),
        "m5stack-s3" => "M5Stack (ESP32-S3)".into(),
        "lilygo-t-display" => "LilyGO T-Display".into(),
        "lilygo-t-display-s3" => "LilyGO T-Display-S3".into(),
        "esp32-2432s028r" => "Cheap Yellow Display".into(),
        "bitaxe-gamma-601" => "Bitaxe Gamma 601".into(),
        "nerdnos" => "NerdNOS".into(),
        e if e.ends_with("-headless") => {
            let chip = e
                .trim_end_matches("-headless")
                .replace("esp32s3", "ESP32-S3")
                .replace("esp32s2", "ESP32-S2")
                .replace("esp32c3", "ESP32-C3")
                .replace("esp32", "ESP32");
            format!(
                "{} {}",
                chip,
                if language() == "fr" {
                    "sans écran"
                } else {
                    "without a screen"
                }
            )
        }
        _ => env.into(),
    }
}
pub fn rate(value: f64) -> String {
    if value <= 0.0 || !value.is_finite() {
        return "0 H/s".into();
    }
    let mut value = value;
    let mut index = 0;
    let units = ["H/s", "kH/s", "MH/s", "GH/s", "TH/s", "PH/s", "EH/s"];
    while value >= 1000.0 && index < 6 {
        index += 1;
        value /= 1000.0;
    }
    format!(
        "{} {}",
        format(
            value,
            json!({"maximumFractionDigits":if value>=100.0{0}else if value>=10.0{1}else{2}})
        ),
        units[index]
    )
}
pub fn init() {
    let json_element = |id: &str| {
        get(id)
            .and_then(|e| e.text_content())
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .unwrap_or(Value::Null)
    };
    let data = json_element("site-data");
    let board_data = json_element("site-boards");
    let boards = board_data.as_array().cloned().unwrap_or_else(|| {
        json(&property(&js_sys::global(), "BDL_BOARDS"))
            .as_array()
            .cloned()
            .unwrap_or_default()
    });
    let mut dictionaries: Value = serde_json::from_str(include_str!("translations.json")).unwrap();
    let version = data["version"].as_str().unwrap_or_default();
    for lang in ["fr", "en"] {
        for (_, value) in dictionaries[lang].as_object_mut().unwrap() {
            if let Some(text) = value.as_str() {
                *value = text.replace("__VERSION__", version).into();
            }
        }
    }
    for (selector, key, attribute) in [
        ("[data-t]", "data-t", None),
        ("[data-h]", "data-h", Some("innerHTML")),
        ("[data-alt]", "data-alt", Some("alt")),
        ("[data-al]", "data-al", Some("aria-label")),
        ("[data-ph]", "data-ph", Some("placeholder")),
    ] {
        for element in all(selector) {
            let key = element.get_attribute(key).unwrap_or_default();
            let value = match attribute {
                Some("innerHTML") => element.inner_html(),
                Some(name) => element.get_attribute(name).unwrap_or_default(),
                None => element.text_content().unwrap_or_default(),
            };
            dictionaries["en"][key] = value.into();
        }
    }
    let language = read_storage("lang")
        .filter(|s| s == "fr" || s == "en")
        .unwrap_or_else(|| {
            if window()
                .navigator()
                .language()
                .unwrap_or_default()
                .starts_with("fr")
            {
                "fr".into()
            } else {
                "en".into()
            }
        });
    let rotating = get("screen").is_some()
        && !window()
            .match_media("(prefers-reduced-motion: reduce)")
            .ok()
            .flatten()
            .map(|m| m.matches())
            .unwrap_or(false);
    UI.with(|u| {
        *u.borrow_mut() = Ui {
            fr: dictionaries["fr"].clone(),
            en: dictionaries["en"].clone(),
            data,
            boards,
            language,
            screen: "mining".into(),
            rotating,
        }
    });
    click("lang", || {
        UI.with(|u| {
            let mut u = u.borrow_mut();
            u.language = if u.language == "fr" { "en" } else { "fr" }.into();
            store("lang", &u.language);
        });
        apply();
    });
    let theme = read_storage("theme")
        .filter(|s| s == "light")
        .unwrap_or_else(|| "dark".into());
    if let Some(root) = document().document_element() {
        let _ = root.set_attribute("data-theme", &theme);
    }
    update_theme();
    click("theme", || {
        if let Some(root) = document().document_element() {
            let theme = if root.get_attribute("data-theme").as_deref() == Some("dark") {
                "light"
            } else {
                "dark"
            };
            let _ = root.set_attribute("data-theme", theme);
            store("theme", theme);
        }
        update_theme();
        dispatch("bdl-theme-change");
    });
    click("menu", || {
        let open = get("site-links")
            .map(|e| !e.class_list().contains("open"))
            .unwrap_or(false);
        menu(open);
    });
    for link in all("#site-links a") {
        on(link.as_ref(), "click", |_| menu(false));
    }
    on(document().as_ref(), "click", |event| {
        let target = event
            .target()
            .and_then(|e| e.dyn_into::<web_sys::Element>().ok());
        if target
            .as_ref()
            .and_then(|e| e.closest("nav").ok().flatten())
            .is_none()
        {
            menu(false);
        }
        if let (Some(more), Some(target)) = (
            get("nav-more"),
            event
                .target()
                .and_then(|e| e.dyn_into::<web_sys::Node>().ok()),
        ) {
            if !more.contains(Some(&target)) {
                let _ = more.remove_attribute("open");
            }
        }
    });
    on(document().as_ref(), "keydown", |event| {
        if event
            .dyn_ref::<web_sys::KeyboardEvent>()
            .map(|e| e.key() == "Escape")
            .unwrap_or(false)
        {
            if let Some(more) = get("nav-more").filter(|e| e.has_attribute("open")) {
                let _ = more.remove_attribute("open");
                if let Some(summary) = more.query_selector("summary").ok().flatten() {
                    let _ = call(&summary, "focus", &[]);
                }
                return;
            }
            if get("site-links")
                .map(|e| e.class_list().contains("open"))
                .unwrap_or(false)
            {
                menu(false);
                focus("menu");
            }
        }
    });
    for button in all("[data-screen]") {
        let name = button.get_attribute("data-screen").unwrap_or_default();
        on(button.as_ref(), "click", move |_| {
            UI.with(|u| {
                let mut u = u.borrow_mut();
                u.rotating = false;
                u.screen = if u.screen == name {
                    "mining".into()
                } else {
                    name.clone()
                };
            });
            show_screen();
        });
    }
    interval(4000, || {
        UI.with(|u| {
            let mut u = u.borrow_mut();
            if u.rotating {
                let screens = ["mining", "graph", "global", "clock"];
                let index = screens.iter().position(|s| *s == u.screen).unwrap_or(0);
                u.screen = screens[(index + 1) % 4].into();
            }
        });
        show_screen();
    });
}
fn menu(open: bool) {
    if let Some(links) = get("site-links") {
        class(&links, "open", open);
    }
    attr("menu", "aria-expanded", if open { "true" } else { "false" });
    if !open {
        remove_attr("nav-more", "open");
    }
}
fn update_theme() {
    let dark = document()
        .document_element()
        .and_then(|r| r.get_attribute("data-theme"))
        .as_deref()
        == Some("dark");
    attr("theme", "aria-pressed", if dark { "true" } else { "false" });
    if let Some(meta) = query("meta[name=theme-color]") {
        let _ = meta.set_attribute("content", if dark { "#141415" } else { "#fafafa" });
    }
}
fn show_screen() {
    if get("screen").is_none() {
        return;
    }
    let name = UI.with(|u| u.borrow().screen.clone());
    let suffix = if language() == "fr" { "-fr" } else { "" };
    attr(
        "screen",
        "src",
        &format!("assets/screen-{name}{suffix}.png"),
    );
    attr(
        "screen",
        "alt",
        &t(match name.as_str() {
            "graph" => "altGraph",
            "global" => "altGlobal",
            "clock" => "altClock",
            _ => "altMining",
        }),
    );
    for button in all("[data-screen]") {
        let value = button.get_attribute("data-screen").unwrap_or_default();
        let _ = button.set_attribute("aria-pressed", if value == name { "true" } else { "false" });
        if let Some(image) = button.query_selector("img").ok().flatten() {
            let _ = image.set_attribute("src", &format!("assets/screen-{value}{suffix}.png"));
        }
    }
}
pub fn apply() {
    if let Some(root) = document().document_element() {
        let _ = root.set_attribute("lang", &language());
    }
    for (selector, key, attribute) in [
        ("[data-t]", "data-t", None),
        ("[data-h]", "data-h", Some("innerHTML")),
        ("[data-al]", "data-al", Some("aria-label")),
        ("[data-alt]", "data-alt", Some("alt")),
        ("[data-ph]", "data-ph", Some("placeholder")),
    ] {
        for element in all(selector) {
            let value = t(&element.get_attribute(key).unwrap_or_default());
            match attribute {
                Some("innerHTML") => element.set_inner_html(&value),
                Some(name) => {
                    let _ = element.set_attribute(name, &value);
                }
                None => element.set_text_content(Some(&value)),
            }
        }
    }
    text(
        "lang",
        if language() == "fr" {
            "English"
        } else {
            "Français"
        },
    );
    show_screen();
    let rows = boards()
        .into_iter()
        .map(|board| {
            let name = string(&board["name"]);
            let experimental = name.to_lowercase().contains("experimental");
            let name = if experimental {
                regex::Regex::new(r"(?i)\s*\(experimental[^)]*\)")
                    .unwrap()
                    .replace(&name, "")
                    .to_string()
                    + " ("
                    + &t("experimental")
                    + ")"
            } else {
                name
            };
            format!(
                "<li class=\"{}\">{}</li>",
                if experimental { "exp" } else { "" },
                escape(&name)
            )
        })
        .collect::<String>();
    html("board-list", &rows);
    dispatch("bdl-language-change");
}
