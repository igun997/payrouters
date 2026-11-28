//! Rupiah exchange rates for providers that cannot charge IDR (PayPal).
//!
//! The source is the European Central Bank's euro foreign exchange reference rates, which the ECB
//! publishes daily for working days on its public Data API (no key). They are official reference
//! rates, not a quote you can trade at, so they are mid-market: the provider's own conversion
//! spread is not in them. A rupiah price per unit of any listed currency is the cross rate
//! `IDR per EUR ÷ currency per EUR`.
//!
//! Rates are cached for an hour (the ECB updates once a day). When the ECB cannot be reached the
//! last rate is reused, but never one that is older than [`MAX_AGE_DAYS`]: a stale rate is a
//! price error, so the payment is refused instead.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use chrono::{NaiveDate, Utc};

use super::{ProviderError, send, truncate};

const ECB: &str = "https://data-api.ecb.europa.eu/service/data/EXR";
const CACHE_FOR: Duration = Duration::from_secs(3600);
/// Covers a long weekend plus a missed day; beyond that the ECB data is considered stale.
pub(crate) const MAX_AGE_DAYS: i64 = 7;

/// How many rupiah one unit of a currency costs, and the ECB publication date it is from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rate {
    pub idr_per_unit: f64,
    pub as_of: NaiveDate,
}

static CACHE: LazyLock<Mutex<HashMap<String, (Rate, Instant)>>> = LazyLock::new(Default::default);

/// The cross rate from the ECB's `CURRENCY,TIME_PERIOD,OBS_VALUE` CSV for the IDR series and, unless
/// the currency is the euro itself, the currency's series. Both must be present.
pub(crate) fn parse_ecb(csv: &str, currency: &str) -> Result<Rate, String> {
    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let headers = reader.headers().map_err(|e| e.to_string())?.clone();
    let column = |name: &str| {
        headers
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| format!("ECB answer has no {name} column"))
    };
    let (cur, date, value) = (
        column("CURRENCY")?,
        column("TIME_PERIOD")?,
        column("OBS_VALUE")?,
    );
    let mut latest: HashMap<String, (NaiveDate, f64)> = HashMap::new();
    for row in reader.records() {
        let row = row.map_err(|e| e.to_string())?;
        let (Some(c), Some(d), Some(v)) = (row.get(cur), row.get(date), row.get(value)) else {
            continue;
        };
        let (Ok(d), Ok(v)) = (NaiveDate::parse_from_str(d, "%Y-%m-%d"), v.parse::<f64>()) else {
            continue;
        };
        if latest.get(c).is_none_or(|(prev, _)| d > *prev) {
            latest.insert(c.to_string(), (d, v));
        }
    }
    let idr = latest
        .get("IDR")
        .ok_or("the ECB published no rupiah rate")?;
    let (unit, unit_date) = if currency == "EUR" {
        (1.0, idr.0)
    } else {
        let c = latest
            .get(currency)
            .ok_or_else(|| format!("the ECB published no {currency} rate"))?;
        (c.1, c.0)
    };
    let rate = idr.1 / unit;
    if !rate.is_finite() || rate <= 0.0 {
        return Err(format!("the ECB rates give no usable IDR/{currency} rate"));
    }
    Ok(Rate {
        idr_per_unit: rate,
        as_of: idr.0.min(unit_date),
    })
}

fn too_old(rate: &Rate, today: NaiveDate) -> bool {
    (today - rate.as_of).num_days() > MAX_AGE_DAYS
}

fn cached(currency: &str) -> Option<(Rate, Instant)> {
    CACHE.lock().ok()?.get(currency).copied()
}

async fn fetch(http: &reqwest::Client, currency: &str) -> Result<Rate, String> {
    // `currency` is one of PayPal's fixed codes, so it is safe in the series key.
    let series = if currency == "EUR" {
        "D.IDR".to_string()
    } else {
        format!("D.IDR+{currency}")
    };
    let url = format!("{ECB}/{series}.EUR.SP00.A?lastNObservations=1&format=csvdata");
    let reply = send(http.get(url).header("Accept", "text/csv"))
        .await
        .map_err(|e| e.to_string())?;
    if reply.status != 200 {
        return Err(format!(
            "ECB responded {}: {}",
            reply.status,
            truncate(&reply.text, 120)
        ));
    }
    parse_ecb(&reply.text, currency)
}

