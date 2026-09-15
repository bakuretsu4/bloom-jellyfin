//! Finding Jellyfin servers on the local network, for the sign-in screen.
//!
//! Jellyfin answers "Who is JellyfinServer?" sent to UDP port 7359 with a little JSON: its id,
//! its name and the address it reckons it's reachable at. Bloom broadcasts that once and
//! listens for a moment. It finds servers on the same network as the computer, which is how most
//! people run one; a server in Docker without that port published, or behind another router,
//! doesn't answer, and its address is typed as before.
//!
//! A broadcast alone isn't enough on a computer with a firewall on (some desktop distributions
//! turn ufw on by default): a server's answer comes from its own address, which the firewall doesn't
//! match to a message sent to the broadcast address, so it drops the answer. The question is
//! also asked directly of each address on the computer's home networks (at most a /24 around
//! each), whose answers the firewall does let back in.
//!
//! An answer is only a suggestion: choosing it runs the same check as a typed address.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

const PORT: u16 = 7359;
const QUESTION: &[u8] = b"Who is JellyfinServer?";
/// Long enough for a busy server to answer, short enough that the list doesn't keep people waiting.
const LISTEN: Duration = Duration::from_millis(2500);

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct AnswerDto {
    address: Option<String>,
    id: Option<String>,
    name: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredServer {
    id: String,
    name: String,
    /// Where to reach it, for the sign-in screen's address check.
    address: String,
}

/// A stated address this computer can't use: the server's own loopback, or no host at all.
fn unusable(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        None => true,
    }
}

/// A server's answer, as a server Bloom can try. The address is the one it states, unless that's
/// one only it can use; then it's the address the answer came from, on the stated port.
fn read_answer(bytes: &[u8], from: SocketAddr) -> Option<DiscoveredServer> {
    let dto: AnswerDto = serde_json::from_slice(bytes).ok()?;
    let id = dto.id.filter(|id| !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))?;
    let stated = dto
        .address
        .and_then(|a| url::Url::parse(a.trim()).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"));
    let address = match stated {
        Some(url) if !unusable(&url) => url.as_str().trim_end_matches('/').to_string(),
        other => {
            let scheme = other.as_ref().map_or("http", |url| url.scheme()).to_string();
            let port = other.and_then(|url| url.port_or_known_default()).unwrap_or(8096);
            let host = match from.ip() {
                IpAddr::V4(ip) => ip.to_string(),
                IpAddr::V6(ip) => format!("[{ip}]"),
            };
            format!("{scheme}://{host}:{port}")
        }
    };
    let name = dto
        .name
        .map(|name| name.trim().chars().take(80).collect::<String>())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| from.ip().to_string());
    Some(DiscoveredServer { id, name, address })
}

/// Asks at `targets` and lists the servers that answer within `listen`, each once, by name.
pub(crate) async fn discover_on(targets: &[SocketAddr], listen: Duration) -> Vec<DiscoveredServer> {
    let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await else { return Vec::new() };
    let _ = socket.set_broadcast(true);
    for target in targets {
        let _ = socket.send_to(QUESTION, target).await;
    }
    let deadline = Instant::now() + listen;
    let mut found: Vec<DiscoveredServer> = Vec::new();
    let mut buffer = [0u8; 4096];
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match tokio::time::timeout(left, socket.recv_from(&mut buffer)).await {
            Ok(Ok((length, from))) => {
                if let Some(server) = read_answer(&buffer[..length], from) {
                    // By id and address: anything on the network can answer with a server's id, and
                    // keeping only the first answer would let it hide the real one.
                    if !found.iter().any(|f| f.id == server.id && f.address == server.address) {
                        found.push(server);
                    }
                }
            }
            // A stray error (such as an unreachable port for a direct ask): keep listening.
            Ok(Err(_)) => tokio::time::sleep(Duration::from_millis(50)).await,
            Err(_) => break,
        }
    }
    found.sort_by_key(|server| server.name.to_lowercase());
    found
}

/// Interface names for containers and virtual machines, whose networks have no Jellyfin to find.
const VIRTUAL: [&str; 6] = ["docker", "br-", "veth", "virbr", "vmnet", "podman"];

/// This computer's IPv4 addresses on home networks, with their prefix lengths.
#[cfg(unix)]
fn home_networks() -> Vec<(Ipv4Addr, u8)> {
    let mut found = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `list` with a linked list that stays valid until freeifaddrs;
    // every pointer is checked for null before it's read, and sockaddr_in is read only for AF_INET.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            return found;
        }
        let mut entry = list;
        while !entry.is_null() {
            let ifa = &*entry;
            entry = ifa.ifa_next;
            let flags = ifa.ifa_flags as libc::c_int;
            if ifa.ifa_addr.is_null() || ifa.ifa_netmask.is_null() || ifa.ifa_name.is_null() {
                continue;
            }
            if (*ifa.ifa_addr).sa_family as libc::c_int != libc::AF_INET
                || flags & libc::IFF_UP == 0
                || flags & libc::IFF_LOOPBACK != 0
            {
                continue;
            }
            let name = std::ffi::CStr::from_ptr(ifa.ifa_name).to_string_lossy();
            if VIRTUAL.iter().any(|prefix| name.starts_with(prefix)) {
                continue;
            }
            let address = Ipv4Addr::from(u32::from_be((*(ifa.ifa_addr as *const libc::sockaddr_in)).sin_addr.s_addr));
            let mask = u32::from_be((*(ifa.ifa_netmask as *const libc::sockaddr_in)).sin_addr.s_addr);
            if address.is_private() {
                found.push((address, mask.count_ones() as u8));
            }
        }
        libc::freeifaddrs(list);
    }
    found
}

