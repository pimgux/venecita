# Local API and `venecita` CLI

Venecita Dock can be controlled by other programs — a stream deck, a macro pad, scripts —
through a small HTTP + WebSocket API on your own computer, and with the `venecita` command.

## Turning it on

The API is on by default and only listens on this computer:

```toml
[api]
enabled = true
bind = "127.0.0.1:47800"
allow_lan = false   # true lets other devices on your network use it: keep the token secret
```

Requests need the access token, which Venecita creates on first start in `token.secret`
next to `config.toml` (readable only by you). Show it, or replace it, with:

```sh
venecita token show
venecita token rotate      # the running app switches to the new token by itself
```

It can also be shown, copied and rotated in **Settings → General → Local API**.

## The `venecita` command

```sh
venecita status                          # profile, layout, pause, focus mode
venecita action relayout-all             # any action, in kebab-case
venecita action move-zone --dir left
venecita action move-to-display --target next
venecita action focus-mode-toggle
venecita profile list
venecita profile activate Casa
venecita displays
venecita windows
venecita --json status                   # raw JSON, for scripts
```

It finds the address in `config.toml` and the token in `token.secret`. `--url`, `--token`
or the `VENECITA_TOKEN` variable override them.

## HTTP

Every endpoint except `/v1/health` and `/v1/openapi.json` needs
`Authorization: Bearer <token>`. Errors are JSON: `{"error": "…"}`.

```sh
TOKEN=$(venecita token show)
API=http://127.0.0.1:47800

curl $API/v1/health                                     # {"status":"ok","version":"…"}
curl -H "Authorization: Bearer $TOKEN" $API/v1/state
curl -H "Authorization: Bearer $TOKEN" $API/v1/displays
curl -H "Authorization: Bearer $TOKEN" $API/v1/windows
curl -H "Authorization: Bearer $TOKEN" $API/v1/profiles

# Actions: the same as in the [hotkeys] table, as JSON.
curl -H "Authorization: Bearer $TOKEN" -X POST -d '{"action":"focus_mode_toggle"}' $API/v1/actions
curl -H "Authorization: Bearer $TOKEN" -X POST -d '{"action":"move_zone","dir":"left"}' $API/v1/actions
curl -H "Authorization: Bearer $TOKEN" -X POST $API/v1/profiles/Casa/activate
```

| Method | Path | Answer |
|---|---|---|
| GET | `/v1/health` | `{"status":"ok","version":"…"}` (no token needed) |
| GET | `/v1/state` | Active profile and layout, `manual`, `paused`, `focus_mode`, `auto_arrange` |
| GET | `/v1/displays` | Connected displays and the definition each one resolved to |
| GET | `/v1/windows` | Known windows: app, title, frame, kind, focus, zone and stack |
| GET | `/v1/profiles` | Profiles and their layouts, marking the active ones |
| POST | `/v1/actions` | Runs an action: `{"action":"move_zone","dir":"left"}` |
| POST | `/v1/profiles/{name}/activate` | Switches to a profile (404 if there is none) |
| GET | `/v1/events` | WebSocket, see below |
| GET | `/v1/openapi.json` | The OpenAPI description (no token needed) |

Actions: `move_zone`, `grow`, `shrink` (`dir`: `left`, `right`, `up`, `down`),
`move_to_display` (`target`: `next`, `prev`), `snap_to_grid`, `pin_here`, `stack_next`,
`stack_prev`, `dynamic_next`, `dynamic_prev`, `focus_window` (`id`), `focus_mode_toggle`,
`relayout_all`, `activate_profile` (`name`), `next_profile`, `auto_profile`, `toggle_pause`,
`open_settings`.

## Events (WebSocket)

`ws://127.0.0.1:47800/v1/events` sends a JSON message for every change:

```json
{"type":"state","state":{"profile":"Casa","layout":"Notebook abierta","paused":false,"focus_mode":false,…}}
{"type":"profile","profile":"Trabajo","layout":"Notebook cerrada"}
{"type":"focus","window":12345,"app":{"id":"com.microsoft.VSCode","name":"Code"}}
```

Send the token as `Authorization: Bearer <token>`, or, where a client can't set headers, as
the WebSocket subprotocol (`Sec-WebSocket-Protocol: <token>`).

## Security

- Only this computer can connect unless `allow_lan = true`.
- The `Host` header must be `127.0.0.1`, `localhost` or `[::1]` with the API's port, which
  blocks DNS rebinding; requests from web pages (with an `Origin` header) are refused.
- After 10 failed logins within a minute, an address gets `429` for a minute.
- Bodies are limited to 16 KiB. The token is compared in constant time and never logged.
