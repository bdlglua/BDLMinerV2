//! ESP ROM/stub protocol over Web Serial. No JavaScript flasher is executed.
use crate::{dom::*, ui};
use md5::{Digest, Md5};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn packet(op: u8, data: &[u8], checksum: u32) -> Vec<u8> {
    let mut bytes = vec![0, op];
    bytes.extend_from_slice(&(data.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&checksum.to_le_bytes());
    bytes.extend_from_slice(data);
    slip(&bytes)
}
fn slip(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![0xc0];
    for byte in bytes {
        match byte {
            0xc0 => out.extend_from_slice(&[0xdb, 0xdc]),
            0xdb => out.extend_from_slice(&[0xdb, 0xdd]),
            byte => out.push(*byte),
        }
    }
    out.push(0xc0);
    out
}
fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0xef_u32, |sum, b| sum ^ u32::from(*b))
}
async fn sleep(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        later(ms, move || {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        });
    });
    let _ = JsFuture::from(promise).await;
}
async fn promise(obj: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let value = call(obj, method, args).map_err(|_| format!("Serial {method} failed"))?;
    JsFuture::from(js_sys::Promise::resolve(&value))
        .await
        .map_err(|_| format!("Serial {method} failed"))
}
struct Serial {
    port: JsValue,
    reader: JsValue,
    writer: JsValue,
    bytes: Rc<RefCell<VecDeque<u8>>>,
    failed: Rc<RefCell<Option<String>>>,
    stopped: Rc<Cell<bool>>,
    done: Rc<Cell<bool>>,
}
impl Serial {
    async fn open(port: JsValue) -> Result<Self, String> {
        promise(
            &port,
            "open",
            &[js(&json!({"baudRate":115200,"bufferSize":65536}))],
        )
        .await?;
        let reader = call(&property(&port, "readable"), "getReader", &[])
            .map_err(|_| "Serial input unavailable")?;
        let writer = call(&property(&port, "writable"), "getWriter", &[])
            .map_err(|_| "Serial output unavailable")?;
        let bytes = Rc::new(RefCell::new(VecDeque::new()));
        let failed = Rc::new(RefCell::new(None));
        let stopped = Rc::new(Cell::new(false));
        let done = Rc::new(Cell::new(false));
        let input = reader.clone();
        let queue = bytes.clone();
        let error = failed.clone();
        let stop = stopped.clone();
        let complete = done.clone();
        run(async move {
            while !stop.get() {
                match promise(&input, "read", &[]).await {
                    Ok(result) => {
                        if property(&result, "done").as_bool() == Some(true) {
                            break;
                        }
                        let value = property(&result, "value");
                        let data = js_sys::Uint8Array::new(&value).to_vec();
                        let mut queue = queue.borrow_mut();
                        if queue.len() + data.len() > 1024 * 1024 {
                            *error.borrow_mut() = Some("Serial response too large".into());
                            break;
                        }
                        queue.extend(data);
                    }
                    Err(e) => {
                        if !stop.get() {
                            *error.borrow_mut() = Some(e);
                        }
                        break;
                    }
                }
            }
            complete.set(true);
        });
        Ok(Self {
            port,
            reader,
            writer,
            bytes,
            failed,
            stopped,
            done,
        })
    }
    async fn signals(&self, dtr: bool, rts: bool) -> Result<(), String> {
        promise(
            &self.port,
            "setSignals",
            &[js(&json!({"dataTerminalReady":dtr,"requestToSend":rts}))],
        )
        .await
        .map(|_| ())
    }
    async fn reset_download(&self) {
        let info = call(&self.port, "getInfo", &[]).ok();
        let usb = info
            .as_ref()
            .map(|i| {
                property(i, "usbVendorId").as_f64() == Some(0x303a as f64)
                    && property(i, "usbProductId").as_f64() == Some(0x1001 as f64)
            })
            .unwrap_or(false);
        if usb {
            let _ = self.signals(false, false).await;
            sleep(100).await;
            let _ = self.signals(true, false).await;
            sleep(100).await;
            let _ = self.signals(true, true).await;
            let _ = self.signals(false, true).await;
            sleep(100).await;
            let _ = self.signals(false, false).await;
        } else {
            let _ = self.signals(false, true).await;
            sleep(100).await;
            let _ = self.signals(true, false).await;
            sleep(50).await;
            let _ = self.signals(false, false).await;
        }
        sleep(50).await;
        self.bytes.borrow_mut().clear();
    }

