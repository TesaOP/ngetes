/** Fase 6 — renderer nyata: Pixi 8 WebGL + Cubism Framework 5-r.5 (Core 6.0.1). */
import { CubismFramework } from "./cubism/live2dcubismframework";
import { CubismUserModel } from "./cubism/model/cubismusermodel";
import { CubismModelSettingJson } from "./cubism/cubismmodelsettingjson";
import { CubismMatrix44 } from "./cubism/math/cubismmatrix44";
import { CubismModelMatrix } from "./cubism/math/cubismmodelmatrix";

let frameworkStarted = false;
function ensureFramework() {
  if (frameworkStarted) return;
  CubismFramework.startUp({
    logFunction: (msg: string) => console.log("[CSM] " + msg),
    loggingLevel: 2 as any, // Info — cukup, Verbose terlalu berisik
  } as any);
  CubismFramework.initialize();
  frameworkStarted = true;
}

export class Live2DRenderer {
  private canvas: HTMLCanvasElement;
  private gl: WebGL2RenderingContext | WebGLRenderingContext;
  private userModel: CubismUserModel | null = null;
  private setting: CubismModelSettingJson | null = null;
  private baseDir = "";
  private modelMatrix: CubismModelMatrix | null = null;
  private proj = new CubismMatrix44();
  private mvp = new CubismMatrix44();
  private shaderPath = "/shaders/cubism/WebGL/";
  private textures: WebGLTexture[] = [];

  constructor(canvas: HTMLCanvasElement, gl?: WebGL2RenderingContext | WebGLRenderingContext) {
    this.canvas = canvas;
    this.gl = (gl ?? (canvas.getContext("webgl2") as any) ?? canvas.getContext("webgl")!) as any;
    if (!this.gl) throw new Error("WebGL tidak tersedia");
    ensureFramework();
  }

