# LN Markets CLI

![version](https://img.shields.io/badge/version-0.2.1-blue)
![license](https://img.shields.io/badge/license-MIT-green)
![platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey)

Command-line interface for trading Bitcoin futures on [LN Markets](https://lnmarkets.com).

Single binary. Live WebSocket streams. Built-in MCP server for Claude, Cursor, VS Code, Windsurf, and other MCP-compatible agents. Optional LLM-driven trading daemon.

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

# Linux x64 / Linux ARM64 / macOS Intel — swap the binary name from the same releases URL

chmod +x lnmarkets && sudo mv lnmarkets /usr/local/bin/
```

Or from source (requires [Rust](https://rustup.rs/)):

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

lnmarkets stream watch positions       # authenticated
lnmarkets stream watch orders          # authenticated
lnmarkets stream watch wallet          # authenticated
lnmarkets stream watch all             # everything the API key permits
```

Auto-reconnects with exponential backoff. Ctrl+C exits cleanly.

In-place live refresh:

```bash
lnmarkets market ticker --watch        # ticker table re-renders on each push
```

## Interactive TUI

Terminal dashboard with live data pushed from the stream — no polling delay.

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

Shortcuts: `1-6` jump to tab, `o` open, `c`/`C` close one / all, `T` theme, `N` testnet, `L` login, `D` daemon, `?` help, `q` quit. Status bar shows the stream connection state (green = live, double dot when authenticated).

## Alerts

Plain-English price + funding rules that fire OS-native notifications when crossed. Watcher subscribes to the public ticker stream — no polling.

```bash
lnmarkets alert add "price > 200000"
lnmarkets alert add "funding > 0.05%"
lnmarkets alert add "funding flips negative"

lnmarkets alert list
lnmarkets alert remove <id>
lnmarkets alert watch                  # foreground, Ctrl+C to exit
```

Rules fire on **threshold crossing**, not while the condition holds — no spam. Stored at `<config-dir>/alerts.toml`. Cross-platform via `notify-rust` (macOS / Linux / Windows).

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

Default services: `market,account`. Dangerous tools require `acknowledged=true` unless `--allow-dangerous`.

## Trading Daemon

LLM-driven trading on cross margin. Claude is the sole decider — every cycle, the daemon collects market data and asks Claude for an action.

> [!CAUTION]
> Dry run is the default. Use `--paper` for simulated trades, `--live` for real money. Start small.

```bash
export ANTHROPIC_API_KEY=...                       # required
export ANTHROPIC_MODEL=claude-opus-4-8             # optional, defaults to this

lnmarkets daemon --collectors pattern,flow         # dry run
lnmarkets daemon --paper --collectors pattern,flow,whale,macro,news
lnmarkets daemon --live --max-position 20 --leverage 10 --max-daily-loss 5000
```

### How it works

Each cycle (`--interval` seconds, default 60):

1. Every enabled collector fetches its raw observations and returns them as JSON.
2. The daemon assembles a market snapshot — collector data, current price, open position, P&L, constraints.
3. Claude receives the snapshot and returns `{ action, confidence, position_pct, reasoning }`.
4. The daemon executes: `open_long` / `open_short` / `close` / `hold`. Reasoning logged on every cycle.

There's no weighted voting, no anti-whipsaw threshold, no conflict detection in code — Claude reasons about all of that contextually. Mechanical exits (TP / SL / trailing) stay in Rust.

### Collectors

All use public APIs, no extra keys.

| Collector | Source | Returns |
|---|---|---|
| `pattern` | Binance Spot | Multi-timeframe (1m/5m/1h): RSI(14), MACD, EMA pair, Bollinger bands, ATR%, 1h/24h change, 24h & 4-day high/low levels |
| `flow` | Binance Futures | OB imbalance, funding rate, OI, L/S ratio, taker volume |
| `macro` | TradingView | Recent releases (actual vs forecast) + upcoming events |
| `news` | Multi-source RSS | Recent headlines verbatim — Claude reads them |
| `whale` | Hyperliquid | BTC positions of 8 verified top traders, size-weighted |

### Exits

| Strategy | Trigger |
|---|---|
| Take profit | Net ROE ≥ `--take-profit` |
| Stop loss | Net ROE ≤ `−-stop-loss` |
| Trailing stop | Net ROE drops `--trailing-stop` from session peak |

### Options

```bash
lnmarkets daemon [OPTIONS]

      --paper                     Simulated trades with real prices
      --live                      Real trades — use carefully
  -i, --interval <SECS>           Analysis interval [default: 60]
      --max-position <USD>        Position size ceiling [default: 10]
      --leverage <N>              [default: 10]
      --take-profit <PCT>         [default: 10]
      --stop-loss <PCT>           [default: 5]
      --trailing-stop <PCT>       [default: 3]
      --max-daily-loss <SATS>     Stop opening positions after this much net
                                  realized loss in a UTC day (off by default)
  -c, --collectors <LIST>         Data collectors [default: pattern,flow]
```

Full reference: `lnmarkets daemon --help`.

## Stats

Performance dashboard for trades placed by the daemon.

```bash
lnmarkets stats                        # summary
lnmarkets stats --trades -l 20         # last 20 orders
```

## Market Recap

24-48h BTC derivatives overview — no credentials.

```bash
lnmarkets recap
lnmarkets recap -o json
```

Pulls price action (Binance), funding / OI / L/S (Binance Futures), Fear & Greed (Alternative.me), economic calendar (TradingView). Failed sources are warned and skipped.

## Commands

10 MCP tools across 4 service groups. 35 CLI commands across 10 groups.

| Group | CLI | MCP | Auth | Description |
|---|---|---|---|---|
| `market` | 4 | 1 | No | Ticker, prices, index, funding |
| `account` | 4 | 2 | Yes | Balance, info, leaderboard, list trades |
| `futures` | 12 | 5 | Yes | Open, close, update, add margin, cross position |
| `funding` | 7 | 2 | Yes | Deposit, withdraw (Lightning + on-chain) |
| `auth` | 4 | — | No | Login, logout, status |
| `tui` | 1 | — | Opt | Interactive terminal dashboard |
| `stream` | 1 | — | Opt | Live WS stream tails for scripting |
| `alert` | 4 | — | No | Price + funding alerts with OS notifications |
| `daemon` | 1 | — | Opt | LLM-driven automated trading |
| `stats` | 1 | — | No | Daemon performance |
| `recap` | 1 | — | No | 24-48h BTC overview |

<details>
<summary>Full command reference</summary>

**Market** (public): `ticker [--watch]`, `prices`, `index`, `info`, `funding`

**Account** (auth): `info`, `balance`, `update`, `leaderboard`

**Futures** (auth): `list`, `open`, `close`, `stoploss`, `takeprofit`, `add-margin`, `cashin`, `cancel`, `cancel-all`, `close-all`, `cross`

**Funding** (auth): `deposit`, `new-address`, `addresses`, `deposits`, `withdraw`, `withdraw-onchain`, `withdrawals`

**Auth**: `login`, `logout`, `status`, `whoami`

**Stream**: `watch <channel>` — `ticker | lastprice | index | buckets | funding | ohlc | positions | orders | wallet | all`

**Alert**: `add "<rule>"`, `list`, `remove <id>`, `watch`. Grammar: `price > N`, `price < N`, `funding > N%`, `funding < N%`, `funding flips positive`, `funding flips negative`.

**Daemon / Stats / TUI / Recap**: see relevant sections + `--help`.

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

Config file at `~/.config/lnmarkets/config.toml` (Linux) or `~/Library/Application Support/lnmarkets/config.toml` (macOS):

```toml
[credentials]
api_key = "..."
api_secret = "..."
passphrase = "..."

[settings]
network = "mainnet"          # or "testnet"
output_format = "table"      # table | json | json-pretty
```

Environment variables override config-file values.

Global flags: `-o <format>` (table / json / json-pretty), `--testnet`.

## License

MIT
