//! Push notifications through ntfy (https://ntfy.sh or a self-hosted
//! server), each also posted as JSON to the webhook URL when one is set.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde_json::json;

/// An HTTP(S) POST; returns the status code.
pub trait HttpClient: Send + Sync {
    fn post(&self, url: &str, headers: &[(&str, &str)], body: &[u8]) -> Result<u16, String>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub title: String,
    pub text: String,
    /// 1 (min) .. 5 (max).
    pub priority: u8,
    /// ntfy emoji tags ("tada,moneybag").
    pub tags: String,
    pub test_id: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub enabled: bool,
    /// 0 before the first delivery.
    pub id: u32,
    pub title: String,
    pub ntfy: String,
    pub webhook: String,
}

#[derive(Clone, Debug, Default)]
pub struct Targets {
    pub ntfy_server: String,
    pub ntfy_topic: String,
    pub webhook_url: String,
}

impl Targets {
    pub fn enabled(&self) -> bool {
        !self.ntfy_topic.is_empty() || !self.webhook_url.is_empty()
    }
}

const QUEUE: usize = 8;

pub struct Notifier {
    targets: Targets,
    queue: Mutex<VecDeque<Message>>,
    status: Mutex<Status>,
    tests: Mutex<(u32, VecDeque<Status>)>,
    log: Box<dyn Fn(&str) + Send + Sync>,
}

/// The JSON a webhook receives: Discord reads "content", Telegram's
/// sendMessage "text", anything else the other fields.
pub fn webhook_body(m: &Message) -> String {
    json!({
        "content": format!("**{}**\n{}", m.title, m.text),
        "text": format!("{}: {}", m.title, m.text),
        "title": m.title,
        "message": m.text,
        "priority": m.priority,
        "tags": m.tags,
    })
    .to_string()
}

fn outcome(r: Result<u16, String>) -> String {
    match r {
        Ok(code) if (200..300).contains(&code) => "OK".into(),
        Ok(code) => format!("HTTP {code}"),
        Err(e) => e.chars().take(23).collect(),
    }
}

impl Notifier {
    pub fn new(targets: Targets, log: impl Fn(&str) + Send + Sync + 'static) -> Self {
        let enabled = targets.enabled();
        Notifier {
            targets,
            queue: Mutex::new(VecDeque::new()),
            status: Mutex::new(Status { enabled, ntfy: "off".into(), webhook: "off".into(), ..Default::default() }),
            tests: Mutex::new((0, VecDeque::new())),
            log: Box::new(log),
        }
    }

    /// Queues a notification (dropped when off or full).
    pub fn notify(&self, title: &str, text: &str, priority: u8, tags: &str) -> bool {
        self.enqueue(Message {
            title: title.chars().take(63).collect(),
            text: text.chars().take(191).collect(),
            priority: priority.clamp(1, 5),
            tags: tags.chars().take(31).collect(),
            test_id: 0,
        })
    }

    fn enqueue(&self, m: Message) -> bool {
        if !self.targets.enabled() {
            return false;
        }
        let mut q = self.queue.lock().unwrap();
        if q.len() >= QUEUE {
            (self.log)(&format!("[notify] queue full, dropped: {}", m.title));
            return false;
        }
        q.push_back(m);
        true
    }

    /// Queues a test; returns its id, 0 when unavailable.
    pub fn test(&self) -> u32 {
        let id = {
            let mut t = self.tests.lock().unwrap();
            t.0 += 1;
            t.0
        };
        let m = Message {
            title: "BDLMiner test".into(),
            text: "Notifications work.".into(),
            priority: 3,
            tags: "white_check_mark".into(),
            test_id: id,
        };
        if self.enqueue(m) {
            id
        } else {
            0
        }
    }

    /// A completed test (0 = latest), as JSON.
    pub fn result(&self, id: u32) -> serde_json::Value {
        let t = self.tests.lock().unwrap();
        let found = t.1.iter().rev().find(|s| id == 0 || s.id == id).cloned().unwrap_or_default();
        json!({"id": found.id, "title": found.title, "ntfy": found.ntfy, "webhook": found.webhook})
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    /// Delivers the queued notifications; call from a task that may block.
    pub fn poll(&self, http: &dyn HttpClient) {
        loop {
            let Some(m) = self.queue.lock().unwrap().pop_front() else { return };
            let t = &self.targets;
            let ntfy = if t.ntfy_topic.is_empty() {
                "off".into()
            } else {
                let url = format!("https://{}/{}", t.ntfy_server, t.ntfy_topic);
                let priority = m.priority.to_string();
                outcome(http.post(
                    &url,
                    &[("Title", &m.title), ("Priority", &priority), ("Tags", &m.tags)],
                    m.text.as_bytes(),
                ))
            };
            let webhook = if t.webhook_url.is_empty() {
                "off".into()
            } else {
                outcome(http.post(&t.webhook_url, &[("Content-Type", "application/json")], webhook_body(&m).as_bytes()))
            };
            (self.log)(&format!("[notify] {}: ntfy {ntfy}, webhook {webhook}", m.title));
            let mut status = self.status.lock().unwrap();
            status.id += 1;
            status.title = m.title.clone();
            status.ntfy = ntfy.clone();
            status.webhook = webhook.clone();
            drop(status);
            if m.test_id != 0 {
                let mut tests = self.tests.lock().unwrap();
                if tests.1.len() == 8 {
                    tests.1.pop_front();
                }
                tests.1.push_back(Status { enabled: true, id: m.test_id, title: m.title, ntfy, webhook });
            }
        }
    }
}
