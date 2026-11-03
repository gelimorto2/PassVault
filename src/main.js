const invoke = (...args) => window.__TAURI__.core.invoke(...args);

const app = document.getElementById("app");

let state = {
  screen: "loading", // loading | setup | setup-confirm | link | unlock | main
  vaultPath: "",
  error: "",
  setupPending: null, // { secret_b32, otpauth_uri, qr_svg }
  accounts: [],
  selectedId: null,
  editing: false,
};

function setState(patch) {
  state = { ...state, ...patch };
  render();
}

async function withLoading(btn, loadingLabel, fn) {
  const original = btn.textContent;
  btn.disabled = true;
  btn.textContent = loadingLabel;
  try {
    await fn();
  } finally {
    btn.disabled = false;
    btn.textContent = original;
  }
}

function showToast(message) {
  const el = document.createElement("div");
  el.className = "toast";
  el.textContent = message;
  document.body.appendChild(el);
  setTimeout(() => el.remove(), 2000);
}

async function copyToClipboard(text) {
  try {
    await navigator.clipboard.writeText(text);
    showToast("Copied to clipboard");
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
    showToast("Copied to clipboard");
  }
}

async function boot() {
  const status = await invoke("app_status");
  setState({
    vaultPath: status.vault_path,
    screen: !status.vault_exists
      ? "setup"
      : !status.device_linked
      ? "link"
      : "unlock",
  });
}

// ---------- Screen: Setup (step 1 - choose password) ----------

function renderSetup() {
  app.innerHTML = "";
  const card = el("div", "auth-card");
  card.innerHTML = `
    <h1>🔐 Set up PassVault</h1>
    <div class="subtitle">Choose a master password. It never leaves this device.</div>
    <label>Master password</label>
    <input type="password" id="pw1" autocomplete="new-password" />
    <label>Confirm password</label>
    <input type="password" id="pw2" autocomplete="new-password" />
    <label>Vault file location</label>
    <div class="folder-row">
      <div class="path">${escapeHtml(state.vaultPath)}</div>
      <button id="choose-folder">Choose…</button>
    </div>
    <div class="hint">Point this at a synced folder (Dropbox, Syncthing, etc.) so the encrypted vault file backs up to your other devices automatically.</div>
    <div class="auth-actions">
      <button id="import-btn">Import vault file…</button>
      <button id="scan-vault-btn">Scan vault QR code…</button>
      <button class="primary" id="continue-btn">Continue</button>
    </div>
    <div class="hint">Already set up on another device? Import the vault file you exported there (from a synced folder, Downloads, etc.) or scan its QR code, then link this device.</div>
    <div id="autodetect-area" style="margin-top:14px"></div>
    ${state.error ? `<div class="error-box">${escapeHtml(state.error)}</div>` : ""}
  `;
  app.appendChild(centered(card));

  initAutoDetect(card.querySelector("#autodetect-area"), (status) => {
    setState({
      vaultPath: status.vault_path,
      screen: status.device_linked ? "unlock" : "link",
      error: "",
    });
    showToast("Vault imported");
  });

  card.querySelector("#choose-folder").onclick = async () => {
    try {
      const picked = await invoke("choose_vault_folder");
      if (picked) setState({ vaultPath: picked, error: "" });
    } catch (e) {
      setState({ error: String(e) });
    }
  };

  card.querySelector("#import-btn").onclick = async () => {
    try {
      const status = await invoke("import_vault");
      if (status) {
        setState({
          vaultPath: status.vault_path,
          screen: status.device_linked ? "unlock" : "link",
          error: "",
        });
        showToast("Vault imported");
      }
    } catch (e) {
      setState({ error: String(e) });
    }
  };

  card.querySelector("#scan-vault-btn").onclick = async () => {
    try {
      const content = await scanQrRaw("Point the camera at the vault QR code");
      const status = await invoke("import_vault_qr", { content });
      setState({
        vaultPath: status.vault_path,
        screen: status.device_linked ? "unlock" : "link",
        error: "",
      });
      showToast("Vault imported");
    } catch (e) {
      setState({ error: String(e) });
    }
  };

  card.querySelector("#continue-btn").onclick = async () => {
    const pw1 = card.querySelector("#pw1").value;
    const pw2 = card.querySelector("#pw2").value;
    if (pw1.length < 8) {
      setState({ error: "Password must be at least 8 characters." });
      return;
    }
    if (pw1 !== pw2) {
      setState({ error: "Passwords don't match." });
      return;
    }
    try {
      const resp = await invoke("start_setup", { password: pw1 });
      setState({ screen: "setup-confirm", setupPending: resp, error: "" });
    } catch (e) {
      setState({ error: String(e) });
    }
  };
}

