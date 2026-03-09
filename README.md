# lnm-cli

Command-line interface for [LN Markets](https://lnmarkets.com) API v3.

Inspired by [kraken-cli](https://github.com/krakenfx/kraken-cli).

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
./target/release/lnm --help
```

## Quick Start

1. Get your API credentials from [LN Markets](https://lnmarkets.com/user/api)

2. Configure credentials:
```bash
lnm auth login
```

Or use environment variables:
```bash
export LNM_API_KEY="your-api-key"
export LNM_API_SECRET="your-api-secret"
export LNM_API_PASSPHRASE="your-passphrase"
```

3. Start trading:
```bash
lnm market ticker
lnm account balance
lnm futures list
```

## Commands

### Market Data (Public)

```bash
lnm market ticker                    # Current ticker (bid/ask/index)
lnm market prices                    # OHLC price history
lnm market prices -r h1 -l 24        # Last 24 hourly candles
lnm market index                     # Index history
lnm market info                      # Market limits and specs
lnm market fees                      # Funding fee history
```

### Futures Trading (Authenticated)

```bash
lnm futures list                     # List all positions
lnm futures list --status running    # List running positions
lnm futures get <trade-id>           # Get trade details

# Open positions
lnm futures open --side buy --quantity 10000 --leverage 10
lnm futures open --side sell -q 50000 -l 5 --stoploss 95000 --takeprofit 85000
lnm futures open --side buy -t limit --price 90000 -q 10000 -l 2

# Manage positions
lnm futures update <id> --stoploss 92000 --takeprofit 110000
lnm futures add-margin <id> --amount 5000
lnm futures cashin <id> --amount 1000
lnm futures close <id>
lnm futures close-all

# Cancel pending orders
lnm futures cancel <id>
lnm futures cancel-all
```

### Account (Authenticated)

```bash
lnm account info                     # Account details
lnm account balance                  # Current balance
lnm account update --username "satoshi"
lnm account leaderboard              # View leaderboard
lnm account leaderboard -p daily     # Daily leaderboard
```

### Funding (Authenticated)

```bash
# Deposits
lnm funding deposit --amount 10000   # Create Lightning invoice
lnm funding new-address              # Generate Bitcoin address
lnm funding addresses                # List Bitcoin addresses
lnm funding deposits                 # Deposit history

# Withdrawals
lnm funding withdraw --amount 5000 --invoice <bolt11>
lnm funding withdraw-onchain --amount 100000 --address <btc-address>
lnm funding withdrawals              # Withdrawal history
```

### Authentication

```bash
lnm auth login                       # Configure credentials
lnm auth logout                      # Remove credentials
lnm auth status                      # Check auth status
lnm auth whoami                      # Show config location
```

## Global Options

```bash
--output, -o <FORMAT>    Output format: table, json, json-pretty
--testnet                Use testnet instead of mainnet
--help, -h               Show help
--version, -V            Show version
```

## Output Formats

```bash
lnm market ticker                    # Table format (default)
lnm market ticker -o json            # JSON (single line)
lnm market ticker -o json-pretty     # JSON (formatted)
```

## Configuration

Config file location: `~/.config/lnm/config.toml`

```toml
[credentials]
api_key = "your-api-key"
api_secret = "your-api-secret"
passphrase = "your-passphrase"

[settings]
network = "mainnet"  # or "testnet"
output_format = "table"
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `LNM_API_KEY` | API key (overrides config) |
| `LNM_API_SECRET` | API secret (overrides config) |
| `LNM_API_PASSPHRASE` | Passphrase (overrides config) |

## API Endpoints

- **Mainnet**: `https://api.lnmarkets.com/v3`
- **Testnet**: `https://api.testnet4.lnmarkets.com/v3`

## Rate Limits

- Authenticated requests: 1 request/second
- Public requests: 30 requests/minute

## License

MIT
