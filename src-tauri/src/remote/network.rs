use std::{
    collections::VecDeque,
    net::{IpAddr, Ipv4Addr},
    sync::{Mutex, OnceLock, PoisonError},
};

use rusqlite::Connection;
use serde::Serialize;

use super::store;

/// 存的是被关掉的网段，新出现的网卡默认放行。
const BLOCKED_NETWORKS_KEY: &str = "remoteBlockedNetworks";
const ALLOW_OTHER_KEY: &str = "remoteAllowOtherSources";
const LOG_CAPACITY: usize = 50;
pub const CONNECTIONS_CHANGED_EVENT: &str = "remote-connections-changed";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalNetwork {
    pub interface: String,
    pub address: String,
    pub cidr: String,
    pub allowed: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionAttempt {
    pub at: String,
    pub ip: String,
    /// 命中的本机网段；None 表示其他来源。
    pub cidr: Option<String>,
    pub allowed: bool,
    pub device: Option<String>,
}

static LOG: OnceLock<Mutex<VecDeque<ConnectionAttempt>>> = OnceLock::new();

fn log() -> std::sync::MutexGuard<'static, VecDeque<ConnectionAttempt>> {
    LOG.get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn network_of(ip: Ipv4Addr, mask: Ipv4Addr) -> (Ipv4Addr, u32) {
    (Ipv4Addr::from(u32::from(ip) & u32::from(mask)), u32::from(mask).count_ones())
}

fn blocked_networks(db: &Connection) -> Vec<String> {
    store::setting(db, BLOCKED_NETWORKS_KEY)
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

pub fn allow_other(db: &Connection) -> bool {
    store::setting(db, ALLOW_OTHER_KEY).as_deref() != Some("false")
}

/// 本机可用的 IPv4 网段：去掉环回、169.254 链路本地和未启用的网卡，同一网段只保留一条。
pub fn local_networks(db: &Connection) -> Vec<LocalNetwork> {
    let blocked = blocked_networks(db);
    let mut networks: Vec<LocalNetwork> = Vec::new();
    for interface in if_addrs::get_if_addrs().unwrap_or_default() {
        let if_addrs::IfAddr::V4(addr) = &interface.addr else {
            continue;
        };
        if addr.ip.is_loopback() || addr.ip.is_link_local() || !interface.is_oper_up() {
            continue;
        }
        let (network, prefix) = network_of(addr.ip, addr.netmask);
        let cidr = format!("{network}/{prefix}");
        if networks.iter().any(|item| item.cidr == cidr) {
            continue;
        }
        networks.push(LocalNetwork {
            allowed: !blocked.contains(&cidr),
            interface: interface.name.clone(),
            address: addr.ip.to_string(),
            cidr,
        });
    }
    networks
}

fn contains(cidr: &str, ip: Ipv4Addr) -> bool {
    let Some((network, prefix)) = cidr.split_once('/') else {
        return false;
    };
    let (Ok(network), Ok(prefix)) = (network.parse::<Ipv4Addr>(), prefix.parse::<u32>()) else {
        return false;
    };
    let mask = u32::MAX.checked_shl(32 - prefix.min(32)).unwrap_or(0);
    u32::from(ip) & mask == u32::from(network) & mask
}

/// 判断来源 IP 能否连接，返回 (是否放行, 命中的本机网段)。本机环回始终放行。
pub fn check(db: &Connection, ip: IpAddr) -> (bool, Option<String>) {
    let ip = match ip {
        IpAddr::V4(ip) => ip,
        IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
            Some(ip) => ip,
            None => return (allow_other(db), None),
        },
    };
    if ip.is_loopback() {
        return (true, None);
    }
    match local_networks(db).into_iter().find(|network| contains(&network.cidr, ip)) {
        Some(network) => (network.allowed, Some(network.cidr)),
        None => (allow_other(db), None),
    }
}

pub fn set_network_allowed(db: &Connection, cidr: &str, allowed: bool) -> Result<(), String> {
    let mut blocked = blocked_networks(db);
    blocked.retain(|item| item != cidr);
    if !allowed {
        blocked.push(cidr.to_string());
    }
    store::set_setting(db, BLOCKED_NETWORKS_KEY, &serde_json::to_string(&blocked).map_err(|error| error.to_string())?)
}

pub fn set_allow_other(db: &Connection, allowed: bool) -> Result<(), String> {
    store::set_setting(db, ALLOW_OTHER_KEY, if allowed { "true" } else { "false" })
}

pub fn record(ip: IpAddr, cidr: Option<String>, allowed: bool, device: Option<String>) {
    let mut log = log();
    log.push_front(ConnectionAttempt {
        at: store::now(),
        ip: ip.to_canonical().to_string(),
        cidr,
        allowed,
        device,
    });
    log.truncate(LOG_CAPACITY);
}

pub fn connection_log() -> Vec<ConnectionAttempt> {
    log().iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_contains_matches_prefix() {
        let ip = Ipv4Addr::new(192, 168, 50, 88);
        assert!(contains("192.168.50.0/24", ip));
        assert!(!contains("192.168.191.0/24", ip));
        assert!(contains("0.0.0.0/0", ip));
        assert!(!contains("garbage", ip));
    }

    #[test]
    fn network_of_masks_host_bits() {
        let (network, prefix) = network_of(Ipv4Addr::new(192, 168, 191, 2), Ipv4Addr::new(255, 255, 255, 0));
        assert_eq!(network, Ipv4Addr::new(192, 168, 191, 0));
        assert_eq!(prefix, 24);
    }
}
