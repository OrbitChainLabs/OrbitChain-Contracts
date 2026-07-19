//! Transaction signing request builder.
//!
//! Builds signing requests for donation, campaign creation, and custom
//! transactions, with JSON serialization for wallet compatibility and QR export.
//! Server-side signing (issue #12) produces a real Ed25519 signature
//! verifiable from the public key alone.

use anyhow::{anyhow, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;

/// Crockford Base32 alphabet used by Stellar strkeys.
///
/// NB: this is the *encoding* alphabet, not the *display* alphabet — `I`,
/// `L`, `O`, and `U` are valid characters in a strkey and must not be
/// filtered. The visual-ambiguity rule (dropping `I/L/O/0/1` from display)
/// is a separate, downstream concern. Stellar keys always emit values in
/// `A-Z` followed by `2-7`.
const CROCKFORD_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Version byte for Stellar ed25519 public account IDs.
///
/// This is the *strkey* version byte (`6 << 3`), not the ASCII letter `G`.
/// It is what makes the base32 encoding begin with `G`; using `b'G'` (0x47)
/// here instead produces an `I…` string that Stellar wallets — and this
/// crate's own `KeyManager::validate_public_key` — reject.
const ED25519_PUBLIC_VERSION_BYTE: u8 = 6 << 3;

/// Version byte for Stellar ed25519 secret seeds (`18 << 3`), which makes the
/// encoding begin with `S`. See the note above; `b'S'` (0x53) is wrong.
///
/// Only the strkey round-trip test re-encodes a seed today (production paths
/// decode seeds but never encode them), so this is test-gated to keep the
/// non-test build free of dead code.
#[cfg(test)]
const ED25519_SEED_VERSION_BYTE: u8 = 18 << 3;

/// Decoded strkey length in bytes: 1 version byte + 32 key bytes + 2 CRC16 bytes.
const STRKEY_DECODED_LEN: usize = 35;

/// Represents a signing request for a Stellar transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningRequest {
    pub id: String,
    pub network: String,
    pub transaction_xdr: String,
    pub description: String,
    pub created_at: u64,
}

/// Builder for creating signing requests
pub struct SigningRequestBuilder {
    id: String,
    network: String,
    transaction_xdr: String,
    description: String,
    created_at: u64,
}

impl SigningRequestBuilder {
    /// Create a new signing request builder
    pub fn new(transaction_xdr: String, network: Option<String>) -> Result<Self> {
        let network = network.unwrap_or_else(|| {
            env::var("SOROBAN_NETWORK").unwrap_or_else(|_| "testnet".to_string())
        });

        let id = format!("req_{}", chrono::Local::now().timestamp_millis());

        Ok(SigningRequestBuilder {
            id,
            network,
            transaction_xdr,
            description: String::new(),
            created_at: chrono::Local::now().timestamp() as u64,
        })
    }

    /// Set description for the signing request
    pub fn with_description(mut self, description: String) -> Self {
        self.description = description;
        self
    }

    /// Build the signing request
    pub fn build(self) -> Result<SigningRequest> {
        if self.transaction_xdr.is_empty() {
            return Err(anyhow!("Transaction XDR cannot be empty"));
        }

        Ok(SigningRequest {
            id: self.id,
            network: self.network,
            transaction_xdr: self.transaction_xdr,
            description: self.description,
            created_at: self.created_at,
        })
    }
}

/// Helper for building common transaction types
pub struct TransactionBuilder;

impl TransactionBuilder {
    /// Build a donation transaction signing request
    pub fn build_donation_request(
        donor_address: String,
        campaign_id: u64,
        amount: i128,
        asset: String,
        memo: Option<String>,
    ) -> Result<SigningRequest> {
        let desc = format!("Donate {} {} to campaign #{}", amount, asset, campaign_id);

        // Placeholder XDR - in real implementation, this would be built from actual transaction
        let transaction_xdr = format!("AAAAAA=={}{}{}", donor_address, campaign_id, amount);

        let mut builder = SigningRequestBuilder::new(transaction_xdr, None)?.with_description(desc);

        if let Some(m) = memo {
            let desc = format!("{} [memo: {}]", builder.description, m);
            builder = builder.with_description(desc);
        }

        builder.build()
    }

