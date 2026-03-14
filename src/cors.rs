/// Returns true if `origin` is allowed by any of the `patterns`.
///
/// Patterns may be:
/// - An exact origin: `"https://example.com"`
/// - A scheme + wildcard subdomain: `"https://*.example.com"` (also matches the apex domain)
/// - A scheme may be omitted, in which case any scheme matches.
pub fn is_origin_allowed(origin: &str, patterns: &[String]) -> bool {
    let origin = origin.trim();
    if origin.is_empty() || patterns.is_empty() {
        return false;
    }

    let (origin_scheme, origin_host) = match origin.split_once("://") {
        Some((s, rest)) => (s, strip_path_and_port(rest)),
        None => return false,
    };

    for pattern in patterns {
        let pat = pattern.trim();
        if pat.is_empty() {
            continue;
        }

        let (pat_scheme, pat_host) = if let Some((s, rest)) = pat.split_once("://") {
            (Some(s), rest.trim_end_matches('/'))
        } else {
            (None, pat.trim_end_matches('/'))
        };

        // Scheme must match if specified in the pattern.
        if let Some(ps) = pat_scheme {
            if !ps.eq_ignore_ascii_case(origin_scheme) {
                continue;
            }
        }

        // Host matching: *.example.com matches example.com and sub.example.com.
        if let Some(suffix) = pat_host.strip_prefix("*.") {
            let suffix_lower = suffix.to_ascii_lowercase();
            let origin_lower = origin_host.to_ascii_lowercase();
            if origin_lower == suffix_lower || origin_lower.ends_with(&format!(".{suffix_lower}")) {
                return true;
            }
        } else if origin_host.eq_ignore_ascii_case(pat_host) {
            return true;
        }
    }

    false
}

/// Returns true if the origin's host is a loopback address.
pub fn is_loopback(origin: &str) -> bool {
    let host = origin
        .split_once("://")
        .map(|(_, rest)| strip_path_and_port(rest))
        .unwrap_or(origin);
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

fn strip_path_and_port(host_and_rest: &str) -> &str {
    // Remove any trailing path.
    let h = host_and_rest.split('/').next().unwrap_or(host_and_rest);
    // Remove port, but only for non-IPv6 addresses.
    if h.starts_with('[') {
        // IPv6 literal — leave as-is (port would be after ']').
        h
    } else {
        h.rsplit_once(':').map(|(h, _)| h).unwrap_or(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(s: &str) -> Vec<String> {
        s.split(',').map(|p| p.trim().to_string()).collect()
    }

    #[test]
    fn exact_match() {
        let ps = patterns("https://example.com");
        assert!(is_origin_allowed("https://example.com", &ps));
        assert!(!is_origin_allowed("https://other.com", &ps));
    }

    #[test]
    fn wildcard_subdomain() {
        let ps = patterns("https://*.krnl64.win");
        assert!(is_origin_allowed("https://app.krnl64.win", &ps));
        assert!(is_origin_allowed("https://krnl64.win", &ps)); // apex also allowed
        assert!(!is_origin_allowed("https://other.win", &ps));
    }

    #[test]
    fn scheme_mismatch() {
        let ps = patterns("https://example.com");
        assert!(!is_origin_allowed("http://example.com", &ps));
    }

    #[test]
    fn loopback_detection() {
        assert!(is_loopback("http://localhost:3000"));
        assert!(is_loopback("http://127.0.0.1:5173"));
        assert!(!is_loopback("https://example.com"));
    }
}
