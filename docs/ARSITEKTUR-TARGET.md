# Arsitektur, Alur, dan Behavioral Modes — Project Live2D AI Companion

> **Status:** Baseline arsitektur untuk memulai ulang integrasi View → Engine.
>
> **Penting:** Dokumen ini adalah *target architecture / mental model*. Mungkin kamu perlu audit by code lagi biar sesuai karena dokumentasi yang saat ini agak berbeda.
>
> **Catatan repo (2026-09-19):** ini dokumen TARGET, bukan deskripsi kode
> aktual. Hasil audit gap target-vs-kode ada di
> [`docs/ARSITEKTUR-GAP.md`](ARSITEKTUR-GAP.md) — kerja implementasi
> mengikuti fase di sana, satu boundary per waktu. Diketahui bertentangan
> dengan `MODES.md` (aturan teardown penuh saat pindah mode) di bagian
> §35–36 context switching — rekonsiliasi dilakukan per fase, jangan
> melanggar docs binding diam-diam.

---

## 1. Prinsip Utama

Project terdiri dari empat lapisan besar:

```text
VIEW / UI
    ↓
BEHAVIOR / AGENT
    ↓
ENGINE
    ↓
LIVE2D RUNTIME / RENDERER
```

Tanggung jawab:

- **View:** menerima input, menampilkan state, memilih konteks/surface.
- **Behavior:** menentukan apa yang seharusnya dilakukan.
- **Engine:** menerjemahkan keputusan menjadi aksi karakter.
- **Live2D Runtime:** mengeksekusi model dan rendering.

Aturan:

1. View tidak mengatur parameter Cubism secara langsung.
2. LLM/Behavior tidak menulis parameter Cubism secara langsung.
3. Renderer tidak mengambil keputusan AI.
4. Mode tidak membutuhkan renderer yang berbeda.
5. Execution state dan speech state tidak boleh otomatis dianggap sama.
6. Sebelum implementasi, audit actual repo terlebih dahulu.

---

# 2. Mental Model Produk

Ada tiga pengalaman utama:

```text
                         APPLICATION
                              │
             ┌────────────────┼────────────────┐
             │                │                │
             ▼                ▼                ▼
         CHAT VIEW        VTUBER VIEW     ASSISTANT VIEW
             │                │                │
             ▼                ▼                ▼
        COMPANION          VTUBER            WORKER
        / PET-LIKE         BEHAVIOR          BEHAVIOR
```

## Chat View

Chat View adalah pengalaman **Companion / PET-like**.

Karakter:

- diajak ngobrol;
- punya personality;
- merespons user;
- dapat berbicara;
- dapat melakukan motion/expression;
- dapat memiliki proactive behavior.

## VTuber View

Karakter diposisikan sebagai streamer/performer.

Event utama:

- Audience
- Donation
- Operator

## Assistant / Harness View

Orientasinya adalah pekerjaan/task.

Agent dapat:

- menjalankan task;
- memakai tools;
- meminta approval;
- melakukan cancel;
- melakukan queue/park;
- memodifikasi task.

---

# 3. Behavioral Identity vs View

Jangan menyamakan View dengan behavioral identity.

Pemetaan konseptual:

```text
Chat View
    ↓
COMPANION

VTuber View
    ↓
VTUBER

Assistant View
    ↓
WORKER
```

Jika Harness memiliki dua surface:

```text
HARNESS
├── Chat Dock
│     └── COMPANION
│
└── Worker input
      └── WORKER
```

Kesimpulan:

> **Chat View = Companion/PET-like behavioral surface.**

Tidak perlu membuat Chat Engine dan PET Engine terpisah hanya karena nama View berbeda.

---

# 4. Tiga Behavioral Context

## 4.1 COMPANION

Tujuan: interaksi natural antara user dan karakter.

```text
User
 ↓
Conversation
 ↓
Companion
 ↓
LLM
 ↓
Decision
 ↓
Motion / Expression / Speech
```

## 4.2 VTUBER

Tujuan: karakter sebagai streamer.

```text
Audience / Donation / Operator
              ↓
       Event Policy
              ↓
        VTuber Behavior
              ↓
             LLM
              ↓
       Response / Action
```

