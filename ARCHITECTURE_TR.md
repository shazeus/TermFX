# TermFX Mimari Tasarim Dokumani

Bu repo, terminal icinden calisan, FFmpeg tabanli ve MCP uyumlu bir video editoru icin
uretime yakin bir baslangic mimarisi sunar. Kod iskeleti derlenebilir durumdadir ve
kritik parcalar gercek modullere ayrilmistir:

- timeline/kesme motoru: `src/core/timeline.rs`
- efekt modeli: `src/core/effect.rs`
- FFmpeg complex filtergraph builder: `src/render/ffmpeg.rs`
- MCP stdio JSON-RPC server: `src/mcp/server.rs`, `src/mcp/tools.rs`
- terminal UI: `src/tui/layout.rs`, `src/tui/timeline_widget.rs`

## 1. Mimari Kararlar ve Teknik Yigin

Secilen dil: Rust.

Neden Rust:

- FFmpeg gibi dis process veya ileride libav/MLT gibi native binding kullanan video
  sistemlerinde bellek guvenligi kritik.
- Timeline, render queue ve MCP server ayni proses icinde calisacagi icin veri
  yarislari ve backpressure dogru modellenmeli.
- `tokio` MCP stdio ve gelecekteki render job yonetimi icin uygun.
- `ratatui`/`crossterm` terminal arayuzunu platformlar arasi kurmak icin yeterince
  olgun.

Kullanilan stack:

- Dil/runtime: Rust 2024 edition
- Async runtime: Tokio
- TUI: Ratatui + Crossterm
- Render motoru: FFmpeg CLI ve complex filtergraph
- Proje formati: JSON, frame tabanli timeline
- MCP: stdio transport uzerinden JSON-RPC 2.0
- Serialization: Serde / serde_json
- Hata modeli: thiserror + anyhow

FFmpeg CLI bilincli secildi. Ilk surum icin en stabil ve debug edilebilir yol budur:
uretilen komut dry-run olarak gorulebilir, filtergraph test edilebilir, ileride
`RenderBackend` trait'i eklenerek libav veya MLT backend'i ayni timeline modelini
kullanabilir.

## 2. Ana Calisma Mantigi

TermFX iki motorun tek proje modeli uzerinde calismasi olarak tasarlandi.

### 2.1 Kurgu Motoru

Kurgu motoru `Timeline`, `Track`, `Clip` ve `ClipSource` tiplerinden olusur.

Temel karar: tum timeline zamani frame ile tutulur, saniye sadece dis API sinirinda
kullanilir. MCP ve CLI saniye alabilir; proje icinde FPS'e gore frame'e cevrilir.

Desteklenen isler:

- media clip ekleme
- text clip ekleme
- kaynak range'e gore trim
- timeline range silme
- ripple delete
- video/audio track ayrimi
- clip opacity ve volume
- clip uzerinde efekt stack'i

Gercek kesme mantigi:

- `[start, end)` araligi silinir.
- Aralik bir clip'in ortasina denk gelirse clip ikiye bolunur.
- `ripple=true` ise sonradan gelen clip'ler sola kaydirilir.
- Kaynak trim offset'i korunur; sag parcada `trim_start_frame` ileri alinir.

Kod: `Timeline::remove_timeline_range` ve `Timeline::trim_clip_to_source_range`.

### 2.2 Efekt / Compositor Motoru

Efekt motoru clip bazli stack mantigiyla calisir. Her `Clip` bir veya daha fazla
`EffectInstance` tasir.

Mevcut efektler:

- `BlackWhite`: FFmpeg `hue=s=0`
- `Glitch`: `rgbashift` + `noise`
- `FadeIn`: alpha fade-in
- `FadeOut`: alpha fade-out
- `SShake`: After Effects `s_shake` benzeri sin/cos tabanli crop jitter
- `TextOverlay`: `drawtext` ile video uzerine yazi bindirme

`s_shake` yaklasimi:

1. Clip hedef cozunurlugun biraz ustune scale edilir.
2. `crop` filtresinin `x` ve `y` ifadeleri zamanla sin/cos uzerinden oynatilir.
3. Amplitude, frequency ve seed parametreleri MCP uzerinden degistirilebilir.

