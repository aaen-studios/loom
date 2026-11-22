//! What this setup carries: loom.exe, compressed with zstd and put on the end of the
//! setup's own program by `calx scripts/release.calx pack`, with a header the release
//! key signs. Nothing is installed unless that signature is the release key's and the
//! unpacked program's SHA-256 is the header's.
//!
//! The end of the file, last first:
//!
//! ```text
//! "LOOMPAK1"                       8 bytes
//! payload length                   u64, little-endian
//! header length                    u32, little-endian
//! signature                        64 bytes: Ed25519 over "loom setup\n" + header
//! header                           JSON: {"version", "sha256", "size"}
//! payload                          loom.exe, as one zstd frame
//! the setup program itself
//! ```

use std::io::Read;

pub const MAGIC: &[u8; 8] = b"LOOMPAK1";
const SIGNED_AS: &[u8] = b"loom setup\n";

pub struct Pack {
    /// The setup program alone, without what it carries: what's kept as uninstall.exe.
    pub stub: Vec<u8>,
    pub version: String,
    sha256: String,
    size: u64,
    payload: Vec<u8>,
}

/// This program's pack, or why there's none. A setup built from source without
/// `release.calx pack` carries nothing.
pub fn read_own() -> Result<Pack, String> {
    let exe = std::env::current_exe().map_err(|e| format!("can't find this program: {e}"))?;
    let data = std::fs::read(&exe).map_err(|e| format!("can't read this program: {e}"))?;
    read(data, key())
}

/// The release key, from shared/release.calx (build.rs).
pub fn key() -> [u8; 32] {
    let hex = env!("LOOM_RELEASE_KEY_HEX");
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0);
    }
    out
}

pub fn read(mut data: Vec<u8>, key: [u8; 32]) -> Result<Pack, String> {
    let none =
        || String::from("this setup carries no loom (it was built without `release.calx pack`)");
    let n = data.len();
    if n < 20 + 64 + 8 || &data[n - 8..] != MAGIC {
        return Err(none());
    }
    let payload_len = u64::from_le_bytes(data[n - 16..n - 8].try_into().unwrap()) as usize;
    let header_len = u32::from_le_bytes(data[n - 20..n - 16].try_into().unwrap()) as usize;
    let sig_at = n - 20 - 64;
    let header_at = sig_at.checked_sub(header_len).ok_or_else(none)?;
    let payload_at = header_at.checked_sub(payload_len).ok_or_else(none)?;
    let header = data[header_at..sig_at].to_vec();
    let sig: [u8; 64] = data[sig_at..sig_at + 64].try_into().unwrap();

    let mut signed = SIGNED_AS.to_vec();
    signed.extend_from_slice(&header);
    let vk = ed25519_dalek::VerifyingKey::from_bytes(&key)
        .map_err(|_| String::from("the release key built into this setup isn't a key"))?;
    vk.verify_strict(&signed, &ed25519_dalek::Signature::from_bytes(&sig))
        .map_err(|_| String::from("what this setup carries isn't signed with loom's release key, so it won't be installed"))?;

    let text = String::from_utf8_lossy(&header).into_owned();
    let version = json_str(&text, "version").ok_or("the setup's header has no version")?;
    let sha256 = json_str(&text, "sha256").ok_or("the setup's header has no hash")?;
    let size = json_num(&text, "size").ok_or("the setup's header has no size")?;
    let payload = data[payload_at..header_at].to_vec();
    // What's left is the setup's own program, a megabyte or so: let the rest go.
    data.truncate(payload_at);
    data.shrink_to_fit();
    Ok(Pack {
        stub: data,
        version,
        sha256,
        size,
        payload,
    })
}

impl Pack {
    /// loom.exe, unpacked and checked against the signed header.
    pub fn unpack(&self, progress: &dyn Fn(f32)) -> Result<Vec<u8>, String> {
        let mut src: &[u8] = &self.payload;
        let mut dec = ruzstd::decoding::StreamingDecoder::new(&mut src)
            .map_err(|e| format!("can't unpack loom: {e}"))?;
        let mut out = Vec::with_capacity(self.size as usize);
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let got = dec
                .read(&mut buf)
                .map_err(|e| format!("can't unpack loom: {e}"))?;
            if got == 0 {
                break;
            }
            out.extend_from_slice(&buf[..got]);
            if out.len() as u64 > self.size {
                return Err("loom unpacked bigger than the setup says it is".into());
            }
            progress(out.len() as f32 / self.size.max(1) as f32);
        }
        if out.len() as u64 != self.size || hex(&sha256(&out)) != self.sha256 {
            return Err("the unpacked loom isn't the one the setup's signature names".into());
        }
        Ok(out)
    }
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(data).into()
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A string field of a flat JSON object, as release.calx writes the header: no escapes
/// beyond what a version or a hash could hold.
fn json_str(text: &str, name: &str) -> Option<String> {
    let at = text.find(&format!("\"{name}\""))? + name.len() + 2;
    let rest = text[at..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

fn json_num(text: &str, name: &str) -> Option<u64> {
    let at = text.find(&format!("\"{name}\""))? + name.len() + 2;
    let rest = text[at..].trim_start().strip_prefix(':')?.trim_start();
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_flat_header() {
        let h = r#"{"version":"0.2.0","sha256":"ab12","size":42}"#;
        assert_eq!(json_str(h, "version").as_deref(), Some("0.2.0"));
        assert_eq!(json_str(h, "sha256").as_deref(), Some("ab12"));
        assert_eq!(json_num(h, "size"), Some(42));
        assert_eq!(json_str(h, "notes"), None);
    }

    #[test]
    fn refuses_a_setup_without_a_pack() {
        assert!(read(b"MZ just a program".to_vec(), key()).is_err());
    }

    #[test]
    fn refuses_a_pack_signed_by_another_key() {
        let header = br#"{"version":"9.9.9","sha256":"00","size":1}"#;
        let mut data = b"MZ stub".to_vec();
        data.extend_from_slice(b"payload");
        data.extend_from_slice(header);
        data.extend_from_slice(&[7u8; 64]);
        data.extend_from_slice(&(header.len() as u32).to_le_bytes());
        data.extend_from_slice(&7u64.to_le_bytes());
        data.extend_from_slice(MAGIC);
        let err = read(data, key()).err().unwrap();
        assert!(err.contains("isn't signed"), "{err}");
    }
}
