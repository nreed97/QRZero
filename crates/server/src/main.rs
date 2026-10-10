//! Runs QRZero as a local web app: `qrzero-server [--data-dir DIR] [--bind ADDR:PORT] [--token TOKEN]`.

use std::path::PathBuf;

use qrzero_server::{default_data_dir, start, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let mut cfg = Config::local(default_data_dir());
    cfg.addr = "127.0.0.1:8073".parse()?;
    if let Ok(url) = std::env::var("QRZERO_PROPAGATION_URL") {
        cfg.propagation_url = url;
    }
    if let Ok(url) = std::env::var("QRZERO_CONTESTS_URL") {
        cfg.contests_url = url;
    }
    if let Ok(url) = std::env::var("QRZERO_UPDATES_URL") {
        cfg.updates_url = url;
    }
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| anyhow::anyhow!("{arg} needs a value"));
        match arg.as_str() {
            "--data-dir" => cfg.data_dir = PathBuf::from(value()?),
            "--bind" => cfg.addr = value()?.parse()?,
            "--token" => cfg.token = Some(value()?),
            "-h" | "--help" => {
                println!("qrzero-server [--data-dir DIR] [--bind 127.0.0.1:8073] [--token TOKEN]");
                return Ok(());
            }
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    let running = start(cfg).await?;
    println!("QRZero is running. Open {}", running.url());
    running.handle.await?;
    Ok(())
}
