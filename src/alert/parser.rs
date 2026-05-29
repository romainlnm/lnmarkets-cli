//! Tiny grammar for the rule strings stored in alerts.toml.
//!
//! Forms accepted:
//!   price > <number>
//!   price < <number>
//!   funding > <number>%
//!   funding < <number>%
//!   funding flips positive
//!   funding flips negative
//!
//! Commas in numbers are stripped. Funding values are written as a percentage
//! and stored as a decimal (0.05% → 0.0005).

use anyhow::{anyhow, bail, Result};

use super::Rule;

pub fn parse(input: &str) -> Result<Rule> {
    let lower = input.trim().to_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().collect();

    match tokens.as_slice() {
        ["price", ">", v] => Ok(Rule::PriceAbove(parse_price(v)?)),
        ["price", "<", v] => Ok(Rule::PriceBelow(parse_price(v)?)),
        ["funding", ">", v] => Ok(Rule::FundingAbove(parse_percent(v)?)),
        ["funding", "<", v] => Ok(Rule::FundingBelow(parse_percent(v)?)),
        ["funding", "flips", "positive"] => Ok(Rule::FundingFlipsPositive),
        ["funding", "flips", "negative"] => Ok(Rule::FundingFlipsNegative),
        _ => bail!(
            "Could not parse rule: {input:?}.\nSupported: \
             'price > N', 'price < N', 'funding > N%', 'funding < N%', \
             'funding flips positive', 'funding flips negative'."
        ),
    }
}

fn parse_price(s: &str) -> Result<f64> {
    let cleaned = s.replace(['$', ','], "");
    cleaned
        .parse::<f64>()
        .map_err(|_| anyhow!("expected a number, got {s:?}"))
}

fn parse_percent(s: &str) -> Result<f64> {
    let cleaned = s.trim_end_matches('%').replace(',', "");
    let pct: f64 = cleaned
        .parse()
        .map_err(|_| anyhow!("expected a percentage, got {s:?}"))?;
    Ok(pct / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_above() {
        assert_eq!(parse("price > 200000").unwrap(), Rule::PriceAbove(200000.0));
        assert_eq!(parse("price > $200,000").unwrap(), Rule::PriceAbove(200000.0));
    }

    #[test]
    fn price_below() {
        assert_eq!(parse("price < 180000").unwrap(), Rule::PriceBelow(180000.0));
    }

    #[test]
    fn funding_above_percent() {
        let r = parse("funding > 0.05%").unwrap();
        match r {
            Rule::FundingAbove(v) => assert!((v - 0.0005).abs() < 1e-9),
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn funding_flips() {
        assert_eq!(parse("funding flips positive").unwrap(), Rule::FundingFlipsPositive);
        assert_eq!(parse("funding flips negative").unwrap(), Rule::FundingFlipsNegative);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(parse("PRICE > 200000").unwrap(), Rule::PriceAbove(200000.0));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("hello world").is_err());
        assert!(parse("price >").is_err());
    }
}
