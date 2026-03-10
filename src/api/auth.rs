use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Generate HMAC-SHA256 signature for LN Markets API v3 authentication
///
/// The signature is computed as: Base64(HMAC-SHA256(secret, timestamp + method.lowercase() + path + data))
pub fn generate_signature(
    secret: &str,
    timestamp: u64,
    method: &str,
    path: &str,
    data: &str,
) -> String {
    let message = format!("{}{}{}{}", timestamp, method.to_lowercase(), path, data);

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(message.as_bytes());

    let result = mac.finalize();
    BASE64.encode(result.into_bytes())
}

/// Get current timestamp in milliseconds
pub fn get_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_generation() {
        let signature = generate_signature(
            "test_secret",
            1234567890000,
            "GET",
            "/v3/user",
            "",
        );
        // Signature should be a base64 string
        assert!(!signature.is_empty());
        assert!(signature.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='));
    }

    #[test]
    fn test_lowercase_method() {
        // Both should produce same signature since method is lowercased
        let sig1 = generate_signature("secret", 1234567890000, "GET", "/v3/test", "");
        let sig2 = generate_signature("secret", 1234567890000, "get", "/v3/test", "");
        assert_eq!(sig1, sig2);
    }
}
