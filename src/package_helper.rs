use reqwest::{Client, Method};

/// Convert a byte count to a human-readable string (B, KB, MB, …).
///
/// Mirrors the C# `BytesToString` helper in PackageHelper.cs.
pub fn bytes_to_string(byte_count: i64) -> String {
    const SUFFIXES: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB", "EB"];
    if byte_count == 0 {
        return format!("0{}", SUFFIXES[0]);
    }
    let bytes = byte_count.unsigned_abs() as f64;
    let place = (bytes.ln() / 1024_f64.ln()).floor() as usize;
    let place = place.min(SUFFIXES.len() - 1);
    let num = (bytes / 1024_f64.powi(place as i32) * 10.0).round() / 10.0;
    let signed = if byte_count < 0 { -num } else { num };
    format!("{signed}{}", SUFFIXES[place])
}

/// Fetch the file size of `url` via a HEAD request, returning a human-readable
/// string, or `"Unknown"` on any failure.
pub async fn get_file_size(client: &Client, url: &str) -> String {
    if url.is_empty() {
        return "Unknown".into();
    }
    match client.head(url).send().await {
        Ok(r) if r.status().is_success() => r
            .content_length()
            .map(|l| bytes_to_string(l as i64))
            .unwrap_or_else(|| "Unknown".into()),
        _ => "Unknown".into(),
    }
}

/// Attempt to determine the filename for `url`.
///
/// Tries HEAD first (with the Delivery-Optimization user-agent), falls back to
/// GET.  Extracts the name from `Content-Disposition` (RFC 5987 `filename*`
/// first, then plain `filename`), then falls back to the URL path.  Returns an
/// empty string on failure.
pub async fn get_file_name(client: &Client, url: &str) -> String {
    if url.trim().is_empty() {
        return String::new();
    }

    let send = |method: Method| {
        client
            .request(method, url)
            .header("User-Agent", "Microsoft-Delivery-Optimization/10.0")
            .send()
    };

    let resp = match send(Method::HEAD).await {
        Ok(r) if r.status().is_success() => r,
        _ => match send(Method::GET).await {
            Ok(r) if r.status().is_success() => r,
            _ => return String::new(),
        },
    };

    extract_file_name(resp.headers(), url)
}

fn extract_file_name(headers: &reqwest::header::HeaderMap, url: &str) -> String {
    if let Some(cd) = headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(name) = extract_filename_star(cd).or_else(|| extract_filename_plain(cd)) {
            if !name.is_empty() {
                return name;
            }
        }
    }

    // Fallback: last path segment of the URL (before any query string).
    url.split('?')
        .next()
        .and_then(|p| p.rsplit('/').next())
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Parse `filename*=UTF-8''encoded-name` (RFC 5987).
fn extract_filename_star(cd: &str) -> Option<String> {
    for part in cd.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("filename*=") {
            let val = val.trim_matches('"');
            // charset'language'encoded-value
            return Some(if let Some(encoded) = val.splitn(3, '\'').nth(2) {
                percent_decode_utf8(encoded)
            } else {
                val.to_string()
            });
        }
    }
    None
}

/// Parse plain `filename="name"`.
fn extract_filename_plain(cd: &str) -> Option<String> {
    for part in cd.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("filename=") {
            return Some(val.trim_matches('"').to_string());
        }
    }
    None
}

fn percent_decode_utf8(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push(((hi << 4) | lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
