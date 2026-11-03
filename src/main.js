// PassVault 0.2 — shared UI for desktop (Linux) and Android.
const invoke = (...args) => window.__TAURI__.core.invoke(...args);
const app = document.getElementById("app");

// ---------------------------------------------------------------- state

const PREFS_KEY = "passvault.prefs";
const prefs = Object.assign(
  { autoLockMin: 10, clipClearSec: 30, genLength: 20, genUpper: true, genLower: true, genDigits: true, genSymbols: true },
  JSON.parse(localStorage.getItem(PREFS_KEY) || "{}")
);
const savePrefs = () => localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));

let state = {
  screen: "loading", // loading | setup | setup-confirm | link | unlock | main
  status: { vault_path: "", platform: "linux" },
  error: "",
  setupPending: null,
  accounts: [],
  tab: "passwords", // passwords | generator | settings | import
  selectedId: null, // account id | "__new__"
  editing: false,
  filter: "all", // all | fav | attention
  query: "",
  bio: null, // { available, has_saved_secret, kind, reason }
  importPreview: null,
};
const sync = { state: "off", msg: "", timer: null, interval: null, running: false };

const isAndroid = () => state.status.platform === "android";
const isDesktop = () => !isAndroid();
const wide = () => window.matchMedia("(min-width: 820px)").matches;

function setState(patch) {
  state = { ...state, ...patch };
  render();
}

// ---------------------------------------------------------------- icons

const ICONS = {
  lock: '<rect x="4" y="10.5" width="16" height="10.5" rx="3"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/>',
  key: '<circle cx="8" cy="15" r="4"/><path d="m10.8 12.2 8.7-8.7M17 6l2.5 2.5M15 8l2 2"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  star: '<path d="m12 3.5 2.6 5.4 5.9.8-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.8z"/>',
  copy: '<rect x="8.5" y="8.5" width="12" height="12" rx="3"/><path d="M15.5 8.5V6a2.5 2.5 0 0 0-2.5-2.5H6A2.5 2.5 0 0 0 3.5 6v7A2.5 2.5 0 0 0 6 15.5h2.5"/>',
  eye: '<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z"/><circle cx="12" cy="12" r="3"/>',
  eyeOff: '<path d="M3 3l18 18M10.6 6.1A9.6 9.6 0 0 1 12 6c6 0 9.5 6 9.5 6a17 17 0 0 1-3 3.6M6.4 7.4A16 16 0 0 0 2.5 12S6 18 12 18a9 9 0 0 0 4-.9"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/>',
  edit: '<path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16z"/><path d="m13.5 6.5 4 4"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3"/>',
  back: '<path d="M15 5l-7 7 7 7"/>',
  chev: '<path d="m9 5 7 7-7 7"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.6 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.6-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>',
  sync: '<path d="M20 11a8 8 0 0 0-14.3-4.9L4 8"/><path d="M4 4v4h4M4 13a8 8 0 0 0 14.3 4.9L20 16"/><path d="M20 20v-4h-4"/>',
  wand: '<path d="m4 20 11-11M14 4v2M19 9h2M17.5 5.5 19 4M12 7l5 5"/><path d="M18 14v2M6 3v2M5 4h2M17 15h2"/>',
  download: '<path d="M12 4v11M7 10.5l5 5 5-5M5 20h14"/>',
  face: '<path d="M4 8V6a2 2 0 0 1 2-2h2M16 4h2a2 2 0 0 1 2 2v2M20 16v2a2 2 0 0 1-2 2h-2M8 20H6a2 2 0 0 1-2-2v-2"/><path d="M9 9.5v1M15 9.5v1M12 9.5v3.5h-1M9.5 15.5s1 1 2.5 1 2.5-1 2.5-1"/>',
  finger: '<path d="M12 11v3a8 8 0 0 1-1.5 4.5M8.5 8.5A4.5 4.5 0 0 1 16.5 11v2c0 2-.4 3.6-1 5"/><path d="M5.5 15c.6-1 1-2.4 1-4a5.5 5.5 0 0 1 9-4.2M19.4 14.5c.4-1.2.6-2.4.6-3.5a8 8 0 0 0-13-6.2M9.5 20.5c1-1.6 1.5-3.4 1.5-5.5"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>',
  shield: '<path d="M12 3 4.5 6v5.5c0 4.6 3.2 8.4 7.5 9.5 4.3-1.1 7.5-4.9 7.5-9.5V6z"/><path d="m9 12 2 2 4-4"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
  check: '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
  cloud: '<path d="M7 18.5a4.5 4.5 0 0 1-.6-9 6 6 0 0 1 11.5 1.6A3.8 3.8 0 0 1 17.5 18.5z"/>',
  phone: '<rect x="6.5" y="2.5" width="11" height="19" rx="3"/><path d="M11 18.5h2"/>',
  alert: '<path d="M12 4 2.5 20h19z"/><path d="M12 10v4M12 17v.01"/>',
  open: '<path d="M14 4h6v6M20 4l-9 9M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4"/>',
  folder: '<path d="M3.5 7a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>',
  qr: '<rect x="4" y="4" width="6" height="6" rx="1"/><rect x="14" y="4" width="6" height="6" rx="1"/><rect x="4" y="14" width="6" height="6" rx="1"/><path d="M14 14h2v2h-2zM18 18h2v2h-2zM14 18h2M18 14h2"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8v.01"/>',
};
const icon = (name, cls = "") => `<svg class="i ${cls}" viewBox="0 0 24 24" aria-hidden="true">${ICONS[name] || ""}</svg>`;
const bioIcon = () => (state.bio?.kind === "face" ? "face" : "finger");
const bioName = () => (state.bio?.kind === "face" ? "Face unlock" : "Fingerprint unlock");

// ---------------------------------------------------------------- helpers

function esc(s) {
  return String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}
function el(tag, cls, html) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (html != null) e.innerHTML = html;
  return e;
}
const errText = (e) => String(e?.message ?? e).replace(/^Error:\s*/, "");

function toast(msg, ms = 2200) {
  document.querySelectorAll(".toast").forEach((t) => t.remove());
  const t = el("div", "toast");
  t.textContent = msg;
  document.body.appendChild(t);
  setTimeout(() => t.remove(), ms);
}

async function busy(btn, label, fn) {
  const html = btn.innerHTML;
  btn.disabled = true;
  if (label) btn.textContent = label;
  try {
    return await fn();
  } finally {
    btn.disabled = false;
    btn.innerHTML = html;
  }
}

/** In-app replacement for alert/confirm/prompt (unreliable in Android WebView). */
function sheet({ title, body = "", input = null, actions }) {
  return new Promise((resolve) => {
    const scrim = el("div", "scrim");
    const s = el("div", "sheet");
    s.innerHTML = `
      ${title ? `<h3>${esc(title)}</h3>` : ""}
      ${body ? `<div class="body">${body}</div>` : ""}
      ${input ? `<div class="field"><input class="input" id="sheet-in" type="${input.type || "text"}" placeholder="${esc(input.placeholder || "")}" autocomplete="${input.autocomplete || "off"}"></div>` : ""}
      <div class="acts"></div>`;
    const acts = s.querySelector(".acts");
    const inp = s.querySelector("#sheet-in");
    const done = (v) => {
      scrim.remove();
      resolve(v);
    };
    for (const a of actions) {
      const b = el("button", `btn ${a.kind || ""}`, esc(a.label));
      b.onclick = () => done(input ? (a.value ? inp.value : null) : a.value);
      acts.appendChild(b);
    }
    scrim.onclick = (e) => {
    if (e.target === scrim) done(input ? null : actions.find((a) => !a.value)?.value ?? null);
  };
    if (inp) inp.onkeydown = (e) => {
    if (e.key === "Enter") done(inp.value);
  };
    scrim.appendChild(s);
    document.body.appendChild(scrim);
    inp?.focus();
  });
}
const confirmSheet = (title, body, okLabel = "OK", danger = false) =>
  sheet({ title, body, actions: [{ label: "Cancel", value: false }, { label: okLabel, value: true, kind: danger ? "danger" : "primary" }] });
const passwordSheet = (title, body) =>
  sheet({ title, body, input: { type: "password", placeholder: "Master password", autocomplete: "current-password" }, actions: [{ label: "Cancel", value: false }, { label: "Continue", value: true, kind: "primary" }] });

// Clipboard with auto-clear. Uses the clipboard plugin so we can check the
// clipboard still holds *our* copy before wiping it.
let clipTimer = null;
async function copy(text, what = "Copied") {
  try {
    await invoke("plugin:clipboard-manager|write_text", { text });
  } catch {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      toast("Couldn't copy");
      return;
    }
  }
  const secs = Number(prefs.clipClearSec);
  toast(secs > 0 ? `${what} · clears in ${secs}s` : what);
  clearTimeout(clipTimer);
  if (secs > 0) {
    clipTimer = setTimeout(async () => {
      try {
        const cur = await invoke("plugin:clipboard-manager|read_text");
        if (cur === text) await invoke("plugin:clipboard-manager|write_text", { text: "" });
      } catch {}
    }, secs * 1000);
  }
}

