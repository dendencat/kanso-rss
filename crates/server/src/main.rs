use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Parser)]
#[command(version, about = "Kanso RSS — authenticated feed API and Web reader")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(long, default_value = "127.0.0.1:8080", env = "KANSO_LISTEN")]
    listen: SocketAddr,
    #[arg(long, default_value = "data/kanso.sqlite", env = "KANSO_DATABASE")]
    database: PathBuf,
    #[arg(long, env = "KANSO_ALLOW_REMOTE_HTTP")]
    allow_remote_http: bool,
    #[arg(long, default_value = "900")]
    refresh_seconds: u64,
}
#[derive(Subcommand)]
enum Command {
    /// Generate a 256-bit API token; store it in KANSO_TOKEN.
    Token,
    /// Create a consistent live backup. Destination must not already exist.
    Backup {
        #[arg(long)]
        output: PathBuf,
    },
    /// Check the local HTTP listener (for container health monitoring).
    Healthcheck,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    match &args.command {
        Some(Command::Token) => {
            println!("{}", kanso_server::token());
            return Ok(());
        }
        Some(Command::Backup { output }) => {
            if !args.database.is_file() {
                bail!("Source database does not exist");
            }
            kanso_core::store::Store::open(&args.database)?.backup(output)?;
            println!("Backup verified");
            return Ok(());
        }
        Some(Command::Healthcheck) => {
            let address = if args.listen.ip().is_unspecified() {
                SocketAddr::from(([127, 0, 0, 1], args.listen.port()))
            } else {
                args.listen
            };
            reqwest::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(3))
                .build()?
                .get(format!("http://{address}/healthz"))
                .send()
                .await?
                .error_for_status()?;
            return Ok(());
        }
        None => {}
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kanso_server=info".into()),
        )
        .init();
    if !args.listen.ip().is_loopback() && !args.allow_remote_http {
        bail!("Remote HTTP requires --allow-remote-http and a TLS reverse proxy");
    }
    let token = std::env::var("KANSO_TOKEN").context("Set KANSO_TOKEN using kanso-server token")?;
    if let Some(parent) = args.database.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&args.database)?;
        std::fs::set_permissions(&args.database, std::fs::Permissions::from_mode(0o600))?;
    }
    let state =
        kanso_server::AppState::new(kanso_core::store::Store::open(&args.database)?, &token)?;
    if args.refresh_seconds > 0 {
        let refresh = state.clone();
        let seconds = args.refresh_seconds.max(60);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(seconds));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if let Err(error) = kanso_server::start_refresh(refresh.clone()).await {
                    tracing::warn!(%error,"Scheduled refresh failed");
                }
            }
        });
    }
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!(address=%listener.local_addr()?,"Kanso RSS started");
    axum::serve(listener, kanso_server::router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
