# ONECLAW HOME — Smart Toilet Pipeline

## Bản Thiết Kế Hệ Thống v0.1.0

**Dự án:** OneClaw Home — Smart Living Module
**Nền tảng:** OneClaw Kernel v1.5.0
**Kiến trúc sư:** Quỳnh — AI Officer, Real-time Robotics
**Phương pháp:** Vibecode Kit v5.0 (Thầu → Thợ)
**Ngày:** 24/02/2026
**Trạng thái:** DRAFT — Chờ duyệt trước khi triển khai

---

## Mục lục

1. [Tổng quan dự án](#1-tổng-quan-dự-án) — Bối cảnh, mục tiêu, phạm vi
2. [Kiến trúc hệ thống](#2-kiến-trúc-hệ-thống) — Topology, luồng dữ liệu, phân lớp
3. [Phần cứng & Thiết bị IoT](#3-phần-cứng--thiết-bị-iot) — Danh sách linh kiện, kết nối, chi phí
4. [Pipeline chi tiết](#4-pipeline-chi-tiết) — Các giai đoạn xử lý từ sensor đến actuator
5. [Giao diện giọng nói](#5-giao-diện-giọng-nói) — STT, TTS, wake word, xử lý lệnh
6. [Dịch vụ thông tin](#6-dịch-vụ-thông-tin) — Tin tức, email, lịch, notification
7. [Cấu hình & TOML](#7-cấu-hình--toml) — Device registry, scene, provider config
8. [Bảo mật & Quyền riêng tư](#8-bảo-mật--quyền-riêng-tư) — Voice data, network, access control
9. [Lộ trình triển khai](#9-lộ-trình-triển-khai) — Sprint plan, milestones, acceptance gate
10. [Phụ lục](#10-phụ-lục) — BOM, wiring, reference

---

## 1. Tổng quan dự án

### 1.1 Bối cảnh

OneClaw Home là module ứng dụng đầu tiên được xây dựng trên OneClaw Kernel v1.5.0 — một Rust AI Agent Kernel thiết kế cho Edge/IoT. Module này biến toilet thành một không gian thông minh: tự động nhận diện người, kích hoạt thiết bị, và cung cấp giao diện giọng nói tự nhiên để điều khiển nhạc, đọc tin tức, kiểm tra email.

Đây là minh chứng cho triết lý "dao mổ" của OneClaw: kernel giữ nguyên, mỗi ứng dụng chuyên sâu là một project riêng biệt, chỉ làm đúng một việc.

### 1.2 Mục tiêu

| Mục tiêu | Mô tả | Đo đo |
|-----------|--------|-------|
| Tự động hoá | Bật/tắt thiết bị khi detect người | Latency < 500ms |
| Điều khiển giọng nói | Lệnh tiếng Việt tự nhiên | Accuracy > 90% |
| Thông tin theo ngữ cảnh | Tin tức, email, lịch khi được hỏi | Response < 3s |
| Offline capable | Hoạt động không cần internet | Core features 100% |
| Chi phí thấp | Toàn bộ phần cứng | < $150 USD |
| Triển khai đơn giản | Cài đặt và cấu hình | < 30 phút |

### 1.3 Phạm vi phiên bản 0.1.0

**TRONG PHẠM VI:** Presence detection, đèn, quạt, hương, nhạc (local), voice control (tiếng Việt), tin tức (RSS), thời tiết.

**NGOÀI PHẠM VI:** Camera, nhận diện khuôn mặt, streaming nhạc online, điều khiển nhiệt độ nước, tích hợp smart mirror.

> **NGUYÊN TẮC:** Không thêm tính năng ngoài phạm vi. Ship v0.1 trước, mở rộng dựa trên feedback thực tế.

---

## 2. Kiến trúc hệ thống

### 2.1 Topology tổng thể

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         SYSTEM TOPOLOGY                                 │
│                                                                         │
│  ┌──────────────┐         ┌──────────────────────────┐    ┌──────────┐ │
│  │  SENSORS     │         │   RASPBERRY PI 4/5       │    │ ACTUATORS│ │
│  │              │  MQTT   │                          │    │          │ │
│  │ ┌──────────┐ │ ──────► │ ┌──────────┐ ┌────────┐ │    │ ┌──────┐ │ │
│  │ │ mmWave   │ │         │ │ OneClaw  │ │ Ollama │ │───►│ │ Đèn  │ │ │
│  │ │ HLK-2410 │ │         │ │ Kernel   │ │ Local  │ │    │ └──────┘ │ │
│  │ └──────────┘ │         │ │ v1.5.0   │ │ LLM    │ │    │ ┌──────┐ │ │
│  │ ┌──────────┐ │         │ └──────────┘ └────────┘ │───►│ │ Quạt │ │ │
│  │ │ PIR      │ │         │ ┌──────────┐ ┌────────┐ │    │ └──────┘ │ │
│  │ │ HC-SR501 │ │         │ │Mosquitto │ │Whisper │ │    │ ┌──────┐ │ │
│  │ └──────────┘ │         │ │  MQTT    │ │  STT   │ │───►│ │Hương │ │ │
│  │ ┌──────────┐ │   USB   │ └──────────┘ └────────┘ │    │ └──────┘ │ │
│  │ │ USB Mic  │ │ ──────► │ ┌──────────┐ ┌────────┐ │    │ ┌──────┐ │ │
│  │ │ ReSpeaker│ │         │ │  Piper   │ │  mpd   │ │───►│ │Speaker│ │
│  │ └──────────┘ │         │ │   TTS    │ │ Music  │ │    │ └──────┘ │ │
│  └──────────────┘         │ └──────────┘ └────────┘ │    └──────────┘ │
│                           └────────────┬─────────────┘                 │
│                                        │ (optional)                    │
│                                ┌───────▼────────┐                     │
│                                │  Cloud LLM     │                     │
│                                │  Claude / GPT  │                     │
│                                │  (Fallback)    │                     │
│                                └────────────────┘                     │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Phân lớp ứng dụng

OneClaw Home xây dựng 4 lớp ứng dụng trên 6 lớp kernel có sẵn:

| Lớp | Module | Trách nhiệm | Mới / Có sẵn |
|-----|--------|-------------|--------------|
| Application | Scene Engine | Kịch bản tự động (vào/ra toilet) | MỚI |
| Application | Device Registry | Danh sách thiết bị + MQTT topics | MỚI |
| Application | Voice Interface | STT + TTS + Wake Word | MỚI |
| Application | Info Aggregator | RSS, Email, Calendar, Weather | MỚI |
| Kernel | Channel (MQTT) | Nhận sensor data, gửi command | CÓ SẴN |
| Kernel | Pipeline | Chuỗi xử lý: detect → activate → respond | CÓ SẴN |
| Kernel | NLP + Providers | Hiểu lệnh tiếng Việt, 6 LLM providers | CÓ SẴN |
| Kernel | Memory (SQLite) | Nhớ nhạc yêu thích, thói quen | CÓ SẴN |
| Kernel | Tool Layer | Thực thi lệnh (MQTT publish, HTTP call) | CÓ SẴN |
| Kernel | Security | HMAC auth, device pairing | CÓ SẴN |

---

## 3. Phần cứng & Thiết bị IoT

### 3.1 Hub trung tâm

| Linh kiện | Model cụ thể | Vai trò | Giá USD |
|-----------|-------------|---------|---------|
| SBC (Single Board Computer) | Raspberry Pi 4B 4GB | Chạy OneClaw + Ollama + MQTT | $55 |
| Thẻ nhớ | Samsung EVO 32GB microSD | OS + data storage | $8 |
| Nguồn | USB-C 5V/3A Official PSU | Cấp nguồn ổn định | $8 |
| WiFi | Built-in (Pi 4) | Kết nối MQTT devices | $0 |
| Zigbee Dongle (tuỳ chọn) | SONOFF ZBDongle-P | Kết nối Zigbee sensors | $12 |

### 3.2 Sensor (đầu vào)

| Thiết bị | Model | Giao tiếp | Đặc điểm | Giá |
|----------|-------|-----------|-----------|-----|
| Presence Sensor | HLK-LD2410B | UART → ESP32 → MQTT | mmWave 24GHz, phát hiện người đứng yên, không bị ảnh hưởng bởi nhiệt độ/độ ẩm | $4 |
| Motion Sensor (backup) | HC-SR501 | GPIO → ESP32 → MQTT | PIR, phát hiện chuyển động, rẻ, đơn giản | $1.5 |
| Microphone | ReSpeaker USB Mic | USB → Pi | Beamforming, khử nhiễu, tốt cho voice | $25 |
| Microphone (budget) | USB Mic generic | USB → Pi | Cơ bản, hoạt động được | $5 |

### 3.3 Actuator (đầu ra)

| Thiết bị | Model | Điều khiển | Ghi chú | Giá |
|----------|-------|-----------|---------|-----|
| Relay Module | ESP32 + 4CH Relay | MQTT subscribe | Điều khiển đèn, quạt, máy hương. 1 board cho 3 thiết bị | $8 |
| Speaker | USB Speaker 3W | mpd/mpc qua local | Phát nhạc + TTS output. Không cần bluetooth | $10 |
| Smart Plug (thay thế) | Zigbee Smart Plug | Zigbee → MQTT | Thay relay cho máy hương. Plug-and-play | $10 |
| Đèn LED (thay thế) | Zigbee LED Bulb | Zigbee → MQTT | Dim được, thay đổi màu. Thay relay+bóng | $10 |

### 3.4 ESP32 — Bộ điều khiển trung gian

ESP32 đóng vai trò cầu nối giữa sensor/actuator và MQTT broker trên Pi. Một ESP32 có thể phục vụ nhiều thiết bị đồng thời (sensor + relay). Firmware: ESPHome hoặc Tasmota — cấu hình MQTT bằng YAML, không cần code C.

### 3.5 Tổng chi phí (BOM)

| Cấu hình | Thành phần | Tổng chi phí |
|-----------|-----------|-------------|
| **BUDGET** (cơ bản) | Pi 4 + SD + PSU + HC-SR501 + ESP32 Relay + USB Mic + USB Speaker | $90 – $100 |
| **RECOMMENDED** | Pi 4 + SD + PSU + HLK-LD2410 + ESP32 Relay + ReSpeaker + USB Speaker | $120 – $130 |
| **PREMIUM** | Pi 5 + SD + PSU + HLK-LD2410 + Zigbee Dongle + Zigbee Bulb + Zigbee Plug + ReSpeaker + Speaker | $150 – $170 |

---

## 4. Pipeline chi tiết

### 4.1 Tổng quan luồng xử lý

```
══════════════════════════════════════════════════════════════════════
  ĐƯỜNG 1 — SENSOR-DRIVEN (Tự động, không cần giọng nói)
══════════════════════════════════════════════════════════════════════

  ┌────────────┐   ┌──────────┐   ┌─────────────┐   ┌──────────┐   ┌─────┐
  │  SENSOR    │──►│  DETECT  │──►│ SCENE MATCH │──►│ ACTIVATE │──►│ LOG │
  │  INPUT     │   │ Presence │   │ Rule Engine │   │ Commands │   │     │
  │ MQTT Sub   │   │ Engine   │   │             │   │ MQTT Pub │   │ Mem │
  └────────────┘   └──────────┘   └─────────────┘   └──────────┘   └─────┘

══════════════════════════════════════════════════════════════════════
  ĐƯỜNG 2 — VOICE-DRIVEN (Người dùng chủ động ra lệnh)
══════════════════════════════════════════════════════════════════════

  ┌────────────┐   ┌──────────┐   ┌─────────────┐   ┌──────────┐   ┌─────┐
  │  VOICE     │──►│   NLP    │──►│ LLM REASON  │──►│  TOOL    │──►│ TTS │
  │  INPUT     │   │  PARSE   │   │ Fallback    │   │  EXEC    │   │ OUT │
  │ Whisper    │   │ Intent   │   │ Chain       │   │ Action   │   │Piper│
  └────────────┘   └──────────┘   └─────────────┘   └──────────┘   └─────┘
```

> **HAI ĐƯỜNG DẪN SONG SONG:** Đường trên là sensor-driven, tự động, không cần giọng nói. Đường dưới là voice-driven, người dùng chủ động ra lệnh.

### 4.2 Sensor Pipeline — Tự động

**Giai đoạn 1: Sensor Input**

mmWave sensor (HLK-LD2410) gửi dữ liệu qua ESP32 lên MQTT topic `home/toilet/presence`. Payload JSON:

```json
{"status": "occupied", "distance_cm": 120, "energy": 85}
```

OneClaw MQTT Channel subscribe topic này và push event vào Event Bus.

**Giai đoạn 2: Presence Detection**

Presence Engine xử lý event với logic:
- `occupied` + energy > threshold → trigger ENTRY scene
- `unoccupied` liên tục 30 giây → trigger EXIT scene
- Debounce 2 giây để tránh false trigger

Kết quả: scene event được push vào Pipeline.

**Giai đoạn 3: Scene Execution**

| Scene | Trigger | Actions | Delay |
|-------|---------|---------|-------|
| toilet_entry | presence = occupied | Đèn ON, Quạt ON, Hương ON (30s), Nhạc play | 0ms |
| toilet_exit | presence = unoccupied (30s) | Nhạc stop, Hương OFF | 0s |
| toilet_exit_delayed | presence = unoccupied (60s) | Đèn OFF | 60s sau exit |
| toilet_exit_fan | presence = unoccupied (120s) | Quạt OFF | 120s sau exit |

**Giai đoạn 4: Device Command**

Scene Engine gửi lệnh qua Tool Layer → MQTT publish. Mỗi thiết bị có mapping rõ ràng trong Device Registry:

| Thiết bị | MQTT Topic | Payload ON | Payload OFF |
|----------|-----------|-----------|------------|
| Đèn | `home/toilet/light/set` | `{"state": "ON"}` | `{"state": "OFF"}` |
| Quạt | `home/toilet/fan/set` | `{"state": "ON"}` | `{"state": "OFF"}` |
| Hương | `home/toilet/aroma/set` | `{"state": "ON", "duration": 30}` | `{"state": "OFF"}` |
| Nhạc | `local://mpd` | play (playlist: morning) | pause |

### 4.3 Ví dụ thực tế — Một buổi sáng

```
06:30 — Quỳnh bước vào toilet
═══════════════════════════════

  mmWave sensor detect presence
       │
       ▼ MQTT: home/toilet/presence → "occupied"
       │
  OneClaw Pipeline trigger: "toilet_entry"
       │
       ├──► Tool: MQTT publish home/toilet/light → ON
       ├──► Tool: MQTT publish home/toilet/fan → ON
       ├──► Tool: MQTT publish home/toilet/aroma → ON (30s)
       ├──► Tool: local mpd → play "morning_chill" playlist
       └──► Memory: log entry time


06:31 — "Ê OneClaw, hôm nay có gì mới không?"
════════════════════════════════════════════════

  Mic → Whisper STT → "hôm nay có gì mới không"
       │
  OneClaw NLP (Ollama local → Claude cloud fallback)
       │
       ├──► Tool: fetch RSS news (VnExpress, Tuổi Trẻ)
       ├──► Tool: check Gmail (3 unread)
       ├──► Tool: check calendar (9AM meeting)
       │
  Response → Piper TTS → Speaker:
  "Sáng nay có 3 email mới, 1 từ sếp về deadline Q1.
   Lịch 9 giờ họp sprint review. Tin nổi bật: VinFast
   vừa ra mắt mẫu xe mới tại CES..."


06:33 — "Chuyển bài khác đi"
══════════════════════════════

  STT → "chuyển bài khác"
       │
  OneClaw NLP → intent: music_next
       │
  Tool: mpd → next track
  TTS: "Bài tiếp: Counting Stars — OneRepublic"


06:35 — "Tắt nhạc"
════════════════════

  STT → "tắt nhạc" → intent: music_pause → Tool: mpd pause


06:37 — Quỳnh rời toilet
══════════════════════════

  mmWave: no presence (30s timeout)
       │
  Pipeline trigger: "toilet_exit"
       ├──► music → stop (ngay lập tức)
       ├──► hương → OFF (ngay lập tức)
       ├──► đèn → OFF (delay 60s — phòng trường hợp quay lại)
       ├──► quạt → OFF (delay 120s — hút thêm cho sạch)
       └──► Memory: log "phiên 7 phút"
```

---

## 5. Giao diện giọng nói

### 5.1 Voice Pipeline

| Bước | Component | Công nghệ | Chạy ở đâu | Latency |
|------|-----------|-----------|-----------|---------|
| 1. Wake Word | openWakeWord | TFLite model, keyword "Ê OneClaw" | Pi local | < 100ms |
| 2. Recording | ALSA / PulseAudio | Ghi 5-10s sau wake word | Pi local | realtime |
| 3. STT | Whisper.cpp | Model small (461MB) hoặc tiny (75MB) | Pi local | 2-4s |
| 4. NLP | OneClaw Pipeline | Intent detection + entity extraction | Pi local | < 50ms |
| 5. LLM | FallbackChain | Ollama (local) → Claude (cloud fallback) | Pi / Cloud | 1-5s |
| 6. TTS | Piper TTS | Vietnamese voice model (vi_VN-vivos) | Pi local | < 1s |
| 7. Playback | ALSA / PulseAudio | Phát audio qua USB speaker | Pi local | < 100ms |

> **TỔNG LATENCY:** Từ lúc nói xong đến lúc nghe phản hồi: 3-10 giây (local) hoặc 5-15 giây (cloud). Chấp nhận được cho bathroom context — người dùng không gấp thời gian.

### 5.2 Intent Map — Các lệnh hỗ trợ

| Intent | Ví dụ câu nói | Action |
|--------|--------------|--------|
| `music_pause` | "Tắt nhạc", "Dừng nhạc" | mpd pause |
| `music_play` | "Bật nhạc", "Mở nhạc" | mpd play |
| `music_next` | "Chuyển bài", "Bài khác" | mpd next |
| `music_prev` | "Bài trước" | mpd prev |
| `music_volume` | "Tăng âm lượng", "Giảm âm lượng", "Âm lượng 50" | mpd volume +-10 / set |
| `music_specific` | "Bật bài Counting Stars" | mpd search + play |
| `light_control` | "Tắt đèn", "Bật đèn" | MQTT light ON/OFF |
| `fan_control` | "Tắt quạt", "Bật quạt" | MQTT fan ON/OFF |
| `info_news` | "Hôm nay có gì mới?", "Đọc tin tức" | RSS fetch → TTS |
| `info_email` | "Kiểm tra email", "Có mail gì không?" | IMAP fetch → TTS |
| `info_weather` | "Thời tiết hôm nay" | Weather API → TTS |
| `info_calendar` | "Lịch hôm nay", "Có cuộc họp nào?" | Calendar → TTS |
| `info_time` | "Mấy giờ rồi?" | System clock → TTS |
| `general_chat` | "Kể chuyện đi", "Hôm nay thế nào?" | LLM generate → TTS |

### 5.3 Wake Word — Lựa chọn

- **Option A:** Wake word "Ê OneClaw" — hands-free, nhưng có false positive trong toilet do echo
- **Option B:** Nút bấm vật lý chống nước gắn tường — chính xác 100%, rẻ ($1), phù hợp toilet
- **Option C:** Cả hai — wake word hoạt động nhưng nút bấm luôn ưu tiên

**Đề xuất: Option C** — linh hoạt nhất.

---

## 6. Dịch vụ thông tin

Khi người dùng hỏi thông tin, OneClaw sử dụng Tool Layer để fetch data từ nhiều nguồn, sau đó LLM tóm tắt và Piper TTS đọc kết quả.

| Dịch vụ | Nguồn dữ liệu | Offline? | Cấu hình | Output format |
|---------|---------------|---------|---------|--------------|
| Tin tức | RSS feeds (VnExpress, Tuổi Trẻ,...) | Không | URL list trong home.toml | Top 3-5 tiêu đề, đọc chi tiết khi hỏi |
| Email | IMAP (Gmail, Outlook,...) | Không | IMAP credentials (app password) | Số email mới, tiêu đề, người gửi |
| Lịch | CalDAV / ICS file | Có (ICS local) | Calendar URL hoặc file path | Cuộc họp hôm nay, giờ, địa điểm |
| Thời tiết | OpenWeatherMap API | Không | API key + location | Nhiệt độ, độ ẩm, dự báo |
| Giờ | System clock | Có | Timezone trong config | Giờ hiện tại |
| Kể chuyện | LLM generate | Có (Ollama) | Không cần cấu hình | Chuyện ngắn theo topic |

### 6.1 LLM Context Window cho Info

Khi fetch data (tin tức, email), raw data được inject vào system prompt của LLM:

```
"Đây là 5 tiêu đề tin mới nhất: [data]. Hãy tóm tắt ngắn gọn bằng tiếng Việt,
giải điệu tự nhiên như đang nói chuyện."
```

LLM sẽ tạo câu trả lời phù hợp voice output — ngắn gọn, không đọc URL, không format markdown.

---

## 7. Cấu hình & TOML

### 7.1 File cấu hình chính: `home.toml`

```toml
# ═══════════════════════════════════════════
# OneClaw Home Configuration
# ═══════════════════════════════════════════

# ─── Provider (kế thừa từ OneClaw Kernel) ───
[provider]
primary = "ollama"
model = "llama3.2:3b"
fallback = ["anthropic", "openai"]

[provider.keys]
anthropic = "sk-ant-..."
openai = "sk-..."

# ─── Thiết bị ───
[[devices]]
id = "toilet_light"
name = "Đèn toilet"
type = "switch"
mqtt_topic = "home/toilet/light/set"
payload_on = '{"state":"ON"}'
payload_off = '{"state":"OFF"}'

[[devices]]
id = "toilet_fan"
name = "Quạt hút"
type = "switch"
mqtt_topic = "home/toilet/fan/set"
payload_on = '{"state":"ON"}'
payload_off = '{"state":"OFF"}'

[[devices]]
id = "toilet_aroma"
name = "Máy khuếch tán"
type = "timed_switch"
mqtt_topic = "home/toilet/aroma/set"
auto_off_seconds = 30

[[devices]]
id = "toilet_speaker"
name = "Loa toilet"
type = "media"
controller = "mpd"
default_playlist = "morning_chill"

# ─── Cảm biến ───
[[sensors]]
id = "toilet_presence"
mqtt_topic = "home/toilet/presence"
type = "presence"
debounce_ms = 2000
exit_timeout_s = 30

# ─── Kịch bản ───
[[scenes]]
id = "toilet_entry"
trigger = "toilet_presence:occupied"
actions = [
  {device = "toilet_light", action = "on"},
  {device = "toilet_fan", action = "on"},
  {device = "toilet_aroma", action = "on"},
  {device = "toilet_speaker", action = "play"},
]

[[scenes]]
id = "toilet_exit"
trigger = "toilet_presence:unoccupied"
actions = [
  {device = "toilet_speaker", action = "stop", delay_s = 0},
  {device = "toilet_aroma", action = "off", delay_s = 0},
  {device = "toilet_light", action = "off", delay_s = 60},
  {device = "toilet_fan", action = "off", delay_s = 120},
]

# ─── Giọng nói ───
[voice]
stt_engine = "whisper"
stt_model = "small"
tts_engine = "piper"
tts_voice = "vi_VN-vivos-medium"
wake_word = "e_oneclaw"
language = "vi"

# ─── Dịch vụ thông tin ───
[info.news]
feeds = [
  "https://vnexpress.net/rss/tin-moi-nhat.rss",
  "https://tuoitre.vn/rss/tin-moi-nhat.rss",
]
max_items = 5

[info.email]
imap_server = "imap.gmail.com"
username = "user@gmail.com"
app_password_env = "ONECLAW_EMAIL_PASSWORD"

[info.weather]
api_key_env = "OPENWEATHER_API_KEY"
location = "Ho Chi Minh City"
```

---

## 8. Bảo mật & Quyền riêng tư

| Mối đe doạ | Biện pháp | Mức độ |
|------------|----------|--------|
| Voice data bị nghe lén | Tất cả xử lý STT/TTS trên Pi local. Audio KHÔNG gửi cloud. Chỉ text được gửi LLM khi cần (và có thể dùng Ollama local 100%) | CAO |
| MQTT bị intercept | MQTT broker chỉ chạy localhost (127.0.0.1). Nếu cần remote: TLS + username/password | TRUNG BÌNH |
| Truy cập trái phép | OneClaw Security Layer: HMAC-SHA256 device pairing. Chỉ paired devices được gửi command | CAO |
| API key bị lộ | Keys đọc từ env var, KHÔNG hardcode trong config. `.env` file có 600 permission | CAO |
| False trigger (voice) | Wake word + confirmation cho lệnh quan trọng. Nút bấm vật lý làm alternative | THẤP |
| Người lạ vào toilet | Presence sensor chỉ trigger scene, không thu thập dữ liệu cá nhân. Không camera. Không nhận diện khuôn mặt | THẤP |

> **NGUYÊN TẮC BẢO MẬT:** Voice data KHÔNG BAO GIỜ rời khỏi thiết bị. Email credentials dùng app-specific password, KHÔNG dùng password chính. MQTT chỉ nhận kết nối local. Zero cloud dependency cho core features.

---

## 9. Lộ trình triển khai

### 9.1 Sprint Plan

| Sprint | TIP | Nội dung | Thời gian | Gate |
|--------|-----|---------|-----------|------|
| S1 | TIP-H01 | Project scaffold: oneclaw-home Cargo workspace, depend oneclaw-core | 2h | cargo build OK |
| S1 | TIP-H02 | Device Registry: parse `[[devices]]` từ TOML, device lookup by ID | 3h | Unit tests pass |
| S2 | TIP-H03 | Scene Engine: parse `[[scenes]]`, trigger matching, action dispatch | 4h | Scene tests pass |
| S2 | TIP-H04 | Presence Engine: subscribe MQTT, debounce, entry/exit detection | 3h | E2E: sensor → scene |
| S3 | TIP-H05 | Voice STT: Whisper.cpp integration, record + transcribe | 4h | Vietnamese STT works |
| S3 | TIP-H06 | Voice TTS: Piper integration, text → audio → speaker | 3h | Vietnamese TTS works |
| S4 | TIP-H07 | Intent Parser: map Vietnamese phrases → intents → actions | 4h | 15 intents recognized |
| S4 | TIP-H08 | Media Controller: mpd/mpc wrapper, play/pause/next/volume | 3h | Music control works |
| S5 | TIP-H09 | Info Services: RSS, Email (IMAP), Weather, Calendar | 5h | Fetch + TTS works |
| S5 | TIP-H10 | Integration Test: full pipeline sensor → voice → info → actuator | 3h | All gates pass |
| S6 | TIP-H11 | ESP32 firmware: ESPHome config cho sensor + relay | 3h | Hardware connected |
| S6 | TIP-H12 | Field deploy + Gate: cài đặt thực tế, đo latency, fix bugs | 4h | Production ready |

### 9.2 Milestones

| Milestone | Sprint | Tiêu chí | Tag |
|-----------|--------|---------|-----|
| M1: Core Infrastructure | S1-S2 | Device + Scene + Presence hoạt động E2E (không voice) | v0.1.0-alpha |
| M2: Voice Enabled | S3-S4 | Nói lệnh điều khiển được đèn, quạt, nhạc | v0.1.0-beta |
| M3: Full Featured | S5 | Info services (tin tức, email, thời tiết) hoạt động | v0.1.0-rc |
| M4: Production | S6 | Deploy trên hardware thật, latency OK, ổn định 24h | v0.1.0 |

### 9.3 Acceptance Gate — v0.1.0

| Tiêu chí | Ngưỡng | Đo đo |
|---------|--------|------|
| Presence detection latency | < 500ms | Từ lúc bước vào đến lúc đèn bật |
| Voice-to-response latency | < 10s (local) | Từ lúc nói xong đến lúc nghe trả lời |
| STT accuracy (tiếng Việt) | > 85% | Test 50 câu lệnh phổ thông |
| Scene execution reliability | 100% | 100 lần vào/ra, không miss |
| Uptime | > 99% / 24h | Chạy liên tục không crash |
| Memory usage | < 512MB RAM | Runtime + Ollama + Whisper |
| Binary size | < 10MB | OneClaw Home (không tính models) |
| Total tests | > 100 | Unit + integration |

---

## 10. Phụ lục

### A. Sơ đồ kết nối vật lý (Wiring)

```
RASPBERRY PI 4/5
├── USB Port 1 → ReSpeaker USB Mic
├── USB Port 2 → USB Speaker
├── USB Port 3 → Zigbee Dongle (tuỳ chọn)
├── GPIO / USB → (dự phòng)
├── WiFi → MQTT Broker (localhost:1883)
│            ├── ESP32-01 (toilet sensors + relays)
│            │    ├── GPIO 16 → HLK-LD2410 (presence)
│            │    ├── GPIO 17 → HC-SR501 (motion backup)
│            │    ├── GPIO 25 → Relay CH1 → Đèn (220V AC)
│            │    ├── GPIO 26 → Relay CH2 → Quạt (220V AC)
│            │    └── GPIO 27 → Relay CH3 → Máy hương (220V AC)
│            └── (ESP32 phòng khác — tương lai)
└── Ethernet (tuỳ chọn) → Router / Internet
```

### B. Công nghệ tham khảo

| Công nghệ | Repository / URL | License |
|-----------|-----------------|---------|
| Whisper.cpp | github.com/ggerganov/whisper.cpp | MIT |
| Piper TTS | github.com/rhasspy/piper | MIT |
| openWakeWord | github.com/dscripka/openWakeWord | Apache 2.0 |
| ESPHome | esphome.io | MIT / Apache 2.0 |
| Mosquitto MQTT | mosquitto.org | EPL 2.0 |
| Ollama | ollama.com | MIT |
| mpd (Music Player) | musicpd.org | GPL v2 |
| OneClaw Kernel | Real-time Robotics (internal) | Proprietary |

### C. Project Structure

```
oneclaw-home/
├── Cargo.toml
├── home.toml                  ← device + scene + voice config
├── src/
│   ├── main.rs
│   ├── config.rs              ← parse home.toml
│   ├── devices/
│   │   ├── mod.rs             ← DeviceRegistry
│   │   ├── switch.rs          ← ON/OFF devices (đèn, quạt, hương)
│   │   └── media.rs           ← mpd controller
│   ├── scenes/
│   │   ├── mod.rs             ← SceneEngine
│   │   ├── presence.rs        ← PresenceEngine (debounce + detect)
│   │   └── executor.rs        ← dispatch actions to devices
│   ├── voice/
│   │   ├── mod.rs             ← VoicePipeline
│   │   ├── stt.rs             ← Whisper.cpp binding
│   │   ├── tts.rs             ← Piper binding
│   │   ├── wakeword.rs        ← openWakeWord
│   │   └── intent.rs          ← Vietnamese intent parser
│   └── info/
│       ├── mod.rs             ← InfoAggregator
│       ├── news.rs            ← RSS fetcher
│       ├── email.rs           ← IMAP client
│       ├── weather.rs         ← OpenWeatherMap
│       └── calendar.rs        ← CalDAV / ICS
├── firmware/
│   └── esphome/
│       └── toilet-esp32.yaml  ← ESPHome config
└── tests/
    ├── scene_test.rs
    ├── voice_test.rs
    └── integration_test.rs
```

---

**— HẾT TÀI LIỆU THIẾT KẾ —**

Tài liệu này cần được duyệt bởi Kiến trúc sư trước khi bắt đầu Sprint 1. Mọi thay đổi phạm vi hoặc phần cứng phải cập nhật lại tài liệu này.

*Real-time Robotics — Vibecode Kit v5.0 — DRAFT v0.1.0 — 24/02/2026*
