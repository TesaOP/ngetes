/**
 * Live2DUserModel — pipeline update dua fase resmi (pola LAppModel 5-r.5)
 * di atas CubismUserModel vendored, yang TIDAK punya update(dt) maupun
 * scheduler sendiri. Urutan per frame (aturan paling sering dilanggar):
 *   loadParameters → motion → saveParameters → scheduler.onLateUpdate →
 *   model.update — motion satu-satunya penulis persisten; tulisan efek
 *   dibuang loadParameters() frame berikutnya supaya efek aditif
 * (breath/look/lipsync) tidak menumpuk tanpa batas.
 * dt dalam DETIK; tiap manager meng-akumulasi userTimeSeconds sendiri.
 */
import { CubismUserModel } from "./cubism/model/cubismusermodel";
import type { CubismModel } from "./cubism/model/cubismmodel";
import type { CubismModelSettingJson } from "./cubism/cubismmodelsettingjson";
import { CubismUpdateScheduler } from "./cubism/motion/cubismupdatescheduler";
import { ICubismUpdater, CubismUpdateOrder } from "./cubism/motion/icubismupdater";
import { CubismEyeBlink } from "./cubism/effect/cubismeyeblink";
import { CubismEyeBlinkUpdater } from "./cubism/motion/cubismeyeblinkupdater";
import { CubismExpressionUpdater } from "./cubism/motion/cubismexpressionupdater";
import { CubismLook, LookParameterData } from "./cubism/effect/cubismlook";
import { CubismLookUpdater } from "./cubism/motion/cubismlookupdater";
import { CubismBreath, BreathParameterData } from "./cubism/effect/cubismbreath";
import { CubismBreathUpdater } from "./cubism/motion/cubismbreathupdater";
import { CubismPhysicsUpdater } from "./cubism/motion/cubismphysicsupdater";
import { CubismPoseUpdater } from "./cubism/motion/cubismposeupdater";
import type { CubismMotion } from "./cubism/motion/cubismmotion";
import { ACubismMotion } from "./cubism/motion/acubismmotion";
import { CubismFramework } from "./cubism/live2dcubismframework";
import type { CubismIdHandle } from "./cubism/id/cubismid";

/** Konvensi prioritas motion sample resmi (LAppDefine). */
export const MotionPriority = {
  None: 0,
  Idle: 1,
  Normal: 2,
  Force: 3,
} as const;

/** Updater lipsync: menulis param mulut (role-resolved) dari penyedia
 * nilai 0..1, diskalakan ke range aktual param. Provider null = tidak
 * bicara → nilai base/motion yang tampil (tulisan efek frame-transien).
 * SET, bukan add: mulut terbuka menggantikan base saat bicara. */
class LipsyncUpdater extends ICubismUpdater {
  constructor(
    private provider: () => number | null,
    private id: CubismIdHandle,
    private min: number,
    private max: number,
    order: number,
  ) {
    super(order);
  }
  onLateUpdate(model: CubismModel, _dt: number): void {
    const v = this.provider();
    if (v == null) return;
    const openness = Math.max(0, Math.min(1, v));
    model.setParameterValueById(
      this.id,
      this.min + openness * (this.max - this.min),
      1,
    );
  }
}

/** Updater ber-gate runtime. Tulisan efek frame-transien (dibuang
 * loadParameters frame berikutnya) — gate false berarti nilai base motion
 * yang tampil, tanpa restore manual. Dipakai breath; blink memakai
 * callback _motionUpdated. */
class GatedUpdater extends ICubismUpdater {
  constructor(
    private inner: ICubismUpdater,
    private gate: () => boolean,
    order: number,
  ) {
    super(order);
  }
  onLateUpdate(model: CubismModel, dt: number): void {
    if (this.gate()) this.inner.onLateUpdate(model, dt);
  }
}

