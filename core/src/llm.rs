//! Klien LLM multi-provider — port `src/shared/llm-client.ts` (jalur non-stream).
//! Provider: openai-compatible, groq, openai, gemini, anthropic, mock.
//! Role routing (chat/motion/sheet/assistant) + fallback/cooldown + persist ke
//! config (byte-compatible). Streaming (SSE) menyusul di batch assistant.

use std::path::Path;

use serde_json::{json, Value};

use crate::config;

const DEFAULT_TIMEOUT_S: u64 = 60;

/// Pesan chat sederhana.
#[derive(Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            role: v.get("role").and_then(|x| x.as_str())?.to_string(),
            content: v.get("content").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        })
    }
}

fn default_model(provider: &str) -> &'static str {
    match provider {
        "gemini" => "gemini-2.0-flash",
        "groq" => "llama-3.3-70b-versatile",
        "openai" => "gpt-4o-mini",
        "anthropic" => "claude-3-5-haiku-latest",
        _ => "",
    }
}

fn clean_key(k: &str) -> String {
    k.chars()
        .filter(|&c| {
            let u = c as u32;
            !(u <= 0x1F || u == 0x7F || u == 0xA0 || (0x200B..=0x200D).contains(&u) || u == 0xFEFF)
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Error LLM dengan status HTTP (untuk classify).
#[derive(Debug)]
pub struct LlmError {
    pub status: u16,
    pub message: String,
}

/// Klasifikasi error → (fallback?, cooldown_ms). Padanan ERROR_RULES/classifyError.
pub fn classify_error(status: u16, text: &str) -> (bool, u64) {
    let lower = text.to_lowercase();
    let text_rules: &[(&str, u64)] = &[
        ("no credentials", 120_000),
        ("request not allowed", 5000),
        ("improperly formed request", 120_000),
        ("rate limit", 0),
        ("too many requests", 0),
        ("quota exceeded", 0),
        ("capacity", 0),
        ("overloaded", 0),
    ];
    for (t, cd) in text_rules {
        if lower.contains(t) {
            return (true, if *cd == 0 { 30_000 } else { *cd });
        }
    }
    let cd = match status {
        401 | 402 | 403 | 404 => 120_000,
        429 => 30_000,
        _ => 30_000,
    };
    (true, cd)
}

fn build_chat_messages(messages: &[ChatMessage], system: &str) -> Vec<Value> {
    let mut out = Vec::new();
    if !system.is_empty() {
        out.push(json!({ "role": "system", "content": system }));
    }
    for m in messages {
        if m.role == "system" {
            continue;
        }
        let role = if m.role == "user" { "user" } else { "assistant" };
        out.push(json!({ "role": role, "content": m.content }));
    }
    out
}

/// Panggil satu koneksi LLM (non-stream). Return teks balasan atau LlmError.
pub async fn call_llm(conn: &Value, messages: &[ChatMessage], client_system: &str) -> Result<String, LlmError> {
    let provider = conn.get("provider").and_then(|v| v.as_str()).unwrap_or("openai-compatible").to_lowercase();
    let api_key = clean_key(conn.get("apiKey").and_then(|v| v.as_str()).unwrap_or(""));
    let model = {
        let m = conn.get("model").and_then(|v| v.as_str()).unwrap_or("");
        if m.is_empty() { default_model(&provider).to_string() } else { m.to_string() }
    };
    let temp = conn.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.8);
    let max_t = conn.get("maxTokens").and_then(|v| v.as_u64()).unwrap_or(2048);
    let sys = {
        let sp = conn.get("systemPrompt").and_then(|v| v.as_str()).unwrap_or("");
        [sp, client_system].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("\n\n")
    };

    if provider == "mock" {
        let last = messages.iter().rev().find(|m| m.role == "user").map(|m| m.content.clone()).unwrap_or_default();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        return Ok(format!(
            "Halo! Kamu bilang: \"{last}\". (Mode mock — isi apiKey di config.json untuk LLM sungguhan.)"
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_S))
        .build()
        .map_err(|e| LlmError { status: 0, message: e.to_string() })?;

    if provider == "openai-compatible" || provider == "groq" || provider == "openai" {
        let base = match provider.as_str() {
            "groq" => "https://api.groq.com/openai/v1".to_string(),
            "openai" => "https://api.openai.com/v1".to_string(),
            _ => {
                let b = conn.get("baseUrl").and_then(|v| v.as_str()).unwrap_or("").trim_end_matches('/').to_string();
                if b.is_empty() {
                    return Err(LlmError { status: 0, message: "baseUrl belum diisi untuk openai-compatible".into() });
                }
                b
            }
        };
        let body = json!({
            "model": model,
            "messages": build_chat_messages(messages, &sys),
            "temperature": temp,
            "max_tokens": max_t,
            "stream": false
        });
        let resp = client
            .post(format!("{base}/chat/completions"))
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError { status: 0, message: e.to_string() })?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        if status >= 400 {
            return Err(LlmError { status, message: text.chars().take(300).collect() });
        }
        let j: Value = serde_json::from_str(&text).map_err(|_| LlmError { status, message: format!("respon bukan JSON: {}", text.chars().take(200).collect::<String>()) })?;
        let content = j.pointer("/choices/0/message/content").and_then(|v| v.as_str()).unwrap_or("");
        if content.is_empty() {
            return Err(LlmError { status, message: format!("{provider} kosong: {}", text.chars().take(200).collect::<String>()) });
        }
        return Ok(content.trim().to_string());
    }

    if provider == "gemini" {
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}");
        let contents: Vec<Value> = messages.iter().filter(|m| m.role != "system").map(|m| {
            let role = if m.role == "user" { "user" } else { "model" };
            json!({ "role": role, "parts": [{ "text": m.content }] })
        }).collect();
        let mut body = json!({
            "contents": contents,
            "generationConfig": { "temperature": temp, "maxOutputTokens": max_t, "candidateCount": 1 }
        });
        if !sys.is_empty() {
            body["systemInstruction"] = json!({ "parts": [{ "text": sys }] });
        }
        let resp = client.post(url).json(&body).send().await.map_err(|e| LlmError { status: 0, message: e.to_string() })?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        if status >= 400 {
            return Err(LlmError { status, message: text.chars().take(300).collect() });
        }
        let j: Value = serde_json::from_str(&text).map_err(|_| LlmError { status, message: "respon bukan JSON".into() })?;
        let parts = j.pointer("/candidates/0/content/parts").and_then(|v| v.as_array());
        let content: String = parts.map(|arr| arr.iter().filter_map(|p| p.get("text").and_then(|t| t.as_str())).collect::<String>()).unwrap_or_default();
        if content.is_empty() {
            return Err(LlmError { status, message: "Gemini kosong".into() });
        }
        return Ok(content.trim().to_string());
    }

    if provider == "anthropic" {
        let msgs: Vec<Value> = messages.iter().filter(|m| m.role != "system").map(|m| json!({ "role": m.role, "content": m.content })).collect();
        let mut body = json!({ "model": model, "messages": msgs, "max_tokens": max_t.min(4096), "temperature": temp });
        if !sys.is_empty() {
            body["system"] = json!(sys);
        }
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError { status: 0, message: e.to_string() })?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        if status >= 400 {
            return Err(LlmError { status, message: text.chars().take(300).collect() });
        }
        let j: Value = serde_json::from_str(&text).map_err(|_| LlmError { status, message: "respon bukan JSON".into() })?;
        let content: String = j.get("content").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|p| p.get("text").and_then(|t| t.as_str())).collect()).unwrap_or_default();
        if content.is_empty() {
            return Err(LlmError { status, message: "Anthropic kosong".into() });
        }
        return Ok(content.trim().to_string());
    }

    Err(LlmError { status: 0, message: format!("provider tidak dikenal: {provider}") })
}

