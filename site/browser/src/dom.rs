//! Browser bindings only. Application decisions and event handlers live in Rust.
use js_sys::{Array, Function, Object, Reflect};
use serde_json::Value;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{Element, Event, EventTarget, HtmlElement};

pub fn window() -> web_sys::Window {
    web_sys::window().expect("browser window")
}
pub fn document() -> web_sys::Document {
    window().document().expect("document")
}
pub fn get(id: &str) -> Option<Element> {
    document().get_element_by_id(id)
}
pub fn query(selector: &str) -> Option<Element> {
    document().query_selector(selector).ok().flatten()
}
pub fn all(selector: &str) -> Vec<Element> {
    document()
        .query_selector_all(selector)
        .ok()
        .map(|nodes| {
            (0..nodes.length())
                .filter_map(|i| nodes.item(i)?.dyn_into().ok())
                .collect()
        })
        .unwrap_or_default()
}
pub fn text(id: &str, value: &str) {
    if let Some(el) = get(id) {
        el.set_text_content(Some(value));
    }
}
pub fn html(id: &str, value: &str) {
    if let Some(el) = get(id) {
        el.set_inner_html(value);
        prune_listeners();
    }
}
pub fn attr(id: &str, name: &str, value: &str) {
    if let Some(el) = get(id) {
        let _ = el.set_attribute(name, value);
    }
}
pub fn remove_attr(id: &str, name: &str) {
    if let Some(el) = get(id) {
        let _ = el.remove_attribute(name);
    }
}
pub fn hidden(id: &str, value: bool) {
    if let Some(el) = get(id) {
        let _ = el.toggle_attribute_with_force("hidden", value);
    }
}
pub fn disabled(id: &str, value: bool) {
    if let Some(el) = get(id) {
        let _ = el.toggle_attribute_with_force("disabled", value);
    }
}
pub fn class(el: &Element, name: &str, enabled: bool) {
    let _ = el.class_list().toggle_with_force(name, enabled);
}
pub fn value(id: &str) -> String {
    get(id)
        .map(|el| property(&el, "value").as_string().unwrap_or_default())
        .unwrap_or_default()
}
pub fn set_value(id: &str, val: &str) {
    if let Some(el) = get(id) {
        set(&el, "value", &JsValue::from_str(val));
    }
}
pub fn checked(id: &str) -> bool {
    get(id)
        .map(|el| property(&el, "checked").as_bool().unwrap_or(false))
        .unwrap_or(false)
}
pub fn focus(id: &str) {
    if let Some(el) = get(id).and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
        let options = Object::new();
        set(&options, "preventScroll", &true.into());
        let _ = call(&el, "focus", &[options.into()]);
    }
}
struct Listener {
    target: EventTarget,
    event: String,
    callback: Closure<dyn FnMut(Event)>,
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.target.remove_event_listener_with_callback(
            &self.event,
            self.callback.as_ref().unchecked_ref(),
        );
    }
}
thread_local! {static LISTENERS:std::cell::RefCell<Vec<Listener>>=const{std::cell::RefCell::new(Vec::new())};}
pub fn prune_listeners() {
    LISTENERS.with(|listeners| {
        listeners.borrow_mut().retain(|listener| {
            listener
                .target
                .dyn_ref::<web_sys::Node>()
                .map(|n| n.is_connected())
                .unwrap_or(true)
        })
    });
}
pub fn on(target: &EventTarget, event: &str, handler: impl FnMut(Event) + 'static) {
    let callback = Closure::wrap(Box::new(handler) as Box<dyn FnMut(Event)>);
    if target
        .add_event_listener_with_callback(event, callback.as_ref().unchecked_ref())
        .is_ok()
    {
        LISTENERS.with(|listeners| {
            listeners.borrow_mut().push(Listener {
                target: target.clone(),
                event: event.into(),
                callback,
            })
        });
    }
}

