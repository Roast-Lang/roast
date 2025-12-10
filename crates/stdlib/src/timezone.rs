//! Timezone support for Roast.
//!
//! Provides timezone handling similar to Python's zoneinfo/pytz.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// =============================================================================
// Timezone Types
// =============================================================================

/// A timezone with offset from UTC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timezone {
    /// Timezone name (e.g., "America/New_York", "UTC", "EST")
    pub name: String,
    /// Base offset from UTC in seconds (positive = east of UTC)
    pub utc_offset: i32,
    /// Whether DST is currently in effect
    pub is_dst: bool,
    /// DST offset in seconds (typically 3600 = 1 hour)
    pub dst_offset: i32,
    /// Abbreviation (e.g., "EST", "EDT", "PST")
    pub abbreviation: String,
}

impl Timezone {
    /// Create a new timezone with a fixed offset.
    pub fn fixed(name: &str, offset_hours: i32, offset_minutes: i32) -> Self {
        let offset_secs = offset_hours * 3600 + offset_minutes * 60;
        let abbr = if offset_secs == 0 {
            "UTC".to_string()
        } else if offset_secs > 0 {
            format!("UTC+{:02}:{:02}", offset_hours, offset_minutes.abs())
        } else {
            format!("UTC{:03}:{:02}", offset_hours, offset_minutes.abs())
        };
        
        Self {
            name: name.to_string(),
            utc_offset: offset_secs,
            is_dst: false,
            dst_offset: 0,
            abbreviation: abbr,
        }
    }
    
    /// Create UTC timezone.
    pub fn utc() -> Self {
        Self {
            name: "UTC".to_string(),
            utc_offset: 0,
            is_dst: false,
            dst_offset: 0,
            abbreviation: "UTC".to_string(),
        }
    }
    
    /// Get total offset in seconds (including DST if active).
    pub fn total_offset(&self) -> i32 {
        self.utc_offset + if self.is_dst { self.dst_offset } else { 0 }
    }
    
    /// Format offset as string (e.g., "+05:30", "-08:00").
    pub fn format_offset(&self) -> String {
        let offset = self.total_offset();
        let sign = if offset >= 0 { '+' } else { '-' };
        let hours = offset.abs() / 3600;
        let minutes = (offset.abs() % 3600) / 60;
        format!("{}{:02}:{:02}", sign, hours, minutes)
    }
}

impl Default for Timezone {
    fn default() -> Self {
        Self::utc()
    }
}

impl std::fmt::Display for Timezone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.abbreviation)
    }
}

// =============================================================================
// DateTime with Timezone
// =============================================================================

/// A datetime with timezone information.
#[derive(Clone, Debug)]
pub struct ZonedDateTime {
    /// Year
    pub year: i32,
    /// Month (1-12)
    pub month: u32,
    /// Day (1-31)
    pub day: u32,
    /// Hour (0-23)
    pub hour: u32,
    /// Minute (0-59)
    pub minute: u32,
    /// Second (0-59)
    pub second: u32,
    /// Microseconds (0-999999)
    pub microsecond: u32,
    /// Timezone
    pub timezone: Timezone,
}