function openUrl(url) {
  if (!url) return;
  const u = /^[a-z]+:\/\//i.test(url) ? url : `https://${url}`;
  if (!/^https?:\/\//i.test(u)) return toast("Only web links can be opened");
  invoke("plugin:opener|open_url", { url: u }).catch((e) => toast(errText(e)));
}

function hostOf(url) {
  try {
    return new URL(/^[a-z]+:\/\//i.test(url) ? url : `https://${url}`).hostname.replace(/^www\./, "");
  } catch {
    return url || "";
  }
}

const PALETTE = ["#3e7bfa", "#7a5af8", "#e5484d", "#f76b15", "#2fa66a", "#0e9fae", "#d6409f", "#8e6a3f", "#5b6b7f", "#c4880a"];
function avatar(name, cls = "") {
  const n = (name || "?").trim();
  let h = 0;
  for (const c of n.toLowerCase()) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return `<div class="avatar ${cls}" style="background:${PALETTE[h % PALETTE.length]}">${esc((n[0] || "?").toUpperCase())}</div>`;
}

function strength(pw) {
  if (!pw) return { score: 0, label: "Empty", color: "var(--text-3)" };
  let pool = 0;
  if (/[a-z]/.test(pw)) pool += 26;
  if (/[A-Z]/.test(pw)) pool += 26;
  if (/\d/.test(pw)) pool += 10;
  if (/[^A-Za-z0-9]/.test(pw)) pool += 33;
  let bits = pw.length * Math.log2(Math.max(pool, 2));
  if (/^(.)\1+$/.test(pw) || /^(?:123|abc|qwe|pass|azerty|motdepasse)/i.test(pw)) bits = Math.min(bits, 20);
  if (/^[a-z]+\d{0,4}$/i.test(pw)) bits *= 0.6;
  const score = bits < 36 ? 1 : bits < 60 ? 2 : bits < 80 ? 3 : 4;
  return [null,
    { score, label: "Weak", color: "var(--danger)" },
    { score, label: "Fair", color: "var(--warn)" },
    { score, label: "Good", color: "var(--ok)" },
    { score, label: "Strong", color: "var(--ok)" }][score];
}
function reuseCounts() {
  const m = new Map();
  for (const a of state.accounts) if (a.password) m.set(a.password, (m.get(a.password) || 0) + 1);
  return m;
}
const needsAttention = (a, reuse) => a.password && (strength(a.password).score <= 1 || (reuse.get(a.password) || 0) > 1);

function ago(ms) {
  if (!ms) return "never";
  const s = Math.round((Date.now() - ms) / 1000);
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  return new Date(ms).toLocaleDateString();
}

function generatePassword(o = prefs) {
  let chars = "";
  if (o.genLower) chars += "abcdefghijkmnopqrstuvwxyz";
  if (o.genUpper) chars += "ABCDEFGHJKLMNPQRSTUVWXYZ";
  if (o.genDigits) chars += "23456789";
  if (o.genSymbols) chars += "!@#$%^&*()-_=+?";
  if (!chars) chars = "abcdefghijkmnopqrstuvwxyz";
  const n = Number(o.genLength) || 20;
  const out = [];
  const buf = new Uint32Array(1);
  const limit = Math.floor(0x100000000 / chars.length) * chars.length; // no modulo bias
  while (out.length < n) {
    crypto.getRandomValues(buf);
    if (buf[0] < limit) out.push(chars[buf[0] % chars.length]);
  }
  return out.join("");
}

// ---------------------------------------------------------------- boot

async function refreshStatus() {
  state.status = await invoke("app_status");
  return state.status;
}
async function refreshBio() {
  state.bio = await invoke("biometric_status").catch(() => ({ available: false, has_saved_secret: false, kind: "none" }));
  return state.bio;
}

async function boot() {
  const s = await refreshStatus();
  document.documentElement.dataset.platform = s.platform;
  await refreshBio();
  setState({ screen: !s.vault_exists ? "setup" : !s.device_linked ? "link" : "unlock" });
}

function enterMain(accounts) {
  unlockUsePassword = false;
  state.tab = "passwords";
  state.selectedId = null;
  state.editing = false;
  state.query = "";
  history.replaceState({ v: "root" }, "");
  setState({ screen: "main", accounts, error: "" });
  startSyncLoop();
  resetIdle();
}

async function lockNow(msg) {
  await invoke("lock").catch(() => {});
  stopSyncLoop();
  unlockUsePassword = false;
  setState({ screen: "unlock", accounts: [], selectedId: null, editing: false, importPreview: null, error: "" });
  if (msg) toast(msg);
}

// ---------------------------------------------------------------- auth screens

function authShell(iconName, title, lead, bodyHtml) {
  app.innerHTML = "";
  const w = el("div", "auth");
  w.innerHTML = `
    <div class="top">
      <div class="logo">${icon(iconName, "lg")}</div>
      <h1>${esc(title)}</h1>
      ${lead ? `<div class="lead">${lead}</div>` : ""}
    </div>
    <div class="bottom">${bodyHtml}
      ${state.error ? `<div class="error">${esc(state.error)}</div>` : ""}
    </div>`;
  app.appendChild(w);
  return w;
}

function renderSetup() {
  const w = authShell("shield", "Welcome to PassVault", "Your passwords, encrypted on your own devices. Pick a strong master password — it can't be recovered.", `
    <div class="field"><label>Master password</label><input class="input" type="password" id="pw1" autocomplete="new-password"></div>
    <div class="strength"><div id="pw-meter" style="width:0"></div></div>
    <div class="field" style="margin-top:14px"><label>Confirm password</label><input class="input" type="password" id="pw2" autocomplete="new-password"></div>
    ${isDesktop() ? `<div class="hint" style="padding:0 4px 10px">Vault file: <span style="word-break:break-all">${esc(state.status.vault_path)}</span> · <button class="link" id="choose-folder">Change</button></div>` : ""}
    <div class="actions"><button class="btn primary block" id="continue">Continue</button></div>
    <div class="hint center" style="margin-top:14px">Already use PassVault on another device?</div>
    <div class="alt">
      <button class="link" id="import-file">Import vault file</button>
      ${isAndroid() ? `<button class="link" id="scan-vault">Scan vault QR</button>` : ""}
    </div>
    <div id="autodetect"></div>`);

  const pw1 = w.querySelector("#pw1");
  pw1.oninput = () => {
    const s = strength(pw1.value);
    const m = w.querySelector("#pw-meter");
    m.style.width = `${s.score * 25}%`;
    m.style.background = s.color;
  };
  w.querySelector("#choose-folder")?.addEventListener("click", async () => {
    try {
      const picked = await invoke("choose_vault_folder");
      if (picked) {
        await refreshStatus();
        setState({ error: "" });
      }
    } catch (e) {
      setState({ error: errText(e) });
    }
  });
  const afterImport = async (st) => {
    if (!st) return;
    state.status = st;
    await refreshBio();
    setState({ screen: st.device_linked ? "unlock" : "link", error: "" });
    toast("Vault imported");
  };
  w.querySelector("#import-file").onclick = async () => {
    try {
      afterImport(await invoke("import_vault"));
    } catch (e) {
      setState({ error: errText(e) });
    }
  };
  w.querySelector("#scan-vault")?.addEventListener("click", async () => {
    try {
      const content = await scanQrRaw("Point the camera at the vault QR code");
      afterImport(await invoke("import_vault_qr", { content }));
    } catch (e) {
      setState({ error: errText(e) });
    }
  });
  w.querySelector("#continue").onclick = async () => {
    const a = pw1.value;
    const b = w.querySelector("#pw2").value;
    if (a.length < 8) return setState({ error: "Use at least 8 characters (longer is much better)." });
    if (a !== b) return setState({ error: "The passwords don't match." });
    try {
      const resp = await invoke("start_setup", { password: a });
      setState({ screen: "setup-confirm", setupPending: resp, error: "" });
    } catch (e) {
      setState({ error: errText(e) });
    }
  };
  if (isAndroid()) initAutoDetect(w.querySelector("#autodetect"), afterImport);
}

function renderSetupConfirm() {
  const p = state.setupPending;
  const w = authShell("phone", "Add to your authenticator", "Scan this in Google Authenticator (or any TOTP app), then type the 6-digit code.", `
    <div class="qr">${p.qr_svg}</div>
    <div class="secret">${esc(formatSecret(p.secret_b32))}</div>
    <div class="hint center" style="margin-bottom:14px"><b>Save this key somewhere safe.</b> You need it to link your other devices.</div>
    <div class="field"><input class="input code" id="code" inputmode="numeric" maxlength="6" autocomplete="one-time-code" placeholder="000000"></div>
    <div class="actions">
      <button class="btn primary block" id="confirm">Create vault</button>
      <button class="btn ghost block" id="back">Back</button>
    </div>`);
  const code = w.querySelector("#code");
  code.focus();
  code.onkeydown = (e) => {
    if (e.key === "Enter") w.querySelector("#confirm").click();
  };
  w.querySelector("#back").onclick = () => setState({ screen: "setup", setupPending: null, error: "" });
  w.querySelector("#confirm").onclick = async (ev) => {
    try {
      const accounts = await busy(ev.currentTarget, "Creating…", () => invoke("confirm_setup", { code: code.value.trim() }));
      await refreshStatus();
      state.setupPending = null;
      enterMain(accounts);
    } catch (e) {
      setState({ error: errText(e) });
    }
  };
}

function renderLink() {
  const w = authShell("phone", "Link this device", "A vault is here, but this device isn't linked yet. Enter your master password and the authenticator key you saved at setup.", `
    <div class="field"><label>Master password</label><input class="input" type="password" id="pw" autocomplete="current-password"></div>
    <div class="field"><label>Authenticator key</label>
      <div class="input-row"><input class="input mono" id="secret" placeholder="ABCD EFGH …" autocomplete="off">
      ${isAndroid() ? `<div class="trail"><button class="icon-btn accent" id="scan-key" title="Scan">${icon("qr")}</button></div>` : ""}</div></div>
    <div class="field"><label>Current 6-digit code</label><input class="input code" id="code" inputmode="numeric" maxlength="6" placeholder="000000"></div>
    <div class="actions"><button class="btn primary block" id="link">Link device</button></div>
    <div class="alt">
      <button class="link" id="import-file">Use a different vault file</button>
      ${isAndroid() ? `<button class="link" id="scan-vault">Scan vault QR</button>` : ""}
    </div>`);
  w.querySelector("#scan-key")?.addEventListener("click", async () => {
    try {
      const secret = extractSecretFromScan(await scanQrRaw("Scan the PassVault key QR"));
      if (secret) {
        w.querySelector("#secret").value = secret;
        toast("Key scanned");
      } else toast("That QR code isn't a PassVault key");
    } catch (e) {
      toast(errText(e));
    }
  });
  const imported = async (st) => {
    if (!st) return;
    state.status = st;
    setState({ error: "" });
    toast("Vault imported");
  };
  w.querySelector("#import-file").onclick = async () => {
    try {
      imported(await invoke("import_vault"));
    } catch (e) {
      setState({ error: errText(e) });
    }
  };
  w.querySelector("#scan-vault")?.addEventListener("click", async () => {
    try {
      imported(await invoke("import_vault_qr", { content: await scanQrRaw("Point the camera at the vault QR code") }));
    } catch (e) {
      setState({ error: errText(e) });
    }
  });
  w.querySelector("#link").onclick = async (ev) => {
    const password = w.querySelector("#pw").value;
    const secretB32 = w.querySelector("#secret").value.replace(/\s+/g, "");
    const code = w.querySelector("#code").value.trim();
    try {
      const accounts = await busy(ev.currentTarget, "Linking…", () => invoke("link_device", { password, secretB32, code }));
      await refreshStatus();
      enterMain(accounts);
    } catch (e) {
      setState({ error: errText(e) });
    }
  };
}

let unlockUsePassword = false;
let bioAttemptToken = 0;
let bioInFlight = false;
let unlistenProgress = null;

function renderUnlock() {
  const b = state.bio;
  if (b?.available && b.has_saved_secret && !unlockUsePassword) return renderBioUnlock();
  renderPasswordUnlock();
}

function renderBioUnlock() {
  const face = state.bio.kind === "face";
  const w = authShell("lock", "PassVault", "", `
    <div class="bio-ring busy" id="ring">${icon(bioIcon())}</div>
    <div class="hint center" id="bio-msg" style="margin:18px 0 10px;font-size:15px;white-space:pre-line">${face ? "Look at the camera…" : "Touch the fingerprint sensor"}</div>
    <div class="actions">
      <button class="btn primary block" id="retry" style="display:none">${icon(bioIcon(), "sm")} Try again</button>
      <button class="btn ghost block" id="use-pw">Use master password</button>
    </div>`);
  const ring = w.querySelector("#ring");
  const msg = w.querySelector("#bio-msg");
  const retry = w.querySelector("#retry");
  const token = ++bioAttemptToken;
  unlistenProgress?.();
  window.__TAURI__.event
    .listen("unlock-progress", (e) => {
      if (token === bioAttemptToken && state.screen === "unlock") {
        msg.textContent = e.payload;
        ring.className = "bio-ring busy";
      }
    })
    .then((un) => (unlistenProgress = un))
    .catch(() => {});
  const attempt = async () => {
    ring.className = "bio-ring busy";
    retry.style.display = "none";
    msg.textContent = face ? "Look at the camera…" : "Touch the fingerprint sensor";
    bioInFlight = true;
    try {
      const accounts = await invoke("biometric_unlock");
      bioInFlight = false;
      if (token !== bioAttemptToken || state.screen !== "unlock") return;
      enterMain(accounts);
    } catch (e) {
      bioInFlight = false;
      if (token !== bioAttemptToken) return;
      ring.className = "bio-ring fail";
      msg.textContent = errText(e);
      retry.style.display = "";
    }
  };
  retry.onclick = attempt;
  w.querySelector("#use-pw").onclick = () => {
    unlockUsePassword = true;
    bioAttemptToken++;
    render();
  };
  attempt();
}

function renderPasswordUnlock() {
  const b = state.bio;
  const w = authShell("lock", "PassVault", "Enter your master password and the code from your authenticator app.", `
    <div class="field"><label>Master password</label><input class="input" type="password" id="pw" autocomplete="current-password"></div>
    <div class="field"><label>6-digit code</label><input class="input code" id="code" inputmode="numeric" maxlength="6" placeholder="000000" autocomplete="one-time-code"></div>
    <div class="actions"><button class="btn primary block" id="unlock">Unlock</button></div>
    ${b?.available && b.has_saved_secret ? `<div class="alt"><button class="link" id="use-bio">${icon(bioIcon(), "sm")} Use ${bioName().toLowerCase()}</button></div>` : ""}`);
  const pw = w.querySelector("#pw");
  const code = w.querySelector("#code");
  pw.focus();
  pw.onkeydown = (e) => {
    if (e.key === "Enter") code.focus();
  };
  code.oninput = () => {
    if (code.value.length === 6 && pw.value) w.querySelector("#unlock").click();
  };
  code.onkeydown = (e) => {
    if (e.key === "Enter") w.querySelector("#unlock").click();
  };
  w.querySelector("#use-bio")?.addEventListener("click", () => {
    unlockUsePassword = false;
    render();
  });
  w.querySelector("#unlock").onclick = async (ev) => {
    const password = pw.value;
    let accounts;
    try {
      accounts = await busy(ev.currentTarget, "Unlocking…", () => invoke("unlock", { password, code: code.value.trim() }));
    } catch (e) {
      return setState({ error: errText(e) });
    }
    enterMain(accounts);
    await refreshBio();
    if (state.bio.available && !state.bio.has_saved_secret && !localStorage.getItem("passvault.bioOffered")) {
      localStorage.setItem("passvault.bioOffered", "1");
      const face = state.bio.kind === "face";
      const yes = await confirmSheet(
        `Turn on ${bioName().toLowerCase()}?`,
        face ? "Unlock with your face (Howdy) instead of typing your password and code. You'll be asked to look at the camera once now." : "Unlock with your fingerprint instead of typing your password and code.",
        "Turn on"
      );
      if (yes) enableBio(password);
    }
  };
}

async function enableBio(password) {
  if (state.bio.kind === "face") toast("Look at the camera…", 4000);
  try {
    await invoke("enable_biometric_unlock", { password });
    await refreshBio();
    toast(`${bioName()} is on`);
  } catch (e) {
    toast(errText(e), 3500);
  }
  if (state.screen === "main" && state.tab === "settings") render();
}

// ---------------------------------------------------------------- main shell

function navItems() {
  return [
    ["passwords", "key", "Passwords"],
    ["generator", "wand", "Generator"],
    ["settings", "settings", "Settings"],
  ];
}

function goTab(tab) {
  state.selectedId = null;
  state.editing = false;
  history.replaceState({ v: "root" }, "");
  setState({ tab });
}

function renderMain() {
  const scrollers = {};
  app.querySelectorAll("[data-scroll]").forEach((p) => (scrollers[p.dataset.scroll] = p.scrollTop));
  app.innerHTML = "";
  const shell = el("div", "shell");

  const rail = el("nav", "rail");
  rail.innerHTML = `<div class="logo-sm">${icon("shield")}</div>` +
    navItems().map(([t, i, l]) => `<button data-tab="${t}" class="${state.tab === t || (t === "settings" && state.tab === "import") ? "on" : ""}">${icon(i)}${l}</button>`).join("") +
    `<div class="spacer"></div><button id="rail-lock">${icon("lock")}Lock</button>`;
  rail.querySelectorAll("[data-tab]").forEach((b) => (b.onclick = () => goTab(b.dataset.tab)));
  rail.querySelector("#rail-lock").onclick = () => lockNow();
  shell.appendChild(rail);

  const phone = !wide();
  if (state.tab === "passwords") {
    const showDetail = !!state.selectedId;
    if (!phone || !showDetail) shell.appendChild(paneList());
    if (!phone || showDetail) shell.appendChild(paneDetail());
  } else {
    const full = el("div", "pane-full");
    if (state.tab === "generator") full.appendChild(pageGenerator());
    if (state.tab === "settings") full.appendChild(pageSettings());
    if (state.tab === "import") full.appendChild(pageImport());
    shell.appendChild(full);
  }

  if (phone && !(state.tab === "passwords" && state.selectedId)) {
    const nav = el("nav", "bottom-nav");
    nav.innerHTML = navItems().map(([t, i, l]) => `<button data-tab="${t}" class="${state.tab === t || (t === "settings" && state.tab === "import") ? "on" : ""}">${icon(i)}${l}</button>`).join("");
    nav.querySelectorAll("[data-tab]").forEach((b) => (b.onclick = () => goTab(b.dataset.tab)));
    shell.appendChild(nav);
  }
  app.appendChild(shell);
  app.querySelectorAll("[data-scroll]").forEach((p) => {
    if (scrollers[p.dataset.scroll] != null) p.scrollTop = scrollers[p.dataset.scroll];
    p.dispatchEvent(new Event("scroll"));
  });
}

/** Page with a One UI–style big title that collapses into the toolbar. */
function page(key, title, metaHtml, toolbarHtml, bodyEl, { back = false, detail = false } = {}) {
  const p = el("div", `page${detail ? " detail-page" : ""}`);
  p.dataset.scroll = key;
  p.innerHTML = `
    <div class="hero collapsible"><h1>${esc(title)}</h1>${metaHtml ? `<div class="meta">${metaHtml}</div>` : ""}</div>
    <div class="toolbar">
      ${back ? `<button class="icon-btn back-btn" id="back">${icon("back")}</button>` : ""}
      <div class="t">${esc(title)}</div>${toolbarHtml || ""}
    </div>`;
  const content = el("div", "content");
  content.appendChild(bodyEl);
  p.appendChild(content);
  const hero = p.querySelector(".hero");
  const tb = p.querySelector(".toolbar");
  p.addEventListener("scroll", () => {
    const h = hero.offsetHeight || 1;
    tb.style.setProperty("--collapse", Math.min(1, Math.max(0, (p.scrollTop - h * 0.45) / (h * 0.4))).toFixed(2));
  });
  p.querySelector("#back")?.addEventListener("click", () => history.back());
  return p;
}

function syncMeta() {
  const n = state.accounts.length;
  const base = `${n} account${n === 1 ? "" : "s"}`;
  if (!state.status.sync_url) return base;
  const cls = sync.state === "ok" ? "ok" : sync.state === "syncing" ? "busy" : sync.state === "error" ? "err" : "";
  const txt = sync.state === "syncing" ? "Syncing…" : sync.state === "error" ? "Not synced" : `Synced ${ago(state.status.last_sync_ms)}`;
  return `${base} · <span class="status-dot ${cls}"></span> <span id="sync-label">${txt}</span>`;
}

// ---------------------------------------------------------------- list

function paneList() {
  const pane = el("div", "pane-list");
  const body = el("div");
  body.innerHTML = `
    <div class="search"><span>${icon("search", "sm")}</span><input id="q" placeholder="Search" value="${esc(state.query)}" autocomplete="off" spellcheck="false">${state.query ? `<button class="icon-btn" id="clear-q" style="width:34px;height:34px">${icon("x", "sm")}</button>` : ""}</div>
    <div class="chips" style="margin:12px 0 4px">
      <button class="chip ${state.filter === "all" ? "on" : ""}" data-f="all">All</button>
      <button class="chip ${state.filter === "fav" ? "on" : ""}" data-f="fav">${icon("star", "sm")} Favorites</button>
      <button class="chip ${state.filter === "attention" ? "on" : ""}" data-f="attention">Weak or reused</button>
    </div>
    <div id="list"></div>`;
  const tools = `
    ${state.status.sync_url ? `<button class="icon-btn" id="sync-btn" title="Sync now">${icon("sync")}</button>` : ""}
    <button class="icon-btn only-desktop-add" id="add-top" title="Add">${icon("plus")}</button>
    <button class="icon-btn only-phone" id="lock-top" title="Lock">${icon("lock")}</button>`;
  const p = page("list", "PassVault", syncMeta(), tools, body);
  pane.appendChild(p);

  const q = body.querySelector("#q");
  q.oninput = () => {
    state.query = q.value;
    drawList(body.querySelector("#list"));
  };
  body.querySelector("#clear-q")?.addEventListener("click", () => setState({ query: "" }));
  body.querySelectorAll("[data-f]").forEach((c) => (c.onclick = () => setState({ filter: c.dataset.f })));
  p.querySelector("#add-top").onclick = openNew;
  p.querySelector("#lock-top").onclick = () => lockNow();
  p.querySelector("#sync-btn")?.addEventListener("click", () => runSync(true));
  drawList(body.querySelector("#list"));

  if (!wide()) {
    const fab = el("button", "fab", `${icon("plus")} Add`);
    fab.onclick = openNew;
    pane.appendChild(fab);
  }
  return pane;
}

function drawList(container) {
  const reuse = reuseCounts();
  const q = state.query.trim().toLowerCase();
  let list = state.accounts.filter((a) => {
    if (state.filter === "fav" && !a.favorite) return false;
    if (state.filter === "attention" && !needsAttention(a, reuse)) return false;
    if (!q) return true;
    return [a.name, a.username, a.url, a.notes].some((f) => (f || "").toLowerCase().includes(q));
  });
  list.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
  container.innerHTML = "";

  if (!list.length) {
    const empty = el("div", "empty");
    if (!state.accounts.length) {
      empty.innerHTML = `<div class="big">${icon("key", "lg")}</div><b>No passwords yet</b><div>Add one, or bring them in from your browser.</div>`;
      const b = el("button", "btn primary", `${icon("download", "sm")} Import passwords`);
      b.onclick = () => goTab("settings");
      empty.appendChild(b);
    } else {
      empty.innerHTML = `<div class="big">${icon("search", "lg")}</div><div>${state.filter === "attention" ? "Nothing weak or reused. Nice." : "No matches"}</div>`;
    }
    container.appendChild(empty);
    return;
  }

  const groups = [];
  if (state.filter === "all" && !q) {
    const favs = list.filter((a) => a.favorite);
    if (favs.length) groups.push(["Favorites", favs]);
  }
  if (q || state.filter !== "all") groups.push([q ? `${list.length} result${list.length === 1 ? "" : "s"}` : "", list]);
  else {
    const by = new Map();
    for (const a of list) {
      const L = /[a-z]/i.test(a.name[0] || "") ? a.name[0].toUpperCase() : "#";
      if (!by.has(L)) by.set(L, []);
      by.get(L).push(a);
    }
    for (const [L, arr] of by) groups.push([L, arr]);
  }

  for (const [label, arr] of groups) {
    if (label) container.appendChild(el("div", "letter", esc(label)));
    const card = el("div", "card");
    for (const a of arr) {
      const row = el("button", `row${a.id === state.selectedId ? " active" : ""}`);
      const warn = needsAttention(a, reuse);
      row.innerHTML = `${avatar(a.name)}
        <div class="grow"><div class="title">${esc(a.name)}</div><div class="sub">${esc(a.username || hostOf(a.url) || "—")}</div></div>
        ${warn ? `<span class="badge warn" title="Weak or reused">${icon("alert", "sm")}</span>` : ""}
        ${a.favorite ? `<span style="color:var(--warn)">${icon("star", "sm")}</span>` : ""}`;
      row.onclick = () => openAccount(a.id);
      card.appendChild(row);
    }
    container.appendChild(card);
  }
}

function openAccount(id) {
  if (!wide()) history.pushState({ v: "detail" }, "");
  setState({ selectedId: id, editing: false });
}
function openNew() {
  if (!wide()) history.pushState({ v: "detail" }, "");
  setState({ tab: "passwords", selectedId: "__new__", editing: true });
}
function closeDetail() {
  setState({ selectedId: null, editing: false });
}

// Android back button / browser back: step back out of detail views.
window.addEventListener("popstate", () => {
  if (state.screen !== "main") return;
  if (state.tab === "import") return setState({ tab: "settings", importPreview: null });
  if (state.selectedId) {
    if (state.editing && state.selectedId !== "__new__") return setState({ editing: false });
    closeDetail();
  }
});

// ---------------------------------------------------------------- detail

function paneDetail() {
  const pane = el("div", "pane-detail");
  if (!state.selectedId) {
    pane.innerHTML = `<div class="empty" style="height:100%"><div class="big">${icon("shield", "lg")}</div><div>Select an account</div></div>`;
    return pane;
  }
  if (state.selectedId === "__new__") {
    pane.appendChild(accountForm(null));
    return pane;
  }
  const acc = state.accounts.find((a) => a.id === state.selectedId);
  if (!acc) {
    state.selectedId = null;
    pane.innerHTML = `<div class="empty" style="height:100%"><div>This account no longer exists.</div></div>`;
    return pane;
  }
  pane.appendChild(state.editing ? accountForm(acc) : accountDetail(acc));
  return pane;
}

function accountDetail(acc) {
  const reuse = reuseCounts().get(acc.password) || 0;
  const st = strength(acc.password);
  const body = el("div", "detail-wrap");
  body.innerHTML = `
    <div class="detail-hero">${avatar(acc.name, "xl")}<h2>${esc(acc.name)}</h2>
      ${acc.url ? `<div class="sub">${esc(hostOf(acc.url))}</div>` : ""}
      <div style="display:flex;gap:6px;flex-wrap:wrap;justify-content:center">
        ${acc.password ? `<span class="badge ${st.score <= 1 ? "bad" : st.score === 2 ? "warn" : "good"}">${st.label} password</span>` : ""}
        ${reuse > 1 ? `<span class="badge warn">${icon("alert", "sm")} Used in ${reuse - 1} other account${reuse > 2 ? "s" : ""}</span>` : ""}
      </div>
    </div>
    <div class="card">
      <div class="kv"><div class="grow"><div class="k">Username</div><div class="v">${esc(acc.username) || "<span class='hint'>—</span>"}</div></div>
        ${acc.username ? `<button class="icon-btn accent" data-copy="user" title="Copy">${icon("copy")}</button>` : ""}</div>
      <div class="kv"><div class="grow"><div class="k">Password</div><div class="v mono" id="pw">••••••••••••</div></div>
        <button class="icon-btn" id="toggle" title="Show">${icon("eye")}</button>
        <button class="icon-btn accent" data-copy="pw" title="Copy">${icon("copy")}</button></div>
      ${acc.url ? `<div class="kv"><div class="grow"><div class="k">Website</div><div class="v">${esc(acc.url)}</div></div>
        <button class="icon-btn accent" id="open" title="Open">${icon("open")}</button></div>` : ""}
    </div>
    ${acc.notes ? `<div class="section-title">Notes</div><div class="card"><div class="kv"><div class="grow"><div class="v notes">${esc(acc.notes)}</div></div></div></div>` : ""}
    <div class="hint center" style="margin-top:18px">Updated ${esc(new Date(acc.updated_at).toLocaleString())}</div>
    <div class="bottom-actions">
      <button class="btn" id="edit">${icon("edit", "sm")} Edit</button>
      <button class="btn primary" id="copy-pw">${icon("copy", "sm")} Copy password</button>
    </div>`;
  const tools = `
    <button class="icon-btn ${acc.favorite ? "on" : ""}" id="fav" title="Favorite">${icon("star")}</button>
    <button class="icon-btn" id="del" title="Delete" style="color:var(--danger)">${icon("trash")}</button>`;
  const p = page("detail", acc.name, "", tools, body, { back: true, detail: true });
  p.querySelector(".hero").style.display = "none";
  p.querySelector(".toolbar").classList.add("always");

  body.querySelector('[data-copy="user"]')?.addEventListener("click", () => copy(acc.username, "Username copied"));
  body.querySelector('[data-copy="pw"]').onclick = () => copy(acc.password, "Password copied");
  body.querySelector("#copy-pw").onclick = () => copy(acc.password, "Password copied");
  body.querySelector("#open")?.addEventListener("click", () => openUrl(acc.url));
  const pwEl = body.querySelector("#pw");
  body.querySelector("#toggle").onclick = (e) => {
    const shown = pwEl.dataset.shown === "1";
    pwEl.dataset.shown = shown ? "0" : "1";
    pwEl.textContent = shown ? "••••••••••••" : acc.password || "—";
    e.currentTarget.innerHTML = icon(shown ? "eye" : "eyeOff");
  };
  body.querySelector("#edit").onclick = () => {
    if (!wide()) history.pushState({ v: "edit" }, "");
    setState({ editing: true });
  };
  p.querySelector("#fav").onclick = async () => {
    try {
      const accounts = await invoke("set_favorite", { id: acc.id, favorite: !acc.favorite });
      setState({ accounts });
      scheduleSync();
    } catch (e) {
      handleErr(e);
    }
  };
  p.querySelector("#del").onclick = async () => {
    if (!(await confirmSheet(`Delete ${acc.name}?`, "It will be removed from all your synced devices.", "Delete", true))) return;
    try {
      const accounts = await invoke("delete_account", { id: acc.id });
      state.accounts = accounts;
      if (!wide()) history.back();
      else closeDetail();
      toast("Deleted");
      scheduleSync();
    } catch (e) {
      handleErr(e);
    }
  };
  return p;
}

function accountForm(acc) {
  const isNew = !acc;
  const body = el("div", "detail-wrap");
  body.innerHTML = `
    <div class="card pad" style="margin-top:8px">
      <div class="field"><label>Name</label><input class="input" id="f-name" value="${esc(acc?.name ?? "")}" placeholder="e.g. Instagram"></div>
      <div class="field"><label>Username or email</label><input class="input" id="f-user" value="${esc(acc?.username ?? "")}" autocomplete="off" autocapitalize="off" spellcheck="false"></div>
      <div class="field"><label>Password</label>
        <div class="input-row"><input class="input mono" id="f-pw" value="${esc(acc?.password ?? "")}" autocomplete="off" autocapitalize="off" spellcheck="false" type="${isNew ? "text" : "password"}">
          <div class="trail"><button class="icon-btn" id="f-eye" title="Show">${icon(isNew ? "eyeOff" : "eye")}</button><button class="icon-btn accent" id="f-gen" title="Generate">${icon("wand")}</button></div></div>
        <div class="strength"><div id="f-meter"></div></div><div class="hint" id="f-st" style="padding-left:4px"></div>
      </div>
      <div class="field"><label>Website</label><input class="input" id="f-url" value="${esc(acc?.url ?? "")}" placeholder="instagram.com" autocomplete="off" autocapitalize="off" spellcheck="false" inputmode="url"></div>
      <div class="field" style="margin-bottom:0"><label>Notes</label><textarea class="input" id="f-notes" rows="3">${esc(acc?.notes ?? "")}</textarea></div>
    </div>
    <div class="bottom-actions">
      <button class="btn" id="cancel">Cancel</button>
      <button class="btn primary" id="save">${icon("check", "sm")} Save</button>
    </div>`;
  const p = page("form", isNew ? "New account" : "Edit account", "", "", body, { back: true, detail: true });
  p.querySelector(".hero").style.display = "none";
  p.querySelector(".toolbar").classList.add("always");

  const pw = body.querySelector("#f-pw");
  const meter = () => {
    const s = strength(pw.value);
    const m = body.querySelector("#f-meter");
    m.style.width = `${s.score * 25}%`;
    m.style.background = s.color;
    const reused = pw.value && state.accounts.some((a) => a.password === pw.value && a.id !== acc?.id);
    body.querySelector("#f-st").textContent = pw.value ? `${s.label}${reused ? " · already used by another account" : ""}` : "";
  };
  pw.oninput = meter;
  meter();
  body.querySelector("#f-eye").onclick = (e) => {
    pw.type = pw.type === "password" ? "text" : "password";
    e.currentTarget.innerHTML = icon(pw.type === "password" ? "eye" : "eyeOff");
  };
  body.querySelector("#f-gen").onclick = () => {
    pw.value = generatePassword();
    pw.type = "text";
    body.querySelector("#f-eye").innerHTML = icon("eyeOff");
    meter();
  };
  const nameEl = body.querySelector("#f-name");
  const urlEl = body.querySelector("#f-url");
  urlEl.onblur = () => {
    if (!nameEl.value.trim() && urlEl.value.trim()) {
      const h = hostOf(urlEl.value).split(".");
      const n = h.length > 1 ? h[h.length - 2] : h[0];
      nameEl.value = n ? n[0].toUpperCase() + n.slice(1) : "";
    }
  };
  const leave = () => (wide() ? setState(isNew ? { selectedId: null, editing: false } : { editing: false }) : history.back());
  body.querySelector("#cancel").onclick = leave;
  body.querySelector("#save").onclick = async (ev) => {
    const payload = {
      name: nameEl.value.trim(),
      username: body.querySelector("#f-user").value.trim(),
      password: pw.value,
      url: urlEl.value.trim(),
      notes: body.querySelector("#f-notes").value,
    };
    if (!payload.name) {
      nameEl.focus();
      return toast("Give it a name");
    }
    try {
      await busy(ev.currentTarget, "Saving…", async () => {
        if (isNew) {
          const r = await invoke("add_account", payload);
          state.accounts = r.accounts;
          if (wide()) setState({ selectedId: r.id, editing: false });
          else {
            // Replace the "new" history entry with the new account's page.
            history.replaceState({ v: "detail" }, "");
            setState({ selectedId: r.id, editing: false });
          }
        } else {
          state.accounts = await invoke("update_account", { id: acc.id, ...payload });
          if (wide()) setState({ editing: false });
          else history.back();
        }
      });
      toast("Saved");
      scheduleSync();
    } catch (e) {
      handleErr(e);
    }
  };
  if (isNew) setTimeout(() => nameEl.focus(), 50);
  return p;
}

function handleErr(e) {
  const m = errText(e);
  if (/locked/i.test(m)) return lockNow("Vault locked");
  toast(m, 3500);
}

// ---------------------------------------------------------------- generator

function pageGenerator() {
  const body = el("div");
  const render = () => {
    const pw = generatePassword();
    const s = strength(pw);
    body.innerHTML = `
      <div class="card">
        <div class="gen-out" id="out"><div>${[...pw].map((c) => (/\d/.test(c) ? `<span class="d">${esc(c)}</span>` : /[^A-Za-z]/.test(c) ? `<span class="s">${esc(c)}</span>` : esc(c))).join("")}</div></div>
        <div style="display:flex;justify-content:center;padding-bottom:18px"><span class="badge ${s.score <= 1 ? "bad" : s.score === 2 ? "warn" : "good"}">${s.label}</span></div>
      </div>
      <div class="card pad">
        <div style="display:flex;justify-content:space-between;font-weight:600"><span>Length</span><span id="len-v">${prefs.genLength}</span></div>
        <input type="range" id="len" min="8" max="64" value="${prefs.genLength}" style="margin:12px 0 4px">
      </div>
      <div class="card">
        ${[["genUpper", "Uppercase (A-Z)"], ["genLower", "Lowercase (a-z)"], ["genDigits", "Numbers (0-9)"], ["genSymbols", "Symbols (!@#…)"]]
          .map(([k, l]) => `<label class="row plain" style="cursor:pointer"><div class="grow">${l}</div><span class="switch"><input type="checkbox" data-k="${k}" ${prefs[k] ? "checked" : ""}><span></span></span></label>`).join("")}
      </div>
      <div class="bottom-actions gen-actions">
        <button class="btn" id="regen">${icon("sync", "sm")} New</button>
        <button class="btn primary" id="copy">${icon("copy", "sm")} Copy</button>
      </div>`;
    body.querySelector("#len").oninput = (e) => {
      body.querySelector("#len-v").textContent = e.target.value;
    };
    body.querySelector("#len").onchange = (e) => {
      prefs.genLength = Number(e.target.value);
      savePrefs();
      render();
    };
    body.querySelectorAll("[data-k]").forEach((c) => (c.onchange = () => {
      prefs[c.dataset.k] = c.checked;
      if (!prefs.genUpper && !prefs.genLower && !prefs.genDigits && !prefs.genSymbols) prefs.genLower = true;
      savePrefs();
      render();
    }));
    body.querySelector("#regen").onclick = render;
    body.querySelector("#copy").onclick = () => copy(pw, "Password copied");
  };
  render();
  return page("gen", "Generator", "Strong, random passwords", "", body);
}

// ---------------------------------------------------------------- settings

function settingRow({ iconName, title, sub = "", right = "", id = "", cls = "" }) {
  return `<div class="row ${cls}" role="button" tabindex="0" ${id ? `id="${id}"` : ""}>
    <div class="avatar" style="background:var(--accent-soft);color:var(--accent)">${icon(iconName, "sm")}</div>
    <div class="grow"><div class="title" style="white-space:normal">${title}</div>${sub ? `<div class="sub" style="white-space:normal">${sub}</div>` : ""}</div>${right}</div>`;
}

function pageSettings() {
  const body = el("div");
  const b = state.bio || {};
  const st = state.status;
  const bioRight = b.available
    ? `<span class="switch"><input type="checkbox" id="bio-sw" ${b.has_saved_secret ? "checked" : ""}><span></span></span>`
    : "";
  const bioSub = b.available
    ? b.kind === "face"
      ? "Unlock with Howdy face recognition. Your password + key are kept in the desktop keyring."
      : "Unlock with your fingerprint. Also used for autofill."
    : esc(b.reason || (isAndroid() ? "No fingerprint enrolled on this phone." : "Install Howdy and run: sudo howdy add"));

  body.innerHTML = `
    <div class="section-title">Import</div>
    <div class="card">
      ${settingRow({ iconName: "download", title: "Import from a CSV file", sub: "Firefox, Chrome, Edge, Brave, Google Password Manager, Bitwarden, 1Password, LastPass, KeePass, Proton Pass, Apple…", id: "imp-csv", right: icon("chev", "sm chev") })}
      <div id="browser-list"></div>
      <div style="padding:4px 18px 14px">
        <details class="help"><summary>How do I export my passwords?</summary><ul>
          <li><b>Firefox (PC):</b> ☰ → Passwords → ⋯ → Export passwords. ${isDesktop() ? "Or use the one-click import above." : "Firefox for Android can't export — export from Firefox on your PC."}</li>
          <li><b>Chrome / Edge / Brave (PC):</b> Settings → Passwords (Google Password Manager) → Settings → Export.</li>
          <li><b>Google Password Manager (phone):</b> Settings → Google → Autofill → Google Password Manager → ⚙ → Export passwords.</li>
          <li><b>Samsung Pass / Samsung Internet:</b> Samsung doesn't offer a CSV export. If you also saved them in Chrome/Google, export from there; otherwise open each one in Samsung Pass and add it here.</li>
          <li><b>Bitwarden, 1Password, LastPass, KeePassXC, Proton Pass:</b> Export → CSV.</li>
          <li>Delete the CSV afterwards — it contains your passwords in plain text.</li>
        </ul></details>
      </div>
    </div>

    ${isAndroid() ? `
    <div class="section-title">Samsung &amp; Android</div>
    <div class="card">
      ${settingRow({ iconName: "key", title: "Autofill service", sub: "Checking…", id: "autofill-row", right: icon("chev", "sm chev") })}
      <div class="hint" style="padding:0 18px 14px">Lets PassVault fill logins in Samsung Internet, Chrome and apps, after a fingerprint check. On Samsung: Settings → General management → Passwords and autofill → Autofill service → PassVault. Add the <b>PassVault</b> tile to Quick Panel for one-tap access.</div>
    </div>` : ""}

    <div class="section-title">Sync</div>
    <div class="card pad">
      <div style="display:flex;align-items:center;gap:8px;margin-bottom:12px;font-weight:600">
        ${icon("cloud", "sm")} <span id="sync-status">${st.sync_url ? (sync.state === "error" ? esc(sync.msg) : `Last synced ${ago(st.last_sync_ms)}`) : "Off"}</span></div>
      <div class="field"><label>Home server</label><input class="input" id="sync-url" value="${esc(st.sync_url || "http://192.168.0.246:8787")}" autocomplete="off" autocapitalize="off" spellcheck="false" inputmode="url"></div>
      <div style="display:flex;gap:10px;flex-wrap:wrap">
        ${st.sync_url
          ? `<button class="btn primary" id="sync-now">${icon("sync", "sm")} Sync now</button><button class="btn" id="sync-save">Save address</button><button class="btn danger" id="sync-off">Turn off</button>`
          : `<button class="btn primary" id="sync-on">${icon("cloud", "sm")} Turn on sync</button>`}
      </div>
      <div class="hint" style="margin-top:12px">Works on your home Wi-Fi (server: leoserv0). The server only stores the encrypted vault — never your password. Devices linked with the same authenticator key find each other automatically; changes merge per account.</div>
    </div>

    <div class="section-title">Security</div>
    <div class="card">
      ${settingRow({ iconName: bioIcon(), title: b.kind === "face" ? "Face unlock" : "Fingerprint unlock", sub: bioSub, id: "bio-row", right: bioRight })}
      ${isDesktop() ? settingRow({ iconName: "lock", title: "Auto-lock", sub: "Lock after this long without activity", right: `<select class="input" id="autolock" style="width:auto;min-height:40px;padding:6px 12px">${[[1, "1 min"], [5, "5 min"], [10, "10 min"], [30, "30 min"], [0, "Never"]].map(([v, l]) => `<option value="${v}" ${Number(prefs.autoLockMin) === v ? "selected" : ""}>${l}</option>`).join("")}</select>` }) : settingRow({ iconName: "lock", title: "Auto-lock", sub: "Locks as soon as you leave the app" })}
      ${settingRow({ iconName: "copy", title: "Clear clipboard", sub: "Wipe copied passwords after", right: `<select class="input" id="clip" style="width:auto;min-height:40px;padding:6px 12px">${[[15, "15 s"], [30, "30 s"], [60, "1 min"], [0, "Never"]].map(([v, l]) => `<option value="${v}" ${Number(prefs.clipClearSec) === v ? "selected" : ""}>${l}</option>`).join("")}</select>` })}
    </div>

    <div class="section-title">Devices &amp; backup</div>
    <div class="card">
      ${settingRow({ iconName: "qr", title: "Show authenticator key", sub: "Needed to link a new device", id: "reveal-key", right: icon("chev", "sm chev") })}
      ${settingRow({ iconName: "download", title: "Export encrypted vault file", sub: "Backup copy; still protected by your master password", id: "export", right: icon("chev", "sm chev") })}
      ${isDesktop() ? settingRow({ iconName: "folder", title: "Show vault in file manager", sub: esc(st.vault_path), id: "reveal-folder", right: icon("chev", "sm chev") }) : ""}
      ${settingRow({ iconName: "qr", title: "Share vault as QR", sub: "Small vaults only", id: "qr-export", right: icon("chev", "sm chev") })}
      ${isAndroid() ? settingRow({ iconName: "qr", title: "Replace vault from QR", sub: "Overwrites this device's vault", id: "qr-scan", right: icon("chev", "sm chev") }) : ""}
      <div id="key-area" style="padding:0 18px"></div>
    </div>

    <div class="card" style="margin-top:22px">${settingRow({ iconName: "lock", title: "Lock now", id: "lock", cls: "" })}</div>
    <div class="hint center" style="margin:18px 0">PassVault 0.2.0</div>`;

  const p = page("settings", "Settings", "", "", body);

  // Import
  body.querySelector("#imp-csv").onclick = async (e) => {
    try {
      const preview = await busy(e.currentTarget, null, () => invoke("import_csv_pick"));
      if (preview) openImport(preview);
    } catch (err) {
      handleErr(err);
    }
  };
  if (isDesktop()) {
    invoke("browser_profiles").then((profiles) => {
      const box = body.querySelector("#browser-list");
      if (!box || !profiles.length) return;
      box.innerHTML = profiles.map((pr, i) => settingRow({ iconName: "globe", title: `Import from ${esc(pr.browser)}`, sub: `Profile: ${esc(pr.profile)}`, id: `br-${i}`, right: icon("chev", "sm chev") })).join("");
      profiles.forEach((pr, i) => {
        box.querySelector(`#br-${i}`).onclick = async (e) => {
          try {
            const preview = await busy(e.currentTarget, null, () => invoke("import_browser", { path: pr.path }));
            openImport(preview);
          } catch (err) {
            toast(errText(err), 5000);
          }
        };
      });
    });
  }

  // Autofill
  if (isAndroid()) {
    const row = body.querySelector("#autofill-row");
    invoke("autofill_status").then((s) => {
      row.querySelector(".sub").textContent = !s.supported ? "Not supported on this Android version" : s.enabled ? "On — PassVault fills your logins" : "Off — tap to choose PassVault";
      if (s.enabled) row.querySelector(".avatar").style.color = "var(--ok)";
    });
    row.onclick = async () => {
      if (!state.bio?.has_saved_secret) toast("Turn on fingerprint unlock too — autofill uses it", 3500);
      invoke("open_autofill_settings").catch((e) => toast(errText(e)));
    };
  }

  // Sync
  const urlIn = body.querySelector("#sync-url");
  const connect = async (btn) => {
    try {
      state.status = await busy(btn, "Connecting…", () => invoke("set_sync_url", { url: urlIn.value }));
      toast("Sync is on");
      await runSync(true);
      render();
    } catch (e) {
      toast(errText(e), 4000);
    }
  };
  body.querySelector("#sync-on")?.addEventListener("click", (e) => connect(e.currentTarget));
  body.querySelector("#sync-save")?.addEventListener("click", (e) => connect(e.currentTarget));
  body.querySelector("#sync-now")?.addEventListener("click", async (e) => {
    await busy(e.currentTarget, "Syncing…", () => runSync(true));
    render();
  });
  body.querySelector("#sync-off")?.addEventListener("click", async () => {
    state.status = await invoke("set_sync_url", { url: "" });
    sync.state = "off";
    toast("Sync turned off on this device");
    render();
  });

  // Security
  body.querySelector("#bio-sw")?.addEventListener("click", async (e) => {
    e.preventDefault();
    if (state.bio.has_saved_secret) {
      await invoke("disable_biometric_unlock");
      await refreshBio();
      toast(`${bioName()} is off`);
      render();
    } else {
      const pw = await passwordSheet(`Turn on ${bioName().toLowerCase()}`, "Enter your master password to confirm.");
      if (pw) enableBio(pw);
    }
  });
  body.querySelector("#autolock")?.addEventListener("change", (e) => {
    prefs.autoLockMin = Number(e.target.value);
    savePrefs();
    resetIdle();
  });
  body.querySelector("#clip").onchange = (e) => {
    prefs.clipClearSec = Number(e.target.value);
    savePrefs();
  };

  // Devices
  const keyArea = body.querySelector("#key-area");
  body.querySelector("#reveal-key").onclick = async () => {
    const pw = await passwordSheet("Show authenticator key", "This key plus your master password can link a new device. Don't share it.");
    if (!pw) return;
    try {
      const r = await invoke("reveal_totp_secret", { password: pw });
      keyArea.innerHTML = `<div style="padding:8px 0 18px"><div class="qr">${r.qr_svg}</div><div class="secret">${esc(formatSecret(r.secret_b32))}</div><div class="hint center">Scan in your authenticator on the new device, then link it.</div></div>`;
    } catch (e) {
      toast(errText(e));
    }
  };
  body.querySelector("#export").onclick = async () => {
    try {
      if (await invoke("export_vault")) toast("Vault exported");
    } catch (e) {
      toast(errText(e));
    }
  };
  body.querySelector("#reveal-folder")?.addEventListener("click", () => invoke("reveal_vault_folder").catch((e) => toast(errText(e))));
  body.querySelector("#qr-export").onclick = async () => {
    try {
      const svg = await invoke("export_vault_qr");
      keyArea.innerHTML = `<div style="padding:8px 0 18px"><div class="qr">${svg}</div><div class="hint center">Scan this from the other device (Settings → Replace vault from QR).</div></div>`;
    } catch (e) {
      toast(errText(e), 4000);
    }
  };
  body.querySelector("#qr-scan")?.addEventListener("click", async () => {
    if (!(await confirmSheet("Replace this vault?", "Everything on this device is replaced by the scanned vault. Accounts only on this device will be lost (a backup file is kept).", "Replace", true))) return;
    try {
      const content = await scanQrRaw("Point the camera at the vault QR code");
      await invoke("import_vault_qr", { content });
      await refreshBio();
      lockNow("Vault replaced — unlock to continue");
    } catch (e) {
      toast(errText(e));
    }
  });
  body.querySelector("#lock").onclick = () => lockNow();
  return p;
}

// ---------------------------------------------------------------- import preview

function openImport(preview) {
  if (!wide()) history.pushState({ v: "import" }, "");
  setState({ tab: "import", importPreview: { ...preview, selected: new Set(preview.items.filter((i) => i.status === "new").map((i) => i.index)), updateChanged: false } });
}

function pageImport() {
  const pv = state.importPreview;
  const body = el("div");
  const tag = { new: ["good", "New"], duplicate: ["", "Already saved"], changed: ["warn", "Different password"] };
  body.innerHTML = `
    <div class="card pad">
      <div style="display:flex;gap:10px;flex-wrap:wrap;justify-content:center">
        <span class="badge good">${pv.new_count} new</span>
        <span class="badge">${pv.duplicate_count} already saved</span>
        ${pv.changed_count ? `<span class="badge warn">${pv.changed_count} with a different password</span>` : ""}
      </div>
      ${pv.changed_count ? `<label class="row plain" style="padding:14px 0 0;min-height:0"><div class="grow"><div class="title" style="white-space:normal">Update ${pv.changed_count} existing password${pv.changed_count > 1 ? "s" : ""}</div><div class="sub" style="white-space:normal">Use the imported password for logins you already have</div></div><span class="switch"><input type="checkbox" id="upd" ${pv.updateChanged ? "checked" : ""}><span></span></span></label>` : ""}
    </div>
    <div style="display:flex;justify-content:space-between;align-items:center;margin:16px 6px 6px">
      <span class="hint" id="sel-count"></span>
      <span><button class="link" id="sel-all">Select all new</button> · <button class="link" id="sel-none">None</button></span>
    </div>
    <div class="card" id="items"></div>
    <div class="bottom-actions">
      <button class="btn" id="cancel">Cancel</button>
      <button class="btn primary" id="go">${icon("download", "sm")} Import</button>
    </div>`;
  const items = body.querySelector("#items");
  const draw = () => {
    items.innerHTML = "";
    for (const it of pv.items) {
      const [cls, label] = tag[it.status];
      const row = el("label", "row");
      row.innerHTML = `<input type="checkbox" class="check" ${pv.selected.has(it.index) ? "checked" : ""} ${it.status === "duplicate" ? "disabled" : ""}>
        ${avatar(it.name)}<div class="grow"><div class="title">${esc(it.name)}</div><div class="sub">${esc(it.username || hostOf(it.url))}</div></div>
        <span class="badge ${cls}">${label}</span>`;
      row.querySelector("input").onchange = (e) => {
        e.target.checked ? pv.selected.add(it.index) : pv.selected.delete(it.index);
        count();
      };
      items.appendChild(row);
    }
    count();
  };
  const count = () => {
    const changedSel = pv.items.filter((i) => i.status === "changed" && pv.selected.has(i.index)).length;
    const n = pv.items.filter((i) => i.status === "new" && pv.selected.has(i.index)).length + (pv.updateChanged ? changedSel : 0);
    body.querySelector("#sel-count").textContent = `${n} to import`;
    body.querySelector("#go").disabled = n === 0;
  };
  body.querySelector("#upd")?.addEventListener("change", (e) => {
    pv.updateChanged = e.target.checked;
    for (const i of pv.items) if (i.status === "changed") e.target.checked ? pv.selected.add(i.index) : pv.selected.delete(i.index);
    draw();
  });
  body.querySelector("#sel-all").onclick = () => {
    pv.items.filter((i) => i.status === "new").forEach((i) => pv.selected.add(i.index));
    draw();
  };
  body.querySelector("#sel-none").onclick = () => {
    pv.selected.clear();
    draw();
  };
  const leave = async () => {
    await invoke("import_cancel").catch(() => {});
    state.importPreview = null;
    if (!wide()) history.back();
    else setState({ tab: "settings" });
  };
  body.querySelector("#cancel").onclick = leave;
  body.querySelector("#go").onclick = async (e) => {
    try {
      const r = await busy(e.currentTarget, "Importing…", () => invoke("import_commit", { indices: [...pv.selected], updateChanged: pv.updateChanged }));
      state.accounts = r.accounts;
      state.importPreview = null;
      history.replaceState({ v: "root" }, "");
      setState({ tab: "passwords", selectedId: null, filter: "all" });
      toast(`Imported ${r.added}${r.updated ? `, updated ${r.updated}` : ""}`, 3000);
      if (/\.csv$/i.test(pv.source)) setTimeout(() => toast("Tip: delete the CSV file now — it has your passwords in plain text", 5000), 3100);
      scheduleSync();
    } catch (err) {
      handleErr(err);
    }
  };
  draw();
  return page("import", "Import", esc(pv.source), "", body, { back: true, detail: true });
}

// ---------------------------------------------------------------- sync loop

function updateSyncLabels() {
  const meta = document.querySelector(".hero .meta");
  if (meta && state.tab === "passwords") meta.innerHTML = syncMeta();
}

async function runSync(manual = false) {
  if (!state.status.sync_url || state.screen !== "main") return;
  if (sync.running) return;
  sync.running = true;
  sync.state = "syncing";
  updateSyncLabels();
  try {
    const r = await invoke("sync_now");
    sync.state = "ok";
    sync.msg = "";
    state.status.last_sync_ms = r.last_sync_ms;
    if (r.changed_locally) {
      state.accounts = r.accounts;
      // Don't yank the form away from someone mid-edit.
      if (!state.editing) render();
      if (manual) toast("Synced — got changes from your other devices");
    } else if (manual) toast("Up to date");
  } catch (e) {
    sync.state = "error";
    sync.msg = errText(e);
    if (/locked/i.test(sync.msg)) {
      sync.running = false;
      return lockNow("Vault locked");
    }
    if (manual) toast(sync.msg, 4000);
  } finally {
    sync.running = false;
    updateSyncLabels();
  }
}
function scheduleSync(delay = 1200) {
  clearTimeout(sync.timer);
  sync.timer = setTimeout(() => runSync(false), delay);
}
function startSyncLoop() {
  stopSyncLoop();
  sync.state = state.status.sync_url ? "ok" : "off";
  if (state.status.sync_url) {
    scheduleSync(300);
    sync.interval = setInterval(() => runSync(false), 120000);
  }
}
function stopSyncLoop() {
  clearTimeout(sync.timer);
  clearInterval(sync.interval);
}

// ---------------------------------------------------------------- auto-lock (desktop)

let lastActivity = Date.now();
function resetIdle() {
  lastActivity = Date.now();
}
["mousemove", "keydown", "mousedown", "touchstart", "wheel"].forEach((ev) => window.addEventListener(ev, resetIdle, { passive: true }));
setInterval(() => {
  if (state.screen !== "main" || isAndroid()) return;
  const m = Number(prefs.autoLockMin);
  if (m > 0 && Date.now() - lastActivity > m * 60000) lockNow("Locked after inactivity");
}, 15000);

// Desktop shortcuts
window.addEventListener("keydown", (e) => {
  if (state.screen !== "main") return;
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key.toLowerCase() === "f") {
    e.preventDefault();
    if (state.tab !== "passwords") goTab("passwords");
    setTimeout(() => document.querySelector("#q")?.focus(), 0);
  } else if (mod && e.key.toLowerCase() === "n") {
    e.preventDefault();
    openNew();
  } else if (mod && e.key.toLowerCase() === "l") {
    e.preventDefault();
    lockNow();
  } else if (e.key === "Escape" && document.querySelector(".scrim") == null && state.selectedId && wide()) {
    if (state.editing && state.selectedId !== "__new__") setState({ editing: false });
    else closeDetail();
  }
});

// ---------------------------------------------------------------- misc

function formatSecret(s) {
  return s.replace(/(.{4})/g, "$1 ").trim();
}

async function initAutoDetect(area, onImported) {
  try {
    if (!(await invoke("has_full_file_access"))) {
      area.innerHTML = `<div class="alt"><button class="link" id="grant">Look for an exported vault on this phone</button></div>`;
      area.querySelector("#grant").onclick = async () => {
        await invoke("request_full_file_access");
        area.innerHTML = `<div class="hint center">Allow "All files access", then come back.</div><div class="alt"><button class="link" id="recheck">Check again</button></div>`;
        area.querySelector("#recheck").onclick = () => initAutoDetect(area, onImported);
      };
      return;
    }
    const paths = await invoke("scan_for_vault_files");
    if (!paths.length) {
      area.innerHTML = "";
      return;
    }
    area.innerHTML = `<div class="hint center" style="margin-top:12px">Found on this phone:</div><div class="card" style="margin-top:8px" id="found"></div>`;
    const list = area.querySelector("#found");
    for (const path of paths) {
      const row = el("button", "row plain", `<div class="grow"><div class="title">${esc(path.split("/").pop())}</div><div class="sub">${esc(path)}</div></div>${icon("chev", "sm chev")}`);
      row.onclick = async () => {
        try {
          onImported(await invoke("import_vault_from_path", { path }));
        } catch (e) {
          toast(errText(e));
        }
      };
      list.appendChild(row);
    }
  } catch {
    area.innerHTML = "";
  }
}

async function scanQrRaw(hintText) {
  const perm = await invoke("plugin:barcode-scanner|request_permissions");
  if (perm.camera !== "granted") throw new Error("Camera permission denied");
  // windowed:true keeps the camera preview behind a transparent webview —
  // the non-windowed mode leaves Android's WebView stuck not repainting.
  const overlay = el("div", "qr-scan-overlay", `<div class="qr-scan-frame"></div><div class="qr-scan-hint">${esc(hintText || "Point the camera at the QR code")}</div><button class="btn qr-scan-cancel">Cancel</button>`);
  document.body.appendChild(overlay);
  document.documentElement.classList.add("qr-scanning");
  app.style.display = "none";
  let cancelled = false;
  const cleanup = () => {
    document.documentElement.classList.remove("qr-scanning");
    app.style.display = "";
    overlay.remove();
  };
  overlay.querySelector(".qr-scan-cancel").onclick = () => {
    cancelled = true;
    cleanup();
    invoke("plugin:barcode-scanner|cancel").catch(() => {});
  };
  try {
    const result = await invoke("plugin:barcode-scanner|scan", { windowed: true, formats: ["QR_CODE"], cameraDirection: "back" });
    if (!cancelled) cleanup();
    return result.content;
  } catch (e) {
    if (!cancelled) cleanup();
    throw e;
  }
}

function extractSecretFromScan(content) {
  if (content.startsWith("otpauth://")) {
    try {
      const secret = new URL(content).searchParams.get("secret");
      if (secret) return secret;
    } catch {}
  }
  const cleaned = content.replace(/\s+/g, "").toUpperCase();
  return /^[A-Z2-7]+=*$/.test(cleaned) ? cleaned : null;
}

function render() {
  switch (state.screen) {
    case "setup":
      return renderSetup();
    case "setup-confirm":
      return renderSetupConfirm();
    case "link":
      return renderLink();
    case "unlock":
      return renderUnlock();
    case "main":
      return renderMain();
    default:
      app.innerHTML = `<div class="auth"><div class="top"><div class="logo">${icon("shield", "lg")}</div></div></div>`;
  }
}

// Re-layout when crossing the phone/desktop breakpoint (resizing, DeX).
let wasWide = wide();
window.addEventListener("resize", () => {
  if (wide() !== wasWide) {
    wasWide = wide();
    if (state.screen === "main") render();
  }
});

// The Rust side auto-locks when the app is backgrounded on mobile. Re-check
// whenever we come back so a stale screen doesn't sit on a locked vault.
document.addEventListener("visibilitychange", async () => {
  if (document.visibilityState !== "visible") return;
  if (state.screen === "main") {
    const s = await refreshStatus();
    if (!s.unlocked) return lockNow();
    if (state.tab === "settings") {
      await refreshBio();
      render(); // autofill status may have changed in system settings
    }
    scheduleSync(500);
  } else if (state.screen === "unlock" && !bioInFlight) {
    await refreshBio();
    unlockUsePassword = false;
    render();
  }
});

render();
boot();
