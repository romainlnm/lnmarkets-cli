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

## Commands

10 MCP tools across 4 service groups. 26 CLI commands across 5 groups.

| Group | CLI Commands | MCP Tools | Auth | Description |
|-------|--------------|-----------|------|-------------|
| market | 4 | 1 | No | Ticker, prices, index, funding rate |
| account | 4 | 2 | Yes | Balance, info, leaderboard, list trades |
| futures | 11 | 5 | Yes | Open, close, update, add margin |
| funding | 7 | 2 | Yes | Deposit, withdraw (Lightning & on-chain) |
| auth | 4 | — | No | Login, logout, status |

7 tools are marked `dangerous` (orders, deposits, withdrawals).

<details>
<summary>Full command reference</summary>

### Market Data (Public)

| Command | Description |
|---------|-------------|
| `lnmarkets market ticker` | BTC price, bid/ask, funding rate |
| `lnmarkets market prices [--limit N]` | Index price history |
| `lnmarkets market index [--from TS] [--to TS]` | Index history with time range |
| `lnmarkets market info` | Full market information |
| `lnmarkets market funding` | Current funding rate |

### Account (Private)

| Command | Description |
|---------|-------------|
| `lnmarkets account info` | Account details + balance |
| `lnmarkets account balance` | Balance only |
| `lnmarkets account update [--username U] [--show-leaderboard]` | Update account settings |
| `lnmarkets account leaderboard [--period P] [--limit N]` | Top traders |

### Futures Trading (Private)

| Command | Description |
|---------|-------------|
| `lnmarkets futures list [--status S] [--limit N]` | List trades (open, running, closed, canceled) |
| `lnmarkets futures open --side S --quantity Q [--leverage L] [--type T] [--price P] [--stoploss SL] [--takeprofit TP]` | Open position |
| `lnmarkets futures close <ID>` | Close running position |
| `lnmarkets futures stoploss <ID> --price P` | Update stop loss |
| `lnmarkets futures takeprofit <ID> --price P` | Update take profit |
| `lnmarkets futures add-margin <ID> --amount A` | Add margin to position |
| `lnmarkets futures cashin <ID> --amount A` | Partial close (cash in profit) |
| `lnmarkets futures cancel <ID>` | Cancel pending order |
| `lnmarkets futures cancel-all` | Cancel all pending orders |
| `lnmarkets futures close-all` | Close all running trades |

### Funding (Private)

| Command | Description |
|---------|-------------|
| `lnmarkets funding deposit --amount A` | Generate Lightning invoice |
| `lnmarkets funding new-address` | Generate Bitcoin deposit address |
| `lnmarkets funding addresses` | List deposit addresses |
| `lnmarkets funding deposits [--limit N]` | Deposit history |
| `lnmarkets funding withdraw --amount A --invoice I` | Withdraw via Lightning |
| `lnmarkets funding withdraw-onchain --amount A --address ADDR` | Withdraw on-chain |
| `lnmarkets funding withdrawals [--limit N]` | Withdrawal history |

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
