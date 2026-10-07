//! All untrusted feed network access passes through this module.
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use reqwest::{redirect::Policy, Client};
use sha2::{Digest, Sha256};
use std::{net::IpAddr, time::Duration};
use url::Url;

pub const MAX_FEED_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 1000;

#[derive(Debug)]
pub struct FetchedFeed {
    pub title: String,
    pub articles: Vec<FetchedArticle>,
}

#[derive(Debug)]
pub struct FetchedArticle {
    pub key: String,
    pub title: String,
    pub url: Option<String>,
    pub content: String,
    pub published: String,
}

pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && (b == 0 || b == 168 || (b == 88 && c == 99)))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            // Only currently allocated global unicast; reject translation,
            // documentation, Teredo and other special-purpose ranges.
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

pub fn validate_url(value: &str) -> Result<Url> {
    if value.len() > 2048 {
        bail!("URL is too long");
    }
    let url = Url::parse(value).context("Invalid URL")?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        bail!("Use an HTTP(S) URL without credentials or a fragment");
    }
    if !matches!(url.port_or_known_default(), Some(80 | 443)) {
        bail!("Only ports 80 and 443 are allowed");
    }
    let host = url.host_str().context("Missing host")?;
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        bail!("Private network feeds are prohibited");
    }
    if let Some(url::Host::Ipv4(ip)) = url.host() {
        if !public_ip(IpAddr::V4(ip)) {
            bail!("Private address prohibited");
        }
    }
    if let Some(url::Host::Ipv6(ip)) = url.host() {
        if !public_ip(IpAddr::V6(ip)) {
            bail!("Private address prohibited");
        }
    }
    Ok(url)
}

pub async fn fetch(value: &str) -> Result<FetchedFeed> {
    tokio::time::timeout(Duration::from_secs(25), fetch_inner(value))
        .await
        .context("Feed request timed out")?
}

async fn fetch_inner(value: &str) -> Result<FetchedFeed> {
    let mut url = validate_url(value)?;
    for _ in 0..=5 {
        let host = url.host_str().context("Missing host")?;
        let port = url.port_or_known_default().context("Missing port")?;
        let addresses: Vec<_> = match url.host() {
            Some(url::Host::Ipv4(ip)) => vec![(IpAddr::V4(ip), port).into()],
            Some(url::Host::Ipv6(ip)) => vec![(IpAddr::V6(ip), port).into()],
            _ => tokio::net::lookup_host((host, port)).await?.collect(),
        };
        if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
            bail!("DNS resolved to a prohibited address");
        }
        // Pin the validated addresses to the connection to prevent DNS rebinding.
        // Ambient proxies would bypass this check; deliberately disable them.
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .resolve_to_addrs(host, &addresses)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent("KansoRSS/0.1")
            .build()?;
        let response = client.get(url.clone()).send().await?;
        if response.status().is_redirection() {
            let target = response
                .headers()
                .get(reqwest::header::LOCATION)
                .context("Redirect without Location")?
                .to_str()?;
            let next = validate_url(url.join(target)?.as_str())?;
            if url.scheme() == "https" && next.scheme() != "https" {
                bail!("HTTPS downgrade prohibited");
            }
            url = next;
            continue;
        }
        let response = response.error_for_status()?;
        if response
            .content_length()
            .is_some_and(|n| n > MAX_FEED_BYTES as u64)
        {
            bail!("Feed exceeds 2 MiB");
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if bytes.len() + chunk.len() > MAX_FEED_BYTES {
                bail!("Feed exceeds 2 MiB");
            }
            bytes.extend_from_slice(&chunk);
        }
        return tokio::task::spawn_blocking(move || parse_with_base(&bytes, Some(url.as_str())))
            .await?;
    }
    bail!("Too many redirects")
}

pub fn parse(bytes: &[u8]) -> Result<FetchedFeed> {
    parse_with_base(bytes, None)
}

