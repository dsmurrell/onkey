// feedback.singonkey.app: receives OnKey's "Send feedback" (src-tauri/src/feedback.rs) and emails it to OnKey's
// maker through Cloudflare Email Routing. Nothing is stored: the email is the only copy. README.md says how to deploy.
import { EmailMessage } from "cloudflare:email";
import { check, mime } from "./compose.js";

const FROM = "feedback@singonkey.app";
const MAX_BYTES = 16 * 1024;

const reply = (status, msg) => new Response(msg ? `${msg}\n` : null, { status, headers: { "Content-Type": "text/plain; charset=utf-8" } });

export default {
  async fetch(request, env) {
    if (request.method !== "POST") return reply(405, "OnKey feedback: send it from the app (Settings → Send feedback).");
    // a few per minute from one address is plenty for a person; more is someone else
    if (env.LIMIT) {
      const { success } = await env.LIMIT.limit({ key: request.headers.get("CF-Connecting-IP") ?? "?" });
      if (!success) return reply(429, "too many, try again in a minute");
    }
    const raw = await request.text();
    if (raw.length > MAX_BYTES) return reply(413, "too long");
    let body;
    try { body = JSON.parse(raw); } catch { return reply(400, "not JSON"); }
    const c = check(body);
    if (!c.ok) return reply(400, c.why);
    if (!env.TO) return reply(500, "not set up (no TO)");
    const message = new EmailMessage(FROM, env.TO, mime(c.f, FROM, env.TO, crypto.randomUUID()));
    try {
      await env.MAIL.send(message);
    } catch (e) {
      console.log("send failed:", String(e));
      return reply(502, "couldn't send");
    }
    return reply(204);
  },
};
