//! live2d-engine — sidecar inferensi native untuk Live2D Agent.
//!
//! HTTP loopback kecil. Endpoint:
//!   GET  /health           → {status, tts, stt, sampleRate}
//!   GET  /voices           → {voices:[...]}   (nama voice style TTS)
//!   POST /tts  {text, voice?, lang?, speed?, steps?}  → audio/wav
//!   POST /stt  (body WAV, header X-Lang opsional)      → {text}
//!
//! Argumen: live2d-engine --port <p> --tts-model <dir> [--stt-model <file.bin>]
//! TTS & STT dimuat malas (lazy) saat endpoint pertama dipakai, supaya start cepat
//! dan model yang tak dipakai tak perlu ada.

mod tts;
mod text;
mod wav;
#[cfg(feature = "stt")]
mod stt;

use std::path::PathBuf;
use std::sync::Mutex;

use tiny_http::{Header, Method, Response, Server};

struct Args {
    port: u16,
    tts_model: Option<PathBuf>,
    #[allow(dead_code)]
    stt_model: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut port = 8320u16;
    let mut tts_model = None;
    let mut stt_model = None;
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--port" => {
                if let Some(v) = argv.get(i + 1) {
                    port = v.parse().unwrap_or(8320);
                    i += 1;
                }
            }
            "--tts-model" => {
                if let Some(v) = argv.get(i + 1) {
                    tts_model = Some(PathBuf::from(v));
                    i += 1;
                }
            }
            "--stt-model" => {
                if let Some(v) = argv.get(i + 1) {
                    stt_model = Some(PathBuf::from(v));
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    Args { port, tts_model, stt_model }
}

fn json_response(body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut r = Response::from_string(body);
    r.add_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    r
}

fn main() {
    let args = parse_args();
    let addr = format!("127.0.0.1:{}", args.port);
    let server = match Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[engine] gagal bind {addr}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("[engine] siap di http://{addr}");

    // Lazy-load: dimuat saat endpoint pertama dipakai.
    let tts_model = args.tts_model.clone();
    let engine: Mutex<Option<tts::SuperTonic>> = Mutex::new(None);
    #[cfg(feature = "stt")]
    let stt_model = args.stt_model.clone();
    #[cfg(feature = "stt")]
    let whisper: Mutex<Option<stt::Whisper>> = Mutex::new(None);

    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();
        let path = url.split('?').next().unwrap_or("").to_string();

        match (&method, path.as_str()) {
            (Method::Get, "/health") => {
                let stt_avail = cfg!(feature = "stt");
                let body = format!(
                    "{{\"status\":\"ok\",\"tts\":{},\"stt\":{},\"sampleRate\":44100}}",
                    tts_model.is_some(),
                    stt_avail
                );
                let _ = request.respond(json_response(body));
            }

            (Method::Get, "/voices") => {
                let voices = tts_model
                    .as_ref()
                    .map(|d| list_voices(d))
                    .unwrap_or_default();
                let items = voices
                    .iter()
                    .map(|v| format!("\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(",");
                let _ = request.respond(json_response(format!("{{\"voices\":[{items}]}}")));
            }

            (Method::Post, "/tts") => {
                let mut body = String::new();
                if request.as_reader().read_to_string(&mut body).is_err() {
                    let _ = request.respond(json_response(
                        "{\"error\":\"gagal baca body\"}".into(),
                    ));
                    continue;
                }
                let req: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                let out = handle_tts(&engine, &tts_model, &req);
                match out {
                    Ok(wav_bytes) => {
                        let mut r = Response::from_data(wav_bytes);
                        r.add_header(
                            Header::from_bytes(&b"Content-Type"[..], &b"audio/wav"[..]).unwrap(),
                        );
                        let _ = request.respond(r);
                    }
                    Err(e) => {
                        let _ = request.respond(json_response(format!(
                            "{{\"error\":{}}}",
                            serde_json::to_string(&e).unwrap_or_else(|_| "\"error\"".into())
                        )));
                    }
                }
            }

            (Method::Post, "/stt") => {
                #[cfg(feature = "stt")]
                {
                    let lang = request
                        .headers()
                        .iter()
                        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("X-Lang"))
                        .map(|h| h.value.as_str().to_string());
                    let mut buf = Vec::new();
                    if request.as_reader().read_to_end(&mut buf).is_err() {
                        let _ = request
                            .respond(json_response("{\"error\":\"gagal baca audio\"}".into()));
                        continue;
                    }
                    let out = handle_stt(&whisper, &stt_model, &buf, lang.as_deref());
                    match out {
                        Ok(text) => {
                            let _ = request.respond(json_response(format!(
                                "{{\"text\":{}}}",
                                serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into())
                            )));
                        }
                        Err(e) => {
                            let _ = request.respond(json_response(format!(
                                "{{\"error\":{}}}",
                                serde_json::to_string(&e).unwrap_or_else(|_| "\"error\"".into())
                            )));
                        }
                    }
                }
                #[cfg(not(feature = "stt"))]
                {
                    let r = json_response(
                        "{\"error\":\"STT native tidak dikompilasi (feature stt mati)\"}".into(),
                    )
                    .with_status_code(503);
                    let _ = request.respond(r);
                }
            }

            _ => {
                let r = json_response("{\"error\":\"not found\"}".into()).with_status_code(404);
                let _ = request.respond(r);
            }
        }
    }
}

fn list_voices(model_dir: &std::path::Path) -> Vec<String> {
    let dir = model_dir.join("voice_styles");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

fn handle_tts(
    engine: &Mutex<Option<tts::SuperTonic>>,
    tts_model: &Option<PathBuf>,
    req: &serde_json::Value,
) -> Result<Vec<u8>, String> {
    let model_dir = tts_model
        .as_ref()
        .ok_or("model TTS tidak dikonfigurasi (--tts-model)")?;
    let text = req["text"].as_str().unwrap_or("").trim();
    if text.is_empty() {
        return Err("teks kosong".into());
    }
    let voice = req["voice"].as_str().unwrap_or("M1");
    let lang = req["lang"].as_str().filter(|s| !s.is_empty());
    let speed = req["speed"].as_f64().unwrap_or(1.05) as f32;
    let steps = req["steps"].as_i64().unwrap_or(8);

    let mut guard = engine.lock().map_err(|_| "lock engine")?;
    if guard.is_none() {
        *guard = Some(tts::SuperTonic::load(model_dir)?);
    }
    let eng = guard.as_mut().unwrap();
    let style = tts::SuperTonic::load_style(model_dir, voice)?;
    let samples = eng.synthesize(text, &style, steps, speed, 0.3, lang)?;
    Ok(wav::encode_pcm16(&samples, eng.sample_rate()))
}

#[cfg(feature = "stt")]
fn handle_stt(
    whisper: &Mutex<Option<stt::Whisper>>,
    stt_model: &Option<PathBuf>,
    body: &[u8],
    lang: Option<&str>,
) -> Result<String, String> {
    let model_path = stt_model
        .as_ref()
        .ok_or("model STT tidak dikonfigurasi (--stt-model)")?;
    let (samples, sr) = wav::decode_pcm16(body)?;
    let mut guard = whisper.lock().map_err(|_| "lock whisper")?;
    if guard.is_none() {
        *guard = Some(stt::Whisper::load(model_path)?);
    }
    guard.as_ref().unwrap().transcribe(&samples, sr, lang)
}