// ---------- Screen: Setup step 2 - scan QR + confirm code ----------

function renderSetupConfirm() {
  app.innerHTML = "";
  const p = state.setupPending;
  const card = el("div", "auth-card");
  card.innerHTML = `
    <h1>📱 Scan with Google Authenticator</h1>
    <div class="subtitle">Add PassVault to your authenticator app, then enter the 6-digit code to confirm.</div>
    <div class="qr-box">${p.qr_svg}</div>
    <div class="hint">Can't scan? Enter this key manually:</div>
    <div class="secret-box">${formatSecret(p.secret_b32)}</div>
    <div class="hint"><strong>Save this key somewhere safe</strong> (password manager, printed backup). You'll need it to link your other devices — Google Authenticator only knows it after you scan it there too.</div>
    <label>6-digit code</label>
    <input type="text" id="code" inputmode="numeric" maxlength="6" autocomplete="one-time-code" placeholder="000000" />
    <div class="auth-actions">
      <button id="back-btn">Back</button>
      <button class="primary" id="confirm-btn">Confirm & create vault</button>
    </div>
    ${state.error ? `<div class="error-box">${escapeHtml(state.error)}</div>` : ""}
  `;
  app.appendChild(centered(card));

  const codeInput = card.querySelector("#code");
  codeInput.focus();
  codeInput.onkeydown = (e) => {
    if (e.key === "Enter") card.querySelector("#confirm-btn").click();
  };

  card.querySelector("#back-btn").onclick = () => {
    setState({ screen: "setup", setupPending: null, error: "" });
  };

  card.querySelector("#confirm-btn").onclick = async (e) => {
    try {
      await withLoading(e.target, "Creating vault…", async () => {
        const accounts = await invoke("confirm_setup", { code: codeInput.value.trim() });
        setState({ screen: "main", accounts, setupPending: null, error: "" });
      });
    } catch (e) {
      setState({ error: String(e) });
    }
  };
}

// ---------- Screen: Link this device ----------

