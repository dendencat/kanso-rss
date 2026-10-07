use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(
    version,
    about = "Kanso RSS API client. JSON output is suitable for jq and scripts."
)]
struct Args {
    #[arg(long, env = "KANSO_API_URL", default_value = "http://127.0.0.1:8080")]
    api: String,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Stats,
    Article {
        id: String,
    },
    Feeds,
    Add {
        url: String,
        #[arg(long, default_value = "")]
        title: String,
        #[arg(long, default_value = "")]
        folder: String,
    },
    Rename {
        id: String,
        title: String,
        #[arg(long, default_value = "")]
        folder: String,
    },
    Remove {
        id: String,
    },
    Articles {
        #[arg(long)]
        feed: Option<String>,
        #[arg(long)]
        folder: Option<String>,
        #[arg(long)]
        unread: bool,
        #[arg(long)]
        starred: bool,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value = "50")]
        limit: u32,
        #[arg(long, default_value = "0")]
        offset: u32,
    },
    Read {
        id: String,
        #[arg(long)]
        unread: bool,
    },
    Star {
        id: String,
        #[arg(long)]
        remove: bool,
    },
    MarkRead {
        #[arg(long)]
        feed: Option<String>,
        #[arg(long)]
        folder: Option<String>,
    },
    Refresh {
        #[arg(long)]
        feed: Option<String>,
        #[arg(long)]
        wait: bool,
    },
    Import {
        file: PathBuf,
    },
    Export {
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

struct Api {
    client: reqwest::Client,
    base: url::Url,
    token: String,
}
impl Api {
    fn new(base: &str, token: String) -> Result<Self> {
        let base = url::Url::parse(base)?;
        let loopback = match base.host() {
            Some(url::Host::Domain(host)) => host == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
        if base.scheme() != "https" && !(base.scheme() == "http" && loopback) {
            bail!("Remote APIs require HTTPS");
        }
        if !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || base.path() != "/"
        {
            bail!("Use an API origin without credentials, path, query or fragment");
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(40));
        let client = if loopback { client.no_proxy() } else { client };
        let client = client.build()?;
        Ok(Self {
            client,
            base,
            token,
        })
    }
    async fn call(
        &self,
        method: Method,
        path: &str,
        query: Vec<(&str, String)>,
        body: Option<Value>,
        raw: Option<String>,
    ) -> Result<String> {
        let mut request = self
            .client
            .request(method, self.base.join(&format!("/api/v1/{path}"))?)
            .bearer_auth(&self.token)
            .query(&query);
        if let Some(body) = body {
            request = request.json(&body);
        }
        if let Some(raw) = raw {
            request = request.header("content-type", "application/xml").body(raw);
        }
        let response = request.send().await.context("Cannot reach Kanso API")?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            bail!("API returned {status}: {text}");
        }
        Ok(text)
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    // Read the token only from the environment; command-line arguments can be
    // visible to other local users through process listings.
    let token = std::env::var("KANSO_TOKEN").context("Set KANSO_TOKEN")?;
    let api = Api::new(&args.api, token)?;
    let (method, path, query, body, raw) = match args.command {
        Command::Stats => (Method::GET, "stats".into(), vec![], None, None),
        Command::Article { id } => (Method::GET, format!("articles/{id}"), vec![], None, None),
        Command::Feeds => (Method::GET, "feeds".into(), vec![], None, None),
        Command::Add { url, title, folder } => (
            Method::POST,
            "feeds".into(),
            vec![],
            Some(json!({"url":url,"title":title,"folder":folder})),
            None,
        ),
        Command::Rename { id, title, folder } => (
            Method::PATCH,
            format!("feeds/{id}"),
            vec![],
            Some(json!({"title":title,"folder":folder})),
            None,
        ),
        Command::Remove { id } => (Method::DELETE, format!("feeds/{id}"), vec![], None, None),
        Command::Articles {
            feed,
            folder,
            unread,
            starred,
            search,
            limit,
            offset,
        } => {
            let mut q = vec![("limit", limit.to_string()), ("offset", offset.to_string())];
            if let Some(f) = feed {
                q.push(("feed_id", f));
            }
            if let Some(f) = folder {
                q.push(("folder", f));
            }
            if unread {
                q.push(("unread", "true".into()));
            }
            if starred {
                q.push(("starred", "true".into()));
            }
            if let Some(s) = search {
                q.push(("q", s));
            }
            (Method::GET, "articles".into(), q, None, None)
        }
        Command::Read { id, unread } => (
            Method::PATCH,
            format!("articles/{id}"),
            vec![],
            Some(json!({"read":!unread})),
            None,
        ),
        Command::Star { id, remove } => (
            Method::PATCH,
            format!("articles/{id}"),
            vec![],
            Some(json!({"starred":!remove})),
            None,
        ),
        Command::MarkRead { feed, folder } => (
            Method::POST,
            "mark-read".into(),
            vec![],
            Some(json!({"feed_id":feed,"folder":folder})),
            None,
        ),
        Command::Refresh { feed, wait } => {
            let path = feed
                .as_ref()
                .map(|id| format!("feeds/{id}/refresh"))
                .unwrap_or_else(|| "refresh".into());
            let text = api.call(Method::POST, &path, vec![], None, None).await?;
            if wait && feed.is_none() {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let status = api.call(Method::GET, "refresh", vec![], None, None).await?;
                    if !serde_json::from_str::<Value>(&status)?["running"]
                        .as_bool()
                        .unwrap_or(false)
                    {
                        println!("{status}");
                        return Ok(());
                    }
                }
            }
            println!("{text}");
            return Ok(());
        }
        Command::Import { file } => {
            if std::fs::metadata(&file)?.len() > 1024 * 1024 {
                bail!("OPML exceeds 1 MiB");
            }
            (
                Method::POST,
                "opml".into(),
                vec![],
                None,
                Some(std::fs::read_to_string(file)?),
            )
        }
        Command::Export { output } => {
            let xml = api.call(Method::GET, "opml", vec![], None, None).await?;
            if let Some(file) = output {
                std::fs::write(file, xml)?;
            } else {
                print!("{xml}");
            }
            return Ok(());
        }
    };
    let text = api.call(method, &path, query, body, raw).await?;
    if !text.is_empty() {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::from_str::<Value>(&text)?)?
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bearer_credentials_require_safe_origin() {
        assert!(Api::new("http://example.org", "token".into()).is_err());
        assert!(Api::new("https://user:pass@example.org", "token".into()).is_err());
        assert!(Api::new("https://example.org?token=leak", "token".into()).is_err());
        assert!(Api::new("http://127.0.0.1:8080", "token".into()).is_ok());
    }
}
