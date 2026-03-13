# LN Markets CLI

![version](https://img.shields.io/badge/version-0.1.0-blue)
![license](https://img.shields.io/badge/license-MIT-green)
![platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey)

Command-line interface for trading Bitcoin futures on [LN Markets](https://lnmarkets.com).

Built-in MCP server. Lightning-native deposits and withdrawals. Single binary.

Works with Claude, Cursor, VS Code, Windsurf, and other MCP-compatible agents.

Try these with your AI agent:

> *"Check the current BTC price and my LN Markets balance."*

> *"Open a small long position with 10x leverage and set a stop loss 5% below entry."*

> *"List my running positions and close any that are in profit."*

---

> [!CAUTION]
> Experimental software. Interacts with the live LN Markets exchange and can execute real trades with real Bitcoin. Use with caution.

## Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [MCP Server](#mcp-server)
- [Trading Daemon](#trading-daemon)
- [Commands](#commands)
- [API Keys & Configuration](#api-keys--configuration)
- [License](#license)

## Installation

Single binary, no runtime dependencies.

### Build from source

Requires [Rust](https://rustup.rs/).

```bash
git clone https://github.com/romainlnm/lnmarkets-cli.git
cd lnmarkets-cli
cargo install --path . --locked
```

Or build and copy manually:

```bash
cargo build --release
cp ./target/release/lnmarkets ~/.cargo/bin/
```

Verify it works:

```bash
lnmarkets market ticker
```

## Quick Start

Public market data requires no credentials:

```bash
lnmarkets market ticker                    # BTC price, bid/ask, funding rate
lnmarkets market ticker -o json            # JSON output
lnmarkets market prices --limit 100        # Price history
```

With authentication:

```bash
export LNM_API_KEY="your-key"
export LNM_API_SECRET="your-secret"
export LNM_API_PASSPHRASE="your-passphrase"

lnmarkets account balance -o json
lnmarkets futures list -o json
lnmarkets futures open --side buy --quantity 1000 --leverage 10 -o json
```

For humans (table output, interactive setup):

```bash
lnmarkets auth login
lnmarkets account balance
lnmarkets futures list
```

## MCP Server

`lnmarkets` includes a built-in [Model Context Protocol](https://modelcontextprotocol.io/) server over stdio. No subprocess wrappers needed.

> [!WARNING]
> MCP is local-first and designed for your own machine. Any agent connected to this MCP server uses the same configured LN Markets account and API key permissions. Do not expose or share this server outside systems you control.

```bash
lnmarkets mcp                              # read-only (market, account)
lnmarkets mcp -s all                       # all services, dangerous calls require acknowledged=true
lnmarkets mcp -s all --allow-dangerous     # all services, no per-call confirmation
lnmarkets mcp -s market,trade              # specific services
```

Configure your MCP client (Claude Desktop, Cursor, VS Code, etc.):

```json
{
  "mcpServers": {
    "lnmarkets": {
      "command": "lnmarkets",
      "args": ["mcp", "-s", "all"]
    }
  }
}
```

With environment variables for credentials:

```json
{
  "mcpServers": {
    "lnmarkets": {
      "command": "lnmarkets",
      "args": ["mcp", "-s", "all"],
      "env": {
        "LNM_API_KEY": "your-key",
        "LNM_API_SECRET": "your-secret",
        "LNM_API_PASSPHRASE": "your-passphrase"
      }
    }
  }
}
```

### Service groups

| Service | Auth | Risk | Tools |
|---------|------|------|-------|
| `market` | No | None | `get_ticker` |
| `account` | Yes | Read-only | `get_balance`, `list_trades` |
| `trade` | Yes | Orders (dangerous) | `open_trade`, `close_trade`, `update_stoploss`, `update_takeprofit`, `add_margin` |
| `funding` | Yes | Transfers (dangerous) | `deposit`, `withdraw` |

Default: `market,account` (read-only).

Dangerous tools carry the `[DANGEROUS: requires acknowledged=true]` annotation. In guarded mode (default), dangerous calls must include `acknowledged=true`. In autonomous mode (`--allow-dangerous`), this requirement is disabled.

### Output format

All tools return JSON. On success:

```json
{
  "content": [{"type": "text", "text": "{...}"}]
}
```

On error:

```json
{
  "content": [{"type": "text", "text": "Error: ..."}],
  "isError": true
}
```

## Trading Daemon

Automated trading with multi-agent signal analysis. Runs continuously, combining signals from technical analysis, economic calendar, and news sentiment.

```bash
lnmarkets daemon --agents pattern,macro,news,flow --interval 60
```

> [!CAUTION]
> Dry run mode is enabled by default. Use `--paper` to test with simulated trades, or `--live` for real trading. Start with small position sizes.

### Trading Modes

| Mode | Flag | Description |
|------|------|-------------|
| Dry run | (default) | Analysis only, no trades |
| Paper | `--paper` | Simulated trades with real prices, tracks P&L |
| Live | `--live` | Real trades with real sats |

### Agents

| Agent | Data Source | Signals |
|-------|-------------|---------|
| `pattern` | Binance Spot API | RSI, EMA crossover, Bollinger Bands |
| `macro` | ForexFactory API | Pre/post event warnings (FOMC, CPI, NFP) |
| `news` | RSS feeds | Sentiment analysis from crypto news |
| `flow` | Binance Futures API | Order book imbalance, funding rate, OI, L/S ratio |

All data sources are **public APIs** — no API keys required.

<details>
<summary>Data source details</summary>

| Agent | Endpoint | Data |
|-------|----------|------|
| `pattern` | `api.binance.com/api/v3/klines` | BTC/USDT price candles |
| `macro` | `nfs.faireconomy.media/ff_calendar_thisweek.json` | Economic calendar events |
| `news` | CoinDesk, Cointelegraph, Bitcoin Magazine, Decrypt, CryptoSlate | RSS headlines |
| `flow` | `fapi.binance.com/fapi/v1/depth`, `/fundingRate`, `/openInterest` | Futures market data |

</details>

### Options

```bash
lnmarkets daemon [OPTIONS]

Options:
  -a, --agents <AGENTS>      Agents to enable [default: pattern]
  -i, --interval <SECS>      Analysis interval in seconds [default: 60]
      --paper                Paper trading (simulated with real prices)
      --live                 Live trading (real sats!)
      --min-confidence <N>   Minimum confidence to act (0.0-1.0) [default: 0.7]
      --max-position <SATS>  Maximum position size in sats [default: 100000]
```

### Examples

```bash
# Dry run: analysis only
lnmarkets daemon --agents pattern,flow --interval 30

# Paper trading: test strategies with real prices
lnmarkets daemon --paper --agents pattern,macro,news,flow --min-confidence 0.5

# Live trading: real sats (use with caution!)
lnmarkets daemon --live --agents pattern,flow --max-position 10000
```

### Sample Output

```
Starting LN Markets trading daemon...
  Mode: PAPER TRADING
  Interval: 30s
  Min confidence: 50%
  Agents: ["pattern", "flow"]

[14:01:52] Analyzing...
  ▲ [pattern] LONG (56%): BTC $73493 | RSI 57.1 | EMA9 > EMA21
  ● [flow] NEUTRAL (50%): OB 89%↑ | FR 0.43bps | L/S 0.89
  → ACTION: BUY 5000 sats (56% confidence)
  [PAPER OPEN] #1 BUY 5000 sats @ $73493
  [PAPER] Open: 1 | Closed: 0 | W/L: 0/0 (0%) | P&L: +0 sats
```

## Commands

10 MCP tools across 4 service groups. 27 CLI commands across 6 groups.

| Group | CLI Commands | MCP Tools | Auth | Description |
|-------|--------------|-----------|------|-------------|
| market | 4 | 1 | No | Ticker, prices, index, funding rate |
| account | 4 | 2 | Yes | Balance, info, leaderboard, list trades |
| futures | 11 | 5 | Yes | Open, close, update, add margin |
| funding | 7 | 2 | Yes | Deposit, withdraw (Lightning & on-chain) |
| auth | 4 | — | No | Login, logout, status |
| daemon | 1 | — | Optional | Automated trading with agents |

7 tools are marked `dangerous` (orders, deposits, withdrawals).

<details>
<summary>Full command reference</summary>

### Market Data (Public)

| Command | Description |
|---------|-------------|
| `lnmarkets market ticker` | BTC price, bid/ask, funding rate |
| `lnmarkets market prices [--limit 100]` | Index price history |
| `lnmarkets market index [--from 1704067200] [--to 1704153600]` | Index history with time range |
| `lnmarkets market info` | Full market information |
| `lnmarkets market funding` | Current funding rate |

### Account (Private)

| Command | Description |
|---------|-------------|
| `lnmarkets account info` | Account details + balance |
| `lnmarkets account balance` | Balance only |
| `lnmarkets account update [--username satoshi] [--show-leaderboard]` | Update account settings |
| `lnmarkets account leaderboard [--period daily] [--limit 10]` | Top traders |

### Futures Trading (Private)

| Command | Description |
|---------|-------------|
| `lnmarkets futures list [--status running] [--limit 50]` | List trades (open, running, closed, canceled) |
| `lnmarkets futures open --side buy --quantity 1000 [--leverage 10] [--type market] [--price 50000] [--stoploss 48000] [--takeprofit 55000]` | Open position |
| `lnmarkets futures close <ID>` | Close running position |
| `lnmarkets futures stoploss <ID> --price 48000` | Update stop loss |
| `lnmarkets futures takeprofit <ID> --price 55000` | Update take profit |
| `lnmarkets futures add-margin <ID> --amount 1000` | Add margin to position |
| `lnmarkets futures cashin <ID> --amount 500` | Partial close (cash in profit) |
| `lnmarkets futures cancel <ID>` | Cancel pending order |
| `lnmarkets futures cancel-all` | Cancel all pending orders |
| `lnmarkets futures close-all` | Close all running trades |

### Funding (Private)

| Command | Description |
|---------|-------------|
| `lnmarkets funding deposit --amount 10000` | Generate Lightning invoice |
| `lnmarkets funding new-address` | Generate Bitcoin deposit address |
| `lnmarkets funding addresses` | List deposit addresses |
| `lnmarkets funding deposits [--limit 20]` | Deposit history |
| `lnmarkets funding withdraw --amount 5000 --invoice lnbc...` | Withdraw via Lightning |
| `lnmarkets funding withdraw-onchain --amount 100000 --address bc1q...` | Withdraw on-chain |
| `lnmarkets funding withdrawals [--limit 20]` | Withdrawal history |

### Auth

| Command | Description |
|---------|-------------|
| `lnmarkets auth login` | Configure API credentials (interactive) |
| `lnmarkets auth logout` | Remove stored credentials |
| `lnmarkets auth status` | Check authentication status |
| `lnmarkets auth whoami` | Show credential file location |

</details>

## API Keys & Configuration

Authenticated commands require LN Markets API credentials. Public market data works without credentials.

### Getting API keys

Create API keys at [LN Markets API Settings](https://lnmarkets.com/user/api). Grant the minimum permissions your workflow needs.

### Environment variables (recommended for agents)

```bash
export LNM_API_KEY="your-key"
export LNM_API_SECRET="your-secret"
export LNM_API_PASSPHRASE="your-passphrase"
```

### Config file (for humans)

Store credentials in `~/.config/lnmarkets/config.toml`:

```toml
[credentials]
api_key = "your-api-key"
api_secret = "your-api-secret"
passphrase = "your-passphrase"

[settings]
network = "mainnet"  # or "testnet"
output_format = "table"  # table, json, json-pretty
```

Or use the interactive setup: `lnmarkets auth login`.

### Credential resolution

Highest precedence first:

1. Environment variables (`LNM_API_KEY`, `LNM_API_SECRET`, `LNM_API_PASSPHRASE`)
2. Config file (`~/.config/lnmarkets/config.toml`)

### Global options

```bash
-o, --output <FORMAT>    table | json | json-pretty (default: table)
--testnet                Use testnet instead of mainnet
```

## License

MIT
