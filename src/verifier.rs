use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use sha2::{Digest, Sha256};

/// Checksum verification utilities for release assets.
pub struct ChecksumVerifier;

impl ChecksumVerifier {
    /// Computes the lowercase SHA256 hexadecimal string for a file on disk.
    pub fn compute_sha256(path: &Path) -> std::io::Result<String> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 16 * 1024];

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash_bytes = hasher.finalize();
        Ok(hex::encode(hash_bytes))
    }

    /// Verifies if the file matches the expected SHA256 string (case-insensitive).
    pub fn verify_file(path: &Path, expected_sha256: &str) -> std::io::Result<bool> {
        let actual = Self::compute_sha256(path)?;
        Ok(actual.eq_ignore_ascii_case(expected_sha256.trim()))
    }

    /// Parses a checksum file content (e.g. `SHA256SUMS.txt` or `checksums.txt`).
    /// Supports GNU/BSD standard formats:
    /// `<hash>  <filename>` or `<hash> *<filename>`
    pub fn parse_checksum_map(content: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Split into hash and filename parts
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let hash = parts[0].trim().to_lowercase();
                // Strip optional leading asterisk from binary-mode hashing
                let filename = parts[1].trim_start_matches('*').trim().to_string();
                if hash.len() == 64 {
                    map.insert(filename, hash);
                }
            }
        }
        map
    }
}
