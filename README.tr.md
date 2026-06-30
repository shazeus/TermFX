# TermFX

[English](README.md) | [Türkçe](README.tr.md)

![TermFX banner](assets/termfx-banner.png)

FFmpeg tabanlı, MCP uyumlu ve masaüstü arayüzlü video editörü.

TermFX Studio; proje oluşturma, önceki projeleri açma, timeline kurgusu, clip
inspector, katmanlı efekt stack’i, transform keyframe’leri, render kontrolleri
ve MCP endpoint’i olan bir masaüstü video editörüdür.

## Ne İşe Yarar?

TermFX üç ana problemi çözer:

- **Kurgu:** Videoları timeline’a ekleme, kesme, trimleme, ripple delete ve ses
  karıştırma.
- **Efekt ve compositing:** Yazı katmanı, fade, black-white, glitch ve
  `s_shake` benzeri hareket efektlerini FFmpeg filtergraph ile üretme.
- **AI entegrasyonu:** Claude, ChatGPT veya başka bir MCP client’ın projedeki
  medyaları listelemesi, kesim yapması ve efekt uygulaması için stdio MCP veya
  yerel HTTP MCP endpoint’i sağlama.

Bu repo şu an üretime yakın bir çekirdek iskeleti sunar: proje formatı, timeline
modeli, FFmpeg komut üretimi, masaüstü arayüz, eski CLI yardımcıları ve MCP tool
handler’ları çalışır durumdadır.

## Özellikler

- Rust + Tokio tabanlı güvenli ve modüler mimari
- FFmpeg complex filtergraph builder
- Frame tabanlı timeline modeli
- Video/audio track ayrımı
- Clip ekleme, trim ve ripple delete
- Clip speed, track mute/lock ve timeline marker desteği
- Easing destekli transform keyframe’leri
- Keyframe graph çıktısı: JSON, ASCII veya SVG
- Native masaüstü uygulama:
  - ana ekran
  - proje oluşturma ve açma
  - önceki projeler geçmişi
  - çözünürlük, FPS ve sample rate proje ayarları
  - medya havuzu
  - viewer alanı
  - görsel timeline
  - clip inspector
  - efekt tarayıcı
  - keyframe editörü
  - render paneli
  - MCP aktivite paneli
- Efekt stack’i:
  - `black_and_white`
  - `sepia`
  - `invert`
  - `edge_detect`
  - `glitch`
  - `brightness_contrast`
  - `hue_rotate`
  - `gaussian_blur`
  - `box_blur`
  - `sharpen`
  - `vignette`
  - `film_grain`
  - `pixelate`
  - `chromatic_aberration`
  - `lens_distortion`
  - `posterize`
  - `letterbox`
  - `border`
  - `fade_in`
  - `fade_out`
  - `s_shake`
  - `text_overlay`
- Opsiyonel eski TUI arayüzü
- Masaüstü uygulamayla başlayan HTTP MCP endpoint’i ve canlı AI aktivite paneli
- MCP stdio server:
  - `list_media`
  - `list_effects`
  - `import_media`
  - `append_media`
  - `add_text_clip`
  - `cut_video`
  - `apply_effect`
  - `remove_effect`
  - `set_effect_enabled`
  - `update_clip`
  - `move_clip`
  - `split_clip`
  - `remove_clip`
  - `add_keyframe`
  - `remove_keyframe`
  - `keyframe_graph`
  - `add_marker`
  - `remove_marker`
  - `add_track`
  - `set_track_state`
  - `set_timeline_settings`
  - `render_command`
  - `smart_edit`
- JSON proje dosyası
- Test edilmiş temel render akışı

## Gereksinimler

- macOS, Linux veya Windows
- Rust toolchain
- FFmpeg ve FFprobe
- GitHub’a push için GitHub CLI (`gh`)

macOS için:

```bash
brew install rust ffmpeg gh
```

Rust zaten kuruluysa sadece:

```bash
brew install ffmpeg gh
```

Kurulumu doğrula:

```bash
rustc --version
cargo --version
ffmpeg -version
ffprobe -version
gh --version
```

## Kurulum

Repoyu klonla:

```bash
git clone https://github.com/shazeus/TermFX.git
cd TermFX
```

Derle:

```bash
cargo build
```

Testleri çalıştır:

```bash
cargo test
```

CLI yardımını gör:

```bash
cargo run -- --help
```

## Hızlı Başlangıç

Masaüstü uygulamayı başlat:

```bash
cargo run --bin termfx-studio
```

Çok komutlu binary üzerinden de açabilirsin:

```bash
cargo run -- studio
```

Uygulama içinden:

- Ana ekrandan proje oluştur veya önceki projeyi aç.
- Proje ayarlarından çözünürlük, FPS ve sample rate değiştir.
- Medya panelinden video, ses veya görsel import et.
- Clip’leri görsel timeline’a ekle.
- Clip seçip timing, opacity, volume, speed, effect ve keyframe ayarlarını düzenle.
- Render panelinden çıktı al.
- MCP panelinden mevcut projeyi AI client’a aç.

