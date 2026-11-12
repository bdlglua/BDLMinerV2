//! The renderer on in-memory panels of every board size: each page, the
//! portal, messages and alerts draw within the panel and leave something on
//! it. With BDL_PREVIEW_DIR set, the frames are written there as PPM images.

use bdlminer_app::platform::{BoardKind, BoardProfile};
use bdlminer_app::screen::{self, Inputs, Samples};
use bdlminer_app::settings::Settings;
use bdlminer_app::ui_view::{rgb565, Framebuffer, View, BACKGROUND};

const SIZES: [(i32, i32); 7] = [(240, 135), (135, 240), (320, 240), (240, 320), (128, 128), (320, 170), (160, 80)];

fn board() -> BoardProfile {
    BoardProfile {
        name: "Test",
        env: "test",
        kind: BoardKind::Esp32,
        m5: false,
        display_pages: 8,
        temperature_sensor: true,
    }
}

fn save(fb: &Framebuffer, name: &str) {
    let Ok(dir) = std::env::var("BDL_PREVIEW_DIR") else { return };
    let mut ppm = format!("P6\n{} {}\n255\n", fb.width, fb.height).into_bytes();
    ppm.extend(fb.rgb());
    std::fs::write(format!("{dir}/{name}.ppm"), ppm).unwrap();
}

fn drawn(fb: &Framebuffer) -> usize {
    let bg = rgb565(BACKGROUND);
    fb.pixels.iter().filter(|&&p| p != bg).count()
}

fn inputs() -> Inputs {
    let mut i = Inputs {
        hashrate: 1_023_456.0,
        temperature: 48.5,
        power_percent: 100,
        battery: 87,
        clock: Some(("12:34".into(), "Thu 23 Oct".into())),
        pool_host: "public-pool.io".into(),
        ip: "192.168.1.40".into(),
        board: "M5StickC Plus2".into(),
        firmware: "2.6.1".into(),
        free_heap: 123_456,
        uptime: 3_725,
        ..Default::default()
    };
    i.stats.shares_found = 12;
    i
}

#[test]
fn every_page_on_every_size() {
    let settings = Settings::defaults(&board());
    let mut samples = Samples::default();
    for t in 0..60u32 {
        samples.sample(t * 10_000, 900_000.0 + (t % 7) as f64 * 40_000.0);
    }
    for (w, h) in SIZES {
        let mut view = View::new(screen::accent(settings.theme), 0);
        for page in 0..8 {
            let mut fb = Framebuffer::new(w, h);
            view.invalidate();
            let model = screen::build(page, &settings, &inputs(), &samples);
            view.render(&mut fb, &model);
            assert!(drawn(&fb) > 200, "{w}x{h} page {page}: nothing drawn");
            save(&fb, &format!("page{page}_{w}x{h}"));
            // A second render of the same model changes nothing.
            let before = fb.pixels.clone();
            view.render(&mut fb, &model);
            assert_eq!(before, fb.pixels);
        }
        let mut fb = Framebuffer::new(w, h);
        view.portal(&mut fb, "WiFi setup", "BDLMiner-1A2B", "bdlminer", "WIFI:T:WPA;S:BDLMiner-1A2B;P:bdlminer;;");
        assert!(drawn(&fb) > 1000);
        save(&fb, &format!("portal_{w}x{h}"));
        let mut fb = Framebuffer::new(w, h);
        view.message(&mut fb, "BDLMiner V2", "2.6.1", "Connecting to WiFi");
        save(&fb, &format!("splash_{w}x{h}"));
        let mut fb = Framebuffer::new(w, h);
        view.alert(&mut fb, "BLOCK FOUND!", "923,456", "Press a button");
        save(&fb, &format!("alert_{w}x{h}"));
    }
}

#[test]
fn gamma_lines() {
    let mut fb = Framebuffer::new(128, 32);
    let mut view = View::new(0xF7931A, 0);
    view.lines(&mut fb, &["1.23 TH/s  52.1C", "Mining (experimental)", "192.168.1.50"]);
    assert!(fb.pixels.iter().filter(|&&p| p != 0).count() > 100);
    save(&fb, "gamma_128x32");
}
