import { useState, useRef, useEffect, useMemo } from "react";
import {
  Send,
  Bot,
  User,
  Activity,
  Shield,
  Database,
  Radio,
  Cpu,
  Zap,
  ChevronRight,
  Search,
  Brain,
  Layers,
  Terminal,
  Wifi,
  Lock,
  BarChart3,
  Clock,
  CircleDot,
  Sparkles,
  ArrowUpRight,
  MessageSquare,
  Settings,
  Moon,
  Sun,
  Languages,
  LayoutDashboard,
  Gauge,
  Box,
  GitBranch,
  Server,
  MemoryStick,
  ShieldCheck,
  Podcast,
  Braces,
  X,
  ChevronDown,
} from "lucide-react";

// ═══════════════════════════════════════════
// THEME SYSTEM
// ═══════════════════════════════════════════

const ACCENT = "#34d399";
const ACCENT_DIM_DARK = "rgba(52,211,153,0.12)";
const ACCENT_DIM_LIGHT = "rgba(5,150,105,0.12)";

const themes = {
  dark: {
    surface: "#0f1117",
    surface2: "#161922",
    surface3: "#1c1f2e",
    border: "rgba(255,255,255,0.06)",
    text: "#e2e8f0",
    textDim: "#64748b",
    textMuted: "#475569",
    accent: ACCENT,
    accentDim: ACCENT_DIM_DARK,
    accentText: ACCENT,
    iconBg: "rgba(255,255,255,0.04)",
    hoverBg: "rgba(255,255,255,0.03)",
    chatUser: "transparent",
    chatSystem: "rgba(52,211,153,0.03)",
    chatAssistant: "rgba(255,255,255,0.015)",
    chatUserAvatar: "rgba(255,255,255,0.06)",
    scrollThumb: "rgba(255,255,255,0.08)",
    scrollThumbHover: "rgba(255,255,255,0.15)",
    inputBg: "#161922",
    cardHoverBorder: "rgba(52,211,153,0.2)",
    progressTrack: "rgba(255,255,255,0.06)",
    warn: "#f59e0b",
    warnBg: "rgba(245,158,11,0.1)",
    sendDisabled: "rgba(255,255,255,0.04)",
    sendDisabledColor: "#475569",
  },
  light: {
    surface: "#f8fafc",
    surface2: "#ffffff",
    surface3: "#f1f5f9",
    border: "rgba(0,0,0,0.08)",
    text: "#0f172a",
    textDim: "#64748b",
    textMuted: "#94a3b8",
    accent: "#059669",
    accentDim: ACCENT_DIM_LIGHT,
    accentText: "#047857",
    iconBg: "rgba(0,0,0,0.04)",
    hoverBg: "rgba(0,0,0,0.03)",
    chatUser: "transparent",
    chatSystem: "rgba(5,150,105,0.05)",
    chatAssistant: "rgba(0,0,0,0.015)",
    chatUserAvatar: "rgba(0,0,0,0.06)",
    scrollThumb: "rgba(0,0,0,0.10)",
    scrollThumbHover: "rgba(0,0,0,0.18)",
    inputBg: "#ffffff",
    cardHoverBorder: "rgba(5,150,105,0.3)",
    progressTrack: "rgba(0,0,0,0.06)",
    warn: "#d97706",
    warnBg: "rgba(217,119,6,0.08)",
    sendDisabled: "rgba(0,0,0,0.04)",
    sendDisabledColor: "#94a3b8",
  },
};

// ═══════════════════════════════════════════
// i18n — VIETNAMESE / ENGLISH
// ═══════════════════════════════════════════