function renderLink() {
  app.innerHTML = "";
  const card = el("div", "auth-card");
  card.innerHTML = `
    <h1>🔗 Link this device</h1>
    <div class="subtitle">A vault was found at this location but this device hasn't been linked yet.</div>
    <div class="folder-row"><div class="path">${escapeHtml(state.vaultPath)}</div></div>
    <label>Master password</label>
    <input type="password" id="pw" autocomplete="current-password" />
    <label>Authenticator secret key</label>
    <div class="field-with-action">
      <input type="text" id="secret" placeholder="Base32 key from your original setup" autocomplete="off" />
      <button id="scan-btn">Scan QR…</button>
    </div>
    <div class="hint">This is the key you saved when you first set up PassVault. Scan the QR code from Settings on an already-linked device, add it to Google Authenticator on this device too, then enter the current code below.</div>
    <label>6-digit code</label>
    <input type="text" id="code" inputmode="numeric" maxlength="6" placeholder="000000" />
    <div class="auth-actions">
      <button id="import-btn">Import a different vault file…</button>
      <button id="scan-vault-btn">Scan vault QR code…</button>
      <button class="primary" id="link-btn">Link device</button>
    </div>
    ${state.error ? `<div class="error-box">${escapeHtml(state.error)}</div>` : ""}
  `;
  app.appendChild(centered(card));

  card.querySelector("#scan-btn").onclick = async () => {
    try {
      const secret = await scanQrForSecret();
      if (secret) {
        card.querySelector("#secret").value = secret;
        showToast("Scanned");
      } else {
        showToast("That QR code doesn't look like a PassVault key");
      }
    } catch (e) {
      showToast(String(e));
    }
  };

  card.querySelector("#import-btn").onclick = async () => {
    try {
      const status = await invoke("import_vault");
      if (status) {
        setState({ vaultPath: status.vault_path, error: "" });
        showToast("Vault imported");
      }
    } catch (e) {
      setState({ error: String(e) });
    }
  };

  card.querySelector("#scan-vault-btn").onclick = async () => {
    try {
      const content = await scanQrRaw("Point the camera at the vault QR code");
      const status = await invoke("import_vault_qr", { content });
      setState({ vaultPath: status.vault_path, error: "" });
      showToast("Vault imported");
    } catch (e) {
      setState({ error: String(e) });
    }
  };

  card.querySelector("#link-btn").onclick = async (e) => {
    const password = card.querySelector("#pw").value;
    const secret_b32 = card.querySelector("#secret").value;
    const code = card.querySelector("#code").value.trim();
    try {
      await withLoading(e.target, "Linking…", async () => {
        const accounts = await invoke("link_device", { password, secretB32: secret_b32, code });
        setState({ screen: "main", accounts, error: "" });
      });
    } catch (e) {
      setState({ error: String(e) });
    }
  };

  card.querySelector("#change-folder-btn").onclick = () => {
    setState({
      error: "This vault already exists — move or delete vault.enc first if you meant to start fresh.",
    });
  };
}

// ---------- Screen: Unlock ----------

// Whether to show the password+code form instead of the biometric-first
// blank screen. Reset to false each time we land back on "unlock" (fresh
// app open, or auto-lock-on-resume) so fingerprint is always tried first.
let unlockUsePasswordForm = false;

function renderUnlock() {
  app.innerHTML = "";
  const container = el("div");
  app.appendChild(container);

  invoke("biometric_status")
    .then((status) => {
      if (status.available && status.has_saved_secret && !unlockUsePasswordForm) {
        renderBiometricUnlock(container, status);
      } else {
        renderPasswordUnlock(container, status);
      }
    })
    .catch(() => renderPasswordUnlock(container, { available: false }));
}

function renderBiometricUnlock(container) {
  const card = el("div", "auth-card");
  card.style.textAlign = "center";
  card.innerHTML = `
    <h1>🔐 PassVault</h1>
    <div class="subtitle" id="bio-status">Waiting for fingerprint…</div>
    <div class="auth-actions" style="justify-content:center; margin-top:20px">
      <button id="retry-fp-btn" style="display:none">👆 Try fingerprint again</button>
      <button id="use-password-btn">Use password instead</button>
    </div>
  `;
  container.innerHTML = "";
  container.appendChild(centered(card));

  const statusEl = card.querySelector("#bio-status");
  const retryBtn = card.querySelector("#retry-fp-btn");

  const attempt = async () => {
    statusEl.textContent = "Waiting for fingerprint…";
    retryBtn.style.display = "none";
    try {
      const accounts = await invoke("biometric_unlock");
      unlockUsePasswordForm = false;
      setState({ screen: "main", accounts, error: "" });
    } catch (e) {
      statusEl.textContent = String(e);
      retryBtn.style.display = "";
    }
  };

  retryBtn.onclick = attempt;
  card.querySelector("#use-password-btn").onclick = () => {
    unlockUsePasswordForm = true;
    renderUnlock();
  };

  attempt();
}

