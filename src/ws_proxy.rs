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

/// Telegram MTProto DC IDs and their WebSocket endpoints
/// Based on official Telegram documentation: https://core.telegram.org/mtproto/transports
/// Each Data Center (DC) has a namespaced WebSocket endpoint.
/// 
/// DC Range Mapping (from official mtproto implementation):
/// - DC1: 149.154.160.0/22 (first 4 subnets), 149.154.172.0/24 (4th octet 172-175)
/// - DC2: 149.154.164.0/22, 149.154.168.0/22, 149.154.172.0/24 (4th octet 172-175)
/// - DC3: 149.154.168.0/22 (4th octet 168-171), 91.108.8.0/22, 91.108.12.0/22
/// - DC4: 91.108.12.0/22 (4th octet 12-15)
/// - DC5: 91.108.56.0/22 (4th octet 56-59)
/// DC extraction from obfuscated2 init packet (same method as tg-ws-proxy)
///
/// The first 64 bytes of MTProto obfuscated2 connection contain:
/// - Bytes 8-39: AES key (32 bytes)
/// - Bytes 40-55: AES IV (16 bytes)
/// - Bytes 60-63: Encrypted DC ID (little-endian)
///
/// This function decrypts the DC ID using the contained key/IV.
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

    // Extract DC ID from the last 4 bytes (little-endian)
    let dc_id = i32::from_le_bytes([dec[60], dec[61], dec[62], dec[63]]);
    let dc = dc_id.unsigned_abs() as u8;
    
    // Telegram officially supports 5 Data Centers
    if (1..=5).contains(&dc) {
        Some(dc)
    } else {
        None
    }
}

/// Get WebSocket endpoint URL for a given DC
///
/// Uses the official tg-ws-proxy naming convention (kws prefix):
/// - DC1: kws1.web.telegram.org
/// - DC2: kws2.web.telegram.org
/// - DC3: kws3.web.telegram.org
/// - DC4: kws4.web.telegram.org
/// - DC5: kws5.web.telegram.org
fn ws_url(dc: u8) -> String {
    format!("wss://kws{}.web.telegram.org/apiws", dc)
}

/// Map IPv4 address to Telegram DC ID
///
/// Telegram uses specific subnet ranges for each Data Center.
/// This mapping is based on official mtproto implementation.
fn dc_from_ip(ip: Ipv4Addr) -> Option<u8> {
    let o = ip.octets();
    
    // DC1 & DC2 & DC3_alt (149.154.160-175.x.x)
    // The 4th octet range 172-175 is used by multiple DCs
    if o[0] == 149 && o[1] == 154 {
        match o[2] {
            160..=163 => Some(1),      // DC1 primary
            164..=167 => Some(2),      // DC2 primary
            168..=171 => Some(3),      // DC3 primary
            172..=175 => Some(1),      // DC1 alternate range (172-175 maps to DC1 per original code)
            _ => None,
        }
    }
    // DC3, DC4, DC5 (91.108.x.x)
    else if o[0] == 91 && o[1] == 108 {
        match o[2] {
            56..=59 => Some(5),        // DC5
            8..=11 => Some(3),         // DC3
            12..=15 => Some(4),        // DC4
            _ => None,
        }
    }
    // DC2 alternate ranges (documented in official mtproto)
    else if (o[0] == 91 && o[1] == 105) || (o[0] == 185 && o[1] == 76) {
        Some(2)
    } else {
        None
    }
}