    /// Build a campaign creation transaction signing request
    pub fn build_campaign_request(
        creator_address: String,
        title: String,
        goal: i128,
        deadline: u64,
    ) -> Result<SigningRequest> {
        let desc = format!(
            "Create campaign '{}' with goal {} until {}",
            title, goal, deadline
        );

        let transaction_xdr = format!("AAAAAA=={}{}{}{}", creator_address, title, goal, deadline);

        SigningRequestBuilder::new(transaction_xdr, None)?
            .with_description(desc)
            .build()
    }
}

impl SigningRequest {
    /// Convert signing request to JSON for transmission.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize signing request to JSON")
    }

    /// Create from JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize signing request from JSON")
    }

    /// Convert to wallet signing format (for Freighter and similar)
    pub fn to_wallet_format(&self) -> Result<String> {
        let wallet_request = json!({
            "id": self.id,
            "type": "tx",
            "xdr": self.transaction_xdr,
            "network": self.network,
            "description": self.description,
            "timestamp": self.created_at,
        });

        Ok(wallet_request.to_string())
    }

    /// Display request details
    pub fn display(&self) {
        println!("📝 Signing Request");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("ID:          {}", self.id);
        println!("Network:     {}", self.network);
        println!("Description: {}", self.description);
        println!("Created:     {}", self.created_at);
        println!();
        println!("Transaction XDR:");
        println!("{}", self.transaction_xdr);
    }

    /// Validate the signing request.
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            return Err(anyhow!("Request ID cannot be empty"));
        }

        if self.transaction_xdr.is_empty() {
            return Err(anyhow!("Transaction XDR cannot be empty"));
        }

        match self.network.as_str() {
            "testnet" | "mainnet" | "public" => Ok(()),
            _ => Err(anyhow!("Invalid network: {}", self.network)),
        }
    }

    /// Get QR code data for mobile wallet.
    pub fn to_qr_data(&self) -> Result<String> {
        self.to_wallet_format()
    }
}

/// Issue #12 — A transaction that has been signed server-side.
///
/// `signature` is a real Ed25519 signature over `transaction_xdr`,
/// verifiable with **only** `signer_public_key` — the verifier does not
/// need to know the secret. `algorithm` discriminates the signature
/// scheme so that a future migration (e.g. to a Soroban-native signer)
/// can be introduced safely. Older payloads that lack `algorithm` /
/// `signer_public_key` still deserialize (fields default), but cannot
/// be verified until those fields are populated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSignedTransaction {
    pub request_id: String,
    pub transaction_xdr: String,
    /// Hex-encoded Ed25519 signature over `transaction_xdr`.
    pub signature: String,
    /// Signature algorithm — currently always `"ed25519"`.
    #[serde(default = "default_signature_algorithm")]
    pub algorithm: String,
    /// Stellar public key (`G…`) of the signer. Required for `verify()`.
    #[serde(default)]
    pub signer_public_key: String,
    pub signed_at: i64,
}

fn default_signature_algorithm() -> String {
    "ed25519".to_string()
}