function renderPasswordUnlock(container, biometricStatus) {
  const card = el("div", "auth-card");
  card.innerHTML = `
    <h1>🔐 PassVault</h1>
    <div class="subtitle">Enter your master password and authenticator code.</div>
    <label>Master password</label>
    <input type="password" id="pw" autocomplete="current-password" />
    <label>6-digit code</label>
    <input type="text" id="code" inputmode="numeric" maxlength="6" placeholder="000000" autocomplete="one-time-code" />
    <div class="auth-actions">
      <button class="primary" id="unlock-btn">Unlock</button>
    </div>
    ${
      biometricStatus?.available && biometricStatus?.has_saved_secret
        ? `<div class="hint" style="text-align:center; margin-top:12px"><a href="#" id="use-fp-link">Use fingerprint instead</a></div>`
        : ""
    }
    ${state.error ? `<div class="error-box">${escapeHtml(state.error)}</div>` : ""}
  `;
  container.innerHTML = "";
  container.appendChild(centered(card));

  const pw = card.querySelector("#pw");
  const code = card.querySelector("#code");
  pw.focus();
  const unlockBtn = card.querySelector("#unlock-btn");

  const doUnlock = async (password) => {
    try {
      await withLoading(unlockBtn, "Unlocking…", async () => {
        const accounts = await invoke("unlock", { password, code: code.value.trim() });
        setState({ screen: "main", accounts, error: "" });
      });
    } catch (e) {
      setState({ error: String(e) });
    }
  };

  card.querySelector("#use-fp-link")?.addEventListener("click", (e) => {
    e.preventDefault();
    unlockUsePasswordForm = false;
    renderUnlock();
  });

  code.onkeydown = (e) => {
    if (e.key === "Enter") doUnlock(pw.value);
  };
  unlockBtn.onclick = async () => {
    const password = pw.value;
    await doUnlock(password);
    if (state.screen === "main") {
      const status = await invoke("biometric_status").catch(() => null);
      if (status?.available && !status.has_saved_secret) {
        if (confirm("Enable fingerprint unlock for next time?")) {
          try {
            await invoke("enable_biometric_unlock", { password });
            showToast("Fingerprint unlock enabled");
          } catch (e) {
            showToast(String(e));
          }
        }
      }
    }
  };
}

// ---------- Screen: Main app ----------

function renderMain() {
  app.innerHTML = "";
  const shell = el("div", "app-shell");

  // Sidebar
  const sidebar = el("div", "sidebar");
  sidebar.innerHTML = `
    <h1>🔐 PassVault</h1>
    <div class="search-box"><input id="search" placeholder="Search accounts…" /></div>
    <div class="account-list" id="account-list"></div>
    <div class="sidebar-footer">
      <button id="add-btn" class="primary">+ Add account</button>
      <button id="settings-btn">Settings</button>
      <button id="lock-btn">Lock vault</button>
    </div>
  `;
  shell.appendChild(sidebar);

  const main = el("div", "main-panel");
  shell.appendChild(main);
  app.appendChild(shell);

  const search = sidebar.querySelector("#search");
  const list = sidebar.querySelector("#account-list");

  function renderList() {
    const q = search.value.trim().toLowerCase();
    const filtered = state.accounts
      .filter(
        (a) =>
          !q ||
          a.name.toLowerCase().includes(q) ||
          a.username.toLowerCase().includes(q)
      )
      .sort((a, b) => a.name.localeCompare(b.name));

    list.innerHTML = "";
    if (filtered.length === 0) {
      list.innerHTML = `<div class="hint" style="padding:10px">No accounts yet.</div>`;
    }
    for (const acc of filtered) {
      const item = el("div", "account-item" + (acc.id === state.selectedId ? " active" : ""));
      item.innerHTML = `<div class="name">${escapeHtml(acc.name)}</div><div class="user">${escapeHtml(acc.username)}</div>`;
      item.onclick = () => setState({ selectedId: acc.id, editing: false });
      list.appendChild(item);
    }
  }
  renderList();
  search.oninput = renderList;

  sidebar.querySelector("#add-btn").onclick = () => {
    setState({ selectedId: "__new__", editing: true });
  };
  sidebar.querySelector("#settings-btn").onclick = () => {
    setState({ selectedId: "__settings__", editing: false });
  };
  sidebar.querySelector("#lock-btn").onclick = async () => {
    await invoke("lock");
    unlockUsePasswordForm = false;
    setState({ screen: "unlock", accounts: [], selectedId: null, error: "" });
  };

  // Main panel content
  shell.classList.toggle("detail-open", !!state.selectedId);

  if (state.selectedId === "__settings__") {
    renderSettings(main);
  } else if (state.selectedId === "__new__") {
    renderAccountForm(main, null);
  } else if (state.selectedId) {
    const acc = state.accounts.find((a) => a.id === state.selectedId);
    if (!acc) {
      renderEmpty(main);
    } else if (state.editing) {
      renderAccountForm(main, acc);
    } else {
      renderAccountDetail(main, acc);
    }
  } else {
    renderEmpty(main);
  }

  // Mobile-only: a way back to the sidebar list (hidden on wide screens by CSS).
  if (state.selectedId) {
    const backBtn = el("button", "mobile-back-btn");
    backBtn.textContent = "‹ Back to accounts";
    backBtn.onclick = () => setState({ selectedId: null, editing: false });
    main.insertBefore(backBtn, main.firstChild);
  }
}