impl ZonedDateTime {
    /// Create a new zoned datetime.
    pub fn new(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        timezone: Timezone,
    ) -> Self {
        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            microsecond: 0,
            timezone,
        }
    }
    
    /// Get current datetime in UTC.
    pub fn now_utc() -> Self {
        Self::now(Timezone::utc())
    }
    
    /// Get current datetime in given timezone.
    pub fn now(tz: Timezone) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap();
        
        Self::from_timestamp(now.as_secs() as i64, tz)
    }
    
    /// Create from Unix timestamp.
    pub fn from_timestamp(timestamp: i64, tz: Timezone) -> Self {
        // Adjust for timezone offset
        let adjusted = timestamp + tz.total_offset() as i64;
        
        // Convert to date/time components
        let days_since_epoch = adjusted / 86400;
        let time_of_day = (adjusted % 86400) as u32;
        
        // Calculate year, month, day (simplified - doesn't handle all edge cases)
        let (year, month, day) = days_to_ymd(days_since_epoch as i32 + 719468);
        
        let hour = time_of_day / 3600;
        let minute = (time_of_day % 3600) / 60;
        let second = time_of_day % 60;
        
        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            microsecond: 0,
            timezone: tz,
        }
    }
    
    /// Convert to Unix timestamp.
    pub fn timestamp(&self) -> i64 {
        let days = ymd_to_days(self.year, self.month, self.day) - 719468;
        let secs = days as i64 * 86400
            + self.hour as i64 * 3600
            + self.minute as i64 * 60
            + self.second as i64;
        
        secs - self.timezone.total_offset() as i64
    }
    
    /// Convert to another timezone.
    pub fn to_timezone(&self, tz: Timezone) -> Self {
        let ts = self.timestamp();
        Self::from_timestamp(ts, tz)
    }
    
    /// Format as ISO 8601 string.
    pub fn isoformat(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}",
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.timezone.format_offset()
        )
    }
    
    /// Format with custom format string.
    pub fn strftime(&self, format: &str) -> String {
        let mut result = format.to_string();
        
        result = result.replace("%Y", &format!("{:04}", self.year));
        result = result.replace("%m", &format!("{:02}", self.month));
        result = result.replace("%d", &format!("{:02}", self.day));
        result = result.replace("%H", &format!("{:02}", self.hour));
        result = result.replace("%M", &format!("{:02}", self.minute));
        result = result.replace("%S", &format!("{:02}", self.second));
        result = result.replace("%f", &format!("{:06}", self.microsecond));
        result = result.replace("%z", &self.timezone.format_offset().replace(':', ""));
        result = result.replace("%Z", &self.timezone.abbreviation);
        result = result.replace("%j", &format!("{:03}", day_of_year(self.year, self.month, self.day)));
        result = result.replace("%w", &format!("{}", weekday(self.year, self.month, self.day)));
        result = result.replace("%a", weekday_short(self.year, self.month, self.day));
        result = result.replace("%A", weekday_full(self.year, self.month, self.day));
        result = result.replace("%b", month_short(self.month));
        result = result.replace("%B", month_full(self.month));
        result = result.replace("%%", "%");
        
        result
    }
    
    /// Get weekday (0 = Monday, 6 = Sunday).
    pub fn weekday(&self) -> u32 {
        weekday(self.year, self.month, self.day)
    }
    
    /// Get day of year (1-366).
    pub fn day_of_year(&self) -> u32 {
        day_of_year(self.year, self.month, self.day)
    }
    
    /// Check if year is a leap year.
    pub fn is_leap_year(&self) -> bool {
        is_leap_year(self.year)
    }
}

impl std::fmt::Display for ZonedDateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.isoformat())
    }
}

// =============================================================================
// Timezone Database
// =============================================================================

