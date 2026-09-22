//! **The wire** — the one socket this workspace owns.
//!
//! The ship's computer reaches exactly one kind of counterparty: the United Cat Foods
//! exchange, over HTTP(S). This crate is the client half of that reach and nothing more —
//! a parsed [`Url`], an [`Error`] that can tell a broken wire from a server that answered
//! rubbish, and [`http`], the smallest blocking client that can carry a JSON request.
//!
//! It began life as an MCP client that reached partner servers over the Model Context
//! Protocol. What came across is the transport; what stayed behind is everything that made
//! it *MCP* — sessions, tool declarations, the offering and serving halves, the
//! boundary/guard check. The boundary here is the ship's signed lease
//! (`ucf_world::lease`), weighed by the pilot before it ever calls this crate, so there is
//! deliberately no gate inside the transport: a socket is a socket, and the decision to
//! open one is made where the doctrine lives.
//!
//! **On trust:** the exchange is a stranger holding a bearer token of ours. An MCP-era
//! posture is kept verbatim in [`tls`]: `https` VERIFIES the certificate chain and refuses
//! rather than degrading, and plain `http` is permitted only to loopback, so a test stub
//! can exist without opening a way to send a token in the clear.

pub mod http;
pub mod tls;

/// Everything that can go wrong reaching the exchange, kept apart so a caller can tell a
/// broken wire from a server that answered rubbish.
#[derive(Debug)]
pub enum Error {
    /// No verifying trust store, so a credential would travel unverified.
    NoTrustStore(String),
    /// The wire would have carried a credential in the clear.
    Insecure(String),
    /// Network, TLS, or framing failure.
    Io(String),
    /// The server answered, but not with what the protocol says it should.
    Protocol(String),
    /// The server answered with a JSON-RPC error.
    Server { code: i64, message: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoTrustStore(w) => write!(f, "{w}"),
            Error::Insecure(w) => write!(f, "{w}"),
            Error::Io(w) => write!(f, "unreachable: {w}"),
            Error::Protocol(w) => write!(f, "the server broke the protocol: {w}"),
            Error::Server { code, message } => write!(f, "server error {code}: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// Result alias for wire operations.
pub type Result<T> = std::result::Result<T, Error>;

/// A parsed absolute URL — enough of one for this crate, and no more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub https: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl Url {
    pub fn parse(raw: &str) -> Result<Self> {
        let (scheme, rest) = raw
            .split_once("://")
            .ok_or_else(|| Error::Protocol(format!("{raw} is not an absolute URL")))?;
        let https = match scheme {
            "https" => true,
            "http" => false,
            other => return Err(Error::Protocol(format!("unsupported scheme {other}"))),
        };
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        if authority.contains('@') {
            return Err(Error::Protocol(
                "credentials in the URL — put the token in the declaration's key file".into(),
            ));
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (
                h.to_string(),
                p.parse()
                    .map_err(|_| Error::Protocol(format!("bad port in {raw}")))?,
            ),
            None => (authority.to_string(), if https { 443 } else { 80 }),
        };
        if host.is_empty() {
            return Err(Error::Protocol(format!("{raw} has no host")));
        }
        Ok(Url {
            https,
            host,
            port,
            path: path.to_string(),
        })
    }

    pub fn origin(&self) -> String {
        format!(
            "{}://{}",
            if self.https { "https" } else { "http" },
            self.host_header()
        )
    }

    pub fn host_header(&self) -> String {
        let default = if self.https { 443 } else { 80 };
        if self.port == default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    pub fn is_loopback(&self) -> bool {
        self.host == "localhost"
            || self
                .host
                .parse::<std::net::IpAddr>()
                .map(|ip| ip.is_loopback())
                .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_parse_or_refuse_and_never_carry_credentials() {
        let u = Url::parse("https://srv1328560.hstgr.cloud/mcp").unwrap();
        assert!(u.https && u.port == 443 && u.path == "/mcp");
        assert_eq!(u.origin(), "https://srv1328560.hstgr.cloud");
        assert_eq!(u.host_header(), "srv1328560.hstgr.cloud");

        let p = Url::parse("http://127.0.0.1:8181/mcp").unwrap();
        assert!(!p.https && p.port == 8181 && p.is_loopback());
        assert_eq!(p.origin(), "http://127.0.0.1:8181");

        assert!(Url::parse("srv/mcp").is_err());
        assert!(Url::parse("ftp://srv/mcp").is_err());
        assert!(Url::parse("https:///mcp").is_err());
        // A token belongs in a 0600 key file, never in a URL that lands in logs.
        assert!(Url::parse("https://user:tok@srv/mcp").is_err());
    }
}
