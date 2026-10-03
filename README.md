# zwilling-tracker

Self-hosted web app for tracking what's inside your Zwilling food storage containers. Scan the QR code on a container with your phone's camera to see what's in it, or record what you just put in. You can also search everything you've stored by contents or container ID.

QR codes look like `zwilling://zwillingapp/food-storage/in/?tc=12LB02&s=m&cc=1VST` (`tc` product code, `s` size, `cc` container code). Contents are stored per container code.

## Run it

```sh
docker run -d -p 3000:3000 -e DATABASE_URL=postgres://user:pass@host:5432/zwilling ghcr.io/graytonio/zwilling-tracker:latest
```

Browsers only allow camera access on `localhost` or over HTTPS, so put it behind a TLS reverse proxy (or e.g. `tailscale serve`) to use it from a phone.

## Develop

Requires [mise](https://mise.jdx.dev) and Docker.

| Command | Does |
|---|---|
| `mise run dev` | App + Postgres in Docker Compose, rebuilding on change |
| `mise run run` | Run natively against the Compose Postgres |
| `mise run test` | Tests (starts Postgres) |
| `mise run lint` | `cargo fmt --check` + clippy |

## Release

Bump `version` in `Cargo.toml`, commit, then `git tag vX.Y.Z && git push --tags`. CI publishes a multi-arch image to GHCR and creates a GitHub release.

## Third-party

`src/jsQR.min.js` is [jsQR](https://github.com/cozmo/jsQR) 1.4.0, Apache-2.0.