pub fn on_id(id: &str, event: &str, handler: impl FnMut(Event) + 'static) {
    if let Some(el) = get(id) {
        on(el.as_ref(), event, handler);
    }
}
pub fn click(id: &str, mut handler: impl FnMut() + 'static) {
    on_id(id, "click", move |_| handler());
}
pub fn dispatch(name: &str) {
    if let Ok(event) = Event::new(name) {
        let _ = document().dispatch_event(&event);
    }
}
pub fn later(ms: i32, handler: impl FnOnce() + 'static) -> i32 {
    let cb = Closure::once_into_js(handler);
    window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), ms)
        .unwrap_or(0)
}
pub fn interval(ms: i32, handler: impl FnMut() + 'static) {
    let cb = Closure::wrap(Box::new(handler) as Box<dyn FnMut()>);
    let _ = window()
        .set_interval_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), ms);
    cb.forget();
}
pub fn frame(handler: impl FnOnce() + 'static) {
    let cb = Closure::once_into_js(handler);
    let _ = window().request_animation_frame(cb.unchecked_ref());
}
pub fn run(future: impl std::future::Future<Output = ()> + 'static) {
    spawn_local(future);
}
pub fn property(obj: &JsValue, key: &str) -> JsValue {
    Reflect::get(obj, &key.into()).unwrap_or(JsValue::UNDEFINED)
}
pub fn set(obj: &JsValue, key: &str, val: &JsValue) {
    let _ = Reflect::set(obj, &key.into(), val);
}
pub fn call(obj: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let f = property(obj, method).dyn_into::<Function>()?;
    let args: Array = args.iter().cloned().collect();
    f.apply(obj, &args)
}
pub fn json(value: &JsValue) -> Value {
    js_sys::JSON::stringify(value)
        .ok()
        .and_then(|s| serde_json::from_str(&String::from(s)).ok())
        .unwrap_or(Value::Null)
}
pub fn js(value: &Value) -> JsValue {
    js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL)
}
pub fn read_storage(key: &str) -> Option<String> {
    window()
        .local_storage()
        .ok()
        .flatten()?
        .get_item(key)
        .ok()
        .flatten()
}
pub fn store(key: &str, value: &str) {
    if let Ok(Some(s)) = window().local_storage() {
        let _ = s.set_item(key, value);
    }
}
pub fn forget(key: &str) {
    if let Ok(Some(s)) = window().local_storage() {
        let _ = s.remove_item(key);
    }
}
pub fn now() -> f64 {
    js_sys::Date::now()
}
pub fn stamp(value: &Value) -> f64 {
    value.as_str().map(js_sys::Date::parse).unwrap_or(f64::NAN)
}
pub fn lang() -> bool {
    document()
        .document_element()
        .map(|e| e.get_attribute("lang").as_deref() == Some("en"))
        .unwrap_or(false)
}
pub fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub fn tr<'a>(fr: &'a str, en: &'a str) -> &'a str {
    if lang() {
        en
    } else {
        fr
    }
}
pub fn string(value: &Value) -> String {
    match value {
        Value::String(v) => v.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}
pub fn finite(value: &Value) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite() && *v >= 0.0)
}
pub fn format(value: f64, options: Value) -> String {
    let intl = property(&js_sys::global(), "Intl");
    let ctor = property(&intl, "NumberFormat");
    let args = Array::of2(&tr("fr-FR", "en-GB").into(), &js(&options));
    Reflect::construct(&ctor.unchecked_into(), &args)
        .and_then(|obj| call(&obj, "format", &[value.into()]))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| value.to_string())
}
pub fn number(value: &Value) -> String {
    finite(value)
        .map(|v| format(v, serde_json::json!({"maximumFractionDigits":2})))
        .unwrap_or_else(|| "—".into())
}
pub fn hash(value: &Value) -> String {
    finite(value)
        .map(|mut v| {
            let units = ["H/s", "kH/s", "MH/s", "GH/s", "TH/s", "PH/s", "EH/s"];
            let mut i = 0;
            while v >= 1000.0 && i < 6 {
                v /= 1000.0;
                i += 1;
            }
            format!(
                "{} {}",
                format(v, serde_json::json!({"maximumFractionDigits":2})),
                units[i]
            )
        })
        .unwrap_or_else(|| "—".into())
}
pub fn date(value: &Value, options: Value) -> String {
    if !stamp(value).is_finite() {
        return "—".into();
    }
    let intl = property(&js_sys::global(), "Intl");
    let ctor = property(&intl, "DateTimeFormat");
    let mut options = options;
    options["timeZone"] = "Europe/Paris".into();
    Reflect::construct(
        &ctor.unchecked_into(),
        &Array::of2(&tr("fr-FR", "en-GB").into(), &js(&options)),
    )
    .and_then(|obj| {
        call(
            &obj,
            "format",
            &[js_sys::Date::new(&string(value).into()).into()],
        )
    })
    .ok()
    .and_then(|v| v.as_string())
    .unwrap_or_else(|| "—".into())
}
pub fn current_url() -> web_sys::Url {
    web_sys::Url::new(&window().location().href().unwrap_or_default()).expect("page URL")
}
pub async fn fetch(url: &str, timeout_ms: u32) -> Result<Value, String> {
    let signal = web_sys::AbortSignal::timeout_with_u32(timeout_ms);
    fetch_signal(url, &signal).await
}
pub async fn fetch_signal(url: &str, signal: &web_sys::AbortSignal) -> Result<Value, String> {
    fetch_cached(url, signal, web_sys::RequestCache::NoStore).await
}
pub async fn fetch_cached(
    url: &str,
    signal: &web_sys::AbortSignal,
    cache: web_sys::RequestCache,
) -> Result<Value, String> {
    let dest = web_sys::Url::new_with_base(
        url,
        &document().base_uri().ok().flatten().unwrap_or_default(),
    )
    .map_err(|_| "Invalid URL")?;
    let loopback_preview = current_url().search_params().get("statsApi").as_deref() == Some("go")
        && ["localhost", "127.0.0.1", "[::1]"].contains(&current_url().hostname().as_str())
        && dest.href() == "http://127.0.0.1:8787/api/summary";
    if !loopback_preview
        && dest.protocol() != "https:"
        && dest.origin() != window().location().origin().unwrap_or_default()
    {
        return Err("HTTPS required".into());
    }
    let options = web_sys::RequestInit::new();
    options.set_credentials(web_sys::RequestCredentials::Omit);
    options.set_cache(cache);
    // Native AbortSignal.timeout also supports the browser's active-time semantics.
    options.set_signal(Some(signal));
    let response = JsFuture::from(window().fetch_with_str_and_init(&dest.href(), &options))
        .await
        .map_err(|_| "Network unavailable")?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| "Invalid response")?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    let data = JsFuture::from(response.json().map_err(|_| "Invalid JSON")?)
        .await
        .map_err(|_| "Invalid JSON")?;
    Ok(json(&data))
}
pub async fn copy(value: &str) -> bool {
    JsFuture::from(window().navigator().clipboard().write_text(value))
        .await
        .is_ok()
}
pub fn download(content: &str, name: &str, mime: &str) {
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let parts = Array::of1(&content.into());
    if let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options) {
        if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
            if let Ok(anchor) = document().create_element("a") {
                let _ = anchor.set_attribute("href", &url);
                let _ = anchor.set_attribute("download", name);
                let _ = call(&anchor, "click", &[]);
            }
            later(1000, move || {
                let _ = web_sys::Url::revoke_object_url(&url);
            });
        }
    }
}
