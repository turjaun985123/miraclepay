//! SMTP transport for notifications (Wave 35; STARTTLS Wave 86).
//!
//! Implements the worker's [`NotificationTransport`] trait over a
//! real SMTP dialogue (EHLO → optional STARTTLS → optional AUTH
//! PLAIN → MAIL/RCPT/DATA), env-gated: without `SMTP_HOST` /
//! `SMTP_FROM` the config constructor fails with a named error and
//! nothing is ever "sent" into the void.
//!
//! TLS modes (`SMTP_TLS`):
//! - `auto` (default): upgrade with STARTTLS when the server
//!   advertises it; otherwise continue in plain text (local relays).
//! - `starttls`: STARTTLS is required — a server that does not
//!   advertise it is an error, so credentials never cross in clear.
//! - `implicit`: TLS from the first byte (port 465 style).
//! - `none`: never use TLS (loopback relays only).
//!
//! AUTH PLAIN is only attempted after TLS is established when the
//! mode is `starttls`/`implicit`; in `auto` mode a configured
//! credential on a non-advertising server is refused rather than
//! sent in clear text. Certificate verification uses the webpki
//! root store; the TLS server name is `SMTP_HOST`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use miraclepay_domain::notification::NotificationChannel;

use crate::notification_job::NotificationTransport;

/// TLS behaviour for the SMTP connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    /// Never use TLS.
    None,
    /// STARTTLS when advertised (default).
    Auto,
    /// STARTTLS required; refuse otherwise.
    Required,
    /// TLS from the first byte (port 465 style).
    Implicit,
}

impl TlsMode {
    fn parse(value: Option<String>) -> Result<Self, String> {
        match value.as_deref().map(str::trim) {
            None | Some("") | Some("auto") => Ok(Self::Auto),
            Some("none") => Ok(Self::None),
            Some("starttls") | Some("required") => Ok(Self::Required),
            Some("implicit") => Ok(Self::Implicit),
            Some(other) => Err(format!(
                "SMTP_TLS: unknown mode {other:?} (expected auto|starttls|implicit|none)"
            )),
        }
    }
}

/// SMTP configuration.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub from: String,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub helo: String,
    pub tls: TlsMode,
}

impl SmtpConfig {
    /// Build from an explicit variable lookup (env in production,
    /// fixtures in tests). Missing host/from → `Err` naming what's
    /// missing (callers surface it as "not configured").
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let host = match get("SMTP_HOST") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return Err("SMTP not configured: missing SMTP_HOST".to_string()),
        };
        let from = match get("SMTP_FROM") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return Err("SMTP not configured: missing SMTP_FROM".to_string()),
        };
        let port = get("SMTP_PORT")
            .and_then(|v| v.trim().parse::<u16>().ok())
            .unwrap_or(25);
        Ok(Self {
            host,
            port,
            from,
            user: get("SMTP_USER").filter(|v| !v.trim().is_empty()),
            pass: get("SMTP_PASS").filter(|v| !v.is_empty()),
            helo: get("SMTP_HELO")
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "miraclepay.local".to_string()),
            tls: TlsMode::parse(get("SMTP_TLS"))?,
        })
    }

    /// Build from the process environment.
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }
}

/// Reject CR/LF in header fields (header-injection guard).
fn check_header(value: &str, field: &str) -> Result<(), String> {
    if value.contains('\r') || value.contains('\n') {
        return Err(format!("invalid {field}: contains a line break"));
    }
    Ok(())
}

/// Render a plain-text RFC 5322 message.
pub fn render_message(
    config: &SmtpConfig,
    to: &str,
    subject: &str,
    body: &str,
    date_rfc2822: &str,
) -> Result<String, String> {
    check_header(&config.from, "from")?;
    check_header(to, "recipient")?;
    check_header(subject, "subject")?;
    Ok(format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nDate: {date}\r\n\
         MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}",
        from = config.from,
        to = to,
        subject = subject,
        date = date_rfc2822,
        body = body.replace('\n', "\r\n"),
    ))
}

