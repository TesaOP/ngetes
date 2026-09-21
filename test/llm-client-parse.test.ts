/**
 * test/llm-client-parse.test.ts — extractJSON: parse toleran untuk proxy LLM
 * tidak standar. Kasus nyata direkam dari relay openai-compatible lokal
 * ("9ruter"): body = satu JSON utuh + ekor `data: [DONE]` tanpa pemisah,
 * Content-Type text/event-stream, padahal request tidak meminta stream.
 */
import { describe, it, expect } from "bun:test";
import { extractJSON, salvageJSONArrayOfObjects, extractJSONArrayLoose, extractJSONObjectLoose } from "../src/shared/llm-client";

const RELAY_BODY =
  '{"id":"chatcmpl-RXCYt5B1g88yDAiLwcqSzYmZ","object":"chat.completion","created":1788073032,' +
  '"model":"big-pickle","choices":[{"index":0,"finish_reason":"stop","logprobs":null,' +
  '"message":{"role":"assistant","content":"Halo! Aku baik-baik saja.","name":null,' +
  '"reasoning_content":null,"tool_calls":[]}}],"usage":{"total_tokens":46},"cost":"0"}data: [DONE]\n\n';

describe("extractJSON", () => {
  it("JSON murni tetap lewat tanpa sentuhan", () => {
    const t = '{"choices":[{"message":{"content":"hi"}}]}';
    expect(extractJSON(t).choices[0].message.content).toBe("hi");
  });

  it("JSON utuh + ekor `data: [DONE]` dari relay 9ruter → terselamatkan utuh", () => {
    const j = extractJSON(RELAY_BODY);
    expect(j.object).toBe("chat.completion");
    expect(j.choices[0].message.content).toContain("baik-baik");
    expect(j.choices[0].finish_reason).toBe("stop");
  });

  it("scanner string-aware: `}` di dalam string tidak dihitung sebagai akhir objek", () => {
    const j = extractJSON('{"a":"}"}data: [DONE]');
    expect(j.a).toBe("}");
    expect(() => extractJSON('{"a":" busted')).toThrow(/respon bukan JSON/);
  });

  it("SSE chunk murni (delta.content) → digabung jadi satu pesan", () => {
    const t =
      'data: {"choices":[{"delta":{"content":"Halo"}}]}\n\n' +
      'data: {"choices":[{"delta":{"content":" dunia"}}]}\n\n' +
      "data: [DONE]\n\n";
    const j = extractJSON(t);
    expect(j.choices[0].message.content).toBe("Halo dunia");
  });

  it("chunk terpotong di tengah di-skip, chunk sehat tetap terselamatkan", () => {
    const t =
      'data: {"choices":[{"delta":{"content":"Aku"}}]}\n\n' +
      'data: {"choices":[{"delta":{"content":"sedang menuli\n\n' + // terpotong
      'data: {"choices":[{"delta":{"content":"kan"}}]}\n\n' +
      "data: [DONE]\n\n";
    const j = extractJSON(t);
    expect(j.choices[0].message.content).toBe("Akukan");
  });

  it("SSE satu baris berisi chat.completion penuh (bukan delta) → langsung dipakai", () => {
    const t = 'data: {"choices":[{"message":{"content":"satu"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n';
    expect(extractJSON(t).choices[0].message.content).toBe("satu");
  });

  it("sampah tanpa JSON → tetap error dengan 200 karakter pertama", () => {
    expect(() => extractJSON("<html>Gateway error</html>")).toThrow(/respon bukan JSON/);
  });

  it("body KOSONG → pesan manusiawi, bukan 'respon bukan JSON'", () => {
    expect(() => extractJSON("")).toThrow(/respon KOSONG/);
    expect(() => extractJSON("   \n")).toThrow(/respon KOSONG/);
  });
});

describe("salvageJSONArrayOfObjects", () => {
  it("array utuh → semua objek terparse, urutan terjaga", () => {
    const t = '[{"a":1},{"b":"dua"},{"c":{"d":3}}]';
    const arr = salvageJSONArrayOfObjects(t);
    expect(arr.length).toBe(3);
    expect(arr[0].a).toBe(1);
    expect(arr[2].c.d).toBe(3);
  });

  it("array TERPOTONG di tengah objek terakhir → N-1 objek utuh diselamatkan", () => {
    // kasus nyata: finishReason MAX_TOKENS membelah preset terakhir
    const t = `[
      {"name":"Senyum","values":{"ParamMouthForm":1}},
      {"name":"Sedih","values":{"ParamMouthForm":-1}},
      {"name":"Kaget","values":{"ParamEyeLSmi`;
    const arr = salvageJSONArrayOfObjects(t);
    expect(arr.length).toBe(2);
    expect(arr[0].name).toBe("Senyum");
    expect(arr[1].name).toBe("Sedih");
  });

  it("objek berisi tanda kurung di dalam string tidak salah hitung", () => {
    const t = '[{"note":"haha } :-{","n":1},{"n":2}]';
    const arr = salvageJSONArrayOfObjects(t);
    expect(arr.length).toBe(2);
    expect(arr[0].note).toBe("haha } :-{");
  });

  it("string dengan escape \\\" tidak menutup string lebih awal", () => {
    const t = '[{"s":"kata \\" pembuka"},{"n":9}]';
    const arr = salvageJSONArrayOfObjects(t);
    expect(arr.length).toBe(2);
    expect(arr[1].n).toBe(9);
  });

  it("bukan array / kosong → []", () => {
    expect(salvageJSONArrayOfObjects('{"bukan":"array"}')).toEqual([]);
    expect(salvageJSONArrayOfObjects("")).toEqual([]);
    expect(salvageJSONArrayOfObjects("respon teks polos tanpa kurung")).toEqual([]);
  });
});

