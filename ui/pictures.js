/* PhoneMirror — illustrations and per-brand setup steps.
 *
 * The pictures are simplified drawings of each phone's Settings screens (not
 * screenshots), with the item to tap highlighted. Menu names differ a little
 * between models and software versions, so the text always says what to look
 * for, too.
 */
(function () {
  "use strict";

  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

  /** One phone screen: a title bar and a list of rows; one row highlighted. */
  function phone({ title, rows, tap, toggle, note, dark }) {
    const W = 180, H = 320, pad = 10;
    const bg = dark ? "#16181d" : "#ffffff";
    const fg = dark ? "#f2f4f8" : "#1b1e24";
    const sub = dark ? "#9aa3b2" : "#6b7280";
    let y = 62;
    let body = "";
    rows.forEach((r, i) => {
      const label = typeof r === "string" ? r : r.label;
      const small = typeof r === "string" ? "" : r.sub || "";
      const h = small ? 38 : 30;
      const hl = i === tap;
      if (hl) {
        body += `<rect x="${pad + 2}" y="${y - 2}" width="${W - 2 * pad - 4}" height="${h}" rx="8" fill="${dark ? "rgba(91,140,255,0.28)" : "var(--pic-accent-soft, #e3ebff)"}" stroke="var(--pic-accent, #2f6bff)" stroke-width="2"/>`;
      }
      const lx = pad + (hl ? 28 : 12);
      body += `<text x="${lx}" y="${y + (small ? 14 : 18)}" font-size="10.5" font-weight="${hl ? 700 : 500}" fill="${fg}">${esc(label)}</text>`;
      if (small) body += `<text x="${lx}" y="${y + 28}" font-size="9" fill="${sub}">${esc(small)}</text>`;
      if (hl && toggle !== undefined) {
        const on = toggle;
        body += `<rect x="${W - pad - 30}" y="${y + h / 2 - 7}" width="24" height="14" rx="7" fill="${on ? "var(--pic-accent, #2f6bff)" : "#c3c8d2"}"/>`;
        body += `<circle cx="${W - pad - (on ? 13 : 23)}" cy="${y + h / 2}" r="5.5" fill="#fff"/>`;
      } else if (!(hl && typeof r === "object" && r.times)) {
        body += `<path d="M${W - pad - 16} ${y + h / 2 - 4} l4 4 l-4 4" stroke="${sub}" stroke-width="1.6" fill="none"/>`;
      }
      if (hl) {
        // Finger tap marker, and a "×7" pill when the row must be tapped repeatedly.
        const times = typeof r === "object" && r.times;
        const cx = pad + 15, cy = y + h / 2;
        body += `<circle cx="${cx}" cy="${cy}" r="8" fill="var(--pic-accent, #2f6bff)" opacity="0.25"/><circle cx="${cx}" cy="${cy}" r="4" fill="var(--pic-accent, #2f6bff)"/>`;
        if (times) {
          body += `<rect x="${W - pad - 46}" y="${cy - 9}" width="30" height="18" rx="9" fill="#111827"/><text x="${W - pad - 31}" y="${cy + 4}" font-size="10" font-weight="700" fill="#fff" text-anchor="middle">×${times}</text>`;
        }
      }
      y += h + 6;
    });
    if (note) {
      body += `<text x="${W / 2}" y="${H - 22}" font-size="9" fill="${sub}" text-anchor="middle">${esc(note)}</text>`;
    }
    return `<svg class="phone-pic" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-label="${esc(title)}">
      <rect x="1" y="1" width="${W - 2}" height="${H - 2}" rx="22" fill="${bg}" stroke="#2a2f3a" stroke-width="2"/>
      <rect x="${W / 2 - 18}" y="9" width="36" height="6" rx="3" fill="#2a2f3a"/>
      <text x="${pad + 4}" y="46" font-size="15" font-weight="700" fill="${fg}">${esc(title)}</text>
      ${body}
    </svg>`;
  }

  function strip(screens) {
    return `<div class="pic-strip">${screens.map((s, i) => (i ? '<span class="pic-arrow" aria-hidden="true">→</span>' : "") + phone(s)).join("")}</div>`;
  }

  // ---------- iPhone pictures ----------

  function iphoneControlCenter() {
    return `<svg class="phone-pic" viewBox="0 0 168 320" width="168" height="320" role="img" aria-label="Control Center with Screen Mirroring">
      <rect x="1" y="1" width="166" height="318" rx="26" fill="#1c1f26" stroke="#2a2f3a" stroke-width="2"/>
      <rect x="60" y="10" width="48" height="12" rx="6" fill="#000"/>
      <text x="150" y="40" font-size="9" fill="#cfd5df" text-anchor="end">swipe down from here ↓</text>
      <rect x="14" y="54" width="66" height="66" rx="16" fill="#2c313b"/>
      <rect x="88" y="54" width="66" height="66" rx="16" fill="#2c313b"/>
      <rect x="14" y="128" width="66" height="30" rx="12" fill="var(--pic-accent, #2f6bff)" stroke="#fff" stroke-width="2"/>
      <rect x="25" y="136" width="14" height="10" rx="2" fill="none" stroke="#fff" stroke-width="1.6"/><rect x="31" y="141" width="14" height="10" rx="2" fill="#fff"/>
      <circle cx="40" cy="168" r="11" fill="var(--pic-accent, #2f6bff)" opacity="0.35"/><circle cx="40" cy="168" r="5" fill="#fff"/>
      <rect x="88" y="128" width="30" height="30" rx="12" fill="#2c313b"/><rect x="124" y="128" width="30" height="30" rx="12" fill="#2c313b"/>
      <rect x="14" y="190" width="30" height="80" rx="14" fill="#2c313b"/><rect x="52" y="190" width="30" height="80" rx="14" fill="#2c313b"/>
      <text x="84" y="300" font-size="9" fill="#9aa3b2" text-anchor="middle">Control Center</text>
    </svg>`;
  }

  function iphoneWifi() {
    return phone({
      title: "Wi-Fi",
      dark: true,
      rows: [{ label: "HomeWiFi  ✓", sub: "Same Wi-Fi as your PC" }, "Neighbour-5G", "Cafe Guest"],
      tap: 0,
      note: "Settings → Wi-Fi",
    });
  }

  function iphoneMirrorList(name) {
    return `<svg class="phone-pic" viewBox="0 0 168 320" width="168" height="320" role="img" aria-label="Screen Mirroring list">
      <rect x="1" y="1" width="166" height="318" rx="26" fill="#1c1f26" stroke="#2a2f3a" stroke-width="2"/>
      <rect x="60" y="10" width="48" height="12" rx="6" fill="#000"/>
      <rect x="14" y="70" width="140" height="150" rx="16" fill="#2c313b"/>
      <text x="84" y="96" font-size="11" fill="#fff" text-anchor="middle" font-weight="700">Screen Mirroring</text>
      <rect x="22" y="112" width="124" height="34" rx="9" fill="var(--pic-accent, #2f6bff)" opacity="0.25" stroke="var(--pic-accent, #2f6bff)" stroke-width="2"/>
      <rect x="30" y="122" width="16" height="12" rx="2" fill="none" stroke="#fff" stroke-width="1.5"/>
      <text x="52" y="132" font-size="8.5" fill="#fff" font-weight="700">${esc(shorten(name, 17))}</text>
      <circle cx="132" cy="129" r="10" fill="var(--pic-accent, #2f6bff)" opacity="0.4"/><circle cx="132" cy="129" r="4.5" fill="#fff"/>
      <rect x="22" y="154" width="124" height="28" rx="9" fill="#353b47"/><text x="52" y="172" font-size="8.5" fill="#9aa3b2">Living Room TV</text>
      <text x="84" y="300" font-size="9" fill="#9aa3b2" text-anchor="middle">Pick your PC</text>
    </svg>`;
  }

  function iphoneAirplayAudio(name) {
    return `<svg class="phone-pic" viewBox="0 0 168 320" width="168" height="320" role="img" aria-label="AirPlay audio picker">
      <rect x="1" y="1" width="166" height="318" rx="26" fill="#1c1f26" stroke="#2a2f3a" stroke-width="2"/>
      <rect x="60" y="10" width="48" height="12" rx="6" fill="#000"/>
      <rect x="14" y="60" width="140" height="70" rx="16" fill="#2c313b"/>
      <rect x="24" y="72" width="34" height="34" rx="6" fill="#ff7a59"/>
      <text x="66" y="86" font-size="9" fill="#fff" font-weight="700">Now Playing</text>
      <path d="M128 78 l8 12 h-16 z" fill="none" stroke="var(--pic-accent, #2f6bff)" stroke-width="2"/>
      <circle cx="140" cy="100" r="9" fill="var(--pic-accent, #2f6bff)" opacity="0.4"/>
      <rect x="14" y="146" width="140" height="92" rx="16" fill="#2c313b"/>
      <text x="26" y="166" font-size="9" fill="#9aa3b2">Speakers &amp; TVs</text>
      <rect x="22" y="176" width="124" height="28" rx="8" fill="var(--pic-accent, #2f6bff)" opacity="0.25" stroke="var(--pic-accent, #2f6bff)" stroke-width="2"/>
      <text x="32" y="194" font-size="8.5" fill="#fff" font-weight="700">${esc(shorten(name, 24))}</text>
      <text x="84" y="300" font-size="9" fill="#9aa3b2" text-anchor="middle">AirPlay to your PC</text>
    </svg>`;
  }

  function shorten(s, n) {
    s = String(s || "");
    return s.length > n ? s.slice(0, n - 1) + "…" : s;
  }

  // ---------- Android brands ----------

  const DEV_NOTE = "Enter your PIN if asked";

  const BRANDS = {
    samsung: {
      label: "Samsung",
      dev: {
        steps: [
          "Open <b>Settings</b> → <b>About phone</b> → <b>Software information</b>.",
          "Tap <b>Build number</b> 7 times. Enter your PIN if asked. You'll see “Developer mode has been turned on”.",
        ],
        screens: [
          { title: "Settings", rows: ["Connections", "Display", "Battery", "About phone"], tap: 3 },
          { title: "About phone", rows: ["Status information", "Legal information", "Software information"], tap: 2 },
          { title: "Software info", rows: ["One UI version", "Android version", { label: "Build number", times: 7 }], tap: 2, note: DEV_NOTE },
        ],
      },
      wireless: {
        steps: [
          "Go back to <b>Settings</b> and scroll to the bottom: open <b>Developer options</b>.",
          "Turn on <b>Wireless debugging</b> and tap <b>Allow</b>.",
          "Tap the words <b>Wireless debugging</b>, then <b>Pair device with QR code</b>.",
        ],
        screens: [
          { title: "Settings", rows: ["About phone", "Developer options"], tap: 1 },
          { title: "Developer options", rows: ["USB debugging", "Wireless debugging"], tap: 1, toggle: true },
          { title: "Wireless debugging", rows: ["Use wireless debugging", "Pair device with QR code", "Pair device with pairing code"], tap: 1 },
        ],
      },
      notes: [],
    },
    xiaomi: {
      label: "Xiaomi / Redmi / POCO",
      dev: {
        steps: [
          "Open <b>Settings</b> → <b>About phone</b>.",
          "Tap <b>OS version</b> (HyperOS) or <b>MIUI version</b> 7 times until it says you're a developer.",
        ],
        screens: [
          { title: "Settings", rows: ["About phone", "SIM cards", "Wi-Fi"], tap: 0 },
          { title: "About phone", rows: ["Device name", "Storage", { label: "OS version", sub: "or MIUI version", times: 7 }], tap: 2, note: DEV_NOTE },
        ],
      },
      wireless: {
        steps: [
          "Open <b>Settings</b> → <b>Additional settings</b> → <b>Developer options</b>.",
          "Turn on <b>USB debugging (Security settings)</b> too — Xiaomi needs it for mouse and keyboard control (you may need to sign in to your Mi account and restart the phone).",
          "Turn on <b>Wireless debugging</b>, tap it, then tap <b>Pair device with QR code</b>.",
        ],
        screens: [
          { title: "Settings", rows: ["About phone", "Additional settings"], tap: 1 },
          { title: "Additional settings", rows: ["Accessibility", "Developer options"], tap: 1 },
          { title: "Developer options", rows: ["USB debugging", { label: "USB debugging", sub: "(Security settings)" }, "Wireless debugging"], tap: 2, toggle: true },
        ],
      },
      notes: [
        "To install apps by dropping APKs, also turn on <b>Install via USB</b> in Developer options.",
      ],
    },
    vivo: {
      label: "Vivo / iQOO",
      dev: {
        steps: [
          "Open <b>Settings</b> → <b>About phone</b> (on some models: <b>System management</b> → <b>About phone</b>) → <b>Version</b>.",
          "Tap <b>Software version</b> 7 times.",
        ],
        screens: [
          { title: "Settings", rows: ["System management", "About phone"], tap: 1 },
          { title: "About phone", rows: ["Device name", "Version"], tap: 1 },
          { title: "Version", rows: ["Android version", { label: "Software version", times: 7 }], tap: 1, note: DEV_NOTE },
        ],
      },
      wireless: {
        steps: [
          "Open <b>Settings</b> → <b>System management</b> (or <b>System</b>) → <b>Developer options</b>.",
          "Turn on <b>Wireless debugging</b>, tap it, then tap <b>Pair device with QR code</b>.",
        ],
        screens: [
          { title: "System management", rows: ["Date & time", "Developer options"], tap: 1 },
          { title: "Developer options", rows: ["USB debugging", "Wireless debugging"], tap: 1, toggle: true },
          { title: "Wireless debugging", rows: ["Pair device with QR code", "Pair with pairing code"], tap: 0 },
        ],
      },
      notes: [],
    },
    oppo: {
      label: "OPPO / Realme",
      dev: {
        steps: [
          "Open <b>Settings</b> → <b>About device</b> (Realme: <b>About phone</b>) → <b>Version</b>.",
          "Tap <b>Build number</b> 7 times.",
        ],
        screens: [
          { title: "Settings", rows: ["Wi-Fi", "Battery", "About device"], tap: 2 },
          { title: "About device", rows: ["Device name", "Version"], tap: 1 },
          { title: "Version", rows: ["ColorOS version", { label: "Build number", times: 7 }], tap: 1, note: DEV_NOTE },
        ],
      },
      wireless: {
        steps: [
          "Open <b>Settings</b> → <b>Additional settings</b> (newer: <b>System settings</b>) → <b>Developer options</b>.",
          "Turn on <b>Disable permission monitoring</b> if you see it — OPPO needs it for mouse and keyboard control.",
          "Turn on <b>Wireless debugging</b>, tap it, then tap <b>Pair device with QR code</b>.",
        ],
        screens: [
          { title: "Settings", rows: ["About device", "Additional settings"], tap: 1 },
          { title: "Additional settings", rows: ["Keyboard", "Developer options"], tap: 1 },
          { title: "Developer options", rows: ["Disable permission monitoring", "Wireless debugging"], tap: 1, toggle: true },
        ],
      },
      notes: [],
    },
    oneplus: {
      label: "OnePlus",
      dev: {
        steps: ["Open <b>Settings</b> → <b>About device</b> → <b>Version</b>.", "Tap <b>Build number</b> 7 times."],
        screens: [
          { title: "Settings", rows: ["Battery", "Apps", "About device"], tap: 2 },
          { title: "About device", rows: ["Device name", "Version"], tap: 1 },
          { title: "Version", rows: ["OxygenOS version", { label: "Build number", times: 7 }], tap: 1, note: DEV_NOTE },
        ],
      },
      wireless: {
        steps: [
          "Open <b>Settings</b> → <b>Additional settings</b> (older: <b>System</b>) → <b>Developer options</b>.",
          "Turn on <b>Wireless debugging</b>, tap it, then tap <b>Pair device with QR code</b>.",
        ],
        screens: [
          { title: "Settings", rows: ["About device", "Additional settings"], tap: 1 },
          { title: "Developer options", rows: ["USB debugging", "Wireless debugging"], tap: 1, toggle: true },
          { title: "Wireless debugging", rows: ["Pair device with QR code", "Pair with pairing code"], tap: 0 },
        ],
      },
      notes: [],
    },
    motorola: {
      label: "Motorola",
      dev: {
        steps: ["Open <b>Settings</b> → <b>About phone</b>.", "Scroll down and tap <b>Build number</b> 7 times."],
        screens: [
          { title: "Settings", rows: ["Display", "System", "About phone"], tap: 2 },
          { title: "About phone", rows: ["Android version", { label: "Build number", times: 7 }], tap: 1, note: DEV_NOTE },
        ],
      },
      wireless: {
        steps: [
          "Open <b>Settings</b> → <b>System</b> → <b>Developer options</b>.",
          "Turn on <b>Wireless debugging</b>, tap it, then tap <b>Pair device with QR code</b>.",
        ],
        screens: [
          { title: "System", rows: ["Gestures", "Developer options"], tap: 1 },
          { title: "Developer options", rows: ["USB debugging", "Wireless debugging"], tap: 1, toggle: true },
          { title: "Wireless debugging", rows: ["Pair device with QR code", "Pair with pairing code"], tap: 0 },
        ],
      },
      notes: [],
    },
    pixel: {
      label: "Google Pixel",
      dev: {
        steps: ["Open <b>Settings</b> → <b>About phone</b>.", "Scroll to the bottom and tap <b>Build number</b> 7 times."],
        screens: [
          { title: "Settings", rows: ["System", "About phone"], tap: 1 },
          { title: "About phone", rows: ["Android version", { label: "Build number", times: 7 }], tap: 1, note: DEV_NOTE },
        ],
      },
      wireless: {
        steps: [
          "Open <b>Settings</b> → <b>System</b> → <b>Developer options</b>.",
          "Turn on <b>Wireless debugging</b>, tap it, then tap <b>Pair device with QR code</b>.",
        ],
        screens: [
          { title: "System", rows: ["Languages", "Developer options"], tap: 1 },
          { title: "Developer options", rows: ["USB debugging", "Wireless debugging"], tap: 1, toggle: true },
          { title: "Wireless debugging", rows: ["Pair device with QR code", "Pair with pairing code"], tap: 0 },
        ],
      },
      notes: [],
    },
    other: {
      label: "Other Android",
      dev: {
        steps: [
          "Open <b>Settings</b> → <b>About phone</b> (sometimes inside <b>System</b> or <b>Software information</b>).",
          "Find <b>Build number</b> and tap it 7 times. Use the Settings search for “Build number” if you can't find it.",
        ],
        screens: [
          { title: "Settings", rows: ["System", "About phone"], tap: 1 },
          { title: "About phone", rows: ["Android version", { label: "Build number", times: 7 }], tap: 1, note: DEV_NOTE },
        ],
      },
      wireless: {
        steps: [
          "Open <b>Developer options</b> (search Settings for “Developer options”).",
          "Turn on <b>Wireless debugging</b>, tap it, then tap <b>Pair device with QR code</b>.",
        ],
        screens: [
          { title: "Developer options", rows: ["USB debugging", "Wireless debugging"], tap: 1, toggle: true },
          { title: "Wireless debugging", rows: ["Pair device with QR code", "Pair with pairing code"], tap: 0 },
        ],
      },
      notes: [],
    },
  };

  const USB_STEPS = {
    steps: [
      "In <b>Developer options</b>, turn on <b>USB debugging</b>.",
      "Plug the phone into the PC with a USB cable that carries data (some cables only charge).",
      "Unlock the phone and tap <b>Allow</b> on “Allow USB debugging?”. Tick “Always allow from this computer”.",
    ],
    screens: [
      { title: "Developer options", rows: ["USB debugging", "Wireless debugging"], tap: 0, toggle: true },
      { title: "Allow USB debugging?", rows: [{ label: "Always allow from", sub: "this computer" }, "Cancel", "Allow"], tap: 2 },
    ],
  };

  window.Pictures = { phone, strip, iphoneWifi, iphoneControlCenter, iphoneMirrorList, iphoneAirplayAudio, BRANDS, USB_STEPS };
})();
