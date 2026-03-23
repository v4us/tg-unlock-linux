use std::net::Ipv4Addr;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite;
use tungstenite::client::IntoClientRequest;
use log::error;
use log::info;
use std::env;
// Removed unused imports: Socket_ADDR, AtomicBool, AtomicU32, Ordering

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

/// Trusted IP records with timestamps
pub struct TrustedIps {
    map: Mutex<HashMap<String, SystemTime>>,
}

impl TrustedIps {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            map: Mutex::new(HashMap::new()),
        })
    }

    pub async fn is_trusted(&self, ip: &str) -> bool {
        let now = SystemTime::now();
        let ten_mins = Duration::from_secs(600); // 10 minutes
        
        let map = self.map.lock().await;
        if let Some(time) = map.get(ip) {
            if now.duration_since(*time).ok().map(|d| d < ten_mins).unwrap_or(false) {
                return true;
            }
        }
        false
    }

    pub async fn record_connection(&self, ip: &str) {
        let now = SystemTime::now();
        let mut map = self.map.lock().await;
        map.insert(ip.to_string(), now);
    }

    pub async fn cleanup_expired(&self) {
        let now = SystemTime::now();
        let ten_mins = Duration::from_secs(600);
        
        let mut map = self.map.lock().await;
        let before = map.len();
        map.retain(|_, time| now.duration_since(*time).ok().map(|d| d < ten_mins).unwrap_or(false));
        let after = map.len();
        if before != after {
            info!("Trusted IP cleanup: removed {} expired entries ({} remaining)", before - after, after);
        }
    }

    pub async fn get_ip_stats(&self) -> HashMap<String, u64> {
        let map = self.map.lock().await;
        map.iter().map(|(ip, _)| (ip.clone(), 1)).collect()
    }
}

// Connection tracker per IP to limit concurrent connections (prevents memory leak under load)
#[derive(Clone)]
struct ConnectionTracker {
    // Arc-wrapped cloneable data
    data: Arc<Mutex<ConnectionData>>,
}

// The actual data that can be cloned
#[derive(Clone)]
struct ConnectionData {
    ips: HashMap<String, Vec<std::time::SystemTime>>,
    max_connections: u32,
}

impl ConnectionTracker {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            data: Arc::new(Mutex::new(ConnectionData {
                ips: HashMap::new(),
                max_connections: 100, // Limit to 100 concurrent connections per IP (raised from 10)
            })),
        })
    }

    async fn try_acquire(&self, ip: &str) -> bool {
        let now = std::time::SystemTime::now();
        let max_connections = self.data.lock().await.max_connections;
        
        let mut data = self.data.lock().await;
        let connections = data.ips.entry(ip.to_string()).or_insert_with(Vec::new);
        
        // Check if already at max - if so, remove oldest (FIFO)
        if connections.len() >= max_connections as usize {
            connections.remove(0); // Remove oldest connection
        }
        
        // Add new connection
        connections.push(now);
        true
    }

    async fn release(&self, ip: &str) {
        let mut data = self.data.lock().await;
        if let Some(connections) = data.ips.get_mut(ip) {
            connections.pop();
            if connections.is_empty() {
                data.ips.remove(ip);
            }
        }
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
    conn_tracker: Arc<ConnectionTracker>,
    client_ip: String,
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

    // Add 10-second timeout for WebSocket connection (prevents pileup under load)
    let ws_result = time::timeout(Duration::from_secs(10), 
        tokio_tungstenite::connect_async_tls_with_config(
            request, None, false, Some(connector),
        )
    ).await;

    let (mut ws, _resp) = match ws_result {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => {
            return Err(format!("WebSocket connect error: {}", e).into());
        }
        Err(_) => {
            return Err(format!("WebSocket connect timeout for DC{}", dc).into());
        }
    };

    let (mut tcp_rx, mut tcp_tx) = tokio::io::split(tcp_stream);

    ws.send(tungstenite::Message::Binary(init.to_vec())).await?;

    const RELAY_BUFFER_SIZE: usize = 32768;
    let mut buf = vec![0u8; RELAY_BUFFER_SIZE];

    // Ping interval: send ping every 5 seconds to keep inbound connection alive
    let mut ping_interval = time::interval(Duration::from_secs(5));

    // Use try-finally pattern to ensure connection is released
    let result = loop {
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
                            break Ok(());
                        }
                    }
                    Some(Ok(tungstenite::Message::Ping(payload))) => {
                        let _ = ws.send(tungstenite::Message::Pong(payload)).await;
                    }
                    Some(Ok(tungstenite::Message::Close(_))) | None => break Ok(()),
                    Some(Err(_)) => break Ok(()),
                    _ => {}
                }
            }

            n = tcp_rx.read(&mut buf) => {
                match n {
                    Ok(0) | Err(_) => break Ok(()),
                    Ok(n) => {
                        let msg = tungstenite::Message::Binary(buf[..n].to_vec());
                        if ws.send(msg).await.is_err() {
                            break Ok(());
                        }
                    }
                }
            }
        }
    };

    let _ = ws.close(None).await;

    info!("Connection closed - WebSocket tunnel ended");

    // Release connection slot
    conn_tracker.release(&client_ip).await;

    result
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
fn select_auth_method(client_methods: &[u8], auth: &AuthConfig, is_trusted: bool) -> Option<u8> {
    // If auth is enabled and IP is trusted (recent connection), allow no-auth
    if auth.enabled && is_trusted {
        return Some(0x00);
    }
    
    // If auth is enabled and IP is not trusted, prefer user-pass (0x02)
    if auth.enabled {
        for &method in client_methods {
            if method == 0x02 {
                return Some(0x02);
            }
        }
    }
    
    // Check for no-auth (0x00) if offered
    for &method in client_methods {
        if method == 0x00 {
            return Some(0x00);
        }
    }
    
    None
}

