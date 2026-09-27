/**
 * test/reply-actions.test.ts — deriveReplyActions (Feature: ekspresi/motion
 * balasan VTuber §7). Fungsi murni: satu set aksi visual dari balasan pendek.
 * Audio balasan VTuber diputar terpisah (pipeline speech app utama); fungsi
 * ini hanya menurunkan emosi + gesture supaya karakter berreaksi model-agnostik.
 */
import { describe, it, expect } from "bun:test";
import { deriveReplyActions } from "../src/client/agent/directive-parser";

describe("deriveReplyActions", () => {
  it("teks kosong → tanpa aksi (expressReply akan no-op)", () => {
    expect(deriveReplyActions("")).toEqual({});
    expect(deriveReplyActions("   ")).toEqual({});
  });

  it("teks polos → emosi saja, TANPA gesture (tabel gesture bawaan dihapus)", () => {
    const a = deriveReplyActions("Makasih banyak donasinya, seru banget!");
    expect(a.emotion).toBe("senang");
    expect(a.gesture).toBeUndefined();
    expect(a.intensity).toBeGreaterThan(0.5);
  });

  it("emosi normal → intensity lebih rendah + tanpa gesture", () => {
    const a = deriveReplyActions("Oke, jadi begitu ceritanya.");
    expect(a.emotion).toBe("normal");
    expect(a.gesture).toBeUndefined();
    expect(a.intensity).toBe(0.5);
  });

  it("menghormati directive eksplisit LLM (tidak menebak emosi lagi)", () => {
    const a = deriveReplyActions("[EMOTION:kaget][MOTION:recoil_surprised] Hah, beneran?");
    expect(a.emotion).toBe("kaget");
    expect(a.motion).toBe("recoil_surprised");
  });

  it("gabung banyak directive jadi satu set (scalar terakhir menang)", () => {
    const a = deriveReplyActions("[EMOTION:senang] Halo [EMOTION:malu] iya deh");
    expect(a.emotion).toBe("malu");
  });
});
