use crate::dom::*;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::JsCast;
use web_sys::{Element, KeyboardEvent};
#[derive(Default)]
struct Scroll {
    previous: f64,
    direction: f64,
    distance: f64,
    pending: bool,
    focus_generation: u64,
}
fn position() -> f64 {
    let y = window().scroll_y().unwrap_or_default();
    let max = document()
        .document_element()
        .map(|e| f64::from(e.scroll_height()))
        .unwrap_or_default()
        - window()
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or_default();
    y.min(max.max(0.0)).max(0.0)
}
fn reveal(header: &Element, state: &Rc<RefCell<Scroll>>) {
    class(header, "nav-hidden", false);
    let mut s = state.borrow_mut();
    s.previous = position();
    s.direction = 0.0;
    s.distance = 0.0;
}
fn finish_focus(state: &Rc<RefCell<Scroll>>) {
    state.borrow_mut().focus_generation += 1;
    if let Some(root) = document().document_element() {
        class(&root, "nav-focusing", false);
    }
}
fn reveal_for_focus(header: &Element, state: &Rc<RefCell<Scroll>>) {
    if let Some(root) = document().document_element() {
        class(&root, "nav-focusing", true);
    }
    class(header, "nav-focus-reveal", true);
    reveal(header, state);
    header.get_bounding_client_rect();
    class(header, "nav-focus-reveal", false);
    let generation = {
        let mut state = state.borrow_mut();
        state.focus_generation += 1;
        state.focus_generation
    };
    let state = state.clone();
    frame(move || {
        if state.borrow().focus_generation == generation {
            if let Some(root) = document().document_element() {
                class(&root, "nav-focusing", false);
            }
        }
    });
}
pub fn floating() {
    let Some(header) = query(".site-header, #site-shell > nav, body > nav") else {
        return;
    };
    let state = Rc::new(RefCell::new(Scroll {
        previous: position(),
        ..Default::default()
    }));
    let h = header.clone();
    let s = state.clone();
    on(window().as_ref(), "scroll", move |_| {
        if s.borrow().pending {
            return;
        }
        s.borrow_mut().pending = true;
        let h = h.clone();
        let s = s.clone();
        frame(move || {
            let y = position();
            let delta = {
                let mut s = s.borrow_mut();
                s.pending = false;
                let d = y - s.previous;
                s.previous = y;
                d
            };
            let interacting = h
                .query_selector_all("[aria-expanded=true], details[open], :focus-visible")
                .ok()
                .map(|nodes| {
                    (0..nodes.length())
                        .filter_map(|i| nodes.item(i))
                        .filter_map(|n| n.dyn_into::<Element>().ok())
                        .any(|e| e.get_bounding_client_rect().width() > 0.0)
                })
                .unwrap_or(false);
            if y < 120.0 || interacting {
                reveal(&h, &s);
                return;
            }
            if delta == 0.0 {
                return;
            }
            let mut s = s.borrow_mut();
            let direction = delta.signum();
            if direction != s.direction {
                s.direction = direction;
                s.distance = 0.0;
            }
            s.distance += delta.abs();
            if s.distance >= if direction > 0.0 { 16.0 } else { 8.0 } {
                class(&h, "nav-hidden", direction > 0.0);
            }
        });
    });
    let h = header.clone();
    let s = state.clone();
    on(header.as_ref(), "focusin", move |_| {
        reveal_for_focus(&h, &s)
    });
    let h = header.clone();
    let s = state.clone();
    on(header.as_ref(), "toggle", move |_| reveal(&h, &s));
    let h = header.clone();
    let s = state.clone();
    on(document().as_ref(), "keydown", move |event| {
        if event
            .dyn_ref::<KeyboardEvent>()
            .map(|e| e.key() == "Tab")
            .unwrap_or(false)
        {
            reveal_for_focus(&h, &s);
        } else {
            finish_focus(&s);
        }
    });
    let s = state.clone();
    on(header.as_ref(), "click", move |_| finish_focus(&s));
    for name in ["resize", "pageshow"] {
        let h = header.clone();
        let s = state.clone();
        on(window().as_ref(), name, move |_| {
            finish_focus(&s);
            if let Some(toggle) = h.query_selector(".menu-toggle, #menu").ok().flatten() {
                if toggle.get_attribute("aria-expanded").as_deref() == Some("true")
                    && toggle.get_bounding_client_rect().width() == 0.0
                {
                    let _ = call(&toggle, "click", &[]);
                }
            }
            reveal(&h, &s);
        });
    }
}
