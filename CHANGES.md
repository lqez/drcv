# Changelog

## v0.2.0 (Current)

### Major Improvements
- **Modular Architecture**: Refactored codebase into clean module structure
  - `apps/` - Application creation and server management
  - `tunnels/` - Extensible tunnel provider system
  - `config.rs` - Centralized configuration management
  - `utils.rs` - Common utility functions

- **Direct Cloudflare Integration**: Replaced tunnel-server with native `cloudflared` integration
  - Auto-creates and manages Cloudflare Named Tunnels
  - Zero-config external access via `{hash}.drcv.app`
  - Automatic installation guides for missing dependencies

- **Standard Logging System**: Migrated from `println!`/`eprintln!` to structured logging
  - Built-in log levels (ERROR, WARN, INFO, DEBUG)
  - `--verbose` flag support
  - `RUST_LOG` environment variable support
  - Better debugging and production monitoring

### New Features
- **Extended CLI Options**:
  - `--verbose` / `-v`: Enable debug logging
  - `--tunnel-provider`: Select tunnel provider (currently: cloudflare)
  - `--tunnel-domain`: Configure tunnel domain root
- **Improved Error Handling**: Better error messages and recovery
- **Code Quality**: Eliminated code duplication and improved maintainability

### Technical Changes
- Trait-based tunnel provider system for extensibility
- Centralized app creation with proper lifecycle management
- Removed deprecated tunnel-server Cloudflare Workers implementation
- Enhanced graceful shutdown handling
- Improved async task management

### Breaking Changes
- Removed tunnel-server directory and related setup scripts
- Changed some internal API structures (affects contributors only)
- Log output format changed (now uses standard Rust logging)

## v0.1.0
- Initial release
- Basic resumable upload functionality
- Admin dashboard
- Tunnel mode for external access
- UPnP port forwarding
- Multi-instance NAT support