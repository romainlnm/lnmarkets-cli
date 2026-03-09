use crate::config::OutputFormat;
use colored::Colorize;
use serde::Serialize;
use tabled::{Table, Tabled};

pub fn print_json<T: Serialize>(data: &T, pretty: bool) -> anyhow::Result<()> {
    let output = if pretty {
        serde_json::to_string_pretty(data)?
    } else {
        serde_json::to_string(data)?
    };
    println!("{}", output);
    Ok(())
}

pub fn print_table<T: Tabled>(data: Vec<T>) {
    if data.is_empty() {
        println!("{}", "No data available".dimmed());
        return;
    }
    let table = Table::new(data).to_string();
    println!("{}", table);
}

pub fn print_single<T: Serialize + Tabled>(data: T, format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => print_json(&data, false),
        OutputFormat::JsonPretty => print_json(&data, true),
        OutputFormat::Table => {
            print_table(vec![data]);
            Ok(())
        }
    }
}

pub fn print_list<T: Serialize + Tabled>(data: Vec<T>, format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => print_json(&data, false),
        OutputFormat::JsonPretty => print_json(&data, true),
        OutputFormat::Table => {
            print_table(data);
            Ok(())
        }
    }
}

pub fn print_success(message: &str) {
    println!("{} {}", "✓".green(), message);
}

pub fn print_error(message: &str) {
    eprintln!("{} {}", "✗".red(), message);
}

pub fn print_warning(message: &str) {
    println!("{} {}", "!".yellow(), message);
}

pub fn print_info(message: &str) {
    println!("{} {}", "i".blue(), message);
}

pub fn format_sats(sats: i64) -> String {
    if sats.abs() >= 100_000_000 {
        format!("{:.8} BTC", sats as f64 / 100_000_000.0)
    } else if sats.abs() >= 1_000_000 {
        format!("{:.2}M sats", sats as f64 / 1_000_000.0)
    } else if sats.abs() >= 1_000 {
        format!("{:.2}K sats", sats as f64 / 1_000.0)
    } else {
        format!("{} sats", sats)
    }
}

pub fn format_price(price: f64) -> String {
    format!("${:.2}", price)
}