/// The current IDR price of one unit of `currency` (an uppercase code PayPal supports).
pub(crate) async fn idr_per_unit(
    http: &reqwest::Client,
    currency: &str,
) -> Result<Rate, ProviderError> {
    let today = Utc::now().date_naive();
    if let Some((rate, at)) = cached(currency)
        && at.elapsed() < CACHE_FOR
        && !too_old(&rate, today)
    {
        return Ok(rate);
    }
    match fetch(http, currency).await {
        Ok(rate) if too_old(&rate, today) => Err(ProviderError::Unavailable(format!(
            "the ECB's latest {currency} rate is from {}, too old to price a payment",
            rate.as_of
        ))),
        Ok(rate) => {
            if let Ok(mut cache) = CACHE.lock() {
                cache.insert(currency.to_string(), (rate, Instant::now()));
            }
            Ok(rate)
        }
        Err(why) => match cached(currency) {
            Some((rate, _)) if !too_old(&rate, today) => {
                tracing::warn!(%currency, %why, "ECB unreachable, reusing the last rate");
                Ok(rate)
            }
            _ => Err(ProviderError::Unavailable(format!(
                "could not get the IDR/{currency} reference rate from the ECB: {why}"
            ))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "KEY,FREQ,CURRENCY,CURRENCY_DENOM,EXR_TYPE,EXR_SUFFIX,TIME_PERIOD,OBS_VALUE,OBS_STATUS,TITLE\n";

    fn csv(rows: &[(&str, &str, &str)]) -> String {
        let mut out = HEADER.to_string();
        for (cur, date, value) in rows {
            out.push_str(&format!(
                "EXR.D.{cur}.EUR.SP00.A,D,{cur},EUR,SP00,A,{date},{value},A,\"{cur}/Euro, ECB reference\"\n"
            ));
        }
        out
    }

    fn day(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn dollar_rate_is_the_cross_of_rupiah_and_dollar_per_euro() {
        // the 2026-11-27 answer: 20136.83 IDR and 1.0957 USD per EUR
        let r = parse_ecb(
            &csv(&[
                ("IDR", "2026-11-27", "20136.83"),
                ("USD", "2026-11-27", "1.0957"),
            ]),
            "USD",
        )
        .unwrap();
        // 20136.83 / 1.0957 = 18 378.05 (Frankfurter, which serves the same ECB data, rounds to 18 378)
        assert!(
            (r.idr_per_unit - 18_378.05).abs() < 0.01,
            "{}",
            r.idr_per_unit
        );
        assert_eq!(r.as_of, day("2026-11-27"));
    }

    #[test]
    fn the_euro_needs_no_second_series() {
        let r = parse_ecb(&csv(&[("IDR", "2026-11-27", "20136.83")]), "EUR").unwrap();
        assert_eq!(r.idr_per_unit, 20_136.83);
    }

    #[test]
    fn the_older_of_the_two_dates_is_the_rate_date() {
        let r = parse_ecb(
            &csv(&[("IDR", "2026-11-27", "20000"), ("JPY", "2026-11-25", "160")]),
            "JPY",
        )
        .unwrap();
        assert_eq!(r.as_of, day("2026-11-25"));
        assert_eq!(r.idr_per_unit, 125.0);
    }

    #[test]
    fn the_newest_observation_wins() {
        let r = parse_ecb(
            &csv(&[
                ("IDR", "2026-11-26", "10000"),
                ("IDR", "2026-11-27", "20000"),
                ("USD", "2026-11-27", "1"),
            ]),
            "USD",
        )
        .unwrap();
        assert_eq!(r.idr_per_unit, 20_000.0);
    }

    #[test]
    fn a_missing_series_is_an_error_not_a_guess() {
        let only_idr = csv(&[("IDR", "2026-11-27", "20136.83")]);
        assert!(parse_ecb(&only_idr, "USD").unwrap_err().contains("USD"));
        let only_usd = csv(&[("USD", "2026-11-27", "1.0957")]);
        assert!(parse_ecb(&only_usd, "USD").unwrap_err().contains("rupiah"));
        assert!(parse_ecb("", "USD").is_err());
        assert!(parse_ecb("not,a,rate\n1,2,3\n", "USD").is_err());
    }

    #[test]
    fn unusable_numbers_are_skipped_or_refused() {
        // a non-numeric observation is skipped, leaving the series missing
        let bad = csv(&[("IDR", "2026-11-27", "-"), ("USD", "2026-11-27", "1.0957")]);
        assert!(parse_ecb(&bad, "USD").is_err());
        // a zero divisor never becomes an infinite rate
        let zero = csv(&[("IDR", "2026-11-27", "20000"), ("USD", "2026-11-27", "0")]);
        assert!(parse_ecb(&zero, "USD").is_err());
    }

    #[test]
    fn rates_older_than_a_week_are_stale() {
        let rate = Rate {
            idr_per_unit: 1.0,
            as_of: day("2026-11-20"),
        };
        assert!(!too_old(&rate, day("2026-11-27")));
        assert!(too_old(&rate, day("2026-11-28")));
    }
}