Proje açılınca masaüstü uygulama yerel MCP endpoint’ini başlatır:

```text
http://127.0.0.1:4739/mcp
```

Otomasyon için eski komut satırı akışı da duruyor:

```bash
cargo run -- new --name demo --project termfx.project.json
cargo run -- add-media --project termfx.project.json --path ./shot.mp4 --kind video
cargo run -- add-clip \
  --project termfx.project.json \
  --media-id 6508eba6-7a9b-4eea-b9d0-6f7b92835c18 \
  --track 0 \
  --start-seconds 0 \
  --duration-seconds 5
```

FFmpeg komutunu render etmeden gör:

```bash
cargo run -- render \
  --project termfx.project.json \
  --output out.mp4 \
  --dry-run
```

Gerçek render al:

```bash
cargo run -- render \
  --project termfx.project.json \
  --output out.mp4
```

## MCP Server Kullanımı

Normal interaktif kullanımda masaüstü uygulamayı başlatman yeterli. Uygulama,
mevcut projeyi yerel HTTP üzerinden MCP olarak açar ve AI’ın yaptığı işleri MCP
panelinde gösterir:

```bash
cargo run --bin termfx-studio
```

HTTP endpoint:

```text
http://127.0.0.1:4739/mcp
```

HTTP endpoint destekleyen MCP client’larda örnek konfigürasyon:

```json
{
  "mcpServers": {
    "termfx": {
      "url": "http://127.0.0.1:4739/mcp"
    }
  }
}
```

Masaüstü uygulama olmadan sadece HTTP MCP server test etmek için:

```bash
cargo run -- mcp-http --project termfx.project.json --port 4739
```

Bazı MCP client’lar sadece stdio destekler. Onlar için TermFX MCP server’ı
stdio üzerinden de çalışır:

```bash
cargo run -- mcp --project termfx.project.json
```

Bir MCP client konfigürasyonu örneği:

```json
{
  "mcpServers": {
    "termfx": {
      "command": "cargo",
      "args": [
        "run",
        "--manifest-path",
        "/absolute/path/to/TermFX/Cargo.toml",
        "--",
        "mcp",
        "--project",
        "/absolute/path/to/project/termfx.project.json"
      ]
    }
  }
}
```

Server şu MCP lifecycle çağrılarını destekler:

- `initialize`
- `notifications/initialized`
- `ping`
- `tools/list`
- `tools/call`

## MCP Tool Örnekleri

Medyaları ve timeline’ı listele:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "list_media",
    "arguments": {}
  }
}
```

Yerleşik efekt kütüphanesini listele:

```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "tools/call",
  "params": {
    "name": "list_effects",
    "arguments": {}
  }
}
```

MCP üzerinden medya import et:

```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "method": "tools/call",
  "params": {
    "name": "import_media",
    "arguments": {
      "path": "/absolute/path/to/shot.mp4",
      "kind": "video",
      "name": "shot"
    }
  }
}
```

Medyayı timeline’a ekle:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "append_media",
    "arguments": {
      "media_id": "6508eba6-7a9b-4eea-b9d0-6f7b92835c18",
      "track": 0,
      "start_seconds": 0,
      "duration_seconds": 5
    }
  }
}
```

Bağımsız text clip oluştur:

```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "method": "tools/call",
  "params": {
    "name": "add_text_clip",
    "arguments": {
      "track": 1,
      "text": "INTRO",
      "start_seconds": 0,
      "duration_seconds": 2
    }
  }
}
```