function renderEmpty(main) {
  main.innerHTML = "";
  const box = el("div", "empty-state");
  box.innerHTML = `<div style="font-size:32px">🔒</div><div>Select an account, or add a new one.</div>`;
  main.appendChild(box);
}

function renderAccountDetail(main, acc) {
  main.innerHTML = "";
  const wrap = el("div");
  wrap.innerHTML = `
    <div class="detail-header">
      <h2>${escapeHtml(acc.name)}</h2>
      <div class="detail-actions">
        <button id="edit-btn">Edit</button>
        <button class="danger" id="delete-btn">Delete</button>
      </div>
    </div>
    <div class="form-grid">
      <label>Username / email</label>
      <div class="field-with-action">
        <input readonly value="${escapeHtml(acc.username)}" />
        <button data-copy="${escapeAttr(acc.username)}">Copy</button>
      </div>
      <label>Password</label>
      <div class="field-with-action">
        <input readonly type="password" value="${escapeHtml(acc.password)}" id="pw-field" />
        <button id="toggle-pw">Show</button>
        <button data-copy="${escapeAttr(acc.password)}">Copy</button>
      </div>
      <label>URL</label>
      <div class="field-with-action">
        <input readonly value="${escapeHtml(acc.url)}" />
      </div>
      <label>Notes</label>
      <textarea readonly rows="4">${escapeHtml(acc.notes)}</textarea>
    </div>
  `;
  main.appendChild(wrap);

  wrap.querySelectorAll("[data-copy]").forEach((btn) => {
    btn.onclick = () => copyToClipboard(btn.getAttribute("data-copy"));
  });
  const pwField = wrap.querySelector("#pw-field");
  wrap.querySelector("#toggle-pw").onclick = (e) => {
    const showing = pwField.type === "text";
    pwField.type = showing ? "password" : "text";
    e.target.textContent = showing ? "Show" : "Hide";
  };
  wrap.querySelector("#edit-btn").onclick = () => setState({ editing: true });
  wrap.querySelector("#delete-btn").onclick = async () => {
    if (!confirm(`Delete "${acc.name}"? This can't be undone.`)) return;
    const accounts = await invoke("delete_account", { id: acc.id });
    setState({ accounts, selectedId: null, editing: false });
  };
}