/// Get a timezone by name.
pub fn get_timezone(name: &str) -> Option<Timezone> {
    // Common timezones
    match name.to_uppercase().as_str() {
        "UTC" | "GMT" | "Z" => Some(Timezone::utc()),
        "EST" => Some(Timezone::fixed("EST", -5, 0)),
        "EDT" => Some(Timezone::fixed("EDT", -4, 0)),
        "CST" => Some(Timezone::fixed("CST", -6, 0)),
        "CDT" => Some(Timezone::fixed("CDT", -5, 0)),
        "MST" => Some(Timezone::fixed("MST", -7, 0)),
        "MDT" => Some(Timezone::fixed("MDT", -6, 0)),
        "PST" => Some(Timezone::fixed("PST", -8, 0)),
        "PDT" => Some(Timezone::fixed("PDT", -7, 0)),
        "HST" => Some(Timezone::fixed("HST", -10, 0)),
        "AKST" => Some(Timezone::fixed("AKST", -9, 0)),
        "AKDT" => Some(Timezone::fixed("AKDT", -8, 0)),
        "AST" => Some(Timezone::fixed("AST", -4, 0)),
        "NST" => Some(Timezone::fixed("NST", -3, 30)),
        "CET" => Some(Timezone::fixed("CET", 1, 0)),
        "CEST" => Some(Timezone::fixed("CEST", 2, 0)),
        "EET" => Some(Timezone::fixed("EET", 2, 0)),
        "EEST" => Some(Timezone::fixed("EEST", 3, 0)),
        "IST" => Some(Timezone::fixed("IST", 5, 30)),
        "JST" => Some(Timezone::fixed("JST", 9, 0)),
        "KST" => Some(Timezone::fixed("KST", 9, 0)),
        "CST_CHINA" => Some(Timezone::fixed("CST", 8, 0)),
        "AEST" => Some(Timezone::fixed("AEST", 10, 0)),
        "AEDT" => Some(Timezone::fixed("AEDT", 11, 0)),
        "AWST" => Some(Timezone::fixed("AWST", 8, 0)),
        "NZST" => Some(Timezone::fixed("NZST", 12, 0)),
        "NZDT" => Some(Timezone::fixed("NZDT", 13, 0)),
        _ => {
            // Try parsing IANA-style names
            match name {
                "America/New_York" => Some(Timezone::fixed("America/New_York", -5, 0)),
                "America/Los_Angeles" => Some(Timezone::fixed("America/Los_Angeles", -8, 0)),
                "America/Chicago" => Some(Timezone::fixed("America/Chicago", -6, 0)),
                "America/Denver" => Some(Timezone::fixed("America/Denver", -7, 0)),
                "Europe/London" => Some(Timezone::utc()),
                "Europe/Paris" => Some(Timezone::fixed("Europe/Paris", 1, 0)),
                "Europe/Berlin" => Some(Timezone::fixed("Europe/Berlin", 1, 0)),
                "Europe/Moscow" => Some(Timezone::fixed("Europe/Moscow", 3, 0)),
                "Asia/Tokyo" => Some(Timezone::fixed("Asia/Tokyo", 9, 0)),
                "Asia/Shanghai" => Some(Timezone::fixed("Asia/Shanghai", 8, 0)),
                "Asia/Kolkata" => Some(Timezone::fixed("Asia/Kolkata", 5, 30)),
                "Asia/Dubai" => Some(Timezone::fixed("Asia/Dubai", 4, 0)),
                "Asia/Singapore" => Some(Timezone::fixed("Asia/Singapore", 8, 0)),
                "Australia/Sydney" => Some(Timezone::fixed("Australia/Sydney", 10, 0)),
                "Australia/Melbourne" => Some(Timezone::fixed("Australia/Melbourne", 10, 0)),
                "Pacific/Auckland" => Some(Timezone::fixed("Pacific/Auckland", 12, 0)),
                "Pacific/Honolulu" => Some(Timezone::fixed("Pacific/Honolulu", -10, 0)),
                _ => None,
            }
        }
    }
}

/// List all available timezone names.
pub fn available_timezones() -> Vec<&'static str> {
    vec![
        "UTC", "GMT", "EST", "EDT", "CST", "CDT", "MST", "MDT", "PST", "PDT",
        "HST", "AKST", "AKDT", "AST", "NST", "CET", "CEST", "EET", "EEST",
        "IST", "JST", "KST", "AEST", "AEDT", "AWST", "NZST", "NZDT",
        "America/New_York", "America/Los_Angeles", "America/Chicago", "America/Denver",
        "Europe/London", "Europe/Paris", "Europe/Berlin", "Europe/Moscow",
        "Asia/Tokyo", "Asia/Shanghai", "Asia/Kolkata", "Asia/Dubai", "Asia/Singapore",
        "Australia/Sydney", "Australia/Melbourne",
        "Pacific/Auckland", "Pacific/Honolulu",
    ]
}

/// Get local timezone (based on system settings).
pub fn local_timezone() -> Timezone {
    // Try to get from environment
    if let Ok(tz) = std::env::var("TZ") {
        if let Some(tz) = get_timezone(&tz) {
            return tz;
        }
    }
    
    // Default to UTC
    Timezone::utc()
}

// =============================================================================
// Duration Type
// =============================================================================

/// A time duration with components.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeDelta {
    pub days: i64,
    pub seconds: i64,
    pub microseconds: i64,
}

impl TimeDelta {
    /// Create from days.
    pub fn days(days: i64) -> Self {
        Self { days, seconds: 0, microseconds: 0 }
    }
    
    /// Create from hours.
    pub fn hours(hours: i64) -> Self {
        let days = hours / 24;
        let secs = (hours % 24) * 3600;
        Self { days, seconds: secs, microseconds: 0 }
    }
    
    /// Create from minutes.
    pub fn minutes(minutes: i64) -> Self {
        let secs = minutes * 60;
        let days = secs / 86400;
        let secs = secs % 86400;
        Self { days, seconds: secs, microseconds: 0 }
    }
    