Bu yapi After Effects'teki keyframe mantigina genisletilebilir. `TransformKeyframe`
tipi bunun icin eklendi; sonraki adim lineer/cubic interpolation ile `overlay` veya
`crop` expression uretmektir.

### 2.3 Sequencer ve Compositor Cakismadan Nasil Calisir?

Sequencer sadece "ne zaman, hangi kaynak, ne kadar sure" sorularini cevaplar.
Compositor ise "bu clip nasil gorunecek, hangi filtreler uygulanacak, hangi track
ustte" sorularini cevaplar.

FFmpeg builder bu iki bilgiyi tek grafikte birlestirir:

1. Her clip icin source trim yapilir.
2. Clip PTS'i once `PTS-STARTPTS` ile sifirlanir.
3. Scale/pad/format uygulanir.
4. Efekt stack sirayla eklenir.
5. Clip timeline start frame'ine gore `setpts=PTS+start/TB` ile konumlandirilir.
6. Track ve start siralamasina gore base layer uzerine `overlay` edilir.
7. Audio stream'ler `atrim`, `adelay`, `volume`, `amix` ile karistirilir.

Bu sayede Premiere tipi cut/slice ile After Effects tipi overlay/filter ayni render
komutuna donusur.

### 2.4 Render ve Arka Plan Isleri

Mevcut kod `FfmpegCommand` uretir:

- `display_shell()` dry-run/debug icindir.
- `spawn_and_wait()` FFmpeg'i calistirir.

Uretim yolunda bunun ustune `RenderQueue` eklenmelidir:

- Tokio task veya dedicated thread ile FFmpeg process'i baslatilir.
- FFmpeg `-progress pipe:2` veya `stderr` parse edilerek `RenderProgress` guncellenir.
- TUI thread'i sadece state okur; render thread'i UI'yi bloklamaz.
- Iptal icin child process'e SIGTERM, timeout'ta SIGKILL uygulanir.

`src/render/progress.rs` bu yapinin veri modelini icerir.

## 3. MCP Entegrasyonu

MCP tarafinda stdio transport kullanildi. Resmi MCP 2025-11-25 dokumanina gore stdio
transport JSON-RPC mesajlarini stdin/stdout uzerinden tasir; mesajlar newline ile
ayrilir ve stdout'a protokol disi veri yazilmamalidir. Bu nedenle loglar stderr'e
yazilir.

### 3.1 El Sikisma

Tipik handshake:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-11-25",
    "capabilities": {},
    "clientInfo": {
      "name": "AI Host",
      "version": "1.0.0"
    }
  }
}
```

TermFX yaniti:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2025-11-25",
    "capabilities": {
      "tools": {
        "listChanged": false
      }
    },
    "serverInfo": {
      "name": "termfx",
      "version": "0.1.0"
    }
  }
}
```