#[cfg(not(unix))]
fn home_networks() -> Vec<(Ipv4Addr, u8)> {
    Vec::new()
}

/// Where to ask on a network: its broadcast address, then every other address in it, keeping to
/// the /24 around `own` on a wider network so a big one isn't swept.
fn network_targets(own: Ipv4Addr, prefix: u8) -> Vec<Ipv4Addr> {
    let prefix = prefix.clamp(24, 30);
    let mask = u32::MAX << (32 - prefix);
    let network = u32::from(own) & mask;
    let broadcast = network | !mask;
    let mut targets = vec![Ipv4Addr::from(broadcast)];
    targets.extend((network + 1..broadcast).map(Ipv4Addr::from).filter(|host| *host != own));
    targets
}

/// The Jellyfin servers on this network that answer.
#[tauri::command]
pub async fn discover_servers() -> Vec<DiscoveredServer> {
    let mut targets = vec![Ipv4Addr::BROADCAST];
    for (own, prefix) in home_networks() {
        for target in network_targets(own, prefix) {
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
    }
    let targets: Vec<SocketAddr> = targets.into_iter().map(|ip| SocketAddr::from((ip, PORT))).collect();
    discover_on(&targets, LISTEN).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from(ip: &str) -> SocketAddr {
        SocketAddr::new(ip.parse().unwrap(), PORT)
    }

    #[test]
    fn answers_become_servers_to_try() {
        let server = read_answer(br#"{"Address":"http://192.168.1.20:8096/","Id":"abc123","Name":" Living room "}"#, from("192.168.1.20")).unwrap();
        assert_eq!((server.name.as_str(), server.address.as_str()), ("Living room", "http://192.168.1.20:8096"));

        // A server stating an address only it can use is tried where the answer came from.
        let loopback = read_answer(br#"{"Address":"http://127.0.0.1:8920","Id":"abc123","Name":"NAS"}"#, from("192.168.1.30")).unwrap();
        assert_eq!(loopback.address, "http://192.168.1.30:8920");
        let unstated = read_answer(br#"{"Id":"abc123"}"#, from("192.168.1.30")).unwrap();
        assert_eq!((unstated.name.as_str(), unstated.address.as_str()), ("192.168.1.30", "http://192.168.1.30:8096"));

        assert!(read_answer(br#"{"Address":"http://x","Id":"../../etc","Name":"x"}"#, from("192.168.1.30")).is_none());
        assert!(read_answer(b"not json", from("192.168.1.30")).is_none());
        let javascript = read_answer(br#"{"Address":"javascript:alert(1)","Id":"abc"}"#, from("10.0.0.5")).unwrap();
        assert_eq!(javascript.address, "http://10.0.0.5:8096");
    }

    /// Asks this computer's real network; run with `--ignored` when a server is there to answer.
    #[test]
    #[ignore]
    fn finds_servers_on_this_network() {
        let found = tauri::async_runtime::block_on(discover_servers());
        println!("{found:#?}");
        assert!(!found.is_empty(), "no server answered");
    }

    #[test]
    fn a_home_network_is_asked_address_by_address() {
        let own: Ipv4Addr = "192.168.1.37".parse().unwrap();
        let targets = network_targets(own, 24);
        assert_eq!(targets[0], "192.168.1.255".parse::<Ipv4Addr>().unwrap());
        assert_eq!(targets.len(), 1 + 253, "every host but this one");
        assert!(!targets.contains(&own));
        assert!(!targets.contains(&"192.168.1.0".parse().unwrap()));
        // A wide network is only swept around this computer.
        let wide = network_targets("10.1.2.3".parse().unwrap(), 8);
        assert_eq!((wide.len(), wide[0]), (254, "10.1.2.255".parse().unwrap()));
    }

    #[test]
    fn a_server_that_answers_is_listed_once() {
        tauri::async_runtime::block_on(async {
            // A pretend Jellyfin on this computer, answering twice as a server might.
            let responder = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let at = responder.local_addr().unwrap();
            let answering = tauri::async_runtime::spawn(async move {
                let mut buffer = [0u8; 256];
                let (length, asker) = responder.recv_from(&mut buffer).await.unwrap();
                assert_eq!(&buffer[..length], QUESTION);
                let answer = br#"{"Address":"http://192.168.1.20:8096","Id":"abc123","Name":"Living room","EndpointAddress":null}"#;
                responder.send_to(answer, asker).await.unwrap();
                responder.send_to(answer, asker).await.unwrap();
                // Something else claiming the same id at another address doesn't hide it.
                let claim = br#"{"Address":"http://192.168.1.66:8096","Id":"abc123","Name":"Living room"}"#;
                responder.send_to(claim, asker).await.unwrap();
            });
            let found = discover_on(&[at], Duration::from_millis(700)).await;
            let _ = answering.await;
            let addresses: Vec<&str> = found.iter().map(|f| f.address.as_str()).collect();
            assert_eq!(addresses.len(), 2, "the same server was listed twice, or an answer went missing: {addresses:?}");
            assert!(addresses.contains(&"http://192.168.1.20:8096") && addresses.contains(&"http://192.168.1.66:8096"));
        });
    }
}