export class Live2DUserModel extends CubismUserModel {
  readonly updateScheduler = new CubismUpdateScheduler();
  /** View integrasi mematikan ini: app.js punya idle scheduler sendiri
   * (startIdleMotion, interval 7 dtk) — dua idle = dua sumber motion. */
  autoIdle = true;
  /** Gate kedip: false = updater melewatkan frame itu. Diputuskan app.js
   * dari konfigurasi sheet (blinkEnabled) + state.frozen. */
  blinkGate: (() => boolean) | null = null;
  /** Gate napas: hasBreath + frozen + motion layer aktif (kurva klip bisa
   * membawa breath sendiri). */
  breathGate: (() => boolean) | null = null;
  /** Gate gaze: off saat otak/klip memegang pose (aiLock/frozen/motion
   * layer) supaya tidak dobel dengan gaze driver. */
  lookGate: (() => boolean) | null = null;
  /** Penyedia lipsync: nilai 0..1 saat bicara, null saat tidak — dipasang
   * app.js (analisis audio TTS lokal milik driver). */
  lipsyncProvider: (() => number | null) | null = null;

  /** Pasang updater lipsync untuk SATU param mulut (role-resolved).
   * Dipanggil renderer setelah role map siap; model tanpa role mulut
   * tidak mendaftar apa pun. */
  registerLipsync(id: CubismIdHandle, min: number, max: number): void {
    this.updateScheduler.addUpdatableList(
      new LipsyncUpdater(() => this.lipsyncProvider?.() ?? null, id, min, max, 450),
    );
  }

  /** Ganti data look (gain keekspresivan gaze per grup sendi) tanpa membuat
   * ulang updater — dipanggil renderer saat config user berubah. */
  setLookParameters(list: LookParameterData[]): void {
    this._look?.setParameters(list);
  }

  /** Fallback blink: rig tanpa grup EyeBlink di manifest menghasilkan
   * instance ber-0 id — isi id mata dari role mapping (by-name). Id milik
   * deklarasi rigger selalu menang (no-op bila sudah ada). */
  ensureEyeBlink(ids: CubismIdHandle[]): void {
    if (!ids.length) return;
    if (this._eyeBlink) {
      if (this._eyeBlink.getParameterIds().length) return;
      this._eyeBlink.setParameterIds(ids);
      return;
    }
    this._eyeBlink = new CubismEyeBlink(null as any);
    this._eyeBlink.setParameterIds(ids);
  }
  private _setting: CubismModelSettingJson | null = null;
  private _baseUrl = "";
  private _motionCache = new Map<string, CubismMotion>();
  private _expressionCache = new Map<string, ACubismMotion>();
  private _motionUpdated = false;
  private _eyeBlinkIds: CubismIdHandle[] = [];
  private _lipSyncIds: CubismIdHandle[] = [];
  private _look: CubismLook | null = null;

  /** Kaitkan manifest + direktori dasar setelah moc termuat.
   * Id blink/lipsync dari grup resmi manifest (by-name, bukan indeks). */
  attachSetting(
    setting: CubismModelSettingJson,
    baseUrl: string,
    eyeBlinkIds: string[],
    lipSyncIds: string[],
  ): void {
    this._setting = setting;
    this._baseUrl = baseUrl;
    const idMgr = CubismFramework.getIdManager();
    this._eyeBlinkIds = eyeBlinkIds.map((id) => idMgr.getId(id));
    this._lipSyncIds = lipSyncIds.map((id) => idMgr.getId(id));
    // Base class menaruh _eyeBlink = null; blink dikonstruksi dari manifest.
    this._eyeBlink = CubismEyeBlink.create(setting);
  }

