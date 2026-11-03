// UI smoke walk, injected via PASSVAULT_TEST_SCRIPT (debug builds only).
(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const q = (s) => document.querySelector(s);
  const badge = document.createElement("div");
  badge.style.cssText = "position:fixed;left:8px;bottom:8px;z-index:9999;background:#c00;color:#fff;font:12px monospace;padding:4px 8px;border-radius:6px;pointer-events:none";
  document.body.appendChild(badge);
  badge.style.display = "none";
  const log = (m) => { badge.textContent = m; badge.style.display = m ? "" : "none"; };
  window.addEventListener("error", (e) => log("JS ERROR: " + e.message));
  window.addEventListener("unhandledrejection", (e) => log("REJECTION: " + (e.reason?.message || e.reason)));
  const setVal = (s, v) => { const e = q(s); e.value = v; e.dispatchEvent(new Event("input")); };
  const clickText = (sel, text) => [...document.querySelectorAll(sel)].find((e) => e.textContent.includes(text))?.click();
  try {
    STEPS
  } catch (e) {
    log("STEP FAILED: " + e.message);
  }
})();
