# DRCV - Direct and Resumable Connection Vault

A fast, secure resumable file receiver with Cloudflare Tunnel integration for easy external access.

## Features

- **Resumable Uploads**: Chunk-based uploads with automatic resume on connection loss
- **Cloudflare Tunnel**: Zero-config external access via `{hash}.drcv.app`  
- **Real-time Dashboard**: Live upload progress monitoring
- **Security**: IP-based isolation, heartbeat monitoring, automatic cleanup
- **Cross-platform**: macOS, Linux, Windows support
- **Structured Logging**: Built-in log levels and RUST_LOG support

## Quick Start

```bash
# 1. Install and authenticate cloudflared (one-time setup)
# macOS: brew install cloudflared
# Linux: see https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/install-and-setup/installation/
cloudflared tunnel login

# 2. Run DRCV 
drcv

# Custom settings
drcv --upload-port 9000 --max-file-size 10GiB --upload-dir ./my-uploads

# With verbose logging
drcv --verbose
# or
RUST_LOG=debug drcv

# 3. DRCV will show:
# DRCV is ready
#   • Share: https://{hash}.drcv.app
#   • Admin: http://127.0.0.1:8081  
#   • Upload dir: ./uploads
```

Share the `https://{hash}.drcv.app` URL for external uploads.

Access: http://localhost:8080 (upload) | http://localhost:8081 (admin)

## Installation

### From Crates.io (Recommended)
```bash
cargo install drcv
drcv --help
```

### From Source
```bash
git clone https://github.com/lqez/drcv.git
cd drcv
cargo build --release
./target/release/drcv --help
```

### Binary Releases
Download from [GitHub Releases](https://github.com/lqez/drcv/releases)

## Command Line Options

```
Usage: drcv [OPTIONS]

Options:
  --max-file-size <SIZE>         Maximum file size [default: 100GiB]
  --chunk-size <SIZE>            Upload chunk size [default: 4MiB]  
  --upload-port <PORT>           Upload server port [default: 8080]
  --admin-port <PORT>            Admin server port [default: 8081]
  --upload-dir <PATH>            Upload directory [default: ./uploads]
  --tunnel-domain <DOMAIN>       Tunnel domain root [default: drcv.app]
  --tunnel-provider <PROVIDER>   Tunnel provider [default: cloudflare]
  -v, --verbose                  Show verbose configuration info
  -h, --help                     Print help
```

## Logging

DRCV uses structured logging with configurable levels:

```bash
# Standard logging (INFO level)
./drcv

# Verbose mode (DEBUG level)  
./drcv --verbose

# Custom log levels
RUST_LOG=error ./drcv     # Errors only
RUST_LOG=debug ./drcv     # Debug and above
RUST_LOG=trace ./drcv     # All logs
```

## How It Works

1. **Chunked Uploads**: Files split into resumable chunks
2. **Auto-Resume**: Interrupted uploads continue from last chunk
3. **Tunnel Integration**: `cloudflared` spawned automatically if available
4. **Real-time Updates**: SSE-based admin dashboard
5. **IP Isolation**: Separate sessions per client IP

## Security

- **Local Admin**: Admin interface bound to 127.0.0.1 only
- **File Limits**: Configurable size restrictions
- **Heartbeat Monitoring**: Automatic stale connection cleanup
- **Cloudflare Security**: External access via Cloudflare's secure tunnel

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and API documentation.

## License

MIT License - see [LICENSE](LICENSE) for details.
