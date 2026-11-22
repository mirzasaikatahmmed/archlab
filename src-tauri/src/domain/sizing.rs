//! Turns "how many users" into "how many requests per second", the number the simulator needs.

const SECONDS_PER_DAY: f64 = 86_400.0;

/// Requests per second at the busiest moment: the daily average times a peak factor.
pub fn peak_rps(daily_users: f64, requests_per_user_per_day: f64, peak_factor: f64) -> f64 {
    daily_users * requests_per_user_per_day / SECONDS_PER_DAY * peak_factor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_million_daily_users_at_twenty_requests_each_peak_near_twelve_hundred_per_second() {
        let peak = peak_rps(1_000_000.0, 20.0, 5.0);
        assert!((peak - 1_157.4).abs() < 0.1, "{peak}");
        assert!((peak_rps(1_000_000.0, 20.0, 1.0) - 231.5).abs() < 0.1);
    }

    #[test]
    fn it_scales_linearly() {
        assert!((peak_rps(2_000.0, 10.0, 3.0) - 2.0 * peak_rps(1_000.0, 10.0, 3.0)).abs() < 1e-9);
    }
}
