//! Signed distribution metadata. A release page alone never authorizes code execution.
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::signature::{ED25519, UnparsedPublicKey};
use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub schema: u32,
    pub version: String,
    pub windows_installer: Asset,
    pub windows_portable: Asset,
    pub windows_binary_sha256: String,
    pub flatpak_commit: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub payload: String,
    pub signature: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    pub repository: String,
    pub public_key: String,
}
impl Channel {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../packaging/updates/channel.json"))
            .expect("Invalid bundled update channel")
    }
    pub fn manifest_url(&self) -> String {
        format!(
            "https://github.com/{}/releases/latest/download/folio-update.json",
            self.repository
        )
    }
}
fn checksum(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
impl Release {
    pub fn newer_than(&self, current: &str) -> Result<bool, String> {
        let version = Version::parse(&self.version).map_err(|e| e.to_string())?;
        let current = Version::parse(current).map_err(|e| e.to_string())?;
        Ok(version.pre.is_empty() && version > current)
    }
    fn validate(&self, channel: &Channel) -> Result<(), String> {
        if self.schema != 1
            || !checksum(&self.windows_binary_sha256)
            || !checksum(&self.flatpak_commit)
        {
            return Err("Unsupported or invalid update manifest".into());
        }
        let version = Version::parse(&self.version).map_err(|e| e.to_string())?;
        if !version.pre.is_empty() || !version.build.is_empty() {
            return Err("Only stable releases can be installed".into());
        }
        for (asset, suffix) in [
            (&self.windows_installer, "windows-x64-setup.exe"),
            (&self.windows_portable, "windows-x64.zip"),
        ] {
            let expected = format!(
                "https://github.com/{}/releases/download/v{}/folio-{}-{suffix}",
                channel.repository, self.version, self.version
            );
            if asset.url != expected
                || !checksum(&asset.sha256)
                || !(1024..=2_000_000_000).contains(&asset.size)
            {
                return Err("Invalid update asset".into());
            }
        }
        Ok(())
    }
}
pub fn verify(bytes: &[u8], channel: &Channel) -> Result<Release, String> {
    if bytes.len() > 64 * 1024 {
        return Err("Update manifest is too large".into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let payload = STANDARD
        .decode(envelope.payload)
        .map_err(|e| e.to_string())?;
    let signature = STANDARD
        .decode(envelope.signature)
        .map_err(|e| e.to_string())?;
    let key = STANDARD
        .decode(&channel.public_key)
        .map_err(|e| e.to_string())?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(&payload, &signature)
        .map_err(|_| "Release signature could not be verified")?;
    let release: Release = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
    release.validate(channel)?;
    Ok(release)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::{
        rand::SystemRandom,
        signature::{Ed25519KeyPair, KeyPair},
    };
    fn signed() -> (Vec<u8>, Channel) {
        let key = Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        let channel = Channel {
            repository: "example/Folio".into(),
            public_key: STANDARD.encode(key.public_key().as_ref()),
        };
        let release = fixture(&channel, "0.1.10");
        let payload = serde_json::to_vec(&release).unwrap();
        let signature = key.sign(&payload);
        (
            serde_json::to_vec(&Envelope {
                payload: STANDARD.encode(payload),
                signature: STANDARD.encode(signature.as_ref()),
            })
            .unwrap(),
            channel,
        )
    }
    pub fn fixture(channel: &Channel, version: &str) -> Release {
        let asset = |suffix| Asset {
            url: format!(
                "https://github.com/{}/releases/download/v{version}/folio-{version}-{suffix}",
                channel.repository
            ),
            sha256: "a".repeat(64),
            size: 4096,
        };
        Release {
            schema: 1,
            version: version.into(),
            windows_installer: asset("windows-x64-setup.exe"),
            windows_portable: asset("windows-x64.zip"),
            windows_binary_sha256: "b".repeat(64),
            flatpak_commit: "c".repeat(64),
        }
    }
    #[test]
    fn verifies_signature_and_compares_numeric_versions() {
        let (bytes, channel) = signed();
        let release = verify(&bytes, &channel).unwrap();
        assert!(release.newer_than("0.1.9").unwrap());
        assert!(!release.newer_than("0.1.10").unwrap());
        assert!(!release.newer_than("0.2.0").unwrap());
    }
    #[test]
    fn rejects_tampering_wrong_keys_and_unbounded_metadata() {
        let (bytes, channel) = signed();
        let mut envelope: Envelope = serde_json::from_slice(&bytes).unwrap();
        let payload = STANDARD.decode(&envelope.payload).unwrap();
        envelope.payload = STANDARD.encode(
            String::from_utf8(payload)
                .unwrap()
                .replace("0.1.10", "9.0.0"),
        );
        assert!(verify(&serde_json::to_vec(&envelope).unwrap(), &channel).is_err());
        let wrong = Channel {
            public_key: STANDARD.encode([0; 32]),
            ..channel.clone()
        };
        assert!(verify(&bytes, &wrong).is_err());
        assert!(verify(&vec![b' '; 65537], &channel).is_err());
    }
    #[test]
    fn rejects_prerelease_wrong_hosts_missing_checksums_and_unknown_schema() {
        let (_, channel) = signed();
        let mut release = fixture(&channel, "0.1.10");
        release.windows_installer.url.push_str("?redirect=bad");
        assert!(release.validate(&channel).is_err());
        release = fixture(&channel, "0.1.10-beta.1");
        assert!(release.validate(&channel).is_err());
        release = fixture(&channel, "0.1.10");
        release.schema = 2;
        assert!(release.validate(&channel).is_err());
        release.schema = 1;
        release.windows_portable.sha256.clear();
        assert!(release.validate(&channel).is_err());
    }
}
