# AGENT.md - AI Agent Context for DRCV

## Project Overview
DRCV is a resumable file upload server written in Rust with tunneling capabilities for external access. It provides chunk-based uploads with automatic resume functionality and includes both local and tunnel modes.

## Architecture

### Core Components
- **Upload Server** (`src/upload.rs`): Handles chunk-based file uploads with resume capability
- **Admin Dashboard** (`src/admin.rs`): Provides real-time monitoring and upload history
- **Tunnel Client** (`src/tunnel.rs`): Connects to external tunnel server for P2P access
- **Database** (`src/db.rs`): SQLite-based upload tracking and session management
- **Main Application** (`src/main.rs`): CLI interface and server coordination

### Key Features
- Resumable uploads with automatic chunk detection
- Real-time progress monitoring via Server-Sent Events
- IP-based session isolation
- Heartbeat mechanism for connection monitoring
- UPnP automatic port forwarding
- Direct P2P tunneling via DNS (no proxy servers)

### Network Architecture

#### Local Mode
- Upload server: `0.0.0.0:8080` (configurable)
- Admin server: `127.0.0.1:8081` (localhost only)
- Automatic local IP detection for LAN access

#### Tunnel Mode
1. Client detects external IP and configures UPnP
2. Registers with tunnel server to get unique subdomain
3. DNS points directly to external IP for P2P connection
4. No traffic goes through proxy servers

### Security Model
- **IP Isolation**: Each client IP maintains separate upload sessions
- **Admin Isolation**: Admin interface only accessible on localhost
- **File Size Limits**: Configurable maximum file size per upload
- **Automatic Cleanup**: Stale uploads removed after timeout
- **Heartbeat Monitoring**: Detects and cleans up disconnected clients

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
```

### API Endpoints

#### Upload API (`port 8080`)
- `HEAD /upload?filename=<name>` - Check upload status
- `POST /upload` - Upload file chunk (multipart/form-data)
- `POST /heartbeat` - Keep session alive

#### Admin API (`port 8081`, localhost only)
- `GET /data?page=<n>&q=<search>` - Upload history with pagination
- `GET /events` - Real-time updates via Server-Sent Events

### Configuration
- `--max-file-size`: Maximum file size (default: 100GiB)
- `--upload-port`: Upload server port (default: 8080)
- `--admin-port`: Admin server port (default: 8081)
- `--upload-dir`: Upload directory (default: ./uploads)
- `--tunnel`: Enable tunnel mode
- `--tunnel-server`: Tunnel server URL (default: https://api.drcv.app)

### Multi-Instance Support
Multiple instances can run behind the same NAT by using different ports:
- Each instance gets a unique subdomain based on IP:port combination
- UPnP automatically forwards different ports to different hosts
- DNS resolution points directly to the correct host

### Static Files
- `static/index.html`: Upload interface with drag-drop and progress
- `static/admin.html`: Admin dashboard with real-time monitoring

### Tunnel Server
- Cloudflare Workers-based implementation in `tunnel-server/`
- Provides DNS-based P2P routing without proxying traffic
- Handles subdomain registration and IP mapping
- See `tunnel-server/README.md` for deployment details