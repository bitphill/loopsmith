//! Who is allowed to talk to this server.
//!
//! The server binds loopback, which stops the network reaching it but does not
//! stop a *browser* reaching it. That gap is DNS rebinding, and it is the one
//! attack a localhost tool with no authentication is genuinely exposed to:
//!
//!   1. The user visits `evil.com`, which resolves normally.
//!   2. Its DNS record has a one-second TTL and re-resolves to `127.0.0.1`.
//!   3. The page fetches `/api/...`. The browser still believes the origin is
//!      `evil.com`, so this is a *same-origin* request and no CORS check ever
//!      runs. The request arrives here, from loopback, looking ordinary.
//!
//! What that would have bought an attacker, before this module existed:
//! `POST /api/jobs` to run loopsmith subcommands as the user, `POST
//! /api/secrets/reveal` to read back every API key in the shell profile, and
//! `POST /api/secrets` to write a variable into that profile — which is code
//! execution at the next login.
//!
//! The defence is the standard one, and it is cheap: the attacker controls the
//! DNS name but cannot change the `Host` header the browser sends, because the
//! browser sets it from the URL. A request that arrived by rebinding therefore
//! carries `Host: evil.com`, and a genuine one carries `Host: 127.0.0.1:3000`.
//! Checking it separates the two exactly.
//!
//! `Origin` is checked as well, for anything that is not a plain read. A
//! browser attaches it to every cross-origin request, so a present-but-foreign
//! Origin is a request no legitimate page here would make.

use axum::extract::Request;
use axum::http::{header, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Is this host name one of the ways to say "this machine"?
///
/// Names only. An attacker's DNS can point *any* name at 127.0.0.1, so a name
/// is trustworthy here precisely when the browser could not have been talked
/// into sending it for somebody else's domain.
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    if host.is_empty() {
        return false;
    }

    // Strip the port. IPv6 literals are bracketed, so find the closing bracket
    // first — splitting on ':' would cut `[::1]:3000` in the wrong place.
    let name = if let Some(end) = host.find(']') {
        if !host.starts_with('[') {
            return false;
        }
        &host[1..end]
    } else {
        host.split(':').next().unwrap_or("")
    };

    if name.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match name.parse::<std::net::IpAddr>() {
        // The whole 127.0.0.0/8 block, not just 127.0.0.1: `127.0.0.2` is
        // equally loopback and equally ours.
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// Reject anything that did not come from a page served by this server.
pub async fn guard(req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_string();

    if !is_loopback_host(&host) {
        return refuse(format!(
            "refused: this server answers only to localhost, and this request \
             arrived with Host `{host}`. That is the signature of DNS rebinding — \
             a page on another site pointing its own domain at 127.0.0.1 to reach \
             a tool running on your machine. Open the URL loopsmith printed."
        ));
    }

    // A same-origin fetch from our own page sends no Origin, or sends ours.
    // Anything else is a cross-site request, and there is no reason for one.
    // Reads are left alone: they carry no side effects, and a browser attaches
    // Origin to some perfectly ordinary navigations.
    let writes = !matches!(req.method(), &Method::GET | &Method::HEAD | &Method::OPTIONS);
    if writes {
        if let Some(origin) = req.headers().get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
            let origin_host = origin
                .split("://")
                .nth(1)
                .unwrap_or(origin);
            if !is_loopback_host(origin_host) {
                return refuse(format!(
                    "refused: a cross-site request from `{origin}` tried to change \
                     something here. Only the page loopsmith serves may do that."
                ));
            }
        }
    }

    next.run(req).await
}

fn refuse(message: String) -> Response {
    (
        StatusCode::FORBIDDEN,
        axum::Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ways_of_saying_this_machine_are_accepted() {
        for host in [
            "localhost", "localhost:3000", "LOCALHOST:3000",
            "127.0.0.1", "127.0.0.1:3000", "127.0.0.2:3117",
            "[::1]", "[::1]:3000",
        ] {
            assert!(is_loopback_host(host), "`{host}` must be accepted");
        }
    }

    #[test]
    fn a_rebound_name_is_refused_however_it_resolves() {
        // Every one of these can be pointed at 127.0.0.1 by whoever owns the
        // domain. The name is the whole signal, which is why it is checked.
        for host in [
            "evil.com", "evil.com:3000", "localhost.evil.com",
            "127.0.0.1.evil.com", "notlocalhost", "0.0.0.0", "192.168.1.5",
            "10.0.0.1:3000", "", "   ",
        ] {
            assert!(!is_loopback_host(host), "`{host}` must be refused");
        }
    }

    #[test]
    fn a_bracketless_ipv6_is_refused_rather_than_mis_parsed() {
        // `::1:3000` is ambiguous: splitting on ':' yields an empty name. It
        // must fail closed rather than be read as loopback.
        assert!(!is_loopback_host("::1:3000"));
        assert!(!is_loopback_host("::1]"));
        assert!(!is_loopback_host("[::1"));
    }

    #[test]
    fn a_public_address_that_merely_starts_with_127_is_refused() {
        // String-prefix matching would accept these. Parsing does not.
        assert!(!is_loopback_host("127.0.0.1.example.com"));
        assert!(!is_loopback_host("1270.0.0.1"));
    }
}
