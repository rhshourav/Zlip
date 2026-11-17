use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use zeroize::Zeroizing;

/// 32 random bytes, wiped on drop. Sub-keys are derived with BLAKE3 so that
/// encryption and deduplication never share a key.
pub struct MasterKey(Zeroizing<[u8; 32]>);

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(..)")
    }
}

impl MasterKey {
    /// A fresh key from the operating system's CSPRNG.
    pub fn generate() -> Result<Self, getrandom::Error> {
        let mut k = [0u8; 32];
        getrandom::fill(&mut k)?;
        Ok(Self(Zeroizing::new(k)))
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub(crate) fn derive(&self, context: &str) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(blake3::derive_key(context, &*self.0))
    }
}

pub(crate) struct Sealer {
    cipher: XChaCha20Poly1305,
    fingerprint_key: Zeroizing<[u8; 32]>,
}

pub(crate) const NONCE_LEN: usize = 24;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CryptoError {
    #[error("the system random generator failed")]
    Random,
    #[error("authentication failed (wrong key, or the data was altered)")]
    Auth,
    #[error("sealed data is too short")]
    Short,
}

impl Sealer {
    pub(crate) fn new(key: &MasterKey) -> Self {
        let aead_key = key.derive("zlip 2026 history payload encryption");
        Self {
            cipher: XChaCha20Poly1305::new_from_slice(&*aead_key).expect("derived key is 32 bytes"),
            fingerprint_key: key.derive("zlip 2026 dedupe fingerprint"),
        }
    }

    /// `nonce || ciphertext+tag`. The nonce is random per call (24 bytes, so
    /// collisions are not a practical concern).
    pub(crate) fn seal(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|_| CryptoError::Random)?;
        let ct = self
            .cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| CryptoError::Auth)?;
        let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    pub(crate) fn open(
        &self,
        sealed: &[u8],
        aad: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        let (nonce, ct) = sealed
            .split_at_checked(NONCE_LEN)
            .ok_or(CryptoError::Short)?;
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| CryptoError::Short)?;
        self.cipher
            .decrypt(&XNonce::from(nonce), Payload { msg: ct, aad })
            .map(Zeroizing::new)
            .map_err(|_| CryptoError::Auth)
    }

    /// Keyed hash used to deduplicate without exposing a brute-forceable digest.
    pub(crate) fn fingerprint(&self, kind: &str, content: &[u8]) -> [u8; 32] {
        let mut h = blake3::Hasher::new_keyed(&self.fingerprint_key);
        h.update(kind.as_bytes());
        h.update(&[0]);
        h.update(content);
        *h.finalize().as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sealer(byte: u8) -> Sealer {
        Sealer::new(&MasterKey::from_bytes([byte; 32]))
    }

    #[test]
    fn round_trip() {
        let s = sealer(1);
        let sealed = s.seal(b"hello", b"aad").unwrap();
        assert_eq!(&**s.open(&sealed, b"aad").unwrap(), b"hello");
    }

    #[test]
    fn ciphertext_hides_plaintext_and_differs_every_time() {
        let s = sealer(1);
        let a = s.seal(b"same text", b"x").unwrap();
        let b = s.seal(b"same text", b"x").unwrap();
        assert_ne!(a, b, "random nonce");
        assert!(!a.windows(9).any(|w| w == b"same text"));
    }

    #[test]
    fn wrong_key_wrong_aad_and_tampering_all_fail() {
        let s = sealer(1);
        let sealed = s.seal(b"data", b"row-1").unwrap();
        assert!(sealer(2).open(&sealed, b"row-1").is_err(), "wrong key");
        assert!(s.open(&sealed, b"row-2").is_err(), "swapped to another row");
        let mut t = sealed.clone();
        *t.last_mut().unwrap() ^= 1;
        assert!(s.open(&t, b"row-1").is_err(), "flipped bit");
        assert!(s.open(&sealed[..10], b"row-1").is_err(), "truncated");
    }

    #[test]
    fn fingerprints_are_keyed_and_kind_separated() {
        assert_eq!(
            sealer(1).fingerprint("text", b"a"),
            sealer(1).fingerprint("text", b"a")
        );
        assert_ne!(
            sealer(1).fingerprint("text", b"a"),
            sealer(2).fingerprint("text", b"a")
        );
        assert_ne!(
            sealer(1).fingerprint("text", b"a"),
            sealer(1).fingerprint("files", b"a")
        );
    }

    #[test]
    fn key_debug_does_not_print_bytes() {
        assert_eq!(
            format!("{:?}", MasterKey::from_bytes([7; 32])),
            "MasterKey(..)"
        );
    }
}
