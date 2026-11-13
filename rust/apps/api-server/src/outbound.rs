//! Carrying a webhook request to its endpoint.
//!
//! This build has no TLS client, so an `https` endpoint cannot be reached from here. Rather than pretend a delivery
//! happened, such an endpoint is reported as unreachable, with the reason kept on the delivery record. An `http`
//! endpoint is posted to directly. Swapping in a TLS client later means replacing this one type and nothing else: the
//! service asks for a [`Sender`] and does not care who fills it.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use webhooks::webhooks::Sender;

/// A sender that speaks plain HTTP and says so plainly when it cannot reach anyone.
pub struct PlainHttp;

impl Sender for PlainHttp {
    fn post(&self, url: &str, body: &str, headers: &[(String, String)]) -> Result<u16, String> {
        let rest = match url.strip_prefix("http://") {
            Some(rest) => rest,
            None if url.starts_with("https://") => {
                return Err(
                    "no TLS client in this build, so an https endpoint cannot be reached"
                        .to_string(),
                )
            }
            None => return Err(format!("not a web address: {url}")),
        };
        let (authority, path) = match rest.split_once('/') {
            Some((authority, path)) => (authority, format!("/{path}")),
            None => (rest, "/".to_string()),
        };
        let address = if authority.contains(':') {
            authority.to_string()
        } else {
            format!("{authority}:80")
        };
        let mut stream = TcpStream::connect(address.as_str()).map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|error| error.to_string())?;
        let mut request = format!(
            "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);
        stream
            .write_all(request.as_bytes())
            .map_err(|error| error.to_string())?;
        let mut answer = String::new();
        stream
            .read_to_string(&mut answer)
            .map_err(|error| error.to_string())?;
        let code = answer
            .split_whitespace()
            .nth(1)
            .and_then(|digits| digits.parse().ok())
            .ok_or_else(|| "the endpoint answered nothing readable".to_string())?;
        Ok(code)
    }
}
