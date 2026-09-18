/** Fase 5 — load satu golden model via Cubism Core native (tanpa byte-hack) + Pixi 8. */
import * as PIXI from "pixi.js";
import type { Live2DAdapterOptions, Live2DModelLike } from "./types";

export async function loadLive2DModel(
  modelPath: string,
  _options?: Live2DAdapterOptions,
): Promise<Live2DModelLike> {
  const core = (globalThis as any).Live2DCubismCore ?? (window as any)?.Live2DCubismCore;
  if (!core) throw new Error("Live2DCubismCore belum dimuat — <script src=\"js/live2dcubismcore.min.js\"> harus sebelum bundle");

  // model3.json → json (pakai fetch, bukan CubismJson — cukup untuk Fase 5)
  const res = await fetch(modelPath);
  if (!res.ok) throw new Error(`fetch model3.json gagal ${res.status} ${modelPath}`);
  const dir = modelPath.slice(0, modelPath.lastIndexOf("/") + 1);
  const setting = await res.json();
  const mocFile: string = setting.FileReferences?.Moc;
  if (!mocFile) throw new Error("FileReferences.Moc tidak ada di model3.json");
  const mocUrl = dir + mocFile;
  const mocRes = await fetch(mocUrl);
  if (!mocRes.ok) throw new Error(`fetch moc gagal ${mocRes.status} ${mocUrl}`);
  const mocBytes = await mocRes.arrayBuffer();

  const mocVersion: number = core.Version.csmGetMocVersion(mocBytes);
  const moc = core.Moc.fromArrayBuffer(mocBytes);
  if (!moc) throw new Error(`Core menolak moc v${mocVersion} — Core basi? (Core ${core.Version.csmGetVersion().toString(16)})`);
  const model = core.Model.fromMoc(moc);
  if (!model) throw new Error("Model.fromMoc gagal");

  const drawable = model.drawables?.count ?? 0;
  const offscreen = (model as any).offscreens?.count ?? 0;
  const textures: number = setting.FileReferences?.Textures?.length ?? 0;

  // Pixi 8 container placeholder — Fase 6 akan ganti dengan render Cubism sesungguhnya
  const container = new PIXI.Container();
  container.label = "Live2D/Fase5";
  const bg = new PIXI.Graphics().rect(0, 0, 320, 360).fill({ color: 0x0f172a, alpha: 0.08 });
  const badge = new PIXI.Graphics().rect(8, 8, 304, 28).fill(0x22c55e);
  const txt = new PIXI.Text({
    text: `moc v${mocVersion}  d:${drawable}  off:${offscreen}  tex:${textures}`,
    style: { fill: 0xffffff, fontSize: 11, fontFamily: "monospace" },
  });
  txt.x = 12; txt.y = 15;
  container.addChild(bg, badge, txt);

  return {
    container,
    info: { mocVersion, drawable, offscreen, textures },
    destroy() {
      try { model._release?.(); } catch {}
      try { moc._release?.(); } catch {}
      container.destroy({ children: true });
    },
  };
}
