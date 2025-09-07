# AGENT.md - AI Agent Context for DRCV v0.2.0

## Project Overview
DRCV is a resumable file upload server written in Rust with native Cloudflare Tunnel integration. It provides chunk-based uploads with automatic resume functionality, structured logging, and zero-config external access via Cloudflare tunnels.

## Architecture (v0.2.0)

### Core Components
- **Main Application** (`src/main.rs`): CLI interface, server coordination, and logging setup
- **Configuration** (`src/config.rs`): Centralized CLI arguments and app configuration
- **Upload Server** (`src/upload.rs`): Handles chunk-based file uploads with resume capability
- **Admin Dashboard** (`src/admin.rs`): Provides real-time monitoring and upload history
- **Database** (`src/db.rs`): SQLite-based upload tracking and session management
- **Utilities** (`src/utils.rs`): Common utility functions (time, string conversion)

### Modular Structure
- **Apps Module** (`src/apps/`): Application creation and server management
  - `upload.rs`: Upload app and server creation
  - `admin.rs`: Admin app and server creation with tunnel info
- **Tunnels Module** (`src/tunnels/`): Extensible tunnel provider system
  - `mod.rs`: Tunnel traits and provider factory
  - `cloudflare.rs`: Native cloudflared integration

### Key Features
- **Resumable uploads** with automatic chunk detection and resume
- **Native Cloudflare Tunnel integration** (no separate tunnel-server)
- **Structured logging** using standard Rust log ecosystem
- **Real-time monitoring** via Server-Sent Events
- **IP-based session isolation**
- **Heartbeat mechanism** for connection monitoring
- **Extensible tunnel providers** via trait system

### Network Architecture

#### Local Mode
- Upload server: `0.0.0.0:8080` (configurable)
- Admin server: `127.0.0.1:8081` (localhost only)

#### Cloudflare Tunnel Mode (New in v0.2.0)
1. DRCV auto-detects `cloudflared` installation
2. Creates or reuses a named Cloudflare tunnel with 6-char hash
3. Exposes local server via `https://{hash}.drcv.app`
4. All traffic routed through Cloudflare's secure tunnel
5. No UPnP or port forwarding required

### Security Model
- **IP Isolation**: Each client IP maintains separate upload sessions
- **Admin Isolation**: Admin interface only accessible on localhost (`127.0.0.1`)
- **File Size Limits**: Configurable maximum file size per upload
- **Automatic Cleanup**: Stale uploads removed after timeout
- **Heartbeat Monitoring**: Detects and cleans up disconnected clients
- **Cloudflare Security**: External access via Cloudflare's secure infrastructure

### Database Schema
```sql
CREATE TABLE uploads (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    filename     TEXT NOT NULL,
    size         INTEGER NOT NULL DEFAULT 0,
    status       TEXT NOT NULL,  -- 'init' | 'uploading' | 'complete' | 'disconnected'
    client_ip    TEXT NOT NULL,
    started_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    completed_at TEXT
);

CREATE TABLE clients (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    client_ip   TEXT NOT NULL,
    user_agent  TEXT NOT NULL,
    status      TEXT NOT NULL,  -- 'connected' | 'disconnected'
    last_seen   TEXT NOT NULL
);

CREATE TABLE kv_store (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);
```

### API Endpoints

#### Upload API (`port 8080`)
- `HEAD /upload?filename=<name>` - Check upload status
- `POST /upload` - Upload file chunk (multipart/form-data)
- `POST /heartbeat` - Keep session alive

#### Admin API (`port 8081`, localhost only)
- `GET /data?page=<n>&q=<search>` - Upload history with pagination
- `GET /clients` - Connected clients list
- `GET /tunnel` - Tunnel hostname information
- `GET /events` - Real-time updates via Server-Sent Events

### Configuration (Updated in v0.2.0)
- `--max-file-size`: Maximum file size (default: 100GiB)
- `--chunk-size`: Upload chunk size (default: 4MiB)
- `--upload-port`: Upload server port (default: 8080)
- `--admin-port`: Admin server port (default: 8081)
- `--upload-dir`: Upload directory (default: ./uploads)
- `--tunnel-domain`: Tunnel domain root (default: drcv.app)
- `--tunnel-provider`: Tunnel provider (default: cloudflare)
- `--verbose`/`-v`: Enable debug logging

### Logging System (New in v0.2.0)
DRCV uses the standard Rust logging ecosystem:
- **Log Levels**: ERROR, WARN, INFO, DEBUG, TRACE
- **Control Methods**:
  - `--verbose` flag: Enables DEBUG level
  - `RUST_LOG` environment variable: Full control (e.g., `RUST_LOG=debug`)
- **Default**: INFO level (shows important status messages)

### Tunnel Provider System (New in v0.2.0)
Extensible trait-based architecture for tunnel providers:
```rust
#[async_trait]
pub trait TunnelProvider: Send + Sync {
    async fn ensure(&self, db: &SqlitePool, config: &TunnelConfig) -> Result<Box<dyn TunnelManager>, TunnelError>;
}
```

Currently supported:
- **Cloudflare**: Direct `cloudflared` integration with auto-setup

### Static Files
- `src/static/index.html`: Upload interface with drag-drop and progress
- `src/static/admin.html`: Admin dashboard with real-time monitoring

### Dependencies (v0.2.0)
- **axum**: Web framework
- **sqlx**: SQLite database interface
- **tokio**: Async runtime
- **clap**: CLI argument parsing
- **log + env_logger**: Structured logging system
- **serde + serde_json**: Serialization
- **chrono**: Date/time handling

### Breaking Changes from v0.1.0
- Removed tunnel-server Cloudflare Workers implementation
- Changed log output format (now uses standard Rust logging)
- Removed UPnP and P2P DNS functionality
- Updated CLI options (removed `--tunnel`, added `--verbose`, etc.)