impl ServerSignedTransaction {
    /// Verify this server signature using only the public key.
    ///
    /// Returns `Ok(true)` when the Ed25519 signature over
    /// `transaction_xdr` authenticates against `signer_public_key`,
    /// `Ok(false)` when the signature is invalid, and an error when the
    /// payload is malformed or the algorithm is unsupported.
    ///
    /// # Errors
    ///
    /// * `algorithm` is not `"ed25519"`.
    /// * `signer_public_key` is empty.
    /// * `signer_public_key` is not a valid Stellar account id (strkey
    ///   decode fails or the CRC16-XModem checksum mismatches).
    /// * `signature` is not valid hex.
    /// * `signature` decodes to anything other than 64 bytes.
    pub fn verify(&self) -> Result<bool> {
        if self.algorithm != "ed25519" {
            anyhow::bail!(
                "Unsupported signature algorithm: {:?} (expected \"ed25519\")",
                self.algorithm
            );
        }
        if self.signer_public_key.is_empty() {
            anyhow::bail!("ServerSignedTransaction.signer_public_key is empty; cannot verify");
        }

        let public_bytes = strkey_decode(&self.signer_public_key, "public")?;
        let verifying_key = VerifyingKey::from_bytes(&public_bytes)
            .map_err(|e| anyhow!("Invalid Ed25519 verifying key: {}", e))?;

        let sig_bytes = hex::decode(&self.signature).context("signature is not valid hex")?;
        if sig_bytes.len() != 64 {
            anyhow::bail!(
                "Ed25519 signature must decode to 64 bytes, got {}",
                sig_bytes.len()
            );
        }
        // ed25519-dalek 2.x takes a fixed-size array; the length is already
        // guaranteed by the check above, so this conversion cannot fail.
        let sig_array: [u8; 64] = sig_bytes
            .as_slice()
            .try_into()
            .context("Ed25519 signature must decode to 64 bytes")?;
        let signature = Signature::from_bytes(&sig_array);

        Ok(verifying_key
            .verify(self.transaction_xdr.as_bytes(), &signature)
            .is_ok())
    }
}

impl SigningRequest {
    /// Issue #12 — Sign this transaction server-side with a real Ed25519
    /// signature verifiable from the public key alone.
    ///
    /// Replaces the previous `SHA-256(secret_key || xdr)` "signature",
    /// which could only be re-derived by callers that already had the
    /// secret key (i.e. was not a signature in any useful sense) and was
    /// flagged as a security issue.
    pub fn sign_server_side(&self, secret_key: &str) -> Result<ServerSignedTransaction> {
        if secret_key.is_empty() {
            return Err(anyhow!("Secret key must not be empty"));
        }
        crate::key_manager::KeyManager::validate_secret_key(secret_key)?;

        // `strkey_decode` already yields a validated [u8; 32]; in ed25519-dalek
        // 2.x `SigningKey::from_bytes` is infallible and returns the key
        // directly (1.x returned a Result, hence the previous `map_err`).
        let seed_bytes = strkey_decode(secret_key, "secret")?;
        let signing_key = SigningKey::from_bytes(&seed_bytes);
        let verifying_key = signing_key.verifying_key();

        // `SigningKey::sign` (from the `Signer` trait) is RFC 8032
        // deterministic Ed25519 — no RNG is required and the same
        // (key, msg) always yields the same signature, matching the
        // property callers previously relied on.
        let signature: Signature = signing_key.sign(self.transaction_xdr.as_bytes());
        let sig_hex = hex::encode(signature.to_bytes());
        let signer_public_key =
            strkey_encode(&verifying_key.to_bytes(), ED25519_PUBLIC_VERSION_BYTE);

        Ok(ServerSignedTransaction {
            request_id: self.id.clone(),
            transaction_xdr: self.transaction_xdr.clone(),
            signature: sig_hex,
            algorithm: default_signature_algorithm(),
            signer_public_key,
            signed_at: chrono::Local::now().timestamp(),
        })
    }

    /// Sign using the secret key stored in the `SOROBAN_SECRET_KEY` env var.
    pub fn sign_from_env(&self) -> Result<ServerSignedTransaction> {
        let secret_key =
            env::var("SOROBAN_SECRET_KEY").context("SOROBAN_SECRET_KEY not set in environment")?;
        self.sign_server_side(&secret_key)
    }
}

// ─── Stellar strkey codec (decoded locally; no separate strkey crate) ───

