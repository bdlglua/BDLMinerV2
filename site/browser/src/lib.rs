mod chart;
mod chat;
mod demo;
mod discovery;
mod dom;
mod flash;
mod navigation;
mod remote;
mod stats;
mod ui;

use wasm_bindgen::prelude::*;
#[wasm_bindgen(start)]
pub fn start() {
    if demo::is_demo() {
        demo::init();
        ready();
        return;
    }
    ui::init();
    navigation::floating();
    discovery::init();
    remote::init();
    chat::init();
    flash::init();
    stats::init();
    ui::apply();
    ready();
}
fn ready() {
    if let Some(body) = dom::document().body() {
        let _ = body.set_attribute("data-runtime", "ready");
    }
}
