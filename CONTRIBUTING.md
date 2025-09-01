# Contributing to DRCV

## Development

### Prerequisites
- Rust 1.70+
- SQLite 3

### Building
```bash
cargo build
cargo test
cargo run -- --help
```

### File Structure

```
drcv/
├── src/
│   ├── main.rs          # Main application and CLI
│   ├── db.rs            # SQLite database operations
│   ├── upload.rs        # Upload handling and chunking
│   ├── admin.rs         # Admin dashboard API
│   └── tunnel.rs        # Tunnel client for external access
├── tunnel-server/       # Cloudflare Workers tunnel server
│   ├── worker.js        # Main tunnel server logic
│   ├── setup.sh         # Setup script
│   └── deploy.sh        # Deployment script
├── static/
│   ├── index.html       # Upload interface
│   └── admin.html       # Admin dashboard
└── README.md
```

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

##### `GET /events`
Server-Sent Events stream for real-time updates.

### Tunnel Server Setup

DRCV includes a Cloudflare Workers-based tunnel server for external access.

#### Deploy Your Own Tunnel Server
```bash
cd tunnel-server
./setup.sh    # One-time setup
./deploy.sh   # Deploy updates
```

See [tunnel-server/README.md](tunnel-server/README.md) for detailed setup instructions.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

MIT License - see [LICENSE](LICENSE) for details.