# feedback.singonkey.app

The other end of OnKey's **Send feedback** (the speech bubble in the top bar, Settings → About, and the "How's OnKey working for you?" card a take's
page shows after your 3rd sung take and your 20th). The app (`src-tauri/src/feedback.rs`) posts a small JSON message
here only when you press Send; this Cloudflare Worker checks it and emails it to OnKey's maker through Cloudflare
Email Routing. Nothing is stored: the email is the only copy.

What arrives: the thumb (up, down or none), what you wrote, the email address you gave for a reply (if any), OnKey's
version, the system and its version, the app's language and where you sent it from. The Worker adds nothing else.

| File | |
|---|---|
| `worker.js` | the Worker: POST only, at most 10 a minute from one address, 16 KB, then the email |
| `compose.js` | checks the message and writes the email (plain text, the thumb in the subject) |
| `compose.test.mjs` | `node --test` here |
| `wrangler.toml` | the Worker's name, its address (a custom domain), the email and rate-limit bindings |

## Deploying (once, then after any change here)

Needs singonkey.app's Email Routing on (it is: hello@singonkey.app) and the address it emails to verified there as a
destination address.

```
cd server/feedback
npx wrangler login                 # your Cloudflare account, in the browser
npx wrangler secret put TO         # the destination address (a verified one in Email Routing)
npx wrangler deploy                # makes feedback.singonkey.app (DNS and certificate included)
```

Check it (an email should arrive):

```
curl -i -X POST https://feedback.singonkey.app/ -H 'Content-Type: application/json' \
  -d '{"mood":"up","text":"test from curl","email":"","from":"settings","version":"dev","os":"macos","os_version":"","language":"en"}'
```

`204` is sent. `npx wrangler tail` shows the Worker's log live.
