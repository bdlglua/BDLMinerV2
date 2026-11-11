mod html;
mod package;

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
    let option = |key: &str| {
        arguments
            .windows(2)
            .find(|pair| pair[0] == key)
            .map(|pair| pair[1].as_str())
    };
    let required = |key: &str| {
        option(key)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("missing {key}")))
    };
    match arguments.first().map(String::as_str) {
        None => { package::browser()?; build(false) },
        Some("--check") if arguments.len() == 1 => build(true),
        Some("channels") => { package::browser()?; build(false)?; package::channels(required("--repository")?, Path::new(required("--output")?)) },
        Some("installer") if arguments.len() >= 3 => {
            if !arguments.iter().any(|arg| arg == "--reuse-browser") { package::browser()?; }
            let release = Path::new(&arguments[1]).canonicalize()?;
            let output = Path::new(&arguments[2]);
            let channels = option("--channels").map(fs::read).transpose()?.map(|raw| serde_json::from_slice::<serde_json::Value>(&raw)).transpose()? .unwrap_or(serde_json::Value::Null);
            package::staged(output, Some(&release), |staged| { package::installer(&release, staged, option("--channel").unwrap_or("stable"), &channels)?; Ok(()) })
        },
        Some("pages") => {
            let boards = serde_json::from_str(required("--boards")?)?;
            let content = serde_json::from_str(required("--content")?)?;
            package::pages(Path::new(required("--output")?), required("--version")?, &boards, &content)
        },
        Some("demos") if arguments.len() == 3 => package::demos(Path::new(&arguments[1]), &arguments[2]),
        Some("content") if arguments.len() >= 2 => { println!("{}", package::content(&arguments[1], option("--channel").unwrap_or("stable"), &serde_json::Value::Null)?); Ok(()) },
        Some("snapshot") => package::snapshot(Path::new(required("--output")?)),
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "usage: bdlminer-site [--check|channels --repository OWNER/REPO --output PATH|installer RELEASE OUTPUT|pages|demos OUTPUT VERSION|snapshot --output PATH]")),
    }
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
            assert!(document.contains("id=\"site-boards\""));
            assert!(!document.contains("_next/"));
            assert!(!document.contains("__NEXT_DATA__"));
            assert_eq!(document.contains("rel=\"preconnect\""), page.chart);
            assert_eq!(document.contains("id=\"installer\""), page.flasher);
            assert!(document.contains("assets/wasm/boot.js"));
            assert_eq!(document.contains("bdl-remote-api"), page.remote);
        }
        assert_eq!(files.len(), 11);
    }
}
