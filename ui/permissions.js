// Permissions screen (macOS only). The list is empty on other platforms,
// and then the tab stays hidden.
const PERM_STATE = {
  granted: "Allowed",
  denied: "Not allowed",
  not_asked: "Not asked yet",
  unknown: "See System Settings",
};

async function loadPermissions() {
  let list;
  try {
    list = await invoke("get_permissions");
  } catch {
    return;
  }
  if (!list.length) return;
  $("tab-permissions").classList.remove("hidden");
  const missing = list.some((p) => p.important && p.state !== "granted");
  $("perm-dot").classList.toggle("hidden", !missing);
  $("perm-list").innerHTML = list.map(permRow).join("");
  showPermissionsOnce(missing);
}

function permRow(p) {
  const dot = p.state === "granted" ? "running" : p.important ? "error" : "stopped";
  const label = p.can_prompt && p.state !== "denied" ? "Allow…" : "Open System Settings";
  const button = p.state === "granted" ? "" : `<button class="btn small" data-perm="${esc(p.id)}">${label}</button>`;
  return `<div class="perm">
    <span class="dot ${dot}"></span>
    <div class="perm-text"><b>${esc(p.name)}</b> <span class="muted">${esc(PERM_STATE[p.state] ?? p.state)}</span><br>
      <small class="muted">${esc(p.why)}</small></div>
    ${button}
  </div>`;
}

// First start: show this screen once, when something important is missing.
function showPermissionsOnce(missing) {
  if (!missing || inSetup) return;
  try {
    if (localStorage.getItem("permissionsShown")) return;
    localStorage.setItem("permissionsShown", "1");
  } catch {
    return;
  }
  showTab("permissions");
}

$("perm-list").addEventListener("click", async (e) => {
  const b = e.target.closest("[data-perm]");
  if (!b) return;
  b.disabled = true;
  try {
    await invoke("request_permission", { id: b.dataset.perm });
  } catch (err) {
    console.error(err);
  }
  setTimeout(loadPermissions, 1000);
});

loadPermissions();
setInterval(loadPermissions, 3000);
