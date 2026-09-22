//! browser/snapshot.rs — Normalisasi AX tree → node ber-ref + store (port
//! `browser/snapshot.ts`). Halaman besar dibatasi agar tak membanjiri prompt.

use std::collections::HashMap;

use serde_json::{json, Value};

const REDACTED: &str = "[disembunyikan]";
const SENSITIVE_WORDS: &[&str] = &["password", "passcode", "pin", "otp", "secret", "token", "api key", "api_key", "api-key", "apikey", "kata sandi", "sandi"];

#[derive(Clone)]
pub struct AxNode {
    pub ref_id: String,
    pub role: String,
    pub name: String,
    pub value: Option<String>,
    pub description: Option<String>,
    pub backend_dom_node_id: Option<i64>,
    pub sensitive: bool,
}

#[derive(Clone)]
pub struct Snapshot {
    pub snapshot_id: String,
    pub url: String,
    pub title: String,
    pub created_at: i64,
    pub nodes: Vec<AxNode>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn opaque_id() -> String {
    // Cukup unik untuk in-memory (bukan token keamanan): nanos + counter base36.
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let n = C.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    crate::config::base36_pub((nanos ^ (n.wrapping_mul(0x9E3779B97F4A7C15))) as u128)
}

/// Ambil teks dari CDP AxValue {value}.
fn ax_text(v: &Value) -> String {
    match v.get("value") {
        Some(Value::String(s)) => s.split_whitespace().collect::<Vec<_>>().join(" "),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn is_sensitive(node: &Value) -> bool {
    let name = ax_text(node.get("name").unwrap_or(&Value::Null));
    let desc = ax_text(node.get("description").unwrap_or(&Value::Null));
    let label = format!("{name} {desc}").to_lowercase();
    if SENSITIVE_WORDS.iter().any(|w| label.contains(w)) {
        return true;
    }
    node.get("properties").and_then(|p| p.as_array()).map(|arr| {
        arr.iter().any(|prop| {
            let n = prop.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
            (n == "protected" || n == "password") && prop.get("value").and_then(|v| v.get("value")).and_then(|v| v.as_bool()) == Some(true)
        })
    }).unwrap_or(false)
}

/// Normalisasi node CDP getFullAXTree → Snapshot (maks 500 node).
pub fn normalize_ax_tree(raw_nodes: &[Value], url: &str, title: &str) -> Snapshot {
    let max_nodes = 500;
    let mut nodes: Vec<AxNode> = Vec::new();
    for raw in raw_nodes {
        if nodes.len() >= max_nodes {
            break;
        }
        if raw.get("ignored").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        let role = {
            let r = ax_text(raw.get("role").unwrap_or(&Value::Null));
            if r.is_empty() { "unknown".to_string() } else { r }
        };
        let name = ax_text(raw.get("name").unwrap_or(&Value::Null));
        let value = ax_text(raw.get("value").unwrap_or(&Value::Null));
        let description = ax_text(raw.get("description").unwrap_or(&Value::Null));
        if name.is_empty() && value.is_empty() && description.is_empty() && role == "generic" {
            continue;
        }
        let sensitive = is_sensitive(raw);
        nodes.push(AxNode {
            ref_id: format!("br_{}", opaque_id()),
            role,
            name,
            value: if value.is_empty() { None } else { Some(if sensitive { REDACTED.to_string() } else { value }) },
            description: if description.is_empty() { None } else { Some(description) },
            backend_dom_node_id: raw.get("backendDOMNodeId").and_then(|v| v.as_i64()),
            sensitive,
        });
    }
    Snapshot {
        snapshot_id: format!("bs_{}", opaque_id()),
        url: url.to_string(),
        title: title.to_string(),
        created_at: now_ms(),
        nodes,
    }
}

/// Kode error ref (padanan SnapshotReferenceError.code).
#[derive(Debug, PartialEq)]
pub enum RefErr {
    Unknown(String),
    Expired(String),
    Stale(String),
}
impl RefErr {
    pub fn message(&self) -> &str {
        match self {
            RefErr::Unknown(m) | RefErr::Expired(m) | RefErr::Stale(m) => m,
        }
    }
}

pub struct SnapshotStore {
    snapshots: Vec<Snapshot>, // urutan insert (untuk evict tertua)
    refs: HashMap<String, String>, // ref → snapshotId
    capacity: usize,
    ttl_ms: i64,
}

impl Default for SnapshotStore {
    fn default() -> Self {
        SnapshotStore { snapshots: Vec::new(), refs: HashMap::new(), capacity: 8, ttl_ms: 120_000 }
    }
}

impl SnapshotStore {
    fn prune(&mut self) {
        let cutoff = now_ms() - self.ttl_ms;
        let expired: Vec<String> = self.snapshots.iter().filter(|s| s.created_at <= cutoff).map(|s| s.snapshot_id.clone()).collect();
        for id in expired {
            self.remove(&id);
        }
    }
    fn remove(&mut self, id: &str) {
        if let Some(pos) = self.snapshots.iter().position(|s| s.snapshot_id == id) {
            let s = self.snapshots.remove(pos);
            for n in &s.nodes {
                self.refs.remove(&n.ref_id);
            }
        }
    }
    pub fn put(&mut self, snapshot: Snapshot) {
        self.prune();
        for n in &snapshot.nodes {
            self.refs.insert(n.ref_id.clone(), snapshot.snapshot_id.clone());
        }
        self.snapshots.push(snapshot);
        while self.snapshots.len() > self.capacity {
            let oldest = self.snapshots[0].snapshot_id.clone();
            self.remove(&oldest);
        }
    }
    pub fn get(&mut self, snapshot_id: &str) -> Result<Snapshot, RefErr> {
        self.prune();
        self.snapshots.iter().find(|s| s.snapshot_id == snapshot_id).cloned().ok_or_else(|| RefErr::Expired("snapshot sudah kedaluwarsa".into()))
    }
    /// Resolve ref → node (cek snapshotId + url bila diberikan).
    pub fn resolve(&mut self, ref_id: &str, expect_snapshot: Option<&str>, expect_url: Option<&str>) -> Result<AxNode, RefErr> {
        self.prune();
        let sid = self.refs.get(ref_id).cloned().ok_or_else(|| RefErr::Unknown("ref browser tidak dikenal atau kedaluwarsa".into()))?;
        let snap = self.snapshots.iter().find(|s| s.snapshot_id == sid).ok_or_else(|| RefErr::Expired("snapshot ref sudah kedaluwarsa".into()))?;
        if expect_snapshot.map(|e| e != sid).unwrap_or(false) || expect_url.map(|u| u != snap.url).unwrap_or(false) {
            return Err(RefErr::Stale("ref browser sudah stale setelah navigasi/inspect baru".into()));
        }
        snap.nodes.iter().find(|n| n.ref_id == ref_id).cloned().ok_or_else(|| RefErr::Unknown("ref browser tidak dikenal".into()))
    }
    pub fn clear(&mut self) {
        self.snapshots.clear();
        self.refs.clear();
    }
}

/// Format hasil inspect (ber-cursor) → JSON {snapshotId,url,title,text,nextCursor,count}.
pub fn format_inspect(snapshot: &Snapshot, cursor: usize, max_chars: usize) -> Value {
    let start = cursor.min(snapshot.nodes.len());
    let mut body = String::new();
    let mut index = start;
    while index < snapshot.nodes.len() {
        let n = &snapshot.nodes[index];
        let mut fields = vec![format!("[{}]", n.ref_id), n.role.clone(), n.name.clone()];
        if let Some(v) = &n.value {
            fields.push(format!("value={v}"));
        }
        if let Some(d) = &n.description {
            fields.push(format!("desc={d}"));
        }
        let line = fields.into_iter().filter(|f| !f.is_empty()).collect::<Vec<_>>().join(" | ") + "\n";
        if body.len() + line.len() > max_chars {
            if body.is_empty() && max_chars > 0 {
                body = line.chars().take(max_chars).collect();
                index += 1;
            }
            break;
        }
        body.push_str(&line);
        index += 1;
    }
    json!({
        "snapshotId": snapshot.snapshot_id,
        "url": snapshot.url,
        "title": snapshot.title,
        "text": body,
        "nextCursor": if index < snapshot.nodes.len() { json!(index) } else { Value::Null },
        "count": snapshot.nodes.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_dan_sensitive() {
        let raw = vec![
            json!({ "role": { "value": "button" }, "name": { "value": "Kirim" }, "backendDOMNodeId": 12 }),
            json!({ "role": { "value": "generic" }, "name": { "value": "" } }), // dibuang
            json!({ "ignored": true, "role": { "value": "x" } }),               // dibuang
            json!({ "role": { "value": "textbox" }, "name": { "value": "Password" }, "value": { "value": "rahasia" } }),
        ];
        let snap = normalize_ax_tree(&raw, "https://x.test/", "Judul");
        assert_eq!(snap.nodes.len(), 2);
        assert_eq!(snap.nodes[0].role, "button");
        assert_eq!(snap.nodes[0].backend_dom_node_id, Some(12));
        // node password → value diredaksi + sensitive
        assert!(snap.nodes[1].sensitive);
        assert_eq!(snap.nodes[1].value.as_deref(), Some(REDACTED));
    }

    #[test]
    fn store_put_resolve_stale() {
        let mut store = SnapshotStore::default();
        let raw = vec![json!({ "role": { "value": "link" }, "name": { "value": "Beranda" }, "backendDOMNodeId": 5 })];
        let snap = normalize_ax_tree(&raw, "https://x.test/", "X");
        let sid = snap.snapshot_id.clone();
        let ref_id = snap.nodes[0].ref_id.clone();
        store.put(snap);
        // resolve ok
        let node = store.resolve(&ref_id, Some(&sid), Some("https://x.test/")).unwrap();
        assert_eq!(node.backend_dom_node_id, Some(5));
        // snapshotId beda → stale
        assert!(matches!(store.resolve(&ref_id, Some("bs_lain"), None), Err(RefErr::Stale(_))));
        // ref tak dikenal → unknown
        assert!(matches!(store.resolve("br_nope", None, None), Err(RefErr::Unknown(_))));
    }

    #[test]
    fn inspect_cursor() {
        let raw: Vec<Value> = (0..3).map(|i| json!({ "role": { "value": "link" }, "name": { "value": format!("n{i}") }, "backendDOMNodeId": i })).collect();
        let snap = normalize_ax_tree(&raw, "u", "t");
        let out = format_inspect(&snap, 0, 12000);
        assert_eq!(out["count"], 3);
        assert!(out["nextCursor"].is_null());
        assert!(out["text"].as_str().unwrap().contains("n0"));
    }
}
