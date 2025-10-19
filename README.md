### Telegram Post Aggregator

Forward new messages from selected Telegram channels/chats to one or more destination channels/chats. Built with `grammers-client`, it maintains a session, handles reconnection with backoff, and performs periodic health checks.

### What it does

- Loads `config.json` with a list of source chat IDs to listen to and target chat IDs to forward to
- Connects to Telegram using API credentials from environment variables
- Stores a persistent session in `first.session` after the first successful login
- On each incoming message from a configured source, forwards it to all configured targets
- Retries forwarding up to a small number of times on transient errors
- Runs a health check every 5 minutes and logs connection status

### Requirements

- Rust toolchain (if running natively)
- Telegram API credentials from [my.telegram.org](https://my.telegram.org)

### Configuration: `config.json`

- **sources**: array of Telegram chat/channel IDs to listen to
- **targets**: array of Telegram chat/channel IDs to forward to

Example:

```json
{
  "sources": [1234567890, 9876543210],
  "targets": [1122334455]
}
```

Notes:

- IDs are `i64` values. You can use either full IDs with the `-100` prefix or the bare numeric IDs without the prefix; both are accepted.
- To discover IDs, run the app once; it logs each message with `Chat: <id> From: <name>`, or use a Telegram ID bot.

### Environment variables (`.env`)

Create a `.env` file in the project root:

```env
TG_ID=123456            # Your Telegram API ID (integer)
TG_HASH=your_api_hash   # Your Telegram API hash
PHONE=+15551234567      # Your phone number for login
RUST_LOG=info           # Optional: logging level
```

First run will request an SMS/Telegram code and save `first.session` for subsequent runs.

Note: Accounts with 2FA password are not currently handled in the code path; if 2FA is enabled, sign‑in may fail.

### Run locally

1. Create `.env` as above and fill `config.json`
2. Build and run:

```bash
cargo run --release
```

3. Enter the login code when prompted. The session is saved to `first.session`.

### Run with Docker

This repository includes a `Dockerfile` and `docker-compose.yml`.

1. Make sure `.env`, `config.json`, and (after first run) `first.session` exist in the project root
2. Build and start:

```bash
docker compose up -d --build
```

`docker-compose.yml` mounts `config.json` and `first.session` into the container and sets `RUST_LOG=info`.

### Behavior details

- Exponential backoff reconnection policy with a cap
- Periodic health check every 5 minutes (`get_me`)
- Forwards only incoming messages from configured sources, skips your own outgoing messages

### Security

- Keep `.env` and `first.session` private; they grant access to your account

### License

See `LICENSE` in this repository.
