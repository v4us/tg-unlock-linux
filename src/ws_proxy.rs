use std::net::Ipv4Addr;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite;
use tungstenite::client::IntoClientRequest;
use log::{info, error};
use std::env;

/// SOCKS5 Auth config (from environment variables)
#[derive(Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let enabled = env::var("TG_UNBLOCK_AUTH")
            .ok()
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        let username = if enabled {
            env::var("TG_UNBLOCK_USERNAME").ok()
        } else {
            None
        };

        let password = if enabled {
            env::var("TG_UNBLOCK_PASSWORD").ok()
        } else {
            None
        };

        Self {
            enabled,
            username,
            password,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig::from_env()
    }
}

// DC extraction from obfuscated2 init packet (same method as tg-ws-proxy)
fn extract_dc_from_init(init: &[u8; 64]) -> Option<u8> {
    use aes::Aes256;
    use cipher::{KeyIvInit, StreamCipher};
    type Aes256Ctr = ctr::Ctr128BE<Aes256>;

    let key = &init[8..40];
    let iv = &init[40..56];

    let mut dec = [0u8; 64];
    dec.copy_from_slice(init);

    let mut cipher = Aes256Ctr::new(key.into(), iv.into());
    cipher.apply_keystream(&mut dec);

    let dc_id = i32::from_le_bytes([dec[60], dec[61], dec[62], dec[63]]);
    let dc = dc_id.unsigned_abs() as u8;
    
    if (1..=5).contains(&dc) {
        Some(dc)
    } else {
        None
    }
}

/// Get WebSocket endpoint URL for a given DC
fn ws_url(dc: u8) -> String {
    format!("wss://kws{}.web.telegram.org/apiws", dc)
}

/// Map IPv4 address to Telegram DC ID
fn dc_from_ip(ip: Ipv4Addr) -> Option<u8> {
    let o = ip.octets();
    
    if o[0] == 149 && o[1] == 154 {
        match o[2] {
            160..=163 => Some(1),
            164..=167 => Some(2),
            168..=171 => Some(3),
            172..=175 => Some(1),
            _ => None,
        }
    }
    else if o[0] == 91 && o[1] == 108 {
        match o[2] {
            56..=59 => Some(5),
            8..=11 => Some(3),
            12..=15 => Some(4),
            _ => None,
        }
    }
    else if (o[0] == 91 && o[1] == 105) || (o[0] == 185 && o[1] == 76) {
        Some(2)
    } else {
        None
    }
}

fn is_telegram_ip(addr: &str) -> bool {
    addr.parse::<Ipv4Addr>()
        .ok()
        .and_then(dc_from_ip)
        .is_some()
}

fn parse_dest(data: &[u8]) -> Result<(String, u16), Box<dyn std::error::Error + Send + Sync>> {
    match data[0] {
        0x01 => {
            if data.len() < 7 {
                return Err("short".into());
            }
            let ip = format!("{}.{}.{}.{}", data[1], data[2], data[3], data[4]);
            let port = u16::from_be_bytes([data[5], data[6]]);
            Ok((ip, port))
        }
        0x03 => {
            let len = data[1] as usize;
            if data.len() < 2 + len + 2 {
                return Err("short".into());
            }
            let domain = std::str::from_utf8(&data[2..2 + len])?.to_string();
            let port = u16::from_be_bytes([data[2 + len], data[3 + len]]);
            Ok((domain, port))
        }
        _ => Err("unknown addr type".into()),
    }
}