/// CRC16-XModem over `data`, matching Stellar's strkey checksum convention.
fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Decode a Stellar strkey (`S…` seed or `G…` account id) into its
/// 32-byte Ed25519 key material, validating the trailing CRC16-XModem
/// checksum.
fn strkey_decode(strkey: &str, kind: &str) -> Result<[u8; 32]> {
    // 35 raw bytes encode to exactly 56 characters (35*8 = 280 bits,
    // 280/5 = 56 chars). Reject everything else up front so we never do
    // extra work or risk partial decoding on malformed input.
    if strkey.len() != 56 {
        anyhow::bail!(
            "{} strkey must be exactly 56 chars, got {}",
            kind,
            strkey.len()
        );
    }

    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut decoded: Vec<u8> = Vec::with_capacity(STRKEY_DECODED_LEN);

    for c in strkey.chars() {
        let v: u32 = match c {
            'A'..='Z' => u32::from(c) - u32::from('A'),
            '2'..='7' => 26 + u32::from(c) - u32::from('2'),
            _ => anyhow::bail!("{} strkey contains invalid character: {:?}", kind, c),
        };
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            decoded.push(((acc >> bits) & 0xff) as u8);
            acc &= (1u32 << bits) - 1;
        }
    }

    if decoded.len() != STRKEY_DECODED_LEN {
        anyhow::bail!(
            "{} strkey decoded to {} bytes, expected {}",
            kind,
            decoded.len(),
            STRKEY_DECODED_LEN
        );
    }

    let (signed_body, crc_part) = decoded.split_at(33);
    let expected = u16::from_le_bytes([crc_part[0], crc_part[1]]);
    let actual = crc16_xmodem(signed_body);
    if expected != actual {
        anyhow::bail!(
            "{} strkey CRC mismatch (got {:04x}, expected {:04x})",
            kind,
            actual,
            expected
        );
    }

    let mut out = [0u8; 32];
    out.copy_from_slice(&signed_body[1..]); // skip version byte
    Ok(out)
}

