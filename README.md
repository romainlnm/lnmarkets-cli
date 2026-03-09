# lnmarkets-cli

Command-line interface for [LN Markets](https://lnmarkets.com) API v3.

Inspired by [kraken-cli](https://github.com/krakenfx/kraken-cli).

## Installation

```bash
cargo install --path . --locked
```

Or build from source:

```bash
cargo build --release
./target/release/lnmarkets --help
```

## Quick Start

1. Get your API credentials from [LN Markets](https://lnmarkets.com/user/api)

2. Configure credentials:
```bash
lnmarkets auth login
```

Or use environment variables:
```bash
export LNM_API_KEY="your-api-key"
export LNM_API_SECRET="your-api-secret"
export LNM_API_PASSPHRASE="your-passphrase"
```

3. Start trading:
```bash
lnmarkets market ticker
lnmarkets account balance
lnmarkets futures list
```

## Commands

### Market Data (Public)

```bash
lnmarkets market ticker                    # Current ticker (bid/ask/index)
lnmarkets market prices                    # OHLC price history
lnmarkets market prices -r h1 -l 24        # Last 24 hourly candles
lnmarkets market index                     # Index history
lnmarkets market info                      # Market limits and specs
lnmarkets market fees                      # Funding fee history
```

### Futures Trading (Authenticated)

```bash
lnmarkets futures list                     # List all positions
lnmarkets futures list --status running    # List running positions
lnmarkets futures get <trade-id>           # Get trade details

# Open positions
lnmarkets futures open --side buy --quantity 10000 --leverage 10
lnmarkets futures open --side sell -q 50000 -l 5 --stoploss 95000 --takeprofit 85000
lnmarkets futures open --side buy -t limit --price 90000 -q 10000 -l 2

# Manage positions
lnmarkets futures update <id> --stoploss 92000 --takeprofit 110000
lnmarkets futures add-margin <id> --amount 5000
lnmarkets futures cashin <id> --amount 1000
lnmarkets futures close <id>
lnmarkets futures close-all

# Cancel pending orders
lnmarkets futures cancel <id>
lnmarkets futures cancel-all
```

### Account (Authenticated)

```bash
lnmarkets account info                     # Account details
lnmarkets account balance                  # Current balance
lnmarkets account update --username "satoshi"
lnmarkets account leaderboard              # View leaderboard
lnmarkets account leaderboard -p daily     # Daily leaderboard
```

### Funding (Authenticated)

```bash
# Deposits
lnmarkets funding deposit --amount 10000   # Create Lightning invoice
lnmarkets funding new-address              # Generate Bitcoin address
lnmarkets funding addresses                # List Bitcoin addresses
lnmarkets funding deposits                 # Deposit history

# Withdrawals
lnmarkets funding withdraw --amount 5000 --invoice <bolt11>
lnmarkets funding withdraw-onchain --amount 100000 --address <btc-address>
lnmarkets funding withdrawals              # Withdrawal history
```

### Authentication

```bash
lnmarkets auth login                       # Configure credentials
lnmarkets auth logout                      # Remove credentials
lnmarkets auth status                      # Check auth status
lnmarkets auth whoami                      # Show config location
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
lnmarkets market ticker                    # Table format (default)
lnmarkets market ticker -o json            # JSON (single line)
lnmarkets market ticker -o json-pretty     # JSON (formatted)
```

## Configuration

Config file location: `~/.config/lnmarkets/config.toml`

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
