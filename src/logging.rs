use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use tracing::Level;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

struct UtcTime;

impl FormatTime for UtcTime {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", chrono::Utc::now().to_rfc3339())
    }
}

pub fn init(log_level: &str, log_file: Option<PathBuf>) {
    let level = Level::from_str(log_level).unwrap_or(Level::INFO);
    let filter = EnvFilter::from_default_env()
        .add_directive(level.into());

    let json_formatter = tracing_subscriber::fmt::format()
        .json()
        .with_timer(UtcTime)
        .with_target(true)
        .with_level(true);

    let stderr_layer = tracing_subscriber::fmt::layer()
        .event_format(json_formatter)
        .with_writer(std::io::stderr);

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer);

    if let Some(path) = log_file {
        match File::create(&path) {
            Ok(file) => {
                let file_formatter = tracing_subscriber::fmt::format()
                    .json()
                    .with_timer(UtcTime)
                    .with_target(true)
                    .with_level(true);

                let file_layer = tracing_subscriber::fmt::layer()
                    .event_format(file_formatter)
                    .with_writer(Mutex::new(file));

                subscriber.with(file_layer).init();
            }
            Err(err) => {
                subscriber.init();
                tracing::error!(path = %path.display(), error = %err, "Failed to create log file, logging to stderr only");
            }
        }
    } else {
        subscriber.init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_with_valid_level() {
        // Verify level parsing doesn't panic for valid levels
        let level = Level::from_str("info").unwrap();
        assert_eq!(level, Level::INFO);
        let level = Level::from_str("debug").unwrap();
        assert_eq!(level, Level::DEBUG);
        let level = Level::from_str("trace").unwrap();
        assert_eq!(level, Level::TRACE);
        let level = Level::from_str("warn").unwrap();
        assert_eq!(level, Level::WARN);
        let level = Level::from_str("error").unwrap();
        assert_eq!(level, Level::ERROR);
    }

    #[test]
    fn test_invalid_level_defaults_to_info() {
        let level = Level::from_str("nonsense");
        assert!(level.is_err());
    }
}
