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
| `macro` | TradingView API | Economic data surprises, event warnings |
| `news` | RSS feeds | Sentiment analysis from crypto news |
| `flow` | Binance Futures API | Order book imbalance, funding rate, OI, L/S ratio |

All data sources are **public APIs** — no API keys required.

<details>
<summary>Data source details</summary>

| Agent | Endpoint | Data |
|-------|----------|------|
| `pattern` | `api.binance.com/api/v3/klines` | BTC/USDT price candles |
| `macro` | `economic-calendar.tradingview.com/events` | Economic releases with actual vs forecast |
| `news` | CoinDesk, Cointelegraph, Bitcoin Magazine, Decrypt, CryptoSlate | RSS headlines |
| `flow` | `fapi.binance.com/fapi/v1/depth`, `/fundingRate`, `/openInterest` | Futures market data |

</details>

### Pattern Agent - Technical Analysis

The pattern agent fetches 1-minute candles from Binance and calculates three indicators:

| Indicator | Bullish Signal | Bearish Signal |
|-----------|----------------|----------------|
| RSI (14-period) | RSI < 30 (oversold) | RSI > 70 (overbought) |
| EMA Crossover (9/21) | EMA9 > EMA21 | EMA9 < EMA21 |
| Bollinger Bands (20, 2σ) | Price below lower band | Price above upper band |

Signals are combined with weighted voting. Confidence scales with indicator agreement and RSI extremes.

### Flow Agent - Order Flow Analysis

The flow agent analyzes Binance Futures market data for institutional positioning:

| Indicator | Bullish Signal | Bearish Signal |
|-----------|----------------|----------------|
| Order Book Imbalance | Bids > Asks (buying pressure) | Asks > Bids (selling pressure) |
| Funding Rate | Negative (shorts pay longs) | Positive (longs pay shorts) |
| Long/Short Ratio | Ratio < 0.8 (contrarian: crowded short) | Ratio > 1.5 (contrarian: crowded long) |

**Contrarian logic:** Extreme positioning often precedes reversals. When everyone is long, the market tends to drop.

### News Agent - Sentiment Analysis

The news agent fetches RSS headlines from crypto news sources and performs keyword-based sentiment analysis:

**Sources:** CoinDesk, Cointelegraph, Bitcoin Magazine, Decrypt, CryptoSlate

| Bullish Keywords | Bearish Keywords |
|------------------|------------------|
| bull, surge, rally, soar, pump | bear, crash, dump, plunge, selloff |
| breakout, ath, adoption, etf approved | hack, ban, fraud, investigation |
| institutional, accumulation | liquidation, capitulation |

- **Lookback:** 4 hours
- **Cache:** 5 minutes (avoids rate limiting)
- **Weighting:** Sources have credibility scores

### Macro Agent - Economic Data Analysis

The macro agent analyzes recent economic releases (past 6 hours) and generates signals based on **surprise factor** (actual vs forecast):

| Indicator | Beat Expectations | Miss Expectations |
|-----------|-------------------|-------------------|
| CPI/PPI/Inflation | SHORT (hawkish Fed) | LONG (dovish Fed) |
| NFP/Jobs/Employment | SHORT (hawkish Fed) | LONG (dovish Fed) |
| Unemployment | SHORT (lower = hawkish) | LONG (higher = dovish) |
| Housing/Home Sales | SHORT (strong) | LONG (weak = dovish) |
| GDP | SHORT (strong = hawkish) | LONG (weak = dovish) |
| Retail Sales | SHORT (hawkish) | LONG (dovish) |

**Example:** New Home Sales 587K vs 722K expected (-17.6% miss) → LONG signal (weak housing = dovish Fed = bullish BTC)

### Signal Aggregation

Each agent produces a signal with **direction** (Long/Short/Neutral) and **confidence** (0.0-1.0).

The orchestrator combines signals using weighted voting:

1. **Sum weights by direction** — Long signals add confidence to `long_weight`, Short to `short_weight`
2. **Choose direction** — Whichever side has higher total weight wins
3. **Calculate final confidence** — Average of winning direction's signals only (opposing signals don't dilute confidence)
4. **Apply threshold** — Only act if confidence ≥ `--min-confidence`
5. **Size position** — Higher confidence = larger position (up to `--max-position`)

