use clap::Parser;
use log::info;
use tg_unblock::{run_proxy, AuthConfig};

#[derive(Parser, Debug)]
#[command(name = "tg_unblock")]
#[command(about = "Telegram Unblock - Command-line WebSocket proxy for bypassing Telegram blocking", long_about = None)]
struct Args {
    /// Bind address for SOCKS5 proxy
    #[arg(short, long, default_value = "127.0.0.1")]
    bind: String,

    /// Port to listen on for SOCKS5 connections
    #[arg(short, long, default_value = "1080")]
    port: u16,

    /// Enable verbose logging (debug level)
    #[arg(short, long)]
    verbose: bool,

    /// Show version and exit
    #[arg(long)]
    version: bool,
}

fn init_logging(verbose: bool) {
    let log_level = if verbose { "debug" } else { "info" };
    std::env::set_var("RUST_LOG", log_level);
    env_logger::init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.version {
        println!("tg_unblock {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    init_logging(args.verbose);

    // Check auth config and display info
    let _auth = AuthConfig::from_env();
    info!("Configured with authentication enabled: {}", _auth.enabled);
    info!("Using no-auth (0x00) mode - compatible with original tg-ws-proxy");

    info!("Starting tg_unblock v{}", env!("CARGO_PKG_VERSION"));
    info!("SOCKS5 proxy on {}:{}", args.bind, args.port);

    run_proxy(&args.bind, args.port).await?;

    Ok(())
}
