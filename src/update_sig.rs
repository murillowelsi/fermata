//! F-34: signatures of update files. The release signs each update file with an Ed25519 key kept outside the
//! repository (`tools/sign-update`); the app has the public key built in and installs nothing whose signature or
//! SHA-256 doesn't match. This file is shared by the app and `tools/sign-update`.
//!
//! A `.sig` file is four lines of text:
//!
//! ```text
//! fermata-update-sig-v1
//! file Fermata-macOS-update.zip
//! sha256 <64 hex digits>
//! signature <128 hex digits>
//! ```
//!
//! The signature covers the first three lines (format, file name and SHA-256), so a signature made for the Windows
//! file can't be used for the Mac one, nor for a changed file.
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

pub const FORMAT: &str = "fermata-update-sig-v1";

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn message(name: &str, sha: &str) -> String {
    format!("{FORMAT}\nfile {name}\nsha256 {sha}\n")
}

/// The `.sig` text for `data`, named `name` in the release (tools/sign-update and the tests).
#[allow(dead_code)]
pub fn sign(key: &SigningKey, name: &str, data: &[u8]) -> String {
    let sha = sha256_hex(data);
    let m = message(name, &sha);
    let sig = key.sign(m.as_bytes());
    format!("{m}signature {}\n", hex(&sig.to_bytes()))
}

/// The public key (hex) of a secret key (hex), for `keygen`.
#[allow(dead_code)]
pub fn public_of(secret: &[u8; 32]) -> String {
    hex(SigningKey::from_bytes(secret).verifying_key().as_bytes())
}

/// Checks `data` against its `.sig` text: the file must be the one named `expect_name`, have the SHA-256 written
/// there, and the signature must be valid for `public_hex`.
pub fn verify(public_hex: &str, expect_name: &str, data: &[u8], sig_text: &str) -> Result<(), String> {
    let pk: [u8; 32] = unhex(public_hex).and_then(|v| v.try_into().ok()).ok_or("this build has no valid update key")?;
    let pk = VerifyingKey::from_bytes(&pk).map_err(|_| "this build has no valid update key".to_string())?;
    let mut lines = sig_text.lines();
    let (Some(f), Some(file), Some(sha), Some(sig)) = (lines.next(), lines.next(), lines.next(), lines.next()) else {
        return Err("the signature file is incomplete".into());
    };
    if f.trim() != FORMAT {
        return Err("unknown signature format".into());
    }
    let file = file.strip_prefix("file ").ok_or("the signature file is damaged")?.trim();
    let sha = sha.strip_prefix("sha256 ").ok_or("the signature file is damaged")?.trim().to_lowercase();
    let sig = sig.strip_prefix("signature ").ok_or("the signature file is damaged")?;
    if file != expect_name {
        return Err(format!("the signature is for {file}, not {expect_name}"));
    }
    if sha != sha256_hex(data) {
        return Err("the file's SHA-256 doesn't match its signature".into());
    }
    let sig: [u8; 64] = unhex(sig).and_then(|v| v.try_into().ok()).ok_or("the signature is damaged")?;
    pk.verify(message(file, &sha).as_bytes(), &Signature::from_bytes(&sig)).map_err(|_| "the signature isn't valid".to_string())
}