/// Check if an address is a Telegram server IP
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

    let url = ws_url(dc);
    let mut request = url.as_str().into_client_request()?;

    // Required header for Telegram WebSocket (from mtproto spec)
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

    // Send the buffered 64-byte init as the first WebSocket message
    ws.send(tungstenite::Message::Binary(init.to_vec())).await?;

    // Single loop: handles TCP→WS, WS→TCP, and Ping/Pong in one place.
    // This ensures Pong replies are sent immediately so the server
    // doesn't kill the connection after a timeout.
    // Note: 32768 is a reasonable buffer size for high-throughput relay
    const RELAY_BUFFER_SIZE: usize = 32768;
    let mut buf = vec![0u8; RELAY_BUFFER_SIZE];

    loop {
        tokio::select! {
            biased;

            ws_msg = ws.next() => {
                match ws_msg {
                    Some(Ok(tungstenite::Message::Binary(data))) => {
                        if tcp_tx.write_all(data.as_ref()).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(tungstenite::Message::Ping(payload))) => {
                        // Telegram servers send regular pings, respond immediately
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

    // Gracefully close WebSocket connection
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

/// Handle SOCKS5 username/password authentication (RFC 1929)
///
/// RFC 1929 format:
/// - Request: [Version: 1][ULen: 1][UserName: ULen][PLen: 1][Password: PLen]
/// - Response: [Version: 1][Status: 1] (0=success, 1=failure)
async fn handle_auth(stream: &mut TcpStream, auth: &AuthConfig) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    // Read version byte (should be 0x01 for RFC 1929)
    let mut v_buf = [0u8; 1];
    stream.read_exact(&mut v_buf).await?;
    if v_buf[0] != 0x01 {
        return Ok(false);
    }

    // Read username length
    let mut ulen_buf = [0u8; 1];
    stream.read_exact(&mut ulen_buf).await?;
    let ulen = ulen_buf[0] as usize;
    
    info!("Auth request - username length: {}", ulen);

    // Read username
    let mut username_buf = vec![0u8; ulen];
    stream.read_exact(&mut username_buf).await?;
    info!("Read username: {:?}", String::from_utf8_lossy(&username_buf));

    // Read password length
    let mut plen_buf = [0u8; 1];
    stream.read_exact(&mut plen_buf).await?;
    let plen = plen_buf[0] as usize;
    
    info!("Auth request - password length: {}", plen);

    // Read password
    let mut password_buf = vec![0u8; plen];
    stream.read_exact(&mut password_buf).await?;

    // Verify credentials using constant-time comparison to prevent timing attacks
    use subtle::ConstantTimeEq;
    let valid = auth.username.as_ref()
        .map(|u| u.as_bytes().ct_eq(&username_buf).into())
        .unwrap_or(false)
        && auth.password.as_ref()
        .map(|p| p.as_bytes().ct_eq(&password_buf).into())
        .unwrap_or(false);
    
    info!("Auth valid={} (user_len={}, pass_len={})", valid, username_buf.len(), password_buf.len());

    if valid {
        // Success (RFC 1929: Version 1, Status 0)
        info!("Auth success, sending response [0x01, 0x00]");
        stream.write_all(&[0x01, 0x00]).await?;
        Ok(true)
    } else {
        // Failure (RFC 1929: Version 1, Status 1)
        // DO NOT log the username - this prevents user enumeration attacks
        info!("Auth failed, sending response [0x01, 0x01]");
        stream.write_all(&[0x01, 0x01]).await?;
        error!("Authentication failed for user attempt");
        Ok(false)
    }
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

    // Determine authentication methods to offer
    // Always offer no-auth (0x00) first for backward compatibility
    let mut methods = vec![0x00];
    // Offer user/pass auth (0x02) if enabled
    if auth.enabled && auth.username.is_some() && auth.password.is_some() {
        methods.push(0x02); // Username/Password auth (RFC 1929)
        info!("Offered auth methods: [no-auth, user/pass]");
    } else {
        info!("Offered auth methods: [no-auth only]");
    }

    // Send authentication methods
    let mut response = vec![0x05, methods.len() as u8];
    response.extend(methods);
    info!("Sending auth methods: {:?}", response);
    stream.write_all(&response).await?;
    
    info!("Sent {} bytes to client", response.len());

    // Read client's choice
    let mut method_buf = [0u8; 1];
    stream.read_exact(&mut method_buf).await?;
    let chosen_method = method_buf[0];
    
    info!("Client chose auth method: 0x{:02x}", chosen_method);

    if chosen_method == 0x02 {
        // Username/Password authentication
        if !handle_auth(&mut stream, auth).await? {
            return Err("Authentication failed".into());
        }
        info!("Authentication successful");
    } else if chosen_method == 0x00 {
        info!("No authentication required");
    } else {
        // No supported auth method - return method not found
        stream.write_all(&[0x05, 0xFF]).await?;
        return Err("No acceptable authentication method".into());
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

    // SOCKS5 success
    stream
        .write_all(&[0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0x04, 0x38])
        .await?;

    if is_tg {
        // Read the first 64 bytes — obfuscated2 init packet
        let mut init = [0u8; 64];
        stream.read_exact(&mut init).await?;

        // Extract DC from init packet (primary), fall back to IP-based
        let dc = extract_dc_from_init(&init).unwrap_or_else(|| {
            dest_addr
                .parse::<Ipv4Addr>()
                .ok()
                .and_then(dc_from_ip)
                .unwrap_or(2)  // Default to DC2 if all else fails
        });

        info!("Using WebSocket tunnel for DC{}", dc);

        // Try WebSocket tunnel
        let ws_result = relay_via_ws(stream, dc, &init).await;

        if let Err(e) = ws_result {
            error!("DC{} tunnel: {}", dc, e);
            return Err(format!("DC{} tunnel: {}", dc, e).into());
        }
    } else {
        // Non-Telegram — direct TCP passthrough
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
        info!("Authentication enabled for SOCKS5 proxy");
    } else {
        info!("Authentication disabled (set TG_UNBLOCK_AUTH=1 to enable)");
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
