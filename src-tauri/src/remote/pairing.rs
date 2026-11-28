use std::{
    collections::HashMap,
    sync::{mpsc, Mutex, OnceLock, PoisonError},
    time::{Duration, Instant},
};

use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::Sha256;
use tauri::{AppHandle, Emitter, Manager};

use super::{identity, store};
use crate::database::AppState;

const CODE_TTL: Duration = Duration::from_secs(10 * 60);
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const CODE_LENGTH: usize = 8;
/// 去掉了易混淆的 0/O、1/I/L。
const CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const LOCK_SCOPE: &str = "remote-pairing";
pub const PAIRING_REQUEST_EVENT: &str = "remote-pairing-request";
pub const PAIRING_CHANGED_EVENT: &str = "remote-pairing-changed";

struct ActiveCode {
    code: String,
    expires_at: Instant,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingCode {
    pub code: String,
    pub expires_in_secs: u64,
    pub link: String,
    pub qr_svg: String,
    pub fingerprint: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPairing {
    pub id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub fingerprint_short: String,
    pub address: String,
}

struct Pending {
    info: PendingPairing,
    reply: mpsc::Sender<Result<store::Device, String>>,
}

static CODE: OnceLock<Mutex<Option<ActiveCode>>> = OnceLock::new();
static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();

fn code_slot() -> std::sync::MutexGuard<'static, Option<ActiveCode>> {
    CODE.get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn pending() -> std::sync::MutexGuard<'static, HashMap<String, Pending>> {
    PENDING
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn random_code() -> String {
    // UUID v4 的第 6、8 字节含固定的版本位，跳过。
    uuid::Uuid::new_v4()
        .as_bytes()
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 6 && *index != 8)
        .take(CODE_LENGTH)
        .map(|(_, byte)| CODE_ALPHABET[*byte as usize % CODE_ALPHABET.len()] as char)
        .collect()
}

pub fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect()
}

pub fn create_code(host_fingerprint: &str, addresses: &[String], port: u16) -> Result<PairingCode, String> {
    let code = random_code();
    *code_slot() = Some(ActiveCode {
        code: code.clone(),
        expires_at: Instant::now() + CODE_TTL,
    });
    let address = addresses.first().cloned().unwrap_or_default();
    let link = format!(
        "multara://pair?addr={address}&port={port}&fp={host_fingerprint}&code={code}"
    );
    let qr_svg = qrcode::QrCode::new(link.as_bytes())
        .map(|qr| {
            qr.render::<qrcode::render::svg::Color>()
                .min_dimensions(180, 180)
                .build()
        })
        .map_err(|error| error.to_string())?;
    Ok(PairingCode {
        code,
        expires_in_secs: CODE_TTL.as_secs(),
        link,
        qr_svg,
        fingerprint: host_fingerprint.to_string(),
    })
}

pub fn cancel_code() {
    *code_slot() = None;
}

/// 证明双方看到的是同一对证书：中间人转发时两端看到的指纹不同，HMAC 一定对不上。
pub fn proof(code: &str, host_fingerprint: &str, client_fingerprint: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(normalize_code(code).as_bytes())
        .expect("hmac accepts any key length");
    mac.update(host_fingerprint.to_ascii_lowercase().as_bytes());
    mac.update(b":");
    mac.update(client_fingerprint.to_ascii_lowercase().as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 主机反向证明自己也知道配对码，防止手输配对码（没有二维码指纹）时被假主机冒充。
pub fn host_proof(code: &str, host_fingerprint: &str, client_fingerprint: &str) -> String {
    proof(code, &format!("host-{host_fingerprint}"), client_fingerprint)
}

/// 校验配对证明并消耗配对码（一次性），返回主机的反向证明。
pub fn verify(
    address: &str,
    host_fingerprint: &str,
    client_fingerprint: &str,
    client_proof: &str,
) -> Result<String, String> {
    let ip = address.split(':').next().unwrap_or(address);
    if crate::channel::pairing::is_locked(LOCK_SCOPE, ip) {
        return Err(tr!("配对失败次数过多，请 10 分钟后再试"));
    }
    let mut slot = code_slot();
    let matched = slot
        .as_ref()
        .filter(|active| {
            active.expires_at > Instant::now()
                && proof(&active.code, host_fingerprint, client_fingerprint)
                    .eq_ignore_ascii_case(client_proof.trim())
        })
        .map(|active| host_proof(&active.code, host_fingerprint, client_fingerprint));
    let Some(reply) = matched else {
        drop(slot);
        crate::channel::pairing::record_failure(LOCK_SCOPE, ip);
        return Err(tr!("配对码错误或已过期"));
    };
    *slot = None;
    crate::channel::pairing::clear(LOCK_SCOPE, ip);
    Ok(reply)
}

/// 配对码校验通过后：在主机界面弹出确认，阻塞等待所有者决定或超时。
pub fn await_confirmation(
    app: &AppHandle,
    device_name: &str,
    client_fingerprint: &str,
    address: &str,
) -> Result<store::Device, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let (reply, receive) = mpsc::channel();
    let info = PendingPairing {
        id: id.clone(),
        device_name: device_name.to_string(),
        fingerprint: client_fingerprint.to_string(),
        fingerprint_short: identity::short_fingerprint(client_fingerprint),
        address: address.to_string(),
    };
    pending().insert(
        id.clone(),
        Pending {
            info: info.clone(),
            reply,
        },
    );
    let _ = app.emit(PAIRING_REQUEST_EVENT, &info);
    let outcome = receive
        .recv_timeout(CONFIRM_TIMEOUT)
        .unwrap_or_else(|_| Err(tr!("主机端没有确认，配对已取消")));
    pending().remove(&id);
    let _ = app.emit(PAIRING_CHANGED_EVENT, ());
    outcome
}

pub fn list_pending() -> Vec<PendingPairing> {
    pending().values().map(|item| item.info.clone()).collect()
}

pub fn confirm(
    app: &AppHandle,
    id: &str,
    accept: bool,
    name: &str,
    owner_note: &str,
    group_id: &str,
) -> Result<Option<store::Device>, String> {
    let Some(item) = pending().remove(id) else {
        return Err(tr!("配对请求已过期"));
    };
    if !accept {
        let _ = item.reply.send(Err(tr!("主机端拒绝了配对")));
        return Ok(None);
    }
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    let group_id = if group_id.trim().is_empty() {
        store::default_group_id(&db)?
    } else {
        group_id.to_string()
    };
    if store::get_group(&db, &group_id)?.is_none() {
        let _ = item.reply.send(Err(tr!("分组不存在")));
        return Err(tr!("分组不存在"));
    }
    let name = if name.trim().is_empty() {
        item.info.device_name.as_str()
    } else {
        name.trim()
    };
    let device = store::insert_device(
        &db,
        name,
        owner_note.trim(),
        &item.info.fingerprint,
        &group_id,
        &item.info.address,
    )?;
    let _ = item.reply.send(Ok(device.clone()));
    Ok(Some(device))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_binds_both_fingerprints() {
        let good = proof("ab-cd 1234", "HOST", "client");
        assert_eq!(good, proof("ABCD1234", "host", "CLIENT"));
        assert_ne!(good, proof("ABCD1234", "mitm", "client"));
        assert_ne!(good, proof("ABCD1234", "host", "mitm"));
        assert_ne!(good, proof("ABCD1235", "host", "client"));
    }

    #[test]
    fn codes_use_unambiguous_alphabet() {
        let code = random_code();
        assert_eq!(code.len(), CODE_LENGTH);
        assert!(code.bytes().all(|byte| CODE_ALPHABET.contains(&byte)));
    }
}