/// Dot-stuff a message for the DATA phase (lines starting with `.`
/// get a second dot) and terminate with `<CRLF>.<CRLF>`.
fn dot_stuff(message: &str) -> String {
    let mut out = String::with_capacity(message.len() + 8);
    for line in message.split("\r\n") {
        if line.starts_with('.') {
            out.push('.');
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str(".\r\n");
    out
}

/// Minimal base64 (standard alphabet, padded) for AUTH PLAIN.
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// The SMTP transport. Holds only config; connects per send.
#[derive(Debug, Clone)]
pub struct SmtpTransport {
    pub config: SmtpConfig,
}

/// A line-buffered SMTP connection over any async stream (plain
/// TCP or TLS), so the dialogue code is written once.
struct Conn<S> {
    reader: BufReader<S>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Conn<S> {
    fn new(stream: S) -> Self {
        Self {
            reader: BufReader::new(stream),
        }
    }

    fn stream_mut(&mut self) -> &mut S {
        self.reader.get_mut()
    }

    async fn write_line(&mut self, line: &str) -> Result<(), String> {
        self.stream_mut()
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("smtp write: {e}"))?;
        self.stream_mut()
            .flush()
            .await
            .map_err(|e| format!("smtp flush: {e}"))
    }

    /// Read one (possibly multi-line) reply; returns code + text.
    async fn read_reply(&mut self) -> Result<(u16, String), String> {
        use tokio::io::AsyncBufReadExt;
        let mut text = String::new();
        loop {
            let mut line = String::new();
            let read = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| format!("smtp read: {e}"))?;
            if read == 0 {
                return Err("smtp: connection closed".to_string());
            }
            let code: u16 = line
                .get(0..3)
                .and_then(|prefix| prefix.parse().ok())
                .ok_or_else(|| format!("smtp: malformed reply {line:?}"))?;
            text.push_str(&line);
            if line.as_bytes().get(3) != Some(&b'-') {
                return Ok((code, text));
            }
        }
    }

    async fn expect(&mut self, wanted: u16, step: &str) -> Result<String, String> {
        let (code, text) = self.read_reply().await?;
        if code == wanted {
            Ok(text)
        } else {
            Err(format!("smtp {step}: expected {wanted}, got {code}"))
        }
    }

    async fn ehlo(&mut self, helo: &str) -> Result<String, String> {
        self.write_line(&format!("EHLO {helo}\r\n")).await?;
        self.expect(250, "EHLO").await
    }
}

/// Build a rustls client config with the webpki root store.
fn tls_connector() -> Result<tokio_rustls::TlsConnector, String> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

/// Wrap a TCP stream in TLS for `host`.
async fn wrap_tls(
    host: &str,
    stream: TcpStream,
) -> Result<tokio_rustls::client::TlsStream<TcpStream>, String> {
    let connector = tls_connector()?;
    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|_| format!("smtp tls: invalid server name {host:?}"))?;
    connector
        .connect(server_name, stream)
        .await
        .map_err(|e| format!("smtp tls handshake with {host}: {e}"))
}

