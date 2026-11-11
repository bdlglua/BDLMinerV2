use crate::{
    chart,
    dom::*,
    ui::{self, rate, short, t},
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::future_to_promise;
const API: &str = "https://api.bdl-pool.com:36018/api/summary";
const SNAPSHOT: &str =
    "https://raw.githubusercontent.com/bdlglua/BDLMinerV2/public-stats/summary.json";
thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
#[derive(Default)]
struct State {
    summary: Option<Value>,
    fetched: f64,
    failed: bool,
    loading: bool,
    failures: u32,
    retry: i32,
    all: bool,
    cards: String,
    table: String,
    query: String,
    inspect: Option<usize>,
}
fn with(f: impl FnOnce(&mut State)) {
    STATE.with(|s| f(&mut s.borrow_mut()));
}

fn active() -> bool {
    get("stats").is_some() || get("home-market").is_some()
}
fn endpoint() -> String {
    let url = current_url();
    if url.search_params().get("statsApi").as_deref() == Some("go")
        && ["localhost", "127.0.0.1", "[::1]"].contains(&url.hostname().as_str())
    {
        "http://127.0.0.1:8787/api/summary".into()
    } else {
        API.into()
    }
}
fn cache_key() -> String {
    format!("bdlminer:public-stats:v1:{}", endpoint())
}
pub fn valid(value: &Value) -> bool {
    value["at"].as_f64().map(|v| v.is_finite()).unwrap_or(false)
        && finite(&value["hashrate"]).is_some()
        && value["miners"].as_u64().is_some()
        && [
            "boards", "versions", "coins", "pools", "fleet", "models", "recent", "history",
        ]
        .iter()
        .all(|key| {
            value[*key].is_null()
                || value[*key]
                    .as_array()
                    .map(|rows| rows.iter().all(Value::is_object))
                    .unwrap_or(false)
        })
}
fn cached() -> Option<Value> {
    let raw = read_storage(&cache_key()).filter(|s| s.len() <= 512 * 1024)?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    let saved = value["savedAt"].as_f64()?;
    let data = &value["summary"];
    let age = now() - data["at"].as_f64()?;
    if !valid(data)
        || !(-60000.0..=600000.0).contains(&(now() - saved))
        || !(-60000.0..=600000.0).contains(&age)
    {
        return None;
    }
    Some(data.clone())
}
fn save(data: &Value) {
    let encoded = json!({"savedAt":now(),"summary":data}).to_string();
    if encoded.len() <= 512 * 1024 {
        store(&cache_key(), &encoded);
    }
}
fn expose(s: &State) {
    let global = js_sys::global();
    for (key, value) in [
        ("statsLoading", json!(s.loading)),
        ("statsFailed", json!(s.failed)),
        ("fetchedAt", json!(s.fetched)),
        ("summary", s.summary.clone().unwrap_or(Value::Null)),
        ("statsCacheKey", json!(cache_key())),
    ] {
        set(&global, key, &js(&value));
    }
}
fn family(env: &str) -> &'static str {
    if env.contains("gamma") || env.contains("bitaxe") || env.contains("nerdnos") {
        "asic"
    } else if env.contains("s3") {
        "s3"
    } else if env.contains("s2") || env.contains("c3") {
        "s2c3"
    } else {
        "esp32"
    }
}
fn family_label(f: &str) -> String {
    t(match f {
        "asic" => "famAsic",
        "s3" => "famS3",
        "s2c3" => "famS2c3",
        _ => "famEsp32",
    })
}
fn whole(value: &Value) -> u64 {
    value
        .as_u64()
        .filter(|n| *n <= 9_007_199_254_740_991)
        .unwrap_or(0)
}
fn connection(s: &State) {
    let age = s
        .summary
        .as_ref()
        .and_then(|v| v["at"].as_f64())
        .map(|at| now() - at)
        .unwrap_or(f64::INFINITY);
    let fresh = s.summary.is_some()
        && s.fetched > 0.0
        && !s.failed
        && (-60000.0..=60000.0).contains(&age)
        && now() - s.fetched <= 60000.0;
    let message = t(if s.summary.is_some() {
        "statsRetained"
    } else if s.failed {
        "statsDown"
    } else {
        "statsLoading"
    });
    text(
        "st-live-label",
        &t(if fresh {
            "live"
        } else if s.summary.is_some() {
            "statsSaved"
        } else if s.failed {
            "statsDown"
        } else {
            "statsLoading"
        }),
    );
    if let Some(el) = get("st-live") {
        class(&el, "is-stale", !fresh);
        if let Some(dot) = el.query_selector(".dot").ok().flatten() {
            let _ = dot.set_attribute("class", if fresh { "dot on" } else { "dot none" });
        }
    }
    text(
        "st-updated",
        &if s.summary.is_some() {
            t("updated").replace("{s}", &(age / 1000.0).round().max(0.0).to_string())
        } else {
            String::new()
        },
    );
    hidden("hm-status", fresh);
    text(
        "hm-status",
        &if fresh {
            String::new()
        } else {
            message.clone()
        },
    );
    if fresh {
        if s.summary
            .as_ref()
            .map(|v| v["miners"] == 0 && v["history"].as_array().map(Vec::is_empty).unwrap_or(true))
            .unwrap_or(false)
        {
            text("st-status", &t("noStats"));
        } else {
            let content = t("statsMore");
            if get("st-status")
                .map(|el| el.inner_html() != content)
                .unwrap_or(false)
            {
                html("st-status", &content);
            }
        }
    } else {
        text("st-status", &message);
    }
}
fn fleet(s: &State) {
    if get("st-fleet").is_none() {
        return;
    }
    let data = s.summary.as_ref().unwrap_or(&Value::Null);
    let boards = ui::boards();
    let known: HashSet<_> = boards.iter().filter_map(|b| b["env"].as_str()).collect();
    let seen: HashMap<_, _> = data["fleet"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| Some((f["board"].as_str()?, f)))
        .collect();
    let mut envs: Vec<_> = boards.iter().filter_map(|b| b["env"].as_str()).collect();
    envs.extend(seen.keys().filter(|k| !known.contains(**k)).copied());
    let mut rows: Vec<_> = envs
        .into_iter()
        .map(|env| {
            (
                env,
                seen.get(env).map(|v| whole(&v["online"])).unwrap_or(0),
                seen.get(env).map(|v| whole(&v["offline"])).unwrap_or(0),
            )
        })
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
    let seen = rows.iter().filter(|r| r.1 > 0 || r.2 > 0).count();
    let folded = rows.len() - seen;
    hidden("st-more", seen == 0 || folded == 0);
    text(
        "st-more",
        &t(if s.all { "devFewer" } else { "devMore" }).replace("{n}", &folded.to_string()),
    );
    attr(
        "st-more",
        "aria-expanded",
        if s.all { "true" } else { "false" },
    );
    let (on, off) = rows
        .iter()
        .fold((0_u64, 0_u64), |(on, off), r| (on + r.1, off + r.2));
    let mut html_out = String::new();
    for (env, on, off) in rows {
        if seen > 0 && !s.all && on == 0 && off == 0 {
            continue;
        }
        let label = short(env);
        let state = if on > 0 {
            "on"
        } else if off > 0 {
            "off"
        } else {
            "none"
        };
        let plain = |v: &str| {
            v.to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
        };
        let models = data["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| {
                m["board"] == env
                    && m["model"]
                        .as_str()
                        .map(|n| {
                            !n.is_empty() && plain(n) != plain(env) && plain(n) != plain(&label)
                        })
                        .unwrap_or(false)
            })
            .map(|m| {
                string(&m["model"])
                    + &if whole(&m["miners"]) > 1 {
                        format!(" ×{}", whole(&m["miners"]))
                    } else {
                        String::new()
                    }
            })
            .collect::<Vec<_>>();
        let mut parts = Vec::new();
        if on > 0 {
            parts.push(format!("<b>{on}</b> {}", escape(&t("devOnline"))));
        }
        if off > 0 {
            parts.push(format!("<b>{off}</b> {}", escape(&t("devOffline"))));
        }
        html_out+=&format!("<li class=\"{state}\"><span class=\"dot {state}\" role=\"img\" aria-label=\"{}\"></span><span class=\"name\">{}{}</span><span class=\"count\">{}</span></li>",escape(&t(match state{"on"=>"devOnline","off"=>"devOffline",_=>"devNone"})),escape(&label),if models.is_empty(){String::new()}else{format!("<span class=\"models\">{}</span>",escape(&models.join(", ")))},if parts.is_empty(){escape(&t("devNone"))}else{parts.join(" · ")});
    }
    html("st-fleet", &html_out);
    text(
        "st-on",
        &if s.summary.is_some() {
            on.to_string()
        } else {
            "–".into()
        },
    );
    text(
        "st-off",
        &if s.summary.is_some() {
            off.to_string()
        } else {
            "–".into()
        },
    );
}
fn table(
    id: &str,
    rows: &[Value],
    header: &str,
    label: impl Fn(&str) -> String,
    website: impl Fn(&str) -> Option<String>,
) {
    if rows.is_empty() {
        html(
            id,
            &format!(
                "<tbody><tr><td class=\"empty\">{}</td></tr></tbody>",
                escape(&t("noneNow"))
            ),
        );
        return;
    }
    let mut out=format!("<thead><tr><th>{}</th><th class=\"num\">{}</th><th class=\"num\">{}</th></tr></thead><tbody>",escape(&t(header)),escape(&t("colMiners")),escape(&t("colHashrate")));
    for row in rows {
        let name = string(&row["name"]);
        let title = label(&name);
        let content=website(&name).map(|url|format!("<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}<small class=\"pool-site\">{}</small></a>",escape(&url),escape(&title),escape(&url))).unwrap_or_else(||escape(&title));
        out += &format!(
            "<tr><td>{content}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            escape(&number(&row["miners"])),
            escape(&rate(finite(&row["hashrate"]).unwrap_or(0.0)))
        );
    }
    out += "</tbody>";
    html(id, &out);
}
fn breakdowns(s: &State) {
    let data = s.summary.as_ref().unwrap_or(&Value::Null);
    let models=data["models"].as_array().cloned().unwrap_or_else(||data["boards"].as_array().into_iter().flatten().map(|b|json!({"board":b["name"],"model":null,"miners":b["miners"],"hashrate":b["hashrate"]})).collect());
    let mut rows: HashMap<String, Value> = HashMap::new();
    for model in models {
        let name = model["model"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| short(model["board"].as_str().unwrap_or_default()));
        let key = name
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>();
        let row = rows
            .entry(key)
            .or_insert_with(|| json!({"name":name,"miners":0,"hashrate":0.0}));
        row["miners"] = json!(whole(&row["miners"]) + whole(&model["miners"]));
        row["hashrate"] = json!(
            finite(&row["hashrate"]).unwrap_or(0.0) + finite(&model["hashrate"]).unwrap_or(0.0)
        );
    }
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_by(|a, b| {
        finite(&b["hashrate"])
            .unwrap_or(0.0)
            .total_cmp(&finite(&a["hashrate"]).unwrap_or(0.0))
    });
    table("st-boards", &rows, "colBoard", str::to_string, |_| None);
    for (id, key, header) in [
        ("st-versions", "versions", "colVersion"),
        ("st-coins", "coins", "colCoin"),
    ] {
        table(
            id,
            data[key].as_array().map(Vec::as_slice).unwrap_or(&[]),
            header,
            str::to_string,
            |_| None,
        );
    }
    let pools = data["pools"].as_array().cloned().unwrap_or_else(|| {
        if whole(&data["miners"]) > 0 {
            vec![json!({"name":"unknown","miners":data["miners"],"hashrate":data["hashrate"]})]
        } else {
            vec![]
        }
    });
    let content = ui::data();
    table(
        "st-pools",
        &pools,
        "colPool",
        |key| {
            if key == "unknown" {
                t("poolUnknown")
            } else {
                content["pool_names"][key]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| t("poolOther"))
            }
        },
        |key| {
            if content["pool_names"].get(key).is_some() {
                content["pool_links"][key].as_str().map(str::to_string)
            } else {
                None
            }
        },
    );
}
fn market(s: &State) {
    if get("home-market").is_none() {
        return;
    }
    let data = s.summary.as_ref().unwrap_or(&Value::Null);
    text(
        "hm-total",
        &if s.summary.is_some() {
            rate(finite(&data["hashrate"]).unwrap_or(0.0))
        } else {
            "–".into()
        },
    );
    text(
        "hm-miners",
        &if s.summary.is_some() {
            number(&data["miners"])
        } else {
            "–".into()
        },
    );
    let total = finite(&data["miners"]).unwrap_or(0.0);
    let mut rows: Vec<_> = data["boards"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| whole(&b["miners"]) > 0)
        .collect();
    rows.sort_by(|a, b| {
        finite(&b["hashrate"])
            .unwrap_or(0.0)
            .total_cmp(&finite(&a["hashrate"]).unwrap_or(0.0))
    });
    let known: HashSet<_> = ui::boards()
        .into_iter()
        .map(|b| string(&b["env"]))
        .collect();
    let pct = |n: f64| {
        format(
            n * 100.0,
            json!({"maximumFractionDigits":if n<0.1{2}else{1}}),
        ) + " %"
    };
    let icon = |env: &str| {
        let label = short(env);
        let initials = label
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(2)
            .collect::<String>()
            .to_uppercase();
        format!(
            "<span class=\"chip-icon\" data-family=\"{}\" aria-hidden=\"true\">{}</span>",
            family(env),
            escape(&initials)
        )
    };
    let mut cards = String::new();
    for row in rows
        .iter()
        .filter(|r| s.cards == "all" || family(r["name"].as_str().unwrap_or_default()) == s.cards)
        .take(3)
    {
        let env = string(&row["name"]);
        let href = if known.contains(&env) {
            format!("flash.html?board={}", js_sys::encode_uri_component(&env))
        } else {
            "stats.html".into()
        };
        let miners = whole(&row["miners"]);
        cards+=&format!("<a class=\"ticker\" href=\"{}\"><span class=\"ticker-head\">{}<span class=\"ticker-name\">{}</span><span class=\"ticker-tag\">{}</span></span><strong class=\"ticker-value\">{}</strong><span class=\"ticker-foot\"><span>{}</span><span class=\"chg\">{} {}</span></span></a>",escape(&href),icon(&env),escape(&short(&env)),escape(&family_label(family(&env))),escape(&rate(finite(&row["hashrate"]).unwrap_or(0.0))),escape(&t(if miners==1{"cardMiner"}else{"cardMiners"}).replace("{n}",&number(&row["miners"]))),escape(&pct(if total>0.0{miners as f64/total}else{0.0})),if ui::language()=="fr"{"des mineurs"}else{"of miners"});
    }
    if cards.is_empty() {
        cards = format!(
            "<p class=\"ticker-empty\">{}</p>",
            escape(&if s.summary.is_none() {
                "–".into()
            } else if rows.is_empty() {
                t("noneNow")
            } else {
                t("noMatch")
            })
        );
    }
    html("hm-cards", &cards);
    let query = s.query.trim().to_lowercase();
    let mut body = String::new();
    for (index, row) in rows
        .iter()
        .filter(|r| {
            let env = r["name"].as_str().unwrap_or_default();
            (s.table == "all" || family(env) == s.table)
                && (query.is_empty()
                    || short(env).to_lowercase().contains(&query)
                    || env.contains(&query))
        })
        .enumerate()
    {
        let env = string(&row["name"]);
        let share = if total > 0.0 {
            whole(&row["miners"]) as f64 / total
        } else {
            0.0
        };
        body+=&format!("<tr><td class=\"rank\">{}</td><td><span class=\"board\">{}<span>{}</span><small>{}</small></span></td><td class=\"num strong\">{}</td><td class=\"num col-miners\">{}</td><td class=\"col-share\"><div class=\"share\"><span class=\"bar\"><i style=\"width:{:.1}%\"></i></span><span>{}</span></div></td><td class=\"act\">{}</td></tr>",index+1,icon(&env),escape(&short(&env)),escape(&family_label(family(&env))),escape(&rate(finite(&row["hashrate"]).unwrap_or(0.0))),escape(&number(&row["miners"])),(share*100.0).max(2.0),escape(&pct(share)),if known.contains(&env){format!("<a class=\"btn-sm\" href=\"flash.html?board={}\">{}</a>",escape(&String::from(js_sys::encode_uri_component(&env))),escape(&t("navInstall")))}else{String::new()});
    }
    if body.is_empty() {
        body = format!(
            "<tr><td class=\"empty\" colspan=\"6\">{}</td></tr>",
            escape(&if s.summary.is_none() {
                "–".into()
            } else if !rows.is_empty() && (!query.is_empty() || s.table != "all") {
                t("noMatch")
            } else {
                t("noneNow")
            })
        );
    }
    if let Some(el) = crate::dom::query("#home-market tbody") {
        el.set_inner_html(&body);
    }
}
fn points(data: &Value) -> Vec<(f64, Option<f64>)> {
    let at = data["at"].as_f64().unwrap_or(0.0);
    let mut points: Vec<_> = data["recent"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let time = p["at"].as_f64()?;
            let value = finite(&p["hashrate"])?;
            (time > at - 86400000.0 && time <= at).then_some((
                time,
                if p["missing"] == true {
                    None
                } else {
                    Some(value)
                },
            ))
        })
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    points
}
fn history(s: &State) {
    if get("cryptoChart").is_none() {
        return;
    }
    let data = s.summary.as_ref().unwrap_or(&Value::Null);
    let points = points(data);
    text(
        "hero-network-rate",
        &if let Some(rate) = finite(&data["hashrate"]) {
            ui::rate(rate)
        } else {
            "—".into()
        },
    );
    let valid: Vec<_> = points.iter().filter_map(|(_, v)| *v).collect();
    let mut path = String::new();
    if valid.len() > 1 {
        let first = points[0].0;
        let span = (points.last().unwrap().0 - first).max(1.0);
        let peak = valid.iter().copied().fold(1.0_f64, f64::max);
        let mut previous = None;
        for (time, value) in &points {
            let Some(value) = value else {
                previous = None;
                continue;
            };
            path += &format!(
                "{}{:.2} {:.2} ",
                if previous.map(|t| *time - t <= 750000.0).unwrap_or(false) {
                    "L"
                } else {
                    "M"
                },
                (*time - first) / span * 260.0,
                100.0 - *value / peak * 88.0
            );
            previous = Some(*time);
        }
    }
    html("hero-network-spark",&format!("<path class=\"hero-spark-grid\" d=\"M0 25H260M0 60H260M0 95H260\"/><path class=\"hero-spark-under\" d=\"{path}\"/><path class=\"hero-spark-line\" d=\"{path}\"/>"));
    let drawn = chart::render("cryptoChart", &points, 750000.0);
    hidden("home-history-empty", drawn);
    text(
        "home-history-empty",
        if valid.len() >= 2 {
            tr("Graphique indisponible.", "Chart unavailable.")
        } else {
            tr(
                "En attente des mesures de hashrate.",
                "Waiting for hashrate measurements.",
            )
        },
    );
    text(
        "home-history-caption",
        &if valid.is_empty() {
            String::new()
        } else {
            format!(
                "{}{}{}",
                tr("Dernières 24 heures · ", "Last 24 hours · "),
                valid.len(),
                tr(" mesures · heure de Paris", " measurements · Paris time")
            )
        },
    );
}
fn render(s: &State) {
    market(s);
    connection(s);
    history(s);
    if get("stats").is_some() {
        fleet(s);
        breakdowns(s);
        if let Some(data) = &s.summary {
            text(
                "st-hashrate",
                &rate(finite(&data["hashrate"]).unwrap_or(0.0)),
            );
            text("st-miners", &number(&data["miners"]));
            text(
                "st-average",
                &if whole(&data["miners"]) > 0 {
                    rate(finite(&data["hashrate"]).unwrap_or(0.0) / whole(&data["miners"]) as f64)
                } else {
                    "–".into()
                },
            );
            text(
                "st-record",
                &finite(&data["record"]["hashrate"])
                    .map(rate)
                    .unwrap_or_else(|| "–".into()),
            );
        }
    }
    expose(s);
}
async fn refresh() {
    let mut proceed = false;
    with(|s| {
        if s.loading || !active() {
            return;
        }
        window().clear_timeout_with_handle(s.retry);
        s.retry = 0;
        s.loading = true;
        expose(s);
        proceed = true;
    });
    if !proceed {
        return;
    }
    let cache = STATE.with(|s| {
        if s.borrow().failures > 0 {
            web_sys::RequestCache::Reload
        } else {
            web_sys::RequestCache::Default
        }
    });
    let result = fetch_cached(
        &endpoint(),
        &web_sys::AbortSignal::timeout_with_u32(10000),
        cache,
    )
    .await;
    with(|s| {
        match result {
            Ok(data) if valid(&data) => {
                save(&data);
                s.summary = Some(data);
                s.failed = false;
                s.failures = 0;
                s.fetched = now();
                render(s);
            }
            _ => {
                s.failed = true;
                s.failures += 1;
                connection(s);
                let delay = (2000_i32 * 2_i32.pow((s.failures - 1).min(4))).min(20000);
                s.retry = later(delay, || {
                    if !document().hidden() {
                        run(async {
                            refresh().await;
                        });
                    }
                });
            }
        }
        s.loading = false;
        expose(s);
    });
}
fn inspect(index: usize) {
    with(|s| {
        let Some(data) = &s.summary else {
            return;
        };
        let points = points(data);
        let valid: Vec<_> = points.iter().filter(|p| p.1.is_some()).collect();
        if valid.is_empty() {
            return;
        }
        let index = index.min(valid.len() - 1);
        s.inspect = Some(index);
        let (time, value) = valid[index];
        let value = value.unwrap();
        let stamp = js_sys::Date::new(&(*time).into())
            .to_iso_string()
            .as_string()
            .unwrap_or_default();
        text(
            "home-history-inspect",
            &(date(
                &json!(stamp),
                json!({"day":"2-digit","month":"short","hour":"2-digit","minute":"2-digit"}),
            ) + " · "
                + &rate(value)),
        );
    });
}
pub fn init() {
    with(|s| {
        s.summary = if active() { cached() } else { None };
        s.cards = "all".into();
        s.table = "all".into();
        expose(s);
    });
    let refresh_fn = Closure::wrap(Box::new(|| {
        future_to_promise(async {
            refresh().await;
            Ok(wasm_bindgen::JsValue::UNDEFINED)
        })
    }) as Box<dyn Fn() -> js_sys::Promise>);
    set(&js_sys::global(), "refresh", refresh_fn.as_ref());
    refresh_fn.forget();
    click("st-more", || {
        with(|s| {
            s.all = !s.all;
            fleet(s);
        })
    });
    for button in all(".family-tabs button[data-family]") {
        let family = button.get_attribute("data-family").unwrap_or_default();
        let group = button.closest(".family-tabs").ok().flatten();
        let target = group
            .as_ref()
            .and_then(|g| g.get_attribute("data-target"))
            .unwrap_or_default();
        on(button.as_ref(), "click", move |_| {
            with(|s| {
                if target == "cards" {
                    s.cards = family.clone();
                } else {
                    s.table = family.clone();
                }
                market(s);
            });
            if let Some(group) = &group {
                if let Ok(buttons) = group.query_selector_all("button") {
                    for i in 0..buttons.length() {
                        if let Some(el) = buttons
                            .item(i)
                            .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
                        {
                            let _ = el.set_attribute(
                                "aria-pressed",
                                if el.get_attribute("data-family").as_deref() == Some(&family) {
                                    "true"
                                } else {
                                    "false"
                                },
                            );
                        }
                    }
                }
            }
        });
    }
    on_id("hm-search", "input", |_| {
        with(|s| {
            s.query = value("hm-search");
            market(s);
        })
    });
    on(document().as_ref(), "bdl-language-change", |_| {
        with(|s| render(s))
    });
    on(document().as_ref(), "bdl-theme-change", |_| {
        with(|s| history(s))
    });
    on(window().as_ref(), "resize", |_| with(|s| history(s)));
    interval(1000, || with(|s| connection(s)));
    interval(20000, || {
        if !document().hidden() {
            run(async {
                refresh().await;
            });
        }
    });
    on(document().as_ref(), "visibilitychange", |_| {
        if !document().hidden() {
            run(async {
                refresh().await;
            });
        }
    });
    on_id("cryptoChart", "keydown", |event| {
        let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>().map(|e| e.key()) else {
            return;
        };
        if !["ArrowLeft", "ArrowRight", "Home", "End"].contains(&key.as_str()) {
            return;
        }
        event.prevent_default();
        let mut index = 0;
        with(|s| {
            let count = s
                .summary
                .as_ref()
                .map(|d| points(d).iter().filter(|p| p.1.is_some()).count())
                .unwrap_or(0);
            index = match key.as_str() {
                "Home" => 0,
                "End" => count.saturating_sub(1),
                "ArrowLeft" => s
                    .inspect
                    .map(|v| v.saturating_sub(1))
                    .unwrap_or(count.saturating_sub(1)),
                _ => s
                    .inspect
                    .map(|v| (v + 1).min(count.saturating_sub(1)))
                    .unwrap_or(0),
            };
        });
        inspect(index);
    });
    on_id("cryptoChart", "blur", |_| {
        with(|s| s.inspect = None);
        text("home-history-inspect", "");
    });
    on_id("cryptoChart", "mousemove", |event| {
        if let (Some(mouse), Some(canvas)) =
            (event.dyn_ref::<web_sys::MouseEvent>(), get("cryptoChart"))
        {
            let rect = canvas.get_bounding_client_rect();
            let ratio = ((f64::from(mouse.client_x()) - rect.left() - 58.0)
                / (rect.width() - 70.0).max(1.0))
            .clamp(0.0, 1.0);
            let mut count = 0;
            with(|s| {
                count = s
                    .summary
                    .as_ref()
                    .map(|d| points(d).iter().filter(|p| p.1.is_some()).count())
                    .unwrap_or(0)
            });
            inspect((ratio * count.saturating_sub(1) as f64).round() as usize);
        }
    });
    if active() {
        let seed = future_to_promise(async {
            if endpoint() == API {
                if let Ok(data) = fetch(SNAPSHOT, 5000).await {
                    with(|s| {
                        let age = now() - data["at"].as_f64().unwrap_or(0.0);
                        if s.fetched == 0.0
                            && s.summary.is_none()
                            && valid(&data)
                            && (-60000.0..=1800000.0).contains(&age)
                        {
                            save(&data);
                            s.summary = Some(data);
                            render(s);
                        }
                    });
                }
            }
            Ok(wasm_bindgen::JsValue::UNDEFINED)
        });
        set(&js_sys::global(), "BDL_PUBLIC_STATS_SEED", &seed);
        run(async {
            refresh().await;
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_summary_rows_and_numbers() {
        let mut data = json!({"at":1,"hashrate":0,"miners":0,"recent":[]});
        assert!(valid(&data));
        data["miners"] = json!(1.5);
        assert!(!valid(&data));
        data["miners"] = 0.into();
        data["boards"] = json!([null]);
        assert!(!valid(&data));
    }
}
