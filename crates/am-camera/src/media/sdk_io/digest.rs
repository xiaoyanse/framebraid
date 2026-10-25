use super::IoError;
use crate::credentials::Secret;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub(super) fn authorization(
    challenge: &str,
    secret: &Secret,
    uri: &str,
) -> Result<String, IoError> {
    let (scheme, rest) = challenge.split_once(' ').ok_or(IoError::Unsupported)?;
    if !scheme.eq_ignore_ascii_case("digest") || challenge.len() > 16 * 1024 {
        return Err(IoError::Unsupported);
    }
    let mut parts = Vec::new();
    let (mut start, mut quoted, mut escaped) = (0, false, false);
    for (i, c) in rest.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && quoted {
            escaped = true;
            continue;
        }
        if c == '"' {
            quoted = !quoted;
        }
        if c == ',' && !quoted {
            parts.push(&rest[start..i]);
            start = i + 1;
        }
    }
    if quoted || escaped {
        return Err(IoError::Unsupported);
    }
    parts.push(&rest[start..]);
    let mut fields = HashMap::new();
    for part in parts {
        let (key, raw) = part.trim().split_once('=').ok_or(IoError::Unsupported)?;
        let raw = raw.trim();
        let value = if let Some(raw) = raw.strip_prefix('"') {
            let raw = raw.strip_suffix('"').ok_or(IoError::Unsupported)?;
            let mut text = String::new();
            let mut chars = raw.chars();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => text.push(chars.next().ok_or(IoError::Unsupported)?),
                    '"' => return Err(IoError::Unsupported),
                    _ => text.push(c),
                }
            }
            text
        } else {
            if raw.contains('"') {
                return Err(IoError::Unsupported);
            }
            raw.to_owned()
        };
        if value.chars().any(char::is_control)
            || fields
                .insert(key.trim().to_ascii_lowercase(), value)
                .is_some()
        {
            return Err(IoError::Unsupported);
        }
    }
    if secret
        .username
        .chars()
        .chain(secret.password.chars())
        .any(char::is_control)
    {
        return Err(IoError::Invalid);
    }
    let realm = fields.get("realm").ok_or(IoError::Unsupported)?;
    let nonce = fields
        .get("nonce")
        .filter(|s| !s.is_empty())
        .ok_or(IoError::Unsupported)?;
    let algorithm = fields.get("algorithm").map(String::as_str).unwrap_or("MD5");
    let (sha, session) = match algorithm.to_ascii_uppercase().as_str() {
        "MD5" => (false, false),
        "MD5-SESS" => (false, true),
        "SHA-256" => (true, false),
        "SHA-256-SESS" => (true, true),
        _ => return Err(IoError::Unsupported),
    };
    let qop = fields.get("qop");
    if qop.is_some_and(|q| !q.split(',').any(|q| q.trim().eq_ignore_ascii_case("auth"))) {
        return Err(IoError::Unsupported);
    }
    let hash = |s: &str| {
        if sha {
            format!("{:x}", Sha256::digest(s.as_bytes()))
        } else {
            format!("{:x}", md5::compute(s))
        }
    };
    let cnonce = uuid::Uuid::new_v4().simple().to_string();
    let mut ha1 = hash(&format!("{}:{realm}:{}", secret.username, secret.password));
    if session {
        ha1 = hash(&format!("{ha1}:{nonce}:{cnonce}"));
    }
    let ha2 = hash(&format!("GET:{uri}"));
    let response = if qop.is_some() {
        hash(&format!("{ha1}:{nonce}:00000001:{cnonce}:auth:{ha2}"))
    } else {
        hash(&format!("{ha1}:{nonce}:{ha2}"))
    };
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut output = format!("Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{response}\", algorithm={algorithm}", quote(&secret.username), quote(realm), quote(nonce), quote(uri));
    if qop.is_some() {
        output.push_str(&format!(", qop=auth, nc=00000001, cnonce=\"{cnonce}\""));
    } else if session {
        output.push_str(&format!(", cnonce=\"{cnonce}\""));
    }
    if let Some(opaque) = fields.get("opaque") {
        output.push_str(&format!(", opaque=\"{}\"", quote(opaque)));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn algorithms_and_quoted_qop() {
        let secret = Secret {
            username: "test".into(),
            password: "secret".into(),
        };
        for algorithm in ["MD5", "MD5-sess", "SHA-256", "SHA-256-sess"] {
            let header = authorization(
                &format!(
                    "Digest realm=\"r\", nonce=\"n\", qop=\"auth,auth-int\", algorithm={algorithm}"
                ),
                &secret,
                "/a?b=1",
            )
            .unwrap();
            assert!(header.contains("qop=auth"));
            assert!(header.contains(&format!("algorithm={algorithm}")));
        }
        assert_eq!(
            authorization(
                "Digest realm=\"r\",nonce=\"n\",qop=\"auth-int\"",
                &secret,
                "/"
            )
            .err(),
            Some(IoError::Unsupported)
        );
    }
}
