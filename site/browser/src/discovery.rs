use crate::{dom::*, ui};
use serde_json::Value;
use std::{cell::RefCell, sync::LazyLock};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};
use wasm_bindgen::JsCast;
static COPY: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("discovery.json")).unwrap());
thread_local! {static STATE:RefCell<State>=RefCell::new(State{route:"first".into(),filter:"measured".into(),expanded:false,demo:"miner".into(),query:String::new()});}
struct State {
    route: String,
    filter: String,
    expanded: bool,
    demo: String,
    query: String,
}
fn with(f: impl FnOnce(&mut State)) {
    STATE.with(|s| f(&mut s.borrow_mut()));
}
fn t(key: &str) -> String {
    COPY["copy"][ui::language()][key]
        .as_str()
        .unwrap_or(key)
        .into()
}
fn brief(key: &str) -> String {
    COPY["concise"][ui::language()][key]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| t(key))
}
fn choose(value: &Value) -> String {
    if value.is_object() {
        value[ui::language()]
            .as_str()
            .or_else(|| value["en"].as_str())
            .unwrap_or_default()
            .into()
    } else {
        string(value)
    }
}
fn link(url: &str, label: &str, class: &str) -> String {
    format!(
        "<a href=\"{}\"{}>{}</a>",
        escape(url),
        if class.is_empty() {
            String::new()
        } else {
            format!(" class=\"{}\"", escape(class))
        },
        escape(label)
    )
}
fn heading(kicker: &str, title: &str, lead: &str) -> String {
    format!(
        "<p class=\"section-kicker\">{}</p><h1>{}</h1><p class=\"section-intro\">{}</p>",
        escape(&brief(kicker)),
        escape(&brief(title)),
        escape(&brief(lead))
    )
}
fn tab_keys(selector: &str, activate: fn(String)) {
    let buttons = all(selector);
    let values: Vec<_> = buttons
        .iter()
        .map(|b| b.get_attribute("data-value").unwrap_or_default())
        .collect();
    for (index, button) in buttons.into_iter().enumerate() {
        let values = values.clone();
        on(button.as_ref(), "keydown", move |event| {
            let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>().map(|e| e.key()) else {
                return;
            };
            if !["ArrowLeft", "ArrowRight", "Home", "End"].contains(&key.as_str()) {
                return;
            }
            event.prevent_default();
            let next = match key.as_str() {
                "Home" => 0,
                "End" => values.len() - 1,
                "ArrowRight" => (index + 1) % values.len(),
                _ => (index + values.len() - 1) % values.len(),
            };
            activate(values[next].clone());
        });
    }
}
fn route_activate(route: String) {
    with(|s| {
        s.route = route.clone();
        start(s);
    });
    focus(&format!("route-{route}"));
}
fn demo_activate(demo: String) {
    with(|s| {
        s.demo = demo.clone();
        demo_view(s);
    });
    focus(&format!("demo-tab-{demo}"));
}
fn start(s: &State) {
    let first = s.route == "first";
    let axe = s.route == "axeos";
    let mut out = heading("kicker", "startTitle", "startLead");
    out += &format!(
        "<div class=\"choice-tabs\" role=\"tablist\" aria-label=\"{}\">",
        escape(&t("discover"))
    );
    for value in ["first", "update", "axeos"] {
        out+=&format!("<button type=\"button\" role=\"tab\" id=\"route-{value}\" aria-controls=\"route-content\" aria-selected=\"{}\" tabindex=\"{}\" data-value=\"{value}\">{}</button>",value==s.route,if value==s.route{0}else{-1},escape(&t(value)));
    }
    let steps = COPY["copy"][ui::language()][format!("{}Steps", s.route)]
        .as_array()
        .into_iter()
        .flatten()
        .map(|step| format!("<li>{}</li>", escape(&string(step))))
        .collect::<String>();
    let guide = format!(
        "https://github.com/bdlglua/BDLMinerV2/blob/main/{}",
        if axe {
            "docs/hardware/bitaxe-gamma-601.md"
        } else {
            "docs/guide.md"
        }
    );
    out+=&format!("</div><div class=\"route-panel\" id=\"route-content\" role=\"tabpanel\" aria-labelledby=\"route-{}\"><div><h3>{}</h3><ol>{steps}</ol></div><aside class=\"route-aside\"><p class=\"route-note\">{}</p>{}<p>{}</p></aside></div>",s.route,escape(&t(&format!("{}Title",s.route))),escape(&t(&format!("{}Note",s.route))),link(if first{"boards.html"}else if axe{"flash.html?board=bitaxe-gamma-601"}else{"flash.html"},&t(if first{"choose"}else{"install"}),"btn"),link(&guide,&t("guide"),"section-inline-link"));
    let guide = format!(
        "https://github.com/bdlglua/BDLMinerV2/blob/main/docs/remote-access{}.md",
        if ui::language() == "fr" { ".fr" } else { "" }
    );
    out+=&format!("<aside class=\"gallery-invite\"><h3>{}</h3><p>{}</p><div class=\"buttons\">{}{}</div></aside>",tr("Vos mineurs, même loin de chez vous.","Your miners, even away from home."),tr("Avec BDLMiner 2.5.4, activez le suivi sur vos cartes puis saisissez votre adresse de paiement pour voir leurs mesures et sept jours d’historique. Rien à copier, aucun ordinateur à laisser allumé ; envoi facultatif toutes les cinq minutes.","With BDLMiner 2.5.4, enable monitoring on your boards, then enter your payout address to view their readings and seven-day history. Nothing to copy, no computer left on; optional reports every five minutes."),link("remote.html",tr("Mes mineurs","My miners"),"btn"),link(&guide,&t("guide"),"btn ghost"));
    html("discovery-start", &out);
    for value in ["first", "update", "axeos"] {
        click(&format!("route-{value}"), move || {
            route_activate(value.into())
        });
    }
    tab_keys("#discovery-start [role=tab]", route_activate);
}
fn demo_view(s: &State) {
    let url = format!("demo/{}/index.html?lang={}", s.demo, ui::language());
    let mut out=heading("demoKicker","demoTitle","demoLead")+&format!("<div class=\"demo-shell\"><div class=\"demo-toolbar\"><div class=\"demo-tabs\" role=\"tablist\" aria-label=\"{}\">",escape(&t("demoTitle")));
    for value in ["miner", "hub"] {
        out+=&format!("<button id=\"demo-tab-{value}\" type=\"button\" role=\"tab\" data-value=\"{value}\" aria-selected=\"{}\" aria-controls=\"demo-frame\" tabindex=\"{}\">{}</button>",s.demo==value,if s.demo==value{0}else{-1},escape(&t(value)));
    }
    out+=&format!("</div><span class=\"demo-label\">{}</span></div><iframe id=\"demo-frame\" title=\"{}\" src=\"{}\" loading=\"lazy\" sandbox=\"allow-scripts allow-same-origin allow-forms allow-downloads allow-popups\" referrerpolicy=\"no-referrer\"></iframe></div><div class=\"demo-caption\"><p>{}</p><a href=\"{}\" target=\"_blank\" rel=\"noopener\">{} ↗</a></div>",escape(&t("demoBadge")),escape(&(t(&s.demo)+" — "+&t("demoBadge"))),escape(&url),escape(&t("demoCaption")),escape(&url),escape(&t("openDemo")));
    html("discovery-demo", &out);
    for value in ["miner", "hub"] {
        click(&format!("demo-tab-{value}"), move || {
            demo_activate(value.into())
        });
    }
    tab_keys("#discovery-demo [role=tab]", demo_activate);
}
fn comparison(s: &State) {
    let content = ui::data();
    let matches: Vec<_> = content["boards"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| {
            s.filter == "all"
                || (s.filter == "measured" && b["measured"] == true)
                || b["family"] == s.filter
        })
        .collect();
    let count = if s.expanded {
        matches.len()
    } else {
        matches.len().min(6)
    };
    let mut out=format!("<h3 class=\"comparison-heading\">{}</h3><p class=\"section-intro\">{}</p><div class=\"filter-tabs\" role=\"group\" aria-label=\"{}\">",escape(&brief("compareTitle")),escape(&brief("compareLead")),escape(&brief("compareTitle")));
    for (value, label) in [
        ("measured", t("measured")),
        ("all", t("all")),
        ("esp32", "ESP32".into()),
        ("s3", "ESP32-S3".into()),
        ("s2c3", "S2 / C3".into()),
        ("asic", t("asic")),
    ] {
        out += &format!(
            "<button type=\"button\" data-filter=\"{value}\" aria-pressed=\"{}\">{}</button>",
            s.filter == value,
            escape(&label)
        );
    }
    out += &format!("</div><div class=\"comparison-grid\" data-count=\"{count}\">");
    for board in matches.iter().take(count) {
        let measured = board["measured"] == true;
        let picture = if let Some(image) = board["image"].as_str().filter(|s| !s.is_empty()) {
            format!("<figure><img src=\"{}\" alt=\"{}\" loading=\"lazy\" decoding=\"async\" width=\"640\" height=\"420\"><figcaption>{}</figcaption></figure>",escape(image),escape(&(string(&board["name"])+" — "+&t("photo"))),escape(&if board["photo_caption"].is_null(){t("photo")}else{choose(&board["photo_caption"])}))
        } else {
            format!("<figure class=\"board-unpictured\"><svg aria-hidden=\"true\" viewBox=\"0 0 64 64\"><rect x=\"15\" y=\"15\" width=\"34\" height=\"34\" rx=\"6\"/><path d=\"M23 5v10m9-10v10m9-10v10M23 49v10m9-10v10m9-10v10M5 23h10M5 32h10M5 41h10m34-18h10M49 32h10M49 41h10\"/></svg><span>{}</span></figure>",escape(&t("noPhoto")))
        };
        let rate = finite(&board["rate"]).unwrap_or(0.0);
        let mut value = rate;
        let mut i = 0;
        while value >= 1000.0 && i < 4 {
            value /= 1000.0;
            i += 1;
        }
        let rate = format!(
            "{} {}",
            format(
                value,
                serde_json::json!({"maximumFractionDigits":2,"minimumFractionDigits":if i>=2{2}else{0}})
            ),
            ["H/s", "kH/s", "MH/s", "GH/s", "TH/s"][i]
        );
        out+=&format!("<article class=\"board-card\">{picture}<div class=\"board-card-body\"><div class=\"board-tags\"><span class=\"board-tag {}\">{}</span>{}</div><h3>{}</h3><p class=\"board-chip\">{}</p><p class=\"board-rate {}\">{}</p><p class=\"board-detail\">{} : {}</p>{}<div class=\"board-actions\"><button type=\"button\" data-board-env=\"{}\">{}</button>{}</div></div></article>",if measured{"measured"}else{""},escape(&t(if measured{"measurement"}else{"build"})),if board["experimental"]==true{format!("<span class=\"board-tag experimental\">{}</span>",escape(&t("experimental")))}else{String::new()},escape(&string(&board["name"])),escape(&string(&board["chip"])),if measured{""}else{"pending"},escape(&if measured{rate}else{t("pending")}),escape(&t("display")),escape(&string(&board["display"])),if board["note"].is_null(){String::new()}else{format!("<p class=\"board-detail\">{}</p>",escape(&choose(&board["note"])))},escape(&string(&board["env"])),escape(&t("select")),link(&string(&board["source"]),&t("source"),""));
    }
    out += "</div>";
    if matches.len() > 6 {
        out += &format!(
            "<button type=\"button\" class=\"btn ghost comparison-more\">{} ({})</button>",
            escape(&t(if s.expanded { "fewer" } else { "more" })),
            matches.len()
        );
    }
    out += &format!(
        "<p class=\"comparison-caption\">{} {}</p>",
        escape(&t("photoNote")),
        link("assets/boards/CREDITS.html", &t("credits"), "")
    );
    html("board-comparison", &out);
    for button in all("#board-comparison [data-filter]") {
        let filter = button.get_attribute("data-filter").unwrap_or_default();
        on(button.as_ref(), "click", move |_| {
            with(|s| {
                s.filter = filter.clone();
                s.expanded = false;
                comparison(s);
            });
            if let Some(el) = query(&format!("#board-comparison [data-filter='{filter}']")) {
                let _ = call(&el, "focus", &[]);
            }
        });
    }
    for button in all("#board-comparison [data-board-env]") {
        let env = button.get_attribute("data-board-env").unwrap_or_default();
        on(button.as_ref(), "click", move |_| {
            if ui::boards().iter().any(|b| b["env"] == env) {
                store("board", &env);
                let _ = window().location().assign(&format!(
                    "flash.html?board={}",
                    js_sys::encode_uri_component(&env)
                ));
            }
        });
    }
    if let Some(button) = query("#board-comparison .comparison-more") {
        on(button.as_ref(), "click", |_| {
            with(|s| {
                s.expanded = !s.expanded;
                comparison(s);
            })
        });
    }
}
fn fold(text: &str) -> String {
    text.nfd()
        .filter(|c| !is_combining_mark(*c) && c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
fn faq_rows(s: &State) {
    let needle = fold(&s.query);
    let lang = ui::language();
    let mut rows = String::new();
    for entry in COPY["faq"].as_array().into_iter().flatten() {
        let texts = &entry[&lang];
        let title = texts[0].as_str().unwrap_or_default();
        let body = texts[1].as_str().unwrap_or_default();
        if !fold(&(title.to_string() + body)).contains(&needle) {
            continue;
        }
        let url = entry["url"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "https://github.com/bdlglua/BDLMinerV2/blob/main/docs/guide.md#{}",
                    entry["anchor"].as_str().unwrap_or_default()
                )
            });
        rows += &format!(
            "<details><summary>{}</summary><p>{}</p>{}</details>",
            escape(title),
            escape(body),
            link(&url, &t("guide"), "")
        );
    }
    hidden("faq-empty", !rows.is_empty());
    html("faq-list", &rows);
}
fn faq(s: &State) {
    html("discovery-help",&(heading("faqKicker","faqTitle","faqLead")+&format!("<label class=\"faq-search\"><span class=\"s\">{}</span><input type=\"search\" id=\"faq-search\" autocomplete=\"off\" value=\"{}\" placeholder=\"USB, Wi-Fi, shares…\"></label><div class=\"faq-list\" id=\"faq-list\"></div><p id=\"faq-empty\" class=\"s\" role=\"status\" hidden>{}</p>",escape(&t("search")),escape(&s.query),escape(&t("noFaq")))));
    faq_rows(s);
    on_id("faq-search", "input", |_| {
        with(|s| {
            s.query = value("faq-search");
            faq_rows(s);
        })
    });
}
fn news() {
    let content = ui::data();
    let stable = &content["channels"]["stable"];
    let beta = &content["channels"]["beta"];
    let rust = &content["channels"]["rust"];
    let same = stable["version"] == beta["version"];
    // Each channel's installer is a folder of its own: stable at the root,
    // beta/ and rust/ below it.
    let here = content["channel"].as_str().unwrap_or("stable");
    let path = |channel: &str| match (here, channel) {
        (a, b) if a == b => "./flash.html".to_string(),
        ("stable", c) => format!("{c}/flash.html"),
        (_, "stable") => "../flash.html".to_string(),
        (_, c) => format!("../{c}/flash.html"),
    };
    let mut out = heading("newsKicker", "newsTitle", "betaNote") + "<div class=\"release-grid\">";
    let mut cards = vec![
        ("stable", stable, "release-card stable"),
        ("beta", beta, "release-card"),
    ];
    if rust["version"].is_string() {
        cards.push(("rust", rust, "release-card"));
    }
    for (channel, item, class) in cards {
        let version = string(&item["version"]);
        let date = if item["published_at"].is_string() {
            format!(
                "<p class=\"release-date\">{} {}</p>",
                escape(&t("published")),
                escape(&crate::dom::date(
                    &item["published_at"],
                    serde_json::json!({"dateStyle":"medium"})
                ))
            )
        } else {
            String::new()
        };
        let notes = if channel == "stable" {
            let notes = content["release_notes"][&version][ui::language()]
                .as_array()
                .cloned()
                .unwrap_or_else(|| vec![Value::String(t("newRelease"))]);
            format!(
                "<ul>{}</ul>",
                notes
                    .iter()
                    .map(|v| format!("<li>{}</li>", escape(&string(v))))
                    .collect::<String>()
            )
        } else {
            let note = match channel {
                "rust" => "rustNote",
                _ if same => "sameBeta",
                _ => "betaNote",
            };
            format!("<p class=\"route-note\">{}</p>", escape(&t(note)))
        };
        let (installer, tag) = match channel {
            "stable" => ("stableInstaller", "v"),
            "beta" => ("betaInstaller", "v"),
            _ => ("rustInstaller", "rust-v"),
        };
        out+=&format!("<article class=\"{class}\"><span class=\"board-tag{}\">{}</span><h3>BDLMiner {}</h3>{date}{notes}<div class=\"buttons\">{}{}</div></article>",if channel=="stable"{" measured"}else{""},escape(&t(channel)),escape(&version),link(&path(channel),&t(installer),if channel=="stable"{"btn"}else{"btn ghost"}),link(&format!("https://github.com/bdlglua/BDLMinerV2/releases/tag/{tag}{}",js_sys::encode_uri_component(&version)),&t("releaseNotes"),"btn ghost"));
    }
    out += "</div>";
    html("discovery-news", &out);
}
fn gallery() {
    let content = ui::data();
    let rows = content["gallery"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let submit = "https://github.com/bdlglua/BDLMinerV2/issues/new?template=community_setup.yml";
    let mut out = heading("galleryKicker", "galleryTitle", "galleryLead");
    if rows.is_empty() {
        out+=&format!("<div class=\"gallery-invite\"><h3>{}</h3><p>{}</p><div class=\"buttons\">{}{}{}</div></div>",escape(&t("galleryEmptyTitle")),escape(&t("galleryEmpty")),link(submit,&t("submit"),"btn"),link("https://www.reddit.com/r/bdlminer/",&t("reddit"),"btn ghost"),link("https://discord.gg/S8kSgD6Xe9","Discord","btn ghost"));
    } else {
        out += "<div class=\"gallery-grid\">";
        for entry in rows {
            out+=&format!("<figure class=\"gallery-card\"><img src=\"{}\" alt=\"{}\" loading=\"lazy\" width=\"640\" height=\"480\"><figcaption><h3>{}</h3><p>{}</p>{}</figcaption></figure>",escape(&string(&entry["image"])),escape(&choose(&entry["caption"])),escape(&string(&entry["board"])),escape(&choose(&entry["caption"])),link(&string(&entry["url"]),&(string(&entry["credit"])+" ↗"),""));
        }
        out += "</div>";
    }
    out += &format!(
        "<p class=\"s gallery-footer\">{}{}</p>",
        escape(&t("galleryNote")),
        if rows.is_empty() {
            String::new()
        } else {
            format!(" {}", link(submit, &t("submit"), ""))
        }
    );
    html("discovery-gallery", &out);
}
fn support() {
    let content = ui::data();
    let support = &content["support"];
    let links = support["links"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let available = !links.is_empty()
        || support["bitcoin"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
        || support["lightning"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
    hidden("support", !available);
    hidden("nav-support", !available);
    if !available {
        return;
    }
    let mut out = format!(
        "<div class=\"support-panel\">{}<div class=\"funding-grid\">",
        heading("supportKicker", "supportTitle", "supportLead")
    );
    for key in ["hardware", "development", "services"] {
        out += &format!(
            "<div><h3>{}</h3><p>{}</p></div>",
            escape(&t(key)),
            escape(&t(&format!("{key}Text")))
        );
    }
    out += "</div><div class=\"buttons\">";
    for entry in links {
        out += &link(&string(&entry["url"]), &choose(&entry["label"]), "btn");
    }
    out += "</div>";
    for key in ["bitcoin", "lightning"] {
        let Some(value) = support[key].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        out+=&format!("<div class=\"donation-address\"><strong>{}</strong><code>{}</code><div class=\"buttons\"><button type=\"button\" data-copy=\"{key}\">{}</button>{}</div></div>",if key=="bitcoin"{"Bitcoin"}else{"Lightning"},escape(value),escape(&t("copy")),link(&format!("{key}:{value}"),&t("wallet"),"btn ghost"));
    }
    out+=&format!("<p id=\"funding-status\" class=\"funding-status\" role=\"status\" aria-live=\"polite\"></p><p class=\"s\">{}</p></div>",escape(&t("supportNote")));
    html("discovery-support", &out);
    for button in all("#discovery-support [data-copy]") {
        let key = button.get_attribute("data-copy").unwrap_or_default();
        let value = string(&support[&key]);
        on(button.as_ref(), "click", move |_| {
            let value = value.clone();
            run(async move {
                let ok = copy(&value).await;
                text(
                    "funding-status",
                    &t(if ok { "copied" } else { "copyFailed" }),
                );
            });
        });
    }
}
fn render(s: &State) {
    let content = ui::data();
    text("nav-discover", &t("discover"));
    text("nav-support", &t("support"));
    let lang = ui::language();
    let page = content["page"].as_str().unwrap_or("home");
    if let Some(title) = content["page_titles"][page][lang].as_str() {
        document().set_title(&format!("BDLMiner V2: {title}"));
    }
    let current = if page == "home" {
        "index.html".into()
    } else {
        format!("{page}.html")
    };
    for link in all("#site-links a,.footer-nav a") {
        if link.get_attribute("href").as_deref() == Some(&current) {
            let _ = link.set_attribute("aria-current", "page");
        }
    }
    if query("#nav-more [aria-current=page]").is_some() {
        attr("nav-more", "data-current", "true");
    }
    if get("discovery-start").is_some() {
        start(s);
    }
    if get("discovery-demo").is_some() {
        demo_view(s);
    }
    if get("board-comparison").is_some() {
        comparison(s);
    }
    if get("discovery-help").is_some() {
        faq(s);
    }
    if get("discovery-news").is_some() {
        news();
    }
    if get("discovery-gallery").is_some() {
        gallery();
    }
    if get("discovery-support").is_some() {
        support();
    }
}
pub fn init() {
    let data = ui::data();
    if data["page"] == "home" {
        let hash = window().location().hash().unwrap_or_default();
        if hash != "#top" {
            if let Some(target) = data["legacy_sections"][hash.trim_start_matches('#')].as_str() {
                if let Ok(url) = web_sys::Url::new_with_base(target, &current_url().href()) {
                    url.set_search(&current_url().search());
                    let _ = window().location().replace(&url.href());
                }
            }
        }
    }
    on(document().as_ref(), "bdl-language-change", |_| {
        with(|s| render(s))
    });
}
