// Umbilical UI. Plain JS, no build step. Talks to Rust with `invoke`.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const PERMISSION_MODES = ["acceptEdits", "auto", "bypassPermissions", "default", "dontAsk", "plan"];
const SPAWN_MODES = ["same-dir", "worktree", "session"];

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

let snapshot = { dirs: [], running: 0, total: 0 };
let selected = null;
let tab = "status";
let config = null; // editable copy
let savedConfig = null; // last loaded / saved
let overrideKey = null;
let info = null;

// ---------- helpers ----------

function ago(unix) {
  const s = Math.max(0, Math.floor(Date.now() / 1000 - unix));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h`;
}

function stateText(d) {
  switch (d.state) {
    case "running": return d.started_at ? `running for ${ago(d.started_at)}` : "running";
    case "waiting": {
      if (!d.next_restart_at) return "waiting";
      const left = Math.max(0, d.next_restart_at - Math.floor(Date.now() / 1000));
      return `restart in ${left}s`;
    }
    case "stopped": return "stopped by you";
    case "disabled": return "disabled in settings";
    default: return d.state;
  }
}

// Red dot for errors: something needs the user, or it crashed and waits to retry.
function dotClass(d) {
  if (d.attention || (d.state === "waiting" && d.last_exit)) return "error";
  return d.state;
}

const ATTENTION = {
  trust_prompt: "The workspace trust dialog is waiting. Turn on <b>Trust every target folder</b> in Settings (it is on by default), or run <code>claude</code> once in this folder, then press ↻.",
  bypass_prompt: "The bypass permissions warning is waiting. Set <code>\"skipDangerousModePermissionPrompt\": true</code> in <code>~/.claude/settings.json</code>, then restart.",
  remote_control_consent: "claude asks <b>Enable Remote Control? (y/n)</b>. The session does not appear on claude.ai until you answer. <button class=\"btn small primary\" data-answer=\"y\">Enable</button> <button class=\"btn small\" data-answer=\"n\">No</button>",
  login_required: "claude is not logged in, or the login expired. Press <b>Log in…</b> to log in from Umbilical, or run <code>claude auth login</code> in a terminal, then press ↻.",
};

function lines(text) {
  return text.split("\n").map((l) => l.trim()).filter(Boolean);
}

function parseEnv(text) {
  const env = {};
  for (const l of lines(text)) {
    const i = l.indexOf("=");
    if (i > 0) env[l.slice(0, i).trim()] = l.slice(i + 1);
  }
  return env;
}

function envText(env) {
  return Object.entries(env || {}).map(([k, v]) => `${k}=${v}`).join("\n");
}

function numOrNull(v) {
  const n = parseInt(v, 10);
  return Number.isFinite(n) && n > 0 ? n : null;
}

function fillSelect(el, values, inheritLabel) {
  el.innerHTML = "";
  if (inheritLabel !== undefined) el.append(new Option(inheritLabel, ""));
  for (const v of values) el.append(new Option(v, v));
}

// ---------- tabs ----------

function showTab(name) {
  tab = name;
  for (const b of document.querySelectorAll(".tab")) b.classList.toggle("active", b.dataset.tab === name);
  for (const v of ["status", "settings", "about"]) $(`view-${v}`).classList.toggle("hidden", v !== name);
  if (name === "settings" && !config) loadConfig();
  if (name === "about") loadAbout();
}

for (const b of document.querySelectorAll(".tab")) b.onclick = () => showTab(b.dataset.tab);
document.addEventListener("click", (e) => {
  const a = e.target.closest("[data-goto]");
  if (a) { e.preventDefault(); showTab(a.dataset.goto); }
});

// ---------- status ----------

function renderStatus() {
  const s = snapshot;
  $("summary-text").textContent = `${s.running} of ${s.total} running`;

  const banner = $("banner");
  const problems = [];
  if (s.daemon_error) problems.push(`Cannot reach the Umbilical daemon: ${esc(s.daemon_error)}`);
  if (s.config_error) problems.push(`Config error (old settings still used): ${esc(s.config_error)}`);
  if (s.dirs.length && !s.claude_bin) problems.push("claude binary not found. Set it in Settings.");
  banner.innerHTML = problems.join("<br>");
  banner.classList.toggle("hidden", problems.length === 0);

  const needConsent = s.dirs.filter((d) => d.attention === "remote_control_consent").length;
  $("consent-count").textContent = needConsent === 1 ? "1 folder" : `${needConsent} folders`;
  $("consent-banner").classList.toggle("hidden", needConsent === 0);

  const needLogin = s.dirs.filter((d) => d.attention === "login_required").length;
  $("login-count").textContent = needLogin === 1 ? "1 folder" : `${needLogin} folders`;
  $("login-banner").classList.toggle("hidden", needLogin === 0);

  const list = $("dir-list");
  list.innerHTML = s.dirs.map((d) => `
    <div class="dir ${d.key === selected ? "selected" : ""}" data-key="${esc(d.key)}">
      <span class="dot ${dotClass(d)}"></span>
      <span class="name">${esc(d.name)}</span>
      <span class="sub">${esc(stateText(d))}</span>
    </div>`).join("");
  for (const el of list.querySelectorAll(".dir")) {
    el.onclick = () => { selected = el.dataset.key; renderStatus(); refreshLog(true); };
  }

  if (!selected || !s.dirs.some((d) => d.key === selected)) selected = s.dirs[0]?.key ?? null;
  const d = s.dirs.find((x) => x.key === selected);
  $("detail-empty").classList.toggle("hidden", !!d);
  $("detail-body").classList.toggle("hidden", !d);
  if (!d) {
    $("roots-hint").textContent = config ? `Roots: ${config.roots.join(", ")}` : "";
    return;
  }

  $("d-name").textContent = d.name;
  $("d-path").textContent = d.path;
  $("d-state").textContent = stateText(d) + (d.pid ? `  (pid ${d.pid})` : "");
  $("d-url").innerHTML = d.session_url ? `<a href="#" data-open="${esc(d.session_url)}">${esc(d.session_url)}</a>` : '<span class="muted">not seen yet</span>';
  $("d-spawn").textContent = d.spawn === d.effective_spawn ? d.spawn : `${d.effective_spawn} (set: ${d.spawn}, no git)`;
  $("d-restarts").textContent = d.restarts;
  $("d-exit").textContent = d.last_exit ?? "—";
  $("d-error").textContent = d.last_error ?? "—";
  $("d-command").textContent = d.command ?? "—";

  const att = $("d-attention");
  att.innerHTML = d.attention ? ATTENTION[d.attention] ?? d.attention : "";
  if (d.attention === "login_required") att.innerHTML += ' <button class="btn small primary" data-login>Log in…</button>';
  att.classList.toggle("hidden", !d.attention);

  const act = (a) => document.querySelector(`[data-act="${a}"]`);
  act("stop").disabled = d.state !== "running" && d.state !== "waiting";
  act("start").disabled = d.state === "running" || d.state === "disabled";
  act("restart").disabled = d.state === "disabled";
}

for (const b of document.querySelectorAll("[data-act]")) {
  b.onclick = () => selected && invoke("dir_action", { key: selected, action: b.dataset.act });
}
$("restart-all").onclick = () => invoke("restart_all");

document.addEventListener("click", (e) => {
  const a = e.target.closest("[data-open]");
  if (a) { e.preventDefault(); invoke("open_path", { path: a.dataset.open }).catch(() => {}); }
});

$("d-open-log").onclick = () => {
  const d = snapshot.dirs.find((x) => x.key === selected);
  if (d) invoke("reveal_path", { path: d.log_path }).catch((e) => alertMsg(e));
};
$("d-reveal").onclick = () => selected && invoke("open_path", { path: selected });
$("d-settings").onclick = () => {
  overrideKey = selected;
  showTab("settings");
  if (config) renderOverride();
};

async function refreshLog(force) {
  if (tab !== "status" || !selected) return;
  const el = $("d-log");
  const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  const tail = await invoke("get_tail", { key: selected, lines: 500 });
  const text = tail.join("\n");
  if (force || el.textContent !== text) {
    el.textContent = text || "(no output yet)";
    if (force || nearBottom) el.scrollTop = el.scrollHeight;
  }
}

function alertMsg(e) {
  $("banner").textContent = String(e);
  $("banner").classList.remove("hidden");
}

// ---------- settings ----------

fillSelect($("s-permission"), PERMISSION_MODES);
fillSelect($("s-spawn"), SPAWN_MODES);

async function loadConfig() {
  try {
    savedConfig = await invoke("get_config");
    config = structuredClone(savedConfig);
    renderSettings();
    setSaveMsg("");
  } catch (e) {
    setSaveMsg(`Cannot read config: ${e}`, true);
  }
}

function renderPathList(el, items, onRemove) {
  el.innerHTML = items.length
    ? items.map((p, i) => `<li><span class="mono">${esc(p)}</span><button class="btn small" data-i="${i}">Remove</button></li>`).join("")
    : '<li class="empty-item">none</li>';
  for (const b of el.querySelectorAll("button")) b.onclick = () => onRemove(parseInt(b.dataset.i, 10));
}

function renderSettings() {
  const c = config;
  renderPathList($("s-roots"), c.roots, (i) => { c.roots.splice(i, 1); renderSettings(); });
  renderPathList($("s-dirs"), c.dirs, (i) => { c.dirs.splice(i, 1); renderSettings(); });
  $("s-exclude").value = c.exclude.join("\n");
  $("s-trust").checked = c.auto_trust;
  $("s-permission").value = c.permission_mode;
  $("s-spawn").value = c.spawn;
  $("s-capacity").value = c.capacity ?? "";
  $("s-claude").value = c.claude_bin;
  $("s-claude").placeholder = snapshot.claude_bin ? `auto: ${snapshot.claude_bin}` : "auto (not found)";
  $("s-args").value = c.extra_args.join("\n");
  $("s-env").value = envText(c.env);
  $("s-bmin").value = c.backoff_min_secs;
  $("s-bmax").value = c.backoff_max_secs;
  $("s-stable").value = c.stable_reset_secs;
  $("s-scan").value = c.scan_interval_secs;
  $("s-autostart").checked = c.autostart;
  $("s-logdir").value = c.log_dir;
  $("s-logdir").placeholder = snapshot.log_dir ? `default: ${snapshot.log_dir}` : "default";
  $("s-logmax").value = Math.round(c.log_max_bytes / (1024 * 1024));
  $("s-channel").value = c.update.channel;
  $("s-uint").value = c.update.check_interval_hours;
  $("s-ustart").checked = c.update.check_on_start;
  $("s-uauto").checked = c.update.auto_install;
  renderOverrideKeys();
}

function renderOverrideKeys() {
  const sel = $("o-key");
  const keys = new Set(snapshot.dirs.map((d) => d.key));
  for (const k of Object.keys(config.overrides)) keys.add(k);
  const current = overrideKey ?? sel.value;
  sel.innerHTML = '<option value="">Choose a folder…</option>';
  for (const k of [...keys].sort()) {
    const d = snapshot.dirs.find((x) => x.key === k);
    const label = d ? `${d.name} — ${k}` : `${k} (not found now)`;
    sel.append(new Option(label, k));
  }
  sel.value = keys.has(current) ? current : "";
  renderOverride();
}

$("o-key").onchange = () => { readOverride(); overrideKey = $("o-key").value || null; renderOverride(); };

function renderOverride() {
  const key = $("o-key").value || overrideKey;
  if (key && $("o-key").value !== key) $("o-key").value = key;
  overrideKey = key || null;
  $("o-form").classList.toggle("hidden", !key);
  if (!key) return;
  const o = config.overrides[key] ?? {};
  const d = snapshot.dirs.find((x) => x.key === key);
  fillSelect($("o-permission"), PERMISSION_MODES, `default (${config.permission_mode})`);
  fillSelect($("o-spawn"), SPAWN_MODES, `default (${config.spawn})`);
  $("o-enabled").checked = o.enabled ?? true;
  $("o-name").value = o.name ?? "";
  $("o-name").placeholder = d ? d.name : "folder name";
  $("o-permission").value = o.permission_mode ?? "";
  $("o-spawn").value = o.spawn ?? "";
  $("o-capacity").value = o.capacity ?? "";
  $("o-capacity").placeholder = config.capacity ? `default (${config.capacity})` : "default";
  $("o-claude").value = o.claude_bin ?? "";
  $("o-claude").placeholder = "default";
  $("o-args-on").checked = Array.isArray(o.extra_args);
  $("o-args").value = (o.extra_args ?? []).join("\n");
  $("o-args").disabled = !Array.isArray(o.extra_args);
  $("o-env").value = envText(o.env);
}

$("o-args-on").onchange = () => { $("o-args").disabled = !$("o-args-on").checked; };

function readOverride() {
  if (!overrideKey || !config) return;
  const o = {};
  if (!$("o-enabled").checked) o.enabled = false;
  const name = $("o-name").value.trim();
  if (name) o.name = name;
  if ($("o-permission").value) o.permission_mode = $("o-permission").value;
  if ($("o-spawn").value) o.spawn = $("o-spawn").value;
  const cap = numOrNull($("o-capacity").value);
  if (cap) o.capacity = cap;
  const bin = $("o-claude").value.trim();
  if (bin) o.claude_bin = bin;
  if ($("o-args-on").checked) o.extra_args = lines($("o-args").value);
  const env = parseEnv($("o-env").value);
  if (Object.keys(env).length) o.env = env;
  if (Object.keys(o).length) config.overrides[overrideKey] = o;
  else delete config.overrides[overrideKey];
}

function readSettings() {
  const c = config;
  c.exclude = lines($("s-exclude").value);
  c.auto_trust = $("s-trust").checked;
  c.permission_mode = $("s-permission").value;
  c.spawn = $("s-spawn").value;
  c.capacity = numOrNull($("s-capacity").value);
  c.claude_bin = $("s-claude").value.trim();
  c.extra_args = lines($("s-args").value);
  c.env = parseEnv($("s-env").value);
  c.backoff_min_secs = parseInt($("s-bmin").value, 10) || 5;
  c.backoff_max_secs = parseInt($("s-bmax").value, 10) || 300;
  c.stable_reset_secs = parseInt($("s-stable").value, 10) || 0;
  c.scan_interval_secs = parseInt($("s-scan").value, 10) || 5;
  c.autostart = $("s-autostart").checked;
  c.log_dir = $("s-logdir").value.trim();
  c.log_max_bytes = Math.max(0, parseInt($("s-logmax").value, 10) || 0) * 1024 * 1024;
  c.update.channel = $("s-channel").value;
  c.update.check_interval_hours = parseInt($("s-uint").value, 10) || 0;
  c.update.check_on_start = $("s-ustart").checked;
  c.update.auto_install = $("s-uauto").checked;
  readOverride();
}

async function addPath(list) {
  readSettings();
  const p = await invoke("pick_folder");
  if (p && !config[list].includes(p)) config[list].push(p);
  renderSettings();
}
$("s-add-root").onclick = () => addPath("roots");
$("s-add-dir").onclick = () => addPath("dirs");

function setSaveMsg(text, error) {
  const el = $("save-msg");
  el.textContent = text;
  el.classList.toggle("error", !!error);
}

$("s-save").onclick = async () => {
  readSettings();
  try {
    await invoke("save_config", { config });
    savedConfig = structuredClone(config);
    setSaveMsg(`Saved at ${new Date().toLocaleTimeString()}. Changed folders restart now.`);
  } catch (e) {
    setSaveMsg(String(e), true);
  }
};
$("s-revert").onclick = () => loadConfig();
$("s-reveal-config").onclick = async () => {
  const i = info ?? (await invoke("app_info"));
  invoke("reveal_path", { path: i.config_path }).catch((e) => setSaveMsg(String(e), true));
};

// ---------- about / update ----------

async function loadAbout() {
  info = await invoke("app_info");
  $("a-version").textContent = `v${info.version}`;
  $("a-claude").textContent = info.claude_bin ?? "not found";
  $("a-config").innerHTML = `<a href="#" id="a-config-link">${esc(info.config_path)}</a>`;
  $("a-config-link").onclick = (e) => { e.preventDefault(); invoke("reveal_path", { path: info.config_path }); };
  $("a-logs").innerHTML = `<a href="#" data-open="${esc(snapshot.log_dir)}">${esc(snapshot.log_dir)}</a>`;
  $("a-repo").textContent = info.repo;
  $("a-repo").dataset.open = info.repo;
  renderUpdate(await invoke("get_update"));
}

function renderUpdate(u) {
  if (!u) return;
  let text;
  if (u.installing) text = "Installing… the app restarts when done.";
  else if (u.checking) text = "Checking…";
  else if (u.error) text = `Update check failed: ${u.error}`;
  else if (u.available) text = `Version ${u.available.version} is available (${u.channel} channel).`;
  else if (u.last_checked) text = `Up to date (${u.channel} channel). Last check ${ago(u.last_checked)} ago.`;
  else text = "Not checked yet.";
  $("u-status").textContent = text;
  const notes = u.available?.notes;
  $("u-notes").textContent = notes ?? "";
  $("u-notes").classList.toggle("hidden", !notes);
  $("u-install").classList.toggle("hidden", !u.available || u.installing);
  $("u-check").disabled = u.checking || u.installing;
}

$("u-check").onclick = async () => {
  $("u-status").textContent = "Checking…";
  try { await invoke("check_update"); } catch (_) { /* shown by the event */ }
  renderUpdate(await invoke("get_update"));
};
$("u-install").onclick = () => invoke("install_update");

// Answer once: a second click would type a second "y" into the session.
$("consent-all").onclick = () => {
  const b = $("consent-all");
  if (b.disabled) return;
  b.disabled = true;
  invoke("answer_all", { attention: "remote_control_consent", text: "y" });
  setTimeout(() => { b.disabled = false; }, 5000);
};
document.addEventListener("click", (e) => {
  const b = e.target.closest("[data-answer]");
  if (!b || !selected || b.disabled) return;
  for (const x of document.querySelectorAll("[data-answer]")) x.disabled = true;
  invoke("answer", { key: selected, text: b.dataset.answer });
});

// ---------- login ----------

let loginTimer = null;

async function openLogin() {
  $("login-modal").classList.remove("hidden");
  $("login-log").textContent = "Starting…";
  $("login-links").innerHTML = "";
  $("login-result").textContent = "";
  $("login-result").className = "";
  $("login-cancel").classList.remove("hidden");
  $("login-close").classList.add("hidden");
  try {
    await invoke("login_start");
  } catch (e) {
    $("login-result").textContent = String(e);
    $("login-result").className = "bad";
    return;
  }
  clearInterval(loginTimer);
  loginTimer = setInterval(pollLogin, 500);
}

async function pollLogin() {
  const v = await invoke("login_view");
  const text = v.lines.map((l) => l.replace(/^\S+ \S+ /, "")).join("\n");
  const el = $("login-log");
  if (el.textContent !== text) { el.textContent = text || "…"; el.scrollTop = el.scrollHeight; }
  const urls = [...new Set(text.match(/https:\/\/[^\s"'<>]+/g) || [])];
  $("login-links").innerHTML = urls.map((u) => `<a href="#" data-open="${esc(u)}">Open login page: ${esc(u)}</a>`).join("");
  if (!v.running && v.exit) {
    clearInterval(loginTimer);
    $("login-result").textContent = v.success ? "Logged in. Restarting all sessions…" : `Login did not finish (${v.exit}).`;
    $("login-result").className = v.success ? "ok" : "bad";
    $("login-cancel").classList.add("hidden");
    $("login-close").classList.remove("hidden");
  }
}

document.addEventListener("click", (e) => {
  if (e.target.closest("[data-login]")) { e.preventDefault(); openLogin(); }
});
$("login-form").onsubmit = (e) => {
  e.preventDefault();
  const t = $("login-input").value.trim();
  if (t) invoke("login_input", { text: t });
  $("login-input").value = "";
};
$("login-cancel").onclick = async () => {
  clearInterval(loginTimer);
  await invoke("login_cancel");
  $("login-modal").classList.add("hidden");
};
$("login-close").onclick = () => $("login-modal").classList.add("hidden");

// ---------- first start ----------

let setupCustom = null;
let inSetup = false;

function setupRoot() {
  const v = document.querySelector('input[name="setup-root"]:checked').value;
  return v === "custom" ? setupCustom : "~/.umbilical/repos";
}

async function renderSetupPreview() {
  const root = setupRoot();
  const el = $("setup-preview");
  if (!root) { el.innerHTML = '<span class="muted">Choose a folder.</span>'; return; }
  try {
    const names = await invoke("list_subdirs", { path: root });
    el.innerHTML = names.length
      ? `<b>${names.length} session${names.length > 1 ? "s" : ""} will start:</b><ul>${names.map((n) => `<li>${esc(n)}</li>`).join("")}</ul>`
      : '<span class="muted">The folder is empty now. No session starts until you add folders.</span>';
  } catch (_) {
    el.innerHTML = '<span class="muted">The folder does not exist yet. It will be created. No session starts until you add folders.</span>';
  }
}

function enterSetup(on) {
  if (on === inSetup) return;
  inSetup = on;
  document.body.classList.toggle("setup-mode", on);
  $("view-setup").classList.toggle("hidden", !on);
  if (on) {
    for (const v of ["status", "settings", "about"]) $(`view-${v}`).classList.add("hidden");
    renderSetupPreview();
  } else {
    config = null;
    showTab("status");
  }
}

for (const r of document.querySelectorAll('input[name="setup-root"]')) r.onchange = renderSetupPreview;
$("setup-pick").onclick = async () => {
  const p = await invoke("pick_folder");
  if (!p) return;
  setupCustom = p;
  $("setup-path").textContent = p;
  document.querySelector('input[name="setup-root"][value="custom"]').checked = true;
  renderSetupPreview();
};
$("setup-start").onclick = async () => {
  const root = setupRoot();
  if (!root) { $("setup-msg").textContent = "Choose a folder first."; return; }
  $("setup-start").disabled = true;
  try {
    await invoke("complete_setup", { root, autostart: $("setup-autostart").checked });
    $("setup-msg").textContent = "Starting…";
  } catch (e) {
    $("setup-msg").textContent = String(e);
    $("setup-start").disabled = false;
  }
};

// ---------- start ----------

listen("status", (e) => {
  snapshot = e.payload;
  enterSetup(!!snapshot.needs_setup);
  if (inSetup) return;
  renderStatus();
  if (tab === "settings" && config && document.activeElement?.id !== "o-key") {
    // Keep the folder list fresh without touching fields being edited.
    const before = $("o-key").options.length;
    const known = new Set([...$("o-key").options].map((o) => o.value));
    if (snapshot.dirs.some((d) => !known.has(d.key)) || before === 0) {
      readSettings();
      renderOverrideKeys();
    }
  }
});
listen("update", (e) => renderUpdate(e.payload));

(async () => {
  snapshot = await invoke("get_status");
  enterSetup(!!snapshot.needs_setup);
  renderStatus();
  refreshLog(true);
  setInterval(() => refreshLog(false), 1000);
  config = null;
  invoke("get_config").then((c) => { savedConfig = c; config = structuredClone(c); renderSettings(); }).catch(() => {});
})();
