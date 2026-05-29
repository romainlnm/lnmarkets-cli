# LN Markets CLI

![version](https://img.shields.io/badge/version-0.1.4-blue)
![license](https://img.shields.io/badge/license-MIT-green)
![platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey)

Command-line interface for trading Bitcoin futures on [LN Markets](https://lnmarkets.com).

Single binary. Lightning-native deposits and withdrawals. Live WebSocket streams. Built-in MCP server for Claude, Cursor, VS Code, Windsurf, and other MCP-compatible agents.

> *"Check the current BTC price and my LN Markets balance."*
>
> *"Open a small long position with 10x leverage and set a stop loss 5% below entry."*
>
> *"List my running positions and close any that are in profit."*

> [!CAUTION]
> Experimental software. Interacts with the live LN Markets exchange and can execute real trades with real Bitcoin.

## Installation

```bash
# macOS (Apple Silicon)
curl -L https://github.com/romainlnm/lnmarkets-cli/releases/latest/download/lnmarkets-macos-arm64 -o lnmarkets

# macOS (Intel) / Linux x64 / Linux ARM64
# → swap the binary name from the same releases URL

chmod +x lnmarkets && sudo mv lnmarkets /usr/local/bin/
```

Or build from source (requires [Rust](https://rustup.rs/)):

```bash
git clone https://github.com/romainlnm/lnmarkets-cli.git && cd lnmarkets-cli
cargo install --path . --locked
```

## Quick Start

Public market data — no credentials:

```bash
lnmarkets market ticker                # BTC price, bid/ask, funding rate
lnmarkets market ticker --watch        # live in-place updates via WS
lnmarkets market prices --limit 100    # price history
```

Authenticated — set `LNM_API_KEY` / `LNM_API_SECRET` / `LNM_API_PASSPHRASE` or run `lnmarkets auth login`:

```bash
lnmarkets account balance
lnmarkets futures list
lnmarkets futures open --side buy --quantity 1000 --leverage 10
```

All commands accept `-o json` or `-o json-pretty` for scripting.

## Live Streams

Tail the LN Markets stream API from the shell. One JSON event per line on stdout, status on stderr — pipe-friendly:

```bash
lnmarkets stream watch ticker
lnmarkets stream watch buckets | jq '.data.buckets[0]'
lnmarkets stream watch ohlc --resolution 1m

# Authenticated channels
lnmarkets stream watch positions       # isolated trades + cross position
lnmarkets stream watch orders          # cross order events
lnmarkets stream watch wallet          # deposits + withdrawals
lnmarkets stream watch all             # everything the API key permits
```

Auto-reconnects with exponential backoff. Ctrl+C exits cleanly.

In-place live refresh on existing commands:

```bash
lnmarkets market ticker --watch        # ticker table re-renders on each push
```

## Interactive TUI

Full-featured terminal dashboard with live data pushed from the stream — no polling delay.

```bash
lnmarkets tui                          # public + private if creds present
lnmarkets tui --no-stream              # REST polling only (debug)
```

| Tab | What you see |
|---|---|
| Dashboard | Balance, P&L, live BTC chart fed by the stream |
| Positions | Running positions; `c` close, `s` SL, `t` TP, `m` margin |
| Orders | Pending orders; `x` cancel |
| History | Closed trades |
| Funding | Lightning ⚡ + on-chain ₿ deposits / withdrawals |
| Recap | Fear & Greed, derivatives data, calendar |

Key shortcuts: `1-6` jump to tab, `o` open, `c`/`C` close one / all, `T` theme, `N` testnet, `L` login, `D` daemon, `?` help, `q` quit. Status bar shows the stream connection state (green = live, double dot when authenticated).

## MCP Server

Built-in [Model Context Protocol](https://modelcontextprotocol.io/) server over stdio.

> [!WARNING]
> MCP is local-first. Any connected agent uses the same API key permissions. Do not expose this server outside systems you control.

```bash
lnmarkets mcp                          # read-only (market, account)
lnmarkets mcp -s all                   # all services, dangerous calls require acknowledged=true
lnmarkets mcp -s all --allow-dangerous # autonomous mode
```

Configure your MCP client:

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

| Service | Auth | Risk | Tools |
|---|---|---|---|
| `market` | No | None | `get_ticker` |
| `account` | Yes | Read-only | `get_balance`, `list_trades` |
| `trade` | Yes | Orders | `open_trade`, `close_trade`, `update_stoploss`, `update_takeprofit`, `add_margin` |
| `funding` | Yes | Transfers | `deposit`, `withdraw` |

Default service set: `market,account`. Dangerous tools require `acknowledged=true` unless `--allow-dangerous`.

## Trading Daemon

Automated trading with multi-agent signal analysis on cross margin.

```bash
lnmarkets daemon --agents pattern,flow --interval 60          # dry run (default)
lnmarkets daemon --paper --agents pattern,macro,news,flow     # simulated trades
lnmarkets daemon --live --max-position 20 --leverage 10       # real money
```

Available agents (all use public APIs, no extra keys):

| Agent | Source | Signals |
|---|---|---|
| `pattern` | Binance Spot | RSI, MACD, EMA, Bollinger, ATR |
| `flow` | Binance Futures | Taker volume, OB imbalance, funding, L/S ratio, OI |
| `whale` | Hyperliquid | Position-weighted consensus from 8 verified top traders |
| `macro` | TradingView calendar | Economic surprise (actual vs forecast) |
| `news` | Multi-source RSS | Headline sentiment + geopolitical |

Signals are aggregated by weighted voting against `--min-confidence`. Position size scales with confidence above threshold (max 50% of `--max-position` per trade).

Exit strategies: take profit, stop loss, trailing stop. Anti-whipsaw guards: ATR volatility filter, reversal premium (+10% confidence), 5-min reversal cooldown, agent conflict detection.

Full options: `lnmarkets daemon --help`.

## Stats

Performance dashboard for daemon orders, filtered from your trade history.

```bash
lnmarkets stats                        # summary
lnmarkets stats --trades -l 20         # last 20 orders
```

Output: orders placed, gross/net P&L, fees, current cross position with unrealized P&L.

## Treasury (claw-cash)

The daemon can keep a target balance on the exchange by auto-funding from / withdrawing to a [claw-cash](https://github.com/ArkLabsHQ/claw-cash) hardware-enclave wallet.

```bash
lnmarkets daemon --live --treasury \
  --treasury-min 10000 --treasury-max 100000

lnmarkets daemon --paper --treasury-mock    # test without a real enclave
```

When the exchange balance crosses either bound, the daemon funds or withdraws via Lightning. Private keys never leave the enclave.

## Market Recap

24-48h BTC derivatives overview — no credentials.

```bash
lnmarkets recap
lnmarkets recap -o json
```

Pulls price action (Binance), funding / OI / L/S (Binance Futures), Fear & Greed (Alternative.me), and the economic calendar (TradingView). Failed sources are warned and skipped.

## Commands

10 MCP tools across 4 service groups. 32 CLI commands across 10 groups.

| Group | CLI | MCP | Auth | Description |
|---|---|---|---|---|
| `market` | 4 | 1 | No | Ticker, prices, index, funding |
| `account` | 4 | 2 | Yes | Balance, info, leaderboard, list trades |
| `futures` | 12 | 5 | Yes | Open, close, update, add margin, cross position |
| `funding` | 7 | 2 | Yes | Deposit, withdraw (Lightning + on-chain) |
| `auth` | 4 | — | No | Login, logout, status |
| `tui` | 1 | — | Opt | Interactive terminal dashboard |
| `stream` | 1 | — | Opt | Live WS stream tails for scripting |
| `daemon` | 1 | — | Opt | Automated trading + treasury |
| `stats` | 1 | — | No | Daemon performance |
| `recap` | 1 | — | No | 24-48h BTC overview |

<details>
<summary>Full command reference</summary>

**Market** (public): `ticker [--watch]`, `prices`, `index`, `info`, `funding`

**Account** (auth): `info`, `balance`, `update`, `leaderboard`

**Futures** (auth): `list`, `open`, `close`, `stoploss`, `takeprofit`, `add-margin`, `cashin`, `cancel`, `cancel-all`, `close-all`, `cross`

**Funding** (auth): `deposit`, `new-address`, `addresses`, `deposits`, `withdraw`, `withdraw-onchain`, `withdrawals`

**Auth**: `login`, `logout`, `status`, `whoami`

**Stream**: `watch <channel>` where channel is `ticker | lastprice | index | buckets | funding | ohlc | positions | orders | wallet | all`

**TUI / Daemon / Stats / Recap**: see relevant sections above and `--help`.

</details>

## API Keys & Configuration

Create API keys at [LN Markets API Settings](https://lnmarkets.com/user/api). Grant the minimum permissions you need.

```bash
# Environment (recommended for agents and scripts)
export LNM_API_KEY="..."
export LNM_API_SECRET="..."
export LNM_API_PASSPHRASE="..."

# Or interactive
lnmarkets auth login
```

Config file lives at `~/.config/lnmarkets/config.toml` (Linux) or `~/Library/Application Support/lnmarkets/config.toml` (macOS):

```toml
[credentials]
api_key = "..."
api_secret = "..."
passphrase = "..."

[settings]
network = "mainnet"          # or "testnet"
output_format = "table"      # table | json | json-pretty
```

Environment variables override config file values.

Global flags: `-o <format>` (table / json / json-pretty), `--testnet`.

## License

MIT