  /** Daftarkan updater efek ke scheduler (urut execution order).
   * Dipanggil SETELAH role map siap karena look/breath diskalakan dari
   * range aktual model (role-space), bukan angka std ±30/±1. Opsi enabled
   * untuk integrasi view: fitur yang app.js miliki (blink/look/breath
   * driver lama) TIDAK didaftarkan — satu fitur satu pemilik. */
  registerEffectUpdaters(opts: {
    look: LookParameterData[];
    breath: BreathParameterData[];
    enabled?: { blink?: boolean; look?: boolean; breath?: boolean };
  }): void {
    const en = { blink: true, look: true, breath: true, ...(opts.enabled ?? {}) };
    if (this._eyeBlink && en.blink) {
      // Kedip mundur otomatis saat motion menganimasikan param frame itu
      // (callback resmi _motionUpdated) atau saat gate ditutup.
      this.updateScheduler.addUpdatableList(
        new CubismEyeBlinkUpdater(
          () => this._motionUpdated || !(this.blinkGate?.() ?? true),
          this._eyeBlink,
        ),
      );
    }
    this.updateScheduler.addUpdatableList(
      new CubismExpressionUpdater(this._expressionManager),
    );
    if (en.look) {
      this._look = CubismLook.create();
      if (opts.look.length) this._look.setParameters(opts.look);
      // Gaze aditif; target di-ease CubismTargetPoint.
      this.updateScheduler.addUpdatableList(
        new GatedUpdater(
          new CubismLookUpdater(this._look, this._dragManager),
          () => this.lookGate?.() ?? true,
          CubismUpdateOrder.CubismUpdateOrder_Drag,
        ),
      );
    }
    if (en.breath) {
      this._breath = CubismBreath.create();
      if (opts.breath.length) this._breath.setParameters(opts.breath);
      // Napas aditif di atas motion; gate menahan saat frozen / motion
      // layer membawa kurva breath sendiri.
      this.updateScheduler.addUpdatableList(
        new GatedUpdater(
          new CubismBreathUpdater(this._breath),
          () => this.breathGate?.() ?? true,
          CubismUpdateOrder.CubismUpdateOrder_Breath,
        ),
      );
    }
    if (this._physics) {
      this.updateScheduler.addUpdatableList(new CubismPhysicsUpdater(this._physics));
    }
    if (this._pose) {
      this.updateScheduler.addUpdatableList(new CubismPoseUpdater(this._pose));
    }
    this.updateScheduler.sortUpdatableList();
  }

  /** Settle simulasi fisika seketika (panggil sekali setelah load /
   * setelah teleport pose) — tanpa ini rambut/ekor berayun liar detik awal. */
  stabilizePhysics(): void {
    const model = this.getModel();
    if (model && this._physics) this._physics.stabilization(model);
  }

  /** Pipeline dua fase resmi — dipanggil tiap frame sebelum draw. */
  update(dtSeconds: number): void {
    const model = this.getModel();
    if (!model) return;
    model.loadParameters();
    this._motionUpdated = false;
    if (this._motionManager.isFinished()) {
      this.startIdleIfAvailable();
    } else {
      this._motionUpdated = this._motionManager.updateMotion(model, dtSeconds);
    }
    model.saveParameters();
    this.updateScheduler.onLateUpdate(model, dtSeconds);
    model.update();
  }

  /** Grup "Idle" dicari by-name dari manifest; model tanpa grup ini
   * dibiarkan diam (pose + efek saja) — tidak ada nama grup yang dipaksakan. */
  private startIdleIfAvailable(): void {
    const s = this._setting;
    if (!s) return;
    for (let i = 0; i < s.getMotionGroupCount(); i++) {
      if (s.getMotionGroupName(i) !== "Idle") continue;
      const count = s.getMotionCount("Idle");
      if (count > 0) {
        const idx = Math.floor(Math.random() * count);
        void this.startMotionGroup("Idle", idx, MotionPriority.Idle);
      }
      return;
    }
  }

