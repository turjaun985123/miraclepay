//! SMTP email transport (Wave 35) — the first real
//! [`NotificationTransport`].
//!
//! Configuration is env-gated, in the same style as the gateway
//! adapters: without `SMTP_HOST` + `SMTP_FROM` the transport
//! reports [`GatewayConfigError::NotConfigured`]-style errors and
//! *nothing is sent* — queued notifications stay queued rather
//! than being fake-sent.
//!
//! Scope, stated plainly: this is a minimal SMTP client (RFC 5321)
//! over a **plain TCP** connection with optional `AUTH PLAIN`. It
//! suits local relays and trusted networks. It does **not** do
//! STARTTLS — point it at a local submission relay (or a tunnel)
//! rather than a public provider's plaintext port.
//!
//! Env:
//! - `SMTP_HOST` (required), `SMTP_PORT` (default 25)
//! - `SMTP_FROM` (required) — envelope + header sender
//! - `SMTP_USER` / `SMTP_PASS` (optional, AUTH PLAIN)
//! - `SMTP_HELO` (default `miraclepay.local`)

use std::future::Future;
use std::pin::Pin;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use miraclepay_domain::notification::NotificationChannel;

use crate::notification_job::NotificationTransport;

/// SMTP configuration.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub from: String,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub helo: String,
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

/// Read one (possibly multi-line) SMTP reply; returns its code.
async fn read_reply(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> Result<u16, String> {
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader
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
        // "250-..." continues; "250 ..." ends the reply.
        if line.as_bytes().get(3) != Some(&b'-') {
            return Ok(code);
        }
    }
}

async fn expect(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    wanted: u16,
    step: &str,
) -> Result<(), String> {
    let code = read_reply(reader).await?;
    if code == wanted {
        Ok(())
    } else {
        Err(format!("smtp {step}: expected {wanted}, got {code}"))
    }
}

impl SmtpTransport {
    /// Send one already-rendered message to `to` over a fresh
    /// connection, running the full SMTP dialogue.
    pub async fn deliver(&self, to: &str, message: &str) -> Result<(), String> {
        check_header(to, "recipient")?;
        let config = &self.config;
        let stream = TcpStream::connect((config.host.as_str(), config.port))
            .await
            .map_err(|e| format!("smtp connect {}:{}: {e}", config.host, config.port))?;
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        expect(&mut reader, 220, "greeting").await?;
        write_half
            .write_all(format!("EHLO {}\r\n", config.helo).as_bytes())
            .await
            .map_err(|e| format!("smtp write: {e}"))?;
        expect(&mut reader, 250, "EHLO").await?;

        if let (Some(user), Some(pass)) = (&config.user, &config.pass) {
            let token = base64_encode(format!("\0{user}\0{pass}").as_bytes());
            write_half
                .write_all(format!("AUTH PLAIN {token}\r\n").as_bytes())
                .await
                .map_err(|e| format!("smtp write: {e}"))?;
            expect(&mut reader, 235, "AUTH").await?;
        }

        write_half
            .write_all(format!("MAIL FROM:<{}>\r\n", config.from).as_bytes())
            .await
            .map_err(|e| format!("smtp write: {e}"))?;
        expect(&mut reader, 250, "MAIL FROM").await?;
        write_half
            .write_all(format!("RCPT TO:<{to}>\r\n").as_bytes())
            .await
            .map_err(|e| format!("smtp write: {e}"))?;
        expect(&mut reader, 250, "RCPT TO").await?;
        write_half
            .write_all(b"DATA\r\n")
            .await
            .map_err(|e| format!("smtp write: {e}"))?;
        expect(&mut reader, 354, "DATA").await?;
        write_half
            .write_all(dot_stuff(message).as_bytes())
            .await
            .map_err(|e| format!("smtp write: {e}"))?;
        expect(&mut reader, 250, "message").await?;
        let _ = write_half.write_all(b"QUIT\r\n").await;
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

    #[test]
    fn missing_config_is_a_clear_error() {
        let err = SmtpConfig::from_lookup(lookup(&[])).unwrap_err();
        assert!(err.contains("SMTP_HOST"));
        let err = SmtpConfig::from_lookup(lookup(&[("SMTP_HOST", "mail.local")])).unwrap_err();
        assert!(err.contains("SMTP_FROM"));
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

        let config = SmtpConfig {
            host: "127.0.0.1".to_string(),
            port,
            from: "pay@example.com".to_string(),
            user: None,
            pass: None,
            helo: "miraclepay.test".to_string(),
        };
        let transport = SmtpTransport { config };
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
    async fn non_email_channels_are_refused() {
        let transport = SmtpTransport {
            config: SmtpConfig {
                host: "127.0.0.1".to_string(),
                port: 1,
                from: "pay@example.com".to_string(),
                user: None,
                pass: None,
                helo: "x".to_string(),
            },
        };
        let err = transport
            .send(NotificationChannel::Sms, "01712345678", None, "hi")
            .await
            .unwrap_err();
        assert!(err.contains("cannot deliver"));
    }
}