/// Streaming: kirim delta teks lewat `tx` selagi mengalir, kembalikan teks
/// penuh. Wire-format OpenAI (openai-compatible/groq/openai) benar-benar
/// mengalir; provider lain (gemini/anthropic/mock) → satu delta utuh.
/// Padanan callLLMStream. Timeout senyap 60s (reset tiap chunk) via reqwest.
pub async fn call_llm_stream(
    conn: &Value,
    messages: &[ChatMessage],
    client_system: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<String>,
) -> Result<String, LlmError> {
    use futures_util::StreamExt;

    let provider = conn.get("provider").and_then(|v| v.as_str()).unwrap_or("openai-compatible").to_lowercase();
    if provider != "openai-compatible" && provider != "groq" && provider != "openai" {
        // provider tanpa jalur stream → satu delta.
        let full = call_llm(conn, messages, client_system).await?;
        let _ = tx.send(full.clone());
        return Ok(full);
    }
    let api_key = clean_key(conn.get("apiKey").and_then(|v| v.as_str()).unwrap_or(""));
    let model = {
        let m = conn.get("model").and_then(|v| v.as_str()).unwrap_or("");
        if m.is_empty() { default_model(&provider).to_string() } else { m.to_string() }
    };
    let temp = conn.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.8);
    let max_t = conn.get("maxTokens").and_then(|v| v.as_u64()).unwrap_or(2048);
    let sys = {
        let sp = conn.get("systemPrompt").and_then(|v| v.as_str()).unwrap_or("");
        [sp, client_system].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("\n\n")
    };
    let base = match provider.as_str() {
        "groq" => "https://api.groq.com/openai/v1".to_string(),
        "openai" => "https://api.openai.com/v1".to_string(),
        _ => {
            let b = conn.get("baseUrl").and_then(|v| v.as_str()).unwrap_or("").trim_end_matches('/').to_string();
            if b.is_empty() {
                return Err(LlmError { status: 0, message: "baseUrl belum diisi untuk openai-compatible".into() });
            }
            b
        }
    };
    let body = json!({
        "model": model,
        "messages": build_chat_messages(messages, &sys),
        "temperature": temp,
        "max_tokens": max_t,
        "stream": true
    });
    // read_timeout = timeout SENYAP per-chunk (bukan total) — reasoning panjang
    // tak dibunuh, diam 60s dibunuh.
    let client = reqwest::Client::builder()
        .read_timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_S))
        .build()
        .map_err(|e| LlmError { status: 0, message: e.to_string() })?;
    let resp = client
        .post(format!("{base}/chat/completions"))
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| LlmError { status: 0, message: e.to_string() })?;
    let status = resp.status().as_u16();
    if status >= 400 {
        let t = resp.text().await.unwrap_or_default();
        return Err(LlmError { status, message: t.chars().take(200).collect() });
    }

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut raw = String::new();
    let mut full = String::new();
    let mut handle_line = |line: &str, full: &mut String| {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("data:") {
            let payload = rest.trim();
            if payload.is_empty() || payload == "[DONE]" {
                return;
            }
            if let Ok(obj) = serde_json::from_str::<Value>(payload) {
                let piece = obj
                    .pointer("/choices/0/delta/content")
                    .or_else(|| obj.pointer("/choices/0/message/content"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !piece.is_empty() {
                    full.push_str(piece);
                    let _ = tx.send(piece.to_string());
                }
            }
        }
    };
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| LlmError { status: 0, message: e.to_string() })?;
        let s = String::from_utf8_lossy(&bytes);
        buf.push_str(&s);
        raw.push_str(&s);
        while let Some(idx) = buf.find('\n') {
            let line: String = buf[..idx].trim_end_matches('\r').to_string();
            buf = buf[idx + 1..].to_string();
            handle_line(&line, &mut full);
        }
    }
    if !buf.trim().is_empty() {
        handle_line(&buf.clone(), &mut full);
    }
    // relay aneh: minta stream, balas satu JSON utuh non-SSE.
    if full.trim().is_empty() {
        if let Ok(j) = crate::jsonx::extract_json(&raw) {
            let text = j
                .pointer("/choices/0/message/content")
                .or_else(|| j.pointer("/choices/0/delta/content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !text.is_empty() {
                full.push_str(text);
                let _ = tx.send(text.to_string());
            }
        }
    }
    if full.trim().is_empty() {
        return Err(LlmError { status, message: format!("{provider} stream kosong") });
    }
    Ok(full.trim().to_string())
}