async fn relay_via_ws(
    tcp_stream: TcpStream,
    dc: u8,
    init: &[u8; 64],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use futures_util::{SinkExt, StreamExt};
    use tokio::time::{self, Duration};

    let url = ws_url(dc);
    let mut request = url.as_str().into_client_request()?;

    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "binary".parse()?);

    let connector = tokio_tungstenite::Connector::NativeTls(
        native_tls::TlsConnector::new().map_err(|e| format!("TLS: {}", e))?,
    );

    let (mut ws, _resp) = tokio_tungstenite::connect_async_tls_with_config(
        request, None, false, Some(connector),
    )
    .await?;

    let (mut tcp_rx, mut tcp_tx) = tokio::io::split(tcp_stream);

    ws.send(tungstenite::Message::Binary(init.to_vec())).await?;

    const RELAY_BUFFER_SIZE: usize = 32768;
    let mut buf = vec![0u8; RELAY_BUFFER_SIZE];

    // Ping interval: send ping every 30 seconds to keep connection alive
    let mut ping_interval = time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            biased;

            // Periodic ping to keep WebSocket alive
            _ = ping_interval.tick() => {
                let _ = ws.send(tungstenite::Message::Ping(vec![])).await;
            }

            ws_msg = ws.next() => {
                match ws_msg {
                    Some(Ok(tungstenite::Message::Binary(data))) => {
                        if tcp_tx.write_all(data.as_ref()).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(tungstenite::Message::Ping(payload))) => {
                        let _ = ws.send(tungstenite::Message::Pong(payload)).await;
                    }
                    Some(Ok(tungstenite::Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }

            n = tcp_rx.read(&mut buf) => {
                match n {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let msg = tungstenite::Message::Binary(buf[..n].to_vec());
                        if ws.send(msg).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }

    let _ = ws.close(None).await;
    Ok(())
}

async fn relay_tcp(client: TcpStream, remote: TcpStream) {
    let (mut cr, mut cw) = tokio::io::split(client);
    let (mut rr, mut rw) = tokio::io::split(remote);
    tokio::select! {
        _ = tokio::io::copy(&mut cr, &mut rw) => {}
        _ = tokio::io::copy(&mut rr, &mut cw) => {}
    }
}

async fn handle_auth(stream: &mut TcpStream, auth: &AuthConfig) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    use subtle::ConstantTimeEq;
    
    let mut v_buf = [0u8; 1];
    stream.read_exact(&mut v_buf).await?;
    if v_buf[0] != 0x01 {
        return Ok(false);
    }

    let mut ulen_buf = [0u8; 1];
    stream.read_exact(&mut ulen_buf).await?;
    let ulen = ulen_buf[0] as usize;

    let mut username_buf = vec![0u8; ulen];
    stream.read_exact(&mut username_buf).await?;

    let mut plen_buf = [0u8; 1];
    stream.read_exact(&mut plen_buf).await?;
    let plen = plen_buf[0] as usize;

    let mut password_buf = vec![0u8; plen];
    stream.read_exact(&mut password_buf).await?;

    let valid = auth.username.as_ref()
        .map(|u| u.as_bytes().ct_eq(&username_buf).into())
        .unwrap_or(false)
        && auth.password.as_ref()
        .map(|p| p.as_bytes().ct_eq(&password_buf).into())
        .unwrap_or(false);

    info!("Auth valid={}", valid);

    if valid {
        stream.write_all(&[0x01, 0x00]).await?;
        Ok(true)
    } else {
        stream.write_all(&[0x01, 0x01]).await?;
        error!("Authentication failed for user attempt");
        Ok(false)
    }
}

/// Choose the best authentication method from client's offers
fn select_auth_method(client_methods: &[u8], auth: &AuthConfig) -> Option<u8> {
    // If auth is enabled, prefer user-pass auth (0x02) regardless of offer order
    if auth.enabled {
        // First check if client offered user-pass auth
        for &method in client_methods {
            if method == 0x02 {
                return Some(0x02);
            }
        }
        // User-pass not offered, fall through to no-auth
    }
    
    // Check for no-auth (0x00) if offered
    for &method in client_methods {
        if method == 0x00 {
            return Some(0x00);
        }
    }
    
    None
}

async fn handle_socks5(mut stream: TcpStream, auth: &AuthConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    stream.set_nodelay(true)?;
    
    info!("New connection accepted");

    // --- auth negotiation ---
    let mut buf = [0u8; 258];
    let n = stream.read(&mut buf).await?;
    info!("Received {} bytes from client", n);
    
    if n < 2 || buf[0] != 0x05 {
        return Err("Not SOCKS5".into());
    }

    let nmethods = buf[1] as usize;
    if n < 2 + nmethods {
        info!("Need {} methods, only got {}", 2 + nmethods, n);
        return Err("Incomplete auth methods".into());
    }

    let offered_methods = &buf[2..2 + nmethods];
    info!("Client offered methods: {:?}", offered_methods);

    // Select the best method we support
    let chosen_method = select_auth_method(offered_methods, auth);
    
    match chosen_method {
        Some(method) => {
            // Send our method choice (version 5, chosen method)
            stream.write_all(&[0x05, method]).await?;
            info!("Chose method 0x{:02x}", method);

            if method == 0x02 {
                // Username/Password authentication
                if !handle_auth(&mut stream, auth).await? {
                    return Err("Authentication failed".into());
                }
                info!("Authentication successful");
            } else if method == 0x00 {
                info!("No authentication required (no-auth 0x00)");
            }
        }
        None => {
            // No acceptable method - return 0xFF
            stream.write_all(&[0x05, 0xFF]).await?;
            return Err("No acceptable authentication method".into());
        }
    }

    // --- CONNECT request ---
    let n = stream.read(&mut buf).await?;
    if n < 7 || buf[0] != 0x05 || buf[1] != 0x01 {
        stream.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
        return Err("Bad CONNECT".into());
    }

    let (dest_addr, dest_port) = parse_dest(&buf[3..n])?;
    let is_tg = is_telegram_ip(&dest_addr);

    info!("Connection: {}:{} (Telegram: {})", dest_addr, dest_port, is_tg);

    stream
        .write_all(&[0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0x04, 0x38])
        .await?;

    if is_tg {
        let mut init = [0u8; 64];
        stream.read_exact(&mut init).await?;

        let dc = extract_dc_from_init(&init).unwrap_or_else(|| {
            dest_addr
                .parse::<Ipv4Addr>()
                .ok()
                .and_then(dc_from_ip)
                .unwrap_or(2)
        });

        info!("Using WebSocket tunnel for DC{}", dc);

        let ws_result = relay_via_ws(stream, dc, &init).await;

        if let Err(e) = ws_result {
            error!("DC{} tunnel: {}", dc, e);
            return Err(format!("DC{} tunnel: {}", dc, e).into());
        }
    } else {
        let target = format!("{}:{}", dest_addr, dest_port);
        match TcpStream::connect(&target).await {
            Ok(remote) => {
                let _ = remote.set_nodelay(true);
                info!("Direct passthrough to {}", target);
                relay_tcp(stream, remote).await;
            }
            Err(e) => {
                return Err(format!("TCP connect {}: {}", target, e).into());
            }
        }
    }

    Ok(())
}

pub async fn run_proxy(bind: &str, port: u16) -> Result<(), String> {
    let auth = AuthConfig::from_env();
    
    if auth.enabled {
        info!("Authentication enabled (method 0x02 available)");
    } else {
        info!("Authentication disabled (only method 0x00 available)");
    }

    let addr = format!("{}:{}", bind, port);
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind to {}: {}", addr, e))?;

    info!("SOCKS5 proxy started on {}", addr);

    loop {
        tokio::select! {
            result = listener.accept() => {
                if let Ok((stream, _)) = result {
                    let auth = auth.clone();
                    tokio::spawn(async move {
                        let _ = handle_socks5(stream, &auth).await;
                    });
                }
            }
        }
    }
}
