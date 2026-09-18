/**
 * Live2DView — integrasi view Pixi 8 ke panggung asli (Fase A: panggung + chat).
 *
 * Prinsip TRANSPLANT JIWA: seluruh perilaku karakter app.js — tick liveliness
 * (601-883), kedip, gaze kursor, lipsync, overlay emosi, preset/sheet, protokol
 * clipAuthority, MotionRuntime + bridge lama — TIDAK ditulis ulang. Semua
 * tulisan app.js bermuara di satu titik mati: coreModel() (app.js:137) →
 * backend di file ini. app.js tetap satu-satunya penulis pose; efek framework
 * blink/look/breath sengaja TIDAK didaftarkan (satu fitur satu pemilik),
 * physics/pose/ekspresi tetap milik framework.
 *
 * Urutan tulis per frame di stack baru = urutan stack lama: framework
 * (motion → efek → flush) dulu, lalu tulisan app.js (updater order 900) —
 * padanan semantik hook beforeModelUpdate pixi-live2d.
 */
import * as PIXI from "pixi.js";
import { Live2DRenderer } from "../Live2DRenderer";
import { Live2DUserModel, MotionPriority } from "../Live2DUserModel";
import { ICubismUpdater } from "../cubism/motion/icubismupdater";
import type { CubismModel } from "../cubism/model/cubismmodel";
import { CubismFramework } from "../cubism/live2dcubismframework";
import {
  computeModelMvp,
  localToScreen,
  type FacadeTransform,
} from "./framing";

/** Satuan px CSS per unit canvas — konstanta bebas: frameModel app.js
 * menyetel scale agar model pas stage, jadi nilai ini terkompensasi. */
const MODEL_UNIT_PX = 400;

/** Flush tulisan app.js (pokeParam/applyOverrides/applyRawDrive) tiap
 * frame SETELAH semua efek framework — padanan beforeModelUpdate lama. */
class AppWriteUpdater extends ICubismUpdater {
  private pending = new Map<string, { v: number; w: number }>();

  constructor() {
    super(900);
  }

  /** backend coreModel app.js — satu-satunya pintu tulis dari legacy. */
  readonly coreModel = {
    setParameterValueById: (id: string, v: number, w = 1) => {
      this.pending.set(id, { v, w });
    },
    getParameterValueById: (_id: string) => 0, // di-overview Live2DView (butuh renderer)
    setPartOpacityById: (_id: string, _v: number) => {}, // idem
    getModel: () => null, // idem
  };

  onLateUpdate(model: CubismModel, _dt: number): void {
    if (!this.pending.size) return;
    const idMgr = CubismFramework.getIdManager();
    for (const [id, { v, w }] of this.pending) {
      model.setParameterValueById(idMgr.getId(id), v, w);
    }
    this.pending.clear();
  }
}

export class Live2DView {
  pixiApp: PIXI.Application | null = null;
  renderer: Live2DRenderer | null = null;
  private writes = new AppWriteUpdater();
  private facade: any = null;
  private transform: FacadeTransform | null = null;
  private rafId: number | null = null;
  private initPromise: Promise<void> | null = null;

  /** Miliki canvas panggung: Pixi 8 Application di atas #live2d-canvas
   * (transparan — latar dari CSS #stage; alpha context dipertahankan
   * untuk blend Atop/Out moc3 v6, pelajaran patchCore6Compat lama). */
  async init(canvas: HTMLCanvasElement, cssW: number, cssH: number): Promise<void> {
    if (this.initPromise) return this.initPromise;
    this.initPromise = (async () => {
      const resolution = Math.min(window.devicePixelRatio || 1, 2);
      const pixiApp = new PIXI.Application();
      await pixiApp.init({
        canvas,
        width: cssW,
        height: cssH,
        backgroundAlpha: 0,
        antialias: true,
        resolution,
        autoDensity: true,
        preference: "webgl",
      });
      this.pixiApp = pixiApp;
      const gl = (canvas.getContext("webgl2") || canvas.getContext("webgl")) as WebGL2RenderingContext;
      this.renderer = new Live2DRenderer(canvas, gl);
      // latar dari CSS stage — Cubism tidak clear
      this.renderer.setClear(null);
      const draw = () => {
        this.renderer?.draw();
        this.rafId = requestAnimationFrame(draw);
      };
      // riset: build ESM Pixi 8 minimal tanpa TickerPlugin — guard + rAF sendiri.
      // Prioritas ticker WAJIB di bawah render Pixi (LOW=-25): draw Cubism
      // harus SETELAH clear Pixi — bila tidak, render Pixi menghapus hasilnya
      // tiap frame (canvas tampak kosong). rAF fallback otomatis aman
      // karena selalu berjalan setelah ticker.
      if (pixiApp?.ticker && typeof pixiApp.ticker.add === "function") {
        pixiApp.ticker.add(() => this.renderer?.draw(), null, -50);
      } else {
        this.rafId = requestAnimationFrame(draw);
      }
    })();
    return this.initPromise;
  }

