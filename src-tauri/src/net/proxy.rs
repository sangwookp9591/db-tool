use super::model::*;
use base64::Engine;
use dbcore::DbError;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
pub type BoxStream = Box<dyn Stream>;

pub async fn connect(
    proxy: Option<&Proxy>,
    password: Option<&str>,
    host: &str,
    port: u16,
) -> Result<BoxStream, DbError> {
    valid_host(host)?;
    let (target, target_port) = proxy
        .map(|p| (p.host.as_str(), p.port))
        .unwrap_or((host, port));
    let mut stream = TcpStream::connect((target, target_port))
        .await
        .map_err(|_| error("TCP_CONNECT_FAILED"))?;
    stream
        .set_nodelay(true)
        .map_err(|_| error("TCP_CONNECT_FAILED"))?;
    if let Some(proxy) = proxy {
        match proxy.kind {
            ProxyKind::Socks5 => {
                socks(&mut stream, proxy.user.as_deref(), password, host, port).await?
            }
            ProxyKind::HttpConnect => {
                http(&mut stream, proxy.user.as_deref(), password, host, port).await?
            }
        }
    }
    Ok(Box::new(stream))
}
async fn socks(
    stream: &mut TcpStream,
    user: Option<&str>,
    password: Option<&str>,
    host: &str,
    port: u16,
) -> Result<(), DbError> {
    let method = if user.is_some() { 2 } else { 0 };
    stream
        .write_all(&[5, 1, method])
        .await
        .map_err(|_| error("PROXY_IO"))?;
    let mut reply = [0; 2];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|_| error("PROXY_IO"))?;
    if reply != [5, method] {
        return Err(error("PROXY_AUTH_REJECTED"));
    }
    if let Some(user) = user {
        let password = password.ok_or_else(|| error("PROXY_PASSWORD_REQUIRED"))?;
        if user.is_empty() || user.len() > 255 || password.len() > 255 {
            return Err(error("INVALID_PROXY_CREDENTIAL"));
        }
        let mut auth = vec![1, user.len() as u8];
        auth.extend(user.as_bytes());
        auth.push(password.len() as u8);
        auth.extend(password.as_bytes());
        stream
            .write_all(&auth)
            .await
            .map_err(|_| error("PROXY_IO"))?;
        stream
            .read_exact(&mut reply)
            .await
            .map_err(|_| error("PROXY_IO"))?;
        if reply != [1, 0] {
            return Err(error("PROXY_AUTH_REJECTED"));
        }
    }
    let mut request = vec![5, 1, 0];
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            request.push(1);
            request.extend(ip.octets());
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            request.push(4);
            request.extend(ip.octets());
        }
        Err(_) => {
            request.extend([3, host.len() as u8]);
            request.extend(host.as_bytes());
        }
    }
    request.extend(port.to_be_bytes());
    stream
        .write_all(&request)
        .await
        .map_err(|_| error("PROXY_IO"))?;
    let mut header = [0; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|_| error("PROXY_IO"))?;
    if header[0] != 5 || header[1] != 0 || header[2] != 0 {
        return Err(error("PROXY_CONNECT_REJECTED"));
    }
    let len = match header[3] {
        1 => 4,
        4 => 16,
        3 => stream.read_u8().await.map_err(|_| error("PROXY_IO"))? as usize,
        _ => return Err(error("PROXY_INVALID_RESPONSE")),
    };
    let mut rest = vec![0; len + 2];
    stream
        .read_exact(&mut rest)
        .await
        .map_err(|_| error("PROXY_IO"))?;
    Ok(())
}
async fn http(
    stream: &mut TcpStream,
    user: Option<&str>,
    password: Option<&str>,
    host: &str,
    port: u16,
) -> Result<(), DbError> {
    let authority = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut request = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n");
    if let Some(user) = user {
        if user.contains(':') {
            return Err(error("INVALID_PROXY_CREDENTIAL"));
        }
        let password = password.ok_or_else(|| error("PROXY_PASSWORD_REQUIRED"))?;
        let basic = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
        request.push_str(&format!("Proxy-Authorization: Basic {basic}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|_| error("PROXY_IO"))?;
    // Read exactly the header so coalesced database bytes remain on the socket.
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 16384 {
            return Err(error("PROXY_HEADER_TOO_LARGE"));
        }
        header.push(stream.read_u8().await.map_err(|_| error("PROXY_IO"))?);
    }
    let line = std::str::from_utf8(&header)
        .map_err(|_| error("PROXY_INVALID_RESPONSE"))?
        .lines()
        .next()
        .ok_or_else(|| error("PROXY_INVALID_RESPONSE"))?;
    let mut words = line.split_whitespace();
    let version = words.next();
    let code = words.next();
    if !matches!(version, Some("HTTP/1.0" | "HTTP/1.1")) || code != Some("200") {
        return Err(error(if code == Some("407") {
            "PROXY_AUTH_REJECTED"
        } else {
            "PROXY_CONNECT_REJECTED"
        }));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn http_leaves_coalesced_payload_and_handles_rejection() {
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut s, _) = server.accept().await.unwrap();
            let mut h = Vec::new();
            while !h.ends_with(b"\r\n\r\n") {
                h.push(s.read_u8().await.unwrap());
            }
            assert!(String::from_utf8_lossy(&h).starts_with("CONNECT db.test:5432"));
            s.write_all(b"HTTP/1.1 200 OK\r\n\r\nDB").await.unwrap();
        });
        let p = Proxy {
            kind: ProxyKind::HttpConnect,
            host: addr.ip().to_string(),
            port: addr.port(),
            user: None,
        };
        let mut s = connect(Some(&p), None, "db.test", 5432).await.unwrap();
        let mut b = [0; 2];
        s.read_exact(&mut b).await.unwrap();
        assert_eq!(&b, b"DB");
        task.await.unwrap();
    }
    #[tokio::test]
    async fn socks_rejects_auth_downgrade() {
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a = server.local_addr().unwrap();
        let t = tokio::spawn(async move {
            let (mut s, _) = server.accept().await.unwrap();
            let mut b = [0; 3];
            s.read_exact(&mut b).await.unwrap();
            assert_eq!(b, [5, 1, 2]);
            s.write_all(&[5, 0]).await.unwrap();
        });
        let p = Proxy {
            kind: ProxyKind::Socks5,
            host: a.ip().to_string(),
            port: a.port(),
            user: Some("u".into()),
        };
        assert_eq!(
            connect(Some(&p), Some("p"), "db.test", 1)
                .await
                .err()
                .unwrap()
                .message,
            "PROXY_AUTH_REJECTED"
        );
        t.await.unwrap();
    }
}