impl SmtpTransport {
    /// Send one already-rendered message to `to` over a fresh
    /// connection, running the full SMTP dialogue.
    pub async fn deliver(&self, to: &str, message: &str) -> Result<(), String> {
        check_header(to, "recipient")?;
        let config = &self.config;
        let tcp = TcpStream::connect((config.host.as_str(), config.port))
            .await
            .map_err(|e| format!("smtp connect {}:{}: {e}", config.host, config.port))?;
        if config.tls == TlsMode::Implicit {
            let tls = wrap_tls(&config.host, tcp).await?;
            let mut conn = Conn::new(tls);
            return self.dialogue(&mut conn, to, message, true).await;
        }
        let mut conn = Conn::new(tcp);
        conn.expect(220, "greeting").await?;
        let ehlo_reply = conn.ehlo(&config.helo).await?;
        let advertises_starttls = ehlo_reply.to_ascii_uppercase().contains("STARTTLS");
        let wants_tls = match config.tls {
            TlsMode::Required => {
                if !advertises_starttls {
                    return Err(
                        "smtp: SMTP_TLS=starttls but the server does not advertise STARTTLS"
                            .to_string(),
                    );
                }
                true
            }
            TlsMode::Auto => advertises_starttls,
            _ => false,
        };
        if wants_tls {
            conn.write_line("STARTTLS\r\n").await?;
            conn.expect(220, "STARTTLS").await?;
            // Recover the raw TCP stream from the buffered reader
            // (the EHLO reply was fully consumed, so nothing is
            // buffered) and upgrade it.
            let raw = conn.reader.into_inner();
            let tls = wrap_tls(&config.host, raw).await?;
            let mut conn = Conn::new(tls);
            conn.ehlo(&config.helo).await?;
            return self.dialogue(&mut conn, to, message, true).await;
        }
        // Plain text: refuse to send credentials in clear when TLS
        // was available-or-required but not established. `auto`
        // without an advertisement means a local relay — allowed
        // only without credentials.
        let has_credentials = config.user.is_some() && config.pass.is_some();
        if has_credentials && config.tls != TlsMode::None {
            return Err(
                "smtp: refusing to send credentials without TLS (server did not advertise STARTTLS)"
                    .to_string(),
            );
        }
        self.dialogue(&mut conn, to, message, false).await
    }

    /// The post-greeting dialogue (AUTH → MAIL → RCPT → DATA).
    async fn dialogue<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        conn: &mut Conn<S>,
        to: &str,
        message: &str,
        _tls_active: bool,
    ) -> Result<(), String> {
        let config = &self.config;
        if let (Some(user), Some(pass)) = (&config.user, &config.pass) {
            let token = base64_encode(format!("\0{user}\0{pass}").as_bytes());
            conn.write_line(&format!("AUTH PLAIN {token}\r\n")).await?;
            conn.expect(235, "AUTH").await?;
        }
        conn.write_line(&format!("MAIL FROM:<{}>\r\n", config.from))
            .await?;
        conn.expect(250, "MAIL FROM").await?;
        conn.write_line(&format!("RCPT TO:<{to}>\r\n")).await?;
        conn.expect(250, "RCPT TO").await?;
        conn.write_line("DATA\r\n").await?;
        conn.expect(354, "DATA").await?;
        conn.write_line(&dot_stuff(message)).await?;
        conn.expect(250, "message").await?;
        let _ = conn.write_line("QUIT\r\n").await;
        Ok(())
    }
}

impl NotificationTransport for SmtpTransport {
    fn send<'a>(
        &'a self,
        channel: NotificationChannel,
        recipient: &'a str,
        subject: Option<&'a str>,
        body: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            if channel != NotificationChannel::Email {
                return Err(format!("SMTP transport cannot deliver channel {channel:?}"));
            }
            let date = chrono::Utc::now()
                .format("%a, %d %b %Y %H:%M:%S +0000")
                .to_string();
            let message = render_message(
                &self.config,
                recipient,
                subject.unwrap_or("MiraclePay notification"),
                body,
                &date,
            )?;
            self.deliver(recipient, &message).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    fn plain_config(port: u16) -> SmtpConfig {
        SmtpConfig {
            host: "127.0.0.1".to_string(),
            port,
            from: "pay@example.com".to_string(),
            user: None,
            pass: None,
            helo: "miraclepay.test".to_string(),
            tls: TlsMode::None,
        }
    }

    #[test]
    fn missing_config_is_a_clear_error() {
        let err = SmtpConfig::from_lookup(lookup(&[])).unwrap_err();
        assert!(err.contains("SMTP_HOST"));
        let err = SmtpConfig::from_lookup(lookup(&[("SMTP_HOST", "mail.local")])).unwrap_err();
        assert!(err.contains("SMTP_FROM"));
    }

