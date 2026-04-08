//! Token bucket rate limiter with per-IP tracking.

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, Response, StatusCode},
    middleware::Next,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub burst_size: u32,
    pub requests_per_second: f64,
    pub window_duration: Duration,
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            burst_size: 100,
            requests_per_second: 50.0,
            window_duration: Duration::from_secs(60),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    burst_size: u32,
    refill_rate: f64,
}

impl TokenBucket {
    fn new(burst_size: u32, refill_rate: f64) -> Self {
        Self {
            tokens: burst_size as f64,
            last_refill: Instant::now(),
            burst_size,
            refill_rate,
        }
    }

    /// Try to consume a token. Returns true if allowed, false if rate limited.
    fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.burst_size as f64);
        self.last_refill = now;
    }

    fn remaining(&self) -> u32 {
        self.tokens as u32
    }

    fn retry_after(&self) -> Duration {
        if self.tokens >= 1.0 {
            Duration::ZERO
        } else {
            let needed = 1.0 - self.tokens;
            Duration::from_secs_f64(needed / self.refill_rate)
        }
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    pub fn check(&self, ip: &str) -> RateLimitResult {
        if !self.config.enabled {
            return RateLimitResult::Allowed {
                remaining: self.config.burst_size,
                limit: self.config.burst_size,
            };
        }

        let mut buckets = self.buckets.lock().unwrap();
        let bucket = buckets.entry(ip.to_string()).or_insert_with(|| {
            TokenBucket::new(self.config.burst_size, self.config.requests_per_second)
        });

        if bucket.try_consume() {
            RateLimitResult::Allowed {
                remaining: bucket.remaining(),
                limit: self.config.burst_size,
            }
        } else {
            RateLimitResult::Limited {
                retry_after: bucket.retry_after(),
                limit: self.config.burst_size,
            }
        }
    }

    pub fn cleanup(&self) {
        let mut buckets = self.buckets.lock().unwrap();
        let now = Instant::now();
        buckets.retain(|_, bucket| {
            now.duration_since(bucket.last_refill) < self.config.window_duration
        });
    }

    pub fn stats(&self) -> RateLimitStats {
        let buckets = self.buckets.lock().unwrap();
        RateLimitStats {
            tracked_ips: buckets.len(),
            config: self.config.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum RateLimitResult {
    Allowed { remaining: u32, limit: u32 },
    Limited { retry_after: Duration, limit: u32 },
}

#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub tracked_ips: usize,
    pub config: RateLimitConfig,
}

pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    state: axum::extract::State<Arc<RateLimiter>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let ip = addr.ip().to_string();

    match state.check(&ip) {
        RateLimitResult::Allowed { remaining, limit } => {
            let mut response = next.run(request).await;

            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", limit.to_string().parse().unwrap());
            headers.insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );

            response
        }
        RateLimitResult::Limited { retry_after, limit } => {
            let mut response = Response::new(Body::from("Too Many Requests"));
            *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;

            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", limit.to_string().parse().unwrap());
            headers.insert("X-RateLimit-Remaining", "0".parse().unwrap());
            headers.insert(
                "Retry-After",
                retry_after.as_secs().to_string().parse().unwrap(),
            );

            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket() {
        let mut bucket = TokenBucket::new(10, 1.0);

        for _ in 0..10 {
            assert!(bucket.try_consume());
        }

        assert!(!bucket.try_consume());
    }

    #[test]
    fn test_rate_limiter() {
        let config = RateLimitConfig {
            burst_size: 5,
            requests_per_second: 1.0,
            window_duration: Duration::from_secs(60),
            enabled: true,
        };

        let limiter = RateLimiter::new(config);

        for _ in 0..5 {
            match limiter.check("127.0.0.1") {
                RateLimitResult::Allowed { .. } => {}
                RateLimitResult::Limited { .. } => panic!("Should be allowed"),
            }
        }

        match limiter.check("127.0.0.1") {
            RateLimitResult::Allowed { .. } => panic!("Should be limited"),
            RateLimitResult::Limited { .. } => {}
        }

        match limiter.check("192.168.1.1") {
            RateLimitResult::Allowed { .. } => {}
            RateLimitResult::Limited { .. } => panic!("Different IP should be allowed"),
        }
    }

    #[test]
    fn test_rate_limiter_disabled() {
        let config = RateLimitConfig {
            burst_size: 1,
            requests_per_second: 0.1,
            window_duration: Duration::from_secs(60),
            enabled: false,
        };

        let limiter = RateLimiter::new(config);

        for _ in 0..100 {
            match limiter.check("127.0.0.1") {
                RateLimitResult::Allowed { .. } => {}
                RateLimitResult::Limited { .. } => panic!("Should be allowed when disabled"),
            }
        }
    }
}