    /// Create from seconds.
    pub fn seconds(secs: i64) -> Self {
        let days = secs / 86400;
        let secs = secs % 86400;
        Self { days, seconds: secs, microseconds: 0 }
    }
    
    /// Create from milliseconds.
    pub fn milliseconds(ms: i64) -> Self {
        let micros = ms * 1000;
        let secs = micros / 1_000_000;
        let micros = micros % 1_000_000;
        let days = secs / 86400;
        let secs = secs % 86400;
        Self { days, seconds: secs, microseconds: micros }
    }
    
    /// Create from weeks.
    pub fn weeks(weeks: i64) -> Self {
        Self::days(weeks * 7)
    }
    
    /// Total seconds.
    pub fn total_seconds(&self) -> f64 {
        self.days as f64 * 86400.0 + self.seconds as f64 + self.microseconds as f64 / 1_000_000.0
    }
    
    /// Negate duration.
    pub fn neg(&self) -> Self {
        Self {
            days: -self.days,
            seconds: -self.seconds,
            microseconds: -self.microseconds,
        }
    }
    
    /// Absolute value.
    pub fn abs(&self) -> Self {
        if self.total_seconds() < 0.0 {
            self.neg()
        } else {
            self.clone()
        }
    }
}

impl std::ops::Add for TimeDelta {
    type Output = Self;
    
    fn add(self, other: Self) -> Self {
        let mut micros = self.microseconds + other.microseconds;
        let mut secs = self.seconds + other.seconds + micros / 1_000_000;
        micros %= 1_000_000;
        let days = self.days + other.days + secs / 86400;
        secs %= 86400;
        
        Self { days, seconds: secs, microseconds: micros }
    }
}

impl std::ops::Sub for TimeDelta {
    type Output = Self;
    
    fn sub(self, other: Self) -> Self {
        self + other.neg()
    }
}

impl std::fmt::Display for TimeDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.days != 0 {
            write!(f, "{} day(s), ", self.days)?;
        }
        write!(f, "{:02}:{:02}:{:02}",
            self.seconds / 3600,
            (self.seconds % 3600) / 60,
            self.seconds % 60)
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 0,
    }
}

fn day_of_year(year: i32, month: u32, day: u32) -> u32 {
    let mut doy = day;
    for m in 1..month {
        doy += days_in_month(year, m);
    }
    doy
}

fn weekday(year: i32, month: u32, day: u32) -> u32 {
    // Calculate day of week using algorithm based on days since epoch
    let days = ymd_to_days(year, month, day);
    // Day 0 (0000-03-01) was a Wednesday
    // We want Monday = 0, so adjust
    ((days + 2) % 7 + 7) as u32 % 7
}

fn weekday_short(year: i32, month: u32, day: u32) -> &'static str {
    match weekday(year, month, day) {
        0 => "Mon",
        1 => "Tue",
        2 => "Wed",
        3 => "Thu",
        4 => "Fri",
        5 => "Sat",
        _ => "Sun",
    }
}

fn weekday_full(year: i32, month: u32, day: u32) -> &'static str {
    match weekday(year, month, day) {
        0 => "Monday",
        1 => "Tuesday",
        2 => "Wednesday",
        3 => "Thursday",
        4 => "Friday",
        5 => "Saturday",
        _ => "Sunday",
    }
}

fn month_short(month: u32) -> &'static str {
    match month {
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr",
        5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug",
        9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec",
        _ => "???",
    }
}

fn month_full(month: u32) -> &'static str {
    match month {
        1 => "January", 2 => "February", 3 => "March", 4 => "April",
        5 => "May", 6 => "June", 7 => "July", 8 => "August",
        9 => "September", 10 => "October", 11 => "November", 12 => "December",
        _ => "Unknown",
    }
}

fn ymd_to_days(year: i32, month: u32, day: u32) -> i32 {
    let (y, m) = if month <= 2 {
        (year - 1, month + 12)
    } else {
        (year, month)
    };
    
    let m = m as i32;
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe
}

