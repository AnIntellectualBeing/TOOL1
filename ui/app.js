/* PhoneMirror UI. Talks to the Rust side through Tauri's invoke/events. */
(function () {
  "use strict";

  const T = window.__TAURI__;
  const invoke = (cmd, args) => T.core.invoke(cmd, args || {});
  const listen = (ev, fn) => T.event.listen(ev, (e) => fn(e.payload));
  const $ = (id) => document.getElementById(id);
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

  // ---------- Icons ----------
  const ICONS = {
    apple: '<path d="M15.5 4.5c-.9.1-2 .7-2.6 1.5-.6.7-1 1.7-.9 2.6 1 .1 2-.5 2.6-1.3.6-.8 1-1.8.9-2.8zM18.6 15.6c-.4 1-1 2-1.7 2.9-.6.7-1.3 1.5-2.2 1.5s-1.2-.5-2.3-.5-1.4.5-2.3.5-1.5-.8-2.2-1.6C6.4 16.7 5.3 14 6 11.8c.4-1.3 1.6-2.3 3-2.3.9 0 1.7.6 2.3.6.6 0 1.6-.7 2.7-.6.5 0 1.8.2 2.6 1.4-2.3 1.4-1.9 4.6.6 5.7z" fill="currentColor"/>',
    android: '<path d="M7 10h10v7a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1zM7.5 9a4.5 4.5 0 0 1 9 0z" fill="currentColor"/><path d="M8 4.5l1.2 2M16 4.5l-1.2 2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>',
    gear: '<circle cx="12" cy="12" r="3" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M12 3v2.2M12 18.8V21M3 12h2.2M18.8 12H21M5.6 5.6l1.6 1.6M16.8 16.8l1.6 1.6M5.6 18.4l1.6-1.6M16.8 7.2l1.6-1.6" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>',
    help: '<circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1 .8-1 1.5v.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="12" cy="17" r="1" fill="currentColor"/>',
    fullscreen: '<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>',
    pin: '<path d="M9 4h6l-1 6 3 3H7l3-3zM12 13v7" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>',
    camera: '<path d="M4 8h3l2-2h6l2 2h3v11H4z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/><circle cx="12" cy="13" r="3.5" fill="none" stroke="currentColor" stroke-width="1.8"/>',
    record: '<circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.8"/><circle cx="12" cy="12" r="4" fill="currentColor"/>',
    rotate: '<path d="M20 12a8 8 0 1 1-2.3-5.7M20 4v5h-5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>',
    sound: '<path d="M4 9h4l5-4v14l-5-4H4z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/><path d="M16 9a4 4 0 0 1 0 6M18.5 6.5a7.5 7.5 0 0 1 0 11" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>',
    muted: '<path d="M4 9h4l5-4v14l-5-4H4z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/><path d="M16 9l5 6M21 9l-5 6" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>',
    quality: '<path d="M4 18h16M7 14v4M12 10v8M17 6v12" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>',
  };
  const icon = (n) => `<svg viewBox="0 0 24 24" aria-hidden="true">${ICONS[n] || ""}</svg>`;
  document.querySelectorAll("[data-icon]").forEach((el) => (el.innerHTML = icon(el.dataset.icon)));

  const QUALITY = [
    ["auto", "Automatic"],
    ["smooth", "Smooth (lowest delay)"],
    ["balanced", "Balanced"],
    ["sharp", "Sharp (best over USB)"],
    ["battery_saver", "Battery saver"],
  ];

  // ---------- State ----------
  let info = { receiver_name: "this PC", os: "windows" };
  let settings = {};
  let iphone = { state: "starting" };
  let android = { state: "idle" };
  let overview = { connected: [], remembered: [] };
  let view = "iphone";

  // ---------- Helpers ----------
  function problemHtml(p, cls, actions) {
    if (!p) return "";
    return `<div class="problem ${cls || ""}">
      <div class="p-msg">${esc(p.message)}</div>
      ${p.hint ? `<div class="p-hint">${esc(p.hint)}</div>` : ""}
      ${actions ? `<div class="p-actions">${actions}</div>` : ""}
      ${p.details ? `<details><summary>Details</summary><pre>${esc(p.details)}</pre></details>` : ""}
    </div>`;
  }

  function toast(kind, message, path) {
    const t = document.createElement("div");
    t.className = `toast ${kind || "info"}`;
    t.innerHTML = `<span>${esc(message)}</span>${path ? '<button class="btn small">Show</button>' : ""}`;
    if (path) t.querySelector("button").onclick = () => invoke("reveal", { path }).catch(showError);
    $("toasts").appendChild(t);
    setTimeout(() => t.remove(), kind === "error" ? 9000 : 6000);
  }

  function showError(e) {
    const p = typeof e === "object" && e ? e : { message: String(e) };
    toast("error", p.hint ? `${p.message} ${p.hint}` : p.message);
  }

  async function call(cmd, args) {
    try {
      return await invoke(cmd, args);
    } catch (e) {
      showError(e);
      throw e;
    }
  }

  function setPill(el, text, kind) {
    el.textContent = text;
    el.className = `pill ${kind || ""}`;
  }

  function fillRname() {
    document.querySelectorAll(".rname").forEach((el) => (el.textContent = info.receiver_name));
  }

  // ---------- Navigation ----------
  document.querySelectorAll(".nav-item").forEach((b) =>
    b.addEventListener("click", () => {
      view = b.dataset.view;
      document.querySelectorAll(".nav-item").forEach((x) => x.classList.toggle("active", x === b));
      document.querySelectorAll(".view").forEach((v) => v.classList.toggle("active", v.id === `view-${view}`));
      if (view === "android") refreshDevices();
      if (view === "settings") refreshFirewall();
    })
  );

  // ---------- Toolbar ----------
  function toolbar(target, s) {
    const btn = (action, label, ic, pressed, extra = "") =>
      `<button class="tb ${extra}" data-action="${action}" ${pressed !== undefined ? `aria-pressed="${!!pressed}"` : ""}>${icon(ic)}${label}</button>`;
    const q = target === "iphone" ? settings.iphone_quality : s.quality || settings.android_quality;
    const opts = QUALITY.map(([v, l]) => `<option value="${v}" ${v === q ? "selected" : ""}>${l}</option>`).join("");
    const qsel = `<label class="tb">${icon("quality")}<select data-action="quality" aria-label="Quality">${opts}</select></label>`;
    if (target === "iphone" && s.state !== "connected") {
      // Before an iPhone connects only the "next session" options make sense.
      return [
        btn("record", s.record_next ? "Will record next session" : "Record next session", "record", !!s.record_next, "rec"),
        btn("rotate", "Rotate", "rotate"),
        qsel,
      ].join("");
    }
    return [
      btn("fullscreen", "Fullscreen", "fullscreen", s.fullscreen),
      btn("topmost", "Always on top", "pin", s.topmost),
      btn("screenshot", "Screenshot", "camera"),
      btn("record", s.recording ? "Stop recording" : target === "iphone" && s.record_next ? "Will record next session" : "Record", "record", !!(s.recording || s.record_next), "rec"),
      btn("rotate", "Rotate", "rotate"),
      btn("mute", s.muted ? "Sound off" : "Sound on", s.muted ? "muted" : "sound", s.muted),
      qsel,
    ].join("");
  }

  function wireToolbar(el, target) {
    const cmd = target === "iphone" ? "iphone_action" : "android_action";
    el.querySelectorAll("button[data-action]").forEach((b) =>
      b.addEventListener("click", async () => {
        const action = b.dataset.action;
        const pressed = b.getAttribute("aria-pressed") === "true";
        let value = null;
        if (action === "topmost" || action === "mute" || action === "record") value = !pressed;
        b.disabled = true;
        try {
          const s = await call(cmd, { action, value });
          if (target === "iphone") renderIphone(s); else renderAndroid(s);
        } catch (_) { /* toast shown */ } finally { b.disabled = false; }
      })
    );
    const sel = el.querySelector("select[data-action=quality]");
    if (sel) sel.addEventListener("change", async () => {
      try {
        const s = await call(cmd, { action: "quality", value: sel.value });
        settings = await invoke("get_settings");
        if (target === "iphone") renderIphone(s); else renderAndroid(s);
      } catch (_) {}
    });
  }

  // ---------- iPhone ----------
  function renderIphone(s) {
    iphone = s || iphone;
    s = iphone;
    if (s.receiver_name) { info.receiver_name = s.receiver_name; fillRname(); }
    const radar = $("iphone-radar");
    radar.className = "radar";
    const client = s.client ? s.client.name : "iPhone";
    let title = "Waiting for iPhone…", sub = `On your iPhone, choose <strong>${esc(info.receiver_name)}</strong> under Screen Mirroring.`;
    switch (s.state) {
      case "connected":
        radar.classList.add("done");
        title = s.mirroring ? `Showing ${esc(client)}` : `Playing sound from ${esc(client)}`;
        sub = s.mirroring
          ? "The picture is in its own window. To stop, tap Stop Mirroring on the iPhone."
          : "Your PC is the speaker. Choose another output on the iPhone to stop.";
        setPill($("iphone-pill"), s.recording ? "Connected · Recording" : "Connected", "ok");
        break;
      case "waiting":
        setPill($("iphone-pill"), s.record_next ? "Ready · will record" : "Ready", "busy");
        if (s.speaker_only) sub = `Sound-only mode. Choose <strong>${esc(info.receiver_name)}</strong> in the iPhone's AirPlay menu.`;
        break;
      case "starting":
        title = "Starting the iPhone receiver…";
        setPill($("iphone-pill"), "Starting…", "busy");
        break;
      case "off":
        radar.classList.add("error");
        title = "iPhone receiver is off";
        sub = "Turn it on in Settings, or press Start.";
        setPill($("iphone-pill"), "Off", "");
        break;
      case "error":
        radar.classList.add("error");
        title = "iPhones can't connect right now";
        sub = "See below for what to do.";
        setPill($("iphone-pill"), "Problem", "bad");
        break;
    }
    $("iphone-title").innerHTML = title;
    $("iphone-sub").innerHTML = sub;
    $("iphone-pin").hidden = !s.pin;
    $("iphone-pin-code").textContent = s.pin || "";
    const prob = $("iphone-problem");
    if (s.problem) {
      prob.hidden = false;
      const act = ["firewall", "advertise_failed"].includes(s.problem.code) ? '<button class="btn small" data-fix="firewall">Fix firewall</button>' : "";
      const restart = s.state === "error" || s.state === "off" ? '<button class="btn small" data-fix="restart">Restart receiver</button>' : "";
      prob.innerHTML = problemHtml(s.problem, s.state === "error" ? "" : "warn", act + restart);
      prob.querySelectorAll("[data-fix]").forEach((b) => (b.onclick = () => (b.dataset.fix === "firewall" ? fixFirewall(false) : restartReceiver())));
    } else {
      prob.hidden = true;
    }
    const tb = $("iphone-toolbar");
    const showTb = s.state === "connected" || s.state === "waiting";
    tb.hidden = !showTb;
    if (showTb) { tb.innerHTML = toolbar("iphone", s); wireToolbar(tb, "iphone"); }
    $("iphone-pending").hidden = !s.pending_restart;
    $("speaker-only").checked = !!s.speaker_only;
    $("dot-iphone").className = `dot ${s.state === "connected" ? "on" : s.state === "error" ? "err" : ""}`;
  }

  async function restartReceiver() {
    try { renderIphone(await call("iphone_action", { action: "restart", value: null })); } catch (_) {}
  }

  $("speaker-only").addEventListener("change", async (e) => {
    try { renderIphone(await call("iphone_action", { action: "speaker_only", value: e.target.checked })); } catch (_) {}
  });
  $("video-casting").addEventListener("change", (e) => saveSetting({ iphone_video_casting: e.target.checked }));

  // ---------- Firewall ----------
  let firewall = null;
  async function refreshFirewall() {
    try { firewall = await invoke("firewall_status"); } catch (_) { firewall = null; }
    renderFirewall();
  }

  function renderFirewall() {
    const f = firewall;
    const card = $("firewall-card"), body = $("firewall-body"), sbody = $("settings-firewall-body");
    if (!f || !f.supported) {
      card.hidden = true;
      sbody.innerHTML = `<p class="muted">PhoneMirror manages Windows Firewall only on Windows. Elsewhere, allow UDP 5353 and TCP/UDP 7100–7102 for uxplay if you use a firewall.</p>`;
      return;
    }
    const ok = f.receiver_allowed && !f.blocked;
    let html = "";
    if (f.blocked) {
      html += problemHtml({ message: "Windows Firewall is blocking the iPhone receiver.", hint: "This usually happens after pressing Cancel on Windows' firewall question. Press Fix firewall to undo it." });
    } else if (f.on_public_network && !f.allowed_on_public) {
      html += problemHtml({ message: "Windows treats this Wi-Fi as a public network, so it hides the PC from iPhones.", hint: "If this is your home Wi-Fi, set it to Private in Windows Settings → Network & internet → Wi-Fi → your network. Or allow PhoneMirror on this network." }, "warn");
    } else if (!f.receiver_allowed) {
      html += problemHtml({ message: "Windows Firewall may block iPhones from reaching this PC.", hint: "Press Fix firewall. Windows asks for permission once." }, "warn");
    } else {
      html += `<p>✅ Firewall is set up: iPhones and Android phones on your network can reach PhoneMirror.</p>`;
    }
    html += `<p class="muted">What this allows: iPhones find the PC using mDNS (UDP 5353), then send picture and sound to the receiver on TCP/UDP ports 7100–7102. Only devices on your local network can use these; nothing is opened to the internet.</p>`;
    html += `<div class="row"><button class="btn" data-fw="fix">Fix firewall</button>${f.on_public_network && !f.allowed_on_public ? '<button class="btn ghost" data-fw="public">Allow on this network</button>' : ""}</div>`;
    if (f.networks && f.networks.length) html += `<p class="muted">Networks: ${f.networks.map(esc).join(" · ")}</p>`;
    sbody.innerHTML = html;
    body.innerHTML = html;
    card.hidden = ok;
    document.querySelectorAll("[data-fw]").forEach((b) => (b.onclick = () => fixFirewall(b.dataset.fw === "public")));
  }

  async function fixFirewall(includePublic) {
    try {
      firewall = await call("firewall_fix", { includePublic });
      renderFirewall();
      if (iphone.state === "error") restartReceiver();
    } catch (_) {}
  }

  // ---------- Android ----------
  function renderAndroid(s) {
    android = s || android;
    s = android;
    const active = ["starting", "mirroring", "reconnecting"].includes(s.state) || (s.state === "error" && s.serial);
    $("android-session").hidden = !active;
    const name = s.name || "your phone";
    const how = s.wireless ? "over Wi-Fi" : "over USB";
    switch (s.state) {
      case "starting":
        $("android-title").textContent = `Connecting to ${name}…`;
        $("android-sub").textContent = "The phone window opens in a moment.";
        setPill($("android-pill"), "Connecting…", "busy");
        break;
      case "mirroring":
        $("android-title").textContent = s.camera ? `${name}'s camera` : `Mirroring ${name}`;
        $("android-sub").textContent = `Connected ${how}.${s.recording ? " Recording…" : ""}`;
        setPill($("android-pill"), s.recording ? "Mirroring · Recording" : "Mirroring", "ok");
        break;
      case "reconnecting":
        $("android-title").textContent = `Reconnecting to ${name}…`;
        $("android-sub").textContent = `The connection dropped. Trying again (attempt ${s.attempt} of 6).`;
        setPill($("android-pill"), "Reconnecting…", "warn");
        break;
      case "error":
        $("android-title").textContent = `Couldn't mirror ${name}`;
        $("android-sub").textContent = "";
        setPill($("android-pill"), "Problem", "bad");
        break;
      default:
        setPill($("android-pill"), "Not connected", "");
    }
    const prob = $("android-problem");
    prob.hidden = !s.problem;
    if (s.problem) prob.innerHTML = problemHtml(s.problem);
    $("android-warnings").innerHTML = (s.warnings || []).map((w) => problemHtml(w, "warn")).join("");
    const tb = $("android-toolbar");
    tb.hidden = s.state !== "mirroring";
    if (!tb.hidden) { tb.innerHTML = toolbar("android", s); wireToolbar(tb, "android"); }
    $("opt-screen-off").checked = !!s.screen_off;
    $("opt-clipboard").checked = settings.android_clipboard_sync !== false;
    if (!s.camera) $("opt-source").value = "screen";
    else if ($("opt-source").value === "screen") $("opt-source").value = "back";
    $("dot-android").className = `dot ${s.state === "mirroring" ? "on" : s.state === "error" ? "err" : ""}`;
  }

  $("android-stop").onclick = () => call("android_action", { action: "stop", value: null }).then(renderAndroid).catch(() => {});
  $("opt-screen-off").onchange = (e) => call("android_action", { action: "screen_off", value: e.target.checked }).then(renderAndroid).catch(() => {});
  $("opt-clipboard").onchange = async (e) => {
    try { renderAndroid(await call("android_action", { action: "clipboard", value: e.target.checked })); settings = await invoke("get_settings"); } catch (_) {}
  };
  $("opt-source").onchange = (e) => {
    const v = e.target.value;
    const value = v === "screen" ? { kind: "screen" } : { kind: "camera", facing: v };
    call("android_action", { action: "source", value }).then(renderAndroid).catch(() => {});
  };

  let refreshing = false;
  async function refreshDevices() {
    if (refreshing) return;
    refreshing = true;
    try {
      overview = await invoke("android_overview");
      $("android-error").hidden = true;
    } catch (e) {
      const el = $("android-error");
      el.hidden = false;
      el.innerHTML = problemHtml(e);
    } finally {
      refreshing = false;
    }
    renderDevices();
    if (wiz.mode === "usb" && wiz.step === "usb") wizUsbCheck();
  }

  function renderDevices() {
    const list = $("device-list");
    const rows = [];
    const connectedIds = new Set();
    for (const d of overview.connected) {
      if (d.remembered_id) connectedIds.add(d.remembered_id);
      const badge = d.transport === "usb" ? '<span class="badge usb">USB</span>' : '<span class="badge">Wi-Fi</span>';
      const mirroringThis = android.serial === d.serial && ["starting", "mirroring", "reconnecting"].includes(android.state);
      const actions = d.state === "device" && !mirroringThis
        ? `<button class="btn primary small" data-mirror="${esc(d.serial)}">Mirror</button>
           <button class="btn ghost small" data-camera="${esc(d.serial)}">Camera</button>`
        : mirroringThis ? '<span class="muted">Mirroring</span>' : "";
      rows.push(`<div class="device"><div class="d-ico"></div><div><div class="d-name">${esc(d.name)}${badge}</div>
        <div class="d-sub">${d.state === "device" ? "Connected" : "Needs attention"}</div>
        ${d.problem ? problemHtml(d.problem, "warn") : ""}</div>
        <div class="d-actions">${actions}</div></div>`);
    }
    for (const r of overview.remembered) {
      if (connectedIds.has(r.id)) continue;
      const when = r.last_connected ? new Date(r.last_connected * 1000).toLocaleDateString() : "";
      rows.push(`<div class="device"><div class="d-ico"></div><div><div class="d-name">${esc(r.name)}<span class="badge">Wi-Fi</span></div>
        <div class="d-sub">Paired${r.android_version ? ` · Android ${esc(r.android_version)}` : ""}${when ? ` · last used ${esc(when)}` : ""}</div></div>
        <div class="d-actions"><button class="btn primary small" data-connect="${esc(r.id)}">Connect</button>
        <button class="btn ghost small" data-forget="${esc(r.id)}">Forget</button></div></div>`);
    }
    list.innerHTML = rows.join("");
    $("device-empty").hidden = rows.length > 0;
    list.querySelectorAll("[data-mirror]").forEach((b) => (b.onclick = () => mirror(b.dataset.mirror)));
    list.querySelectorAll("[data-camera]").forEach((b) => (b.onclick = () => mirror(b.dataset.camera, { kind: "camera", facing: "back" })));
    list.querySelectorAll("[data-connect]").forEach((b) => (b.onclick = () => connectRemembered(b.dataset.connect, b)));
    list.querySelectorAll("[data-forget]").forEach((b) => (b.onclick = () => forget(b.dataset.forget)));
  }

  async function mirror(serial, source) {
    try { renderAndroid(await call("android_mirror", { serial, source: source || null })); refreshDevices(); } catch (_) {}
  }

  async function connectRemembered(id, btn) {
    btn.disabled = true;
    btn.textContent = "Looking…";
    try {
      renderAndroid(await invoke("android_connect", { id }));
    } catch (e) {
      const el = $("android-error");
      el.hidden = false;
      el.innerHTML = problemHtml(e);
    } finally {
      refreshDevices();
    }
  }

  async function forget(id) {
    const r = overview.remembered.find((x) => x.id === id);
    if (!confirm(`Forget ${r ? r.name : "this phone"}? You'll need to pair it again to use Wi-Fi.`)) return;
    try { await call("android_forget", { id }); refreshDevices(); } catch (_) {}
  }

  $("android-refresh").onclick = refreshDevices;
  setInterval(() => { if (view === "android" || wiz.open) refreshDevices(); }, 3000);

  // ---------- Wizard ----------
  const wiz = { open: false, step: "brand", brand: null, mode: "wifi", qr: null, pairing: null, device: null, serial: null };
  const WIFI_STEPS = ["brand", "dev", "wireless", "scan", "done"];
  const USB_FLOW = ["brand", "dev", "usb", "done"];

  $("add-phone").onclick = () => openWizard();
  function openWizard() {
    Object.assign(wiz, { open: true, step: "brand", mode: "wifi", qr: null, pairing: null, device: null });
    $("wizard").showModal();
    renderWizard();
  }
  $("wizard").addEventListener("close", () => {
    wiz.open = false;
    invoke("android_cancel_pairing").catch(() => {});
  });

  function flow() { return wiz.mode === "usb" ? USB_FLOW : WIFI_STEPS; }

  function go(step) {
    if (wiz.step === "scan" && step !== "scan") invoke("android_cancel_pairing").catch(() => {});
    wiz.step = step;
    renderWizard();
  }

  $("wiz-next").onclick = () => {
    if (wiz.step === "done") {
      $("wizard").close();
      if (wiz.serial) mirror(wiz.serial);
      return;
    }
    const f = flow();
    const i = f.indexOf(wiz.step);
    if (wiz.step === "brand" && !wiz.brand) { toast("info", "Pick your phone's brand first (or Other)."); return; }
    if (i >= 0 && i < f.length - 1) go(f[i + 1]);
  };
  $("wiz-back").onclick = () => {
    const f = flow();
    const i = f.indexOf(wiz.step);
    if (i > 0) go(f[i - 1]);
  };
  $("wiz-usb").onclick = () => {
    wiz.mode = wiz.mode === "usb" ? "wifi" : "usb";
    if (!wiz.brand) wiz.brand = "other";
    go(wiz.mode === "usb" ? "usb" : "wireless");
  };

  function renderWizard() {
    const B = window.Pictures.BRANDS;
    const brand = B[wiz.brand || "other"];
    const body = $("wiz-body");
    const f = flow();
    const labels = wiz.mode === "usb" ? ["Brand", "Developer options", "USB cable"] : ["Brand", "Developer options", "Wireless debugging", "Scan"];
    const idx = f.indexOf(wiz.step);
    $("wiz-progress").innerHTML = labels.map((l, i) => `<li class="${i === idx ? "current" : i < idx ? "done" : ""}">${l}</li>`).join("");
    $("wiz-back").hidden = idx <= 0 || wiz.step === "done";
    $("wiz-usb").hidden = wiz.step === "done" || wiz.step === "brand";
    $("wiz-usb").textContent = wiz.mode === "usb" ? "Use Wi-Fi instead" : "Use a USB cable instead";
    $("wiz-next").hidden = wiz.step === "scan" || wiz.step === "usb";
    $("wiz-next").textContent = wiz.step === "done" ? "Start mirroring" : "Next";

    if (wiz.step === "brand") {
      body.innerHTML = `<p>Which phone do you have? The steps are almost the same for all Android phones; we'll show the exact menu names for yours.</p>
        <div class="brands">${Object.entries(B).map(([k, v]) => `<button type="button" class="brand-btn" data-brand="${k}" aria-pressed="${wiz.brand === k}">${esc(v.label)}</button>`).join("")}</div>
        <p class="muted" style="margin-top:14px">You do this once. After that, one click connects your phone over Wi-Fi.</p>`;
      body.querySelectorAll("[data-brand]").forEach((b) => (b.onclick = () => { wiz.brand = b.dataset.brand; go("dev"); }));
    } else if (wiz.step === "dev") {
      body.innerHTML = `<h3>Turn on Developer options</h3>
        <p class="muted">This unlocks the settings PhoneMirror needs. It's safe and you can turn it off later. Skip this if you've done it before.</p>
        ${window.Pictures.strip(brand.dev.screens)}
        <ol class="wiz-steps">${brand.dev.steps.map((s) => `<li>${s}</li>`).join("")}</ol>`;
    } else if (wiz.step === "wireless") {
      body.innerHTML = `<h3>Turn on Wireless debugging</h3>
        <p class="muted">Needs Android 11 or newer and the phone on the same Wi-Fi as this PC. Older phone? Use a USB cable instead.</p>
        ${window.Pictures.strip(brand.wireless.screens)}
        <ol class="wiz-steps">${brand.wireless.steps.map((s) => `<li>${s}</li>`).join("")}</ol>
        ${brand.notes.map((n) => `<div class="note">${n}</div>`).join("")}
        <p>When the phone's camera opens, press <b>Next</b> to show the code.</p>`;
    } else if (wiz.step === "scan") {
      renderScan();
    } else if (wiz.step === "usb") {
      body.innerHTML = `<h3>Connect with a USB cable</h3>
        ${window.Pictures.strip(window.Pictures.USB_STEPS.screens)}
        <ol class="wiz-steps">${window.Pictures.USB_STEPS.steps.map((s) => `<li>${s}</li>`).join("")}</ol>
        <div class="qr-status" id="usb-status"><div class="state"><span class="spinner"></span>Waiting for your phone…</div></div>`;
      wizUsbCheck();
    } else if (wiz.step === "done") {
      const name = wiz.device ? wiz.device.name : "Your phone";
      body.innerHTML = `<div class="success"><div class="big">✅</div><h2>${esc(name)} is connected</h2>
        <p class="muted">${wiz.mode === "usb" ? "Keep the cable plugged in while mirroring." : "Next time, just press Connect: PhoneMirror remembers this phone."}</p></div>`;
    }
  }

  function wizUsbCheck() {
    const el = $("usb-status");
    if (!el) return;
    const usb = overview.connected.filter((d) => d.transport === "usb");
    const ready = usb.find((d) => d.state === "device");
    if (ready) {
      wiz.serial = ready.serial;
      wiz.device = { name: ready.name };
      go("done");
    } else if (usb.length) {
      el.innerHTML = problemHtml(usb[0].problem || { message: "Your phone needs attention." }, "warn");
    }
  }

  function renderScan() {
    const body = $("wiz-body");
    body.innerHTML = `<h3>Scan this code with your phone</h3>
      <div class="qr-wrap">
        <div class="qr" id="qr"><span class="spinner"></span></div>
        <div class="qr-status">
          <ol class="wiz-steps"><li>On the phone: <b>Wireless debugging</b> → <b>Pair device with QR code</b>.</li><li>Point the camera at this code.</li></ol>
          <div class="state" id="pair-state"><span class="spinner"></span>Waiting for your phone to scan…</div>
          <div id="pair-problem"></div>
          <p><button type="button" class="btn link" id="show-code-form">Can't scan? Pair with a code instead</button></p>
          <div id="code-form" hidden>
            <p class="muted">On the phone tap <b>Pair device with pairing code</b>. Type what it shows:</p>
            <div class="code-form">
              <label>IP address &amp; port<input type="text" id="code-addr" placeholder="192.168.1.23:37099" inputmode="url"></label>
              <label>Wi-Fi pairing code<input type="text" id="code-code" placeholder="123456" inputmode="numeric" maxlength="8"></label>
              <button type="button" class="btn primary" id="code-pair">Pair</button>
            </div>
          </div>
        </div>
      </div>`;
    $("show-code-form").onclick = () => ($("code-form").hidden = false);
    $("code-pair").onclick = async () => {
      const address = $("code-addr").value.trim(), code = $("code-code").value.trim();
      if (!address || !code) return;
      invoke("android_cancel_pairing").catch(() => {});
      setPairState("Pairing…", true);
      try {
        const device = await invoke("android_pair_code", { address, code });
        await refreshDevices();
        const row = overview.connected.find((d) => d.remembered_id === device.id);
        pairDone(device, row ? row.serial : null);
      } catch (e) {
        pairError(e);
      }
    };
    startPairing();
  }

  async function startPairing() {
    try {
      const r = await invoke("android_start_pairing");
      const qr = $("qr");
      if (qr) qr.innerHTML = r.svg;
    } catch (e) {
      pairError(e);
    }
  }

  function setPairState(text, busy) {
    const el = $("pair-state");
    if (el) el.innerHTML = `${busy ? '<span class="spinner"></span>' : ""}${esc(text)}`;
  }

  function pairError(p) {
    setPairState("", false);
    const el = $("pair-problem");
    if (el) el.innerHTML = problemHtml(p, "", '<button type="button" class="btn small" id="pair-retry">Show a new code</button>');
    const retry = $("pair-retry");
    if (retry) retry.onclick = () => renderScan();
  }

  function pairDone(device, serial) {
    wiz.device = device;
    wiz.serial = serial;
    go("done");
    refreshDevices();
  }

  const STEP_TEXT = {
    waiting_for_scan: "Waiting for your phone to scan…",
    phone_found: "Found your phone!",
    pairing: "Pairing…",
    looking_for_phone: "Connecting…",
    connecting: "Connecting…",
    reading_info: "Almost done…",
  };

  listen("pairing", (e) => {
    if (!wiz.open) return;
    if (e.type === "step") setPairState(STEP_TEXT[e.step.step] || "Working…", true);
    else if (e.type === "done") pairDone(e.device, e.serial);
    else if (e.type === "error") pairError(e.problem);
  });

  // ---------- Drag & drop ----------
  function canDrop() {
    return android.state === "mirroring" || overview.connected.some((d) => d.state === "device");
  }
  if (T.webview && T.webview.getCurrentWebview) {
    T.webview.getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload;
      if (p.type === "enter" || p.type === "over") {
        $("drop-overlay").hidden = !canDrop();
      } else if (p.type === "leave") {
        $("drop-overlay").hidden = true;
      } else if (p.type === "drop") {
        $("drop-overlay").hidden = true;
        if (!canDrop()) { toast("info", "Connect an Android phone first, then drop files to send them."); return; }
        const serial = android.state === "mirroring" ? null : (overview.connected.find((d) => d.state === "device") || {}).serial;
        invoke("android_send_files", { paths: p.paths, serial }).catch(showError);
      }
    });
  }

  // ---------- Settings ----------
  function fillQualitySelects() {
    document.querySelectorAll(".quality-select").forEach((sel) => {
      sel.innerHTML = QUALITY.map(([v, l]) => `<option value="${v}">${l}</option>`).join("");
    });
  }

  function renderSettings() {
    $("s-receiver-name").value = settings.receiver_name || "";
    $("s-receiver-name").placeholder = `${info.pc_name || "This PC"} – PhoneMirror`;
    $("s-start-receiver").checked = !!settings.start_iphone_receiver;
    $("s-iphone-quality").value = settings.iphone_quality;
    $("s-video-output").value = settings.iphone_video_output;
    $("s-require-pin").checked = !!settings.iphone_require_pin;
    $("s-android-quality").value = settings.android_quality;
    $("s-android-audio").checked = !!settings.android_audio;
    $("s-stay-awake").checked = !!settings.android_stay_awake;
    $("s-auto-reconnect").checked = !!settings.auto_reconnect;
    $("s-shots").value = settings.screenshots_dir || "";
    $("s-shots").placeholder = info.screenshots_dir || "";
    $("s-recs").value = settings.recordings_dir || "";
    $("s-recs").placeholder = info.recordings_dir || "";
    $("video-casting").checked = !!settings.iphone_video_casting;
  }

  async function saveSetting(patch) {
    try {
      settings = await call("save_settings", { settings: Object.assign({}, settings, patch) });
      info = await invoke("app_info");
      fillRname();
      renderSettings();
    } catch (_) {}
  }

  const bind = (id, key, kind) =>
    $(id).addEventListener("change", (e) => {
      let v = kind === "bool" ? e.target.checked : e.target.value;
      if (kind === "opt") v = v.trim() || null;
      saveSetting({ [key]: v });
    });
  bind("s-receiver-name", "receiver_name", "opt");
  bind("s-start-receiver", "start_iphone_receiver", "bool");
  bind("s-iphone-quality", "iphone_quality");
  bind("s-video-output", "iphone_video_output");
  bind("s-require-pin", "iphone_require_pin", "bool");
  bind("s-android-quality", "android_quality");
  bind("s-android-audio", "android_audio", "bool");
  bind("s-stay-awake", "android_stay_awake", "bool");
  bind("s-auto-reconnect", "auto_reconnect", "bool");
  bind("s-shots", "screenshots_dir", "opt");
  bind("s-recs", "recordings_dir", "opt");
  document.querySelectorAll("[data-open]").forEach((b) =>
    b.addEventListener("click", (e) => {
      e.preventDefault();
      const input = $(b.dataset.open);
      invoke("reveal", { path: input.value || input.placeholder }).catch(showError);
    })
  );

  $("copy-diagnostics").onclick = async () => {
    try {
      const text = await call("diagnostics");
      await navigator.clipboard.writeText(text);
      toast("success", "Diagnostics copied. Paste them into your bug report.");
    } catch (_) {}
  };
  $("restart-receiver").onclick = restartReceiver;

  document.querySelectorAll("a[data-link]").forEach((a) =>
    a.addEventListener("click", (e) => { e.preventDefault(); invoke("open_link", { url: a.href }).catch(showError); })
  );

  // ---------- Events & start ----------
  listen("iphone-status", renderIphone);
  listen("android-status", (s) => { renderAndroid(s); refreshDevices(); });
  listen("toast", (t) => toast(t.kind, t.message, t.path));

  async function init() {
    fillQualitySelects();
    try {
      [info, settings] = await Promise.all([invoke("app_info"), invoke("get_settings")]);
    } catch (e) { showError(e); }
    $("version").textContent = `PhoneMirror ${info.version || ""}`;
    fillRname();
    $("pic-cc").innerHTML = window.Pictures.iphoneWifi();
    $("pic-cc2").innerHTML = window.Pictures.iphoneControlCenter();
    $("pic-list").innerHTML = window.Pictures.iphoneMirrorList(info.receiver_name);
    renderSettings();
    renderIphone(await invoke("iphone_status"));
    renderAndroid(await invoke("android_status"));
    refreshFirewall();
    refreshDevices();
    if (info.engines && (!info.engines.iphone || !info.engines.android)) {
      const missing = [!info.engines.iphone && "iPhone", !info.engines.android && "Android"].filter(Boolean).join(" and ");
      toast("error", `The ${missing} engine is missing from this installation. Reinstall PhoneMirror.`);
    }
  }
  init();
})();
