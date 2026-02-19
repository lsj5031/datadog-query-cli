use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};

pub fn parse_to_unix(expr: &str, now: DateTime<Utc>) -> Result<i64> {
    let trimmed = expr.trim();
    if trimmed == "now" {
        return Ok(now.timestamp());
    }

    if let Ok(unix_seconds) = trimmed.parse::<i64>() {
        return Ok(unix_seconds);
    }

    if let Some(offset) = trimmed.strip_prefix("now-") {
        return parse_relative(offset, now);
    }

    let dt = chrono::DateTime::parse_from_rfc3339(trimmed)
        .map_err(|_| anyhow!("Unsupported time format `{trimmed}`"))?;
    Ok(dt.with_timezone(&Utc).timestamp())
}

fn parse_relative(offset: &str, now: DateTime<Utc>) -> Result<i64> {
    if offset.len() < 2 {
        return Err(anyhow!(
            "Invalid relative time `{offset}`. Expected e.g. now-15m."
        ));
    }

    let (value, unit) = offset.split_at(offset.len() - 1);
    let quantity = value
        .parse::<i64>()
        .map_err(|_| anyhow!("Invalid relative duration quantity `{value}`"))?;

    let duration = match unit {
        "s" => Duration::seconds(quantity),
        "m" => Duration::minutes(quantity),
        "h" => Duration::hours(quantity),
        "d" => Duration::days(quantity),
        "w" => Duration::weeks(quantity),
        _ => {
            return Err(anyhow!(
                "Invalid relative duration unit `{unit}`. Use one of s,m,h,d,w."
            ));
        }
    };

    Ok((now - duration).timestamp())
}
