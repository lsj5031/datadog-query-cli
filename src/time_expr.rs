use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};

pub fn parse_time(expr: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    let trimmed = expr.trim();
    if trimmed == "now" {
        return Ok(now);
    }

    if let Ok(unix_seconds) = trimmed.parse::<i64>() {
        return chrono::DateTime::from_timestamp(unix_seconds, 0)
            .ok_or_else(|| anyhow!("Invalid unix timestamp `{unix_seconds}`"));
    }

    if let Some(offset) = trimmed.strip_prefix("now-") {
        return parse_relative(offset, now);
    }

    let dt = chrono::DateTime::parse_from_rfc3339(trimmed)
        .map_err(|_| anyhow!("Unsupported time format `{trimmed}`"))?;
    Ok(dt.with_timezone(&Utc))
}

fn parse_relative(offset: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
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

    Ok(now - duration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_parse_time() {
        let now = Utc.with_ymd_and_hms(2023, 1, 1, 12, 0, 0).unwrap();

        // now
        assert_eq!(parse_time("now", now).unwrap(), now);

        // unix timestamp
        assert_eq!(parse_time("1672574400", now).unwrap().timestamp(), 1672574400);

        // relative
        assert_eq!(parse_time("now-15m", now).unwrap(), now - Duration::minutes(15));
        assert_eq!(parse_time("now-2h", now).unwrap(), now - Duration::hours(2));
        assert_eq!(parse_time("now-1d", now).unwrap(), now - Duration::days(1));

        // rfc3339
        assert_eq!(
            parse_time("2023-01-01T10:00:00Z", now).unwrap(),
            Utc.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap()
        );

        // invalid
        assert!(parse_time("invalid", now).is_err());
        assert!(parse_time("now-15x", now).is_err());
        assert!(parse_time("now-m", now).is_err());
        assert!(parse_time("now-15", now).is_err());
    }
}