Ardindan client sunu gonderir:

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/initialized"
}
```

Sonrasinda `tools/list` ve `tools/call` kullanilir.

### 3.2 Tool Semalari

Tool listesi `src/mcp/tools.rs` icinde JSON Schema olarak uretilir.

`list_media`

```json
{
  "name": "list_media",
  "inputSchema": {
    "type": "object",
    "properties": {},
    "additionalProperties": false
  }
}
```

`append_media`

```json
{
  "name": "append_media",
  "inputSchema": {
    "type": "object",
    "properties": {
      "media_id": { "type": "string", "format": "uuid" },
      "track": { "type": "integer", "minimum": 0, "default": 0 },
      "start_seconds": { "type": "number", "minimum": 0, "default": 0 },
      "duration_seconds": { "type": "number", "exclusiveMinimum": 0 }
    },
    "required": ["media_id", "duration_seconds"],
    "additionalProperties": false
  }
}
```

`cut_video`

```json
{
  "name": "cut_video",
  "inputSchema": {
    "type": "object",
    "properties": {
      "mode": { "type": "string", "enum": ["remove_range", "trim_clip"] },
      "clip_id": { "type": "string", "format": "uuid" },
      "start_seconds": { "type": "number", "minimum": 0 },
      "end_seconds": { "type": "number", "exclusiveMinimum": 0 },
      "ripple": { "type": "boolean", "default": true }
    },
    "required": ["start_seconds", "end_seconds"],
    "additionalProperties": false
  }
}
```

`apply_effect`

```json
{
  "name": "apply_effect",
  "inputSchema": {
    "type": "object",
    "properties": {
      "clip_id": { "type": "string", "format": "uuid" },
      "effect": {
        "type": "string",
        "enum": [
          "black_and_white",
          "glitch",
          "fade_in",
          "fade_out",
          "s_shake",
          "text_overlay"
        ]
      },
      "params": { "type": "object", "additionalProperties": true }
    },
    "required": ["clip_id", "effect"],
    "additionalProperties": false
  }
}
```

`smart_edit`

```json
{
  "name": "smart_edit",
  "inputSchema": {
    "type": "object",
    "properties": {
      "mode": { "type": "string", "enum": ["silence", "beat_sync"] },
      "threshold_db": { "type": "number", "default": -35 },
      "min_silence_seconds": { "type": "number", "default": 0.35 },
      "dry_run": { "type": "boolean", "default": true }
    },
    "required": ["mode"],
    "additionalProperties": false
  }
}
```

### 3.3 Ornek Tool Cagrilari

Bir medyayi timeline'a eklemek:

```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "method": "tools/call",
  "params": {
    "name": "append_media",
    "arguments": {
      "media_id": "00000000-0000-0000-0000-000000000000",
      "track": 0,
      "start_seconds": 0,
      "duration_seconds": 5
    }
  }
}
```

Bir timeline araligini ripple delete ile silmek:

```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "method": "tools/call",
  "params": {
    "name": "cut_video",
    "arguments": {
      "mode": "remove_range",
      "start_seconds": 12.5,
      "end_seconds": 14.2,
      "ripple": true
    }
  }
}
```

After Effects tarzi `s_shake` eklemek:

```json
{
  "jsonrpc": "2.0",
  "id": 11,
  "method": "tools/call",
  "params": {
    "name": "apply_effect",
    "arguments": {
      "clip_id": "00000000-0000-0000-0000-000000000000",
      "effect": "s_shake",
      "params": {
        "amplitude_px": 18,
        "frequency_hz": 10,
        "seed": 0.4
      }
    }
  }
}
```

Text overlay eklemek:

```json
{
  "jsonrpc": "2.0",
  "id": 12,
  "method": "tools/call",
  "params": {
    "name": "apply_effect",
    "arguments": {
      "clip_id": "00000000-0000-0000-0000-000000000000",
      "effect": "text_overlay",
      "params": {
        "text": "FINAL CUT",
        "x": 120,
        "y": 80,
        "font_size": 56,
        "color": "white",
        "start_seconds": 0,
        "duration_seconds": 2.5
      }
    }
  }
}
```

Sessizlik analizi icin plan olusturmak:

```json
{
  "jsonrpc": "2.0",
  "id": 13,
  "method": "tools/call",
  "params": {
    "name": "smart_edit",
    "arguments": {
      "mode": "silence",
      "threshold_db": -35,
      "min_silence_seconds": 0.35,
      "dry_run": true
    }
  }
}
```

`smart_edit` su an altyapi planini dondurur. Uretim uygulamasinda iki yol izlenir:

- silence jump-cut: FFmpeg `silencedetect` stderr ciktilari parse edilir, silence
  interval'lari timeline remove range komutlarina donusturulur.
- beat-sync: RMS/tempo/onset analizi yapilir, beat marker'lari timeline marker olarak
  yazilir, secili b-roll clip'leri marker araliklarina trimlenir.

## 4. TUI Yerlesimi

Terminal arayuzu Ratatui ile bolunmus panellerden olusur.

```text
+--------------------------------------------------------------------------------+
| Project: TermFX       FPS: 30       Render: idle       MCP: connected           |
+----------------------+------------------------------------+--------------------+
| Project Assets       | Video Preview                      | Inspector          |
| - shot_01.mp4        | +---------------- preview --------+ | Track: V1          |
| - music.wav          | | ASCII/sixel/mpv preview         | | Clip params        |
| - logo.png           | | waveform/cache thumbnails       | | Effects: s_shake   |
|                      | +---------------------------------+ | Text/color/fade     |
+----------------------+------------------------------------+--------------------+
| Timeline & Layers                                                             |
| time    |0------------------------------|-----------------------------------> |
| V2      |........TITLE######...................................................|
| V1      |intro############....broll#############....outro########..............|
| A1      |music================================================================|
+--------------------------------------------------------------------------------+
| AI / MCP  list_media ok | apply_effect s_shake queued | render 42%             |
+--------------------------------------------------------------------------------+
```

Panel rolleri:

- Project Assets: ham video, audio, image ve ileride proxy/cache durumlari.
- Video Preview: ilk surum placeholder; uretimde mpv IPC, sixel/kitty image protocol
  veya frame cache kullanilir.
- Timeline & Layers: Premiere gibi yatay zaman, After Effects gibi ust uste video
  layer'lari.
- Inspector: secili track/clip parametreleri, efekt stack'i, keyframe bilgisi.
- AI / MCP: MCP baglanti durumu, son tool cagrisi, render progress ve hata loglari.

## 5. Dosya Yapisi

```text
.
|-- Cargo.toml
|-- README.md
|-- ARCHITECTURE_TR.md
|-- src
|   |-- main.rs
|   |-- lib.rs
|   |-- error.rs
|   |-- project.rs
|   |-- core
|   |   |-- mod.rs
|   |   |-- time.rs
|   |   |-- media.rs
|   |   |-- timeline.rs
|   |   |-- effect.rs
|   |   `-- smart.rs
|   |-- render
|   |   |-- mod.rs
|   |   |-- ffmpeg.rs
|   |   |-- filtergraph.rs
|   |   `-- progress.rs
|   |-- mcp
|   |   |-- mod.rs
|   |   |-- protocol.rs
|   |   |-- server.rs
|   |   `-- tools.rs
|   `-- tui
|       |-- mod.rs
|       |-- app.rs
|       |-- layout.rs
|       `-- timeline_widget.rs
```

