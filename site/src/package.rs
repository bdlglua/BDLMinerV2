//! Public website, installer, demos and release channels built entirely in Rust.
use crate::{html, render, PAGES};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    env, fs,
    io::{self, Read},
    path::Path,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::Url;
pub const BOARDS: &[(&str, &str)] = &[
    ("m5stickc-plus2", "M5StickC Plus2"),
    (
        "m5stack-esp32",
        "M5Stack ESP32 (Core, Core2, StickC Plus, Atom...)",
    ),
    (
        "m5stack-s3",
        "M5Stack ESP32-S3 (AtomS3, CoreS3, Cardputer...)",
    ),
    ("lilygo-t-display", "LilyGO T-Display"),
    ("lilygo-t-display-s3", "LilyGO T-Display-S3"),
    ("esp32-2432s028r", "ESP32-2432S028R (Cheap Yellow Display)"),
    ("esp32-headless", "ESP32 without a screen"),
    ("esp32s3-headless", "ESP32-S3 without a screen"),
    ("esp32s2-headless", "ESP32-S2 without a screen"),
    ("esp32c3-headless", "ESP32-C3 without a screen"),
    (
        "bitaxe-gamma-601",
        "Bitaxe Gamma 601 (experimental; 5V barrel supply + fan required)",
    ),
    (
        "nerdnos",
        "NerdNOS: BM1397 board on a T-Display-S3 (experimental; 5V 2A USB)",
    ),
];
/// The Rust firmware's beta (firmware-rs, `rust-v*` prereleases): its image
/// names, the installer's board names and their chips.
const RUST_BOARDS: &[(&str, &str, &str)] = &[
    ("m5stickc-plus2", "m5stickc-plus2", "esp32"),
    ("m5stack-esp32", "m5stack-esp32", "esp32"),
    ("m5stack-s3", "m5stack-s3", "esp32s3"),
    ("lilygo-t-display", "lilygo-t-display", "esp32"),
    ("lilygo-t-display-s3", "lilygo-t-display-s3", "esp32s3"),
    ("esp32-2432s028r", "esp32-2432s028r", "esp32"),
    ("esp32", "esp32-headless", "esp32"),
    ("esp32s3", "esp32s3-headless", "esp32s3"),
    ("esp32s2", "esp32s2-headless", "esp32s2"),
    ("esp32c3", "esp32c3-headless", "esp32c3"),
    ("bitaxe-gamma-601", "bitaxe-gamma-601", "esp32s3"),
    ("nerdnos", "nerdnos", "esp32s3"),
];
pub fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn read_json(path: &Path) -> io::Result<Value> {
    let data = fs::read(path)?;
    serde_json::from_slice(&data).map_err(invalid_json)
}
fn invalid_json(error: serde_json::Error) -> io::Error {
    invalid(error.to_string())
}
fn json_file(path: &Path, value: &Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec(value).map_err(invalid_json)?)
}
pub fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))[..12].into()
}
pub fn browser() -> io::Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args([
            "build",
            "--locked",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--manifest-path",
        ])
        .arg(root().join("site/browser/Cargo.toml"))
        .status()?;
    if !status.success() {
        return Err(invalid("browser Rust compilation failed"));
    }
    let output = root().join("installer/assets/wasm");
    fs::create_dir_all(&output)?;
    let status = Command::new("wasm-bindgen")
        .args(["--target", "web", "--no-typescript", "--out-dir"])
        .arg(&output)
        .arg(
            root().join("site/browser/target/wasm32-unknown-unknown/release/bdlminer_browser.wasm"),
        )
        .status()?;
    if !status.success() {
        return Err(invalid("WebAssembly bindings failed"));
    }
    let module = fingerprint(&fs::read(output.join("bdlminer_browser.js"))?);
    let wasm = fingerprint(&fs::read(output.join("bdlminer_browser_bg.wasm"))?);
    fs::write(output.join("boot.js"),format!("// Generated WASM loader; application source: site/browser/src/*.rs\nimport init from './bdlminer_browser.js?v={module}';\nawait init({{module_or_path:new URL('./bdlminer_browser_bg.wasm?v={wasm}',import.meta.url)}});\n"))?;
    Ok(())
}
pub fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(invalid("Public resources must not contain symlinks"));
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        } else {
            let source = entry.path();
            if source.extension().and_then(|s| s.to_str()) == Some("js")
                && !source.components().any(|c| c.as_os_str() == "wasm")
            {
                continue;
            }
            fs::copy(source, destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}
fn public_url(value: &str) -> io::Result<Url> {
    let url = Url::parse(value).map_err(|_| invalid("Invalid public URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid("Public links require HTTPS without credentials"));
    }
    Ok(url)
}
pub fn content(version: &str, channel: &str, channels: &Value) -> io::Result<Value> {
    validate_content(
        read_json(&root().join("installer/content.json"))?,
        version,
        channel,
        channels,
    )
}
fn validate_content(
    mut content: Value,
    version: &str,
    channel: &str,
    channels: &Value,
) -> io::Result<Value> {
    if let Some(links) = content["pool_links"].as_object() {
        for (pool, destination) in links {
            let url = public_url(
                destination
                    .as_str()
                    .ok_or_else(|| invalid("Invalid pool link"))?,
            )?;
            let host = url.host_str().unwrap();
            if ["other", "unknown"].contains(&pool.as_str())
                || content["pool_names"].get(pool).is_none()
                || url.query().is_some()
                || url.fragment().is_some()
                || !(host == pool || host.ends_with(&format!(".{pool}")))
            {
                return Err(invalid(
                    "Pool websites must belong to their recognised group",
                ));
            }
        }
    }
    for link in content["support"]["links"].as_array().into_iter().flatten() {
        public_url(
            link["url"]
                .as_str()
                .ok_or_else(|| invalid("Invalid support link"))?,
        )?;
    }
    for key in ["bitcoin", "lightning"] {
        if content["support"][key]
            .as_str()
            .unwrap_or_default()
            .chars()
            .any(|c| c.is_whitespace() || "<>\"'\\".contains(c))
        {
            return Err(invalid("Invalid donation destination"));
        }
    }
    for entry in content["gallery"].as_array().into_iter().flatten() {
        let image = Path::new(
            entry["image"]
                .as_str()
                .ok_or_else(|| invalid("Invalid gallery image"))?,
        );
        if image.is_absolute()
            || image
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            || !image.starts_with("assets")
            || !root().join("installer").join(image).is_file()
        {
            return Err(invalid("Gallery images must be existing local assets"));
        }
        let url = public_url(
            entry["url"]
                .as_str()
                .ok_or_else(|| invalid("Invalid gallery link"))?,
        )?;
        if ![
            "reddit.com",
            "www.reddit.com",
            "old.reddit.com",
            "discord.com",
            "discord.gg",
        ]
        .contains(&url.host_str().unwrap())
            || entry["permission"] != true
        {
            return Err(invalid(
                "Gallery photos require permission and an original Reddit/Discord post",
            ));
        }
    }
    content["version"] = version.into();
    content["channel"] = channel.into();
    content["channels"] = if channels.is_null() {
        json!({"stable":{"version":version,"published_at":null},"beta":{"version":version,"published_at":null}})
    } else {
        channels.clone()
    };
    Ok(content)
}
fn title(key: &str) -> [&'static str; 2] {
    match key {
        "home" => [
            "Bitcoin solo mining firmware",
            "Firmware de minage solo Bitcoin",
        ],
        "start" => ["Get started", "Bien démarrer"],
        "boards" => ["Supported boards", "Cartes compatibles"],
        "flash" => ["Flash your board", "Flasher ta carte"],
        "demo" => ["Interactive demos", "Démos interactives"],
        "faq" => ["Troubleshooting", "Dépannage"],
        "releases" => ["Releases", "Nouveautés"],
        "stats" => ["Live statistics", "Statistiques en direct"],
        "gallery" => ["Community", "Communauté"],
        "support" => ["Support BDLMiner", "Soutenir BDLMiner"],
        _ => ["My miners", "Mes mineurs"],
    }
}
fn script_json(value: &Value) -> String {
    value.to_string().replace('<', "\\u003c")
}
pub fn pages(output: &Path, version: &str, boards: &Value, content: &Value) -> io::Result<()> {
    fs::create_dir_all(output)?;
    let titles: serde_json::Map<String, Value> = PAGES
        .iter()
        .map(|p| {
            let labels = title(p.key);
            (p.key.into(), json!({"en":labels[0],"fr":labels[1]}))
        })
        .collect();
    let legacy = json!({"top":"index.html","discover":"start.html","boards":"boards.html","install":"flash.html","demo":"demo.html","help":"faq.html","news":"releases.html","stats":"stats.html","gallery":"gallery.html","community":"gallery.html","support":"support.html"});
    let mut fingerprints = Vec::new();
    for name in [
        "site.css",
        "discovery.css",
        "remote.css",
        "pulse-theme.css",
        "wasm/boot.js",
    ] {
        fingerprints.push((
            name,
            fingerprint(&fs::read(root().join("installer/assets").join(name))?),
        ));
    }
    for page in PAGES {
        let mut data = content.clone();
        data["page"] = page.key.into();
        data["page_titles"] = Value::Object(titles.clone());
        data["legacy_sections"] = legacy.clone();
        let mut html = render(page)?
            .replace("__PAGE_KEY__", page.key)
            .replace(
                "__PAGE_TITLE__",
                &html::escape(&format!("BDLMiner V2: {}", title(page.key)[0])),
            )
            .replace("__BOARDS__", &script_json(boards))
            .replace("__VERSION__", version)
            .replace("__SITE_DATA__", &script_json(&data));
        if content["channel"] == "rust" {
            html = html
                .replace(
                    &format!("releases/tag/v{version}"),
                    &format!("releases/tag/rust-v{version}"),
                )
                .replace(
                    "<p class=\"lead\" data-t=\"installLead\">",
                    "<p class=\"route-note\" data-t=\"rustFlashNote\">Rust firmware beta: experimental, not tested on every board. Settings are kept; flash the stable firmware to go back.</p><p class=\"lead\" data-t=\"installLead\">",
                );
        }
        for (name, digest) in &fingerprints {
            html = html.replace(
                &format!("\"assets/{name}\""),
                &format!("\"assets/{name}?v={digest}\""),
            );
        }
        for (key, destination) in legacy.as_object().unwrap() {
            html = html.replace(
                &format!("href=\"#{key}\""),
                &format!("href=\"{}\"", destination.as_str().unwrap()),
            );
        }
        fs::write(output.join(page.file), html)?;
    }
    fs::create_dir_all(output.join("remote"))?;
    fs::write(output.join("remote/index.html"),"<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"robots\" content=\"noindex\"><meta http-equiv=\"refresh\" content=\"0; url=../remote.html\"><link rel=\"canonical\" href=\"../remote.html\"><title>BDLMiner V2: My miners</title><p><a href=\"../remote.html\">My miners / Mes mineurs</a></p></html>\n")?;
    Ok(())
}
pub fn demos(output: &Path, version: &str) -> io::Result<()> {
    let strip_scripts = regex::Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>").unwrap();
    let strip_handlers = regex::Regex::new(r#"(?i)\s+on[a-z]+\s*=\s*(?:"[^"]*"|'[^']*')"#).unwrap();
    let policy="default-src 'self' data: blob:; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self'; form-action 'none'; base-uri 'self'";
    for mode in ["miner", "hub"] {
        let folder = output.join("demo").join(mode);
        fs::create_dir_all(&folder)?;
        fs::copy(
            root().join("installer/demo/demo.css"),
            folder.join("demo.css"),
        )?;
        let setup=format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{}\"><meta name=\"bdl-demo\" content=\"{mode}\"><meta name=\"bdl-demo-version\" content=\"{}\"><script type=\"module\" src=\"../../assets/wasm/boot.js?v={}\"></script><link rel=\"stylesheet\" href=\"demo.css\">",html::escape(policy),html::escape(version),fingerprint(&fs::read(root().join("installer/assets/wasm/boot.js"))?));
        if mode == "miner" {
            let source = root().join("web");
            fs::copy(source.join("style.css"), folder.join("style.css"))?;
            let mut header = fs::read_to_string(source.join("header.html"))?;
            for (old, new) in [
                ("/settings", "settings.html"),
                ("/log", "log.html"),
                ("/", "index.html"),
            ] {
                header = header.replace(&format!("href=\"{old}\""), &format!("href=\"{new}\""));
            }
            for (page, file) in [
                ("dashboard", "index.html"),
                ("settings", "settings.html"),
                ("log", "log.html"),
            ] {
                let mut body = header.clone()
                    + &fs::read_to_string(source.join(format!("{page}.html")))?
                    + "</div></div>";
                for (old, new) in [
                    ("/settings", "settings.html"),
                    ("/log", "log.html"),
                    ("/", "index.html"),
                ] {
                    body = body
                        .replace(&format!("href=\"{old}\""), &format!("href=\"{new}\""))
                        .replace(&format!("href=\"{old}#"), &format!("href=\"{new}#"));
                }
                body = strip_handlers.replace_all(&body, "").into_owned();
                fs::write(folder.join(file),format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>BDLMiner V2 · Demo</title>{setup}<link rel=\"icon\" href=\"../../assets/icon.svg\"><link rel=\"stylesheet\" href=\"style.css\"></head><body data-runtime=\"loading\">{body}</body></html>"))?;
            }
        } else {
            let source = root().join("tools/bdlhost/web");
            for name in ["style.css", "icon.svg"] {
                fs::copy(source.join(name), folder.join(name))?;
            }
            let original = fs::read_to_string(source.join("index.html"))?;
            let page = strip_scripts.replace_all(&original, "");
            let page = strip_handlers
                .replace_all(&page, "")
                .replace("href=\"/icon.svg\"", "href=\"icon.svg\"")
                .replace("href=\"/style.css\"", "href=\"style.css\"")
                .replace("href=\"/\"", "href=\"index.html\"")
                .replace("</head>", &(setup + "</head>"));
            fs::write(folder.join("index.html"), page)?;
        }
    }
    Ok(())
}
pub fn installer(
    release: &Path,
    output: &Path,
    channel: &str,
    channels: &Value,
) -> io::Result<(String, usize)> {
    let version = release
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| invalid("Missing release version"))?;
    parse_version(version)?;
    let chips = read_json(&release.join("boards.json"))?;
    let mut boards = Vec::new();
    fs::create_dir_all(output)?;
    for (env, name) in BOARDS {
        let Some(chip) = chips.get(env) else {
            continue;
        };
        let chip = chip.as_str().ok_or_else(|| invalid("Invalid board chip"))?;
        let family = match chip {
            "esp32" => "ESP32",
            "esp32s2" => "ESP32-S2",
            "esp32s3" => "ESP32-S3",
            "esp32c3" => "ESP32-C3",
            _ => return Err(invalid("Unsupported chip")),
        };
        let file = format!("{env}-factory.bin");
        let image = fs::read(release.join(&file))?;
        if image.is_empty() || image.len() > 16 * 1024 * 1024 {
            return Err(invalid("Invalid factory image size"));
        }
        let digest = format!("{:x}", Sha256::digest(&image));
        fs::write(output.join(&file), image)?;
        json_file(
            &output.join(format!("manifest-{env}.json")),
            &json!({"name":if channel == "rust" { format!("BDLMiner V2 Rust beta - {name}") } else { format!("BDLMiner V2 - {name}") },"version":version,"new_install_prompt_erase":true,"builds":[{"chipFamily":family,"parts":[{"path":file,"offset":0,"sha256":digest}]}]}),
        )?;
        boards.push(json!({"env":env,"name":name}));
    }
    if boards.is_empty() {
        return Err(invalid("No supported boards with factory images"));
    }
    pages(
        output,
        version,
        &json!(boards),
        &content(version, channel, channels)?,
    )?;
    copy_tree(&root().join("installer/assets"), &output.join("assets"))?;
    demos(output, version)?;
    Ok((version.into(), boards.len()))
}
fn parse_version(tag: &str) -> io::Result<semver::Version> {
    let text = tag.strip_prefix('v').unwrap_or(tag);
    if text.len() > 23 {
        return Err(invalid("Release version exceeds firmware limit"));
    }
    let core = text.split(['-', '+']).next().unwrap_or_default();
    let text = if core.matches('.').count() == 1 {
        text.replacen(core, &format!("{core}.0"), 1)
    } else {
        text.into()
    };
    let version = semver::Version::parse(&text).map_err(|_| invalid("Invalid release version"))?;
    if [version.major, version.minor, version.patch]
        .iter()
        .any(|n| *n > u64::from(u32::MAX))
    {
        return Err(invalid("Version component exceeds firmware limit"));
    }
    Ok(version)
}
fn select(releases: &[Value], stable: bool) -> Option<&Value> {
    releases
        .iter()
        .filter(|r| r["draft"] != true)
        .filter_map(|r| {
            let version = parse_version(r["tag_name"].as_str()?).ok()?;
            if stable && (!version.pre.is_empty() || r["prerelease"] == true) {
                return None;
            }
            Some((version, r))
        })
        .max_by(|(a, _), (b, _)| a.cmp_precedence(b))
        .map(|(_, release)| release)
}
/// The newest published Rust firmware release (`rust-v<version>`).
fn select_rust(releases: &[Value]) -> Option<&Value> {
    releases
        .iter()
        .filter(|r| r["draft"] != true)
        .filter_map(|r| {
            let tag = r["tag_name"].as_str()?.strip_prefix("rust-")?;
            Some((parse_version(tag).ok()?, r))
        })
        .max_by(|(a, _), (b, _)| a.cmp_precedence(b))
        .map(|(_, release)| release)
}
/// The Rust release's full images (`BDLMiner-rust-<version>-<board>.bin`)
/// as the installer reads a release: `<env>-factory.bin` and boards.json.
fn prepare_rust(folder: &Path, version: &str) -> io::Result<()> {
    let mut boards = serde_json::Map::new();
    for (board, env, chip) in RUST_BOARDS {
        let image = folder.join(format!("BDLMiner-rust-{version}-{board}.bin"));
        if image.is_file() {
            fs::rename(image, folder.join(format!("{env}-factory.bin")))?;
            boards.insert((*env).into(), (*chip).into());
        }
    }
    if boards.is_empty() {
        return Err(invalid("No Rust firmware images in the release"));
    }
    json_file(&folder.join("boards.json"), &Value::Object(boards))
}
fn encoded(value: &str) -> String {
    const SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    percent_encoding::utf8_percent_encode(value, SET).to_string()
}
fn validate_manifest(
    manifest: &Value,
    repository: &str,
    tag: &str,
    boards: &Value,
) -> io::Result<()> {
    if manifest["tag_name"] != tag {
        return Err(invalid("Channel manifest release mismatch"));
    }
    let boards = boards
        .as_object()
        .ok_or_else(|| invalid("Invalid board catalog"))?;
    let pattern = regex::Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap();
    if boards.keys().any(|k| k.len() > 64 || !pattern.is_match(k)) {
        return Err(invalid("Invalid board name"));
    }
    let expected: HashSet<_> = boards
        .keys()
        .flat_map(|env| {
            [
                format!("{env}-firmware.bin"),
                format!("{env}-firmware.bin.sig"),
            ]
        })
        .collect();
    let assets = manifest["assets"]
        .as_array()
        .ok_or_else(|| invalid("Missing signed firmware assets"))?;
    let actual: HashSet<_> = assets
        .iter()
        .filter_map(|a| a["name"].as_str().map(str::to_string))
        .collect();
    if assets.len() != expected.len() || actual != expected {
        return Err(invalid(
            "Each board requires exactly one signed firmware pair",
        ));
    }
    let base = format!(
        "https://github.com/{repository}/releases/download/{}/",
        encoded(tag)
    );
    let old = format!(
        "https://github.com/bdlglua/BDLMinerV1/releases/download/{}/",
        encoded(tag)
    );
    let allow_old =
        repository == "bdlglua/BDLMinerV2" && parse_version(tag)? <= semver::Version::new(2, 0, 0);
    for asset in assets {
        let name = asset["name"].as_str().unwrap();
        let url = asset["browser_download_url"].as_str().unwrap_or_default();
        if url != base.clone() + name && !(allow_old && url == old.clone() + name) {
            return Err(invalid(
                "Channel assets must belong to the selected repository release",
            ));
        }
    }
    Ok(())
}
fn download(repository: &str, tag: &str, folder: &Path) -> io::Result<()> {
    gh_download(
        repository,
        tag,
        folder,
        &["*-factory.bin", "boards.json", "update.json"],
    )
}
fn download_rust(repository: &str, tag: &str, folder: &Path) -> io::Result<()> {
    gh_download(repository, tag, folder, &["BDLMiner-rust-*.bin"])?;
    let version = tag
        .strip_prefix("rust-v")
        .ok_or_else(|| invalid("Not a Rust release"))?;
    prepare_rust(folder, version)
}
fn gh_download(repository: &str, tag: &str, folder: &Path, patterns: &[&str]) -> io::Result<()> {
    let transient_error = regex::Regex::new(r"\bHTTP 5\d\d\b").unwrap();
    for attempt in 0..4 {
        let mut command = Command::new("gh");
        command
            .args(["release", "download", tag, "--repo", repository, "--dir"])
            .arg(folder);
        for pattern in patterns {
            command.args(["--pattern", pattern]);
        }
        let result = command.arg("--clobber").output()?;
        if result.status.success() {
            return Ok(());
        }
        let transient = transient_error.is_match(&String::from_utf8_lossy(&result.stderr));
        if !transient || attempt == 3 {
            return Err(invalid("GitHub release asset download failed"));
        }
        std::thread::sleep(Duration::from_secs(1 << attempt));
    }
    unreachable!()
}
pub fn channels(repository: &str, output: &Path) -> io::Result<()> {
    if !regex::Regex::new(r"^[\w.-]+/[\w.-]+$")
        .unwrap()
        .is_match(repository)
    {
        return Err(invalid("Use owner/repository"));
    }
    let listing = Command::new("gh")
        .args([
            "api",
            "--paginate",
            &format!("repos/{repository}/releases?per_page=100"),
            "--jq",
            ".[] | {tag_name,draft,prerelease,published_at}",
        ])
        .output()?;
    if !listing.status.success() {
        return Err(invalid("Unable to list published releases"));
    }
    let releases = String::from_utf8(listing.stdout)
        .map_err(|_| invalid("Invalid release listing"))?
        .lines()
        .filter(|s| !s.trim().is_empty())
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid_json)?;
    assemble_channels(&releases, repository, output, download, download_rust)
}
fn assemble_channels(
    releases: &[Value],
    repository: &str,
    output: &Path,
    download: impl Fn(&str, &str, &Path) -> io::Result<()>,
    download_rust: impl Fn(&str, &str, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let stable =
        select(releases, true).ok_or_else(|| invalid("A published stable release is required"))?;
    let beta = select(releases, false).unwrap_or(stable);
    let temp = tempfile::Builder::new().prefix("bdl-channels-").tempdir()?;
    let mut channels = json!({"stable":{"version":stable["tag_name"].as_str().unwrap().trim_start_matches('v'),"published_at":stable["published_at"]},"beta":{"version":beta["tag_name"].as_str().unwrap().trim_start_matches('v'),"published_at":beta["published_at"]}});
    // The Rust firmware's beta, its own installer under rust/. Optional: a
    // missing or unreadable Rust release never holds up the stable site.
    let rust = select_rust(releases).and_then(|release| {
        let tag = release["tag_name"].as_str().unwrap();
        let version = tag.trim_start_matches("rust-v");
        let folder = temp.path().join("rust").join(version);
        match fs::create_dir_all(&folder).and_then(|_| download_rust(repository, tag, &folder)) {
            Ok(()) => {
                channels["rust"] =
                    json!({"version":version,"published_at":release["published_at"]});
                Some(folder)
            }
            Err(error) => {
                eprintln!("Rust firmware beta {tag} left out: {error}");
                None
            }
        }
    });
    let mut tags = HashSet::new();
    for item in [stable, beta] {
        let tag = item["tag_name"].as_str().unwrap();
        if !tags.insert(tag) {
            continue;
        }
        let folder = temp.path().join(tag.trim_start_matches('v'));
        fs::create_dir(&folder)?;
        download(repository, tag, &folder)?;
        let mut manifest = read_json(&folder.join("update.json"))?;
        let boards = read_json(&folder.join("boards.json"))?;
        validate_manifest(&manifest, repository, tag, &boards)?;
        for asset in manifest["assets"].as_array_mut().unwrap() {
            asset["browser_download_url"] = format!(
                "https://github.com/{repository}/releases/download/{}/{}",
                encoded(tag),
                asset["name"].as_str().unwrap()
            )
            .into();
        }
        json_file(&folder.join("update.json"), &manifest)?;
    }
    staged(output, None, |staged| {
        installer(
            &temp
                .path()
                .join(channels["stable"]["version"].as_str().unwrap()),
            staged,
            "stable",
            &channels,
        )?;
        installer(
            &temp
                .path()
                .join(channels["beta"]["version"].as_str().unwrap()),
            &staged.join("beta"),
            "beta",
            &channels,
        )?;
        if let Some(folder) = &rust {
            installer(folder, &staged.join("rust"), "rust", &channels)?;
        }
        fs::create_dir_all(staged.join("updates"))?;
        for channel in ["stable", "beta"] {
            fs::copy(
                temp.path()
                    .join(channels[channel]["version"].as_str().unwrap())
                    .join("update.json"),
                staged.join("updates").join(format!("{channel}.json")),
            )?;
        }
        Ok(())
    })?;
    println!(
        "Published stable {}, opt-in beta {} and Rust beta {}",
        channels["stable"]["version"], channels["beta"]["version"], channels["rust"]["version"]
    );
    Ok(())
}
pub fn staged(
    output: &Path,
    release: Option<&Path>,
    build: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        env::current_dir()?.join(output)
    };
    let parent = output
        .parent()
        .ok_or_else(|| invalid("Invalid export directory"))?;
    fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    let output = parent.join(
        output
            .file_name()
            .ok_or_else(|| invalid("Invalid export directory"))?,
    );
    let source = root().canonicalize()?;
    if source.starts_with(&output)
        || ["site", "installer", "web", "tools", ".git"]
            .iter()
            .any(|name| output.starts_with(source.join(name)))
        || release
            .map(|r| {
                r.canonicalize()
                    .map(|r| r.starts_with(&output))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
        || fs::symlink_metadata(&output)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    {
        return Err(invalid(
            "Export must not overwrite source or release directories",
        ));
    }
    let temp = tempfile::Builder::new()
        .prefix(".bdl-site-")
        .tempdir_in(&parent)?;
    build(temp.path())?;
    let backup = temp.path().with_extension("previous");
    let existed = output.exists();
    if existed {
        fs::rename(&output, &backup)?;
    }
    if let Err(error) = fs::rename(temp.path(), &output) {
        if existed {
            let _ = fs::rename(&backup, &output);
        }
        return Err(error);
    }
    if existed {
        fs::remove_dir_all(backup)?;
    }
    Ok(())
}
pub fn snapshot(output: &Path) -> io::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0;
    let mut response = None;
    for attempt in 0..2 {
        match ureq::get("https://api.bdl-pool.com:36018/api/summary")
            .set("User-Agent", "BDLMiner-public-snapshot/2 (Rust)")
            .set("Accept", "application/json")
            .set("Cache-Control", "no-cache")
            .timeout(Duration::from_secs(15))
            .call()
        {
            Ok(value) => {
                response = Some(value);
                break;
            }
            Err(error) => {
                let retry = matches!(&error, ureq::Error::Transport(_))
                    || matches!(&error,ureq::Error::Status(status,_) if *status==429 || *status>=500);
                if attempt == 1 || !retry {
                    return Err(invalid(match error {
                        ureq::Error::Status(status, _) => {
                            format!("Public statistics HTTP {status}")
                        }
                        ureq::Error::Transport(error) => {
                            format!("Public statistics transport {:?}", error.kind())
                        }
                    }));
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
    let response = response.ok_or_else(|| invalid("Public statistics unavailable"))?;
    let mut raw = Vec::new();
    response
        .into_reader()
        .take(512 * 1024 + 1)
        .read_to_end(&mut raw)?;
    if raw.len() > 512 * 1024 {
        return Err(invalid("Public summary exceeds size limit"));
    }
    let value: Value = serde_json::from_slice(&raw).map_err(invalid_json)?;
    write_summary(output, &public_summary(value, now)?)
}
fn public_summary(value: Value, now: f64) -> io::Result<Value> {
    let age = value["at"]
        .as_f64()
        .map(|at| now - at)
        .filter(|v| (-60000.0..=300000.0).contains(v));
    if age.is_none()
        || value["hashrate"]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .is_none()
        || value["miners"].as_u64().is_none()
    {
        return Err(invalid("Public summary is invalid or too old"));
    }
    for key in [
        "boards", "versions", "coins", "pools", "fleet", "models", "recent", "history",
    ] {
        if !value[key].is_null()
            && !value[key]
                .as_array()
                .map(|r| r.iter().all(Value::is_object))
                .unwrap_or(false)
        {
            return Err(invalid("Invalid public statistics rows"));
        }
    }
    let mut public = serde_json::Map::new();
    for key in [
        "at",
        "miners",
        "hashrate",
        "boards",
        "versions",
        "coins",
        "pools",
        "fleet",
        "models",
        "recent",
        "history",
        "current_hour",
        "periods",
        "record",
    ] {
        if let Some(value) = value.get(key) {
            public.insert(key.into(), value.clone());
        }
    }
    Ok(Value::Object(public))
}
fn write_summary(output: &Path, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > 512 * 1024 {
        return Err(invalid("Public summary exceeds size limit"));
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temp, &bytes)?;
    temp.persist(output).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest(repository: &str, tag: &str) -> Value {
        json!({"tag_name":tag,"assets":[{"name":"m5stickc-plus2-firmware.bin","browser_download_url":format!("https://github.com/{repository}/releases/download/{tag}/m5stickc-plus2-firmware.bin")},{"name":"m5stickc-plus2-firmware.bin.sig","browser_download_url":format!("https://github.com/{repository}/releases/download/{tag}/m5stickc-plus2-firmware.bin.sig")}]})
    }
    fn fixture(tag: &str, directory: &Path, repository: &str) -> io::Result<()> {
        json_file(
            &directory.join("boards.json"),
            &json!({"m5stickc-plus2":"esp32"}),
        )?;
        fs::write(
            directory.join("m5stickc-plus2-factory.bin"),
            b"fixture: never flash",
        )?;
        json_file(&directory.join("update.json"), &manifest(repository, tag))
    }
    #[test]
    fn stable_and_beta_use_semver_and_publish_exact_signed_pairs() {
        let temp = tempfile::tempdir().unwrap();
        let releases = json!([{"tag_name":"v2.9.0"},{"tag_name":"v2.10.0-beta.1","prerelease":true},{"tag_name":"v3.0.0","draft":true}]);
        assemble_channels(
            releases.as_array().unwrap(),
            "bdlglua/BDLMinerV2",
            &temp.path().join("site"),
            |repo, tag, dir| fixture(tag, dir, repo),
            |_, _, _| Err(invalid("no Rust release in this test")),
        )
        .unwrap();
        let out = temp.path().join("site");
        assert_eq!(
            read_json(&out.join("manifest-m5stickc-plus2.json")).unwrap()["version"],
            "2.9.0"
        );
        assert_eq!(
            read_json(&out.join("beta/manifest-m5stickc-plus2.json")).unwrap()["version"],
            "2.10.0-beta.1"
        );
        assert_eq!(
            read_json(&out.join("updates/beta.json")).unwrap(),
            manifest("bdlglua/BDLMinerV2", "v2.10.0-beta.1")
        );
        let html = fs::read_to_string(out.join("flash.html")).unwrap();
        assert!(html.contains("assets/wasm/boot.js"));
        assert!(!html.contains("esp-web-tools"));
        assert!(html.contains("data-runtime=\"loading\""));
        assert!(!out.join("assets/site.js").exists());
        let demo = fs::read_to_string(out.join("demo/miner/index.html")).unwrap();
        assert!(demo.contains("wasm-unsafe-eval"));
        assert!(demo.contains("connect-src &#39;self&#39;") || demo.contains("connect-src 'self'"));
        assert!(!out.join("demo/miner/dashboard.js").exists());
    }
    #[test]
    fn rust_beta_gets_its_own_installer_and_never_blocks_the_site() {
        let temp = tempfile::tempdir().unwrap();
        let releases = json!([{"tag_name":"v2.9.0"},{"tag_name":"rust-v2.6.1-beta.1","prerelease":true},{"tag_name":"rust-v2.6.1-beta.2","draft":true}]);
        assert_eq!(
            select_rust(releases.as_array().unwrap()).unwrap()["tag_name"],
            "rust-v2.6.1-beta.1"
        );
        assert_eq!(
            select(releases.as_array().unwrap(), false).unwrap()["tag_name"],
            "v2.9.0"
        );
        assemble_channels(
            releases.as_array().unwrap(),
            "bdlglua/BDLMinerV2",
            &temp.path().join("site"),
            |repo, tag, dir| fixture(tag, dir, repo),
            |_, tag, dir| {
                assert_eq!(tag, "rust-v2.6.1-beta.1");
                for board in ["bitaxe-gamma-601", "esp32"] {
                    fs::write(
                        dir.join(format!("BDLMiner-rust-2.6.1-beta.1-{board}.bin")),
                        b"fixture: never flash",
                    )?;
                }
                prepare_rust(dir, "2.6.1-beta.1")
            },
        )
        .unwrap();
        let out = temp.path().join("site");
        let gamma = read_json(&out.join("rust/manifest-bitaxe-gamma-601.json")).unwrap();
        assert_eq!(gamma["version"], "2.6.1-beta.1");
        assert!(gamma["name"].as_str().unwrap().contains("Rust beta"));
        assert!(out.join("rust/esp32-headless-factory.bin").exists());
        let flash = fs::read_to_string(out.join("rust/flash.html")).unwrap();
        assert!(flash.contains("releases/tag/rust-v2.6.1-beta.1"));
        assert!(flash.contains("rustFlashNote"));
        assert!(!fs::read_to_string(out.join("flash.html"))
            .unwrap()
            .contains("rustFlashNote"));
        assert!(fs::read_to_string(out.join("releases.html"))
            .unwrap()
            .contains("2.6.1-beta.1"));
        // A failing Rust download leaves the stable and beta site as it was.
        let without = temp.path().join("without");
        assemble_channels(
            releases.as_array().unwrap(),
            "bdlglua/BDLMinerV2",
            &without,
            |repo, tag, dir| fixture(tag, dir, repo),
            |_, _, _| Err(invalid("download failed")),
        )
        .unwrap();
        assert!(without.join("flash.html").exists() && !without.join("rust").exists());
    }
    #[test]
    fn repository_rename_alias_is_exact_and_only_for_archived_releases() {
        let boards = json!({"m5stickc-plus2":"esp32"});
        assert!(validate_manifest(
            &manifest("bdlglua/BDLMinerV1", "v2.0.0"),
            "bdlglua/BDLMinerV2",
            "v2.0.0",
            &boards
        )
        .is_ok());
        for (from, to, tag) in [
            ("bdlglua/BDLMinerV1", "bdlglua/BDLMinerV2", "v2.0.1"),
            ("someone/BDLMinerV1", "bdlglua/BDLMinerV2", "v2.0.0"),
            ("bdlglua/BDLMinerV1", "someone/BDLMinerV2", "v2.0.0"),
        ] {
            assert!(validate_manifest(&manifest(from, tag), to, tag, &boards).is_err());
        }
        for suffix in ["?redirect=elsewhere", "/other"] {
            let mut m = manifest("bdlglua/BDLMinerV1", "v2.0.0");
            m["assets"][0]["browser_download_url"] = format!(
                "{}{suffix}",
                m["assets"][0]["browser_download_url"].as_str().unwrap()
            )
            .into();
            assert!(validate_manifest(&m, "bdlglua/BDLMinerV2", "v2.0.0", &boards).is_err());
        }
        let temp = tempfile::tempdir().unwrap();
        assemble_channels(
            &[json!({"tag_name":"v2.0.0"})],
            "bdlglua/BDLMinerV2",
            &temp.path().join("site"),
            |_, tag, dir| fixture(tag, dir, "bdlglua/BDLMinerV1"),
            |_, _, _| Err(invalid("no Rust release in this test")),
        )
        .unwrap();
        assert_eq!(
            read_json(&temp.path().join("site/updates/stable.json")).unwrap(),
            manifest("bdlglua/BDLMinerV2", "v2.0.0")
        );
    }
    #[test]
    fn failed_export_keeps_previous_site_and_success_removes_stale_boards() {
        let temp = tempfile::tempdir().unwrap();
        let release = temp.path().join("2.0.1");
        fs::create_dir(&release).unwrap();
        json_file(
            &release.join("boards.json"),
            &json!({"esp32-headless":"esp32","esp32c3-headless":"esp32c3"}),
        )
        .unwrap();
        fs::write(release.join("esp32-headless-factory.bin"), b"fixture").unwrap();
        let out = temp.path().join("site");
        fs::create_dir(&out).unwrap();
        fs::write(out.join("index.html"), "previous").unwrap();
        fs::write(out.join("stale-factory.bin"), "old").unwrap();
        let build = |dest: &Path| installer(&release, dest, "stable", &Value::Null).map(|_| ());
        assert!(staged(&out, Some(&release), build).is_err());
        assert_eq!(
            fs::read_to_string(out.join("index.html")).unwrap(),
            "previous"
        );
        assert!(out.join("stale-factory.bin").exists());
        fs::write(release.join("esp32c3-headless-factory.bin"), b"fixture").unwrap();
        staged(&out, Some(&release), build).unwrap();
        assert!(!out.join("stale-factory.bin").exists());
        assert!(out.join("manifest-esp32c3-headless.json").exists());
        assert!(staged(&root().join("installer"), None, |_| Ok(())).is_err());
        assert!(staged(&release, Some(&release), |_| Ok(())).is_err());
    }
    #[test]
    fn public_content_rejects_script_credentials_private_links_and_unconsented_photos() {
        let original = read_json(&root().join("installer/content.json")).unwrap();
        for url in [
            "javascript:alert(1)",
            "http://example.com",
            "https://name:secret@example.com",
        ] {
            let mut c = original.clone();
            c["support"]["links"] = json!([{"url":url}]);
            assert!(validate_content(c, "2.0.1", "stable", &Value::Null).is_err());
        }
        for (pool, url) in [
            ("public-pool.io", "https://192.168.1.40/"),
            ("public-pool.io", "https://public-pool.io.evil.example/"),
            (
                "public-pool.io",
                "https://web.public-pool.io/?worker=secret",
            ),
            ("public-pool.io", "https://web.public-pool.io/#secret"),
            ("other", "https://private.example/"),
            ("unknown", "https://example.com/"),
        ] {
            let mut c = original.clone();
            c["pool_links"] = json!({pool:url});
            assert!(validate_content(c, "2.0.1", "stable", &Value::Null).is_err());
        }
        for entry in [
            json!({"image":"assets/icon.svg","url":"https://reddit.com/r/bdlminer"}),
            json!({"image":"../image.png","url":"https://reddit.com/r/bdlminer","permission":true}),
            json!({"image":"assets/icon.svg","url":"javascript:alert(1)","permission":true}),
        ] {
            let mut c = original.clone();
            c["gallery"] = json!([entry]);
            assert!(validate_content(c, "2.0.1", "stable", &Value::Null).is_err());
        }
        let label = "</script><script id=\"injected\">alert(1)</script>";
        let mut c = original;
        c["support"]["links"][0]["label"] = label.into();
        let encoded = script_json(&c);
        assert!(!encoded.contains("</script>"));
        assert_eq!(serde_json::from_str::<Value>(&encoded).unwrap(), c);
    }
    #[test]
    fn public_snapshot_preserves_real_zero_and_excludes_unknown_fields() {
        let now = 1794326400000.0;
        let good = json!({"at":now,"miners":0,"hashrate":0,"recent":[],"history":[],"boards":[],"private_token":"secret","internal":{"addresses":["private"]}});
        let public = public_summary(good.clone(), now).unwrap();
        assert_eq!(public["hashrate"], 0);
        assert!(public.get("private_token").is_none());
        assert!(public.get("internal").is_none());
        for change in [
            json!({"at":now-300001.0}),
            json!({"at":now+60001.0}),
            json!({"miners":true}),
            json!({"miners":-1}),
            json!({"hashrate":-1}),
            json!({"recent":[null]}),
            json!({"models":{}}),
        ] {
            let mut bad = good.clone();
            for (k, v) in change.as_object().unwrap() {
                bad[k] = v.clone();
            }
            assert!(public_summary(bad, now).is_err());
        }
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("summary.json");
        write_summary(&file, &public).unwrap();
        assert!(write_summary(&file, &json!({"boards":[{"name":"x".repeat(512*1024)}]})).is_err());
        assert_eq!(read_json(&file).unwrap(), public);
    }
}