/// True bila koneksi melayani role (roles kosong = wildcard). Padanan connHasRole.
pub fn conn_has_role(conn: &Value, role: &str) -> bool {
    let roles = config::normalize_roles(&conn.get("roles").cloned().unwrap_or(Value::Null));
    roles.is_empty() || roles.iter().any(|r| r == role)
}

fn enabled(conn: &Value) -> bool {
    conn.get("enabled").and_then(|v| v.as_bool()) != Some(false)
}

/// Urutan kandidat untuk role: eksplisit dulu, lalu wildcard. Kosong = pakai
/// default (semua). Padanan orderForRole (mengembalikan indeks ke `conns`).
pub fn order_for_role(role: &str, conns: &[Value]) -> Vec<usize> {
    let usable: Vec<usize> = (0..conns.len()).filter(|&i| enabled(&conns[i])).collect();
    let matching: Vec<usize> = usable.iter().cloned().filter(|&i| conn_has_role(&conns[i], role)).collect();
    if matching.is_empty() {
        return Vec::new();
    }
    let explicit: Vec<usize> = usable
        .iter()
        .cloned()
        .filter(|&i| config::normalize_roles(&conns[i].get("roles").cloned().unwrap_or(Value::Null)).iter().any(|r| r == role))
        .collect();
    let mut out = explicit.clone();
    for i in matching {
        if !explicit.contains(&i) {
            out.push(i);
        }
    }
    out
}

/// Hasil pemanggilan LLM: teks + id koneksi terpakai.
pub struct LlmOk {
    pub reply: String,
    pub used: String,
}