/// Encode a 32-byte Stellar key back into a strkey with the given
/// version byte (`b'G'` for account id, `b'S'` for seed).
#[must_use]
fn strkey_encode(key: &[u8; 32], type_byte: u8) -> String {
    let mut to_encode: Vec<u8> = Vec::with_capacity(STRKEY_DECODED_LEN);
    to_encode.push(type_byte);
    to_encode.extend_from_slice(key);
    let crc = crc16_xmodem(&to_encode);
    to_encode.push(crc as u8); // little-endian: low byte first
    to_encode.push((crc >> 8) as u8);

    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = String::with_capacity(56);
    for byte in &to_encode {
        acc = (acc << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((acc >> bits) & 0x1f) as usize;
            out.push(CROCKFORD_ALPHABET[idx] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Classic Stellar testnet keypair — round-trips through `strkey_*`.
    /// Public key is the canonical Ed25519 point derived from the seed;
    /// signing and verifying should never produce a different public key.
    const FIXTURE_SECRET: &str = "SAVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCVLG5";
    const FIXTURE_PUBLIC: &str = "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W";

    fn fixture_request() -> SigningRequest {
        SigningRequest {
            id: "req_123".to_string(),
            network: "testnet".to_string(),
            transaction_xdr: "AAAAAA==test_xdr".to_string(),
            description: "Test".to_string(),
            created_at: 0,
        }
    }

    #[test]
    fn test_signing_request_builder() {
        let xdr = "AAAAAA==test".to_string();
        let req = SigningRequestBuilder::new(xdr, Some("testnet".to_string()))
            .unwrap()
            .with_description("Test donation".to_string())
            .build();

        assert!(req.is_ok());
        let req = req.unwrap();
        assert!(req.id.starts_with("req_"));
        assert_eq!(req.network, "testnet");
        assert_eq!(req.description, "Test donation");
    }

    #[test]
    fn test_signing_request_validation() {
        let req = SigningRequest {
            id: "req_123".to_string(),
            network: "testnet".to_string(),
            transaction_xdr: "AAAAAA==".to_string(),
            description: "Test".to_string(),
            created_at: 0,
        };

        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_signing_request_json() {
        let req = SigningRequest {
            id: "req_123".to_string(),
            network: "testnet".to_string(),
            transaction_xdr: "AAAAAA==".to_string(),
            description: "Test".to_string(),
            created_at: 0,
        };

        let json = req.to_json().unwrap();
        let restored = SigningRequest::from_json(&json).unwrap();
        assert_eq!(restored.id, req.id);
    }

    #[test]
    fn test_strkey_roundtrip_preserves_key() {
        let decoded_seed = strkey_decode(FIXTURE_SECRET, "secret").unwrap();
        let encoded_seed = strkey_encode(&decoded_seed, ED25519_SEED_VERSION_BYTE);
        assert_eq!(encoded_seed, FIXTURE_SECRET);

        let decoded_pub = strkey_decode(FIXTURE_PUBLIC, "public").unwrap();
        let encoded_pub = strkey_encode(&decoded_pub, ED25519_PUBLIC_VERSION_BYTE);
        assert_eq!(encoded_pub, FIXTURE_PUBLIC);
    }

    #[test]
    fn test_strkey_decode_rejects_bad_crc() {
        // Flip the last char of a known-good seed — CRC must fail.
        let mut bad: String = FIXTURE_SECRET.into();
        let last = bad.pop().unwrap();
        bad.push(if last == 'U' { 'A' } else { 'U' });
        assert!(strkey_decode(&bad, "secret").is_err());
    }

    #[test]
    fn test_strkey_decode_rejects_invalid_or_short() {
        assert!(
            strkey_decode(
                "GBZXVMIRWXL5VZVKXWV2FGKYTQ5VV5VRNJYQVZKYWW3XYVYP3IXGKD0",
                "public"
            )
            .is_err(),
            "digit `0` must be rejected by Crockford decoder"
        );
        assert!(
            strkey_decode("tooshort", "secret").is_err(),
            "short input must be rejected"
        );
    }

    #[test]
    fn test_sign_server_side_produces_ed25519_signature() {
        let req = fixture_request();
        let signed = req.sign_server_side(FIXTURE_SECRET).unwrap();

        assert_eq!(signed.algorithm, "ed25519");
        assert_eq!(signed.signer_public_key, FIXTURE_PUBLIC);
        assert_eq!(signed.signature.len(), 128); // 64 bytes hex-encoded
        assert!(hex::decode(&signed.signature).is_ok());
        assert_eq!(signed.transaction_xdr, req.transaction_xdr);
        assert_eq!(signed.request_id, req.id);
    }

    #[test]
    fn test_sign_server_side_signature_verifies() {
        let req = fixture_request();
        let signed = req.sign_server_side(FIXTURE_SECRET).unwrap();
        assert!(signed.verify().unwrap(), "fresh signature must verify");
    }

    #[test]
    fn test_verify_rejects_tampered_xdr() {
        let req = fixture_request();
        let mut signed = req.sign_server_side(FIXTURE_SECRET).unwrap();
        signed.transaction_xdr.push_str("tampered");
        assert!(
            !signed.verify().unwrap(),
            "XDR tamper must fail verification"
        );
    }

    #[test]
    fn test_verify_rejects_single_bit_signature_flip() {
        let req = fixture_request();
        let mut signed = req.sign_server_side(FIXTURE_SECRET).unwrap();
        let mut sig_bytes = hex::decode(&signed.signature).unwrap();
        sig_bytes[0] ^= 0x01;
        signed.signature = hex::encode(sig_bytes);
        assert!(!signed.verify().unwrap(), "1-bit signature flip must fail");
    }

    #[test]
    fn test_verify_rejects_wrong_public_key() {
        let req = fixture_request();
        // Sign with one secret, then swap in an unrelated public key.
        let mut signed = req.sign_server_side(FIXTURE_SECRET).unwrap();
        signed.signer_public_key =
            "GCT7NX5PR44LRG5IZZSJWWKPSHSNAH64K744SSJ56Q5V4UFJTBZWOOJ6".to_string();
        assert!(!signed.verify().unwrap());
    }

    #[test]
    fn test_verify_rejects_unknown_algorithm() {
        let req = fixture_request();
        let mut signed = req.sign_server_side(FIXTURE_SECRET).unwrap();
        signed.algorithm = "rsa".to_string();
        assert!(signed.verify().is_err());
    }

    #[test]
    fn test_verify_rejects_missing_public_key() {
        let req = fixture_request();
        let mut signed = req.sign_server_side(FIXTURE_SECRET).unwrap();
        signed.signer_public_key.clear();
        assert!(signed.verify().is_err());
    }

    #[test]
    fn test_sign_server_side_is_deterministic() {
        // RFC 8032 deterministic Ed25519 must produce byte-identical
        // signatures for the same (key, message).
        let req = fixture_request();
        let sig1 = req.sign_server_side(FIXTURE_SECRET).unwrap().signature;
        let sig2 = req.sign_server_side(FIXTURE_SECRET).unwrap().signature;
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_sign_server_side_different_keys_differ() {
        let req = fixture_request();
        let sig1 = req.sign_server_side(FIXTURE_SECRET).unwrap().signature;
        let sig2 = req
            .sign_server_side("SBRWGY3DMNRWGY3DMNRWGY3DMNRWGY3DMNRWGY3DMNRWGY3DMNRWGK3F")
            .unwrap()
            .signature;
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_sign_server_side_different_xdrs_differ() {
        let mut req_a = fixture_request();
        req_a.transaction_xdr = "AAAAAA==xdr_a".to_string();
        let mut req_b = fixture_request();
        req_b.transaction_xdr = "AAAAAA==xdr_b".to_string();
        let sig_a = req_a.sign_server_side(FIXTURE_SECRET).unwrap().signature;
        let sig_b = req_b.sign_server_side(FIXTURE_SECRET).unwrap().signature;
        assert_ne!(
            sig_a, sig_b,
            "same key, different XDR must yield different signatures"
        );
    }

    #[test]
    fn test_sign_server_side_rejects_invalid_key() {
        let req = fixture_request();
        assert!(req.sign_server_side("not_a_valid_key").is_err());
        assert!(req.sign_server_side("").is_err());
    }

    #[test]
    fn test_server_signed_transaction_json_roundtrip_works() {
        let req = fixture_request();
        let signed = req.sign_server_side(FIXTURE_SECRET).unwrap();
        let json = serde_json::to_string_pretty(&signed).unwrap();
        let restored: ServerSignedTransaction = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.signature, signed.signature);
        assert_eq!(restored.signer_public_key, signed.signer_public_key);
        assert_eq!(restored.algorithm, signed.algorithm);
        assert!(
            restored.verify().unwrap(),
            "restored payload must still verify"
        );
    }

    #[test]
    fn test_server_signed_transaction_json_back_compat_parses_legacy_payload() {
        // Pre-#12 payloads lack `algorithm` / `signer_public_key`. They
        // must still parse (serde defaults), but `verify()` must refuse
        // because there is no signer public key to verify against.
        //
        // The 32-byte `signature` here is intentionally the legacy
        // SHA-256 size; the modern verifier rejects anything that isn't
        // 64 bytes (Ed25519 produces 64-byte signatures).
        let legacy = r#"{
            "request_id": "req_legacy",
            "transaction_xdr": "AAAAAA==",
            "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            "signed_at": 1234567890
        }"#;
        let parsed: ServerSignedTransaction = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.algorithm, "ed25519");
        assert!(parsed.signer_public_key.is_empty());
        assert!(parsed.verify().is_err());
    }
}
