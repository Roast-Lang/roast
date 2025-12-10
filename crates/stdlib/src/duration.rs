//! Duration utilities.

pub use std::time::Duration;

/// Create a duration from seconds.
pub fn seconds(secs: u64) -> Duration {
    Duration::from_secs(secs)
}

/// Create a duration from milliseconds.
pub fn millis(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

/// Create a duration from microseconds.
pub fn micros(us: u64) -> Duration {
    Duration::from_micros(us)
}

/// Create a duration from nanoseconds.
pub fn nanos(ns: u64) -> Duration {
    Duration::from_nanos(ns)
}

/// Create a duration from minutes.
pub fn minutes(mins: u64) -> Duration {
    Duration::from_secs(mins * 60)
}

/// Create a duration from hours.
pub fn hours(hrs: u64) -> Duration {
    Duration::from_secs(hrs * 3600)
}

/// Create a duration from days.
pub fn days(d: u64) -> Duration {
    Duration::from_secs(d * 86400)
}

/// Extension trait for Duration.
pub trait DurationExt {
    /// Get total seconds (including fractional).
    fn total_secs(&self) -> f64;
    
    /// Get total milliseconds.
    fn total_millis(&self) -> u128;
    
    /// Get total minutes.
    fn total_minutes(&self) -> f64;
    
    /// Get total hours.
    fn total_hours(&self) -> f64;
    
    /// Format as human-readable string.
    fn format(&self) -> String;
    
    /// Format with custom precision.
    fn format_precise(&self) -> String;
}

impl DurationExt for Duration {
    fn total_secs(&self) -> f64 {
        self.as_secs_f64()
    }
    
    fn total_millis(&self) -> u128 {
        self.as_millis()
    }
    
    fn total_minutes(&self) -> f64 {
        self.as_secs_f64() / 60.0
    }
    
    fn total_hours(&self) -> f64 {
        self.as_secs_f64() / 3600.0
    }
    
    fn format(&self) -> String {
        let secs = self.as_secs();
        
        if secs == 0 {
            let millis = self.as_millis();
            if millis == 0 {
                let micros = self.as_micros();
                if micros == 0 {
                    format!("{}ns", self.as_nanos())
                } else {
                    format!("{}µs", micros)
                }
            } else {
                format!("{}ms", millis)
            }
        } else if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else if secs < 86400 {
            let hours = secs / 3600;
            let mins = (secs % 3600) / 60;
            format!("{}h {}m", hours, mins)
        } else {
            let days = secs / 86400;
            let hours = (secs % 86400) / 3600;
            format!("{}d {}h", days, hours)
        }
    }
    
    fn format_precise(&self) -> String {
        let total = self.as_secs_f64();
        
        if total < 0.001 {
            format!("{:.3}µs", total * 1_000_000.0)
        } else if total < 1.0 {
            format!("{:.3}ms", total * 1000.0)
        } else if total < 60.0 {
            format!("{:.3}s", total)
        } else if total < 3600.0 {
            format!("{:.2}m", total / 60.0)
        } else if total < 86400.0 {
            format!("{:.2}h", total / 3600.0)
        } else {
            format!("{:.2}d", total / 86400.0)
        }
    }
}

/// Parse a duration string.
/// Supports: 1s, 1m, 1h, 1d, 100ms, 100us, 100ns
pub fn parse(s: &str) -> Result<Duration, ParseError> {
    let s = s.trim();
    
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    
    // Find where the number ends
    let num_end = s.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(s.len());
    
    if num_end == 0 {
        return Err(ParseError::NoNumber);
    }
    
    let number: f64 = s[..num_end].parse()
        .map_err(|_| ParseError::InvalidNumber)?;
    
    let unit = s[num_end..].trim();
    
    let duration = match unit {
        "ns" | "nanos" => Duration::from_nanos(number as u64),
        "us" | "µs" | "micros" => Duration::from_micros(number as u64),
        "ms" | "millis" => Duration::from_millis(number as u64),
        "s" | "sec" | "secs" | "second" | "seconds" | "" => Duration::from_secs_f64(number),
        "m" | "min" | "mins" | "minute" | "minutes" => Duration::from_secs_f64(number * 60.0),
        "h" | "hr" | "hrs" | "hour" | "hours" => Duration::from_secs_f64(number * 3600.0),
        "d" | "day" | "days" => Duration::from_secs_f64(number * 86400.0),
        "w" | "week" | "weeks" => Duration::from_secs_f64(number * 604800.0),
        _ => return Err(ParseError::InvalidUnit(unit.to_string())),
    };
    
    Ok(duration)
}

/// Parse error.
#[derive(Debug, Clone)]
pub enum ParseError {
    Empty,
    NoNumber,
    InvalidNumber,
    InvalidUnit(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Empty => write!(f, "Empty duration string"),
            ParseError::NoNumber => write!(f, "No number in duration string"),
            ParseError::InvalidNumber => write!(f, "Invalid number in duration string"),
            ParseError::InvalidUnit(u) => write!(f, "Invalid unit: {}", u),
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse() {
        assert_eq!(parse("1s").unwrap(), seconds(1));
        assert_eq!(parse("100ms").unwrap(), millis(100));
        assert_eq!(parse("5m").unwrap(), minutes(5));
        assert_eq!(parse("2h").unwrap(), hours(2));
        assert_eq!(parse("1d").unwrap(), days(1));
    }
    
    #[test]
    fn test_format() {
        assert_eq!(seconds(5).format(), "5s");
        assert_eq!(seconds(65).format(), "1m 5s");
        assert_eq!(seconds(3665).format(), "1h 1m");
        assert_eq!(millis(500).format(), "500ms");
    }
}

