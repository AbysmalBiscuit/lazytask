# Running a TaskChampion sync server

LazyTask syncs through the official [`taskchampion-sync-server`][upstream]
HTTP protocol. This doc shows how to run one locally so you can test the
sync feature end-to-end on a single machine, or set one up on your own
LAN / VPS for cross-device sync.

There are two ways to run it: via the supplied `compose.yaml` (recommended)
or via a single `docker run` / `podman run` command.

[upstream]: https://github.com/GothenburgBitFactory/taskchampion-sync-server

## Option A: Docker / Podman Compose (recommended)

The repo ships a `compose.yaml` at the root. It works with both
`docker compose` and `podman compose` (or `podman-compose`).

```bash
# 1. Generate a lowercase UUID and put it in .env
echo "CLIENT_ID=$(uuidgen | tr 'A-Z' 'a-z')" > .env

# 2. Start the server in the background
docker compose up -d            # or: podman compose up -d

# 3. Tail the logs to confirm it's listening
docker compose logs sync-server # should show "Serving on 0.0.0.0:8080"

# 4. Configure LazyTask
./target/release/lazytask
#   - press Shift+S
#   - Server URL:        http://localhost:8810
#   - Client ID:         <copy the UUID from your .env file>
#   - Encryption Secret: any string (must be the same on every replica)
#   - press Enter to save, then `s` to sync

# 5. Stop the server when done
docker compose down             # keep the database
docker compose down --volumes   # also wipe the database
```

The persisted SQLite database lives in a Docker named volume called
`lazytask_sync-data`, so it survives container restarts. To start fresh,
run `docker compose down --volumes`.

### Customizing

`.env` is auto-loaded by compose. Override any of these:

| Variable | Default | Purpose |
|---|---|---|
| `CLIENT_ID` | unset | Restrict the server to one client UUID. Unset = accept anything (only safe on a trusted local network). |
| `LAZYTASK_SYNC_PORT` | `8810` | Host port the server is exposed on. |
| `RUST_LOG` | `info` | Server log verbosity (`info` / `debug` / `trace`). |

## Option B: One-shot `docker run` / `podman run`

If you don't want to keep `compose.yaml` around (e.g. for a quick test):

```bash
CLIENT_ID=$(uuidgen | tr 'A-Z' 'a-z')
echo "client_id: $CLIENT_ID"

# Replace `podman` with `docker` if you use Docker.
podman run -d \
  --name=lazytask-sync-server \
  -p 8810:8080 \
  -e CLIENT_ID="$CLIENT_ID" \
  -e RUST_LOG=info \
  ghcr.io/gothenburgbitfactory/taskchampion-sync-server:0.7.1

# When done:
podman rm -f lazytask-sync-server
```

> ⚠️ **The image's entrypoint expects `CLIENT_ID` as an environment variable,
> *not* as a CLI flag.** Earlier versions of this doc and various blog posts
> show `--allow-client-id <uuid>` style invocations — those don't work with
> the 0.7.1 image and will fail with `eval: --allow-client-id: not found`.

## Sanity-check the server from the test suite

Once the server is up, you can run LazyTask's opt-in remote-sync integration
tests against it:

```bash
LAZYTASK_REMOTE_SYNC_URL=http://localhost:8810 \
LAZYTASK_REMOTE_CLIENT_ID=$(grep CLIENT_ID .env | cut -d= -f2) \
cargo test --test integration_remote_sync -- --nocapture
```

Both tests should pass:
- `remote_sync_round_trip` pushes a task from replica A → server → pulls
  into replica B, then propagates a `done` status back to A.
- `remote_sync_rejects_unauthorized_client_id` confirms the server returns
  HTTP 403 when a client UUID isn't in the allow-list.

## Sync from multiple devices

For cross-device sync, every device's LazyTask instance must agree on:

1. **Server URL** — reachable from each device (a LAN IP, Tailscale name,
   or public hostname; not just `localhost` once you leave the host machine).
2. **Client ID** — the **same** UUID on every device. You only generate it
   once.
3. **Encryption Secret** — the **same** secret string on every device. The
   server never sees the secret in plaintext (everything is encrypted
   client-side before upload), so the server can't help you recover it if
   you forget it.

Put those three identical values in each device's taskrc as
`sync.server.url`, `sync.server.client_id` and `sync.encryption_secret`, or
enter them in the LazyTask sync config modal (`Shift+S`), which keeps them
for the session only. Then press `s` to sync.

## Versions

The compose file pins `taskchampion-sync-server:0.7.1`. That's the version
LazyTask's own integration tests run against on every release. Newer tags
(`:latest`) may work but aren't validated by our test suite.