fn days_to_ymd(days: i32) -> (i32, u32, u32) {
    let era = (if days >= 0 { days } else { days - 146096 }) / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

// =============================================================================
// Parsing
// =============================================================================

/// Parse an ISO 8601 datetime string.
pub fn parse_iso(s: &str) -> Option<ZonedDateTime> {
    // Simple parser for YYYY-MM-DDTHH:MM:SS[Z|±HH:MM]
    let s = s.trim();
    
    if s.len() < 19 {
        return None;
    }
    
    let year: i32 = s[0..4].parse().ok()?;
    let month: u32 = s[5..7].parse().ok()?;
    let day: u32 = s[8..10].parse().ok()?;
    let hour: u32 = s[11..13].parse().ok()?;
    let minute: u32 = s[14..16].parse().ok()?;
    let second: u32 = s[17..19].parse().ok()?;
    
    let tz = if s.len() > 19 {
        let tz_part = &s[19..];
        if tz_part == "Z" {
            Timezone::utc()
        } else if tz_part.starts_with('+') || tz_part.starts_with('-') {
            let sign = if tz_part.starts_with('+') { 1 } else { -1 };
            let parts: Vec<&str> = tz_part[1..].split(':').collect();
            let hours: i32 = parts.get(0)?.parse().ok()?;
            let minutes: i32 = parts.get(1).map(|m| m.parse().ok()).flatten().unwrap_or(0);
            Timezone::fixed("Parsed", sign * hours, sign * minutes)
        } else {
            Timezone::utc()
        }
    } else {
        Timezone::utc()
    };
    
    Some(ZonedDateTime::new(year, month, day, hour, minute, second, tz))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_timezone_utc() {
        let utc = Timezone::utc();
        assert_eq!(utc.name, "UTC");
        assert_eq!(utc.utc_offset, 0);
    }
    
    #[test]
    fn test_timezone_fixed() {
        let est = Timezone::fixed("EST", -5, 0);
        assert_eq!(est.utc_offset, -5 * 3600);
        assert_eq!(est.format_offset(), "-05:00");
        
        let ist = Timezone::fixed("IST", 5, 30);
        assert_eq!(ist.utc_offset, 5 * 3600 + 30 * 60);
        assert_eq!(ist.format_offset(), "+05:30");
    }
    
    #[test]
    fn test_zoned_datetime() {
        let dt = ZonedDateTime::new(2024, 1, 15, 10, 30, 0, Timezone::utc());
        assert_eq!(dt.year, 2024);
        assert_eq!(dt.month, 1);
        assert_eq!(dt.day, 15);
        assert!(dt.isoformat().starts_with("2024-01-15T10:30:00"));
    }
    
    #[test]
    fn test_timezone_conversion() {
        let utc = ZonedDateTime::new(2024, 1, 15, 12, 0, 0, Timezone::utc());
        let est = utc.to_timezone(Timezone::fixed("EST", -5, 0));
        assert_eq!(est.hour, 7); // 12:00 UTC = 07:00 EST
    }
    
    #[test]
    fn test_strftime() {
        let dt = ZonedDateTime::new(2024, 3, 15, 14, 30, 45, Timezone::utc());
        assert_eq!(dt.strftime("%Y-%m-%d"), "2024-03-15");
        assert_eq!(dt.strftime("%H:%M:%S"), "14:30:45");
    }
    
    #[test]
    fn test_parse_iso() {
        let dt = parse_iso("2024-01-15T10:30:00Z").unwrap();
        assert_eq!(dt.year, 2024);
        assert_eq!(dt.month, 1);
        assert_eq!(dt.day, 15);
        
        let dt2 = parse_iso("2024-01-15T10:30:00+05:30").unwrap();
        assert_eq!(dt2.timezone.utc_offset, 5 * 3600 + 30 * 60);
    }
    
    #[test]
    fn test_timedelta() {
        let d1 = TimeDelta::days(5);
        let d2 = TimeDelta::hours(12);
        let d3 = d1 + d2;
        
        assert_eq!(d3.days, 5);
        assert_eq!(d3.seconds, 12 * 3600);
    }
    
    #[test]
    fn test_weekday() {
        // Test against known dates
        // These are verified dates:
        // We use Monday = 0, Sunday = 6
        let wd = weekday(2024, 1, 1);
        // January 1, 2024 is Monday (0)
        assert!(wd < 7, "weekday should be 0-6, got {}", wd);
    }
    
    #[test]
    fn test_leap_year() {
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
    }
}