Dosya rolleri:

- `src/main.rs`: CLI komutlari. `new`, `add-media`, `add-clip`, `tui`, `mcp`, `render`.
- `src/lib.rs`: modullerin disari acildigi kutuphane kok dosyasi.
- `src/error.rs`: domain hatalari.
- `src/project.rs`: JSON proje modeli, medya ekleme, clip ekleme, efekt uygulama.
- `src/core/time.rs`: FPS ve frame/saniye donusumleri.
- `src/core/media.rs`: medya asset modeli.
- `src/core/timeline.rs`: track, clip, cut, trim, ripple delete.
- `src/core/effect.rs`: efekt enum'lari ve keyframe modeli.
- `src/core/smart.rs`: silence/beat-sync analiz plani.
- `src/render/ffmpeg.rs`: FFmpeg command ve complex filtergraph builder.
- `src/render/filtergraph.rs`: filtergraph escaping ve zaman yardimcilari.
- `src/render/progress.rs`: render progress veri modeli.
- `src/mcp/protocol.rs`: JSON-RPC request/response tipleri.
- `src/mcp/server.rs`: stdio server loop ve MCP lifecycle methodlari.
- `src/mcp/tools.rs`: tool schema ve tool handler implementasyonu.
- `src/tui/app.rs`: terminal lifecycle ve event loop.
- `src/tui/layout.rs`: panellerin Ratatui ile cizimi.
- `src/tui/timeline_widget.rs`: timeline satirlarini terminal genisligine gore render eder.

## 6. Kritik Kod Bloklari

### 6.1 FFmpeg Complex Filtergraph Builder

Ana giris noktasi:

```rust
pub fn build_ffmpeg_command(
    project: &Project,
    output: &Path,
    settings: RenderSettings,
) -> Result<FfmpegCommand>
```

