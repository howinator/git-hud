use std::{str::FromStr, time::Duration};

use crate::strings;

pub fn log_duration(log_line: &str, duration: &Duration) {
    let log_level = match std::env::var(strings::LOG_LEVEL) {
        Ok(val) => val,
        Err(_) => String::from_str("").unwrap(),
    };
    if log_level == "debug" {
        println!(
            "{log_line} {duration:.2?}",
            log_line = log_line,
            duration = duration
        )
    }
}
