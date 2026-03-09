use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Generate HMAC-SHA256 signature for LN Markets API authentication
///
/// The signature is computed as: HMAC-SHA256(secret, timestamp + method + path + body)
pub fn generate_signature(
    secret: &str,
    timestamp: u64,
    method: &str,
    path: &str,
    body: &str,
) -> String {
    let message = format!("{}{}{}{}", timestamp, method.to_uppercase(), path, body);

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(message.as_bytes());

    let result = mac.finalize();
    hex::encode(result.into_bytes())
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
        // Signature should be a 64-character hex string (32 bytes)
        assert_eq!(signature.len(), 64);
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
