// OnKey's "Send feedback": checks what the app sent (src-tauri/src/feedback.rs) and writes it as a plain-text email.
// Kept apart from worker.js (which needs Cloudflare's runtime) so `node --test` can check it.

const LIMITS = { text: 4000, email: 200, version: 40, os: 20, os_version: 40, language: 16 };
const MOODS = { up: "👍", down: "👎" };

/** One line, no control characters (nothing a header could be split with). */
const line = (v, max) => (typeof v === "string" ? v : "").replace(/[\u0000-\u001f\u007f]/g, " ").trim().slice(0, max);

/** The feedback, checked: { ok: true, f } or { ok: false, why }. */
export function check(body) {
  if (!body || typeof body !== "object") return { ok: false, why: "not JSON" };
  const mood = body.mood == null ? null : body.mood;
  if (mood !== null && !(mood in MOODS)) return { ok: false, why: "unknown thumb" };
  const text = typeof body.text === "string" ? body.text.replace(/\r\n?/g, "\n").trim() : "";
  if (text.length > LIMITS.text) return { ok: false, why: "too long" };
  if (mood === null && !text) return { ok: false, why: "nothing to send" };
  const email = line(body.email, LIMITS.email);
  const f = {
    mood, text,
    // a reply address only if it looks like one
    email: /^[^\s@<>()",;:]+@[^\s@<>()",;:]+\.[^\s@<>()",;:]+$/.test(email) ? email : "",
    emailGiven: email,
    from: ["take", "bar"].includes(body.from) ? body.from : "settings",
    version: line(body.version, LIMITS.version) || "?",
    os: line(body.os, LIMITS.os) || "?",
    os_version: line(body.os_version, LIMITS.os_version),
    language: line(body.language, LIMITS.language) || "?",
  };
  return { ok: true, f };
}

/** "=?UTF-8?B?…?=": a header value that can hold any character. */
const word = s => `=?UTF-8?B?${btoa(String.fromCharCode(...new TextEncoder().encode(s)))}?=`;

/** Base64, in lines of 76. */
function b64(s) {
  const bytes = new TextEncoder().encode(s);
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin).replace(/.{76}/g, "$&\r\n");
}

/** The subject: the thumb (or "words"), the system, the version and the language. */
export function subject(f) {
  const sys = [f.os, f.os_version].filter(Boolean).join(" ");
  return `OnKey feedback: ${f.mood ? MOODS[f.mood] : "words"} (${sys}, ${f.version}, ${f.language})`;
}

/** The email's text. */
export function text(f, when = new Date()) {
  const where = { take: "the card on a take's page", bar: "the top bar's speech bubble" }[f.from] ?? "Settings → Send feedback";
  return [
    f.mood ? `${MOODS[f.mood]} ${f.mood === "up" ? "Good" : "Not great"}` : "(no thumb)",
    "",
    f.text || "(nothing written)",
    "",
    "—",
    `Reply to: ${f.email || (f.emailGiven ? `(not an address: ${f.emailGiven})` : "(no email given)")}`,
    `OnKey ${f.version} on ${[f.os, f.os_version].filter(Boolean).join(" ")}, in ${f.language}`,
    `Sent from ${where}, ${when.toISOString()}`,
  ].join("\n");
}

/** The whole message (RFC 5322), from `from` to `to`. */
export function mime(f, from, to, id, when = new Date()) {
  const headers = [
    `From: ${word("OnKey feedback")} <${from}>`,
    `To: <${to}>`,
    ...(f.email ? [`Reply-To: <${f.email}>`] : []),
    `Subject: ${word(subject(f))}`,
    `Date: ${when.toUTCString()}`,
    `Message-ID: <${id}@singonkey.app>`,
    "MIME-Version: 1.0",
    "Content-Type: text/plain; charset=utf-8",
    "Content-Transfer-Encoding: base64",
  ];
  return `${headers.join("\r\n")}\r\n\r\n${b64(text(f, when))}\r\n`;
}
