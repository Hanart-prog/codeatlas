//! `codeatlas serve`.

use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;

use atlas_api::AppState;
use atlas_model::{AtlasConfig, AtlasError};
use tokio::net::TcpListener;

use crate::cli::ServeArgs;

pub fn run(args: ServeArgs, config: AtlasConfig, data_dir: &Path) -> Result<(), AtlasError> {
    let host = args.host.as_deref().unwrap_or(&config.server.host);
    let address = SocketAddr::new(
        parse_loopback(host)?,
        args.port.unwrap_or(config.server.port),
    );

    let database_path = crate::database_path(data_dir)?;
    if let Some(repository) = &args.repo {
        let database = atlas_storage::Database::open(&database_path)?;
        let entry = atlas_analysis::open_repository(&database, repository)?;
        tracing::info!(id = entry.id.0, "registered {}", entry.path);
    }
    let state = AppState::open(&database_path, config)?;

    let runtime = tokio::runtime::Runtime::new().map_err(|source| AtlasError::Io {
        context: "could not start the async runtime".to_owned(),
        source,
    })?;
    runtime.block_on(listen(address, state))
}

async fn listen(address: SocketAddr, state: AppState) -> Result<(), AtlasError> {
    let listener = TcpListener::bind(address)
        .await
        .map_err(|source| AtlasError::Io {
            context: format!("could not bind {address}"),
            source,
        })?;
    let local = listener.local_addr().map_err(|source| AtlasError::Io {
        context: "could not read the bound address".to_owned(),
        source,
    })?;

    let url = format!("http://{local}");
    println!("{}", serde_json::json!({ "url": url }));
    std::io::stdout().flush().map_err(|source| AtlasError::Io {
        context: "could not write to stdout".to_owned(),
        source,
    })?;
    tracing::info!("listening on {url}");

    atlas_api::serve(listener, state, shutdown_signal())
        .await
        .map_err(|source| AtlasError::Io {
            context: "server failed".to_owned(),
            source,
        })
}

/// The API has no authentication, so it must never be reachable off-machine.
fn parse_loopback(host: &str) -> Result<IpAddr, AtlasError> {
    if host.eq_ignore_ascii_case("localhost") {
        return Ok(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    match host.parse::<IpAddr>() {
        Ok(ip) if ip.is_loopback() => Ok(ip),
        Ok(_) => Err(AtlasError::InvalidInput(format!(
            "refusing to bind {host}: the API has no authentication, so only loopback addresses are allowed"
        ))),
        Err(_) => Err(AtlasError::InvalidInput(format!(
            "--host must be an IP address or `localhost`, got {host:?}"
        ))),
    }
}

async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::parse_loopback;

    #[test]
    fn accepts_loopback_addresses() {
        for host in ["127.0.0.1", "127.1.2.3", "::1", "localhost", "LocalHost"] {
            assert!(parse_loopback(host).unwrap().is_loopback(), "{host}");
        }
    }

    #[test]
    fn refuses_everything_else() {
        for host in [
            "0.0.0.0",
            "::",
            "192.168.1.10",
            "8.8.8.8",
            "example.com",
            "",
        ] {
            assert!(parse_loopback(host).is_err(), "{host}");
        }
    }
}
