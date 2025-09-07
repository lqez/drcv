# Contributing to DRCV

## Development

### Prerequisites
- Rust 1.70+
- SQLite 3
- (Optional) cloudflared for tunnel functionality

### Building
```bash
cargo build
cargo test
cargo run -- --help

# With logging
RUST_LOG=debug cargo run -- --verbose
```

### Project Structure

```
drcv/
├── src/
│   ├── main.rs              # Main application entry point
│   ├── config.rs            # CLI arguments and app configuration
│   ├── db.rs                # SQLite database operations
│   ├── upload.rs            # Upload handling and chunking logic
│   ├── admin.rs             # Admin dashboard API endpoints
│   ├── utils.rs             # Utility functions (time, string conversion)
│   ├── apps/                # App creation modules
│   │   ├── mod.rs           # Apps module declarations
│   │   ├── upload.rs        # Upload app and server creation
│   │   └── admin.rs         # Admin app and server creation
│   ├── tunnels/             # Tunnel provider implementations
│   │   ├── mod.rs           # Tunnel traits and provider factory
│   │   └── cloudflare.rs    # Cloudflare Tunnel implementation
│   └── static/              # Static web assets
│       ├── index.html       # Upload interface
│       └── admin.html       # Admin dashboard
└── Cargo.toml
```

### Dependencies
- **axum**: Web framework
- **sqlx**: SQLite database interface
- **tokio**: Async runtime
- **clap**: CLI argument parsing
- **log + env_logger**: Structured logging
- **serde + serde_json**: Serialization
- **chrono**: Date/time handling

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

### API Reference

#### Upload Endpoints

##### `HEAD /upload?filename=<name>`
Check upload status and get uploaded bytes.

**Response Headers:**
- `x-uploaded-bytes`: Number of bytes already uploaded

##### `POST /upload`
Upload file chunk.

**Request:** `multipart/form-data`
- `filename`: File name
- `chunk_index`: Current chunk index (0-based)  
- `total_chunks`: Total number of chunks
- `chunk`: Chunk data (binary)

**Response:** Upload ID (text)

##### `POST /heartbeat`
Keep upload session alive.

**Request JSON:**
```json
{
  "upload_ids": [123, 456, 789]
}
```

#### Admin Endpoints

##### `GET /data?page=<n>&q=<search>`
Get upload history with pagination and search.

##### `GET /clients`
Get connected clients list.

##### `GET /tunnel`
Get tunnel hostname information.

##### `GET /events`
Server-Sent Events stream for real-time updates.

### Logging

DRCV uses the standard Rust logging ecosystem:

```rust
use log::{info, warn, error, debug};

info!("Server started");
warn!("Upload timeout detected");  
error!("Database connection failed");
debug!("Processing chunk {}", chunk_id);
```

Log levels can be controlled via:
- `--verbose` flag (enables DEBUG level)
- `RUST_LOG` environment variable

### Tunnel System

The tunnel system uses a trait-based architecture:

```rust
#[async_trait]
pub trait TunnelProvider: Send + Sync {
    async fn ensure(&self, db: &SqlitePool, config: &TunnelConfig) -> Result<Box<dyn TunnelManager>, TunnelError>;
}
```

Currently supports:
- **Cloudflare Tunnel**: Direct `cloudflared` integration

### Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Add tests if applicable
5. Run `cargo check` and `cargo test`
6. Submit a pull request

### Code Style
- Use `cargo fmt` for formatting
- Follow Rust naming conventions
- Add logging at appropriate levels
- Document public APIs

## License

MIT License - see [LICENSE](LICENSE) for details.