function renderAccountForm(main, acc) {
  main.innerHTML = "";
  const isNew = !acc;
  const wrap = el("div");
  wrap.innerHTML = `
    <div class="detail-header"><h2>${isNew ? "Add account" : "Edit account"}</h2></div>
    <div class="form-grid">
      <label>Name</label>
      <input id="f-name" value="${escapeAttr(acc?.name ?? "")}" placeholder="e.g. GitHub" />
      <label>Username / email</label>
      <input id="f-username" value="${escapeAttr(acc?.username ?? "")}" />
      <label>Password</label>
      <div class="field-with-action">
        <input id="f-password" value="${escapeAttr(acc?.password ?? "")}" />
        <button id="gen-btn">Generate</button>
      </div>
      <label>URL</label>
      <input id="f-url" value="${escapeAttr(acc?.url ?? "")}" placeholder="https://" />
      <label>Notes</label>
      <textarea id="f-notes" rows="4">${escapeHtml(acc?.notes ?? "")}</textarea>
      <div class="auth-actions">
        <button id="cancel-btn">Cancel</button>
        <button class="primary" id="save-btn">Save</button>
      </div>
    </div>
  `;
  main.appendChild(wrap);

  wrap.querySelector("#gen-btn").onclick = () => {
    wrap.querySelector("#f-password").value = generatePassword();
  };
  wrap.querySelector("#cancel-btn").onclick = () => {
    setState(isNew ? { selectedId: null, editing: false } : { editing: false });
  };
  wrap.querySelector("#save-btn").onclick = async () => {
    const payload = {
      name: wrap.querySelector("#f-name").value.trim(),
      username: wrap.querySelector("#f-username").value,
      password: wrap.querySelector("#f-password").value,
      url: wrap.querySelector("#f-url").value,
      notes: wrap.querySelector("#f-notes").value,
    };
    if (!payload.name) {
      alert("Name is required.");
      return;
    }
    let accounts;
    if (isNew) {
      accounts = await invoke("add_account", payload);
      const created = accounts.find(
        (a) => a.name === payload.name && a.username === payload.username
      );
      setState({ accounts, selectedId: created?.id ?? null, editing: false });
    } else {
      accounts = await invoke("update_account", { id: acc.id, ...payload });
      setState({ accounts, editing: false });
    }
  };
}

