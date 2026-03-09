# lnmarkets-cli

Command-line interface for [LN Markets](https://lnmarkets.com) API v3.

## Installation

```bash
cargo build --release
cp ./target/release/lnmarkets ~/.cargo/bin/
```

## Quick Start

1. Get API credentials from [LN Markets](https://lnmarkets.com/user/api)

2. Configure:
```bash
lnmarkets auth login
```

3. Trade:
```bash
lnmarkets market ticker
lnmarkets account balance
lnmarkets futures list
```

## Commands

### Market Data

```bash
lnmarkets market ticker              # BTC price, bid/ask, funding rate
lnmarkets market prices              # Index price history
lnmarkets market prices -l 100       # Last 100 prices
```

### Futures Trading

```bash
# List positions
lnmarkets futures list                        # Running positions (default)
lnmarkets futures list --status open          # Pending orders
lnmarkets futures list --status closed        # Closed trades

# Open position
lnmarkets futures open --side buy --quantity 1000 --leverage 10
lnmarkets futures open --side sell -q 5000 -l 5 --stoploss 95000
lnmarkets futures open -s buy -t limit --price 60000 -q 1000 -l 2

# Manage position
lnmarkets futures stoploss <id> --price 92000
lnmarkets futures takeprofit <id> --price 110000
lnmarkets futures add-margin <id> --amount 5000
lnmarkets futures cashin <id> --amount 1000
lnmarkets futures close <id>

# Cancel orders
lnmarkets futures cancel <id>
lnmarkets futures cancel-all
```

### Account

```bash
lnmarkets account info               # Account details + balance
lnmarkets account balance            # Balance only
lnmarkets account leaderboard        # Top traders
```

### Funding

```bash
# Deposit
lnmarkets funding deposit --amount 10000      # Lightning invoice
lnmarkets funding new-address                 # Bitcoin address
lnmarkets funding deposits                    # Deposit history

# Withdraw
lnmarkets funding withdraw --amount 5000 --invoice <bolt11>
lnmarkets funding withdraw-onchain --amount 100000 --address <addr>
lnmarkets funding withdrawals                 # Withdrawal history
```

### Auth

```bash
lnmarkets auth login                 # Set credentials
lnmarkets auth logout                # Clear credentials
lnmarkets auth status                # Check status
```

## Options

```bash
-o, --output <FORMAT>    table | json | json-pretty
--testnet                Use testnet
```

## Config

File: `~/.config/lnmarkets/config.toml`

```toml
[credentials]
api_key = "..."
api_secret = "..."
passphrase = "..."
```

Environment variables override config:
- `LNM_API_KEY`
- `LNM_API_SECRET`
- `LNM_API_PASSPHRASE`

## License

MIT
