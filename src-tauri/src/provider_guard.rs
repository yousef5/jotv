//! A circuit breaker for IPTV provider servers.
//!
//! Panels answer an account lock or a rate limit with odd 4xx codes (461, 469, 429) and tend to
//! make the block longer when they keep being hit. So after such an answer, requests to that host
//! are held back for a while instead of being sent: 10, 20, 40, 80, 160, then 240 minutes when it
//! happens again. One good answer resets it; a user action ("Try again") lifts the pause but keeps
//! the escalation. Same policy as JoTV Mobile.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct State {
    until: Option<Instant>,
    level: u32,
}

#[derive(Default)]
pub struct Guard {
    hosts: HashMap<String, State>,
}

/// 429 and the unusual 4xx codes; 400/401/403/404 are ordinary errors.
pub fn is_refusal(code: u16) -> bool {
    code == 429 || (430..=499).contains(&code)
}

pub fn pause_minutes(level: u32) -> u64 {
    (10u64 << level.saturating_sub(1).min(5)).min(240)
}

impl Guard {
    /// Err with a readable message while `host` is paused.
    pub fn check(&self, host: &str, now: Instant) -> Result<(), String> {
        match self.hosts.get(host).and_then(|s| s.until) {
            Some(until) if until > now => {
                let mins = ((until - now).as_secs() + 59) / 60;
                Err(format!(
                    "To protect your account, requests to the provider are paused for {} more minute{} because the server refused them. Try again later, or use \"Try anyway\".",
                    mins,
                    if mins == 1 { "" } else { "s" }
                ))
            }
            _ => Ok(()),
        }
    }

    pub fn record(&mut self, host: &str, code: u16, now: Instant) {
        if is_refusal(code) {
            let level = self.hosts.get(host).map_or(0, |s| s.level) + 1;
            let until = now + Duration::from_secs(pause_minutes(level) * 60);
            self.hosts.insert(host.to_string(), State { until: Some(until), level });
        } else if (200..400).contains(&code) {
            self.hosts.remove(host);
        }
    }

    pub fn try_again(&mut self) {
        for s in self.hosts.values_mut() {
            s.until = None;
        }
    }
}

fn global() -> &'static Mutex<Guard> {
    static G: OnceLock<Mutex<Guard>> = OnceLock::new();
    G.get_or_init(|| Mutex::new(Guard::default()))
}

fn host_of(url: &str) -> String {
    reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default()
}

/// Call before sending a request to a provider.
pub fn check(url: &str) -> Result<(), String> {
    global().lock().unwrap().check(&host_of(url), Instant::now())
}

/// Call with the status of every answer.
pub fn record(url: &str, status: u16) {
    global().lock().unwrap().record(&host_of(url), status, Instant::now());
}

/// The user asked to try anyway.
pub fn try_again() {
    global().lock().unwrap().try_again();
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = "provider.example";

    fn mins(m: u64) -> Duration {
        Duration::from_secs(m * 60)
    }

    #[test]
    fn ordinary_errors_do_not_pause() {
        let (mut g, now) = (Guard::default(), Instant::now());
        for code in [400, 401, 403, 404, 500, 502] {
            g.record(HOST, code, now);
        }
        assert!(g.check(HOST, now).is_ok());
    }

    #[test]
    fn a_refusal_pauses_that_host_only_and_ends_by_itself() {
        let (mut g, now) = (Guard::default(), Instant::now());
        g.record(HOST, 469, now);
        assert!(g.check(HOST, now).unwrap_err().contains("10 more minutes"));
        assert!(g.check("other.example", now).is_ok());
        assert!(g.check(HOST, now + mins(10) + Duration::from_secs(1)).is_ok());
    }

    #[test]
    fn repeated_refusals_pause_longer_up_to_four_hours() {
        let (mut g, mut now) = (Guard::default(), Instant::now());
        for expected in [10u64, 20, 40, 80, 160, 240, 240] {
            g.record(HOST, 461, now);
            assert!(g.check(HOST, now + mins(expected) - Duration::from_secs(1)).is_err(), "level {expected}");
            assert!(g.check(HOST, now + mins(expected) + Duration::from_secs(1)).is_ok(), "level {expected}");
            now += mins(expected) + Duration::from_secs(2);
        }
    }

    #[test]
    fn a_good_answer_resets_and_try_again_keeps_the_escalation() {
        let (mut g, now) = (Guard::default(), Instant::now());
        g.record(HOST, 469, now);
        g.record(HOST, 200, now);
        g.record(HOST, 429, now);
        assert!(g.check(HOST, now + mins(10) - Duration::from_secs(1)).is_err());
        assert!(g.check(HOST, now + mins(10) + Duration::from_secs(1)).is_ok());

        g.try_again();
        assert!(g.check(HOST, now).is_ok());
        g.record(HOST, 469, now);
        assert!(g.check(HOST, now + mins(20) - Duration::from_secs(1)).is_err(), "second refusal pauses 20 min");
    }
}