  /** Putar motion native: lazy-fetch per manifest, cache milik model,
   * protokol prioritas resmi (reserveMotion → startMotionPriority).
   * Return -1 = kalah prioritas / grup tidak ada / gagal load (reservasi
   * sudah direset); 1 = motion masuk antrean. (Port vendored mengembalikan
   * objek CubismMotionQueueEntry dari startMotionPriority — kontrak adapter
   * diratakan jadi angka; polling per-entry belum dibutuhkan.) */
  async startMotionGroup(
    group: string,
    index: number,
    priority: number,
  ): Promise<number> {
    const s = this._setting;
    if (!s) return -1;
    let known = false;
    for (let i = 0; i < s.getMotionGroupCount(); i++) {
      if (s.getMotionGroupName(i) === group) {
        known = true;
        break;
      }
    }
    if (!known) {
      console.warn(`[Live2DUserModel] grup motion "${group}" tidak ada di manifest`);
      return -1;
    }
    if (index < 0 || index >= s.getMotionCount(group)) return -1;
    if (priority !== MotionPriority.Force && !this._motionManager.reserveMotion(priority)) {
      return -1;
    }
    if (priority === MotionPriority.Force) {
      this._motionManager.setReservePriority(priority);
    }
    const key = `${group}_${index}`;
    let motion = this._motionCache.get(key);
    if (!motion) {
      const file = s.getMotionFileName(group, index);
      if (!file) {
        this._motionManager.setReservePriority(MotionPriority.None);
        return -1;
      }
      let buf: ArrayBuffer;
      try {
        const res = await fetch(this._baseUrl + file);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        buf = await res.arrayBuffer();
      } catch (e) {
        console.warn(`[Live2DUserModel] fetch motion ${file} gagal`, e);
        this._motionManager.setReservePriority(MotionPriority.None);
        return -1;
      }
      motion = this.loadMotion(buf, buf.byteLength, key, undefined, undefined, s, group, index);
      if (!motion) {
        this._motionManager.setReservePriority(MotionPriority.None);
        return -1;
      }
      // beri tahu motion kurva mana milik blink/lipsync supaya auto-blink
      // berdiri turun saat motion menganimasikan mata
      motion.setEffectIds(this._eyeBlinkIds, this._lipSyncIds);
      this._motionCache.set(key, motion);
    }
    const started = this._motionManager.startMotionPriority(motion, false, priority);
    return started ? 1 : -1;
  }

  /** Putar ekspresi by-name dari manifest. 5-r.5: ekspresi TANPA prioritas —
   * startMotion baru akan cross-fade menimpa yang lama. Return false bila
   * nama tidak ada di manifest / gagal load. */
  async playExpression(name: string): Promise<boolean> {
    const s = this._setting;
    if (!s) return false;
    let file: string | null = null;
    for (let i = 0; i < s.getExpressionCount(); i++) {
      if (s.getExpressionName(i) === name) {
        file = s.getExpressionFileName(i);
        break;
      }
    }
    if (!file) return false;
    let expr = this._expressionCache.get(name);
    if (!expr) {
      let buf: ArrayBuffer;
      try {
        const res = await fetch(this._baseUrl + file);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        buf = await res.arrayBuffer();
      } catch (e) {
        console.warn(`[Live2DUserModel] fetch ekspresi ${file} gagal`, e);
        return false;
      }
      expr = this.loadExpression(buf, buf.byteLength, name);
      if (!expr) return false;
      this._expressionCache.set(name, expr);
    }
    // autoDelete=false: cache milik model, dihapus eksplisit saat dispose.
    this._expressionManager.startMotion(expr, false);
    return true;
  }

  /** Target look/gaze (view coords ±1) — di-ease CubismTargetPoint. */
  setLookTarget(x: number, y: number): void {
    this.setDragging(x, y);
  }

  /** Teardown lengkap: cache motion/ekspresi dihapus eksplisit (Core-side
   * tidak di-GC), scheduler dilepas, baru rantai release() base dijalankan. */
  dispose(): void {
    for (const m of this._motionCache.values()) ACubismMotion.delete(m);
    this._motionCache.clear();
    for (const e of this._expressionCache.values()) ACubismMotion.delete(e);
    this._expressionCache.clear();
    if (this._look) {
      CubismLook.delete(this._look);
      this._look = null;
    }
    this.updateScheduler.release();
    this.release();
  }
}
