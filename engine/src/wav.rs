//! Encoder WAV minimal: f32 mono [-1,1] → WAV PCM 16-bit.
//! Sepadan output `sf.write` SuperTonic (44.1kHz mono 16-bit).

/// Bungkus sampel f32 mono jadi byte WAV PCM16.
pub fn encode_pcm16(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let n = samples.len();
    let data_len = (n * 2) as u32; // 2 byte/sampel
    let byte_rate = sample_rate * 2; // mono, 16-bit
    let mut buf = Vec::with_capacity(44 + n * 2);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // audio format = PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // channels = mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * 32767.0).round() as i16;
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

/// Dekode WAV PCM16 mono → f32 [-1,1]. Untuk STT (browser kirim WAV).
/// Mengembalikan (samples, sample_rate). Sederhana: asumsi PCM16.
pub fn decode_pcm16(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("bukan file WAV RIFF".into());
    }
    // Cari chunk fmt & data (tak asumsi offset tetap).
    let mut pos = 12;
    let mut sample_rate = 0u32;
    let mut channels = 1u16;
    let mut bits = 16u16;
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let sz = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]])
            as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
            sample_rate = u32::from_le_bytes([
                bytes[body + 4],
                bytes[body + 5],
                bytes[body + 6],
                bytes[body + 7],
            ]);
            bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
        } else if id == b"data" {
            let end = (body + sz).min(bytes.len());
            data = Some(&bytes[body..end]);
        }
        pos = body + sz + (sz & 1); // chunk padded ke genap
    }
    let data = data.ok_or("chunk data tak ditemukan")?;
    if bits != 16 {
        return Err(format!("hanya PCM16 didukung, dapat {bits}-bit"));
    }
    let ch = channels.max(1) as usize;
    let mut out = Vec::with_capacity(data.len() / 2 / ch);
    let mut i = 0;
    while i + 2 * ch <= data.len() {
        // downmix ke mono (rata-rata channel)
        let mut acc = 0i32;
        for c in 0..ch {
            let s = i16::from_le_bytes([data[i + 2 * c], data[i + 2 * c + 1]]);
            acc += s as i32;
        }
        out.push((acc as f32 / ch as f32) / 32768.0);
        i += 2 * ch;
    }
    Ok((out, sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_header() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let wav = encode_pcm16(&samples, 44100);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        let (dec, sr) = decode_pcm16(&wav).unwrap();
        assert_eq!(sr, 44100);
        assert_eq!(dec.len(), samples.len());
        // toleransi kuantisasi PCM16
        for (a, b) in dec.iter().zip(samples.iter()) {
            assert!((a - b).abs() < 0.001, "{a} vs {b}");
        }
    }
}