Timeline aralığını ripple delete ile kes:

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "cut_video",
    "arguments": {
      "mode": "remove_range",
      "start_seconds": 1.2,
      "end_seconds": 2.1,
      "ripple": true
    }
  }
}
```

Clip’i timeline zamanında böl:

```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "method": "tools/call",
  "params": {
    "name": "split_clip",
    "arguments": {
      "clip_id": "33c6f411-29d9-4e77-b606-4f444c0b5817",
      "at_seconds": 2.5
    }
  }
}
```

Clip zamanlama, speed ve mix parametrelerini güncelle:

```json
{
  "jsonrpc": "2.0",
  "id": 11,
  "method": "tools/call",
  "params": {
    "name": "update_clip",
    "arguments": {
      "clip_id": "33c6f411-29d9-4e77-b606-4f444c0b5817",
      "speed": 1.25,
      "opacity": 0.85,
      "volume": 0.6
    }
  }
}
```

Easing destekli transform keyframe ekle:

```json
{
  "jsonrpc": "2.0",
  "id": 14,
  "method": "tools/call",
  "params": {
    "name": "add_keyframe",
    "arguments": {
      "clip_id": "33c6f411-29d9-4e77-b606-4f444c0b5817",
      "time_seconds": 1,
      "x": 120,
      "y": 80,
      "scale": 0.85,
      "rotation_degrees": 8,
      "opacity": 1,
      "volume": 1,
      "easing": "ease_in_out"
    }
  }
}
```

Keyframe graph’ını SVG olarak üret:

```json
{
  "jsonrpc": "2.0",
  "id": 15,
  "method": "tools/call",
  "params": {
    "name": "keyframe_graph",
    "arguments": {
      "clip_id": "33c6f411-29d9-4e77-b606-4f444c0b5817",
      "property": "x",
      "format": "svg",
      "width": 640,
      "height": 240
    }
  }
}
```

Timeline marker ekle:

```json
{
  "jsonrpc": "2.0",
  "id": 16,
  "method": "tools/call",
  "params": {
    "name": "add_marker",
    "arguments": {
      "time_seconds": 8.5,
      "label": "Beat drop",
      "color": "cyan",
      "note": "Burada b-roll kes"
    }
  }
}
```

Clip’e `s_shake` efekti uygula:

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "apply_effect",
    "arguments": {
      "clip_id": "33c6f411-29d9-4e77-b606-4f444c0b5817",
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

Sinematik lens efekti uygula:

```json
{
  "jsonrpc": "2.0",
  "id": 12,
  "method": "tools/call",
  "params": {
    "name": "apply_effect",
    "arguments": {
      "clip_id": "33c6f411-29d9-4e77-b606-4f444c0b5817",
      "effect": "vignette",
      "params": {
        "angle": 0.7
      }
    }
  }
}
```

Yazı katmanı ekle:

```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "tools/call",
  "params": {
    "name": "apply_effect",
    "arguments": {
      "clip_id": "33c6f411-29d9-4e77-b606-4f444c0b5817",
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

Render almadan FFmpeg komutunu üret:

```json
{
  "jsonrpc": "2.0",
  "id": 13,
  "method": "tools/call",
  "params": {
    "name": "render_command",
    "arguments": {
      "output": "out.mp4"
    }
  }
}
```

Sessizlik veya beat-sync analizi için plan üret:

```json
{
  "jsonrpc": "2.0",
  "id": 6,
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

## Masaüstü Arayüz

Masaüstü çalışma alanı medya, viewer, inspector, timeline ve MCP panellerinden
oluşur:

```text
+--------------------------------------------------------------------------------+
| TermFX Studio         Home  Save  Render  Start MCP          1920 x 1080 30 fps |
+----------------------+------------------------------------+--------------------+
| Media                | Viewer                             | Inspector          |
| - shot_01.mp4        | +------------- frame ------------+ | Clip               |
| - music.wav          | | selected clip summary          | | Effects            |
| - logo.png           | | render/open output controls    | | Keyframes          |
|                      | +---------------------------------+ | Project / MCP       |
+----------------------+------------------------------------+--------------------+
| Timeline & Layers                                                             |
| time    |0------------------------------|-----------------------------------> |
| V2      |........TITLE######...................................................|
| V1      |intro############....broll#############....outro########..............|
| A1      |music================================================================|
+--------------------------------------------------------------------------------+
| MCP  list_media ok | apply_effect s_shake queued | render ready                 |
+--------------------------------------------------------------------------------+
```

## Proje Dosya Yapısı

```text
src/
  bin/
    termfx-studio.rs   Native masaüstü uygulama giriş noktası
  core/
    effect.rs          Efekt, easing ve keyframe graph veri tipleri
    media.rs           Medya asset modeli
    smart.rs           Smart edit analiz planı
    time.rs            FPS ve frame/saniye dönüşümü
    timeline.rs        Track, clip, marker, keyframe, trim ve ripple delete
  mcp/
    protocol.rs        JSON-RPC request/response tipleri
    server.rs          MCP stdio server loop
    tools.rs           MCP tool schema ve handler’lar
  render/
    ffmpeg.rs          FFmpeg command ve filtergraph builder
    filtergraph.rs     Escaping ve zaman yardımcıları
    progress.rs        Render progress modeli
  desktop/
    mod.rs             Masaüstü uygulama, proje hub, editör panelleri, MCP aktivitesi
  tui/
    app.rs             Opsiyonel eski text UI lifecycle ve event loop
    layout.rs          Opsiyonel eski text UI panel yerleşimi
    timeline_widget.rs Opsiyonel eski timeline çizimi
  project.rs           JSON proje modeli
  main.rs              CLI giriş noktası
```

Daha ayrıntılı mimari açıklama için:

[ARCHITECTURE_TR.md](ARCHITECTURE_TR.md)

## Geliştirme

Format:

```bash
cargo fmt
```

Test:

```bash
cargo test
```

Dry-run render:

```bash
cargo run -- render \
  --project termfx.project.json \
  --output out.mp4 \
  --dry-run
```

## Durum

Bu proje aktif geliştirme aşamasındadır. Çekirdek timeline, MCP tool handler’ları,
keyframe graph ve FFmpeg render path’i çalışır durumdadır. Sıradaki üretim adımları:

- FFprobe ile otomatik medya metadata okuma
- Background render queue ve progress parsing
- Preview cache, waveform ve proxy sistemi
- Smart edit planlarını gerçek timeline mutation’a dönüştürme
- MCP resource desteği

## Lisans

MIT
