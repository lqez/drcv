# DRCV - Resumable File Upload Server

A fast, secure, and resumable file upload server with tunneling capabilities for easy external access.

## Features

- ✅ **Resumable Uploads**: Chunk-based uploads with automatic resume on connection loss
- 🌐 **External Access**: Tunnel mode for ngrok-like external connectivity via `{hash}.drcv.app`
- 🔄 **Real-time Monitoring**: Live upload progress and admin dashboard
- 🛡️ **Security**: IP-based isolation, heartbeat monitoring, automatic cleanup
- 📱 **Multi-platform**: Works on macOS, Linux, and Windows
- 🏠 **Local Network**: Automatic local IP detection for LAN access
- 🔧 **UPnP**: Automatic port forwarding for NAT environments

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

### Tunnel Mode (External Access)
```bash
# Enable tunnel for external access
cargo run -- --tunnel

# The server will display something like:
# 🎉 Tunnel established: https://abc123.drcv.app
# 📁 Share this URL for others to upload files to your computer!
```

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
  --upload-port <PORT>        Upload server port (use different ports if multiple instances behind NAT) [default: 8080]
  --admin-port <PORT>         Admin server port [default: 8081]
  --upload-dir <PATH>         Upload directory path [default: ./uploads]
  --tunnel                    Enable tunnel mode to expose server via drcv.app subdomain
  --tunnel-server <URL>       Tunnel server URL [default: https://api.drcv.app]
  -h, --help                  Print help
```

## How It Works

### Local Mode
1. Server binds to `0.0.0.0:8080` for uploads and `127.0.0.1:8081` for admin
2. Clients upload files in chunks with automatic resume capability
3. Heartbeat mechanism prevents stale connections
4. Admin dashboard shows real-time progress

### Tunnel Mode
1. Client detects external IP and sets up UPnP port forwarding
2. Registers with tunnel server to get unique subdomain (`{hash}.drcv.app`)
3. DNS points directly to your external IP for P2P connection
4. No proxy servers = faster uploads and lower costs

## Network Access

When you run DRCV, it will show all available access methods:

```
▶️ drcv uploader running on:
   • http://0.0.0.0:8080 (all interfaces)
   • http://192.168.1.100:8080 (local network)
   • http://10.0.0.50:8080 (local network)

▶️ drcv admin running on http://127.0.0.1:8081 (localhost only)

🎉 Tunnel established: https://abc123.drcv.app
```

- **Localhost**: `http://localhost:8080`
- **Local Network**: `http://192.168.1.100:8080` (other devices on same WiFi/LAN)
- **External**: `https://abc123.drcv.app` (tunnel mode only)

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
- Tunnel mode uses direct P2P connections (no proxy server)

## Tunnel Server Setup

DRCV includes a Cloudflare Workers-based tunnel server for external access. See [tunnel-server/README.md](tunnel-server/README.md) for detailed setup instructions.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, API documentation, and contribution guidelines.

## License

MIT License - see [LICENSE](LICENSE) for details.