    #[test]
    fn tls_mode_parsing() {
        let config = SmtpConfig::from_lookup(lookup(&[
            ("SMTP_HOST", "mail.local"),
            ("SMTP_FROM", "pay@example.com"),
            ("SMTP_TLS", "starttls"),
        ]))
        .unwrap();
        assert_eq!(config.tls, TlsMode::Required);
        let err = SmtpConfig::from_lookup(lookup(&[
            ("SMTP_HOST", "mail.local"),
            ("SMTP_FROM", "pay@example.com"),
            ("SMTP_TLS", "bogus"),
        ]))
        .unwrap_err();
        assert!(err.contains("SMTP_TLS"));
    }

    #[test]
    fn render_rejects_header_injection() {
        let config = SmtpConfig::from_lookup(lookup(&[
            ("SMTP_HOST", "mail.local"),
            ("SMTP_FROM", "pay@example.com"),
        ]))
        .unwrap();
        let err = render_message(
            &config,
            "victim@example.com\r\nBcc: all@example.com",
            "Hi",
            "body",
            "date",
        )
        .unwrap_err();
        assert!(err.contains("recipient"));
    }

    #[test]
    fn dot_stuffing_and_base64() {
        assert_eq!(dot_stuff("a\r\n.b\r\nc"), "a\r\n..b\r\nc\r\n.\r\n");
        assert_eq!(base64_encode(b"\0user\0pass"), "AHVzZXIAcGFzcw==");
    }

    /// A fake SMTP server that plays the dialogue and captures DATA.
    async fn fake_server(
        listener: TcpListener,
        captured: std::sync::Arc<std::sync::Mutex<String>>,
    ) {
        use tokio::io::AsyncBufReadExt;
        let (stream, _) = listener.accept().await.unwrap();
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        write_half.write_all(b"220 fake ready\r\n").await.unwrap();
        let mut line = String::new();
        let mut in_data = false;
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await.unwrap();
            if n == 0 {
                break;
            }
            if in_data {
                if line == ".\r\n" {
                    in_data = false;
                    write_half.write_all(b"250 queued\r\n").await.unwrap();
                } else {
                    captured.lock().unwrap().push_str(&line);
                }
                continue;
            }
            let upper = line.to_ascii_uppercase();
            if upper.starts_with("EHLO") {
                write_half.write_all(b"250 hello\r\n").await.unwrap();
            } else if upper.starts_with("MAIL") || upper.starts_with("RCPT") {
                write_half.write_all(b"250 ok\r\n").await.unwrap();
            } else if upper.starts_with("DATA") {
                in_data = true;
                write_half.write_all(b"354 go ahead\r\n").await.unwrap();
            } else if upper.starts_with("QUIT") {
                break;
            }
        }
    }

    #[tokio::test]
    async fn delivers_through_a_real_smtp_dialogue() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let server = tokio::spawn(fake_server(listener, captured.clone()));

        let transport = SmtpTransport {
            config: plain_config(port),
        };
        transport
            .send(
                NotificationChannel::Email,
                "customer@example.com",
                Some("Invoice INV-1"),
                "You owe BDT 500.00.",
            )
            .await
            .unwrap();
        server.await.unwrap();
        let data = captured.lock().unwrap().clone();
        assert!(data.contains("Subject: Invoice INV-1"), "captured: {data}");
        assert!(data.contains("You owe BDT 500.00."), "captured: {data}");
        assert!(
            data.contains("To: customer@example.com"),
            "captured: {data}"
        );
    }

    #[tokio::test]
    async fn starttls_required_refuses_a_plain_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let server = tokio::spawn(fake_server(listener, captured));
        let mut config = plain_config(port);
        config.tls = TlsMode::Required;
        let transport = SmtpTransport { config };
        let err = transport
            .send(NotificationChannel::Email, "c@example.com", None, "hi")
            .await
            .unwrap_err();
        assert!(err.contains("STARTTLS"), "error: {err}");
        server.abort();
    }

    #[tokio::test]
    async fn non_email_channels_are_refused() {
        let transport = SmtpTransport {
            config: plain_config(1),
        };
        let err = transport
            .send(NotificationChannel::Sms, "01712345678", None, "hi")
            .await
            .unwrap_err();
        assert!(err.contains("cannot deliver"));
    }
}