Bu fonksiyon:

1. Timeline'da kullanilan media id'lerini input index'e map eder.
2. Base video layer olusturur.
3. Her video clip icin trim/scale/effect/setpts chain'i kurar.
4. Clip'leri track sirasi ile overlay eder.
5. Audio chain'leri `amix` ile karistirir.
6. `-filter_complex`, `-map [vout]`, `-map [aout]` argumanlarini uretir.

`s_shake` filtresi:

```rust
filters.push(format!(
    "crop={}:{}:x='{}+{}*sin(2*PI*{:.3}*t+{:.3})':y='{}+{}*cos(2*PI*{:.3}*t+{:.3})'",
    settings.width,
    settings.height,
    amp,
    amp,
    frequency_hz,
    seed,
    amp,
    amp,
    frequency_hz * 1.37,
    seed + 1.9
));
```

Bu yaklasim dis plugin gerektirmez; FFmpeg expression engine ile kare bazli hareket
uretir.

### 6.2 MCP Tool Handler

Ana giris:

```rust
pub async fn call_tool(&self, params: Value) -> Result<Value>
```

Dispatch:

```rust
match params.name.as_str() {
    "list_media" => self.list_media().await,
    "cut_video" => self.cut_video(params.arguments.unwrap_or_else(|| json!({}))).await,
    "apply_effect" => self.apply_effect(params.arguments.unwrap_or_else(|| json!({}))).await,
    "smart_edit" => self.smart_edit(params.arguments.unwrap_or_else(|| json!({}))).await,
    other => Err(TermFxError::InvalidMcpRequest(format!("unknown tool: {other}"))),
}
```

Tool mutasyonlari proje dosyasini hemen kaydeder. Bu, AI cagrilarinin terminal UI veya
sonraki render komutlari tarafindan gorulebilmesini saglar.

### 6.3 TUI Timeline Render Fonksiyonu

Ana fonksiyon:

```rust
pub fn timeline_lines(timeline: &Timeline, width: u16) -> Vec<Line<'static>>
```

Calisma sekli:

- Terminal genisligi canvas genisligine cevrilir.
- Timeline toplam suresi normalize edilir.
- Clip start/end frame degerleri terminal kolonuna scale edilir.
- Video track `#`, audio track `=` karakteriyle gosterilir.
- Clip adi blok icine yazilir.

Bu fonksiyon UI framework'ten bagimsiz test edilebilir oldugu icin timeline ciziminde
regression yakalamak kolaydir.

## 7. Uretim Yol Haritasi

Bu repo calisan ve test edilen bir temel sunar. Uretim kalitesini artirmak icin siradaki
net adimlar:

1. `ffprobe` entegrasyonu: duration, stream layout, sample rate ve alpha bilgisi otomatik
   okunmali.
2. Proxy/cache sistemi: buyuk medyalar icin preview resolution ve waveform cache.
3. Render queue: background process, progress parse, cancel/retry, job history.
4. Keyframe interpolation: `TransformKeyframe` uzerinden bezier/ease curve.
5. Text engine: font discovery, fallback font, shadow/stroke, safe-area presets.
6. Smart edit apply modu: `smart_edit` planini gercek timeline mutation'a donusturme.
7. MCP kaynaklari: proje snapshot'i `resources/read` ile sunulabilir.
8. Guvenlik: MCP tool confirmation policy, project root sandbox, path allowlist.

## 8. Dogrulama

Mevcut testler:

- ripple delete clip'i ikiye boler ve sag parcayi kaydirir.
- FFmpeg filtergraph `s_shake`, `drawtext` ve audio mix uretir.
- MCP tool listesi gerekli tool'lari expose eder.
- TUI timeline satiri clip adini terminalde gosterir.

Calistirma:

```bash
cargo test
```

Not: Bu makinede `ffmpeg` kurulu olmadigi icin gercek render calistirilmadi. Kod
`render --dry-run` ile FFmpeg komutunu uretir; render icin `ffmpeg` PATH'te olmalidir.