pub async fn handle_socks5(
    mut stream: TcpStream,
    auth: &AuthConfig,
    trusted_ips: &TrustedIps,
    conn_tracker: &ConnectionTracker,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    stream.set_nodelay(true)?;
    
    // Extract client IP for trusted IP check and connection tracking
    let client_ip = stream.peer_addr().ok().map(|addr| addr.ip().to_string());
    let is_trusted = if let Some(ref ip) = client_ip {
        trusted_ips.is_trusted(ip).await
    } else {
        false
    };
    
    // --- auth negotiation ---
    let mut buf = [0u8; 258];
    let n = stream.read(&mut buf).await?;
    
    // Log inbound connection from client
    info!("Inbound connection from {}:{}", client_ip.as_deref().unwrap_or("unknown"), stream.peer_addr().ok().map(|a| a.port()).unwrap_or(0));
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
    // Auto-auth bypass: if trusted IP (recent connection) and auth enabled, allow 0x00
    let chosen_method = select_auth_method(offered_methods, auth, is_trusted);
    
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
                // Record trusted IP for future auto-auth bypass
                if let Some(ref ip) = client_ip {
                    trusted_ips.record_connection(ip).await;
                    info!("Trusted IP recorded: {}", ip);
                }
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

        // Log connection destination for traffic statistics
        info!("Connection to {}:{}", dest_addr, dest_port);

        // Check connection limit before proceeding
        if let Some(ref ip) = client_ip {
            if !conn_tracker.try_acquire(ip).await {
                error!("Connection limit exceeded for IP {}", ip);
                return Err("Too many connections from this IP".into());
            }
        }

        let ws_result = relay_via_ws(
            stream, 
            dc, 
            &init,
            Arc::new(conn_tracker.clone()), 
            client_ip.unwrap_or_else(|| "unknown".to_string())
        ).await;

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
    let trusted_ips = TrustedIps::new();
    let conn_tracker = ConnectionTracker::new();
    
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

    // Cleanup interval - remove expired trusted IPs every 5 minutes
    let mut cleanup_interval = tokio::time::interval(Duration::from_secs(300));

    loop {
        tokio::select! {
            result = listener.accept() => {
                if let Ok((stream, _)) = result {
                    let auth = auth.clone();
                    let trusted_ips = trusted_ips.clone();
                    let conn_tracker = conn_tracker.clone();
                    let _client_ip = stream.peer_addr().ok().map(|a| a.ip().to_string()).unwrap_or_default();
                    tokio::spawn(async move {
                        let _ = handle_socks5(stream, &auth, &trusted_ips, &conn_tracker).await;
                    });
                }
            }
            _ = cleanup_interval.tick() => {
                trusted_ips.cleanup_expired().await;
            }
        }
    }
}
