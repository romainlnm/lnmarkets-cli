//! Recap command - BTC market overview

use crate::config::OutputFormat;
use crate::recap::{fetch_market_recap, MarketRecap};
use anyhow::Result;
use clap::Args;
use colored::Colorize;

/// Arguments for the recap command
#[derive(Args, Debug)]
pub struct RecapArgs {}

impl RecapArgs {
    pub async fn execute(&self, format: OutputFormat) -> Result<()> {
        let recap = fetch_market_recap().await;

        match format {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string(&recap)?);
            }
            OutputFormat::JsonPretty => {
                println!("{}", serde_json::to_string_pretty(&recap)?);
            }
            OutputFormat::Table => {
                print_recap_table(&recap);
            }
        }

        Ok(())
    }
}

fn print_recap_table(recap: &MarketRecap) {
    println!();
    println!("{}", "BTC Market Recap (24h)".bold());
    println!("{}", "═".repeat(50));

    // Price Action
    println!();
    println!("{}", "Price Action".bold().underline());
    if let Some(price) = &recap.price {
        println!(
            "  Current:    {}",
            format!("${:.0}", price.current).cyan()
        );
        println!(
            "  24h High:   ${:.0} ({:+.1}%)",
            price.high_24h, price.high_pct
        );
        println!(
            "  24h Low:    ${:.0} ({:+.1}%)",
            price.low_24h, -price.low_pct
        );

        let change_str = format!("{:+.1}%", price.change_24h_pct);
        let change_colored = if price.change_24h_pct >= 0.0 {
            change_str.green()
        } else {
            change_str.red()
        };
        println!("  24h Change: {}", change_colored);
    } else {
        println!("  {}", "Data unavailable".dimmed());
    }

    // Derivatives
    println!();
    println!("{}", "Derivatives".bold().underline());
    if let Some(deriv) = &recap.derivatives {
        let funding_str = format!("{:+.4}%", deriv.funding_rate);
        let funding_colored = if deriv.funding_rate > 0.01 {
            funding_str.green()
        } else if deriv.funding_rate < -0.01 {
            funding_str.red()
        } else {
            funding_str.normal()
        };
        println!(
            "  Funding Rate:  {} ({})",
            funding_colored,
            deriv.funding_sentiment.label()
        );

        let oi_billions = deriv.open_interest / 1_000_000_000.0;
        println!("  Open Interest: ${:.1}B", oi_billions);

        let ls_colored = if deriv.long_short_ratio > 1.1 {
            format!("{:.2}", deriv.long_short_ratio).green()
        } else if deriv.long_short_ratio < 0.9 {
            format!("{:.2}", deriv.long_short_ratio).red()
        } else {
            format!("{:.2}", deriv.long_short_ratio).normal()
        };
        println!(
            "  Long/Short:    {} ({})",
            ls_colored,
            deriv.ls_sentiment.label()
        );
    } else {
        println!("  {}", "Data unavailable".dimmed());
    }

    // Sentiment
    println!();
    println!("{}", "Sentiment".bold().underline());
    if let Some(sentiment) = &recap.sentiment {
        let value_colored = match sentiment.value {
            0..=25 => format!("{}", sentiment.value).red(),
            26..=45 => format!("{}", sentiment.value).yellow(),
            46..=55 => format!("{}", sentiment.value).normal(),
            56..=75 => format!("{}", sentiment.value).green(),
            _ => format!("{}", sentiment.value).bright_green(),
        };

        let prev_str = sentiment
            .previous_value
            .map(|p| format!(" {} from {}", sentiment.change_indicator(), p))
            .unwrap_or_default();

        println!(
            "  Fear & Greed:  {} ({}){}",
            value_colored, sentiment.label, prev_str
        );
    } else {
        println!("  {}", "Data unavailable".dimmed());
    }

    // Recent Events
    if !recap.recent_events.is_empty() {
        println!();
        println!("{}", "Recent Events (24h)".bold().underline());
        for event in &recap.recent_events {
            let impact_str = event
                .btc_impact
                .map(|i| format!(" - {}", i.label()))
                .unwrap_or_default();

            let impact_colored = match event.btc_impact {
                Some(crate::recap::types::BtcImpact::Bullish) => impact_str.green(),
                Some(crate::recap::types::BtcImpact::Bearish) => impact_str.red(),
                _ => impact_str.normal(),
            };

            let surprise_str = event
                .surprise_pct
                .map(|s| format!(" ({:+.1}%)", s))
                .unwrap_or_default();

            let actual_forecast = match (event.actual, event.forecast) {
                (Some(a), Some(f)) => format!(": {:.1} vs {:.1} exp{}", a, f, surprise_str),
                (Some(a), None) => format!(": {:.1}", a),
                _ => String::new(),
            };

            println!(
                "  {} {}{}{}",
                "v".dimmed(),
                event.title,
                actual_forecast,
                impact_colored
            );
        }
    }

    // Upcoming Events
    if !recap.upcoming_events.is_empty() {
        println!();
        println!("{}", "Upcoming Events (48h)".bold().underline());
        for event in &recap.upcoming_events {
            let importance_icon = event.importance.icon();
            let time_str = format!("in {}", event.time);

            let importance_colored = match event.importance {
                crate::recap::types::EventImportance::High => importance_icon.yellow().bold(),
                crate::recap::types::EventImportance::Medium => importance_icon.normal(),
                crate::recap::types::EventImportance::Low => importance_icon.dimmed(),
            };

            println!(
                "  {} {} {} ({}) {}",
                "->".dimmed(),
                importance_colored,
                event.title,
                event.importance.label(),
                time_str.dimmed()
            );
        }
    }

    // Warnings for failed data sources
    if !recap.errors.is_empty() {
        println!();
        println!("{}", "Warnings".yellow().bold());
        for error in &recap.errors {
            println!("  {} {}", "!".yellow(), error.dimmed());
        }
    }

    println!();
}
