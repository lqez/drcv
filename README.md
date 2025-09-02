# DRCV - Resumable File Upload Server

A fast, secure, and resumable file upload server with tunneling capabilities for easy external access.

## Features

- ✅ **Resumable Uploads**: Chunk-based uploads with automatic resume on connection loss
- 🌐 **External Access**: Tunnel mode for ngrok-like external connectivity via `{hash}.drcv.app`
- 🔄 **Real-time Monitoring**: Live upload progress and admin dashboard
- 🛡️ **Security**: IP-based isolation, heartbeat monitoring, automatic cleanup
- 📱 **Multi-platform**: Works on macOS, Linux, and Windows
- 🔒 **Cloudflare Tunnel**: Automatic external access via `{hash}.drcv.app`

## Quick Start

### Local Mode
```bash
# Basic usage
cargo run

# Custom settings
cargo run -- --upload-port 9000 --max-file-size 10GiB --upload-dir ./my-uploads

# Access the uploader
open http://localhost:8080

# Access the admin dashboard  
open http://localhost:8081
```

### External Access via Cloudflare Tunnel (Recommended)

Use Cloudflare Tunnel to expose your local uploader under `https://{hash}.drcv.app` without opening inbound ports.

Quick outline:

1) Run the uploader locally
```bash
cargo run -- --upload-port 8080 --upload-dir ./uploads
```

2) Ensure Cloudflare Tunnel is installed and logged in on the receiver:
   - Install: see Cloudflare docs
   - One-time login: `cloudflared tunnel login`
   DRCV will auto-create and run a named tunnel on startup.

3) Share `https://{hash}.drcv.app` with senders. Files are saved to your local `--upload-dir`.

## Installation

### From Source
```bash
git clone https://github.com/your-username/drcv.git
cd drcv
cargo build --release
./target/release/drcv --help
```

### Binary Releases
Download from [GitHub Releases](https://github.com/your-username/drcv/releases)

## Command Line Options

```
Usage: drcv [OPTIONS]

Options:
  --max-file-size <SIZE>      Maximum file size (e.g., 100GiB, 10TB, 500MB) [default: 100GiB]
  --chunk-size <SIZE>         Upload chunk size (e.g., 4MiB, 1MiB, 512KB) [default: 4MiB]
  --upload-port <PORT>        Upload server port (use different ports if multiple instances behind NAT) [default: 8080]
  --admin-port <PORT>         Admin server port [default: 8081]
  --upload-dir <PATH>         Upload directory path [default: ./uploads]
  --cf-domain <DOMAIN>        Cloudflare Tunnel domain root [default: drcv.app]
  -h, --help                  Print help
```

## How It Works

### Local Mode
1. Server binds to `0.0.0.0:8080` for uploads and `127.0.0.1:8081` for admin
2. Clients upload files in chunks with automatic resume capability
3. Heartbeat mechanism prevents stale connections
4. Admin dashboard shows real-time progress

### Cloudflare Tunnel Mode
DRCV automatically creates and runs a Cloudflare Named Tunnel at startup (if `cloudflared` is installed and logged in). It generates or reuses a 6-char hash and exposes:

1. Receiver runs DRCV locally
2. DRCV spawns `cloudflared` to route `{hash}.drcv.app` to `localhost:8080`
3. Sender opens `https://{hash}.drcv.app` and uploads
4. Files are stored locally in the configured upload directory

## Network Access

On startup, DRCV prints a concise banner like:

```
DRCV is ready
  • Share: https://abc123.drcv.app
  • Admin: http://127.0.0.1:8081
  • Upload dir: ./uploads
```

## Security

### Built-in Protections
- **IP Isolation**: Each client IP gets separate upload sessions
- **File Size Limits**: Configurable maximum file size
- **Automatic Cleanup**: Stale uploads removed after timeout
- **Admin Localhost Only**: Admin interface only accessible locally
- **Heartbeat Monitoring**: Detects disconnected clients

### Network Security
- Admin dashboard (`8081`) only binds to `127.0.0.1`
- Upload server (`8080`) binds to `0.0.0.0` but can be firewalled
- External access is provided via Cloudflare Tunnel

<!-- Tunnel server section removed: replaced by automatic Cloudflare Tunnel -->

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, API documentation, and contribution guidelines.

## License

MIT License - see [LICENSE](LICENSE) for details.