**Example with 4 agents:**
```
pattern: LONG  60%  →  long_weight += 0.60, long_count++
macro:   NEUTRAL     →  (ignored)
news:    LONG  55%  →  long_weight += 0.55, long_count++
flow:    SHORT 40%  →  short_weight += 0.40, short_count++

long_weight = 1.15, short_weight = 0.40
Direction: LONG (1.15 > 0.40)
Confidence: 1.15 / 2 = 57% (average of LONG signals only)
→ Above 50% threshold, TRADE!
```

**Example with aligned signals:**
```
pattern: LONG  65%  →  long_weight += 0.65, long_count++
macro:   NEUTRAL     →  (ignored)
news:    LONG  55%  →  long_weight += 0.55, long_count++
flow:    NEUTRAL     →  (ignored)

long_weight = 1.20, long_count = 2
Direction: LONG
Confidence: 1.20 / 2 = 60%
→ Above 50% threshold, TRADE!
```

### Position Sizing

Position size scales with confidence above the threshold:

```
size_factor = (confidence - min_confidence) / (1.0 - min_confidence)
position_usd = max_position × size_factor × 0.5
```

| Confidence | Threshold | Size Factor | Position ($100 max) |
|------------|-----------|-------------|---------------------|
| 60% | 50% | 20% | $10 |
| 70% | 50% | 40% | $20 |
| 80% | 50% | 60% | $30 |
| 90% | 50% | 80% | $40 |

- **Maximum per trade:** 50% of `--max-position`
- **Cross margin:** All positions share the same margin pool
- Higher confidence = larger position

### Position Management

The daemon automatically manages positions:

**Take Profit / Stop Loss:**
- Checks P&L every interval
- Closes position when TP or SL threshold is hit
- Default: +5% take profit, -3% stop loss

**Signal Reversal:**
- If holding LONG and agents signal SHORT → closes long, opens short
- If holding SHORT and agents signal LONG → closes short, opens long
- If signal matches current position → skips (won't pyramid)

### Options

```bash
lnmarkets daemon [OPTIONS]

Options:
  -a, --agents <AGENTS>      Agents to enable [default: pattern]
  -i, --interval <SECS>      Analysis interval in seconds [default: 60]
      --paper                Paper trading (simulated with real prices)
      --live                 Live trading (real money!)
      --min-confidence <N>   Minimum confidence to act (0.0-1.0) [default: 0.7]
      --max-position <USD>   Maximum position size in USD [default: 10]
      --leverage <N>         Leverage (1-100) [default: 10]
      --take-profit <PCT>    Take profit percentage [default: 5]
      --stop-loss <PCT>      Stop loss percentage [default: 3]
```

### Examples

```bash
# Dry run: analysis only
lnmarkets daemon --agents pattern,flow --interval 30

# Paper trading: test strategies with real prices
lnmarkets daemon --paper --agents pattern,macro,news,flow --min-confidence 0.6

# Live trading: $20 max position at 10x leverage
lnmarkets daemon --live --agents pattern,flow --max-position 20 --leverage 10

# Custom TP/SL: tighter stop loss, wider take profit
lnmarkets daemon --live --agents pattern,macro,news,flow --take-profit 10 --stop-loss 2

# Conservative: smaller positions, higher confidence required
lnmarkets daemon --live --agents pattern,macro,news,flow --max-position 10 --leverage 5 --min-confidence 0.8
```

### Sample Output

```
Starting LN Markets trading daemon...
  Mode: LIVE TRADING
  Interval: 60s
  Min confidence: 70%
  Max position: $20 USD
  Leverage: 10x
  Take profit: +5.0%
  Stop loss: -3.0%
  Agents: ["pattern", "flow", "news", "macro"]

[14:30:00] Analyzing...
  [POSITION] ▼ $10 @ $69500 | P&L: $2 (+4.50%)
  ▲ [pattern] LONG (75%): BTC $69876 | RSI: 32.1 | EMA bullish crossover
  ▼ [flow] SHORT (60%): OB -45%↓ | FR -0.30bps | L/S 1.51
  ● [news] NEUTRAL (50%): 4 articles | 1B/2N/1b
  ● [macro] NEUTRAL (50%): Next: CPI Release in 3d
  → REVERSAL: SHORT → LONG (75% confidence)
  [CLOSE] Position closed: Signal reversal
  → ACTION: BUY $4 USD @ 10x (75% confidence)
  [LIVE] Order placed: 5eeb79e3-88cc-4399-9b77-c61a8b507be0
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
