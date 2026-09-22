//! browser/policy.rs — Kebijakan URL browser agent (port `browser/policy.ts`).
//! Agent tidak boleh diarahkan diam-diam ke file lokal, metadata cloud,
//! loopback, atau LAN. Origin privat hanya setelah persetujuan eksplisit.

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Mutex;

use url::Url;

#[derive(Debug, Default, Clone)]
pub struct UrlDecision {
    pub ok: bool,
    pub url: Option<String>,
    pub origin: Option<String>,
    pub private_network: bool,
    pub error: Option<String>,
}

fn err(msg: &str) -> UrlDecision {
    UrlDecision { ok: false, error: Some(msg.to_string()), ..Default::default() }
}

const DENIED_SCHEMES: &[&str] = &["file", "javascript", "data", "blob", "chrome", "devtools", "edge", "about"];

/// Klasifikasi alamat literal IPv4/IPv6 yang tak boleh diakses agent.
pub fn is_private_address(input: &str) -> bool {
    let ip = input.trim_matches(|c| c == '[' || c == ']').to_lowercase();
    let parsed: IpAddr = match ip.parse() {
        Ok(p) => p,
        Err(_) => return false,
    };
    match parsed {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            let (a, b) = (o[0], o[1]);
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || a == 0
                || (a == 100 && (64..=127).contains(&b)) // CGNAT
                || a >= 224 // multicast/reserved
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return true;
            }
            // IPv4-mapped → cek sbg IPv4.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_private_address(&v4.to_string());
            }
            let seg = v6.segments()[0];
            (seg & 0xffc0) == 0xfe80 // link-local fe80::/10
                || (seg & 0xfe00) == 0xfc00 // ULA fc00::/7
        }
    }
}

/// Normalisasi + validasi skema/kredensial. Tanpa DNS.
pub fn normalize_browser_url(raw: &str) -> UrlDecision {
    let text = raw.trim();
    if text.is_empty() {
        return err("URL kosong");
    }
    let lower = text.to_lowercase();
    if let Some(scheme) = lower.split(':').next() {
        if DENIED_SCHEMES.contains(&scheme) && lower.contains(':') {
            return err("scheme URL tidak diizinkan");
        }
    }
    let with_scheme = if lower.starts_with("http://") || lower.starts_with("https://") {
        text.to_string()
    } else {
        format!("https://{text}")
    };
    let mut u = match Url::parse(&with_scheme) {
        Ok(u) => u,
        Err(_) => return err("URL tidak valid"),
    };
    if u.scheme() != "http" && u.scheme() != "https" {
        return err("hanya URL http/https yang diizinkan");
    }
    let _ = u.set_username("");
    let _ = u.set_password(None);
    u.set_fragment(None);
    let host = u.host_str().unwrap_or("").to_string();
    let private_network = host == "localhost" || is_private_address(&host);
    let origin = u.origin().unicode_serialization();
    UrlDecision { ok: true, url: Some(u.to_string()), origin: Some(origin), private_network, error: None }
}

/// Resolve DNS dan cek SEMUA alamat hasilnya (cegah hostname → LAN).
pub async fn inspect_browser_url(raw: &str) -> UrlDecision {
    let base = normalize_browser_url(raw);
    if !base.ok {
        return base;
    }
    if base.private_network {
        return base;
    }
    let url_str = base.url.clone().unwrap_or_default();
    let host = Url::parse(&url_str).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_default();
    // resolve host:80 → cek tiap IP.
    match tokio::net::lookup_host(format!("{host}:80")).await {
        Ok(addrs) => {
            let mut any = false;
            for a in addrs {
                any = true;
                if is_private_address(&a.ip().to_string()) {
                    return UrlDecision { private_network: true, ..base };
                }
            }
            if !any {
                return err("hostname tidak dapat di-resolve");
            }
            base
        }
        Err(_) => err("hostname tidak dapat di-resolve"),
    }
}

/// Grant origin privat in-memory per sesi.
#[derive(Default)]
pub struct OriginGrants {
    origins: Mutex<HashSet<String>>,
}

impl OriginGrants {
    pub fn grant(&self, origin: &str) {
        if let Ok(mut g) = self.origins.lock() {
            g.insert(origin.to_string());
        }
    }
    pub fn revoke_all(&self) {
        if let Ok(mut g) = self.origins.lock() {
            g.clear();
        }
    }
    pub fn has(&self, origin: &str) -> bool {
        self.origins.lock().map(|g| g.contains(origin)).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_addrs() {
        assert!(is_private_address("127.0.0.1"));
        assert!(is_private_address("10.0.0.5"));
        assert!(is_private_address("192.168.1.1"));
        assert!(is_private_address("172.16.0.1"));
        assert!(is_private_address("169.254.1.1"));
        assert!(is_private_address("::1"));
        assert!(is_private_address("fe80::1"));
        assert!(is_private_address("fd00::1"));
        assert!(is_private_address("100.64.0.1"));
        assert!(!is_private_address("8.8.8.8"));
        assert!(!is_private_address("1.1.1.1"));
        assert!(!is_private_address("bukan-ip"));
    }

    #[test]
    fn normalize() {
        assert!(!normalize_browser_url("file:///etc/passwd").ok);
        assert!(!normalize_browser_url("javascript:alert(1)").ok);
        let d = normalize_browser_url("example.com");
        assert!(d.ok);
        assert_eq!(d.url.as_deref(), Some("https://example.com/"));
        assert!(!d.private_network);
        let loc = normalize_browser_url("http://localhost:8310");
        assert!(loc.ok && loc.private_network);
        let ip = normalize_browser_url("http://192.168.1.5");
        assert!(ip.ok && ip.private_network);
        // kredensial dibuang
        let cred = normalize_browser_url("https://user:pass@example.com/x#frag");
        assert!(cred.ok);
        assert!(!cred.url.as_deref().unwrap().contains("user"));
        assert!(!cred.url.as_deref().unwrap().contains("frag"));
    }

    #[test]
    fn grants() {
        let g = OriginGrants::default();
        assert!(!g.has("https://x.local"));
        g.grant("https://x.local");
        assert!(g.has("https://x.local"));
        g.revoke_all();
        assert!(!g.has("https://x.local"));
    }
}