const i18n = {
  en: {
    dashboard: "Dashboard",
    chat: "Chat",
    terminal: "Terminal",
    memory: "Memory",
    security: "Security",
    providers: "Providers",
    channels: "Channels",
    settings: "Settings",
    tests: "Tests",
    loc: "LOC",
    eventsDay: "Events/24h",
    providerChain: "Provider Chain",
    vectorSearch: "Vector Search",
    mode: "Mode",
    pairedDevices: "Paired devices",
    denied24h: "Denied (24h)",
    commands: "Commands",
    model: "Model",
    dim: "Dim",
    failures: "failures",
    filesCrates: "files, {crates} crates",
    embedded: "embedded",
    avgLatency: "avg latency",
    totalEntries: "Total Entries",
    withEmbeddings: "with embeddings",
    embeddingModel: "Embedding Model",
    dimensions: "dimensions",
    searchModes: "Search Modes",
    searchModesDesc: "FTS5 · Vector · Hybrid (RRF)",
    denyByDefault: "deny-by-default",
    commandsSecured: "Commands Secured",
    perCommandAuth: "per-command authorization",
    pairedDev: "paired devices",
    kernelTerminal: "OneClaw Kernel Terminal",
    inputPlaceholder: "remember, recall, status, ask ...",
    footerInfo: "OneClaw v1.6.0 · 3 crates · 550 tests · {n} embedded memories",
    welcomeSub: "v1.6.0 — SCALPEL · 550 tests · 3 crates · 6 providers",
    system: "system",
    you: "you",
    oneclaw: "oneclaw",
    // Chat responses
    respRemember: "Remembered (with embedding): {c}\n\n✓ Stored with nomic-embed-text (768d)\n✓ FTS5 indexed",
    respRecall: 'Recalled memories for "{q}":\n1. [score:0.91] Room temperature 31°C\n2. [score:0.67] AC set to 26°C\n\nHybrid: FTS5 + vector + RRF',
    respStatus: "OneClaw v1.6.0 SCALPEL\nUptime: {uptime}\nProviders: 2 online, 1 standby\nMemory: {total} entries ({embedded} embedded)\nSecurity: {mode}\nEvent Bus: {bus} ({latency}ms)",
    respHealth: "All systems nominal.\n✓ Memory: healthy\n✓ Providers: 2/3 online\n✓ Security: enforced\n✓ Events: flowing (4.2ms avg)",
    respDefault: 'Processing: "{input}"\n\nNo LLM provider targeted. Use "ask <question>" for AI response or try: status, health, providers, remember, recall',
    respAsk: 'AI Analysis ({q}):\n\nBased on available context and provider chain (Anthropic → Ollama → DeepSeek), here is the response:\n\nThe OneClaw kernel processes your query through the Smart Router, which classified it as "medium" complexity. The DefaultContextManager injected relevant memory entries as context.\n\nNote: This is a simulated response. Connect an LLM provider for real AI answers.',
    // Settings tab
    settingsAppearance: "Appearance",
    settingsTheme: "Theme",
    settingsThemeDesc: "Switch between dark and light mode",
    settingsLang: "Language",
    settingsLangDesc: "Interface language (Vietnamese / English)",
    settingsSystem: "System Info",
    settingsBinary: "Binary Size",
    settingsRuntime: "Runtime",
    settingsEdition: "Rust Edition",
    settingsVersion: "Version",
    settingsAbout: "About",
    settingsAboutDesc: "OneClaw — Rust AI Agent Kernel for Edge/IoT. 5-layer trait-driven architecture. Dual MIT/Apache 2.0 license.",
  },
  vi: {
    dashboard: "Tổng quan",
    chat: "Trò chuyện",
    terminal: "Dòng lệnh",
    memory: "Bộ nhớ",
    security: "Bảo mật",
    providers: "Nhà cung cấp",
    channels: "Kênh kết nối",
    settings: "Cài đặt",
    tests: "Kiểm thử",
    loc: "Dòng mã",
    eventsDay: "Sự kiện/24h",
    providerChain: "Chuỗi nhà cung cấp",
    vectorSearch: "Tìm kiếm Vector",
    mode: "Chế độ",
    pairedDevices: "Thiết bị ghép nối",
    denied24h: "Từ chối (24h)",
    commands: "Lệnh",
    model: "Mô hình",
    dim: "Chiều",
    failures: "lỗi",
    filesCrates: "tệp, {crates} gói",
    embedded: "đã nhúng",
    avgLatency: "độ trễ TB",
    totalEntries: "Tổng bản ghi",
    withEmbeddings: "có nhúng vector",
    embeddingModel: "Mô hình nhúng",
    dimensions: "chiều",
    searchModes: "Chế độ tìm kiếm",
    searchModesDesc: "FTS5 · Vector · Kết hợp (RRF)",
    denyByDefault: "từ chối mặc định",
    commandsSecured: "Lệnh được bảo vệ",
    perCommandAuth: "phân quyền từng lệnh",
    pairedDev: "thiết bị ghép nối",
    kernelTerminal: "Dòng lệnh OneClaw",
    inputPlaceholder: "ghi nhớ, tìm lại, trạng thái, hỏi ...",
    footerInfo: "OneClaw v1.6.0 · 3 gói · 550 kiểm thử · {n} bản ghi đã nhúng",
    welcomeSub: "v1.6.0 — SCALPEL · 550 kiểm thử · 3 gói · 6 nhà cung cấp",
    system: "hệ thống",
    you: "bạn",
    oneclaw: "oneclaw",
    // Chat responses
    respRemember: "Đã ghi nhớ (kèm nhúng): {c}\n\n✓ Lưu với nomic-embed-text (768 chiều)\n✓ Đã đánh chỉ mục FTS5",
    respRecall: 'Ký ức tìm được cho "{q}":\n1. [điểm:0.91] Nhiệt độ phòng khách 31°C\n2. [điểm:0.67] Điều hoà set 26°C\n\nTìm kết hợp: FTS5 + vector + RRF',
    respStatus: "OneClaw v1.6.0 SCALPEL\nThời gian chạy: {uptime}\nNhà cung cấp: 2 trực tuyến, 1 chờ\nBộ nhớ: {total} bản ghi ({embedded} đã nhúng)\nBảo mật: {mode}\nBus sự kiện: {bus} ({latency}ms)",
    respHealth: "Tất cả hệ thống hoạt động tốt.\n✓ Bộ nhớ: bình thường\n✓ Nhà cung cấp: 2/3 trực tuyến\n✓ Bảo mật: đang áp dụng\n✓ Sự kiện: đang chạy (4.2ms TB)",
    respDefault: 'Đang xử lý: "{input}"\n\nChưa chọn nhà cung cấp LLM. Dùng "ask <câu hỏi>" để hỏi AI hoặc thử: status, health, providers, remember, recall',
    respAsk: 'Phân tích AI ({q}):\n\nDựa trên ngữ cảnh và chuỗi nhà cung cấp (Anthropic → Ollama → DeepSeek), đây là phản hồi:\n\nKernel OneClaw xử lý truy vấn qua Smart Router, phân loại độ phức tạp "trung bình". DefaultContextManager đã đưa các bản ghi nhớ liên quan vào ngữ cảnh.\n\nLưu ý: Đây là phản hồi mô phỏng. Kết nối nhà cung cấp LLM để có câu trả lời AI thực.',
    // Settings tab
    settingsAppearance: "Giao diện",
    settingsTheme: "Chủ đề",
    settingsThemeDesc: "Chuyển đổi giữa chế độ tối và sáng",
    settingsLang: "Ngôn ngữ",
    settingsLangDesc: "Ngôn ngữ giao diện (Tiếng Việt / English)",
    settingsSystem: "Thông tin hệ thống",
    settingsBinary: "Kích thước nhị phân",
    settingsRuntime: "Thời gian chạy",
    settingsEdition: "Phiên bản Rust",
    settingsVersion: "Phiên bản",
    settingsAbout: "Giới thiệu",
    settingsAboutDesc: "OneClaw — Kernel AI Agent bằng Rust cho Edge/IoT. Kiến trúc 5 lớp trait-driven. Giấy phép kép MIT/Apache 2.0.",
  },
};

