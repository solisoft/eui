//! The signature an APK must carry before Android will install it: APK
//! Signature Scheme v2, with an ECDSA P-256 key.
//!
//! v2 alone is enough for every device the client runs on — the package
//! asks for API 26 and v2 is read from 24 — and it is the scheme Android
//! wants from anything that targets 30 or later. So there is no JAR
//! signature (`META-INF/*.SF`) here at all, and the one in a template is
//! dropped on the way through: it covered the template's bytes, not these.
//!
//! The key is the person's, made the first time and kept: Android only lets
//! a package upgrade another one signed by the same certificate, so a key
//! made afresh on each run would be a package that has to be uninstalled
//! before every update. It lives in two files, `key.pk8` (PKCS#8) and
//! `cert.der` (a self-signed X.509 certificate for it), which is what
//! `apksigner --key … --cert …` takes too — so a package signed here can be
//! re-signed elsewhere, and a key made elsewhere can be used here.
//!
//! The certificate is not checked against anyone: Android compares it with
//! the one the installed package has, and that is the whole of what it is
//! for. Its dates are fixed — from 2000 to the end of 9999 — so the same
//! key always yields the same certificate.

use std::path::Path;

use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};

/// `SIGNATURE_ECDSA_WITH_SHA256` in the v2 scheme.
const ECDSA_SHA256: u32 = 0x0201;
/// The v2 block's id in the signing block.
const V2_BLOCK_ID: u32 = 0x7109_871a;
/// The signing block's trailing magic.
const MAGIC: &[u8; 16] = b"APK Sig Block 42";
/// The chunk the content digest is taken over.
const CHUNK: usize = 1024 * 1024;

/// A signing key and its certificate.
pub struct Key {
    pair: EcdsaKeyPair,
    cert: Vec<u8>,
}

impl Key {
    /// The key in `dir`, made there first if there is none. Returns whether
    /// it was made, so the caller can say so — a new key is a package that
    /// will not upgrade one signed with an old one.
    pub fn load_or_create(dir: &Path) -> Result<(Self, bool), String> {
        let key_path = dir.join("key.pk8");
        let cert_path = dir.join("cert.der");
        match (key_path.exists(), cert_path.exists()) {
            (true, true) => {
                let pkcs8 = std::fs::read(&key_path).map_err(|e| format!("{}: {e}", key_path.display()))?;
                let cert = std::fs::read(&cert_path).map_err(|e| format!("{}: {e}", cert_path.display()))?;
                let key = Self::from_parts(&pkcs8, cert).map_err(|e| format!("{}: {e}", dir.display()))?;
                Ok((key, false))
            }
            (false, false) => {
                let rng = SystemRandom::new();
                let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).map_err(|_| "could not make a signing key")?;
                let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng).map_err(|_| "could not read the key just made")?;
                let cert = certificate(&pair, &rng)?;
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                write_private(&key_path, pkcs8.as_ref())?;
                std::fs::write(&cert_path, &cert).map_err(|e| format!("{}: {e}", cert_path.display()))?;
                Ok((Self { pair, cert }, true))
            }
            _ => Err(format!("{}: has one of key.pk8 and cert.der and not the other; a certificate is for one key, so both or neither", dir.display())),
        }
    }

    /// A key from its PKCS#8 bytes and its certificate, which must be for it.
    pub fn from_parts(pkcs8: &[u8], cert: Vec<u8>) -> Result<Self, String> {
        let rng = SystemRandom::new();
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8, &rng).map_err(|_| "key.pk8 is not an ECDSA P-256 key in PKCS#8")?;
        let spki = spki(pair.public_key().as_ref());
        if !cert.windows(spki.len()).any(|w| w == spki.as_slice()) {
            return Err("cert.der is not a certificate for key.pk8".into());
        }
        Ok(Self { pair, cert })
    }

    /// The certificate's SHA-256, as `apksigner verify --print-certs` prints it.
    pub fn fingerprint(&self) -> String {
        ring::digest::digest(&ring::digest::SHA256, &self.cert).as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(bytes).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

// ------------------------------------------------------------------ DER

fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let n = content.len();
    let mut out = vec![tag];
    if n < 0x80 {
        out.push(n as u8);
    } else {
        let bytes: Vec<u8> = n.to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
        out.push(0x80 | bytes.len() as u8);
        out.extend_from_slice(&bytes);
    }
    out.extend_from_slice(content);
    out
}

