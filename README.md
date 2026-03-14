# qsl_rs

Rust rewrite of [query-store-links](https://github.com/query-store-links/qsl) — a REST API that resolves Microsoft Store product info and direct package download links.

Built with [storelib_rs](https://github.com/query-store-links/storelib_rs).

## API

### `POST /api/links/resolve-all`

**Request body** (JSON, all fields optional except `ProductInput`):

```json
{
  "ProductInput": "9NBLGGH4NNS1",
  "Locale": "en-US",
  "Market": "US",
  "IdentifierType": "ProductId"
}
```

| Field | Default | Description |
|---|---|---|
| `ProductInput` | — | Product ID, package family name, Xbox title ID, etc. IDs starting with `xp` are resolved via the winget package manifest endpoint instead of Display Catalog. |
| `Locale` | `en-US` | BCP-47 locale for product info |
| `Market` | `US` | Two-letter market code |
| `IdentifierType` | `ProductId` | `ProductId` · `PackageFamilyName` · `ContentId` · `XboxTitleId` · `LegacyWindowsPhoneProductId` · `LegacyWindowsStoreProductId` · `LegacyXboxProductId` |

**Response**:

```json
{
  "ProductId": "9NBLGGH4NNS1",
  "AppInfo": {
    "Name": "App name",
    "Publisher": "Publisher name",
    "Description": "...",
    "CategoryId": "...",
    "ProductId": "9NBLGGH4NNS1"
  },
  "AppxPackages": [
    {
      "FileName": "Package.msixbundle",
      "FileLink": "https://tlu.dl.delivery.mp.microsoft.com/...",
      "FileSize": "123.4 MB"
    }
  ],
  "NonAppxPackages": null,
  "Errors": null
}
```

`AppxPackages` is populated for standard Store products; `NonAppxPackages` for `xp`-prefixed winget IDs. On failure, `Errors` contains a list of error messages and the response is still HTTP 200.

---

## Deployment

### Docker (recommended)

Pull and run the pre-built image:

```sh
docker run -d \
  -p 5236:5236 \
  -e ALLOWED_ORIGINS="https://*.yourdomain.com" \
  --restart unless-stopped \
  ghcr.io/query-store-links/qsl_rs:latest
```

### Docker Compose

```sh
git clone https://github.com/query-store-links/qsl_rs.git
cd qsl_rs
docker compose up -d
```

Edit `docker-compose.yml` to change the port or `ALLOWED_ORIGINS` before starting.

### Build from source

Requirements: Rust 1.75+, OpenSSL dev headers.

```sh
git clone https://github.com/query-store-links/qsl_rs.git
cd qsl_rs
cargo build --release
./target/release/qsl_rs --host 0.0.0.0 --port 5236
```

---

## Configuration

All options can be set via CLI flags or environment variables:

| Flag | Env | Default | Description |
|---|---|---|---|
| `-h, --host` | `HOST` | `0.0.0.0` | Bind address |
| `-p, --port` | `PORT` | `5236` | Bind port |
| `--allowed-origins` | `ALLOWED_ORIGINS` | `https://*.krnl64.win` | Comma-separated list of allowed CORS origins. Supports `*.example.com` wildcard subdomains. |
| `--dev` | `DEV` | off | Allow loopback origins (`localhost`, `127.0.0.1`) — useful for local frontend development |
| `--log-level` | `LOG_LEVEL` | `info` | `error` · `warn` · `info` · `debug` · `trace` |

```sh
# Example: custom origins and dev mode
./qsl_rs --port 3000 --allowed-origins "https://app.example.com,https://*.example.com" --dev
```

---

## License

[GPL-3.0-only](LICENSE)