fn parse_with_base(bytes: &[u8], base: Option<&str>) -> Result<FetchedFeed> {
    if bytes.len() > MAX_FEED_BYTES {
        bail!("Feed exceeds 2 MiB");
    }
    // Bound XML structure before invoking the semantic parser, including DTDs.
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut depth = 0usize;
    let mut elements = 0usize;
    loop {
        match reader.read_event_into(&mut buffer)? {
            quick_xml::events::Event::Start(_) => {
                depth += 1;
                elements += 1;
            }
            quick_xml::events::Event::Empty(_) => elements += 1,
            quick_xml::events::Event::End(_) => depth = depth.saturating_sub(1),
            quick_xml::events::Event::DocType(_) => bail!("Feed DTD is prohibited"),
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
        if depth > 64 || elements > 50_000 {
            bail!("Feed XML complexity limit exceeded");
        }
        buffer.clear();
    }
    let feed = feed_rs::parser::Builder::new()
        .base_uri(base)
        .id_generator(|links, title, _| {
            let identity = format!(
                "{}:{}",
                links.first().map(|l| l.href.as_str()).unwrap_or(""),
                title.as_ref().map(|t| t.content.as_str()).unwrap_or("")
            );
            format!("{:x}", Sha256::digest(identity.as_bytes()))
        })
        .build()
        .parse(bytes)
        .context("Invalid RSS/Atom feed")?;
    let title = feed
        .title
        .map(|v| v.content)
        .unwrap_or_else(|| "Untitled feed".into());
    let now = chrono::Utc::now().to_rfc3339();
    let articles = feed
        .entries
        .into_iter()
        .take(MAX_ENTRIES)
        .map(|entry| {
            let url = entry.links.iter().find_map(|l| safe_article_url(&l.href));
            let title = entry
                .title
                .map(|v| v.content)
                .unwrap_or_else(|| "Untitled article".into());
            let content = entry
                .content
                .and_then(|c| c.body)
                .or_else(|| entry.summary.map(|s| s.content))
                .unwrap_or_default();
            let identity = if !entry.id.is_empty() {
                entry.id
            } else {
                format!("{}:{}", url.as_deref().unwrap_or(""), title)
            };
            let key = format!("{:x}", Sha256::digest(identity.as_bytes()));
            FetchedArticle {
                key,
                title: title.chars().take(1000).collect(),
                url,
                content: content.chars().take(100_000).collect(),
                published: entry
                    .published
                    .or(entry.updated)
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|| now.clone()),
            }
        })
        .collect();
    Ok(FetchedFeed {
        title: title.chars().take(256).collect(),
        articles,
    })
}

pub fn safe_article_url(value: &str) -> Option<String> {
    let u = Url::parse(value).ok()?;
    (matches!(u.scheme(), "http" | "https") && u.username().is_empty() && u.password().is_none())
        .then(|| u.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ssrf_variants() {
        for value in [
            "http://127.0.0.1",
            "http://2130706433",
            "http://0x7f000001",
            "http://169.254.169.254/latest/meta-data",
            "http://100.64.0.1",
            "http://[::ffff:127.0.0.1]",
            "http://[::1]",
            "http://[fc00::1]",
            "file:///etc/passwd",
            "https://user:pass@example.org",
            "https://example.org:8443",
            "http://192.168.1.1",
            "http://localhost",
            "http://[2001:db8::1]",
        ] {
            assert!(validate_url(value).is_err(), "{value}");
        }
        assert!(validate_url("https://example.org/feed.xml").is_ok());
    }
    #[test]
    fn parses_atom_and_blocks_active_links() {
        let feed = parse(br#"<feed xmlns="http://www.w3.org/2005/Atom"><title>News</title><id>x</id><updated>2025-01-01T00:00:00Z</updated><entry><id>a</id><title>Hello</title><updated>2025-01-01T00:00:00Z</updated><link href="javascript:alert(1)"/><summary type="html">&lt;script&gt;alert(1)&lt;/script&gt;</summary></entry></feed>"#).unwrap();
        assert_eq!(feed.articles.len(), 1);
        assert!(feed.articles[0].url.is_none());
        assert!(parse(&vec![0; MAX_FEED_BYTES + 1]).is_err());
    }
    #[test]
    fn rejects_deep_xml_and_dtd_and_stable_missing_ids() {
        let deep = format!("{}{}", "<x>".repeat(70), "</x>".repeat(70));
        assert!(parse(deep.as_bytes()).is_err());
        assert!(parse(b"<!DOCTYPE rss><rss/>").is_err());
        let rss = br#"<rss version="2.0"><channel><title>X</title><link>https://example.org</link><description>x</description><item><title>Stable</title></item></channel></rss>"#;
        assert_eq!(
            parse(rss).unwrap().articles[0].key,
            parse(rss).unwrap().articles[0].key
        );
    }
}
