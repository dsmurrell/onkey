//! A challenge link's signature. "Challenge a friend" copies a link to singonkey.app/c/ that says "<name> scored 78
//! on <song>" (ui/lib/challenge.ts); the score was plain text in the link, so anyone could raise it in the address
//! bar. The app now signs what the link says, and the site shows a score as checked only when the signature fits
//! (site/lib/challenge.ts, which holds the public key). Nothing goes to a server: the signature travels in the
//! link's fragment with the rest.
//!
//! The private key is given to the build (ONKEY_CHALLENGE_KEY: 64 hex digits, the P-256 secret number), as the
//! updater's key is, and is not in the source: a build without it makes unsigned links, which the site shows as not
//! checked. This stops a score being edited in the link. It can't stop someone who takes the key out of the app, or
//! changes a take's file on their own computer: the score is worked out on the singer's machine.

use p256::ecdsa::{signature::Signer, Signature, SigningKey};

const KEY: Option<&str> = option_env!("ONKEY_CHALLENGE_KEY");

/// The longest each part of a link may be (ui/lib/challenge.ts MAX; UTF-16 units there, never more chars than this).
const MAX: (usize, usize, usize) = (100, 80, 24);

/// What is signed: the link's parts as it holds them, one to a line (no part can hold a line break: challenge.ts
/// turns control characters into spaces). The site builds the same text from the link it was given.
pub fn message(song: &str, by: &str, score: u32, name: &str) -> String {
    format!("onkey-challenge-v1\n{song}\n{by}\n{score}\n{name}")
}

/// The signature for a link with these parts (base64url of r and s, 86 characters), or None from a build without the
/// key, or for parts no link can hold.
pub fn sign(song: &str, by: &str, score: u32, name: &str) -> Option<String> {
    sign_with(KEY?, song, by, score, name)
}

fn sign_with(key_hex: &str, song: &str, by: &str, score: u32, name: &str) -> Option<String> {
    let fits = |s: &str, max: usize| s.chars().count() <= max && !s.chars().any(char::is_control);
    if song.is_empty() || !fits(song, MAX.0) || !fits(by, MAX.1) || !fits(name, MAX.2) || score > 100 {
        return None;
    }
    let key = SigningKey::from_slice(&unhex(key_hex.trim())?).ok()?;
    let sig: Signature = key.sign(message(song, by, score, name).as_bytes());
    Some(base64url(&sig.to_bytes()))
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.is_ascii() {
        return None;
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect()
}

/// Base64 with - and _ and no padding (RFC 4648 section 5), as it goes in a link.
fn base64url(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..=c.len() {
            out.push(ABC[(n >> (18 - 6 * k) & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{signature::Verifier, VerifyingKey};

    /// A key for tests only (site/tests/challenge.test.mjs checks the same signature with its public half).
    const TEST_KEY: &str = "c9afa9d845ba75166b5c215767b1d6934e50c3db36e89b127b8a622b120f6721";

    #[test]
    fn a_link_is_signed_and_the_signature_fits_only_what_it_says() {
        let sig = sign_with(TEST_KEY, "Everybody's Changing", "Keane", 81, "Daniel").unwrap();
        assert_eq!(sig.len(), 86);
        // (the same signature every time: site/tests/challenge.test.mjs has this one)
        assert_eq!(sig, "F9tUas_rFTv-shV8UOEw3cT9aS1l9wK0_Re0rKqdkWjOi7iSx6gmwMXmC7Px8v2UED2_2kUILb53QfOxuwIKTQ");
        let key = SigningKey::from_slice(&unhex(TEST_KEY).unwrap()).unwrap();
        let public = VerifyingKey::from(&key);
        let raw = |s: &str| -> Vec<u8> {
            let abc = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let bits: Vec<u8> = s.chars().flat_map(|c| { let v = abc.find(c).unwrap() as u8; (0..6).rev().map(move |k| v >> k & 1) }).collect();
            bits.chunks(8).filter(|c| c.len() == 8).map(|c| c.iter().fold(0, |a, b| a << 1 | b)).collect()
        };
        let fits = |score: u32, name: &str| {
            let s = Signature::from_slice(&raw(&sig)).unwrap();
            public.verify(message("Everybody's Changing", "Keane", score, name).as_bytes(), &s).is_ok()
        };
        assert!(fits(81, "Daniel"));
        assert!(!fits(99, "Daniel") && !fits(81, "Dan"));
        // the same link signed again is the same link
        assert_eq!(sign_with(TEST_KEY, "Everybody's Changing", "Keane", 81, "Daniel").unwrap(), sig);
    }

    #[test]
    fn nothing_is_signed_that_a_link_cant_hold() {
        assert!(sign_with(TEST_KEY, "", "", 50, "").is_none());
        assert!(sign_with(TEST_KEY, "Yellow", "", 101, "").is_none());
        assert!(sign_with(TEST_KEY, "Yel\nlow", "", 50, "").is_none());
        assert!(sign_with(TEST_KEY, &"x".repeat(101), "", 50, "").is_none());
        assert!(sign_with("not a key", "Yellow", "", 50, "").is_none());
        assert!(sign_with(TEST_KEY, "Yellow", "", 50, "").is_some());
    }

    #[test]
    fn base64url_as_in_the_rfc() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }
}
