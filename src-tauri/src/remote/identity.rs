use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};

/// 本机设备身份：自签名证书 + 私钥。主机与控制端共用。
#[derive(Clone)]
pub struct Identity {
    pub cert: CertificateDer<'static>,
    pub key: Arc<Vec<u8>>,
    pub fingerprint: String,
}

impl Identity {
    pub fn private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key.as_ref().clone()))
    }
}

static IDENTITY: OnceLock<Identity> = OnceLock::new();

pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn short_fingerprint(fingerprint: &str) -> String {
    fingerprint
        .get(fingerprint.len().saturating_sub(8)..)
        .unwrap_or(fingerprint)
        .to_uppercase()
}

fn dir(app_data: &Path) -> PathBuf {
    app_data.join("remote")
}

pub fn load_or_create(app_data: &Path) -> Result<Identity, String> {
    if let Some(identity) = IDENTITY.get() {
        return Ok(identity.clone());
    }
    let dir = dir(app_data);
    let cert_path = dir.join("identity.crt.der");
    let key_path = dir.join("identity.key.der");
    let (cert, key) = match (fs::read(&cert_path), fs::read(&key_path)) {
        (Ok(cert), Ok(key)) if !cert.is_empty() && !key.is_empty() => (cert, key),
        _ => {
            fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
            let generated = rcgen::generate_simple_self_signed(vec!["multara.local".to_string()])
                .map_err(|error| error.to_string())?;
            let cert = generated.cert.der().to_vec();
            let key = generated.key_pair.serialize_der();
            fs::write(&key_path, &key).map_err(|error| error.to_string())?;
            fs::write(&cert_path, &cert).map_err(|error| error.to_string())?;
            restrict_permissions(&key_path);
            (cert, key)
        }
    };
    let identity = Identity {
        fingerprint: fingerprint(&cert),
        cert: CertificateDer::from(cert),
        key: Arc::new(key),
    };
    Ok(IDENTITY.get_or_init(|| identity).clone())
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

/// Windows 上应用数据目录本身只有当前用户可写，这里不额外改 ACL。
#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}