  async loadModel(modelPath: string): Promise<any> {
    if (!this.renderer) throw new Error("Live2DView belum init");
    const prev = this.facade;
    if (prev?.destroy) { try { prev.destroy(); } catch {} }
    this.facade = null;

    await this.renderer.loadModel(modelPath, {
      // blink/look/breath milik driver app.js (transplant jiwa) —
      // physics/pose/ekspresi tetap framework
      effects: { blink: false, look: false, breath: false },
      autoIdle: false, // app.js startIdleMotion pemilik idle
    });

    const userModel = (this.renderer as any).userModel as Live2DUserModel;
    if (!(this.renderer as any).__writesRegistered) {
      userModel.updateScheduler.addUpdatableList(this.writes);
      userModel.updateScheduler.sortUpdatableList();
      (this.renderer as any).__writesRegistered = true;
    }

    const info = this.renderer.getModelCanvasSize();
    const groups = this.renderer.getOfficialGroups();
    const profile = this.renderer.getModelProfile();
    const t: FacadeTransform = {
      x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0,
      anchorX: 0, anchorY: 0,
      origW: MODEL_UNIT_PX * info.width,
      origH: MODEL_UNIT_PX * info.height,
      canvasW: info.width, canvasH: info.height,
      stageW: this.pixiApp!.screen.width,
      stageH: this.pixiApp!.screen.height,
      resolution: this.pixiApp!.renderer.resolution,
    };
    this.transform = t;
    this.renderer.setMvpProvider(() => computeModelMvp(this.transform!));

    // backend untuk coreModel() app.js
    const renderer = this.renderer;
    const self = this;
    const backend = {
      setParameterValueById(id: string, v: number, w = 1) {
        self.writes.coreModel.setParameterValueById(id, v, w);
      },
      getParameterValueById(id: string) {
        return renderer.getParameter(id) ?? 0;
      },
      setPartOpacityById(id: string, v: number) {
        const m = (renderer as any).userModel?.getModel() as any;
        if (!m) return;
        try {
          const h = CubismFramework.getIdManager().getId(id);
          const idx = m.getPartIndex?.(h);
          if (idx != null && idx >= 0) m.setPartOpacityByIndex?.(idx, v);
        } catch (e) {}
      },
      getModel() {
        const m = (renderer as any).userModel?.getModel() as any;
        if (!m) return null;
        const partCount = () => m.getPartCount?.() ?? 0;
        return {
          // API part — enumerateParts app.js (sheet/inspeksi)
          getPartCount: partCount,
          getPartIds: () => {
            const n = partCount();
            const out: string[] = [];
            for (let i = 0; i < n; i++) out.push(m.getPartId?.(i)?.getString?.() ?? "");
            return out;
          },
          getPartOpacityByIndex: (i: number) => m.getPartOpacityByIndex?.(i) ?? 1,
          // API parameter — detectModelCapabilities app.js: roleMap (via
          // m.getParameterIds jalur resmi) + paramRange (min/max/def per id).
          // getParameterId wajib mengembalikan STRING — itu kunci paramRange.
          getParameterCount: () => m.getParameterCount?.() ?? 0,
          getParameterId: (i: number) => m.getParameterId?.(i)?.getString?.() ?? "",
          getParameterMinimumValue: (i: number) => m.getParameterMinimumValue?.(i) ?? 0,
          getParameterMaximumValue: (i: number) => m.getParameterMaximumValue?.(i) ?? 0,
          getParameterDefaultValue: (i: number) => m.getParameterDefaultValue?.(i) ?? 0,
        };
      },
    };

    const exprDefs = (profile?.expressions ?? []).map((name) => ({
      Name: name, File: `expressions/${name}.exp3.json`,
    }));
    const motionDefs: Record<string, unknown[]> = {};
    for (const g of profile?.motions ?? []) motionDefs[g.group] = new Array(g.count).fill({});

    const facade = {
      get x() { return t.x }, set x(v: number) { t.x = v },
      get y() { return t.y }, set y(v: number) { t.y = v },
      get rotation() { return t.rotation }, set rotation(v: number) { t.rotation = v },
      get zIndex() { return 0 }, set zIndex(_: number) {},
      get width() { return t.origW * t.scaleX },
      get height() { return t.origH * t.scaleY },
      scale: {
        get x() { return t.scaleX }, set x(v: number) { t.scaleX = v },
        get y() { return t.scaleY }, set y(v: number) { t.scaleY = v },
        set(v: number) { t.scaleX = v; t.scaleY = v; },
      },
      anchor: {
        get x() { return t.anchorX }, set x(v: number) { t.anchorX = v },
        get y() { return t.anchorY }, set y(v: number) { t.anchorY = v },
        set(x: number, y: number) { t.anchorX = x; t.anchorY = y; },
      },
      toGlobal(p: { x: number; y: number }) {
        return localToScreen(t, p.x, p.y);
      },
      /** AABB dunia (CSS) dari 4 pojok box lokal — padanan getBounds pixi
       * untuk frameModel/applyStageLayout (natW/natH = bounds / scale). */
      getBounds() {
        const pts = [
          localToScreen(t, 0, 0),
          localToScreen(t, t.origW, 0),
          localToScreen(t, t.origW, t.origH),
          localToScreen(t, 0, t.origH),
        ];
        const xs = pts.map((p) => p.x);
        const ys = pts.map((p) => p.y);
        const x = Math.min(...xs);
        const y = Math.min(...ys);
        return { x, y, width: Math.max(...xs) - x, height: Math.max(...ys) - y };
      },
      /** Jalur resmi #1 detectModelCapabilities (capProbe "pixi-api"):
       * daftar id parameter sebagai string[]. */
      getParameterIds() {
        return renderer.getParameters();
      },
      /** prioritas lama 1/2/3 = Idle/Normal/Force; index -1 = acak grup. */
      motion(group: string, index: number, priority: number): void {
        const prio = priority >= 3 ? MotionPriority.Force : priority >= 2 ? MotionPriority.Normal : MotionPriority.Idle;
        const cnt = (motionDefs[group] || []).length;
        const idx = index < 0 ? Math.floor(Math.random() * cnt) : index;
        void renderer.playNativeMotion(group, Math.max(0, idx), prio)
          .then((h) => { if (h < 0) console.warn(`[view] motion ${group} tidak jalan (prioritas/grup)`); });
      },
      async expression(name: string): Promise<boolean> {
        return renderer.playExpression(name);
      },
      expressions: exprDefs,
      destroy() { self.destroyModel(); },
      internalModel: {
        coreModel: backend,
        // startIdleMotion app.js hanya Object.keys(definitions)
        motionManager: {
          definitions: motionDefs,
          expressionManager: { definitions: exprDefs },
        },
        settings: {
          expressions: exprDefs,
          getLipSyncParameters: () => groups.lipSyncIds,
          getEyeBlinkParameters: () => groups.eyeBlinkIds,
        },
        eyeBlinkIds: groups.eyeBlinkIds,
        lipSyncIds: groups.lipSyncIds,
        focusController: { x: 0, y: 0 },
        // `on` sengaja tidak ada: installOverrideGuard dilewati — padanannya
        // (re-tulis sticky tiap frame setelah framework) = AppWriteUpdater.
      },
    };
    this.facade = facade;
    return facade;
  }

  destroyModel(): void {
    // rAF loop TIDAK dibatalkan: ganti model menghancurkan model lama lalu
    // loadModel baru — loop menunggu (draw() early-return saat userModel null).
    this.renderer?.destroy();
  }

  resize(w: number, h: number): void {
    this.pixiApp?.renderer.resize(w, h);
    if (this.transform) {
      this.transform.stageW = this.pixiApp!.screen.width;
      this.transform.stageH = this.pixiApp!.screen.height;
    }
  }

  setBackground(hex: number): void {
    const bg: any = (this.pixiApp?.renderer as any)?.background;
    if (bg) bg.color = hex;
  }
}