function renderSettings(main) {
  main.innerHTML = "";
  const wrap = el("div", "settings-panel");
  wrap.innerHTML = `
    <div class="detail-header"><h2>Settings</h2></div>
    <div class="settings-row">
      <div class="label">Vault file location</div>
      <div class="folder-row"><div class="path">${escapeHtml(state.vaultPath)}</div></div>
      <div class="hint">This is the file to keep in sync (Dropbox, Syncthing, USB drive, etc.) for backup and to use PassVault on another device. Copy it there, then run PassVault and choose "Link this device" with your saved authenticator key.</div>
      <div style="margin-top:10px"><button id="reveal-btn">Show in file manager</button></div>
    </div>
    <div class="settings-row">
      <div class="label">Export vault</div>
      <div class="hint">Save a copy of the encrypted vault file through the share/save sheet — send it to Syncthing, a cloud drive, or anywhere else you sync from. It stays encrypted with your master password the whole time.</div>
      <div style="margin-top:10px"><button id="export-btn">Export vault…</button></div>
    </div>
    <div class="settings-row">
      <div class="label">Sync via QR code</div>
      <div class="hint">Quick device-to-device sync with no cloud needed. Only works while the vault is small enough to fit in a QR code (a handful of accounts); for a bigger vault, use Export instead.</div>
      <div id="qr-export-area" style="margin-top:10px"><button id="export-qr-btn">Show QR code to share…</button></div>
      <div class="hint" style="margin-top:12px">Pulling in a newer vault from another device? Scanning replaces everything below — accounts added only on this device since the last sync will be lost.</div>
      <div style="margin-top:8px"><button id="scan-qr-btn">Scan a vault QR code…</button></div>
    </div>
    <div class="settings-row">
      <div class="label">Adding a new device</div>
      <div class="hint">1. Export the vault here (or make sure it syncs to the new device's folder).<br>2. Install PassVault there and import that vault file (or point it at the synced copy).<br>3. Enter your master password and the authenticator secret key you saved during setup, plus a current code, to link it.</div>
    </div>
    <div class="settings-row">
      <div class="label">Lost your authenticator secret key?</div>
      <div class="hint">Re-enter your master password to view it again on this already-linked device, so you can link another one.</div>
      <div id="secret-area" style="margin-top:10px">
        <input type="password" id="secret-pw" placeholder="Master password" autocomplete="current-password" style="max-width:260px" />
        <button id="reveal-secret-btn">Reveal</button>
      </div>
    </div>
    <div class="settings-row" id="biometric-row" style="display:none">
      <div class="label">Fingerprint unlock</div>
      <div class="hint">Skip typing your master password on this device — you'll still need the authenticator code.</div>
      <div style="margin-top:10px"><button id="biometric-toggle-btn"></button></div>
    </div>
  `;
  main.appendChild(wrap);

  invoke("biometric_status")
    .then((status) => {
      if (!status.available) return;
      const row = wrap.querySelector("#biometric-row");
      row.style.display = "";
      const btn = wrap.querySelector("#biometric-toggle-btn");
      btn.textContent = status.has_saved_secret ? "Disable fingerprint unlock" : "Enable fingerprint unlock";
      btn.onclick = async () => {
        if (status.has_saved_secret) {
          await invoke("disable_biometric_unlock");
          showToast("Fingerprint unlock disabled");
          renderSettings(main);
        } else {
          const password = prompt("Enter your master password to enable fingerprint unlock:");
          if (!password) return;
          try {
            await invoke("enable_biometric_unlock", { password });
            showToast("Fingerprint unlock enabled");
            renderSettings(main);
          } catch (e) {
            showToast(String(e));
          }
        }
      };
    })
    .catch(() => {});

  wrap.querySelector("#reveal-btn").onclick = async () => {
    try {
      await invoke("reveal_vault_folder");
    } catch (e) {
      showToast(String(e));
    }
  };
  wrap.querySelector("#reveal-secret-btn").onclick = async () => {
    const password = wrap.querySelector("#secret-pw").value;
    try {
      const resp = await invoke("reveal_totp_secret", { password });
      const area = wrap.querySelector("#secret-area");
      area.innerHTML = `
        <div class="qr-box">${resp.qr_svg}</div>
        <div class="hint">Scan this on the new device, or enter the key manually:</div>
        <div class="secret-box">${formatSecret(resp.secret_b32)}</div>
      `;
    } catch (e) {
      showToast(String(e));
    }
  };
  wrap.querySelector("#export-btn").onclick = async () => {
    try {
      const exported = await invoke("export_vault");
      if (exported) showToast("Vault exported");
    } catch (e) {
      showToast(String(e));
    }
  };

  wrap.querySelector("#export-qr-btn").onclick = async () => {
    try {
      const svg = await invoke("export_vault_qr");
      wrap.querySelector("#qr-export-area").innerHTML = `
        <div class="qr-box">${svg}</div>
        <div class="hint">Scan this from the other device.</div>
      `;
    } catch (e) {
      showToast(String(e));
    }
  };

  wrap.querySelector("#scan-qr-btn").onclick = async () => {
    if (!confirm("This replaces the vault on this device with the one you scan. Continue?")) {
      return;
    }
    try {
      const content = await scanQrRaw("Point the camera at the vault QR code");
      await invoke("import_vault_qr", { content });
      showToast("Vault synced — unlock again to see the update");
      unlockUsePasswordForm = false;
      setState({ screen: "unlock", accounts: [], selectedId: null, error: "" });
    } catch (e) {
      showToast(String(e));
    }
  };
}

// ---------- helpers ----------

function el(tag, className) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  return e;
}

function centered(child) {
  const wrap = el("div", "centered-screen");
  wrap.appendChild(child);
  return wrap;
}

