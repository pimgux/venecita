# Phase 8: Local API and CLI

## Goal
Other software, such as a stream deck, a macro pad or scripts, can control Venecita Dock safely.

## Server (`venecita-api`, axum on tokio)
The API listens on `127.0.0.1:47800` by default. LAN access requires `api.allow_lan = true`,
and the settings window shows a warning when it is on.

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/v1/health` | no | `{"status":"ok","version":"…"}` |
| GET | `/v1/state` | yes | Active profile and layout, pause and focus-mode flags |
| GET | `/v1/displays` | yes | Connected displays and the definitions they resolved to |
| GET | `/v1/windows` | yes | Managed windows with their app, zone and stack |
| GET | `/v1/profiles` | yes | Profiles and their layouts |
| POST | `/v1/actions` | yes | Body: an `Action` as JSON (`{"action":"move_zone","dir":"left"}`) |
| POST | `/v1/profiles/{name}/activate` | yes | Shortcut for `activate_profile` |
| GET (WS) | `/v1/events` | yes | Stream of state, focus and profile events |

## Security
- The token is 32 random bytes from the OS CSPRNG, encoded as base64url. It is created on
  first run and stored in `token.secret` with mode 0600; on Windows the file's ACL is
  limited to the current user.
- `Authorization: Bearer <token>` is checked with `subtle::ConstantTimeEq`. WebSocket clients
  may send the token in the `Sec-WebSocket-Protocol` header instead.
- The `Host` header must be `127.0.0.1:<port>` or `localhost:<port>` unless LAN access is on.
  This blocks DNS rebinding.
- Requests carrying a browser `Origin` header are rejected (there is no CORS), which blocks
  cross-site requests from web pages.
- Failed logins are rate-limited: after 10 failures per minute from one IP, the server
  answers 429 for 60 seconds.
- Request bodies are limited to 16 KiB, and errors are returned as JSON with no internal details.
- The token can be rotated from the tray, from the settings window, or with `venecita token rotate`.
- Tokens are never written to the logs.

## CLI (`venecita`)
- `venecita status`
- `venecita action focus-mode-toggle`
- `venecita action move-zone --dir left`
- `venecita profile list`
- `venecita profile activate Home`
- `venecita token show`
- `venecita token rotate`

The CLI reads the token file directly. It also accepts the `--url` and `--token` flags or
the `VENECITA_TOKEN` environment variable.

## Docs
`docs/API.md` has curl examples. The OpenAPI JSON is generated with `utoipa` and served at `/v1/openapi.json`.

## Tests (`tower::ServiceExt::oneshot`, no real socket)
- A request without a token, or with a bad one, gets 401.
- A foreign `Host` gets 403, and a request with an `Origin` header gets 403.
- A valid action is forwarded to the engine channel.
- The rate limiter kicks in after 10 failures.
- An oversized body gets 413.
- The WebSocket receives an event after an action.
- The CLI's argument parsing is tested with `clap`'s debug_assert.

## Manual checklist
- [ ] `curl -H "Authorization: Bearer $(venecita token show)" -X POST -d '{"action":"focus_mode_toggle"}' http://127.0.0.1:47800/v1/actions` turns focus mode on.
