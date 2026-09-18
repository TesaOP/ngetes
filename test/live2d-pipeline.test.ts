/**
 * Unit test stack Live2D baru (src/live2d/*) — logika murni, tanpa jaringan,
 * tanpa WebGL, tanpa menulis data/config.json.
 */
import { describe, expect, it } from "bun:test";
import { ParameterArbiter } from "../src/live2d/ParameterArbiter";
import { analyzeCapabilities } from "../src/live2d/CapabilityAnalyzer";
import type { ModelProfile } from "../src/live2d/ModelInspector";
import { mapRoles } from "../src/client/engine/role-mapping";

describe("ParameterArbiter — prioritas & konflik", () => {
  it("manual (100) mengalahkan blink (90)", () => {
    const a = new ParameterArbiter();
    a.set("ParamAngleX", 1, "blink");
    a.set("ParamAngleX", 2, "manual");
    expect(a.resolve().get("ParamAngleX")).toBe(2);
  });

  it("blink (90) mengalahkan emotion (80) dan motion (60)", () => {
    const a = new ParameterArbiter();
    a.set("ParamEyeLOpen", 0.1, "emotion");
    a.set("ParamEyeLOpen", 0.2, "motion");
    a.set("ParamEyeLOpen", 0, "blink");
    expect(a.resolve().get("ParamEyeLOpen")).toBe(0);
  });

  it("prioritas sama → seq terbaru menang", () => {
    const a = new ParameterArbiter();
    a.set("ParamAngleX", 1, "motion");
    a.set("ParamAngleX", 5, "motion");
    expect(a.resolve().get("ParamAngleX")).toBe(5);
  });

  it("hasConflict hanya true bila >1 sumber menulis param sama", () => {
    const a = new ParameterArbiter();
    a.set("ParamAngleX", 1, "motion");
    expect(a.hasConflict("ParamAngleX")).toBe(false);
    a.set("ParamAngleX", 2, "emotion");
    expect(a.hasConflict("ParamAngleX")).toBe(true);
    expect(a.hasConflict("ParamTidakAda")).toBe(false);
  });

  it("clearSource hanya membuang sumber itu — param lain utuh", () => {
    const a = new ParameterArbiter();
    a.set("ParamAngleX", 1, "motion");
    a.set("ParamAngleX", 2, "gaze");
    a.set("ParamAngleY", 3, "gaze");
    a.clearSource("gaze");
    expect(a.resolve().get("ParamAngleX")).toBe(1);
    expect(a.resolve().has("ParamAngleY")).toBe(false);
  });

  it("resolve() murni — dua panggilan hasil sama, tidak mengubah state", () => {
    const a = new ParameterArbiter();
    a.set("ParamAngleX", 7, "motion");
    const r1 = a.resolve();
    const r2 = a.resolve();
    expect(r1.get("ParamAngleX")).toBe(7);
    expect(r2.get("ParamAngleX")).toBe(7);
    expect(a.hasConflict("ParamAngleX")).toBe(false);
  });
});

describe("CapabilityAnalyzer — kapabilitas via role, bukan id model", () => {
  const profileDasar = (over: Partial<ModelProfile> = {}): ModelProfile => ({
    parameters: [{ id: "ParamAngleX", min: -30, max: 30, default: 0 }],
    parts: [],
    motions: [],
    expressions: [],
    physics: false,
    pose: false,
    canvas: { width: 2, height: 3 },
    drawable: 10,
    offscreen: 0,
    ...over,
  });

  it("semua role ada → kapabilitas inti true", () => {
    const roleMap = {
      angleX: "P1", angleY: "P2", angleZ: "P3",
      eyeLOpen: "P4", eyeROpen: "P5",
      mouthOpenY: "P6", mouthForm: "P7",
      eyeBallX: "P8", eyeBallY: "P9",
      bodyAngleX: "P10",
      blush: "P11", ear: "P12",
    };
    const caps = analyzeCapabilities(profileDasar({ physics: true }), roleMap);
    expect(caps.has.headRotation).toBe(true);
    expect(caps.has.eyeBlink).toBe(true);
    expect(caps.has.mouth).toBe(true);
    expect(caps.has.gaze).toBe(true);
    expect(caps.has.bodyRotation).toBe(true);
    expect(caps.has.physics).toBe(true);
    expect(caps.has.blush).toBe(true);
    expect(caps.has.earMovement).toBe(true);
    expect(caps.missing).toEqual([]);
  });

  it("tanpa role ear → earMovement missing dengan fallback ignore (bukan crash)", () => {
    const caps = analyzeCapabilities(profileDasar(), { angleX: "P1" });
    expect(caps.has.earMovement).toBe(false);
    expect(caps.missing).toContain("earMovement");
    expect(caps.fallback.earMovement).toBe("ignore");
  });

  it("blush missing + model punya ekspresi → fallback ke expression pertama", () => {
    const caps = analyzeCapabilities(
      profileDasar({ expressions: ["f01_normal", "f02_shy"] }),
      { angleX: "P1" },
    );
    expect(caps.has.blush).toBe(false);
    expect(caps.fallback.blush).toBe("expression:f01_normal");
  });

  it("blush missing + tanpa ekspresi → fallback ignore", () => {
    const caps = analyzeCapabilities(profileDasar(), { angleX: "P1" });
    expect(caps.fallback.blush).toBe("ignore");
  });

  it("physics diambil dari profile (file physics3 ada di manifest)", () => {
    const capsOn = analyzeCapabilities(profileDasar({ physics: true }), {});
    const capsOff = analyzeCapabilities(profileDasar({ physics: false }), {});
    expect(capsOn.has.physics).toBe(true);
    expect(capsOff.has.physics).toBe(false);
    expect(capsOff.missing).toContain("physics");
  });
});

describe("role ear — deteksi via role space, bebas positif palsu", () => {
  it("keluarga ParamEar* terpetakan ke role ear", () => {
    const m = mapRoles(new Set(["ParamAngleX", "ParamEarL", "ParamEarR"]));
    expect(["ParamEarL", "ParamEarR"]).toContain(m.ear);
  });

  it("varian left_ear / Right_Ear terpetakan via token terpisah", () => {
    const m = mapRoles(new Set(["Left_Ear", "Right_Ear"]));
    expect(["Left_Ear", "Right_Ear"]).toContain(m.ear);
  });

  it("ParamEarLeft / ParamEarRight terpetakan", () => {
    const m = mapRoles(new Set(["ParamEarLeft", "ParamEarRight"]));
    expect(["ParamEarLeft", "ParamEarRight"]).toContain(m.ear);
  });

  it("HEART dan PEARL tidak boleh positif-palsu jadi ear (pelajaran 'earl' ⊂ 'pearl')", () => {
    const m = mapRoles(new Set(["ParamHeart", "PearlOpacity"]));
    expect(m.ear).toBeUndefined();
  });

  it("invariansi nama: rig diganti nama total → ear tidak terpetakan (tidak nebak)", () => {
    const m = mapRoles(new Set(["m_001", "m_002", "m_003"]));
    expect(m.ear).toBeUndefined();
  });
});