const MOCK_STATS = {
  providers: [
    { name: "Anthropic", status: "online", latency: 142, model: "claude-sonnet-4-20250514" },
    { name: "Ollama", status: "online", latency: 38, model: "llama3.2" },
    { name: "DeepSeek", status: "standby", latency: null, model: "deepseek-chat" },
  ],
  memory: { total: 1847, embedded: 1203, fts_only: 644, model: "nomic-embed-text", dim: 768 },
  security: { paired: 3, denied_24h: 12, mode: "production", commands_secured: 15 },
  events: { total_24h: 4821, bus: "async", latency_ms: 4.2, subscribers: 7 },
  system: { uptime: "14d 7h 23m", binary: "3.5MB", tests: 550, crates: 3, loc: 19193 },
  channels: [
    { name: "CLI", status: "active", icon: Terminal },
    { name: "MQTT", status: "active", icon: Podcast },
    { name: "TCP", status: "idle", icon: Server },
    { name: "Telegram", status: "active", icon: MessageSquare },
  ],
};

// ═══════════════════════════════════════════
// TOGGLE SWITCH COMPONENT
// ═══════════════════════════════════════════

function ToggleSwitch({ checked, onChange, iconOn, iconOff, labelOn, labelOff, theme }) {
  return (
    <button
      onClick={onChange}
      title={checked ? labelOn : labelOff}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "5px 10px",
        borderRadius: 20,
        border: `1px solid ${theme.border}`,
        background: checked ? theme.accentDim : theme.iconBg,
        cursor: "pointer",
        transition: "all 0.2s ease",
      }}
    >
      {checked ? iconOn : iconOff}
      <span style={{ fontSize: 10, color: theme.textDim, fontFamily: "'JetBrains Mono', monospace", userSelect: "none" }}>
        {checked ? labelOn : labelOff}
      </span>
    </button>
  );
}

// ═══════════════════════════════════════════
// SUB-COMPONENTS (theme-aware)
// ═══════════════════════════════════════════

function StatusDot({ status }) {
  const color =
    status === "online" || status === "active"
      ? ACCENT
      : status === "standby" || status === "idle"
        ? "#f59e0b"
        : "#ef4444";
  return (
    <span
      style={{
        width: 6,
        height: 6,
        borderRadius: "50%",
        background: color,
        display: "inline-block",
        boxShadow: `0 0 6px ${color}40`,
      }}
    />
  );
}

function MetricCard({ icon: Icon, label, value, sub, accent = false, theme, onClick }) {
  return (
    <div
      onClick={onClick}
      style={{
        background: theme.surface2,
        border: `1px solid ${theme.border}`,
        borderRadius: 12,
        padding: "16px 18px",
        display: "flex",
        flexDirection: "column",
        gap: 10,
        transition: "all 0.2s ease",
        cursor: onClick ? "pointer" : "default",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.borderColor = theme.cardHoverBorder;
        e.currentTarget.style.background = theme.surface3;
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.borderColor = theme.border;
        e.currentTarget.style.background = theme.surface2;
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <div
          style={{
            width: 28,
            height: 28,
            borderRadius: 7,
            background: accent ? theme.accentDim : theme.iconBg,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Icon size={14} color={accent ? theme.accent : theme.textDim} strokeWidth={1.8} />
        </div>
        <span style={{ fontSize: 11, color: theme.textDim, letterSpacing: "0.04em", textTransform: "uppercase", fontFamily: "'DM Sans', sans-serif" }}>
          {label}
        </span>
      </div>
      <div>
        <div style={{ fontSize: 22, fontWeight: 600, color: theme.text, fontFamily: "'Newsreader', serif", letterSpacing: "-0.02em" }}>
          {value}
        </div>
        {sub && <div style={{ fontSize: 11, color: theme.textMuted, marginTop: 2, fontFamily: "'DM Sans', sans-serif" }}>{sub}</div>}
      </div>
    </div>
  );
}

function ProviderRow({ provider, theme }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "8px 0",
        borderBottom: `1px solid ${theme.border}`,
      }}
    >
      <StatusDot status={provider.status} />
      <span style={{ fontSize: 13, color: theme.text, flex: 1, fontFamily: "'DM Sans', sans-serif" }}>{provider.name}</span>
      <span style={{ fontSize: 11, color: theme.textMuted, fontFamily: "'JetBrains Mono', monospace" }}>{provider.model}</span>
      {provider.latency && (
        <span
          style={{
            fontSize: 10,
            color: provider.latency < 100 ? theme.accent : theme.warn,
            background: provider.latency < 100 ? theme.accentDim : theme.warnBg,
            padding: "2px 7px",
            borderRadius: 4,
            fontFamily: "'JetBrains Mono', monospace",
          }}
        >
          {provider.latency}ms
        </span>
      )}
    </div>
  );
}

function ChatMessage({ message, isLast, theme, t }) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";

  return (
    <div
      style={{
        display: "flex",
        gap: 12,
        padding: "16px 20px",
        animation: isLast ? "fadeSlideIn 0.35s ease" : "none",
        background: isUser ? theme.chatUser : isSystem ? theme.chatSystem : theme.chatAssistant,
        borderBottom: `1px solid ${theme.border}`,
      }}
    >
      <div
        style={{
          width: 28,
          height: 28,
          borderRadius: isUser ? 8 : 14,
          background: isSystem
            ? theme.accentDim
            : isUser
              ? theme.chatUserAvatar
              : `linear-gradient(135deg, ${theme.accentDim}, transparent)`,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
          marginTop: 2,
        }}
      >
        {isSystem ? (
          <Zap size={13} color={theme.accent} strokeWidth={2} />
        ) : isUser ? (
          <User size={13} color={theme.textDim} strokeWidth={2} />
        ) : (
          <Sparkles size={13} color={theme.accent} strokeWidth={2} />
        )}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}>
          <span
            style={{
              fontSize: 12,
              fontWeight: 600,
              color: isSystem ? theme.accentText : isUser ? theme.text : theme.accentText,
              fontFamily: "'DM Sans', sans-serif",
              letterSpacing: "0.01em",
            }}
          >
            {isSystem ? t.system : isUser ? t.you : t.oneclaw}
          </span>
          <span style={{ fontSize: 10, color: theme.textMuted, fontFamily: "'JetBrains Mono', monospace" }}>{message.time}</span>
        </div>
        <div
          style={{
            fontSize: 13.5,
            lineHeight: 1.65,
            color: isSystem ? theme.accentText : theme.text,
            opacity: isSystem ? 0.8 : 1,
            fontFamily: isUser ? "'JetBrains Mono', monospace" : "'DM Sans', sans-serif",
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
          }}
        >
          {message.content}
        </div>
      </div>
    </div>
  );
}