    async fn write(&self, bytes: &[u8]) -> Result<(), String> {
        promise(
            &self.writer,
            "write",
            &[js_sys::Uint8Array::from(bytes).into()],
        )
        .await
        .map(|_| ())
    }
    async fn frame(&self, timeout: f64) -> Result<Vec<u8>, String> {
        let deadline = now() + timeout;
        let mut output = Vec::new();
        let mut started = false;
        let mut escaped = false;
        loop {
            if now() > deadline {
                return Err(
                    "Serial response timed out. Hold BOOT while connecting, then release it."
                        .into(),
                );
            }
            if let Some(error) = self.failed.borrow().clone() {
                return Err(error);
            }
            let byte = self.bytes.borrow_mut().pop_front();
            match byte {
                Some(0xc0) => {
                    if started && !output.is_empty() {
                        return Ok(output);
                    }
                    started = true;
                    escaped = false;
                }
                Some(byte) if started => {
                    if escaped {
                        match byte {
                            0xdc => output.push(0xc0),
                            0xdd => output.push(0xdb),
                            _ => {
                                started = false;
                                output.clear();
                            }
                        }
                        escaped = false;
                    } else if byte == 0xdb {
                        escaped = true;
                    } else {
                        output.push(byte);
                    }
                    if output.len() > 256 * 1024 {
                        return Err("Invalid serial response".into());
                    }
                }
                Some(_) => {}
                None => {
                    if self.done.get() {
                        return Err("Serial connection closed".into());
                    }
                    sleep(5).await;
                }
            }
        }
    }
    async fn command(
        &self,
        op: u8,
        data: &[u8],
        checksum: u32,
        timeout: f64,
    ) -> Result<(u32, Vec<u8>), String> {
        self.write(&packet(op, data, checksum)).await?;
        let deadline = now() + timeout;
        for _ in 0..100 {
            let frame = self.frame((deadline - now()).max(1.0)).await?;
            if frame.len() < 8 || frame[0] != 1 || frame[1] != op {
                continue;
            }
            let length = u16::from_le_bytes([frame[2], frame[3]]) as usize;
            if frame.len() != length + 8 {
                return Err("Invalid ESP response length".into());
            }
            return Ok((
                u32::from_le_bytes(frame[4..8].try_into().unwrap()),
                frame[8..].to_vec(),
            ));
        }
        Err("No matching ESP response".into())
    }
    async fn checked(
        &self,
        op: u8,
        data: &[u8],
        checksum: u32,
        expected: usize,
        timeout: f64,
    ) -> Result<Vec<u8>, String> {
        let (_, reply) = self.command(op, data, checksum, timeout).await?;
        if reply.len() < expected + 2 || reply[expected] != 0 {
            return Err(format!(
                "ESP command 0x{op:02x} failed. No further writes were sent."
            ));
        }
        Ok(reply[..expected].to_vec())
    }
    async fn synchronize(&self) -> Result<bool, String> {
        let mut sync = vec![7, 7, 18, 32];
        sync.extend_from_slice(&[0x55; 32]);
        for _ in 0..5 {
            if let Ok((value, response)) = self.command(8, &sync, 0, 1000.0).await {
                if response.first() == Some(&0) {
                    return Ok(value == 0);
                }
            }
        }
        Err("Unable to synchronize. Hold BOOT while connecting, then release it.".into())
    }
    async fn read_reg(&self, address: u32) -> Result<u32, String> {
        let (value, response) = self.command(10, &words(&[address]), 0, 3000.0).await?;
        if response.first() != Some(&0) {
            return Err("Unable to identify this chip".into());
        }
        Ok(value)
    }
    async fn close(self) {
        self.stopped.set(true);
        let _ = promise(&self.reader, "cancel", &[]).await;
        for _ in 0..100 {
            if self.done.get() {
                break;
            }
            sleep(10).await;
        }
        let _ = call(&self.reader, "releaseLock", &[]);
        let _ = call(&self.writer, "releaseLock", &[]);
        let _ = promise(&self.port, "close", &[]).await;
    }
}
struct Stub {
    entry: u32,
    text_start: u32,
    data_start: u32,
    text: &'static [u8],
    data: &'static [u8],
}
fn stub(chip: &str) -> Result<Stub, String> {
    let all: Value = serde_json::from_str(include_str!("../stubs/metadata.json")).unwrap();
    let meta = &all[chip];
    let (text, data): (&[u8], &[u8]) = match chip {
        "esp32" => (
            include_bytes!("../stubs/esp32-text.bin"),
            include_bytes!("../stubs/esp32-data.bin"),
        ),
        "esp32s2" => (
            include_bytes!("../stubs/esp32s2-text.bin"),
            include_bytes!("../stubs/esp32s2-data.bin"),
        ),
        "esp32s3" => (
            include_bytes!("../stubs/esp32s3-text.bin"),
            include_bytes!("../stubs/esp32s3-data.bin"),
        ),
        "esp32c3" => (
            include_bytes!("../stubs/esp32c3-text.bin"),
            include_bytes!("../stubs/esp32c3-data.bin"),
        ),
        _ => return Err("Unsupported chip".into()),
    };
    Ok(Stub {
        entry: meta["entry"].as_u64().unwrap() as u32,
        text_start: meta["text_start"].as_u64().unwrap() as u32,
        data_start: meta["data_start"].as_u64().unwrap() as u32,
        text,
        data,
    })
}
async fn load_stub(serial: &Serial, stub: Stub) -> Result<(), String> {
    for (address, data) in [(stub.text_start, stub.text), (stub.data_start, stub.data)] {
        let blocks = data.len().div_ceil(6144);
        serial
            .checked(
                5,
                &words(&[data.len() as u32, blocks as u32, 6144, address]),
                0,
                0,
                3000.0,
            )
            .await?;
        for (index, chunk) in data.chunks(6144).enumerate() {
            let mut packet = words(&[chunk.len() as u32, index as u32, 0, 0]);
            packet.extend_from_slice(chunk);
            serial
                .checked(7, &packet, checksum(chunk), 0, 3000.0)
                .await?;
        }
    }
    serial
        .checked(6, &words(&[0, stub.entry]), 0, 0, 3000.0)
        .await?;
    if serial.frame(3000.0).await? != b"OHAI" {
        return Err("Flash stub did not start".into());
    }
    Ok(())
}
fn chip_name(magic: u32) -> Option<&'static str> {
    match magic {
        0x00f01d83 => Some("esp32"),
        0x000007c6 => Some("esp32s2"),
        0x00000009 => Some("esp32s3"),
        0x6921506f | 0x1b31506f | 0x4881606f | 0x4361606f => Some("esp32c3"),
        _ => None,
    }
}
fn family_chip(family: &str) -> Option<&'static str> {
    match family {
        "ESP32" => Some("esp32"),
        "ESP32-S2" => Some("esp32s2"),
        "ESP32-S3" => Some("esp32s3"),
        "ESP32-C3" => Some("esp32c3"),
        _ => None,
    }
}
fn image_chip(image: &[u8], chip: &str) -> Result<(), String> {
    let offset = if chip == "esp32" || chip == "esp32s2" {
        0x1000
    } else {
        0
    };
    let header = image
        .get(offset..offset + 24)
        .ok_or("Factory image is truncated")?;
    let expected = match chip {
        "esp32" => 0,
        "esp32s2" => 2,
        "esp32s3" => 9,
        "esp32c3" => 5,
        _ => return Err("Unsupported image chip".into()),
    };
    if header[0] != 0xe9 || u16::from_le_bytes([header[12], header[13]]) != expected {
        return Err("Factory image does not match the selected chip".into());
    }
    Ok(())
}
fn status(message: &str, percent: Option<f64>) {
    text("flash-status", message);
    if let Some(percent) = percent {
        attr("flash-progress", "value", &percent.to_string());
        hidden("flash-progress", false);
    }
}
async fn install(serial: &Serial, board: &str) -> Result<(), String> {
    status(tr("Lecture du firmware…", "Loading firmware…"), None);
    let manifest = fetch(&format!("manifest-{board}.json"), 15000).await?;
    let builds = manifest["builds"]
        .as_array()
        .filter(|b| b.len() == 1)
        .ok_or("Invalid installer manifest")?;
    let build = &builds[0];
    let chip = family_chip(build["chipFamily"].as_str().unwrap_or_default())
        .ok_or("Unsupported chip family")?;
    let parts = build["parts"]
        .as_array()
        .filter(|p| p.len() == 1)
        .ok_or("Invalid factory image manifest")?;
    let file = parts[0]["path"].as_str().ok_or("Missing factory image")?;
    if file != format!("{board}-factory.bin") || parts[0]["offset"] != 0 {
        return Err("Unexpected factory image path or offset".into());
    }
    let options = web_sys::RequestInit::new();
    options.set_credentials(web_sys::RequestCredentials::Omit);
    options.set_signal(Some(&web_sys::AbortSignal::timeout_with_u32(30000)));
    let response = JsFuture::from(window().fetch_with_str_and_init(file, &options))
        .await
        .map_err(|_| "Firmware download failed")?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| "Firmware response invalid")?;
    if !response.ok() {
        return Err("Firmware download failed".into());
    }
    let image = js_sys::Uint8Array::new(
        &JsFuture::from(
            response
                .array_buffer()
                .map_err(|_| "Firmware response invalid")?,
        )
        .await
        .map_err(|_| "Firmware download failed")?,
    )
    .to_vec();
    if image.is_empty() || image.len() > 16 * 1024 * 1024 {
        return Err("Invalid factory image size".into());
    }
    image_chip(&image, chip)?;
    let expected = parts[0]["sha256"]
        .as_str()
        .ok_or("Missing factory image checksum")?;
    if format!("{:x}", sha2::Sha256::digest(&image)) != expected {
        return Err("Factory image checksum does not match its manifest".into());
    }
    status(
        tr(
            "Connexion au chargeur de la carte…",
            "Connecting to the board bootloader…",
        ),
        None,
    );
    serial.reset_download().await;
    let running = serial.synchronize().await?;
    let detected = chip_name(serial.read_reg(0x40001000).await?)
        .ok_or("Unrecognized chip; flash was not erased")?;
    if detected != chip {
        return Err(format!(
            "Chip mismatch: expected {chip}, found {detected}. Flash was not erased."
        ));
    }
    if !running {
        load_stub(serial, stub(chip)?).await?;
    }
    serial.checked(0x0d, &words(&[0]), 0, 0, 3000.0).await?;
    // Only after chip/image validation may the selected device be erased.
    status(
        tr("Effacement de la carte…", "Erasing the board…"),
        Some(0.0),
    );
    serial.checked(0xd0, &[], 0, 0, 120000.0).await?;
    let blocks = image.len().div_ceil(4096);
    serial
        .checked(
            2,
            &words(&[image.len() as u32, blocks as u32, 4096, 0]),
            0,
            0,
            60000.0,
        )
        .await?;
    for (index, chunk) in image.chunks(4096).enumerate() {
        let mut data = vec![0xff; 4096];
        data[..chunk.len()].copy_from_slice(chunk);
        let mut packet = words(&[4096, index as u32, 0, 0]);
        packet.extend_from_slice(&data);
        serial
            .checked(3, &packet, checksum(&data), 0, 10000.0)
            .await?;
        status(
            tr("Écriture du firmware…", "Writing firmware…"),
            Some((index + 1) as f64 / blocks as f64 * 95.0),
        );
    }
    status(
        tr(
            "Vérification du firmware écrit…",
            "Verifying written firmware…",
        ),
        Some(97.0),
    );
    let digest = serial
        .checked(0x13, &words(&[0, image.len() as u32, 0, 0]), 0, 16, 60000.0)
        .await?;
    let expected = Md5::digest(&image);
    if digest.as_slice() != expected.as_slice() {
        return Err(
            "Flash verification failed. Keep the board connected and retry installation.".into(),
        );
    }
    serial.checked(4, &words(&[1]), 0, 0, 3000.0).await?;
    let _ = serial.signals(false, true).await;
    sleep(100).await;
    let _ = serial.signals(false, false).await;
    status(
        tr(
            "Installation vérifiée. La carte redémarre.",
            "Installation verified. The board is restarting.",
        ),
        Some(100.0),
    );
    Ok(())
}
fn selected() {
    let board = value("board");
    attr("installer", "manifest", &format!("manifest-{board}.json"));
    if let Some(button) = get("installer") {
        set(
            &button,
            "manifest",
            &format!("manifest-{board}.json").into(),
        );
    }
    store("board", &board);
}
pub fn init() {
    if get("installer").is_none() {
        return;
    }
    let boards = ui::boards();
    html(
        "board",
        &boards
            .iter()
            .map(|b| {
                format!(
                    "<option value=\"{}\">{}</option>",
                    escape(&string(&b["env"])),
                    escape(&string(&b["name"]))
                )
            })
            .collect::<String>(),
    );
    let board = current_url()
        .search_params()
        .get("board")
        .filter(|env| boards.iter().any(|b| b["env"] == *env))
        .or_else(|| read_storage("board").filter(|env| boards.iter().any(|b| b["env"] == *env)));
    if let Some(board) = board {
        set_value("board", &board);
    }
    selected();
    on_id("board", "change", |_| selected());
    let serial = property(&window().navigator(), "serial");
    let supported = !serial.is_undefined() && !serial.is_null();
    let secure = window().is_secure_context();
    for el in all("#installer [slot=unsupported]") {
        let _ = el.toggle_attribute_with_force("hidden", supported || !secure);
    }
    for el in all("#installer [slot=not-allowed]") {
        let _ = el.toggle_attribute_with_force("hidden", secure);
    }
    if let Some(button) = query("#installer [slot=activate]") {
        let _ = button.toggle_attribute_with_force("hidden", !secure || !supported);
        on(button.as_ref(), "click", move |_| {
            if let Some(old) = get("flash-dialog") {
                old.remove();
                prune_listeners();
            }
            let out=format!("<dialog id=\"flash-dialog\" class=\"card\" aria-labelledby=\"flash-title\"><h2 id=\"flash-title\">{}</h2><p>{}</p><p id=\"flash-status\" role=\"status\"></p><progress id=\"flash-progress\" max=\"100\" value=\"0\" hidden></progress><div class=\"buttons\"><button type=\"button\" id=\"flash-cancel\" class=\"btn ghost\">{}</button><button type=\"button\" id=\"flash-connect\" class=\"btn\">{}</button></div></dialog>",tr("Installation USB","USB installation"),tr("La première installation efface les réglages et statistiques de la carte sélectionnée.","A first installation erases the selected board’s settings and statistics."),tr("Fermer","Close"),tr("Connecter et installer","Connect and install"));
            if let Some(body) = document().body() {
                let _ = body.insert_adjacent_html("beforeend", &out);
            }
            if let Some(dialog) = get("flash-dialog") {
                let _ = call(&dialog, "showModal", &[]);
            }
            click("flash-cancel", || {
                if let Some(dialog) = get("flash-dialog") {
                    let _ = call(&dialog, "close", &[]);
                }
            });
            click("flash-connect", || {
                let serial = property(&window().navigator(), "serial");
                let request = call(&serial, "requestPort", &[]);
                disabled("flash-connect", true);
                disabled("flash-cancel", true);
                let board = value("board");
                run(async move {
                    let result = match request {
                        Ok(request) => {
                            match JsFuture::from(js_sys::Promise::resolve(&request)).await {
                                Ok(port) => match Serial::open(port).await {
                                    Ok(serial) => {
                                        let result = install(&serial, &board).await;
                                        serial.close().await;
                                        result
                                    }
                                    Err(e) => Err(e),
                                },
                                Err(_) => {
                                    Err(tr("Aucun port sélectionné.", "No port selected.").into())
                                }
                            }
                        }
                        Err(_) => {
                            Err(tr("Port série indisponible.", "Serial port unavailable.").into())
                        }
                    };
                    if let Err(error) = result {
                        status(&error, None);
                    }
                    disabled("flash-connect", false);
                    disabled("flash-cancel", false);
                });
            });
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_checksums_and_slip_are_exact() {
        assert_eq!(
            slip(&[0, 0xc0, 0xdb, 255]),
            vec![0xc0, 0, 0xdb, 0xdc, 0xdb, 0xdd, 255, 0xc0]
        );
        assert_eq!(checksum(&[1, 2, 3]), 0xef);
        assert_eq!(
            &packet(3, &[1, 2], 0xef)[1..9],
            &[0, 3, 2, 0, 0xef, 0, 0, 0]
        );
    }
    #[test]
    fn firmware_chip_must_match_before_erase() {
        let mut image = vec![0xff; 0x2000];
        image[0x1000] = 0xe9;
        image[0x100c] = 0;
        image[0x100d] = 0;
        assert!(image_chip(&image, "esp32").is_ok());
        assert!(image_chip(&image, "esp32s2").is_err());
        assert!(image_chip(&image, "esp32s3").is_err());
    }
}