## 4.3 WORKER

Tujuan: menyelesaikan pekerjaan.

```text
Task
 ↓
Worker Agent
 ↓
LLM
 ↓
Tools
 ↓
Approval / More tools
 ↓
Result
```

Live2D hanya merepresentasikan state/action agent; task state tetap berada di Worker.

---

# 5. End-to-End Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                         VIEW / UI                            │
│ Chat · VTuber · Assistant/Harness                            │
└─────────────────────────────┬────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                     BEHAVIOR / AGENT                         │
│ Companion · VTuber Behavior · Worker Behavior                │
└─────────────────────────────┬────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                         ENGINE                               │
│ Motion · Expression · Parameters · Speech · Model State      │
└─────────────────────────────┬────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                    LIVE2D RUNTIME                            │
│ Adapter · Cubism Framework/Core · PixiJS/WebGL               │
└──────────────────────────────────────────────────────────────┘
```

---

# 6. Chat Flow

```text
Chat View
   │
   ▼
User message
   │
   ▼
Companion Behavior
   │
   ▼
LLM
   │
   ▼
Decision / Response
   │
   ├── Text
   ├── Emotion
   ├── Motion
   ├── Expression
   └── Speech
          │
          ▼
        Engine
          │
          ├── Motion
          ├── Expression
          ├── Parameters
          └── Speech
          │
          ▼
      Live2D Runtime
```

## Companion concurrency

### THINKING + new input

Target behavior:

**MERGE**

```text
Request A
   ↓
THINKING
   ↓
User sends B
   ↓
A + B
   ↓
single continued thinking flow
```

### SPEAKING + new input

Target behavior:

**PREEMPT**

```text
A sedang berbicara
       ↓
User mengirim B
       ↓
speech/chain A dipreempt
       ↓
process B
```

Speech yang dipotong tidak otomatis dianggap completed.

---

# 7. VTuber Flow

```text
VTUBER VIEW
    │
    ├──────────────┬───────────────┐
    ▼              ▼               ▼
 Audience       Donation        Operator
    │              │               │
    ▼              ▼               ▼
 Suppression      FIFO             FIFO
    │              │               │
    └──────────────┼───────────────┘
                   ▼
            VTuber Behavior
                   │
                   ▼
                  LLM
                   │
                   ▼
          Speech / Motion / Expression
```

## Audience

Audience bersifat noisy/high-volume.

Target behavior:

```text
event
 ↓
duplicate?
 ├─ yes → DROP
 └─ no
     ↓
cooldown?
 ├─ yes → DROP
 └─ no
     ↓
accept
```

## Donation

Donation memakai queue:

```text
Donation
   ↓
FIFO Queue
   ↓
Active item
   ↓
LLM
   ↓
Speech
   ↓
Done
   ↓
Next item
```

Target queue yang pernah ditetapkan:

- FIFO;
- maksimum 20;
- item baru ketika penuh ditolak;
- tidak silent-evict item lama.

## Operator

Operator adalah event class tersendiri:

```text
Operator
   ↓
Operator Queue
   ↓
VTuber Behavior
   ↓
Response
```

Donation dan Operator berbagi active scheduling slot.

Jika sebuah item sedang aktif, item tersebut tidak dipreempt oleh class lain.

Jika donation dan operator sama-sama menunggu, scheduling precedence yang pernah dikunci adalah Donation sebelum Operator.

---

# 8. Worker / Assistant Flow

```text
Assistant View
      ↓
Worker Input
      ↓
Worker Agent
      ↓
┌─────┼───────────────────┐
│     │                   │
LLM  Tool              Approval
│     │                   │
└─────┼───────────────────┘
      ↓
More work / Result
```

Worker berbeda dari Companion karena fokusnya adalah **task completion**, bukan percakapan natural.

---

# 9. Worker Task Identity

Worker tidak idealnya hanya mempunyai:

```text
busy = true
```

Tetapi:

```text
activeTask
parkedTasks[]
taskId
```

Contoh:

```text
ACTIVE
┌──────────────┐
│ t_1 RUNNING  │
└──────────────┘