function NavItem({ icon: Icon, label, active, onClick, collapsed, theme }) {
  return (
    <button
      onClick={onClick}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: collapsed ? "10px" : "9px 12px",
        borderRadius: 8,
        border: "none",
        background: active ? theme.accentDim : "transparent",
        color: active ? theme.accentText : theme.textDim,
        cursor: "pointer",
        width: "100%",
        justifyContent: collapsed ? "center" : "flex-start",
        transition: "all 0.15s ease",
        fontFamily: "'DM Sans', sans-serif",
        fontSize: 13,
        fontWeight: active ? 500 : 400,
      }}
      onMouseEnter={(e) => {
        if (!active) e.currentTarget.style.background = theme.hoverBg;
      }}
      onMouseLeave={(e) => {
        if (!active) e.currentTarget.style.background = "transparent";
      }}
    >
      <Icon size={16} strokeWidth={1.7} />
      {!collapsed && <span>{label}</span>}
    </button>
  );
}

// ═══════════════════════════════════════════
// MAIN DASHBOARD
// ═══════════════════════════════════════════

export default function OneclawDashboard() {
  const [isDark, setIsDark] = useState(true);
  const [lang, setLang] = useState("vi");
  const [messages, setMessages] = useState([]);
  const [input, setInput] = useState("");
  const [activeTab, setActiveTab] = useState("dashboard");
  const [sideCollapsed, setSideCollapsed] = useState(false);
  const chatEndRef = useRef(null);
  const inputRef = useRef(null);

  const theme = isDark ? themes.dark : themes.light;
  const t = i18n[lang];

  // Initialize messages with current language
  useEffect(() => {
    setMessages([
      {
        role: "system",
        content: lang === "vi"
          ? "OneClaw v1.6.0 — SCALPEL. Kernel sẵn sàng. 3 nhà cung cấp trực tuyến, tìm kiếm vector đang hoạt động."
          : "OneClaw v1.6.0 — SCALPEL. Kernel ready. 3 providers online, vector search active.",
        time: "09:00",
      },
      {
        role: "user",
        content: "remember Nhiệt độ phòng khách đang 31°C, cần bật quạt",
        time: "09:12",
      },
      {
        role: "assistant",
        content: lang === "vi"
          ? "Đã ghi nhớ (kèm nhúng): Nhiệt độ phòng khách đang 31°C, cần bật quạt\n\n✓ Lưu với nomic-embed-text (768 chiều)\n✓ Đã đánh chỉ mục FTS5\n✓ Có thể tìm kiếm tương đồng vector"
          : "Remembered (with embedding): Nhiệt độ phòng khách đang 31°C, cần bật quạt\n\n✓ Stored with nomic-embed-text (768d)\n✓ FTS5 indexed\n✓ Vector similarity searchable",
        time: "09:12",
      },
      {
        role: "user",
        content: "recall phòng nóng",
        time: "09:15",
      },
      {
        role: "assistant",
        content: lang === "vi"
          ? "Ký ức tìm được:\n1. [điểm:0.87] Nhiệt độ phòng khách đang 31°C, cần bật quạt\n2. [điểm:0.43] Điều hoà phòng ngủ set 26°C\n\nTìm kết hợp: FTS5 + cosine similarity + RRF"
          : "Recalled memories:\n1. [score:0.87] Nhiệt độ phòng khách đang 31°C, cần bật quạt\n2. [score:0.43] Điều hoà phòng ngủ set 26°C\n\nHybrid search: FTS5 + cosine similarity + RRF fusion",
        time: "09:15",
      },
    ]);
  }, [lang]);

  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleSend = () => {
    if (!input.trim()) return;
    const now = new Date();
    const time = `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;

    setMessages((prev) => [...prev, { role: "user", content: input, time }]);

    const userInput = input.toLowerCase();
    const rawInput = input;
    setInput("");

    setTimeout(() => {
      let response = "";
      if (userInput.startsWith("remember ")) {
        const content = rawInput.slice(9);
        response = t.respRemember.replace("{c}", content);
      } else if (userInput.startsWith("recall ")) {
        const query = rawInput.slice(7);
        response = t.respRecall.replace("{q}", query);
      } else if (userInput.startsWith("ask ")) {
        const question = rawInput.slice(4);
        response = t.respAsk.replace("{q}", question);
      } else if (userInput === "status") {
        response = t.respStatus
          .replace("{uptime}", MOCK_STATS.system.uptime)
          .replace("{total}", String(MOCK_STATS.memory.total))
          .replace("{embedded}", String(MOCK_STATS.memory.embedded))
          .replace("{mode}", MOCK_STATS.security.mode)
          .replace("{bus}", MOCK_STATS.events.bus)
          .replace("{latency}", String(MOCK_STATS.events.latency_ms));
      } else if (userInput === "providers") {
        response = MOCK_STATS.providers.map((p) => `${p.status === "online" ? "●" : "○"} ${p.name} — ${p.model}${p.latency ? ` (${p.latency}ms)` : ""}`).join("\n");
      } else if (userInput === "health") {
        response = t.respHealth;
      } else {
        response = t.respDefault.replace("{input}", rawInput);
      }
      setMessages((prev) => [...prev, { role: "assistant", content: response, time }]);
    }, 400);
  };

  const tabLabel = (key) => {
    const map = {
      dashboard: t.dashboard,
      chat: t.terminal,
      memory: t.memory,
      security: t.security,
      providers: t.providers,
      channels: t.channels,
      settings: t.settings,
    };
    return map[key] || key;
  };

  const showChat = activeTab === "chat" || activeTab === "dashboard";

  return (
    <div
      style={{
        width: "100vw",
        height: "100vh",
        background: theme.surface,
        color: theme.text,
        display: "flex",
        overflow: "hidden",
        fontFamily: "'DM Sans', sans-serif",
        transition: "background 0.3s ease, color 0.3s ease",
      }}
    >
      <style>{`
        @import url('https://fonts.googleapis.com/css2?family=DM+Sans:ital,wght@0,300;0,400;0,500;0,600;0,700;1,400&family=Newsreader:ital,wght@0,400;0,600;0,700;1,400&family=JetBrains+Mono:wght@300;400;500&display=swap');
        @keyframes fadeSlideIn {
          from { opacity: 0; transform: translateY(6px); }
          to { opacity: 1; transform: translateY(0); }
        }
        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.5; }
        }
        * { box-sizing: border-box; margin: 0; padding: 0; }
        ::-webkit-scrollbar { width: 4px; }
        ::-webkit-scrollbar-track { background: transparent; }
        ::-webkit-scrollbar-thumb { background: ${theme.scrollThumb}; border-radius: 4px; }
        ::-webkit-scrollbar-thumb:hover { background: ${theme.scrollThumbHover}; }
        input::placeholder { color: ${theme.textMuted} !important; }
      `}</style>

      {/* ═══ SIDEBAR ═══ */}
      <div
        style={{
          width: sideCollapsed ? 56 : 200,
          height: "100%",
          background: theme.surface,
          borderRight: `1px solid ${theme.border}`,
          display: "flex",
          flexDirection: "column",
          padding: sideCollapsed ? "16px 8px" : "16px 12px",
          transition: "width 0.2s ease, background 0.3s ease",
          flexShrink: 0,
        }}
      >
        {/* Logo */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            padding: "4px 4px 20px",
            justifyContent: sideCollapsed ? "center" : "flex-start",
            cursor: "pointer",
          }}
          onClick={() => setSideCollapsed(!sideCollapsed)}
        >
          <div
            style={{
              width: 30,
              height: 30,
              borderRadius: 9,
              background: `linear-gradient(135deg, ${theme.accentDim}, transparent)`,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              border: `1px solid ${isDark ? "rgba(52,211,153,0.15)" : "rgba(16,185,129,0.2)"}`,
            }}
          >
            <Box size={15} color={theme.accent} strokeWidth={2} />
          </div>
          {!sideCollapsed && (
            <div>
              <div style={{ fontSize: 14, fontWeight: 600, color: theme.text, letterSpacing: "-0.01em" }}>OneClaw</div>
              <div style={{ fontSize: 9.5, color: theme.accentText, fontFamily: "'JetBrains Mono', monospace", letterSpacing: "0.05em" }}>v1.6.0</div>
            </div>
          )}
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          <NavItem icon={LayoutDashboard} label={t.dashboard} active={activeTab === "dashboard"} onClick={() => setActiveTab("dashboard")} collapsed={sideCollapsed} theme={theme} />
          <NavItem icon={MessageSquare} label={t.chat} active={activeTab === "chat"} onClick={() => setActiveTab("chat")} collapsed={sideCollapsed} theme={theme} />
          <NavItem icon={Database} label={t.memory} active={activeTab === "memory"} onClick={() => setActiveTab("memory")} collapsed={sideCollapsed} theme={theme} />
          <NavItem icon={ShieldCheck} label={t.security} active={activeTab === "security"} onClick={() => setActiveTab("security")} collapsed={sideCollapsed} theme={theme} />
          <NavItem icon={Layers} label={t.providers} active={activeTab === "providers"} onClick={() => setActiveTab("providers")} collapsed={sideCollapsed} theme={theme} />
          <NavItem icon={Radio} label={t.channels} active={activeTab === "channels"} onClick={() => setActiveTab("channels")} collapsed={sideCollapsed} theme={theme} />
        </div>

        <div style={{ flex: 1 }} />

        {/* Bottom */}
        <div style={{ borderTop: `1px solid ${theme.border}`, paddingTop: 12, display: "flex", flexDirection: "column", gap: 2 }}>
          <NavItem icon={Settings} label={t.settings} active={activeTab === "settings"} onClick={() => setActiveTab("settings")} collapsed={sideCollapsed} theme={theme} />
        </div>
      </div>

      {/* ═══ MAIN AREA ═══ */}
      <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
        {/* Top bar */}
        <div
          style={{
            height: 52,
            borderBottom: `1px solid ${theme.border}`,
            display: "flex",
            alignItems: "center",
            padding: "0 24px",
            justifyContent: "space-between",
            flexShrink: 0,
            background: theme.surface,
            transition: "background 0.3s ease",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <span style={{ fontSize: 14, fontWeight: 500, color: theme.text }}>
              {tabLabel(activeTab)}
            </span>
            <div style={{ width: 1, height: 16, background: theme.border }} />
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <StatusDot status="online" />
              <span style={{ fontSize: 11, color: theme.textDim, fontFamily: "'JetBrains Mono', monospace" }}>
                {MOCK_STATS.system.uptime}
              </span>
            </div>
          </div>

          {/* ═══ TOGGLE SWITCHES ═══ */}
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <ToggleSwitch
              checked={lang === "vi"}
              onChange={() => setLang(lang === "vi" ? "en" : "vi")}
              iconOn={<Languages size={12} color={theme.accentText} strokeWidth={2} />}
              iconOff={<Languages size={12} color={theme.textDim} strokeWidth={2} />}
              labelOn="VI"
              labelOff="EN"
              theme={theme}
            />
            <ToggleSwitch
              checked={isDark}
              onChange={() => setIsDark(!isDark)}
              iconOn={<Moon size={12} color={theme.accentText} strokeWidth={2} />}
              iconOff={<Sun size={12} color={theme.textDim} strokeWidth={2} />}
              labelOn={lang === "vi" ? "Tối" : "Dark"}
              labelOff={lang === "vi" ? "Sáng" : "Light"}
              theme={theme}
            />

            <div style={{ width: 1, height: 20, background: theme.border, margin: "0 4px" }} />

            {MOCK_STATS.channels.map((ch) => (
              <button
                key={ch.name}
                title={`${ch.name}: ${ch.status}`}
                onClick={() => setActiveTab(ch.name === "CLI" ? "chat" : "channels")}
                style={{
                  width: 28,
                  height: 28,
                  borderRadius: 6,
                  background: theme.iconBg,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  border: "none",
                  cursor: "pointer",
                  transition: "background 0.15s ease",
                }}
                onMouseEnter={(e) => e.currentTarget.style.background = theme.accentDim}
                onMouseLeave={(e) => e.currentTarget.style.background = theme.iconBg}
              >
                <ch.icon size={13} color={ch.status === "active" ? theme.accent : theme.textMuted} strokeWidth={1.7} />
              </button>
            ))}
          </div>
        </div>

        {/* Content */}
        <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
          {/* ═══ DASHBOARD PANELS ═══ */}
          {activeTab === "dashboard" && (
            <div
              style={{
                width: "42%",
                minWidth: 340,
                maxWidth: 480,
                borderRight: `1px solid ${theme.border}`,
                overflow: "auto",
                padding: 20,
                display: "flex",
                flexDirection: "column",
                gap: 16,
              }}
            >
              {/* Metrics Grid */}
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
                <MetricCard icon={Gauge} label={t.tests} value="550" sub={`0 ${t.failures}`} accent theme={theme} onClick={() => setActiveTab("security")} />
                <MetricCard icon={Braces} label={t.loc} value="19.2K" sub={t.filesCrates.replace("{crates}", "3").replace("67", "67")} theme={theme} onClick={() => setActiveTab("providers")} />
                <MetricCard icon={Brain} label={t.memory} value={`${MOCK_STATS.memory.total}`} sub={`${MOCK_STATS.memory.embedded} ${t.embedded}`} accent theme={theme} onClick={() => setActiveTab("memory")} />
                <MetricCard icon={Zap} label={t.eventsDay} value={`${MOCK_STATS.events.total_24h}`} sub={`${MOCK_STATS.events.latency_ms}ms ${t.avgLatency}`} theme={theme} onClick={() => setActiveTab("channels")} />
              </div>

              {/* Providers */}
              <div onClick={() => setActiveTab("providers")} style={{ background: theme.surface2, border: `1px solid ${theme.border}`, borderRadius: 12, padding: 16, transition: "all 0.2s ease", cursor: "pointer" }}
                onMouseEnter={(e) => { e.currentTarget.style.borderColor = theme.cardHoverBorder; }}
                onMouseLeave={(e) => { e.currentTarget.style.borderColor = theme.border; }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 12 }}>
                  <GitBranch size={13} color={theme.textDim} strokeWidth={1.8} />
                  <span style={{ fontSize: 11, color: theme.textDim, textTransform: "uppercase", letterSpacing: "0.04em" }}>
                    {t.providerChain}
                  </span>
                </div>
                {MOCK_STATS.providers.map((p) => (
                  <ProviderRow key={p.name} provider={p} theme={theme} />
                ))}
              </div>

              {/* Security */}
              <div onClick={() => setActiveTab("security")} style={{ background: theme.surface2, border: `1px solid ${theme.border}`, borderRadius: 12, padding: 16, transition: "all 0.2s ease", cursor: "pointer" }}
                onMouseEnter={(e) => { e.currentTarget.style.borderColor = theme.cardHoverBorder; }}
                onMouseLeave={(e) => { e.currentTarget.style.borderColor = theme.border; }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }}>
                  <Shield size={13} color={theme.textDim} strokeWidth={1.8} />
                  <span style={{ fontSize: 11, color: theme.textDim, textTransform: "uppercase", letterSpacing: "0.04em" }}>{t.security}</span>
                </div>
                <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
                  <div>
                    <div style={{ fontSize: 10, color: theme.textMuted, marginBottom: 2 }}>{t.mode}</div>
                    <div style={{ display: "flex", alignItems: "center", gap: 5 }}>
                      <Lock size={11} color={theme.accent} />
                      <span style={{ fontSize: 12, color: theme.accentText, fontFamily: "'JetBrains Mono', monospace" }}>{MOCK_STATS.security.mode}</span>
                    </div>
                  </div>
                  <div>
                    <div style={{ fontSize: 10, color: theme.textMuted, marginBottom: 2 }}>{t.pairedDevices}</div>
                    <span style={{ fontSize: 16, fontWeight: 600, color: theme.text, fontFamily: "'Newsreader', serif" }}>{MOCK_STATS.security.paired}</span>
                  </div>
                  <div>
                    <div style={{ fontSize: 10, color: theme.textMuted, marginBottom: 2 }}>{t.denied24h}</div>
                    <span style={{ fontSize: 16, fontWeight: 600, color: theme.warn, fontFamily: "'Newsreader', serif" }}>{MOCK_STATS.security.denied_24h}</span>
                  </div>
                  <div>
                    <div style={{ fontSize: 10, color: theme.textMuted, marginBottom: 2 }}>{t.commands}</div>
                    <span style={{ fontSize: 16, fontWeight: 600, color: theme.text, fontFamily: "'Newsreader', serif" }}>{MOCK_STATS.security.commands_secured}</span>
                  </div>
                </div>
              </div>

              {/* Vector Memory */}
              <div onClick={() => setActiveTab("memory")} style={{ background: theme.surface2, border: `1px solid ${theme.border}`, borderRadius: 12, padding: 16, transition: "all 0.2s ease", cursor: "pointer" }}
                onMouseEnter={(e) => { e.currentTarget.style.borderColor = theme.cardHoverBorder; }}
                onMouseLeave={(e) => { e.currentTarget.style.borderColor = theme.border; }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }}>
                  <Search size={13} color={theme.textDim} strokeWidth={1.8} />
                  <span style={{ fontSize: 11, color: theme.textDim, textTransform: "uppercase", letterSpacing: "0.04em" }}>{t.vectorSearch}</span>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 16, marginBottom: 10 }}>
                  <div style={{ flex: 1 }}>
                    <div style={{ height: 6, background: theme.progressTrack, borderRadius: 3, overflow: "hidden" }}>
                      <div
                        style={{
                          width: `${(MOCK_STATS.memory.embedded / MOCK_STATS.memory.total) * 100}%`,
                          height: "100%",
                          background: `linear-gradient(90deg, ${theme.accent}, ${isDark ? "rgba(52,211,153,0.4)" : "rgba(16,185,129,0.35)"})`,
                          borderRadius: 3,
                          transition: "width 0.5s ease",
                        }}
                      />
                    </div>
                  </div>
                  <span style={{ fontSize: 12, color: theme.text, fontFamily: "'JetBrains Mono', monospace", whiteSpace: "nowrap" }}>
                    {Math.round((MOCK_STATS.memory.embedded / MOCK_STATS.memory.total) * 100)}%
                  </span>
                </div>
                <div style={{ display: "flex", gap: 16 }}>
                  <div style={{ fontSize: 10, color: theme.textMuted }}>
                    {t.model}: <span style={{ color: theme.text, fontFamily: "'JetBrains Mono', monospace" }}>{MOCK_STATS.memory.model}</span>
                  </div>
                  <div style={{ fontSize: 10, color: theme.textMuted }}>
                    {t.dim}: <span style={{ color: theme.text, fontFamily: "'JetBrains Mono', monospace" }}>{MOCK_STATS.memory.dim}</span>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* ═══ CHAT AREA ═══ */}
          {showChat && (
            <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
              {/* Messages */}
              <div style={{ flex: 1, overflow: "auto" }}>
                {/* Welcome header */}
                {messages.length > 0 && activeTab === "chat" && (
                  <div style={{ padding: "32px 20px 0", textAlign: "center" }}>
                    <div
                      style={{
                        width: 40,
                        height: 40,
                        borderRadius: 20,
                        background: `linear-gradient(135deg, ${theme.accentDim}, transparent)`,
                        display: "inline-flex",
                        alignItems: "center",
                        justifyContent: "center",
                        marginBottom: 12,
                        border: `1px solid ${isDark ? "rgba(52,211,153,0.1)" : "rgba(16,185,129,0.15)"}`,
                      }}
                    >
                      <Box size={18} color={theme.accent} strokeWidth={1.5} />
                    </div>
                    <div style={{ fontSize: 14, color: theme.textDim, marginBottom: 4, fontFamily: "'DM Sans', sans-serif" }}>
                      {t.kernelTerminal}
                    </div>
                    <div style={{ fontSize: 11, color: theme.textMuted, fontFamily: "'JetBrains Mono', monospace" }}>
                      {t.welcomeSub}
                    </div>
                  </div>
                )}

                <div style={{ paddingTop: activeTab === "chat" ? 20 : 0 }}>
                  {messages.map((msg, i) => (
                    <ChatMessage key={i} message={msg} isLast={i === messages.length - 1} theme={theme} t={t} />
                  ))}
                </div>
                <div ref={chatEndRef} />
              </div>

              {/* Input area */}
              <div
                style={{
                  padding: "12px 20px 16px",
                  borderTop: `1px solid ${theme.border}`,
                  background: theme.surface,
                  transition: "background 0.3s ease",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    background: theme.inputBg,
                    border: `1px solid ${theme.border}`,
                    borderRadius: 12,
                    padding: "6px 6px 6px 16px",
                    transition: "border-color 0.15s ease, background 0.3s ease",
                  }}
                  onFocus={(e) => (e.currentTarget.style.borderColor = theme.cardHoverBorder)}
                  onBlur={(e) => (e.currentTarget.style.borderColor = theme.border)}
                >
                  <ChevronRight size={14} color={theme.textMuted} strokeWidth={2} />
                  <input
                    ref={inputRef}
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleSend()}
                    placeholder={t.inputPlaceholder}
                    style={{
                      flex: 1,
                      background: "transparent",
                      border: "none",
                      outline: "none",
                      color: theme.text,
                      fontSize: 13,
                      fontFamily: "'JetBrains Mono', monospace",
                      letterSpacing: "0.01em",
                    }}
                  />
                  <button
                    onClick={handleSend}
                    disabled={!input.trim()}
                    style={{
                      width: 32,
                      height: 32,
                      borderRadius: 8,
                      border: "none",
                      background: input.trim() ? theme.accent : theme.sendDisabled,
                      color: input.trim() ? (isDark ? "#0f1117" : "#ffffff") : theme.sendDisabledColor,
                      cursor: input.trim() ? "pointer" : "default",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      transition: "all 0.15s ease",
                    }}
                  >
                    <ArrowUpRight size={15} strokeWidth={2} />
                  </button>
                </div>
                <div style={{ display: "flex", justifyContent: "center", marginTop: 8 }}>
                  <span style={{ fontSize: 10, color: theme.textMuted }}>
                    {t.footerInfo.replace("{n}", String(MOCK_STATS.memory.embedded))}
                  </span>
                </div>
              </div>
            </div>
          )}

          {/* ═══ DETAIL PANELS (non-dashboard, non-chat) ═══ */}
          {!showChat && (
            <div style={{ flex: 1, overflow: "auto", padding: 24 }}>
              <div style={{ maxWidth: 600, margin: "0 auto", display: "flex", flexDirection: "column", gap: 16 }}>
                <div style={{ fontSize: 20, fontWeight: 600, fontFamily: "'Newsreader', serif", color: theme.text }}>
                  {tabLabel(activeTab)}
                </div>

                {activeTab === "memory" && (
                  <>
                    <MetricCard icon={Database} label={t.totalEntries} value={MOCK_STATS.memory.total} sub={`${MOCK_STATS.memory.embedded} ${t.withEmbeddings}`} accent theme={theme} />
                    <MetricCard icon={Brain} label={t.embeddingModel} value={MOCK_STATS.memory.model} sub={`${MOCK_STATS.memory.dim} ${t.dimensions}`} theme={theme} />
                    <MetricCard icon={Search} label={t.searchModes} value="3" sub={t.searchModesDesc} accent theme={theme} />
                  </>
                )}

                {activeTab === "security" && (
                  <>
                    <MetricCard icon={Lock} label={t.mode} value={MOCK_STATS.security.mode} sub={t.denyByDefault} accent theme={theme} />
                    <MetricCard icon={ShieldCheck} label={t.commandsSecured} value={MOCK_STATS.security.commands_secured} sub={t.perCommandAuth} theme={theme} />
                    <MetricCard icon={Shield} label={t.denied24h} value={MOCK_STATS.security.denied_24h} sub={`${MOCK_STATS.security.paired} ${t.pairedDev}`} theme={theme} />
                  </>
                )}

                {activeTab === "providers" &&
                  MOCK_STATS.providers.map((p) => (
                    <div
                      key={p.name}
                      style={{
                        background: theme.surface2,
                        border: `1px solid ${theme.border}`,
                        borderRadius: 12,
                        padding: 16,
                        display: "flex",
                        alignItems: "center",
                        gap: 12,
                        transition: "background 0.3s ease",
                      }}
                    >
                      <StatusDot status={p.status} />
                      <div style={{ flex: 1 }}>
                        <div style={{ fontSize: 14, fontWeight: 500, color: theme.text }}>{p.name}</div>
                        <div style={{ fontSize: 11, color: theme.textMuted, fontFamily: "'JetBrains Mono', monospace" }}>{p.model}</div>
                      </div>
                      {p.latency && (
                        <span style={{ fontSize: 12, color: p.latency < 100 ? theme.accent : theme.warn, fontFamily: "'JetBrains Mono', monospace" }}>
                          {p.latency}ms
                        </span>
                      )}
                    </div>
                  ))}

                {activeTab === "settings" && (
                  <>
                    {/* Appearance */}
                    <div style={{ background: theme.surface2, border: `1px solid ${theme.border}`, borderRadius: 12, padding: 16, display: "flex", flexDirection: "column", gap: 14 }}>
                      <span style={{ fontSize: 11, color: theme.textDim, textTransform: "uppercase", letterSpacing: "0.04em" }}>{t.settingsAppearance}</span>
                      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                        <div>
                          <div style={{ fontSize: 14, fontWeight: 500, color: theme.text }}>{t.settingsTheme}</div>
                          <div style={{ fontSize: 11, color: theme.textMuted }}>{t.settingsThemeDesc}</div>
                        </div>
                        <ToggleSwitch
                          checked={isDark}
                          onChange={() => setIsDark(!isDark)}
                          iconOn={<Moon size={12} color={theme.accentText} strokeWidth={2} />}
                          iconOff={<Sun size={12} color={theme.textDim} strokeWidth={2} />}
                          labelOn={lang === "vi" ? "Tối" : "Dark"}
                          labelOff={lang === "vi" ? "Sáng" : "Light"}
                          theme={theme}
                        />
                      </div>
                      <div style={{ height: 1, background: theme.border }} />
                      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                        <div>
                          <div style={{ fontSize: 14, fontWeight: 500, color: theme.text }}>{t.settingsLang}</div>
                          <div style={{ fontSize: 11, color: theme.textMuted }}>{t.settingsLangDesc}</div>
                        </div>
                        <ToggleSwitch
                          checked={lang === "vi"}
                          onChange={() => setLang(lang === "vi" ? "en" : "vi")}
                          iconOn={<Languages size={12} color={theme.accentText} strokeWidth={2} />}
                          iconOff={<Languages size={12} color={theme.textDim} strokeWidth={2} />}
                          labelOn="VI"
                          labelOff="EN"
                          theme={theme}
                        />
                      </div>
                    </div>
                    {/* System Info */}
                    <div style={{ background: theme.surface2, border: `1px solid ${theme.border}`, borderRadius: 12, padding: 16, display: "flex", flexDirection: "column", gap: 10 }}>
                      <span style={{ fontSize: 11, color: theme.textDim, textTransform: "uppercase", letterSpacing: "0.04em" }}>{t.settingsSystem}</span>
                      {[
                        [t.settingsVersion, "v1.6.0 — SCALPEL"],
                        [t.settingsEdition, "Rust 2024"],
                        [t.settingsRuntime, "Tokio async"],
                        [t.settingsBinary, `${MOCK_STATS.system.binary}`],
                      ].map(([label, val]) => (
                        <div key={label} style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                          <span style={{ fontSize: 12, color: theme.textMuted }}>{label}</span>
                          <span style={{ fontSize: 12, color: theme.text, fontFamily: "'JetBrains Mono', monospace" }}>{val}</span>
                        </div>
                      ))}
                    </div>
                    {/* About */}
                    <div style={{ background: theme.surface2, border: `1px solid ${theme.border}`, borderRadius: 12, padding: 16 }}>
                      <span style={{ fontSize: 11, color: theme.textDim, textTransform: "uppercase", letterSpacing: "0.04em" }}>{t.settingsAbout}</span>
                      <p style={{ fontSize: 12, color: theme.textMuted, lineHeight: 1.6, marginTop: 8 }}>{t.settingsAboutDesc}</p>
                    </div>
                  </>
                )}

                {activeTab === "channels" &&
                  MOCK_STATS.channels.map((ch) => (
                    <div
                      key={ch.name}
                      style={{
                        background: theme.surface2,
                        border: `1px solid ${theme.border}`,
                        borderRadius: 12,
                        padding: 16,
                        display: "flex",
                        alignItems: "center",
                        gap: 12,
                        transition: "background 0.3s ease",
                      }}
                    >
                      <div
                        style={{
                          width: 32,
                          height: 32,
                          borderRadius: 8,
                          background: ch.status === "active" ? theme.accentDim : theme.iconBg,
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                        }}
                      >
                        <ch.icon size={15} color={ch.status === "active" ? theme.accent : theme.textMuted} strokeWidth={1.7} />
                      </div>
                      <div style={{ flex: 1 }}>
                        <div style={{ fontSize: 14, fontWeight: 500, color: theme.text }}>{ch.name}</div>
                      </div>
                      <StatusDot status={ch.status} />
                      <span style={{ fontSize: 11, color: theme.textDim }}>{ch.status}</span>
                    </div>
                  ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