describe("extractJSONArrayLoose — balasan LLM tak steril", () => {
  it("array sehat langsung terparse", () => {
    const arr = extractJSONArrayLoose('[{"name":"Senang","category":"emosi"}]');
    expect(arr.length).toBe(1);
    expect(arr[0].name).toBe("Senang");
  });

  it("dikelilingi prosa/markdown tetap terparse", () => {
    const t = "Berikut sarannya:\n```json\n[{\"name\":\"Sedih\"}]\n```\nSemoga membantu!";
    expect(extractJSONArrayLoose(t)[0].name).toBe("Sedih");
  });

  it("objek pembungkus {presets:[...]} diterima", () => {
    const arr = extractJSONArrayLoose('{"presets":[{"name":"Kaget"},{"name":"Malu"}]}');
    expect(arr.length).toBe(2);
    expect(arr[1].name).toBe("Malu");
  });

  it("ECHO: model mengulang prompt (contoh format ikut terbawa) → array JAWABAN yang terakhir yang menang", () => {
    const promptEcho = `Kamu pakar rigging Live2D Cubism. Berdasarkan daftar parameter model di bawah...
PARAMETER TERSEDIA:
- "ParamMouthForm" range [-1, 1] default 0
KEMBALIKAN HANYA JSON array valid.
Format:
[{"name":"Senang","category":"emosi","values":{"ParamMouthForm":1},"parts":{}}]`;
    const answer = '[{"name":"Ceria Bahagia","category":"emosi","values":{"ParamMouthForm":0.8},"parts":{}}]';
    const arr = extractJSONArrayLoose(promptEcho + "\n\n" + answer);
    expect(arr.length).toBe(1);
    expect(arr[0].name).toBe("Ceria Bahagia");
  });

  it("echo tanpa jawaban → salvage contoh dari prompt (lebih baik dari 0), tetap array", () => {
    const t = "Kamu pakar rigging. PARAMETER TERSEDIA:\n- \"ParamX\" range [-1, 1]\nFormat:\n" +
      '[{"name":"Senang","category":"emosi","values":{"ParamMouthForm":1},"parts":{}}]';
    const arr = extractJSONArrayLoose(t);
    expect(arr.length).toBe(1);
    expect(arr[0].name).toBe("Senang");
  });

  it("array terpotong di tengah → salvage N-1 objek utuh", () => {
    const t = '[{"name":"Senyum"},{"name":"Sedih"},{"name":"Kaget","val';
    const arr = extractJSONArrayLoose(t);
    expect(arr.length).toBe(2);
  });

  it("sampah total → []", () => {
    expect(extractJSONArrayLoose("")).toEqual([]);
    expect(extractJSONArrayLoose("balasan teks polos tanpa kurung sekali pun")).toEqual([]);
  });
});

describe("extractJSONObjectLoose — balasan objek tunggal (motion studio)", () => {
  it("objek sehat langsung terparse", () => {
    const o = extractJSONObjectLoose('{"description":"anggukan santai","tags":["santai"]}');
    expect(o?.description).toBe("anggukan santai");
  });

  it("prosa + markdown tetap terparse", () => {
    const o = extractJSONObjectLoose("Ini hasilnya:\n```json\n{\"description\":\"menyeringai\"}\n```");
    expect(o?.description).toBe("menyeringai");
  });

  it("ECHO: template instruksi (0.0-1.0, bukan JSON valid) terlewati → objek JAWABAN yang menang", () => {
    const echo = `Kamu menganalisa satu gerakan karakter Live2D.
balas JSON:
{
  "description": "satu kalimat bahasa Indonesia",
  "tags": ["3-5 tag"],
  "emotionCompatibility": { "<emosi>": 0.0-1.0 }
}`;
    const answer = '{"description":"kepala manggut-manggut","tags":["mengantuk"],"emotionCompatibility":{"mengantuk":0.9}}';
    const o = extractJSONObjectLoose(echo + "\n\n" + answer);
    expect(o?.description).toBe("kepala manggut-manggut");
    expect(o?.emotionCompatibility.mengantuk).toBe(0.9);
  });

  it("echo murni tanpa jawaban valid → null (bukan template sampah)", () => {
    const echo = "Kamu menganalisa satu gerakan. balas JSON:\n{ \"description\": \"x\", \"emotionCompatibility\": { \"<emosi>\": 0.0-1.0 } }";
    expect(extractJSONObjectLoose(echo)).toBeNull();
  });

  it("sampah total → null", () => {
    expect(extractJSONObjectLoose("")).toBeNull();
    expect(extractJSONObjectLoose("teks polos tanpa kurung")).toBeNull();
  });
});
