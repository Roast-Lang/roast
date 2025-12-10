//! Time and date utilities.
//!
//! Time measurement, formatting, and parsing.

use std::time::{SystemTime, UNIX_EPOCH, Instant as StdInstant, Duration};

/// Get current Unix timestamp in seconds.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Get current Unix timestamp in milliseconds.
pub fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// Get current Unix timestamp in nanoseconds.
pub fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

/// A monotonic instant for measuring elapsed time.
pub struct Instant {
    inner: StdInstant,
}

impl Instant {
    /// Get the current instant.
    pub fn now() -> Self {
        Self { inner: StdInstant::now() }
    }
    
    /// Get elapsed time in seconds.
    pub fn elapsed_secs(&self) -> f64 {
        self.inner.elapsed().as_secs_f64()
    }
    
    /// Get elapsed time in milliseconds.
    pub fn elapsed_millis(&self) -> u128 {
        self.inner.elapsed().as_millis()
    }
    
    /// Get elapsed time in microseconds.
    pub fn elapsed_micros(&self) -> u128 {
        self.inner.elapsed().as_micros()
    }
    
    /// Get elapsed time in nanoseconds.
    pub fn elapsed_nanos(&self) -> u128 {
        self.inner.elapsed().as_nanos()
    }
}

/// Date/time components.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub nanosecond: u32,
    pub offset_seconds: i32,
}

impl DateTime {
    /// Create a new DateTime.
    pub fn new(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Self {
        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanosecond: 0,
            offset_seconds: 0,
        }
    }
    
    /// Get current local datetime.
    pub fn now() -> Self {
        // Simplified - just returns UTC
        Self::from_timestamp(now() as i64)
    }
    
    /// Create from Unix timestamp.
    pub fn from_timestamp(timestamp: i64) -> Self {
        // Days since epoch
        let days = timestamp / 86400;
        let remaining = timestamp % 86400;
        
        let hour = (remaining / 3600) as u8;
        let minute = ((remaining % 3600) / 60) as u8;
        let second = (remaining % 60) as u8;
        
        // Calculate date from days
        let (year, month, day) = days_to_date(days);
        
        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanosecond: 0,
            offset_seconds: 0,
        }
    }
    
    /// Convert to Unix timestamp.
    pub fn timestamp(&self) -> i64 {
        let days = date_to_days(self.year, self.month, self.day);
        days * 86400 + self.hour as i64 * 3600 + self.minute as i64 * 60 + self.second as i64
    }
    
    /// Get day of week (0 = Sunday, 6 = Saturday).
    pub fn weekday(&self) -> u8 {
        let days = date_to_days(self.year, self.month, self.day);
        ((days + 4) % 7) as u8 // Jan 1, 1970 was Thursday (4)
    }
    
    /// Get day of year (1-366).
    pub fn day_of_year(&self) -> u16 {
        let leap = is_leap_year(self.year);
        let days_before_month: [u16; 12] = if leap {
            [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335]
        } else {
            [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334]
        };
        days_before_month[self.month as usize - 1] + self.day as u16
    }
    
    /// Format as ISO 8601 string.
    pub fn to_iso8601(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day,
            self.hour, self.minute, self.second
        )
    }
    
    /// Format with custom format string.
    pub fn format(&self, fmt: &str) -> String {
        let weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                      "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        
        fmt.replace("%Y", &format!("{:04}", self.year))
           .replace("%m", &format!("{:02}", self.month))
           .replace("%d", &format!("{:02}", self.day))
           .replace("%H", &format!("{:02}", self.hour))
           .replace("%M", &format!("{:02}", self.minute))
           .replace("%S", &format!("{:02}", self.second))
           .replace("%a", weekdays[self.weekday() as usize])
           .replace("%b", months[self.month as usize - 1])
           .replace("%j", &format!("{:03}", self.day_of_year()))
    }
}

impl std::fmt::Display for DateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_iso8601())
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 0,
    }
}

fn date_to_days(year: i32, month: u8, day: u8) -> i64 {
    // Days from year 1 to year-1
    let y = year as i64 - 1;
    let mut days = y * 365 + y / 4 - y / 100 + y / 400;
    
    // Days from months
    let leap = is_leap_year(year);
    for m in 1..month {
        days += days_in_month(year, m) as i64;
    }
    
    // Days
    days += day as i64;
    
    // Adjust for epoch (Jan 1, 1970)
    days - 719528
}

fn days_to_date(days: i64) -> (i32, u8, u8) {
    // Add days from epoch to get days from year 1
    let days = days + 719528;
    
    // Estimate year
    let mut year = (days * 400 / 146097) as i32;
    
    // Adjust year
    while date_to_days(year + 1, 1, 1) <= days - 719528 {
        year += 1;
    }
    
    // Days remaining in year
    let mut remaining = days - date_to_days(year, 1, 1) - 719528;
    
    // Find month
    let mut month = 1u8;
    while remaining >= days_in_month(year, month) as i64 {
        remaining -= days_in_month(year, month) as i64;
        month += 1;
    }
    
    let day = (remaining + 1) as u8;
    
    (year, month, day)
}

/// Timer for measuring durations.
pub struct Timer {
    start: StdInstant,
    laps: Vec<Duration>,
}

impl Timer {
    /// Start a new timer.
    pub fn start() -> Self {
        Self {
            start: StdInstant::now(),
            laps: Vec::new(),
        }
    }
    
    /// Record a lap.
    pub fn lap(&mut self) -> Duration {
        let elapsed = self.start.elapsed();
        self.laps.push(elapsed);
        elapsed
    }
    
    /// Get total elapsed time.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
    
    /// Get all laps.
    pub fn laps(&self) -> &[Duration] {
        &self.laps
    }
    
    /// Reset the timer.
    pub fn reset(&mut self) {
        self.start = StdInstant::now();
        self.laps.clear();
    }
}

/// Sleep for a duration.
pub fn sleep(duration: Duration) {
    std::thread::sleep(duration);
}

/// Sleep for milliseconds.
pub fn sleep_millis(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// Sleep for seconds.
pub fn sleep_secs(secs: u64) {
    std::thread::sleep(Duration::from_secs(secs));
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_now() {
        let t = now();
        assert!(t > 1700000000); // After 2023
    }
    
    #[test]
    fn test_datetime() {
        let dt = DateTime::new(2024, 1, 15, 12, 30, 45);
        assert_eq!(dt.year, 2024);
        assert_eq!(dt.month, 1);
        assert_eq!(dt.day, 15);
    }
    
    #[test]
    fn test_format() {
        let dt = DateTime::new(2024, 1, 15, 12, 30, 45);
        assert_eq!(dt.format("%Y-%m-%d"), "2024-01-15");
        assert_eq!(dt.format("%H:%M:%S"), "12:30:45");
    }
    
    #[test]
    fn test_timestamp_roundtrip() {
        let ts: i64 = 1705320000; // 2024-01-15 12:00:00 UTC
        let dt = DateTime::from_timestamp(ts);
        assert_eq!(dt.timestamp(), ts);
    }
}