/// llmForRole + llmWithFallback digabung: pilih kandidat per role, coba
/// berurutan (skip rate-limited), update status + persist ke config.
pub async fn llm_for_role(
    config_path: &Path,
    role: &str,
    messages: &[ChatMessage],
    client_system: &str,
) -> Result<LlmOk, (u16, String)> {
    let cfg = config::load(config_path);
    let mut conns: Vec<Value> = cfg.get("connections").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let active_id = cfg.get("activeId").and_then(|v| v.as_str()).map(String::from);

    if conns.iter().all(|c| !enabled(c)) {
        return Err((400, "Semua connection dinonaktifkan — aktifkan di panel ⚙️.".into()));
    }

    // urutan: order_for_role bila ada; else active dulu lalu sisanya.
    let mut order = order_for_role(role, &conns);
    if order.is_empty() {
        let mut o: Vec<usize> = Vec::new();
        if let Some(aid) = &active_id {
            if let Some(i) = conns.iter().position(|c| c.get("id").and_then(|v| v.as_str()) == Some(aid.as_str()) && enabled(c)) {
                o.push(i);
            }
        }
        for i in 0..conns.len() {
            if enabled(&conns[i]) && !o.contains(&i) {
                o.push(i);
            }
        }
        order = o;
    }

    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let mut last_err = String::from("Semua connection gagal");
    for &i in &order {
        // skip bila masih rate-limited.
        if let Some(until) = conns[i].get("rateLimitedUntil").and_then(|v| v.as_str()) {
            if parse_iso_ms(until).map(|t| t > now_ms).unwrap_or(false) {
                continue;
            }
        }
        match call_llm(&conns[i], messages, client_system).await {
            Ok(reply) => {
                let id = conns[i].get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if let Some(o) = conns[i].as_object_mut() {
                    o.insert("testStatus".into(), json!("success"));
                    o.insert("lastError".into(), json!(""));
                    o.insert("rateLimitedUntil".into(), Value::Null);
                }
                let _ = config::save_connections(config_path, conns, json!(active_id));
                return Ok(LlmOk { reply, used: id });
            }
            Err(e) => {
                let (fallback, cooldown) = classify_error(e.status, &e.message);
                last_err = format!("LLM error: {}", e.message);
                if let Some(o) = conns[i].as_object_mut() {
                    o.insert("testStatus".into(), json!("error"));
                    o.insert("lastError".into(), json!(e.message));
                    if fallback {
                        o.insert("rateLimitedUntil".into(), json!(iso_from_ms(now_ms + cooldown as u128)));
                    }
                }
                // lanjut ke kandidat berikutnya bila fallback.
            }
        }
    }
    let _ = config::save_connections(config_path, conns, json!(active_id));
    Err((502, last_err))
}

fn parse_iso_ms(s: &str) -> Option<u128> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis().max(0) as u128)
}

/// ISO8601 UTC dari epoch ms (padanan new Date(ms).toISOString()).
fn iso_from_ms(ms: u128) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_sesuai_rules() {
        assert_eq!(classify_error(429, "").0, true);
        assert_eq!(classify_error(401, "").1, 120_000);
        assert_eq!(classify_error(200, "rate limit hit").1, 30_000);
        assert_eq!(classify_error(500, "overloaded").1, 30_000);
    }

    #[test]
    fn role_routing_eksplisit_dulu() {
        let conns = vec![
            json!({ "id": "a", "roles": [] }),                 // wildcard
            json!({ "id": "b", "roles": ["chat"] }),           // eksplisit chat
            json!({ "id": "c", "roles": ["motion"] }),         // bukan chat
        ];
        let order = order_for_role("chat", &conns);
        // eksplisit (b) dulu, lalu wildcard (a); c tak masuk
        assert_eq!(order, vec![1, 0]);
        // role tanpa penanda → kosong (pakai default di pemanggil)
        assert_eq!(order_for_role("sheet", &conns), vec![0]); // a wildcard cocok
    }

    #[tokio::test]
    async fn mock_provider_balas() {
        let conn = json!({ "id": "m", "provider": "mock" });
        let msgs = vec![ChatMessage { role: "user".into(), content: "tes".into() }];
        let r = call_llm(&conn, &msgs, "").await.unwrap();
        assert!(r.contains("tes"));
        assert!(r.contains("mock"));
    }

    #[tokio::test]
    async fn llm_for_role_pakai_mock() {
        let dir = std::env::temp_dir().join(format!("l2dllm-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("config.json");
        std::fs::write(&f, r#"{"activeId":"m","connections":[{"id":"m","provider":"mock"}]}"#).unwrap();
        let msgs = vec![ChatMessage { role: "user".into(), content: "halo".into() }];
        let ok = llm_for_role(&f, "chat", &msgs, "").await.unwrap();
        assert_eq!(ok.used, "m");
        assert!(ok.reply.contains("halo"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn now() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
    }
}
