use std::sync::LazyLock;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::http::{HeaderMap, header};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use rand::RngCore;
use regex::Regex;
use sha2::Sha256;
use url::Url;

use crate::error::{AppError, AppResult};

type HmacSha256 = Hmac<Sha256>;

static QUOTE_REFERENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&gt;&gt;(\d{1,19})").expect("valid quote regex"));

pub fn keyed_hash(secret: &str, value: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(value);
    mac.finalize().into_bytes().to_vec()
}

fn secure_trip_display_name(value: &str) -> AppResult<String> {
    let name = clean_text(value, 35);
    if name.contains('!') {
        return Err(AppError::bad_request(
            "Display names cannot contain ! because that marker is reserved for secure tripcodes.",
        ));
    }
    Ok(if name.is_empty() {
        "Anonymous".to_owned()
    } else {
        name
    })
}

pub fn secure_trip_identity(
    app_secret: &str,
    raw_name: &str,
) -> AppResult<(String, Option<String>)> {
    let Some((display_name, trip_secret)) = raw_name.split_once("##") else {
        return Ok((secure_trip_display_name(raw_name)?, None));
    };

    let trip_secret = trip_secret.trim();
    let secret_chars = trip_secret.chars().count();
    if secret_chars < 12 {
        return Err(AppError::bad_request(
            "A secure tripcode secret must contain at least 12 characters.",
        ));
    }
    if secret_chars > 128 {
        return Err(AppError::bad_request(
            "A secure tripcode secret cannot exceed 128 characters.",
        ));
    }

    let name = secure_trip_display_name(display_name)?;
    let mut mac =
        HmacSha256::new_from_slice(app_secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(b"adelia-secure-tripcode-v1\0");
    mac.update(trip_secret.as_bytes());
    let encoded = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Ok((name, Some(encoded[..16].to_owned())))
}

pub fn random_token(bytes: usize) -> String {
    let mut value = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut value);
    URL_SAFE_NO_PAD.encode(value)
}

pub fn password_hash(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| anyhow::anyhow!("password hashing failed: {error}"))?
        .to_string())
}

pub fn password_matches(password: &str, encoded: &str) -> bool {
    let Ok(hash) = PasswordHash::new(encoded) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .is_ok()
}

pub fn format_post_body(body: &str) -> String {
    let mut rendered = String::with_capacity(body.len() + 32);
    for (index, line) in body.lines().enumerate() {
        if index > 0 {
            rendered.push_str("<br>\n");
        }
        let escaped = html_escape::encode_text(line).into_owned();
        let is_green = escaped.starts_with("&gt;") && !escaped.starts_with("&gt;&gt;");
        let linked = QUOTE_REFERENCE.replace_all(&escaped, |captures: &regex::Captures<'_>| {
            let id = &captures[1];
            format!("<a class=\"quote-link\" href=\"#{id}\">&gt;&gt;{id}</a>")
        });
        if is_green {
            rendered.push_str("<span class=\"quote\">");
            rendered.push_str(&linked);
            rendered.push_str("</span>");
        } else {
            rendered.push_str(&linked);
        }
    }
    rendered
}

pub fn clean_text(value: &str, max_chars: usize) -> String {
    value
        .trim()
        .chars()
        .filter(|character| !character.is_control() || *character == '\n' || *character == '\t')
        .take(max_chars)
        .collect()
}

pub fn enforce_same_origin(headers: &HeaderMap) -> AppResult<()> {
    if headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("cross-site"))
    {
        return Err(AppError::forbidden(
            "Cross-site form submissions are not accepted.",
        ));
    }

    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(());
    };
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return Err(AppError::forbidden("The request host is missing."));
    };
    let parsed =
        Url::parse(origin).map_err(|_| AppError::forbidden("The request origin is invalid."))?;
    let origin_host = match parsed.port() {
        Some(port) => format!("{}:{port}", parsed.host_str().unwrap_or_default()),
        None => parsed.host_str().unwrap_or_default().to_owned(),
    };
    if !origin_host.eq_ignore_ascii_case(host) {
        return Err(AppError::forbidden(
            "Cross-site form submissions are not accepted.",
        ));
    }
    Ok(())
}

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(key, value)| (key == name).then(|| value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_markup_is_escaped_and_links_quotes() {
        let body = format_post_body("<script>x</script>\n>>42\n>good move");
        assert!(!body.contains("<script>"));
        assert!(body.contains("href=\"#42\""));
        assert!(body.contains("class=\"quote\""));
    }

    #[test]
    fn secure_tripcodes_are_stable_keyed_and_secret_free() {
        let first = secure_trip_identity(
            "a sufficiently long application secret",
            "Grandmaster##a-long-private-passphrase",
        )
        .expect("tripcode should be accepted");
        let repeated = secure_trip_identity(
            "a sufficiently long application secret",
            "Different name##a-long-private-passphrase",
        )
        .expect("tripcode should be accepted");
        let different_key = secure_trip_identity(
            "a different sufficiently long secret",
            "Grandmaster##a-long-private-passphrase",
        )
        .expect("tripcode should be accepted");

        assert_eq!(first.0, "Grandmaster");
        assert_eq!(first.1, repeated.1);
        assert_ne!(first.1, different_key.1);
        let code = first.1.expect("secure tripcode should be present");
        assert_eq!(code.len(), 16);
        assert!(!code.contains("private"));
    }

    #[test]
    fn secure_tripcodes_require_a_strong_private_part() {
        assert!(secure_trip_identity("application secret", "Player##short").is_err());
        assert_eq!(
            secure_trip_identity("application secret", "Player")
                .expect("ordinary name should be accepted"),
            ("Player".to_owned(), None)
        );
    }

    #[test]
    fn secure_tripcode_marker_cannot_be_spoofed_in_a_plain_name() {
        assert!(secure_trip_identity("application secret", "Player!!fake-code").is_err());
        assert!(
            secure_trip_identity("application secret", "Player!##a-long-private-passphrase")
                .is_err()
        );
    }

    #[test]
    fn argon2_password_hashes_verify_without_storing_plaintext() {
        let hash = password_hash("club-password-example").expect("password should hash");
        assert!(!hash.contains("club-password-example"));
        assert!(password_matches("club-password-example", &hash));
        assert!(!password_matches("wrong-password", &hash));
    }
}
