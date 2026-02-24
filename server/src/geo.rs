use dashmap::DashMap;
use serde::Deserialize;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

// ── Types ──────────────────────────────────────────────────────────────────

/// Geolocation data for a single IP address.
#[derive(Debug, Clone)]
pub struct GeoInfo {
    pub country: String,
    pub region: String,
    pub city: String,
}

/// Thread-safe in-memory cache: IP string → Option<GeoInfo>.
/// `None` means we already tried and the lookup failed/returned no data.
#[derive(Clone, Debug)]
pub struct GeoCache {
    inner: Arc<DashMap<String, Option<GeoInfo>>>,
}

impl GeoCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }
}

impl Default for GeoCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── ip-api.com response shape ──────────────────────────────────────────────

#[derive(Deserialize)]
struct IpApiResponse {
    status: String,
    country: Option<String>,
    #[serde(rename = "regionName")]
    region_name: Option<String>,
    city: Option<String>,
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Look up geolocation for `ip`, using `cache` to avoid repeated network
/// requests for the same address.
///
/// Returns `None` for:
/// - private / loopback / link-local addresses
/// - failed or rate-limited API responses
/// - IPs that previously returned no useful data
///
/// The lookup is performed with a 3-second timeout so it can never stall a
/// background task for long.
pub async fn lookup(ip: &str, cache: &GeoCache) -> Option<GeoInfo> {
    // Skip addresses that can never be geolocated
    if is_private(ip) {
        return None;
    }

    // Check cache first (covers both successful hits and known misses)
    if let Some(entry) = cache.inner.get(ip) {
        return entry.clone();
    }

    // Not cached — ask ip-api.com
    let result = fetch_geo(ip).await;

    // Store in cache regardless of outcome so we don't retry endlessly
    cache.inner.insert(ip.to_owned(), result.clone());

    result
}

// ── Internal helpers ───────────────────────────────────────────────────────

async fn fetch_geo(ip: &str) -> Option<GeoInfo> {
    // Build a lightweight client with a strict timeout
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok()?;

    let url = format!(
        "http://ip-api.com/json/{}?fields=status,country,regionName,city",
        ip
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| tracing::debug!("geo lookup network error for {}: {}", ip, e))
        .ok()?;

    let body: IpApiResponse = resp
        .json()
        .await
        .map_err(|e| tracing::debug!("geo lookup parse error for {}: {}", ip, e))
        .ok()?;

    if body.status != "success" {
        tracing::debug!("geo lookup returned non-success status for {}", ip);
        return None;
    }

    let country = body.country.filter(|s| !s.is_empty()).unwrap_or_default();
    let region = body
        .region_name
        .filter(|s| !s.is_empty())
        .unwrap_or_default();
    let city = body.city.filter(|s| !s.is_empty()).unwrap_or_default();

    // Treat completely empty results as a miss
    if country.is_empty() && region.is_empty() && city.is_empty() {
        return None;
    }

    Some(GeoInfo {
        country,
        region,
        city,
    })
}

/// Return `true` for addresses that should never be sent to a public
/// geolocation API: loopback, link-local, private ranges, and IPv6 special
/// addresses.
fn is_private(ip_str: &str) -> bool {
    // Strip IPv6-mapped IPv4 prefix: "::ffff:1.2.3.4" → "1.2.3.4"
    let ip_str = ip_str.strip_prefix("::ffff:").unwrap_or(ip_str);

    match IpAddr::from_str(ip_str) {
        Ok(IpAddr::V4(addr)) => {
            let octets = addr.octets();
            addr.is_loopback()          // 127.x.x.x
            || addr.is_link_local()     // 169.254.x.x
            || addr.is_unspecified()    // 0.0.0.0
            || addr.is_broadcast()
            // 10.x.x.x
            || octets[0] == 10
            // 172.16.x.x – 172.31.x.x
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            // 192.168.x.x
            || (octets[0] == 192 && octets[1] == 168)
        }
        Ok(IpAddr::V6(addr)) => {
            addr.is_loopback()       // ::1
            || addr.is_unspecified() // ::
            // fe80::/10  link-local
            || (addr.segments()[0] & 0xffc0) == 0xfe80
            // fc00::/7   unique-local
            || (addr.segments()[0] & 0xfe00) == 0xfc00
        }
        Err(_) => true, // unparseable → treat as private / skip
    }
}
