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
- [Interactive TUI](#interactive-tui)
- [MCP Server](#mcp-server)
- [Trading Daemon](#trading-daemon)
- [Stats Dashboard](#stats-dashboard)
- [Treasury Integration](#treasury-integration-claw-cash)
- [Market Recap](#market-recap)
- [Commands](#commands)
- [API Keys & Configuration](#api-keys--configuration)
- [License](#license)

## Installation

Single binary, no runtime dependencies.

### Download (recommended)

Download the latest binary from [GitHub Releases](https://github.com/romainlnm/lnmarkets-cli/releases):

```bash
# macOS (Apple Silicon)
curl -L https://github.com/romainlnm/lnmarkets-cli/releases/latest/download/lnmarkets-macos-arm64 -o lnmarkets

# macOS (Intel)
curl -L https://github.com/romainlnm/lnmarkets-cli/releases/latest/download/lnmarkets-macos-x64 -o lnmarkets

# Linux (x64)
curl -L https://github.com/romainlnm/lnmarkets-cli/releases/latest/download/lnmarkets-linux-x64 -o lnmarkets

# Linux (ARM64)
curl -L https://github.com/romainlnm/lnmarkets-cli/releases/latest/download/lnmarkets-linux-arm64 -o lnmarkets

# Make executable and move to PATH
chmod +x lnmarkets
sudo mv lnmarkets /usr/local/bin/
```

### Build from source

Requires [Rust](https://rustup.rs/).

```bash
git clone https://github.com/romainlnm/lnmarkets-cli.git
cd lnmarkets-cli
cargo install --path . --locked
```

### Verify installation

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

## Interactive TUI

Full-featured terminal dashboard for monitoring and trading.

```bash
lnmarkets tui              # Launch TUI
lnmarkets tui --refresh 3  # Custom refresh interval (seconds)
```

### Features

| Tab | Description |
|-----|-------------|
| Dashboard | Balance, P&L summary, BTC sparkline, positions preview |
| Positions | Manage running positions (close, SL, TP, margin, cash-in) |
| Orders | Pending orders (cancel individual or all) |
| History | Closed trades |
| Funding | Deposit/withdraw Lightning ⚡, on-chain ₿, generate addresses |
| Recap | Fear & Greed, derivatives data, economic calendar |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `1-6` | Jump to tab |
| `Tab` / `←→` | Switch tabs |
| `↑↓` / `jk` | Select row |
| `o` | Open position |
| `c` / `C` | Close position / Close all |
| `s` / `t` | Set stop loss / take profit |
| `d` / `w` | Deposit / Withdraw Lightning |
| `T` | Toggle dark/light theme |
| `N` | Toggle testnet/mainnet |
| `L` | Login (if not authenticated) |
| `D` | Launch trading daemon |
| `?` | Help |
| `q` | Quit |

### Screenshots

The TUI provides a live dashboard with auto-refreshing data, keyboard-driven trading, and full account management — all without leaving the terminal.

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

Automated trading with multi-agent signal analysis. Runs continuously, combining signals from technical analysis, economic calendar, and news sentiment. Uses **cross margin** trading — all positions share the same margin pool.

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
| `pattern` | Binance Spot API | RSI, MACD, EMA crossover, Bollinger Bands, ATR |
| `flow` | Binance Futures API | Taker volume, order book, funding rate, L/S ratio |
| `whale` | Hyperliquid API | Copy top BTC perp traders (8 verified whales) |
| `macro` | TradingView API | Economic data surprises, event warnings |
| `news` | RSS feeds | Sentiment analysis from crypto news |

Default: `pattern,flow` — the two most reliable signal generators.

All data sources are **public APIs** — no API keys required.

<details>
<summary>Data source details</summary>

| Agent | Endpoint | Data |
|-------|----------|------|
| `pattern` | `api.binance.com/api/v3/klines` | BTC/USDT price candles |
| `flow` | `fapi.binance.com/fapi/v1/*` | Depth, funding, OI, L/S ratio, taker volume |
| `whale` | `api.hyperliquid.xyz/info` | Verified whale BTC positions |
| `macro` | `economic-calendar.tradingview.com/events` | Economic releases with actual vs forecast |
| `news` | CNBC, Yahoo Finance, CoinDesk, etc. | RSS headlines |

</details>

### Pattern Agent - Technical Analysis

The pattern agent fetches 1-minute candles from Binance and calculates four indicators with weighted voting:

| Indicator | Weight | Bullish Signal | Bearish Signal |
|-----------|--------|----------------|----------------|
| RSI (14-period) | 1.5 | RSI < 30 (oversold) | RSI > 70 (overbought) |
| MACD (12/26/9) | 1.3 | MACD > Signal line | MACD < Signal line |
| EMA Crossover (9/21) | 1.0 | EMA9 > EMA21 (+0.05%) | EMA9 < EMA21 (-0.05%) |
| Bollinger Bands (20, 2σ) | 0.8 | Price below lower band | Price above upper band |

Confidence = weighted average of agreeing signals. RSI and MACD carry the most weight as leading indicators.

**ATR (Average True Range)** is also calculated and reported as a percentage of price. This measures market volatility — low ATR indicates a ranging/choppy market where signals are less reliable. See `--min-atr` below.

### Flow Agent - Order Flow Analysis

The flow agent analyzes Binance Futures market data for real-time order flow and positioning:

| Indicator | Weight | Bullish Signal | Bearish Signal |
|-----------|--------|----------------|----------------|
| Taker Buy/Sell Volume | 1.5 | Ratio > 1.15 (aggressive buying) | Ratio < 0.87 (aggressive selling) |
| Long/Short Ratio | 1.3 | Ratio < 0.77 (contrarian: crowded short) | Ratio > 1.3 (contrarian: crowded long) |
| Funding Rate | 1.2 | < -5bps (shorts pay longs) | > +5bps (longs pay shorts) |
| Order Book Imbalance | 1.0 | Bids > Asks by 15%+ | Asks > Bids by 15%+ |
| Open Interest Change | 0.8 | Rising OI (new positions) | Falling OI (closing positions) |

**Taker volume** is the strongest signal — it shows actual market orders hitting the book, not just passive liquidity.

**Contrarian logic:** Extreme positioning often precedes reversals. When everyone is long, the market tends to drop.

### Whale Agent - Copy Trading

The whale agent tracks BTC positions of **verified top performers** on Hyperliquid and generates **position-size-weighted** signals:

1. **8 verified whale addresses** from public sources (Arkham, Lookonchain, OnchainDataNerd, leaderboards)
2. **Queries each trader's BTC position** via Hyperliquid's free `clearinghouseState` API
3. **Calculates weighted consensus** — weights by position size (BTC), not just count
4. **Signals when 70%+ of weighted size agrees** — a 50 BTC position counts more than a 2 BTC position
5. **Requires minimum 3 traders** with BTC positions to generate a signal

**Verified whales include:**
- `0x5b5d...` — #1 top earner, $143M+ profit, algorithmic trader
- `0xb317...` — "BTC OG" whale, $500M positions, $150M+ profit
- `0x2eA1...` — $282M ETH position whale (from Arkham)
- And 5 more verified active traders

| Positions | Weighted Consensus | Signal |
|-----------|-------------------|--------|
| 3 long (80 BTC) vs 2 short (10 BTC) | 89% long | LONG |
| 4 long (20 BTC) vs 3 short (15 BTC) | 57% long | NEUTRAL (below 70%) |
| 2 long (5 BTC) vs 5 short (60 BTC) | 92% short | SHORT |

**Sample output:**
```
▲ [whale] LONG (75%): 4 long (80.5 BTC) vs 2 short (12.3 BTC) | 87%/13% | PnL: +$125K
```

**Rate limits:** 8 requests per cycle (one per whale) — no API key required.

### News Agent - Sentiment Analysis

The news agent fetches RSS headlines from news sources and performs keyword-based sentiment analysis:

**Sources:** CNBC, ZeroHedge, MarketWatch, Yahoo Finance, CoinDesk, Cointelegraph, Bitcoin Magazine, Decrypt, CryptoSlate, The Block

| Bullish Keywords | Bearish Keywords |
|------------------|------------------|
| bull, surge, rally, soar, pump | bear, crash, dump, plunge, selloff |
| breakout, ath, adoption, etf approved | hack, ban, fraud, investigation |
| institutional, accumulation | liquidation, capitulation |
| peace, ceasefire, de-escalation | war, strike, attack, missile, sanctions |

**Geopolitical keywords:** The agent also monitors geopolitical events (Trump, Iran, Israel, Russia, China, NATO, etc.) that can move BTC markets.

- **Lookback:** 2 hours
- **Cache:** 2 minutes (fast refresh for breaking news)
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

### Anti-Whipsaw Protections

Rapid market news (geopolitical events, Fed announcements) can cause agents to flip-flop between LONG and SHORT signals. Four safeguards prevent excessive reversals:

**ATR Volatility Filter:**
- Skips trading when ATR% is below threshold (low volatility = choppy market)
- Default: 0.5% (`--min-atr 0.5`)
- ATR < 0.5% typically indicates ranging conditions where signals whipsaw
- Example: ATR 0.44% on a $300 range day → skip all trades
- Prevents stop-loss churn in sideways markets

**Reversal Premium (+10% confidence):**
- Reversals require 10% higher confidence than regular trades
- Default: 80% min confidence → reversals need 90%
- Accounts for double trading fees (close current + open opposite)
- Example: 85% signal would open a new position, but won't reverse an existing one

**Reversal Cooldown:**
- After a position reversal, the daemon waits before allowing another reversal
- Default: 5 minutes (`--reversal-cooldown 300`)
- During cooldown, contradicting signals are logged but not acted upon
- TP/SL exits are still allowed during cooldown

**Agent Conflict Detection:**
- When agents strongly disagree, skip the trade entirely
- Compares winning vs losing signal weights
- Default threshold: 30% (`--conflict-threshold 0.3`)
- Example: LONG 55% vs SHORT 45% = 10% margin → conflict, skip trade
- Example: LONG 70% vs SHORT 30% = 40% margin → clear signal, trade

**Why this matters:**
```
[14:30:00] NEWS: "Trump announces Iran sanctions"
  → Confidence: 85% (min: 80%)
  → REVERSAL BLOCKED: 85% < 90% required (+10% fee premium)
[14:35:00] NEWS: "Iran threatens retaliation"
  → Confidence: 92%
  → REVERSAL: LONG → SHORT
[14:36:00] NEWS: "Iran signals de-escalation"
  → Confidence: 88%
  → COOLDOWN: Reversal blocked (4m remaining)
```

Without these protections, the daemon would churn through positions, racking up fees on noise.

### Options

```bash
lnmarkets daemon [OPTIONS]

Options:
  -a, --agents <AGENTS>           Agents to enable [default: pattern]
  -i, --interval <SECS>           Analysis interval in seconds [default: 60]
      --paper                     Paper trading (simulated with real prices)
      --live                      Live trading (real money!)
      --min-confidence <N>        Minimum confidence to act (0.0-1.0) [default: 0.8]
      --max-position <USD>        Maximum position size in USD [default: 10]
      --leverage <N>              Leverage (1-100) [default: 10]
      --take-profit <PCT>         Take profit percentage [default: 10]
      --stop-loss <PCT>           Stop loss percentage [default: 5]
      --trailing-stop <PCT>       Trailing stop - close if ROE drops this much from peak [default: 3]
      --reversal-cooldown <SECS>  Cooldown after position reversal [default: 300]
      --conflict-threshold <N>    Skip if agents disagree by less than this [default: 0.3]
      --min-atr <PCT>             Minimum ATR% to trade (volatility filter) [default: 0.5]

Treasury options (claw-cash integration):
      --treasury                  Enable claw-cash treasury integration
      --treasury-mock             Mock treasury (simulates claw-cash for testing)
      --claw-url <URL>            Claw-cash daemon URL [default: http://127.0.0.1:9137]
      --treasury-min <SATS>       Min balance on exchange (fund below) [default: 10000]
      --treasury-max <SATS>       Max balance on exchange (withdraw above) [default: 100000]
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

# Trailing stop: lock in gains when ROE drops 3% from peak (e.g., peak 8% → close at 5%)
lnmarkets daemon --live --agents pattern,flow --trailing-stop 3

# Conservative: smaller positions, higher confidence required
lnmarkets daemon --live --agents pattern,macro,news,flow --max-position 10 --leverage 5 --min-confidence 0.8

# Volatility filter: only trade when ATR > 0.6% (skip choppy days)
lnmarkets daemon --live --agents pattern,flow,whale --min-atr 0.6
```

### Exit Strategies

The daemon supports three exit mechanisms based on **Net ROE** (after estimated fees):

| Strategy | Trigger | Use Case |
|----------|---------|----------|
| **Take Profit** | Net ROE >= +X% | Lock in target gains |
| **Stop Loss** | Net ROE <= -X% | Limit downside risk |
| **Trailing Stop** | Net ROE drops X% from peak | Protect profits in winning trades |

**Trailing Stop Example:**
```
Position opens at $70,000
  → Price rises to $71,500, Net ROE hits +8% (peak)
  → Price retraces to $71,000, Net ROE at +5%
  → With --trailing-stop 3: Close at +5% (dropped 3% from peak of 8%)
  → Without trailing stop: Would hold until TP (+10%) or SL (-5%)
```

The trailing stop only activates once the position is profitable and the peak ROE exceeds the trail percentage.

### Sample Output

```
Starting LN Markets trading daemon...
  Mode: LIVE TRADING
  Interval: 60s
  Min confidence: 80%
  Max position: $20 USD
  Leverage: 10x
  Take profit: +10.0%
  Stop loss: -5.0%
  Trailing stop: 3.0% from peak
  Agents: ["pattern", "flow", "news", "macro"]

[14:30:00] Analyzing...
  [POSITION] ▼ $10 @ $69500 | Net ROE: +4.50% (TP: +10% / SL: -5%) | Trail: 3.0% from peak 4.8%
  ▲ [pattern] LONG (75%): BTC $69876 | RSI: 32.1 | EMA bullish crossover
  ▼ [flow] SHORT (60%): OB -45%↓ | FR -0.30bps | L/S 1.51
  ● [news] NEUTRAL (50%): 4 articles | 1B/2N/1b
  ● [macro] NEUTRAL (50%): Next: CPI Release in 3d
  → REVERSAL: SHORT → LONG (75% confidence)
  [CLOSE] Position closed: Signal reversal
  → ACTION: BUY $4 USD @ 10x (75% confidence)
  [LIVE] Order placed: 5eeb79e3-88cc-4399-9b77-c61a8b507be0
```

## Stats Dashboard

Track your daemon trading performance. Stats are fetched from the LN Markets API, filtered to orders placed by the daemon.

```bash
lnmarkets stats              # Show stats summary
lnmarkets stats --trades     # List recent orders
lnmarkets stats --trades -l 20  # Last 20 orders
```

### Sample Output

```
Daemon Stats (cross margin)
────────────────────────────────────────
Orders placed:   47
Total bought:    $120 USD
Total sold:      $85 USD
Trading fees:    235 sats

Current Position:
  LONG $35 @ $68500
  Margin: 50000 sats
  P&L:    +1250 sats
```

With `--trades`:

```
Daemon Orders (3 total)
──────────────────────────────────────────────────
  ▲ BUY $10 @ $68200 (fee: 5 sats) - 2026-03-19T14:30
  ▼ SELL $5 @ $68500 (fee: 3 sats) - 2026-03-19T15:45
  ▲ BUY $30 @ $68400 (fee: 15 sats) - 2026-03-19T16:20
```

### How It Works

- Daemon saves order IDs when placing cross margin orders
- Stats fetches order history from `futures/cross/orders/filled`
- Shows current cross position with unrealized P&L
- Cross margin aggregates all orders into a single position

## Treasury Integration (claw-cash)

The daemon can connect to a [claw-cash](https://github.com/ArkLabsHQ/claw-cash) wallet for autonomous fund management. This allows an AI trading agent to maintain a target balance on the exchange while keeping excess funds in a secure hardware enclave.

### How It Works

1. **Auto-withdraw:** When exchange balance exceeds `--treasury-max`, withdraw to claw-cash
2. **Auto-fund:** When exchange balance drops below `--treasury-min`, fund from claw-cash
3. **Insufficient funds:** If claw-cash balance is too low to fund, log warning and continue

```
Exchange balance: 150,000 sats (max: 100,000)
→ Withdraw 50,000 sats to claw-cash

Exchange balance: 5,000 sats (min: 10,000)
→ Request 20,000 sats from claw-cash
```

### Setup

1. **Install claw-cash:** Follow [claw-cash setup](https://github.com/ArkLabsHQ/claw-cash)
2. **Start the daemon:**
   ```bash
   cd claw-cash && ENCLAVE_DEV_MODE=true pnpm start:enclave
   ```
3. **Fund the enclave:** Use `claw-cash receive` to get a Lightning address
4. **Enable treasury:**
   ```bash
   lnmarkets daemon --live --treasury --claw-url http://127.0.0.1:9137
   ```

### Mock Mode

Test treasury logic without a real claw-cash instance:

```bash
lnmarkets daemon --paper --treasury-mock
```

Mock mode simulates:
- Balance checks (starts at 100,000 sats)
- Invoice generation (returns fake bolt11)
- Payments (logs action, returns fake preimage)

### Sample Output

```
Starting LN Markets trading daemon...
  Mode: LIVE TRADING
  Treasury: claw-cash connected (balance: 50000 sats)
  ...

[14:30:00] Analyzing...
  [TREASURY] Exchange: 8500 sats (min: 10000) - funding...
  [TREASURY] Requested 20000 sats from claw-cash
  [TREASURY] Invoice paid, new exchange balance: 28500 sats
```

### Why claw-cash?

- **Hardware enclave:** Private keys never leave secure memory
- **AI-native:** Designed for autonomous agents
- **Lightning-native:** Instant deposits/withdrawals
- **Non-custodial:** You control the enclave

## Market Recap

Get a 24-48h BTC derivatives market overview. Aggregates data from multiple free APIs — no authentication required.

```bash
lnmarkets recap              # Table output
lnmarkets recap -o json      # JSON output
```

### Sample Output

```
BTC Market Recap (24h)
══════════════════════════════════════════════════

Price Action
  Current:    $69,250
  24h High:   $70,100 (+1.2%)
  24h Low:    $68,200 (-1.5%)
  24h Change: +2.3%

Derivatives
  Funding Rate:  +0.0045% (neutral)
  Open Interest: $18.2B
  Long/Short:    1.23 (longs dominant)

Sentiment
  Fear & Greed:  72 (Greed) ^ from 65

Recent Events (24h)
  v CPI m/m: 3.2% vs 3.4% exp (-5.9%) - BULLISH

Upcoming Events (48h)
  -> [!] FOMC Minutes (high) in 18h
  -> [!] NFP (high) in 42h
```

### Data Sources

| Data | Source | Endpoint |
|------|--------|----------|
| Price action | Binance Spot | `/api/v3/klines` |
| Funding rate | Binance Futures | `/fapi/v1/fundingRate` |
| Open interest | Binance Futures | `/fapi/v1/openInterest` |
| Long/Short ratio | Binance Futures | `/futures/data/globalLongShortAccountRatio` |
| Fear & Greed | Alternative.me | `api.alternative.me/fng` |
| Economic calendar | TradingView | `economic-calendar.tradingview.com/events` |

All sources are public APIs with no authentication required. Failed sources are shown as warnings — partial data is still displayed.

## Commands

10 MCP tools across 4 service groups. 31 CLI commands across 9 groups.

| Group | CLI Commands | MCP Tools | Auth | Description |
|-------|--------------|-----------|------|-------------|
| market | 4 | 1 | No | Ticker, prices, index, funding rate |
| account | 4 | 2 | Yes | Balance, info, leaderboard, list trades |
| futures | 12 | 5 | Yes | Open, close, update, add margin, cross position |
| funding | 7 | 2 | Yes | Deposit, withdraw (Lightning & on-chain) |
| auth | 4 | — | No | Login, logout, status |
| tui | 1 | — | Optional | Interactive terminal dashboard |
| daemon | 1 | — | Optional | Automated trading with agents, treasury integration |
| stats | 1 | — | No | Trading performance dashboard |
| recap | 1 | — | No | 24-48h BTC market overview |

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
| `lnmarkets futures cross` | Show cross-margin position |

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

### Recap (Public)

| Command | Description |
|---------|-------------|
| `lnmarkets recap` | 24-48h BTC market overview (price, derivatives, sentiment, calendar) |

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

Store credentials in the config file:
- **Linux**: `~/.config/lnmarkets/config.toml`
- **macOS**: `~/Library/Application Support/lnmarkets/config.toml`

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
2. Config file (platform-specific path above)

### Global options

```bash
-o, --output <FORMAT>    table | json | json-pretty (default: table)
--testnet                Use testnet instead of mainnet
```

## License

MIT