fn seq(parts: &[&[u8]]) -> Vec<u8> {
    der(0x30, &parts.concat())
}

/// `ecdsa-with-SHA256`, as an AlgorithmIdentifier.
const ECDSA_WITH_SHA256: &[u8] = &[0x30, 0x0a, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02];

/// SubjectPublicKeyInfo for an uncompressed P-256 point.
fn spki(point: &[u8]) -> Vec<u8> {
    // id-ecPublicKey, prime256v1.
    let alg = [0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
    let bits = der(0x03, &[&[0u8][..], point].concat());
    seq(&[&alg, &bits])
}

/// A self-signed X.509 v3 certificate for `pair`.
fn certificate(pair: &EcdsaKeyPair, rng: &SystemRandom) -> Result<Vec<u8>, String> {
    let mut serial = [0u8; 8];
    rng.fill(&mut serial).map_err(|_| "no randomness for a serial number")?;
    // Positive, and minimal: the top bit clear and the first byte not zero.
    serial[0] = (serial[0] & 0x7f) | 0x40;
    let version = der(0xa0, &der(0x02, &[2]));
    let name = seq(&[&der(0x31, &seq(&[&der(0x06, &[0x55, 0x04, 0x03]), &der(0x0c, b"EUI package signing")]))]);
    let validity = seq(&[&der(0x17, b"000101000000Z"), &der(0x18, b"99991231235959Z")]);
    let tbs = seq(&[&version, &der(0x02, &serial), ECDSA_WITH_SHA256, &name, &validity, &name, &spki(pair.public_key().as_ref())]);
    let sig = pair.sign(rng, &tbs).map_err(|_| "could not sign the certificate")?;
    let bits = der(0x03, &[&[0u8][..], sig.as_ref()].concat());
    Ok(seq(&[&tbs, ECDSA_WITH_SHA256, &bits]))
}

// ------------------------------------------------------------- v2 scheme

/// `bytes`, prefixed with its length as a little-endian u32.
fn lp(bytes: &[u8]) -> Vec<u8> {
    let mut out = (bytes.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(bytes);
    out
}

/// The v2 content digest: every section in 1 MiB chunks, each chunk's
/// SHA-256 taken with its length in front, and the SHA-256 of all of those.
fn content_digest(sections: &[&[u8]]) -> Vec<u8> {
    use ring::digest::{Context, SHA256};
    let mut chunks: Vec<u8> = Vec::new();
    let mut count: u32 = 0;
    for section in sections {
        for chunk in section.chunks(CHUNK) {
            let mut c = Context::new(&SHA256);
            c.update(&[0xa5]);
            c.update(&(chunk.len() as u32).to_le_bytes());
            c.update(chunk);
            chunks.extend_from_slice(c.finish().as_ref());
            count += 1;
        }
    }
    let mut top = Context::new(&SHA256);
    top.update(&[0x5a]);
    top.update(&count.to_le_bytes());
    top.update(&chunks);
    top.finish().as_ref().to_vec()
}

/// Sign the archive `zip`, whose central directory starts at `cd_offset`
/// and whose end record is its last 22 bytes (it has no comment): the
/// signing block goes between the entries and the central directory, and
/// the end record is told the directory moved.
pub fn sign(zip: &[u8], cd_offset: usize, key: &Key) -> Result<Vec<u8>, String> {
    let eocd_at = zip.len().checked_sub(22).ok_or("the archive is too short")?;
    let entries = zip.get(..cd_offset).ok_or("the central directory is past the end")?;
    let cd = zip.get(cd_offset..eocd_at).ok_or("the central directory is past the end")?;
    let eocd = zip.get(eocd_at..).ok_or("the archive is too short")?;
    if eocd.get(..4) != Some(&[0x50, 0x4b, 0x05, 0x06][..]) {
        return Err("the archive's last 22 bytes are not its end record".into());
    }

    let digest = content_digest(&[entries, cd, eocd]);
    let digests = lp(&lp(&[&ECDSA_SHA256.to_le_bytes()[..], &lp(&digest)].concat()));
    let certificates = lp(&lp(&key.cert));
    let attributes = lp(&[]);
    let signed_data = [digests, certificates, attributes].concat();
    let rng = SystemRandom::new();
    let signature = key.pair.sign(&rng, &signed_data).map_err(|_| "could not sign the package")?;
    let signatures = lp(&lp(&[&ECDSA_SHA256.to_le_bytes()[..], &lp(signature.as_ref())].concat()));
    let public_key = lp(&spki(key.pair.public_key().as_ref()));
    let signer = [lp(&signed_data), signatures, public_key].concat();
    let v2 = lp(&lp(&signer));

    // The signing block: its size, one id–value pair, its size again, magic.
    let pair_len = (4 + v2.len()) as u64;
    let block_size = 8 + pair_len + 8 + 16;
    let mut block = Vec::with_capacity(block_size as usize + 8);
    block.extend_from_slice(&block_size.to_le_bytes());
    block.extend_from_slice(&pair_len.to_le_bytes());
    block.extend_from_slice(&V2_BLOCK_ID.to_le_bytes());
    block.extend_from_slice(&v2);
    block.extend_from_slice(&block_size.to_le_bytes());
    block.extend_from_slice(MAGIC);

    let mut out = Vec::with_capacity(zip.len() + block.len());
    out.extend_from_slice(entries);
    out.extend_from_slice(&block);
    out.extend_from_slice(cd);
    let mut end = eocd.to_vec();
    let moved = u32::try_from(cd_offset + block.len()).map_err(|_| "the package is larger than a zip can say")?;
    end.get_mut(16..20).ok_or("the end record is short")?.copy_from_slice(&moved.to_le_bytes());
    out.extend_from_slice(&end);
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;
    use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_ASN1};

    fn u32_at(b: &[u8], at: usize) -> usize {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
    }

    /// Read a signed package back the way a verifier does: find the block
    /// from the end record, recompute the digest over the three sections
    /// with the directory offset put back, and check the signature.
    #[test]
    fn a_signed_archive_verifies_as_the_scheme_says() {
        let dir = std::env::temp_dir().join(format!("eui-package-sign-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (key, made) = Key::load_or_create(&dir).unwrap();
        assert!(made);
        let (again, made) = Key::load_or_create(&dir).unwrap();
        assert!(!made);
        assert_eq!(key.fingerprint(), again.fingerprint());

        let entries = vec![crate::zip::Entry::stored("a", vec![1u8; 3 * CHUNK + 5], 0), crate::zip::Entry::stored("b", b"two".to_vec(), 0)];
        let w = crate::zip::write(&entries, |_| 4);
        let signed = sign(&w.bytes, w.cd_offset, &key).unwrap();

        let eocd = signed.len() - 22;
        let cd_at = u32_at(&signed, eocd + 16);
        assert_eq!(&signed[cd_at - 16..cd_at], MAGIC);
        let size = u64::from_le_bytes(signed[cd_at - 24..cd_at - 16].try_into().unwrap()) as usize;
        let block_at = cd_at - size - 8;
        assert_eq!(block_at, w.cd_offset);
        let pair_len = u64::from_le_bytes(signed[block_at + 8..block_at + 16].try_into().unwrap()) as usize;
        assert_eq!(u32_at(&signed, block_at + 16) as u32, V2_BLOCK_ID);
        let v2 = &signed[block_at + 20..block_at + 16 + pair_len];

        // signers → signer → signed data, signatures, public key.
        let signer = &v2[8..8 + u32_at(v2, 4)];
        let sd_len = u32_at(signer, 0);
        let signed_data = &signer[4..4 + sd_len];
        let sigs = &signer[4 + sd_len..];
        let sigs_len = u32_at(sigs, 0);
        let first = &sigs[8..8 + u32_at(sigs, 4)];
        assert_eq!(u32_at(first, 0) as u32, ECDSA_SHA256);
        let sig = &first[8..8 + u32_at(first, 4)];
        let pk = &sigs[4 + sigs_len..];
        let spki_bytes = &pk[4..4 + u32_at(pk, 0)];
        let point = &spki_bytes[spki_bytes.len() - 65..];
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, point).verify(signed_data, sig).unwrap();

        // The digest inside is the digest of the archive as it was.
        let digest = &signed_data[4 + 4 + 4 + 4..4 + 4 + 4 + 4 + 32];
        let mut end = signed[eocd..].to_vec();
        end[16..20].copy_from_slice(&(block_at as u32).to_le_bytes());
        assert_eq!(digest, content_digest(&[&signed[..block_at], &signed[cd_at..eocd], &end]).as_slice());
        // And the archive still reads.
        assert_eq!(crate::zip::read(&signed).unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
