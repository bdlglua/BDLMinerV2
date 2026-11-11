mod html;

use std::{env, fs, io, path::Path};

struct Page {
    key: &'static str,
    file: &'static str,
    body: &'static str,
    chart: bool,
    flasher: bool,
    remote: bool,
}

macro_rules! page {
    ($key:literal, $file:literal, $chart:literal, $flasher:literal, $remote:literal) => {
        Page {
            key: $key,
            file: $file,
            body: include_str!(concat!("../templates/pages/", $file)),
            chart: $chart,
            flasher: $flasher,
            remote: $remote,
        }
    };
}

const PAGES: &[Page] = &[
    page!("home", "index.html", true, false, false),
    page!("start", "start.html", false, false, false),
    page!("boards", "boards.html", false, false, false),
    page!("flash", "flash.html", false, true, false),
    page!("demo", "demo.html", false, false, false),
    page!("faq", "faq.html", false, false, false),
    page!("releases", "releases.html", false, false, false),
    page!("stats", "stats.html", true, false, false),
    page!("gallery", "gallery.html", false, false, false),
    page!("support", "support.html", false, false, false),
    page!("remote", "remote.html", false, false, true),
];

fn render(page: &Page) -> io::Result<String> {
    let mut extra = String::new();
    if page.chart {
        extra.push_str(include_str!("../templates/chart-head.html"));
    }
    if page.flasher {
        extra.push_str(include_str!("../templates/flash-head.html"));
    }
    if page.remote {
        extra.push_str(include_str!("../templates/remote-head.html"));
    }
    let head = html::expand(
        include_str!("../templates/head.html"),
        &[("page_head", extra)],
    )?;
    html::expand(
        include_str!("../templates/document.html"),
        &[
            ("head", head),
            ("page_key", html::escape(page.key)),
            (
                "navigation",
                include_str!("../templates/navigation.html").into(),
            ),
            ("content", page.body.into()),
            ("footer", include_str!("../templates/footer.html").into()),
            ("chat", include_str!("../templates/chat.html").into()),
        ],
    )
}

fn build(check: bool) -> io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let destination = root.join("installer/site");
    // Render and validate everything before touching the generated export.
    let pages = PAGES
        .iter()
        .map(|page| render(page).map(|html| (page.file, html)))
        .collect::<io::Result<Vec<_>>>()?;
    if !check {
        fs::create_dir_all(&destination)?;
    }
    for (file, html) in pages {
        let output = destination.join(file);
        if check {
            if fs::read_to_string(&output)? != html {
                return Err(io::Error::other(format!(
                    "{} is stale; run cargo run --manifest-path site/Cargo.toml",
                    output.display()
                )));
            }
        } else {
            fs::write(output, html)?;
        }
    }
    println!(
        "{} {} Rust-rendered pages in {}",
        if check { "Checked" } else { "Exported" },
        PAGES.len(),
        destination.display()
    );
    Ok(())
}

fn main() -> io::Result<()> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.iter().any(|arg| arg != "--check") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: bdlminer-site [--check]",
        ));
    }
    build(arguments.iter().any(|arg| arg == "--check"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_route_is_unique_and_framework_free() {
        let mut files = HashSet::new();
        for page in PAGES {
            assert!(files.insert(page.file));
            let document = render(page).unwrap();
            assert!(document.starts_with("<!doctype html>"));
            assert!(document.contains("<main id=\"main\""));
            assert!(document.contains("__SITE_DATA__"));
            assert!(document.contains("__VERSION__"));
            assert!(!document.contains("_next/"));
            assert!(!document.contains("__NEXT_DATA__"));
            assert_eq!(document.contains("public-stats-bootstrap"), page.chart);
            assert_eq!(document.contains("install-button.js"), page.flasher);
            assert_eq!(document.contains("assets/remote.js"), page.remote);
        }
        assert_eq!(files.len(), 11);
    }
}
