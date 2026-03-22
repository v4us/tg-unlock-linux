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

    // Setup logging
    init_logging(args.verbose);

    // Check auth config and display info
    let auth = AuthConfig::from_env();
    if auth.enabled {
        info!("Authentication enabled (USERNAME={})", 
            auth.username.map(|u| format!("***{}***", u.len())).unwrap_or_else(|| "none".to_string()));
    } else {
        info!("Authentication disabled");
    }

    info!("Starting tg_unblock v{}", env!("CARGO_PKG_VERSION"));
    info!("SOCKS5 proxy on {}:{}", args.bind, args.port);

    run_proxy(&args.bind, args.port).await?;

    Ok(())
}