PARKED
┌──────────────┐
│ t_2          │
├──────────────┤
│ t_3          │
├──────────────┤
│ t_4          │
└──────────────┘
```

Target queue:

- FIFO;
- maksimum 20;
- task baru tidak mematikan active task;
- overflow ditolak dengan feedback eksplisit.

---

# 10. Worker Pause / Approval

Approval pause tetap mempertahankan ownership task:

```text
RUNNING
   ↓
WAITING APPROVAL
   ↓
PAUSED
```

Saat paused:

```text
ACTIVE SLOT = task tersebut
```

Task baru tetap park/queue.

---

# 11. Worker Cancel

Gunakan task identity:

```text
cancel(taskId)
```

Target dapat berupa:

- active task;
- paused task;
- parked task.

Cancel bersifat cooperative.

Penting:

> Cancel tidak berarti side effect yang sudah terjadi otomatis di-undo.

---

# 12. Worker Modify

Modify active task bukan sekadar task independen baru.

Target ordering:

```text
A → A' → B → C
```

Jika A running:

```text
A RUNNING
   ↓
cancel requested
   ↓
A terminal
   ↓
A' active
```

Jika A paused:

```text
A PAUSED
   ↓
A terminal
   ↓
A' active
```

Replacement mewarisi posisi A.

---

# 13. Harness: Dua Lane

Harness bukan dua engine.

```text
                         HARNESS
                            │
                ┌───────────┴───────────┐
                │                       │
                ▼                       ▼
        Companion Lane             Worker Lane
                │                       │
            AgentBrain              Agent Loop
                │                       │
            /api/chat             /api/assistant/*
```

Input surface menentukan lane:

```text
Chat Dock
   → Companion

Worker/#as-input
   → Worker
```

Jika boundary surface sudah cukup jelas, tidak perlu menambah IntentRouter hanya untuk membedakan dua lane.

---

# 14. Execution State Isolation

Companion dan Worker harus mempunyai state yang berbeda.

```text
COMPANION
├── conversation history
├── busy/request state
├── request generation
├── conversational chain
└── companion context

WORKER
├── task context
├── activeTask
├── parkedTasks
├── approvals
├── cancellation
└── task identity
```

Worker tidak boleh:

- mengubah Companion history;
- membatalkan Companion request;
- mengubah Companion thinking state.

Companion tidak boleh:

- membatalkan Worker task;
- mengubah Worker queue;
- mengambil alih Worker task state.

Boleh berbagi karakter/engine; tidak boleh mencampur execution state.

---

# 15. Speech Ownership

Speech adalah resource yang berbeda dari execution.

```text
Execution
    ↓
menghasilkan speech intent

Speech
    ↓
menghasilkan audio
```

Jangan otomatis menyamakan:

```text
speech done == task done
```

kecuali flow tertentu memang sengaja mengikat keduanya.

Potential producers:

```text
Companion
VTuber Audience
VTuber Donation
VTuber Operator
Worker Actor
Direct/App fallback
```

Target boundary:

```text
Speech Producer
      ↓
Speech Policy / Ownership
      ↓
TTS / Audio
```

Policy dapat menghasilkan:

```text
ALLOW
SUPPRESS
PREEMPT
QUEUE
CANCEL
```

---

# 16. Speech Conflict Rules

Behavioral contract yang pernah dikunci:

### Companion explicit user input vs Worker speech

Companion user input dapat memenangkan audio.

```text
Companion user
      >
Worker decorative speech
```

### Worker speech vs live Companion chain

Worker tidak boleh memotong Companion yang sedang berbicara.

```text
Companion speaking
       +
Worker speech
       ↓
Worker SUPPRESS
```

### Worker vs Worker

Worker speech harus serialized.

Tidak boleh hanya mengandalkan last-claim-wins.

---

# 17. Proactive Behavior

Proactive event berbeda dari explicit user input.

Contoh:

```text
idle
user away
user returned
mood event
```

Target:

```text
reactEvent()
    ↓
proactiveAllowed?
    │
 ┌──┴──┐
 NO   YES
 │      │
DROP   process
```

Gate harus terjadi sebelum:

- LLM request;
- director processing;
- side effect;
- speech.

---

# 18. Proactive Context Policy

Secara behavioral target:

### Companion aktif

Proactive behavior boleh berjalan sesuai quiet/idle rules.

### VTuber aktif

Companion-style idle/away/return/mood proactive sebaiknya ditekan karena VTuber memiliki event model sendiri.

### Worker RUNNING / PAUSED

Companion proactive sebaiknya ditekan agar tidak mengganggu task.

### Brain OFF

Companion proactive tidak berjalan.

> Ini adalah target policy dari arsitektur sebelumnya. Repo hasil revert harus diverifikasi ulang sebelum mengklaim sudah memiliki behavior tersebut.

---

# 19. Engine Layer

Engine adalah boundary antara behavior dan Live2D.

Contoh output behavior:

```text
{
  text: "Hai!",
  emotion: "happy",
  motion: "nod",
  expression: "smile",
  speech: true
}
```

Engine mengubahnya menjadi aksi:

```text
emotion
   ↓
expression mapping

motion
   ↓
motion mapping

lookAtUser
   ↓
parameter mapping

speech
   ↓
speech adapter
```

Engine tidak perlu mengetahui detail View.

---

# 20. Semantic Motion

AI/Behavior sebaiknya menggunakan semantic action:

```text
nod
wave
greet
think
happy
sad
```

bukan langsung:

```text
motion_01
motion_02
```

Motion catalog dapat menyediakan:

- semantic verb;
- compatible emotions;
- duration;
- native motion ID.

Engine kemudian melakukan mapping ke model yang sedang digunakan.

---

# 21. Expression

Expression juga menggunakan semantic abstraction:

```text
happy
sad
surprised
angry
neutral
```

Jika model hanya mempunyai ID opaque:

```text
exp_01
exp_02
```

jangan mengarang arti ID tersebut tanpa metadata/evidence.

---

# 22. Parameter API

Semantic role dipetakan ke parameter model:

```text
Semantic Role
     ↓
Model-specific mapping
     ↓
Parameter API
     ↓
Cubism parameter
```

Contoh:

```text
headYaw
   ↓
AngleX

bodyYaw
   ↓
BodyAngleX
```

Behavior tidak boleh hard-code semua ID model.

---

# 23. Parameter Arbitration

Banyak sumber dapat memengaruhi parameter:

```text
Native Motion
Idle
Mouse Follow
AI Override
Eye Blink
Breath
Physics
```

Harus ada ownership/ordering yang jelas:

```text
Sources
   ↓
Parameter Arbitration
   ↓
Final parameter state
   ↓
Cubism Core
```

Jangan membiarkan banyak writer menulis parameter yang sama tanpa aturan.

---

# 24. Mouse Follow

Flow:

```text
Pointer
   ↓
Normalize x/y
   ↓
Mouse Follow Gain
   ↓
Semantic target
   ↓
Role Clamp
   ↓
Parameter API
   ↓
Arbitration / final write
   ↓
Cubism
```

Gain dapat dikonfigurasi:

```text
DEFAULT
STRONG
WILD
CUSTOM
```

Gain hanya mengatur respons; normalization, clamp, dan safety boundary tetap dipertahankan.

---

# 25. Idle Motion vs Mouse Follow

Keduanya berbeda:

```text
Mouse Follow
= response terhadap pointer

Idle Motion
= autonomous baseline movement
```

Keduanya dapat aktif bersamaan, tetapi ownership/prioritas harus jelas.

---

# 26. Framework Effects

Contoh:

- EyeBlink;
- Breath;
- Physics.

Jangan membuat duplicate implementation jika framework sudah menyediakan effect.

Jika engine membutuhkan kontrol:

```text
Engine
 ↓
effect configuration / enable
 ↓
Framework effect
```

---

# 27. Eye Blink Ownership

Idle blink tidak boleh mempunyai dua writer independen:

```text
Framework EyeBlink
       +
Custom idle-blink
```

Jika dua writer aktif, dapat terjadi:

- frequency doubling;
- phase collision;
- hasil blink tidak natural.

Ownership harus eksplisit.

---

# 28. Breath dan Update Order

Breath dapat dipengaruhi urutan update.

Contoh masalah:

```text
Breath
  ↓
Engine absolute write
  ↓
Breath effect tertimpa
```

Karena itu update order harus diverifikasi secara runtime.

Target konseptual:

```text
Load parameters
    ↓
Motion
    ↓
Framework effects
    ↓
Engine overrides / arbitration
    ↓
Late additive effects jika diperlukan
    ↓
Core.update()
```

Urutan final harus mengikuti implementasi aktual yang telah diverifikasi, bukan asumsi dokumen.

---

# 29. Live2D Runtime

Target stack:

```text
Engine
  ↓
Live2D Adapter
  ↓
Cubism Framework
  ↓
Cubism Core
  ↓
PixiJS 8 / WebGL
  ↓
Canvas
```

Runtime bertanggung jawab terhadap:

- model loading;
- capability;
- parameter;
- motion;
- expression;
- physics;
- framework effects;
- texture;
- mask;
- blend;
- rendering;
- compositing.

---

# 30. Renderer Responsibility

Renderer bertanggung jawab terhadap:

- GPU resource;
- texture;
- mesh;
- draw order;
- opacity;
- mask;
- blend;
- shader;
- compositing;
- WebGL/canvas.

Renderer tidak bertanggung jawab terhadap:

- personality;
- conversation intent;
- VTuber event policy;
- Worker queue;
- proactive decision;
- task semantics.

---

# 31. Model Capability

Setiap model dapat mempunyai capability berbeda:

```text
Model
├── Parameters
├── Motions
├── Expressions
├── Physics
├── Drawables
├── Textures
└── Version metadata
```

Engine/AI sebaiknya melihat abstraction:

```text
Model Capability
      ↓
Engine-safe Capability
      ↓
Behavior
```

Bukan detail renderer seperti drawable index atau texture slot.

---

# 32. Request Lifecycle

Companion request ideal:

```text
INPUT
  ↓
CLAIM
  ↓
LOAD CONTEXT
  ↓
LLM
  ↓
DIRECTOR
  ↓
ENGINE
  ↓
SPEECH / MOTION
  ↓
DONE
```

Request perlu:

- cancellation;
- timeout;
- generation/stale protection;
- cleanup.

---

# 33. Stale Request Protection

Contoh:

```text
Request A
   ↓
await network
   ↓
Request B menjadi current
   ↓
A kembali terlambat
```

A tidak boleh menulis state milik B.

```text
generation A != current generation
        ↓
discard A
```

---

# 34. Model Loading Race

Hal yang sama berlaku untuk model loading:

```text
Load A
  ↓
await

Load B
  ↓
B menjadi current

A kembali terlambat
  ↓
A tidak boleh menggantikan B
```

Setiap continuation setelah await harus memeriksa generation/identity.

Stale resource milik A boleh perlu dibersihkan, tetapi jangan mengubah state model B.

---

# 35. Context Switching

Saat pindah View:

```text
Chat
 ↓
VTuber
```

idealnya yang berubah terutama:

- input source;
- behavior context;
- event producers;
- policy;
- speech policy;
- proactive policy.

Tidak otomatis berarti:

```text
destroy renderer
destroy model
destroy engine
```

Jika runtime dapat tetap digunakan, pertahankan.

---

# 36. Mode Transition Checklist

Setiap perpindahan context harus menjawab:

1. Apa yang tetap hidup?
2. Apa yang dihentikan?
3. Apa yang disuppress?
4. Apa yang di-reset?
5. Apakah speech aktif dihentikan?
6. Apakah Worker task tetap berjalan?
7. Apakah Companion history dipertahankan?
8. Apakah VTuber queue tetap berjalan?

Jangan menyelesaikan semua pertanyaan dengan satu `resetEverything()`.

---

# 37. Data Flow

Control flow:

```text
View
 ↓
Behavior
 ↓
Engine
 ↓
Runtime
```

Observation flow dapat kembali:

```text
Runtime
 ↓
Engine
 ↓
Behavior
```

Contoh observation:

- model capability;
- available motions;
- available expressions;
- parameter ranges.

Jangan mengirim seluruh internal renderer ke AI tanpa kebutuhan.

---

# 38. Debugging Principle

Selalu cari **titik pertama** di mana expected state berbeda dari actual state.

```text
USER ACTION
    ↓
VIEW
    ↓
EVENT
    ↓
BEHAVIOR
    ↓
DECISION
    ↓
ENGINE
    ↓
MOTION / PARAMETER
    ↓
CUBISM
    ↓
RENDER
```

Jika behavior salah, audit Behavior.

Jika motion salah, audit Engine/Motion.

Jika parameter salah, audit Mapping/API/Arbitration.

Jika visual salah, audit Runtime/Renderer.

Jangan langsung mengubah renderer untuk masalah AI.

---

# 39. Contoh Debug Mouse Follow

```text
Pointer event
 ↓
Canvas hit?
 ↓
Normalization
 ↓
Gain
 ↓
Target
 ↓
Role mapping
 ↓
Clamp
 ↓
Arbitration
 ↓
Cubism
 ↓
Render
```

Cari titik pertama yang salah.

---

# 40. Contoh Debug Companion Speech

```text
User input
 ↓
Companion accepted?
 ↓
Thinking?
 ↓
LLM response?
 ↓
Directive?
 ↓
Engine command?
 ↓
Speech producer?
 ↓
Speech policy?
 ↓
TTS?
 ↓
Audio?
```

Jangan langsung menyalahkan TTS jika request belum mencapai speech stage.

---

# 41. Contoh Debug Worker

```text
Task input
 ↓
Slot claimed?
 ↓
taskId created?
 ↓
active/parked?
 ↓
Agent loop
 ↓
Tool
 ↓
Approval?
 ↓
Terminal?
 ↓
Queue drain?
 ↓
Actor feedback?
```

Execution dan speech diperiksa secara terpisah.

---

# 42. Anti-Patterns

## View langsung menulis parameter

```text
button
 ↓
model.AngleX = 30
```

Hindari.

## LLM langsung menulis Cubism

```text
LLM
 ↓
ParamAngleX = ...
```

Hindari.

## Setiap mode mempunyai renderer sendiri

Hindari kecuali ada kebutuhan teknis yang terbukti.

## Global state untuk Companion dan Worker

Jangan mencampur:

```text
history
busy
cancel
task
```

dalam satu state global.

## IntentRouter tanpa kebutuhan

Jika surface sudah menentukan:

```text
Chat → Companion
Assistant → Worker
VTuber → VTuber
```

tidak perlu menambah router hanya untuk memecahkan masalah yang sudah terselesaikan oleh boundary.

## Banyak writer parameter tanpa ownership

Ini berpotensi menghasilkan motion regression.

## Proactive event langsung memanggil LLM

Harus melewati policy gate terlebih dahulu.

## Reset seluruh aplikasi ketika pindah View

Context switch tidak otomatis berarti full destruction.

---

# 43. Single Engine Principle

Target:

```text
Companion ─┐
VTuber ────┼──→ ONE ENGINE ─→ LIVE2D RUNTIME
Worker ────┘
```

Bukan:

```text
Companion → Engine A
VTuber    → Engine B
Worker    → Engine C
```

Satu engine memungkinkan:

- model yang sama;
- parameter system yang sama;
- motion system yang sama;
- expression system yang sama;
- renderer yang sama.

Perbedaan berada pada behavior/policy.

---

# 44. Shared vs Isolated

## Shared

Boleh shared:

- Live2D model;
- model loader;
- Cubism runtime;
- renderer;
- Parameter API;
- motion API;
- expression API;
- engine;
- capability metadata;
- speech adapter.

## Isolated

Sebaiknya terisolasi:

- Companion history/state;
- Worker task state;
- VTuber queues;
- request generation;
- cancellation;
- approval;
- proactive state;
- mode-specific event queues.

---

# 45. Urutan Integrasi Ulang

Karena project sengaja diulang dari sebelum integrasi View → Engine baru:

## Phase A — Audit View

Petakan:

```text
Chat
VTuber
Assistant/Harness
```

Untuk setiap View:

- input;
- output;
- state;
- endpoint;
- event;
- speech;
- motion;
- model access.

**Audit only.**

## Phase B — Audit Behavior

Petakan:

```text
Chat → Companion
VTuber → VTuber behavior
Assistant → Worker
```

Cari siapa yang:

- memanggil LLM;
- membuat directive;
- memanggil speech;
- memanggil motion;
- menulis parameter.

**Audit only.**

## Phase C — Tentukan Engine Boundary

Kelompokkan existing Live2D control:

```text
Motion
Expression
Parameter
Speech
Model
```

Tentukan titik adapter.

## Phase D — Integrasikan Companion

Mulai dari jalur sederhana:

```text
Chat
 ↓
Companion
 ↓
Engine
 ↓
Live2D
```

Pastikan stabil sebelum VTuber/Worker.

## Phase E — Integrasikan VTuber

```text
VTuber events
 ↓
VTuber Behavior
 ↓
same Engine
 ↓
same Live2D Runtime
```

## Phase F — Integrasikan Worker

```text
Worker
 ↓
Task lifecycle
 ↓
Engine actor feedback
 ↓
same Live2D Runtime
```

---

# 46. Golden Rules untuk Coding Agent

Sebelum mengubah kode:

1. Audit repo aktual.
2. Jangan menganggap S6/S7/Behavior Contract lama masih terimplementasi setelah revert.
3. Petakan View aktual.
4. Petakan Behavior aktual.
5. Petakan Engine boundary aktual.
6. Petakan Live2D boundary aktual.
7. Bedakan fakta kode dari target architecture.
8. Buat implementation plan sebelum perubahan besar.
9. Jangan membuat subsystem baru jika boundary existing sudah cukup.
10. Implementasikan satu boundary pada satu waktu.
11. Verifikasi runtime setelah setiap perubahan.
12. Jangan memperbaiki masalah satu layer dengan merombak layer lain tanpa evidence.

---

# 47. Final Mental Model

```text
                           APPLICATION
                                │
              ┌─────────────────┼─────────────────┐
              │                 │                 │
              ▼                 ▼                 ▼
          CHAT VIEW         VTUBER VIEW      ASSISTANT VIEW
              │                 │                 │
              ▼                 ▼                 ▼
         COMPANION           VTUBER            WORKER
          / PET-LIKE         BEHAVIOR          BEHAVIOR
              │                 │                 │
              └─────────────────┼─────────────────┘
                                │
                                ▼
                         BEHAVIOR OUTPUT
                                │
                                ▼
                           ONE ENGINE
                                │
                 ┌──────────────┼──────────────┐
                 │              │              │
                 ▼              ▼              ▼
              Motion       Expression      Speech
                 │              │              │
                 └──────────────┼──────────────┘
                                ▼
                         LIVE2D RUNTIME
                                │
                                ▼
                         CUBISM / PIXI
                                │
                                ▼
                              WEBGL
                                │
                                ▼
                            LIVE2D
```

## Kalimat inti

> **View menentukan konteks. Behavior menentukan tindakan. Engine menerjemahkan tindakan menjadi aksi karakter. Live2D Runtime mengeksekusinya.**

Tiga behavioral context utama:

```text
CHAT      → COMPANION / PET-LIKE
VTUBER    → STREAM / EVENT BEHAVIOR
ASSISTANT → WORKER / TASK BEHAVIOR
```

Semua dapat berbagi:

```text
ONE ENGINE
ONE LIVE2D RUNTIME
ONE RENDERING STACK
```

sementara execution state tetap terisolasi.

---

# 48. Status dan Batas Dokumen

Dokumen ini adalah **dasar pemahaman arsitektur sebelum integrasi ulang**.

Urutan kerja yang wajib:

```text
AUDIT ACTUAL REPO
       ↓
ACTUAL FLOW MAP
       ↓
COMPARE WITH THIS DOCUMENT
       ↓
IDENTIFY GAP
       ↓
IMPLEMENT ONE BOUNDARY
       ↓
RUNTIME VERIFY
```