  async loadModel(model3Path: string): Promise<{ mocVersion: number; drawable: number; offscreen: number }> {
    const res = await fetch(model3Path);
    if (!res.ok) throw new Error(`fetch model3 ${res.status} ${model3Path}`);
    const buf = await res.arrayBuffer();
    this.setting = new CubismModelSettingJson(buf, buf.byteLength);
    this.baseDir = model3Path.slice(0, model3Path.lastIndexOf("/") + 1);

    // moc
    const mocFile = this.setting.getModelFileName();
    const mocBuf = await (await fetch(this.baseDir + mocFile)).arrayBuffer();
    const core = (globalThis as any).Live2DCubismCore;
    const mocVersion = core ? core.Version.csmGetMocVersion(mocBuf) : -1;

    this.userModel = new CubismUserModel();
    // CubismUserModel.loadModel expects buffer + flag consistency
    (this.userModel as any).loadModel(mocBuf, false);

    // physics / pose
    const phys = this.setting.getPhysicsFileName();
    if (phys) {
      const b = await (await fetch(this.baseDir + phys)).arrayBuffer();
      (this.userModel as any).loadPhysics(b, b.byteLength);
    }
    const poseFile = this.setting.getPoseFileName();
    if (poseFile) {
      const b = await (await fetch(this.baseDir + poseFile)).arrayBuffer();
      (this.userModel as any).loadPose(b, b.byteLength);
    }

    // renderer
    const w = this.canvas.width, h = this.canvas.height;
    (this.userModel as any).createRenderer(w, h, 1);
    const renderer: any = (this.userModel as any).getRenderer();
    renderer.setIsPremultipliedAlpha(true);
    renderer.startUp(this.gl);
    renderer.loadShaders(this.shaderPath);

    // textures — tunggu sampai shader selesai? bind saja, draw akan skip kalau belum siap
    const texCount = this.setting.getTextureCount();
    for (let i = 0; i < texCount; i++) {
      const texPath = this.baseDir + this.setting.getTextureFileName(i);
      const img = new Image();
      img.crossOrigin = "anonymous";
      img.src = texPath;
      try { await img.decode(); } catch (e) { console.warn(`[Live2DRenderer] img decode gagal ${texPath}`, e); }
      console.log(`[Live2DRenderer] texture ${i} ${texPath} ${img.width}x${img.height}`);
      const tex = this.gl.createTexture()!;
      this.gl.bindTexture(this.gl.TEXTURE_2D, tex);
      this.gl.pixelStorei(this.gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, 1);
      this.gl.texImage2D(this.gl.TEXTURE_2D, 0, this.gl.RGBA, this.gl.RGBA, this.gl.UNSIGNED_BYTE, img);
      this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_MIN_FILTER, this.gl.LINEAR);
      this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_MAG_FILTER, this.gl.LINEAR);
      this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_WRAP_S, this.gl.CLAMP_TO_EDGE);
      this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_WRAP_T, this.gl.CLAMP_TO_EDGE);
      const err = this.gl.getError();
      if (err) console.warn(`[Live2DRenderer] glError setelah tex ${i}: ${err}`);
      renderer.bindTexture(i, tex);
      this.textures.push(tex);
    }

    // model matrix
    const model: any = (this.userModel as any).getModel?.() ?? (this.userModel as any)._model;
    if (model) {
      const cw = model.getCanvasWidth(), ch = model.getCanvasHeight();
      console.log(`[Live2DRenderer] canvas ${cw}x${ch} drawable ${model.getDrawableCount?.()}`);
      this.modelMatrix = new CubismModelMatrix(cw, ch);
    } else {
      this.modelMatrix = new CubismModelMatrix(1, 1);
    }

    // tunggu shader siap (Framework fetch 13 shader files async)
    const rendererAny: any = (this.userModel as any).getRenderer();
    for (let i = 0; i < 40; i++) {
      // akses internal: CubismShaderManager_WebGL._instance? coba via global
      const mgr: any = (rendererAny as any)._shaderManager ?? (this.gl as any).__shaderMgr;
      // fallback: cek via renderer loadShaders state — cukup tunggu fetch selesai
      await new Promise((r) => setTimeout(r, 100));
      // cek isShaderLoaded jika tersedia
      try {
        const sh: any = (await import("./cubism/rendering/cubismshader_webgl")).CubismShaderManager_WebGL;
        const inst = sh.getInstance?.();
        const shader = inst?.getShader?.(this.gl as any);
        if (shader?._isShaderLoaded) {
          console.log(`[Live2DRenderer] shader ready after ${i * 100}ms`);
          break;
        }
      } catch {}
      if (i === 39) console.warn("[Live2DRenderer] shader masih belum ready setelah 4s");
    }

    const drawable = (model as any)?.getDrawableCount?.() ?? (model as any)?.drawables?.count ?? 0;
    const offscreen = (model as any)?.getOffscreenCount?.() ?? (model as any)?.offscreens?.count ?? 0;
    return { mocVersion, drawable, offscreen };
  }

  /** Gambar satu frame — dipanggil dari rAF. */
  draw() {
    if (!this.userModel) return;
    const model: any = (this.userModel as any).getModel?.() ?? (this.userModel as any)._model;
    if (!model) return;
    model.loadParameters?.();
    model.update?.();
    model.saveParameters?.();

    const renderer: any = (this.userModel as any).getRenderer();
    if (!renderer) return;

    const w = (this.gl as any).drawingBufferWidth ?? this.canvas.width;
    const h = (this.gl as any).drawingBufferHeight ?? this.canvas.height;
    // clear default framebuffer ke warna latar Pixi (232,232,232) — biar readPixels bisa bedakan background vs model
    this.gl.bindFramebuffer(this.gl.FRAMEBUFFER, null);
    this.gl.viewport(0, 0, w, h);
    this.gl.clearColor(0.909, 0.909, 0.909, 1.0);
    this.gl.clear(this.gl.COLOR_BUFFER_BIT | this.gl.DEPTH_BUFFER_BIT);

    renderer.setRenderState(null, [0, 0, w, h]);

    this.proj.loadIdentity();
    // framing mirip LAppView: scale agar tinggi model ~80% canvas, center
    const scale = 1.45;
    this.proj.scale(scale, scale * (this.canvas.width / this.canvas.height));
    this.proj.translateY(-0.12);
    // modelMatrix sudah punya scaling dari canvas size
    if (this.modelMatrix) {
      this.mvp.loadIdentity();
      this.mvp.multiplyByMatrix(this.proj);
      this.mvp.multiplyByMatrix(this.modelMatrix);
      renderer.setMvpMatrix(this.mvp);
    } else {
      renderer.setMvpMatrix(this.proj);
    }
    renderer.drawModel(this.shaderPath);
  }

  getDrawableCount(): number {
    const m: any = (this.userModel as any)?.getModel?.() ?? (this.userModel as any)?._model;
    return m?.getDrawableCount?.() ?? m?.drawables?.count ?? 0;
  }

  destroy() {
    try { (this.userModel as any)?.release?.(); } catch {}
    for (const t of this.textures) try { this.gl.deleteTexture(t); } catch {}
    this.textures = [];
    this.userModel = null;
  }
}