function escapeHtml(s) {
  return String(s ?? "").replace(/[&<>"']/g, (c) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  }[c]));
}

function escapeAttr(s) {
  return escapeHtml(s);
}

function formatSecret(secret) {
  return secret.replace(/(.{4})/g, "$1 ").trim();
}

async function initAutoDetect(area, onImported) {
  const renderGrantPrompt = () => {
    area.innerHTML = `
      <div class="hint">Let PassVault check your Downloads/Documents folders for a vault file exported from another device.</div>
      <div style="margin-top:8px">
        <button id="grant-access-btn">Look for existing vaults…</button>
      </div>
    `;
    area.querySelector("#grant-access-btn").onclick = async () => {
      await invoke("request_full_file_access");
      area.innerHTML = `
        <div class="hint">Grant "All files access" in the settings screen that just opened, then come back here.</div>
        <div style="margin-top:8px"><button id="recheck-btn">I granted it — check again</button></div>
      `;
      area.querySelector("#recheck-btn").onclick = () => initAutoDetect(area, onImported);
    };
  };

  const renderResults = (paths) => {
    if (paths.length === 0) {
      area.innerHTML = "";
      return;
    }
    area.innerHTML = `
      <div class="hint">Found ${paths.length === 1 ? "a possible vault file" : "possible vault files"}:</div>
      <div id="found-list" style="margin-top:8px"></div>
    `;
    const list = area.querySelector("#found-list");
    for (const path of paths) {
      const btn = el("button");
      btn.textContent = path.split("/").pop();
      btn.title = path;
      btn.style.display = "block";
      btn.style.marginBottom = "6px";
      btn.onclick = async () => {
        try {
          const status = await invoke("import_vault_from_path", { path });
          onImported(status);
        } catch (e) {
          showToast(String(e));
        }
      };
      list.appendChild(btn);
    }
  };

  try {
    const granted = await invoke("has_full_file_access");
    if (!granted) {
      renderGrantPrompt();
      return;
    }
    const paths = await invoke("scan_for_vault_files");
    renderResults(paths);
  } catch {
    area.innerHTML = "";
  }
}

async function scanQrRaw(hintText) {
  const perm = await invoke("plugin:barcode-scanner|request_permissions");
  if (perm.camera !== "granted") {
    throw new Error("Camera permission denied");
  }

  // windowed:true keeps the camera preview behind a transparent webview
  // instead of a separate native view — the non-windowed mode leaves
  // Android's WebView surface stuck not-repainting after the native view
  // is torn down, which looks like a black/frozen screen.
  const appEl = document.getElementById("app");
  const overlay = el("div", "qr-scan-overlay");
  overlay.innerHTML = `
    <div class="qr-scan-frame"></div>
    <div class="qr-scan-hint">${escapeHtml(hintText || "Point the camera at the QR code")}</div>
    <button class="qr-scan-cancel">Cancel</button>
  `;
  document.body.appendChild(overlay);
  document.documentElement.classList.add("qr-scanning");
  appEl.style.display = "none";

  let cancelled = false;
  const cleanup = () => {
    document.documentElement.classList.remove("qr-scanning");
    appEl.style.display = "";
    overlay.remove();
  };
  overlay.querySelector(".qr-scan-cancel").onclick = () => {
    cancelled = true;
    cleanup();
    invoke("plugin:barcode-scanner|cancel").catch(() => {});
  };

  try {
    const result = await invoke("plugin:barcode-scanner|scan", {
      windowed: true,
      formats: ["QR_CODE"],
      cameraDirection: "back",
    });
    if (!cancelled) cleanup();
    return result.content;
  } catch (e) {
    if (!cancelled) cleanup();
    throw e;
  }
}

async function scanQrForSecret() {
  return extractSecretFromScan(await scanQrRaw());
}

function extractSecretFromScan(content) {
  if (content.startsWith("otpauth://")) {
    try {
      const secret = new URL(content).searchParams.get("secret");
      if (secret) return secret;
    } catch {
      // fall through to raw-secret handling below
    }
  }
  const cleaned = content.replace(/\s+/g, "").toUpperCase();
  return /^[A-Z2-7]+=*$/.test(cleaned) ? cleaned : null;
}

function generatePassword(length = 20) {
  const chars =
    "abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%^&*()-_=+";
  const bytes = new Uint32Array(length);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => chars[b % chars.length]).join("");
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
      app.innerHTML = `<div class="centered-screen">Loading…</div>`;
  }
}

// The Rust side auto-locks (clears the decryption key) when the app is
// backgrounded on mobile. Re-check status whenever we come back to the
// foreground so a stale "main" screen doesn't sit on top of a now-locked
// vault silently failing every action.
document.addEventListener("visibilitychange", async () => {
  if (document.visibilityState !== "visible") return;
  if (state.screen !== "main") return;
  const status = await invoke("app_status");
  if (!status.unlocked) {
    unlockUsePasswordForm = false;
    setState({ screen: "unlock", accounts: [], selectedId: null, error: "" });
  }
});

render();
boot();
