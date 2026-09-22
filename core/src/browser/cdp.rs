//! browser/cdp.rs — Transport CDP (Chrome DevTools Protocol) via WebSocket,
//! port `browser/cdp.ts`. Korelasi request/response by id; event diabaikan
//! (manager mem-poll state on-demand, tak butuh listener — lebih sederhana &
//! robust daripada infra event di TS).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

pub struct CdpClient {
    write: tokio::sync::Mutex<futures_util::stream::SplitSink<WsStream, Message>>,
    next_id: AtomicU64,
    pending: Pending,
    closed: Arc<AtomicBool>,
}

type WsStream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

impl CdpClient {
    /// Hubungkan ke webSocketDebuggerUrl target CDP.
    pub async fn connect(url: &str) -> Result<CdpClient, String> {
        let (ws, _resp) = tokio_tungstenite::connect_async(url).await.map_err(|e| format!("CDP gagal terhubung: {e}"))?;
        let (write, mut read) = ws.split();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));

        // Reader task: resolve response by id; event (tanpa id) diabaikan.
        let p2 = pending.clone();
        let c2 = closed.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(t)) => {
                        if let Ok(v) = serde_json::from_str::<Value>(t.as_str()) {
                            if let Some(id) = v.get("id").and_then(|x| x.as_u64()) {
                                let tx = p2.lock().unwrap().remove(&id);
                                if let Some(tx) = tx {
                                    if let Some(e) = v.get("error") {
                                        let m = e.get("message").and_then(|x| x.as_str()).unwrap_or("error CDP").to_string();
                                        let _ = tx.send(Err(format!("CDP {m}")));
                                    } else {
                                        let _ = tx.send(Ok(v.get("result").cloned().unwrap_or(json!({}))));
                                    }
                                }
                            }
                            // pesan event (method tanpa id) diabaikan.
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            // Koneksi tutup → gagalkan semua pending.
            c2.store(true, Ordering::SeqCst);
            let mut g = p2.lock().unwrap();
            for (_, tx) in g.drain() {
                let _ = tx.send(Err("CDP terputus".into()));
            }
        });

        Ok(CdpClient {
            write: tokio::sync::Mutex::new(write),
            next_id: AtomicU64::new(1),
            pending,
            closed,
        })
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Kirim perintah CDP, tunggu hasil (timeout ms). Return result JSON.
    pub async fn send(&self, method: &str, params: Value, timeout_ms: u64) -> Result<Value, String> {
        if self.is_closed() {
            return Err("CDP sudah ditutup".into());
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let payload = json!({ "id": id, "method": method, "params": params }).to_string();
        {
            let mut w = self.write.lock().await;
            if let Err(e) = w.send(Message::Text(payload.into())).await {
                self.pending.lock().unwrap().remove(&id);
                return Err(format!("CDP gagal mengirim: {e}"));
            }
        }
        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms.max(1)), rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(format!("CDP kanal tertutup: {method}")),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!("CDP timeout: {method}"))
            }
        }
    }

    /// Tutup koneksi (best-effort).
    pub async fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let mut w = self.write.lock().await;
        let _ = w.close().await;
    }
}
