//! Port `supertonic/core.py::UnicodeProcessor` ke Rust (1:1).
//!
//! Pipeline preprocess (urutan identik Python):
//!   1. NFKD normalize
//!   2. buang emoji
//!   3. normalisasi simbol (dash/quote → ASCII)
//!   4. buang simbol dekoratif (♥☆♡©\)
//!   5. expand singkatan (@ → " at ", e.g., → "for example, ", i.e., → "that is, ")
//!   6. rapikan spasi sekitar tanda baca
//!   7. buang quote ganda
//!   8. bersihkan whitespace
//!   9. tambah "." bila tak diakhiri tanda baca
//!  10. bungkus <lang>...</lang> (bila lang != None)
//!
//! Lalu tiap char → codepoint Unicode → indexer[codepoint] = id model.
//! Codepoint di luar jangkauan indexer / bernilai -1 → 0 (tak didukung; Python
//! memvalidasi lebih dulu, kita degrade anggun).

use unicode_normalization::UnicodeNormalization;

pub struct UnicodeProcessor {
    /// indexer[codepoint] = id model (-1 = tak didukung).
    indexer: Vec<i64>,
}

impl UnicodeProcessor {
    /// Muat dari isi unicode_indexer.json (array angka).
    pub fn from_indexer_json(text: &str) -> Result<Self, String> {
        let arr: Vec<i64> =
            serde_json::from_str(text).map_err(|e| format!("unicode_indexer.json rusak: {e}"))?;
        if arr.is_empty() {
            return Err("unicode_indexer.json kosong".into());
        }
        Ok(Self { indexer: arr })
    }

    /// Preprocess + encode ke id model. `lang` mis. Some("id"); None = tanpa token bahasa.
    pub fn encode(&self, text: &str, lang: Option<&str>) -> Vec<i64> {
        let pre = self.preprocess(text, lang);
        pre.chars()
            .map(|c| {
                let cp = c as usize;
                let id = self.indexer.get(cp).copied().unwrap_or(-1);
                if id < 0 {
                    0
                } else {
                    id
                }
            })
            .collect()
    }

    fn preprocess(&self, text: &str, lang: Option<&str>) -> String {
        // 1. NFKD
        let mut t: String = text.nfkd().collect();
        // 2. emoji
        t = remove_emojis(&t);
        // 3. normalisasi simbol
        t = normalize_symbols(&t);
        // 4. simbol dekoratif
        t = t.chars().filter(|c| !matches!(c, '♥' | '☆' | '♡' | '©' | '\\')).collect();
        // 5. singkatan
        t = t.replace("e.g.,", "for example, ");
        t = t.replace("i.e.,", "that is, ");
        t = t.replace('@', " at ");
        // 6. spasi sekitar tanda baca
        for (from, to) in [
            (" ,", ","),
            (" .", "."),
            (" !", "!"),
            (" ?", "?"),
            (" ;", ";"),
            (" :", ":"),
            (" '", "'"),
        ] {
            t = t.replace(from, to);
        }
        // 7. quote ganda berturut → satu
        t = dedup_quotes(&t);
        // 8. whitespace
        t = collapse_whitespace(&t);
        // 9. tambah period bila perlu
        if !ends_with_punct(&t) {
            t.push('.');
        }
        // 10. token bahasa
        if let Some(l) = lang {
            t = format!("<{l}>{t}</{l}>");
        }
        t
    }
}

/// Buang codepoint di rentang emoji umum (sepadan _EMOJI_PATTERN Python).
fn remove_emojis(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            let cp = c as u32;
            !((0x1F600..=0x1F64F).contains(&cp)
                || (0x1F300..=0x1F5FF).contains(&cp)
                || (0x1F680..=0x1F6FF).contains(&cp)
                || (0x1F700..=0x1F77F).contains(&cp)
                || (0x1F780..=0x1F7FF).contains(&cp)
                || (0x1F800..=0x1F8FF).contains(&cp)
                || (0x1F900..=0x1F9FF).contains(&cp)
                || (0x1FA00..=0x1FA6F).contains(&cp)
                || (0x1FA70..=0x1FAFF).contains(&cp)
                || (0x2600..=0x26FF).contains(&cp)
                || (0x2700..=0x27BF).contains(&cp)
                || (0x1F1E6..=0x1F1FF).contains(&cp))
        })
        .collect()
}

fn normalize_symbols(text: &str) -> String {
    let mut t = text.to_string();
    for (from, to) in [
        ("\u{2013}", "-"), // – en dash
        ("\u{2011}", "-"), // ‑ non-breaking hyphen
        ("\u{2014}", "-"), // — em dash
        ("\u{00af}", " "), // ¯ macron
        ("_", " "),
        ("\u{201c}", "\""), // " left double quote
        ("\u{201d}", "\""), // " right double quote
        ("\u{2018}", "'"),  // ' left single quote
        ("\u{2019}", "'"),  // ' right single quote
        ("\u{00b4}", "'"),  // ´ acute accent
        ("`", "'"),
        ("[", " "),
        ("]", " "),
        ("|", " "),
        ("/", " "),
        ("#", " "),
        ("→", " "),
        ("←", " "),
    ] {
        t = t.replace(from, to);
    }
    t
}

/// Buang quote/tanda kutip berturut-turut identik (["'`]) jadi satu.
fn dedup_quotes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev: Option<char> = None;
    for c in text.chars() {
        if matches!(c, '"' | '\'' | '`') && prev == Some(c) {
            continue;
        }
        out.push(c);
        prev = Some(c);
    }
    out
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Sepadan _ENDING_PUNCTUATION_PATTERN Python.
fn ends_with_punct(text: &str) -> bool {
    match text.chars().last() {
        Some(c) => matches!(
            c,
            '.' | '!' | '?' | ';' | ':' | ',' | '\'' | '"' | ')' | ']' | '}'
                | '…' | '。' | '」' | '』' | '】' | '〉' | '》' | '›' | '»'
        ),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc() -> UnicodeProcessor {
        // indexer identitas kecil: codepoint 0..512 → dirinya sendiri.
        let ids: Vec<i64> = (0..512).collect();
        UnicodeProcessor { indexer: ids }
    }

    #[test]
    fn tambah_period_bila_perlu() {
        let p = proc();
        let out = p.preprocess("halo dunia", None);
        assert!(out.ends_with('.'));
    }

    #[test]
    fn tak_tambah_period_bila_sudah_ada() {
        let p = proc();
        assert_eq!(p.preprocess("halo!", None), "halo!");
    }

    #[test]
    fn bungkus_token_bahasa() {
        let p = proc();
        assert_eq!(p.preprocess("halo!", Some("id")), "<id>halo!</id>");
    }

    #[test]
    fn collapse_spasi() {
        let p = proc();
        assert_eq!(p.preprocess("a    b", None), "a b.");
    }

    #[test]
    fn normalisasi_em_dash() {
        let p = proc();
        assert_eq!(p.preprocess("a\u{2014}b", None), "a-b.");
    }
}
