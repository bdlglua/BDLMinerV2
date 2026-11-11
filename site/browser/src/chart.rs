//! Canvas renderer implemented in Rust; no JavaScript charting library.
use crate::dom::*;
use serde_json::{json, Value};
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

pub fn segments(points: &[(f64, Option<f64>)], gap_ms: f64) -> Vec<Vec<(f64, f64)>> {
    let mut groups: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut previous = None;
    for (time, value) in points {
        let Some(value) = value else {
            previous = None;
            continue;
        };
        let joined = previous.map(|p| *time - p <= gap_ms).unwrap_or(false);
        if !joined {
            groups.push(Vec::new());
        }
        groups.last_mut().unwrap().push((*time, *value));
        previous = Some(*time);
    }
    groups
}
pub fn render(id: &str, points: &[(f64, Option<f64>)], gap_ms: f64) -> bool {
    let Some(canvas) = get(id).and_then(|e| e.dyn_into::<HtmlCanvasElement>().ok()) else {
        return false;
    };
    let groups = segments(points, gap_ms);
    let _ = canvas.set_attribute(
        "data-readings",
        &serde_json::to_string(points).unwrap_or_default(),
    );
    let _ = canvas.set_attribute("data-segments", &groups.len().to_string());
    let valid = points.iter().filter(|(_, v)| v.is_some()).count();
    if valid < 2 {
        hidden(id, true);
        return false;
    }
    hidden(id, false);
    let width = canvas
        .parent_element()
        .map(|p| p.get_bounding_client_rect().width())
        .unwrap_or(640.0)
        .max(240.0);
    let height = canvas
        .parent_element()
        .map(|p| p.get_bounding_client_rect().height())
        .unwrap_or(200.0)
        .clamp(160.0, 350.0);
    let scale = window().device_pixel_ratio().clamp(1.0, 3.0);
    canvas.set_width((width * scale) as u32);
    canvas.set_height((height * scale) as u32);
    let _ = canvas.style().set_property("width", &format!("{width}px"));
    let _ = canvas
        .style()
        .set_property("height", &format!("{height}px"));
    let Some(ctx) = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok())
    else {
        return false;
    };
    let _ = ctx.scale(scale, scale);
    let dark = document()
        .document_element()
        .and_then(|r| r.get_attribute("data-theme"))
        .as_deref()
        != Some("light");
    let foreground = if dark { "#999999" } else { "#666666" };
    let grid = if dark { "#303030" } else { "#dddddd" };
    let peak = points
        .iter()
        .filter_map(|(_, v)| *v)
        .fold(1.0_f64, f64::max);
    let first = points.first().map(|p| p.0).unwrap_or(0.0);
    let end = points.last().map(|p| p.0).unwrap_or(first + 1.0);
    let plot_width = width - 70.0;
    let plot_height = height - 34.0;
    ctx.set_font("11px 'IBM Plex Mono',monospace");
    ctx.set_line_width(1.0);
    for i in 0..=4 {
        let y = 10.0 + f64::from(i) * plot_height / 4.0;
        ctx.set_stroke_style_str(grid);
        ctx.begin_path();
        ctx.move_to(58.0, y);
        ctx.line_to(width - 12.0, y);
        ctx.stroke();
        ctx.set_fill_style_str(foreground);
        let _ = ctx.fill_text(
            &hash(&json!(peak * (1.0 - f64::from(i) / 4.0))),
            0.0,
            y + 3.0,
        );
    }
    ctx.set_stroke_style_str("#f7931a");
    ctx.set_line_width(2.0);
    ctx.begin_path();
    for group in &groups {
        for (index, (time, value)) in group.iter().enumerate() {
            let x = 58.0 + (*time - first) / (end - first).max(1.0) * plot_width;
            let y = 10.0 + plot_height - *value / peak * plot_height;
            if index > 0 {
                ctx.line_to(x, y);
            } else {
                ctx.move_to(x, y);
            }
        }
    }

    ctx.stroke();
    ctx.set_fill_style_str(foreground);
    for (time, x) in [(first, 58.0), (end, width - 54.0)] {
        let date = js_sys::Date::new(&time.into())
            .to_iso_string()
            .as_string()
            .unwrap_or_default();
        let label = crate::dom::date(
            &Value::String(date),
            json!({"hour":"2-digit","minute":"2-digit"}),
        );
        let _ = ctx.fill_text(&label, x, height - 3.0);
    }
    // The exact source series is also available as an accessible text alternative.
    let _ = canvas.set_attribute(
        "aria-label",
        &format!(
            "Hashrate: {}",
            hash(&json!(points.last().and_then(|p| p.1)))
        ),
    );
    let _ = canvas.set_attribute(
        "data-readings",
        &serde_json::to_string(points).unwrap_or_default(),
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_reports_and_long_intervals_split_lines() {
        assert_eq!(
            segments(
                &[
                    (0.0, Some(1.0)),
                    (300000.0, Some(2.0)),
                    (1500000.0, Some(3.0))
                ],
                750000.0
            )
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(
            segments(
                &[(0.0, Some(1.0)), (300000.0, None), (600000.0, Some(3.0))],
                750000.0
            )
            .len(),
            2
        );
    }
}
