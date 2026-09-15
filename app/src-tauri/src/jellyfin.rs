//! Talking to a Jellyfin server: checking an address, signing in, and signing out.
//!
//! Every request is made from here, never from the page, so the access token stays out of
//! page JavaScript. The page only ever sees the server and the user's name and id.
//!
//! On disk, in the app data folder:
//!   server.json    the last address that answered as a Jellyfin server (not secret)
//!   servers.json   every server added, and when each last answered (not secret)
//!   device-id      a random id, so the server sees one device across restarts
//!   accounts.json  the saved sign-ins (server, user and access token) and which is in use;
//!                  owner-only (0600). A sign-in is only saved when "Stay signed in" is on,
//!                  and the password is never stored anywhere. Earlier versions kept one
//!                  session.json, which is folded in and removed on first read.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::State;
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Enter the address of your Jellyfin server.")]
    EmptyAddress,
    #[error("That doesn't look like a server address.")]
    BadAddress,
    #[error("Couldn't reach {0}. Check the address and that the server is running.")]
    Unreachable(String),
    #[error("{0} answered, but it isn't a Jellyfin server.")]
    NotJellyfin(String),
    #[error("Wrong username or password.")]
    BadCredentials,
    #[error("Quick Connect is turned off on this server.")]
    QuickConnectOff,
    #[error("That code expired. Get a new one to try again.")]
    QuickConnectExpired,
    #[error("Choose a server first.")]
    NoServer,
    #[error("The server refused the request ({0}).")]
    Status(u16),
    #[error("The server sent a reply Bloom couldn't read.")]
    Unreadable,
    #[error("Couldn't save your sign-in: {0}")]
    Storage(String),
    #[error("Your sign-in has ended. Sign in again to carry on.")]
    SignedOut,
    #[error("The sign-in for {0} has ended. Sign in again to use that account.")]
    AccountEnded(String),
    #[error("The server won't play this here ({0}).")]
    NotPlayable(String),
    #[error("This show has no episodes to play.")]
    NothingToPlay,
    #[error("The player reported a problem: {0}.")]
    Player(String),
    /// A play request that gave way to a newer play or stop; the page has moved on and ignores it.
    #[error("Something else was asked to play first.")]
    Superseded,
    #[error("{0}")]
    Download(String),
    #[error("Bloom can't save downloads there: {0}")]
    DownloadFolder(String),
}

// Commands reject with `{ code, message }`. The message is written for people and shown as-is;
// the code lets the page react, such as returning to the sign-in screen.
impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let code = match self {
            Error::SignedOut => "signed-out",
            Error::Unreachable(_) => "unreachable",
            Error::Superseded => "superseded",
            _ => "other",
        };
        let mut st = s.serialize_struct("Error", 2)?;
        st.serialize_field("code", code)?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub(crate) address: String,
    pub(crate) id: String,
    pub(crate) name: String,
    version: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub(crate) id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicUser {
    id: String,
    name: String,
    has_password: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCheck {
    server: Server,
    quick_connect: bool,
    /// Empty on most servers: accounts are hidden from the login screen by default.
    users: Vec<PublicUser>,
}

/// What the page is allowed to know about a signed-in session.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    server: Server,
    user: User,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Startup {
    account: Option<Account>,
    /// True when a saved session exists but the server could not be reached to confirm it.
    offline: bool,
    last_address: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Session {
    server: Server,
    user: User,
    token: String,
}

impl Session {
    fn account(&self) -> Account {
        Account { server: self.server.clone(), user: self.user.clone() }
    }

    /// One sign-in per user per server.
    fn key(&self) -> String {
        format!("{}/{}", self.server.id, self.user.id)
    }
}

#[derive(Serialize, Deserialize, Default)]
struct SavedSessions {
    /// The key of the sign-in to use at startup.
    current: Option<String>,
    sessions: Vec<Session>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SavedServer {
    server: Server,
    /// Seconds since the Unix epoch when the server last answered.
    last_seen: Option<u64>,
}

/// A saved sign-in as the page may see it: never the token.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAccount {
    server: Server,
    user: User,
    current: bool,
}

/// A saved server, with what's known about it without asking it anything.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEntry {
    pub(crate) server: Server,
    last_seen: Option<u64>,
    /// The server of the account in use.
    current: bool,
    /// Users with a saved sign-in on it.
    accounts: Vec<User>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reachability {
    reachable: bool,
    last_seen: Option<u64>,
    version: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PublicInfo {
    id: Option<String>,
    server_name: Option<String>,
    version: Option<String>,
    product_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JfUser {
    id: String,
    name: String,
    #[serde(default)]
    has_password: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthResult {
    access_token: String,
    user: JfUser,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct QuickConnectInit {
    code: String,
    secret: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct QuickConnectState {
    authenticated: bool,
}

#[derive(Default)]
struct Inner {
    /// The server the sign-in screen is pointed at, once its address has been confirmed.
    server: Option<Server>,
    session: Option<Session>,
    quick_connect_secret: Option<String>,
}

/// Who a request goes out as, taken when it's queued rather than when it's sent, so a playback
/// report reaches the server and account that played, even after a switch (see `request_as`).
/// Carries the token, so it never leaves Rust.
#[derive(Clone)]
pub(crate) struct Credentials {
    address: String,
    token: String,
}

/// The signed-in session as the live-updates socket needs it (see live.rs). Compared whole to
/// notice a switch of account or a new sign-in.
#[derive(Clone, PartialEq)]
pub(crate) struct SocketAccess {
    pub(crate) address: String,
    pub(crate) user_id: String,
    /// A full MediaBrowser Authorization header, token included.
    pub(crate) authorization: String,
}

pub struct Jellyfin {
    http: reqwest::Client,
    /// For downloads, which can take hours: no overall time limit, only a limit on silence.
    transfer: reqwest::Client,
    dir: PathBuf,
    /// Disposable files, such as artwork. Safe to delete at any time.
    pub(crate) cache: PathBuf,
    device_id: String,
    device_name: String,
    version: String,
    inner: Mutex<Inner>,
    /// Held while accounts.json or servers.json is read, changed and written back.
    files: Mutex<()>,
}

impl Jellyfin {
    pub fn new(dir: PathBuf, cache: PathBuf, version: String) -> Result<Self, Error> {
        make_private_dir(&dir).map_err(|e| Error::Storage(e.to_string()))?;
        make_private_dir(&cache).map_err(|e| Error::Storage(e.to_string()))?;
        // Artwork was once cached in one folder for every server; it's a folder per server now,
        // so loose files from then are cleared (they download again as they're seen).
        if let Ok(entries) = fs::read_dir(cache.join("images")) {
            for entry in entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_file())) {
                let _ = fs::remove_file(entry.path());
            }
        }
        let id_path = dir.join("device-id");
        let device_id = match fs::read_to_string(&id_path) {
            Ok(id) if !id.trim().is_empty() => id.trim().to_string(),
            _ => {
                let id = uuid::Uuid::new_v4().simple().to_string();
                write_private(&id_path, id.as_bytes()).map_err(|e| Error::Storage(e.to_string()))?;
                id
            }
        };
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let transfer = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(Self {
            http,
            transfer,
            dir,
            cache,
            device_id,
            device_name: device_name(),
            version,
            inner: Mutex::default(),
            files: Mutex::default(),
        })
    }

    /// The signed-in server's id, which names its artwork cache folder.
    pub(crate) fn server_id(&self) -> Result<String, Error> {
        Ok(self.lock().session.as_ref().ok_or(Error::SignedOut)?.server.id.clone())
    }

    /// The signed-in server's address. Not secret.
    pub(crate) fn server_address(&self) -> Result<String, Error> {
        Ok(self.lock().session.as_ref().ok_or(Error::SignedOut)?.server.address.clone())
    }

    pub(crate) fn user_id(&self) -> Result<String, Error> {
        Ok(self.lock().session.as_ref().ok_or(Error::SignedOut)?.user.id.clone())
    }

    pub(crate) async fn get(&self, path: &str) -> Result<reqwest::Response, Error> {
        self.request(reqwest::Method::GET, path).await
    }

    pub(crate) async fn request(&self, method: reqwest::Method, path: &str) -> Result<reqwest::Response, Error> {
        self.request_with(method, path, None).await
    }

    /// A request as the signed-in user. A 401 means the token was revoked elsewhere (signed out
    /// from the dashboard, password changed), so the session is forgotten here as well.
    pub(crate) async fn request_with(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, Error> {
        self.authorized(&self.http, method, path, body, None).await
    }

    /// A download's GET, on the client without an overall time limit. A `range_from` above zero
    /// asks for the rest of a file from that byte.
    pub(crate) async fn transfer(&self, path: &str, range_from: u64) -> Result<reqwest::Response, Error> {
        let range = (range_from > 0).then(|| format!("bytes={range_from}-"));
        self.authorized(&self.transfer, reqwest::Method::GET, path, None, range).await
    }

    async fn authorized(
        &self,
        client: &reqwest::Client,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
        range: Option<String>,
    ) -> Result<reqwest::Response, Error> {
        let session = self.lock().session.clone().ok_or(Error::SignedOut)?;
        let res = self.send_on(client, method, &session.server.address, path, Some(&session.token), body, range).await?;
        if res.status().as_u16() == 401 {
            let still_current = {
                let mut inner = self.lock();
                let same = inner.session.as_ref().is_some_and(|s| s.token == session.token);
                if same {
                    inner.session = None;
                }
                same
            };
            // Only when nobody signed in again while this request was out.
            if still_current {
                let key = session.key();
                // Reading and writing accounts.json blocks, so the async worker hands its tasks on.
                tokio::task::block_in_place(|| {
                    let _ = self.update_sessions(|saved| {
                        saved.sessions.retain(|s| s.key() != key);
                        if saved.current.as_deref() == Some(key.as_str()) {
                            saved.current = None;
                        }
                    });
                });
            }
            return Err(Error::SignedOut);
        }
        check_status(res)
    }

    pub(crate) async fn get_json<T: DeserializeOwned>(&self, path: String) -> Result<T, Error> {
        read_json(self.get(&path).await?).await
    }

    pub(crate) async fn post_json<T: DeserializeOwned>(&self, path: String, body: serde_json::Value) -> Result<T, Error> {
        read_json(self.request_with(reqwest::Method::POST, &path, Some(body)).await?).await
    }

    pub(crate) async fn post_empty(&self, path: &str, body: serde_json::Value) -> Result<(), Error> {
        self.request_with(reqwest::Method::POST, path, Some(body)).await.map(|_| ())
    }

    /// The server address and an Authorization header value for media requests. It carries the
    /// token, so it is only ever handed to the player, never to the page.
    pub(crate) fn media_access(&self) -> Result<(String, String), Error> {
        let inner = self.lock();
        let session = inner.session.as_ref().ok_or(Error::SignedOut)?;
        Ok((session.server.address.clone(), format!(r#"MediaBrowser Token="{}""#, session.token)))
    }

    /// What the live-updates socket connects with. Its header carries the token, so it only ever
    /// goes to that connection.
    pub(crate) fn socket_access(&self) -> Result<SocketAccess, Error> {
        let inner = self.lock();
        let session = inner.session.as_ref().ok_or(Error::SignedOut)?;
        Ok(SocketAccess {
            address: session.server.address.clone(),
            user_id: session.user.id.clone(),
            authorization: self.authorization(Some(&session.token)),
        })
    }

    /// The signed-in account, to send requests as later.
    pub(crate) fn credentials(&self) -> Result<Credentials, Error> {
        let inner = self.lock();
        let session = inner.session.as_ref().ok_or(Error::SignedOut)?;
        Ok(Credentials { address: session.server.address.clone(), token: session.token.clone() })
    }

    /// A request as `who`, which may no longer be the account signed in. A 401 is only reported:
    /// the sign-in it would end might not be the current one.
    pub(crate) async fn request_as(
        &self,
        who: &Credentials,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, Error> {
        let res = self.send_on(&self.http, method, &who.address, path, Some(&who.token), body, None).await?;
        if res.status().as_u16() == 401 {
            return Err(Error::SignedOut);
        }
        check_status(res)
    }

    pub(crate) fn device_id(&self) -> &str {
        &self.device_id
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves plain data behind; carrying on is safe.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn authorization(&self, token: Option<&str>) -> String {
        let mut v = format!(
            r#"MediaBrowser Client="Bloom", Device="{}", DeviceId="{}", Version="{}""#,
            self.device_name, self.device_id, self.version
        );
        if let Some(t) = token {
            v.push_str(&format!(r#", Token="{t}""#));
        }
        v
    }

    async fn send(
        &self,
        method: reqwest::Method,
        base: &str,
        path: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, Error> {
        self.send_on(&self.http, method, base, path, token, body, None).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_on(
        &self,
        client: &reqwest::Client,
        method: reqwest::Method,
        base: &str,
        path: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
        range: Option<String>,
    ) -> Result<reqwest::Response, Error> {
        let mut req = client.request(method, format!("{base}{path}")).header("Authorization", self.authorization(token));
        if let Some(b) = body {
            req = req.json(&b);
        }
        if let Some(range) = range {
            req = req.header("Range", range);
        }
        req.send().await.map_err(|_| Error::Unreachable(display_host(base)))
    }

    fn server(&self) -> Result<Server, Error> {
        self.lock().server.clone().ok_or(Error::NoServer)
    }

    /// Saved sign-ins. A session.json from an earlier version becomes the one in use.
    fn saved_sessions(&self) -> SavedSessions {
        let path = self.dir.join("accounts.json");
        let mut saved: SavedSessions = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let old = self.dir.join("session.json");
        if let Some(session) = fs::read(&old).ok().and_then(|b| serde_json::from_slice::<Session>(&b).ok()) {
            if !saved.sessions.iter().any(|s| s.key() == session.key()) {
                saved.current = Some(session.key());
                saved.sessions.push(session);
            }
            if self.write_sessions(&saved).is_ok() {
                let _ = remove_if_present(&old);
            }
        }
        saved
    }

    fn write_sessions(&self, saved: &SavedSessions) -> Result<(), Error> {
        let bytes = serde_json::to_vec(saved).map_err(|e| Error::Storage(e.to_string()))?;
        write_private(&self.dir.join("accounts.json"), &bytes).map_err(|e| Error::Storage(e.to_string()))
    }

    fn update_sessions(&self, change: impl FnOnce(&mut SavedSessions)) -> Result<(), Error> {
        let _files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        let mut saved = self.saved_sessions();
        change(&mut saved);
        self.write_sessions(&saved)
    }

    fn saved_servers(&self) -> Vec<SavedServer> {
        fs::read(self.dir.join("servers.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn update_servers(&self, change: impl FnOnce(&mut Vec<SavedServer>)) -> Result<(), Error> {
        let _files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        let mut list = self.saved_servers();
        change(&mut list);
        let bytes = serde_json::to_vec(&list).map_err(|e| Error::Storage(e.to_string()))?;
        write_private(&self.dir.join("servers.json"), &bytes).map_err(|e| Error::Storage(e.to_string()))
    }

    /// Adds a server to the list, or refreshes it, as having answered now. Returns that time.
    fn remember_server(&self, server: &Server) -> u64 {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        // Not worth failing a sign-in over: at worst the Servers screen doesn't list it.
        let _ = self.update_servers(|list| match list.iter_mut().find(|v| v.server.id == server.id) {
            Some(saved) => {
                saved.server = server.clone();
                saved.last_seen = Some(now);
            }
            None => list.push(SavedServer { server: server.clone(), last_seen: Some(now) }),
        });
        now
    }

    fn finish(&self, server: Server, auth: AuthResult, remember: bool) -> Result<Account, Error> {
        let session = Session {
            server,
            user: User { id: auth.user.id, name: auth.user.name },
            token: auth.access_token,
        };
        let key = session.key();
        self.update_sessions(|saved| {
            saved.sessions.retain(|s| s.key() != key);
            if remember {
                saved.sessions.push(session.clone());
                saved.current = Some(key.clone());
            } else {
                // Held for this run only: next launch starts at sign-in, with the saved ones listed.
                saved.current = None;
            }
        })?;
        self.remember_server(&session.server);
        let account = session.account();
        let mut inner = self.lock();
        inner.quick_connect_secret = None;
        inner.session = Some(session);
        Ok(account)
    }
}

async fn read_json<T: DeserializeOwned>(res: reqwest::Response) -> Result<T, Error> {
    res.json::<T>().await.map_err(|_| Error::Unreadable)
}

fn check_status(res: reqwest::Response) -> Result<reqwest::Response, Error> {
    if res.status().is_success() {
        Ok(res)
    } else {
        Err(Error::Status(res.status().as_u16()))
    }
}

/// Load the saved session, if any, and confirm the server still accepts its token.
pub async fn startup_with(jf: &Jellyfin) -> Result<Startup, Error> {
    let last_address = fs::read(jf.dir.join("server.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Server>(&b).ok())
        .map(|s| s.address);

    let saved = jf.saved_sessions();
    let current = saved.current.as_deref().and_then(|key| saved.sessions.iter().find(|s| s.key() == key)).cloned();
    let Some(session) = current else {
        return Ok(Startup { account: None, offline: false, last_address });
    };

    let offline = match jf
        .send(reqwest::Method::GET, &session.server.address, "/Users/Me", Some(&session.token), None)
        .await
    {
        Ok(res) if res.status().is_success() => {
            jf.remember_server(&session.server);
            false
        }
        Ok(res) if matches!(res.status().as_u16(), 401 | 403) => {
            // Revoked elsewhere (signed out from the dashboard, password changed): forget it.
            let key = session.key();
            jf.update_sessions(|saved| {
                saved.sessions.retain(|s| s.key() != key);
                saved.current = None;
            })?;
            let mut inner = jf.lock();
            inner.server = Some(session.server.clone());
            return Ok(Startup { account: None, offline: false, last_address });
        }
        // Any other answer, or none at all: keep the session and let the page say it's offline.
        _ => true,
    };

    let account = session.account();
    let mut inner = jf.lock();
    inner.server = Some(session.server.clone());
    inner.session = Some(session);
    Ok(Startup { account: Some(account), offline, last_address })
}

/// Confirm an address belongs to a Jellyfin server and remember it.
pub async fn check_server_with(jf: &Jellyfin, address: String) -> Result<ServerCheck, Error> {
    let typed = address.trim().to_string();
    let candidates = candidates(&typed)?;

    let mut found = None;
    for base in &candidates {
        match jf.send(reqwest::Method::GET, base, "/System/Info/Public", None, None).await {
            Ok(res) => {
                found = Some((base.clone(), res));
                break;
            }
            Err(_) => continue,
        }
    }
    let Some((base, res)) = found else { return Err(Error::Unreachable(typed)) };

    let not_jellyfin = || Error::NotJellyfin(typed.clone());
    if !res.status().is_success() {
        return Err(not_jellyfin());
    }
    let info: PublicInfo = res.json().await.map_err(|_| not_jellyfin())?;
    let is_jellyfin = info.product_name.as_deref().is_none_or(|p| p.starts_with("Jellyfin"));
    let (Some(id), Some(version), true) = (info.id, info.version, is_jellyfin) else {
        return Err(not_jellyfin());
    };

    let server = Server {
        name: info.server_name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| display_host(&base)),
        address: base.clone(),
        id,
        version,
    };

    // Both are optional extras; a server that errors on either still gets a sign-in form.
    let quick_connect = match jf.send(reqwest::Method::GET, &base, "/QuickConnect/Enabled", None, None).await {
        Ok(res) if res.status().is_success() => read_json::<bool>(res).await.unwrap_or(false),
        _ => false,
    };
    let users = match jf.send(reqwest::Method::GET, &base, "/Users/Public", None, None).await {
        Ok(res) if res.status().is_success() => read_json::<Vec<JfUser>>(res).await.unwrap_or_default(),
        _ => Vec::new(),
    };

    let bytes = serde_json::to_vec(&server).map_err(|e| Error::Storage(e.to_string()))?;
    write_private(&jf.dir.join("server.json"), &bytes).map_err(|e| Error::Storage(e.to_string()))?;
    jf.remember_server(&server);

    {
        let mut inner = jf.lock();
        inner.server = Some(server.clone());
        inner.quick_connect_secret = None;
    }
    Ok(ServerCheck {
        server,
        quick_connect,
        users: users
            .into_iter()
            .map(|u| PublicUser { id: u.id, name: u.name, has_password: u.has_password })
            .collect(),
    })
}

/// Go back to the address step. Nothing on the server changes.
#[tauri::command]
pub fn forget_server(jf: State<'_, Jellyfin>) {
    let mut inner = jf.lock();
    inner.server = None;
    inner.quick_connect_secret = None;
}

pub async fn sign_in_with(jf: &Jellyfin, username: String, password: String, remember: bool) -> Result<Account, Error> {
    let server = jf.server()?;
    let res = jf
        .send(
            reqwest::Method::POST,
            &server.address,
            "/Users/AuthenticateByName",
            None,
            Some(json!({ "Username": username.trim(), "Pw": password })),
        )
        .await?;
    if matches!(res.status().as_u16(), 400 | 401 | 403) {
        return Err(Error::BadCredentials);
    }
    let auth = read_json::<AuthResult>(check_status(res)?).await?;
    jf.finish(server, auth, remember)
}

/// Ask the server for a Quick Connect code. The secret that redeems it stays in Rust.
pub async fn quick_connect_start_with(jf: &Jellyfin) -> Result<String, Error> {
    let server = jf.server()?;
    let res = jf.send(reqwest::Method::POST, &server.address, "/QuickConnect/Initiate", None, None).await?;
    if matches!(res.status().as_u16(), 401 | 403) {
        return Err(Error::QuickConnectOff);
    }
    let init = read_json::<QuickConnectInit>(check_status(res)?).await?;
    jf.lock().quick_connect_secret = Some(init.secret);
    Ok(init.code)
}

/// Check once whether the code has been approved; signs in when it has.
pub async fn quick_connect_poll_with(jf: &Jellyfin, remember: bool) -> Result<Option<Account>, Error> {
    let server = jf.server()?;
    let Some(secret) = jf.lock().quick_connect_secret.clone() else { return Err(Error::QuickConnectExpired) };

    let path = format!("/QuickConnect/Connect?secret={secret}");
    let res = jf.send(reqwest::Method::GET, &server.address, &path, None, None).await?;
    if matches!(res.status().as_u16(), 400 | 404) {
        jf.lock().quick_connect_secret = None;
        return Err(Error::QuickConnectExpired);
    }
    if !read_json::<QuickConnectState>(check_status(res)?).await?.authenticated {
        return Ok(None);
    }

    let res = jf
        .send(
            reqwest::Method::POST,
            &server.address,
            "/Users/AuthenticateWithQuickConnect",
            None,
            Some(json!({ "Secret": secret })),
        )
        .await?;
    let auth = read_json::<AuthResult>(check_status(res)?).await?;
    jf.finish(server, auth, remember).map(Some)
}

#[tauri::command]
pub fn quick_connect_cancel(jf: State<'_, Jellyfin>) {
    jf.lock().quick_connect_secret = None;
}

/// Revoke the token on the server, then forget it here even if the server can't be reached.
pub async fn sign_out_with(jf: &Jellyfin) -> Result<(), Error> {
    let Some(session) = jf.lock().session.take() else { return Ok(()) };
    let _ = jf.send(reqwest::Method::POST, &session.server.address, "/Sessions/Logout", Some(&session.token), None).await;
    forget_saved(jf, &session.key())
}

/// Removes a saved sign-in, and stops it being the one used at startup.
fn forget_saved(jf: &Jellyfin, key: &str) -> Result<(), Error> {
    jf.update_sessions(|saved| {
        saved.sessions.retain(|s| s.key() != key);
        if saved.current.as_deref() == Some(key) {
            saved.current = None;
        }
    })
}

/// Every saved sign-in, and the one in use if it isn't saved. Never the tokens.
pub fn saved_accounts_with(jf: &Jellyfin) -> Vec<SavedAccount> {
    let current = jf.lock().session.clone();
    let current_key = current.as_ref().map(Session::key);
    let saved = jf.saved_sessions();
    let mut list: Vec<SavedAccount> = saved
        .sessions
        .iter()
        .map(|s| SavedAccount { server: s.server.clone(), user: s.user.clone(), current: Some(s.key()) == current_key })
        .collect();
    if let Some(session) = current.filter(|c| !saved.sessions.iter().any(|s| s.key() == c.key())) {
        list.insert(0, SavedAccount { server: session.server, user: session.user, current: true });
    }
    list
}

/// Use another saved sign-in, once its server confirms it still stands. A sign-in held only for
/// this run (Stay signed in off) is signed out on the server as it's left: nothing could come back
/// to it.
pub async fn switch_account_with(jf: &Jellyfin, server_id: &str, user_id: &str) -> Result<Account, Error> {
    let key = format!("{server_id}/{user_id}");
    let saved = jf.saved_sessions();
    let session = saved.sessions.iter().find(|s| s.key() == key).cloned().ok_or(Error::SignedOut)?;
    let res = jf.send(reqwest::Method::GET, &session.server.address, "/Users/Me", Some(&session.token), None).await?;
    if matches!(res.status().as_u16(), 401 | 403) {
        forget_saved(jf, &key)?;
        return Err(Error::AccountEnded(session.user.name));
    }
    check_status(res)?;
    jf.update_sessions(|saved| saved.current = Some(key.clone()))?;
    jf.remember_server(&session.server);
    let account = session.account();
    let previous = {
        let mut inner = jf.lock();
        inner.server = Some(session.server.clone());
        inner.quick_connect_secret = None;
        inner.session.replace(session)
    };
    if let Some(left) = previous.filter(|p| p.key() != key && !saved.sessions.iter().any(|s| s.key() == p.key())) {
        let _ = jf.send(reqwest::Method::POST, &left.server.address, "/Sessions/Logout", Some(&left.token), None).await;
    }
    Ok(account)
}

/// Forget a saved sign-in, signing it out on its server where that can be reached. Returns
/// whether it was the one in use, which leaves Bloom signed out.
pub async fn forget_account_with(jf: &Jellyfin, server_id: &str, user_id: &str) -> Result<bool, Error> {
    let key = format!("{server_id}/{user_id}");
    let in_use = jf.lock().session.clone().filter(|s| s.key() == key);
    if let Some(s) = jf.saved_sessions().sessions.into_iter().find(|s| s.key() == key).or(in_use) {
        let _ = jf.send(reqwest::Method::POST, &s.server.address, "/Sessions/Logout", Some(&s.token), None).await;
    }
    forget_saved(jf, &key)?;
    let mut inner = jf.lock();
    let was_current = inner.session.as_ref().is_some_and(|s| s.key() == key);
    if was_current {
        inner.session = None;
    }
    Ok(was_current)
}

/// Saved servers, including any with a saved sign-in from before the list existed.
pub fn servers_with(jf: &Jellyfin) -> Vec<ServerEntry> {
    let current = jf.lock().session.as_ref().map(|s| s.server.id.clone());
    let sessions = jf.saved_sessions().sessions;
    let mut servers = jf.saved_servers();
    for s in &sessions {
        if !servers.iter().any(|v| v.server.id == s.server.id) {
            servers.push(SavedServer { server: s.server.clone(), last_seen: None });
        }
    }
    servers
        .into_iter()
        .map(|v| ServerEntry {
            current: current.as_deref() == Some(v.server.id.as_str()),
            accounts: sessions.iter().filter(|s| s.server.id == v.server.id).map(|s| s.user.clone()).collect(),
            last_seen: v.last_seen,
            server: v.server,
        })
        .collect()
}

/// Asks a saved server whether it's there (as the same server, not something else now at its
/// address), and records when it last answered.
pub async fn ping_server_with(jf: &Jellyfin, server_id: &str) -> Result<Reachability, Error> {
    let entry = servers_with(jf).into_iter().find(|v| v.server.id == server_id).ok_or(Error::NoServer)?;
    let answered = match jf.send(reqwest::Method::GET, &entry.server.address, "/System/Info/Public", None, None).await {
        Ok(res) if res.status().is_success() => {
            read_json::<PublicInfo>(res).await.ok().filter(|info| info.id.as_deref() == Some(server_id))
        }
        _ => None,
    };
    let Some(info) = answered else {
        return Ok(Reachability { reachable: false, last_seen: entry.last_seen, version: None });
    };
    let server = Server {
        name: info.server_name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| entry.server.name.clone()),
        version: info.version.unwrap_or_else(|| entry.server.version.clone()),
        ..entry.server
    };
    let seen = jf.remember_server(&server);
    Ok(Reachability { reachable: true, last_seen: Some(seen), version: Some(server.version) })
}

/// Forgets a server and every sign-in saved for it, signing each out where the server can be
/// reached. Returns whether that left Bloom signed out.
pub async fn remove_server_with(jf: &Jellyfin, server_id: &str) -> Result<bool, Error> {
    let mut leaving: Vec<Session> = jf.saved_sessions().sessions.into_iter().filter(|s| s.server.id == server_id).collect();
    if let Some(in_use) = jf.lock().session.clone().filter(|c| c.server.id == server_id) {
        if !leaving.iter().any(|s| s.key() == in_use.key()) {
            leaving.push(in_use);
        }
    }
    for s in &leaving {
        let _ = jf.send(reqwest::Method::POST, &s.server.address, "/Sessions/Logout", Some(&s.token), None).await;
    }
    let prefix = format!("{server_id}/");
    jf.update_sessions(|saved| {
        saved.sessions.retain(|s| s.server.id != server_id);
        if saved.current.as_deref().is_some_and(|k| k.starts_with(&prefix)) {
            saved.current = None;
        }
    })?;
    jf.update_servers(|list| list.retain(|v| v.server.id != server_id))?;
    // Its artwork goes with it, so the sign-in wall can't show it afterwards.
    if let Some(folder) = crate::images::server_folder(&jf.cache, server_id) {
        let _ = fs::remove_dir_all(folder);
    }
    let mut inner = jf.lock();
    let was_current = inner.session.as_ref().is_some_and(|s| s.server.id == server_id);
    if was_current {
        inner.session = None;
    }
    if inner.server.as_ref().is_some_and(|s| s.id == server_id) {
        inner.server = None;
    }
    Ok(was_current)
}

// The commands themselves are thin: the work lives in the `_with` functions above so the live
// test below can run it without a Tauri app.

#[tauri::command]
pub async fn startup(jf: State<'_, Jellyfin>) -> Result<Startup, Error> {
    startup_with(&jf).await
}

#[tauri::command]
pub async fn check_server(jf: State<'_, Jellyfin>, address: String) -> Result<ServerCheck, Error> {
    check_server_with(&jf, address).await
}

#[tauri::command]
pub async fn sign_in(
    jf: State<'_, Jellyfin>,
    username: String,
    password: String,
    remember: bool,
) -> Result<Account, Error> {
    sign_in_with(&jf, username, password, remember).await
}

#[tauri::command]
pub async fn quick_connect_start(jf: State<'_, Jellyfin>) -> Result<String, Error> {
    quick_connect_start_with(&jf).await
}

#[tauri::command]
pub async fn quick_connect_poll(jf: State<'_, Jellyfin>, remember: bool) -> Result<Option<Account>, Error> {
    quick_connect_poll_with(&jf, remember).await
}

#[tauri::command]
pub async fn sign_out(jf: State<'_, Jellyfin>) -> Result<(), Error> {
    sign_out_with(&jf).await
}

#[tauri::command]
pub fn saved_accounts(jf: State<'_, Jellyfin>) -> Vec<SavedAccount> {
    saved_accounts_with(&jf)
}

#[tauri::command]
pub async fn switch_account(jf: State<'_, Jellyfin>, server_id: String, user_id: String) -> Result<Account, Error> {
    switch_account_with(&jf, &server_id, &user_id).await
}

#[tauri::command]
pub async fn forget_account(jf: State<'_, Jellyfin>, server_id: String, user_id: String) -> Result<bool, Error> {
    forget_account_with(&jf, &server_id, &user_id).await
}

#[tauri::command]
pub fn servers(jf: State<'_, Jellyfin>) -> Vec<ServerEntry> {
    servers_with(&jf)
}

#[tauri::command]
pub async fn ping_server(jf: State<'_, Jellyfin>, server_id: String) -> Result<Reachability, Error> {
    ping_server_with(&jf, &server_id).await
}

#[tauri::command]
pub async fn remove_server(jf: State<'_, Jellyfin>, server_id: String) -> Result<bool, Error> {
    remove_server_with(&jf, &server_id).await
}

/// Turn what someone typed or pasted into the base addresses worth trying, in order.
///
/// Accepts "192.168.1.20:8096", "jellyfin.home", or a URL copied from the web client such as
/// "http://host:8096/web/#/home". A reverse-proxy path like "/jellyfin" is kept.
fn candidates(input: &str) -> Result<Vec<String>, Error> {
    if input.is_empty() {
        return Err(Error::EmptyAddress);
    }
    let scheme_given = input.contains("://");
    let mut url = Url::parse(&if scheme_given { input.to_string() } else { format!("http://{input}") })
        .map_err(|_| Error::BadAddress)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return Err(Error::BadAddress);
    }
    url.set_query(None);
    url.set_fragment(None);
    let _ = url.set_username("");
    let _ = url.set_password(None);
    let kept: Vec<String> = url
        .path_segments()
        .map(|segs| segs.take_while(|s| *s != "web").filter(|s| !s.is_empty()).map(String::from).collect())
        .unwrap_or_default();
    url.set_path(&kept.join("/"));

    let base = |u: &Url| u.as_str().trim_end_matches('/').to_string();
    let mut out = vec![base(&url)];
    // A bare host with no scheme or port most likely means Jellyfin's default port.
    if !scheme_given && url.port().is_none() {
        let mut with_port = url.clone();
        let _ = with_port.set_port(Some(8096));
        out.push(base(&with_port));
    }
    Ok(out)
}

fn display_host(base: &str) -> String {
    Url::parse(base)
        .ok()
        .and_then(|u| u.host_str().map(|h| match u.port() {
            Some(p) => format!("{h}:{p}"),
            None => h.to_string(),
        }))
        .unwrap_or_else(|| base.to_string())
}

fn device_name() -> String {
    let raw = fs::read_to_string("/etc/hostname")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_default();
    // The name sits inside a quoted header value.
    let clean: String = raw.trim().chars().filter(|c| !c.is_control() && *c != '"' && *c != '\\').collect();
    if clean.is_empty() { "Desktop".into() } else { clean }
}

fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Write via a temporary file and rename, so a crash never leaves half a session behind.
/// On Unix the file is owner-only from the moment it exists. Windows (a later phase) relies on
/// the per-user profile folder's own permissions.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    #[cfg(unix)]
    {
        // `mode` only applies on creation; a leftover temp file could have kept looser bits.
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(&tmp, path)
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs against a real server. Reads BLOOM_SERVER, BLOOM_USER and BLOOM_PASS from the
    /// environment (see .env.local), never from arguments:
    ///
    ///     set -a; . ../../.env.local; set +a; cargo test live -- --ignored --nocapture
    #[test]
    #[ignore = "needs a Jellyfin server and a test account"]
    fn live_sign_in_round_trip() {
        let (Ok(address), Ok(user), Ok(pass)) =
            (std::env::var("BLOOM_SERVER"), std::env::var("BLOOM_USER"), std::env::var("BLOOM_PASS"))
        else {
            panic!("set BLOOM_SERVER, BLOOM_USER and BLOOM_PASS");
        };
        let dir = std::env::temp_dir().join(format!("bloom-test-{}", uuid::Uuid::new_v4().simple()));
        // Removes the folder even when an assertion fails, so no token is left lying around.
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(dir.clone());
        let open = || Jellyfin::new(dir.clone(), dir.join("cache"), "0.0.0-test".into()).unwrap();
        let accounts_path = dir.join("accounts.json");
        let no_saved_sessions = |path: &Path| {
            fs::read(path)
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .is_none_or(|v| v["sessions"].as_array().is_none_or(|a| a.is_empty()))
        };

        tauri::async_runtime::block_on(async {
            let jf = open();
            assert!(matches!(sign_in_with(&jf, user.clone(), pass.clone(), true).await, Err(Error::NoServer)));

            let check = check_server_with(&jf, address.clone()).await.unwrap();
            println!("server {} ({}), quick connect {}", check.server.name, check.server.version, check.quick_connect);

            let wrong = sign_in_with(&jf, user.clone(), format!("{pass}-wrong"), true).await;
            assert!(matches!(wrong, Err(Error::BadCredentials)), "wrong password was not refused");

            let account = sign_in_with(&jf, user.clone(), pass.clone(), true).await.unwrap();
            assert_eq!(account.user.name, user);
            let saved = fs::read(&accounts_path).unwrap();
            // A substring search would misfire (a short password can sit inside the username),
            // so check the shape instead: exactly these fields, and no password-like key.
            let fields: serde_json::Value = serde_json::from_slice(&saved).unwrap();
            let sessions = fields["sessions"].as_array().expect("no saved sessions");
            assert_eq!(sessions.len(), 1);
            assert!(fields["current"].is_string(), "the saved sign-in isn't the one in use");
            let mut keys: Vec<_> = sessions[0].as_object().unwrap().keys().cloned().collect();
            keys.sort();
            assert_eq!(keys, ["server", "token", "user"]);
            let user_keys: Vec<_> = sessions[0]["user"].as_object().unwrap().keys().cloned().collect();
            assert!(user_keys.iter().all(|k| k == "id" || k == "name"), "unexpected user fields {user_keys:?}");
            assert_ne!(sessions[0]["token"].as_str().unwrap(), pass);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = fs::metadata(&accounts_path).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o600, "session file is readable by others");
                let dir_mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
                assert_eq!(dir_mode, 0o700);
            }

            // A fresh start finds the session and the server still accepts it.
            let restarted = open();
            let s = startup_with(&restarted).await.unwrap();
            assert!(s.account.is_some() && !s.offline, "saved session was not restored");

            // The home screen loads, and its artwork comes through the image cache.
            let sections = crate::home::home_with(&restarted).await.unwrap();
            for s in &sections {
                println!("section {:?}: {} cards", s.title, s.cards.len());
            }
            assert!(sections.iter().any(|s| s.id == "libraries" && !s.cards.is_empty()), "no libraries");
            let image = sections.iter().flat_map(|s| &s.cards).find_map(|c| c.image.as_ref()).expect("no artwork");
            let path = format!("/{}/{}/{}/300", image.item_id, image.kind, image.tag);
            let first = crate::images::fetch(&restarted, &path).await.unwrap();
            assert!(first.starts_with(&[0xFF, 0xD8]) || first.starts_with(b"\x89PNG") || first.starts_with(b"RIFF"));
            let server_folder = crate::images::server_folder(&restarted.cache, &restarted.server_id().unwrap()).unwrap();
            let cached = fs::read_dir(&server_folder).unwrap().count();
            assert_eq!(cached, 1, "artwork was not cached in the server's folder");
            // A cache-only path naming the server reads the same file.
            let named = format!("/{}/{}/{}/{}/360", restarted.server_id().unwrap(), image.item_id, image.kind, image.tag);
            assert_eq!(crate::images::fetch(&restarted, &named).await.unwrap(), first);
            assert_eq!(crate::images::fetch(&restarted, &path).await.unwrap(), first);
            assert!(crate::images::fetch(&restarted, "/../../session.json/Primary/x/300").await.is_err());

            // Spotlight items all carry a backdrop; favouriting one round-trips and is undone.
            let spots = crate::home::spotlight_with(&restarted).await.unwrap();
            println!("spotlight: {} items, {} with logos", spots.len(), spots.iter().filter(|s| s.logo.is_some()).count());
            assert!(!spots.is_empty(), "spotlight is empty");
            let first = &spots[0];
            let flipped = crate::home::set_favorite_with(&restarted, first.id.clone(), !first.favorite).await.unwrap();
            assert_eq!(flipped, !first.favorite);
            let restored = crate::home::set_favorite_with(&restarted, first.id.clone(), first.favorite).await.unwrap();
            assert_eq!(restored, first.favorite);
            assert!(crate::home::set_favorite_with(&restarted, "../Users".into(), true).await.is_err());

            // Playback: an unwatched film resolves to direct play with a token-free URL, and
            // reported progress becomes its resume point.
            let uid = restarted.user_id().unwrap();
            let films: serde_json::Value = restarted
                .get_json(format!(
                    "/Items?userId={uid}&includeItemTypes=Movie&recursive=true&limit=200&isPlayed=false&fields=MediaStreams"
                ))
                .await
                .unwrap();
            let films = films["Items"].as_array().cloned().unwrap_or_default();
            let has = |film: &serde_json::Value, test: &dyn Fn(&serde_json::Value) -> bool| {
                film["MediaStreams"].as_array().is_some_and(|streams| streams.iter().any(test))
            };
            // A file the server has scanned: one with no stream data can only be transcoded.
            let film_id = films
                .iter()
                .find(|f| has(f, &|s| s["Type"] == "Video"))
                .and_then(|f| f["Id"].as_str())
                .expect("no unwatched film with a video stream")
                .to_string();
            let no_choices = |_: &str| None;
            let original = crate::playback::PlayOptions {
                quality: Default::default(),
                start_seconds: None,
                remembered: &no_choices,
                direct_play: true,
                preferences: Default::default(),
            };
            let plan = crate::playback::prepare_with(&restarted, &film_id, &original).await.unwrap();
            println!("playback: {:?} from {:.0}s", plan.method, plan.start_seconds);
            assert_eq!(plan.method, crate::playback::Method::DirectPlay);
            assert!(plan.url.contains("/Videos/") && plan.url.contains("static=true"));
            let lower = plan.url.to_lowercase();
            assert!(!lower.contains("api_key") && !lower.contains("apikey") && !lower.contains("token"), "token in URL");

            // A film with a subtitle file beside it still direct-plays, and the subtitle reaches
            // mpv without the token the server puts in its URL.
            if let Some(with_file) = films.iter().find(|f| has(f, &|s| s["IsExternal"] == true)).and_then(|f| f["Id"].as_str()) {
                let plan = crate::playback::prepare_with(&restarted, with_file, &original).await.unwrap();
                assert_eq!(plan.method, crate::playback::Method::DirectPlay, "an external subtitle forced a transcode");
                assert!(!plan.external_subtitles.is_empty(), "external subtitle not offered");
                for subtitle in &plan.external_subtitles {
                    let lower = subtitle.url.to_lowercase();
                    assert!(!lower.contains("apikey") && !lower.contains("api_key"), "token in subtitle URL");
                }
                println!("playback: film with a subtitle file direct-plays ({} file)", plan.external_subtitles.len());
            }

            // Jellyfin ignores resume points under 5% of the runtime, so report 20% in.
            let at = plan.now_playing.duration_seconds.expect("film has no runtime") * 0.2;
            let mut session = crate::playback::Session::new(&plan);
            session.position = at;
            for report in [session.playing_report(), session.progress_report("TimeUpdate"), session.stopped_report()] {
                crate::playback::send_report(&restarted, report).await.unwrap();
            }
            let item: serde_json::Value = restarted.get_json(format!("/Items/{film_id}?userId={uid}")).await.unwrap();
            let ticks = item["UserData"]["PlaybackPositionTicks"].as_i64().unwrap_or(0) as f64 / 10_000_000.0;
            assert!((ticks - at).abs() < 2.0, "resume point is {ticks:.1}s, expected {at:.1}s");
            let resumed = crate::playback::prepare_with(&restarted, &film_id, &original).await.unwrap();
            assert!((resumed.start_seconds - at).abs() < 2.0, "resumes from {:.1}s", resumed.start_seconds);
            println!("playback: resume point {ticks:.0}s round-tripped");

            // Finishing marks the film played: a file that ends reports its full runtime.
            session.position = plan.now_playing.duration_seconds.expect("film has no runtime");
            for report in [session.playing_report(), session.stopped_report()] {
                crate::playback::send_report(&restarted, report).await.unwrap();
            }
            let item: serde_json::Value = restarted.get_json(format!("/Items/{film_id}?userId={uid}")).await.unwrap();
            assert_eq!(item["UserData"]["Played"], true, "a finished film wasn't marked played");
            println!("playback: finishing marks the film played");

            // Leave the test account as it was.
            restarted.request(reqwest::Method::DELETE, &format!("/UserPlayedItems/{film_id}?userId={uid}")).await.unwrap();
            let item: serde_json::Value = restarted.get_json(format!("/Items/{film_id}?userId={uid}")).await.unwrap();
            assert_eq!(item["UserData"]["PlaybackPositionTicks"].as_i64(), Some(0), "resume point not cleared");
            assert_eq!(item["UserData"]["Played"], false, "played state not cleared");

            // A whole show resolves to one of its episodes.
            let shows: serde_json::Value = restarted
                .get_json(format!("/Items?userId={uid}&includeItemTypes=Series&recursive=true&limit=1&sortBy=Random"))
                .await
                .unwrap();
            let show_id = shows["Items"][0]["Id"].as_str().expect("no show").to_string();
            let episode = crate::playback::prepare_with(&restarted, &show_id, &original).await.unwrap();
            assert_ne!(episode.item_id, show_id);
            println!("playback: show resolves to {:?}", episode.now_playing.subtitle);
            assert!(episode.now_playing.subtitle.is_some());

            // Up next follows the show's own order, and a film has none.
            let first_two: serde_json::Value = restarted
                .get_json(format!("/Shows/{show_id}/Episodes?userId={uid}&limit=2&isMissing=false"))
                .await
                .unwrap();
            if let [first, second] = first_two["Items"].as_array().map(Vec::as_slice).unwrap_or_default() {
                let first_id = first["Id"].as_str().unwrap();
                let next = crate::playback::next_episode_after(&restarted, first_id).await.unwrap();
                let next = next.expect("the first episode has no next episode");
                assert_eq!(next.item_id, second["Id"].as_str().unwrap());
                println!("playback: up next after the first episode is {:?}", next.subtitle);
            }
            assert!(crate::playback::next_episode_after(&restarted, &film_id).await.unwrap().is_none());

            // The shell's pages: libraries, a library grid per kind, search, and item pages.
            assert!(!crate::library::libraries_with(&restarted).await.unwrap().is_empty(), "no libraries");
            let views: serde_json::Value = restarted.get_json(format!("/UserViews?userId={uid}")).await.unwrap();
            for (collection, sort) in [("movies", "name"), ("tvshows", "added")] {
                let Some(id) = views["Items"]
                    .as_array()
                    .and_then(|v| v.iter().find(|l| l["CollectionType"] == collection))
                    .and_then(|l| l["Id"].as_str())
                else {
                    continue;
                };
                let grid = crate::library::library_items_with(&restarted, id, sort, 0, None).await.unwrap();
                assert!(grid.total > 0 && !grid.cards.is_empty(), "empty {collection} library");
                assert!(grid.cards.len() <= crate::library::GRID_PAGE as usize);
            }
            assert!(!crate::library::search_with(&restarted, "the", crate::library::GRID_PAGE).await.unwrap().is_empty(), "search found nothing");
            assert!(crate::library::search_with(&restarted, "t", crate::library::GRID_PAGE).await.unwrap().is_empty());
            assert!(crate::library::search_with(&restarted, "the", 8).await.unwrap().len() <= 8, "suggestions not limited");

            let series = crate::library::item_detail_with(&restarted, &show_id).await.unwrap();
            assert!(!series.seasons.is_empty() && series.play.is_some(), "series page has no seasons or play target");
            let season_page = crate::library::season_episodes_with(&restarted, &show_id, &series.seasons[0].id).await.unwrap();
            assert!(season_page.total > 0 && !season_page.episodes.is_empty());
            // Whole seasons, not a page of one.
            assert_eq!(season_page.episodes.len() as u32, season_page.total, "season cut short");
            let similar_titles = crate::library::similar_with(&restarted, &show_id).await.unwrap();
            println!(
                "pages: show audio {:?}, subtitles {:?}, {} similar titles",
                series.audio_languages,
                series.subtitle_languages,
                similar_titles.len()
            );
            let hover = crate::library::card_detail_with(&restarted, &show_id).await.unwrap();
            assert_eq!(hover.kind, "Series");
            assert!(hover.episode_count.is_some_and(|n| n > 0), "hover card has no episode count");

            let film = crate::library::item_detail_with(&restarted, &film_id).await.unwrap();
            assert!(film.media.is_some() && film.play.is_some() && film.seasons.is_empty(), "film page incomplete");
            // Marking watched round-trips and is put back.
            let flipped = crate::library::set_played_with(&restarted, &film_id, !film.played).await.unwrap();
            assert_eq!(flipped, !film.played);
            assert_eq!(crate::library::set_played_with(&restarted, &film_id, film.played).await.unwrap(), film.played);
            println!(
                "pages: series with {} season(s) and {} people, season page of {}, film media info present",
                series.seasons.len(),
                series.people.len(),
                season_page.total
            );

            // The watch screen's list: a season in order with the next season's start, or films
            // like one.
            if let (Some(first), Some(season_id)) = (first_two["Items"].get(0), first_two["Items"][0]["SeasonId"].as_str()) {
                let list = crate::library::season_list_with(&restarted, &show_id, season_id).await.unwrap();
                // Its season as the picker lists it (a server can name the same season by another id).
                assert!(list.seasons.iter().any(|s| s.id == list.season_id), "season missing from the picker ({})", show_id);
                assert_eq!(list.entries.first().map(|e| e.id.as_str()), first["Id"].as_str(), "season doesn't start at its first episode");
                if let Some(next) = &list.next {
                    assert!(!next.entries.is_empty() && next.entries.len() <= 3, "next season preview has {} entries", next.entries.len());
                }
                println!("watch: season of {} with {} season(s) in the picker", list.entries.len(), list.seasons.len());
            }
            let similar = crate::library::watch_queue_with(&restarted, &film_id).await.unwrap();
            assert_eq!(similar.heading, "More like this");
            if let Some(first) = first_two["Items"].get(0).and_then(|e| e["Id"].as_str()) {
                let segments = crate::library::skip_segments_with(&restarted, first).await.unwrap();
                assert!(segments.windows(2).all(|w| w[0].start_seconds <= w[1].start_seconds), "segments out of order");
                assert!(segments.iter().all(|s| s.end_seconds > s.start_seconds));
                println!("watch: {} skippable segment(s) in an episode: {:?}", segments.len(), segments.iter().map(|s| s.kind).collect::<Vec<_>>());
            }

            // Chapters arrive in order, and within the runtime.
            let with_chapters: serde_json::Value = restarted
                .get_json(format!("/Items?userId={uid}&includeItemTypes=Episode,Movie&recursive=true&limit=100&fields=Chapters"))
                .await
                .unwrap();
            if let Some(id) = with_chapters["Items"]
                .as_array()
                .and_then(|items| items.iter().find(|i| i["Chapters"].as_array().is_some_and(|c| c.len() > 1)))
                .and_then(|i| i["Id"].as_str())
            {
                let detail = crate::library::item_detail_with(&restarted, id).await.unwrap();
                assert!(detail.chapters.len() > 1, "chapters lost");
                assert!(detail.chapters.windows(2).all(|w| w[0].start_seconds <= w[1].start_seconds), "chapters out of order");
                println!("watch: {} chapters, {} similar films", detail.chapters.len(), similar.entries.len());
            }

            // A lower quality transcodes a larger file down to fit, with the remembered audio.
            let episodes: serde_json::Value = restarted
                .get_json(format!("/Items?userId={uid}&includeItemTypes=Episode,Movie&recursive=true&limit=400&fields=MediaStreams"))
                .await
                .unwrap();
            let candidate = episodes["Items"].as_array().and_then(|items| {
                items.iter().find_map(|i| {
                    let streams = i["MediaStreams"].as_array()?;
                    let tall = streams.iter().any(|s| s["Type"] == "Video" && s["Height"].as_i64().unwrap_or(0) > 720);
                    let audio: Vec<&serde_json::Value> = streams.iter().filter(|s| s["Type"] == "Audio").collect();
                    let (first, last) = (audio.first()?, audio.last()?);
                    if !tall || first["Language"] == last["Language"] {
                        return None;
                    }
                    Some((i["Id"].as_str()?.to_string(), last["Index"].as_i64()?, last["Language"].as_str()?.to_string()))
                })
            });
            if let Some((id, audio_index, lang)) = candidate {
                let choice: crate::playback::TrackChoice = serde_json::from_value(serde_json::json!({
                    "audio": { "lang": lang, "title": null, "codec": null, "forced": false, "hearingImpaired": false }
                }))
                .unwrap();
                let remembered = move |_: &str| Some(choice.clone());
                let hd720 = crate::playback::PlayOptions {
                    quality: crate::playback::Quality::Hd720,
                    start_seconds: Some(30.0),
                    remembered: &remembered,
                    direct_play: true,
                    preferences: Default::default(),
                };
                let plan = crate::playback::prepare_with(&restarted, &id, &hd720).await.unwrap();
                assert_eq!(plan.method, crate::playback::Method::Transcode, "720p didn't transcode a larger file");
                let lower = plan.url.to_lowercase();
                assert!(lower.contains("maxheight=720") || lower.contains("maxwidth=1280"), "transcode not scaled down");
                assert_eq!(plan.server_audio.iter().find(|t| t.selected).map(|t| t.id), Some(audio_index), "remembered audio not asked for");
                assert_eq!(plan.start_seconds, 30.0);
                println!("quality: 720p transcodes with audio #{audio_index} ({lang}), {} subtitle file(s)", plan.external_subtitles.len());
            }

            if check.quick_connect {
                let code = quick_connect_start_with(&restarted).await.unwrap();
                assert_eq!(code.len(), 6);
                assert!(quick_connect_poll_with(&restarted, false).await.unwrap().is_none());
                println!("quick connect code issued and pending, not approved (expected)");
            }

            // Saved accounts and servers: the sign-in and its server are listed, switching to it
            // checks it with the server, and the server answers a ping.
            let accounts = saved_accounts_with(&restarted);
            assert!(accounts.len() == 1 && accounts[0].current, "the saved account isn't listed as in use");
            let (server_id, user_id) = (accounts[0].server.id.clone(), accounts[0].user.id.clone());
            switch_account_with(&restarted, &server_id, &user_id).await.unwrap();
            let servers = servers_with(&restarted);
            assert!(
                servers.iter().any(|v| v.server.id == server_id && v.current && v.accounts.len() == 1),
                "the server isn't listed with its account"
            );
            let reach = ping_server_with(&restarted, &server_id).await.unwrap();
            assert!(reach.reachable && reach.last_seen.is_some(), "the server didn't answer a ping");
            println!("accounts: 1 saved, server listed and reachable");

            sign_out_with(&restarted).await.unwrap();
            assert!(no_saved_sessions(&accounts_path), "sign out left the sign-in saved");

            // Put the old token back: the server must have revoked it, and startup forgets it.
            write_private(&accounts_path, &saved).unwrap();
            let s = startup_with(&open()).await.unwrap();
            assert!(s.account.is_none(), "token still valid after sign out");
            assert!(no_saved_sessions(&accounts_path), "revoked session was not removed");
            assert_eq!(s.last_address.as_deref(), Some(check.server.address.as_str()));

            // An earlier version's single session.json is folded into accounts.json.
            let legacy = dir.join("session.json");
            write_private(&legacy, &serde_json::to_vec(&sessions[0]).unwrap()).unwrap();
            let migrated = open().saved_sessions();
            assert!(migrated.sessions.len() == 1 && migrated.current.is_some(), "session.json wasn't folded in");
            assert!(!legacy.exists(), "session.json was left behind");
            open().update_sessions(|saved| *saved = SavedSessions::default()).unwrap();

            // Not staying signed in keeps the token in memory only.
            let jf = open();
            check_server_with(&jf, address.clone()).await.unwrap();
            sign_in_with(&jf, user.clone(), pass.clone(), false).await.unwrap();
            assert!(no_saved_sessions(&accounts_path), "token written despite Stay signed in being off");
            assert!(saved_accounts_with(&jf).iter().any(|a| a.current), "the unsaved sign-in isn't listed as in use");

            // Downloads: an episode's facts, a ranged request to carry on a partial file, and the
            // resume point written back exactly as it was read (so the account's data is untouched).
            let uid = jf.user_id().unwrap();
            // The first episode with a file behind it: a library can list episodes it has no file for.
            let page: serde_json::Value = jf
                .get_json(format!("/Items?userId={uid}&recursive=true&includeItemTypes=Episode&isMissing=false&fields=MediaSources&limit=50"))
                .await
                .unwrap();
            let listed = page["Items"].as_array().cloned().unwrap_or_default();
            let with_file = listed
                .iter()
                .find(|item| item["MediaSources"][0]["Size"].as_i64().is_some_and(|size| size > 0))
                .expect("the test account sees no episode with a file");
            let episode = with_file["Id"].as_str().unwrap().to_string();
            let facts = crate::playback::download_facts_with(&jf, &episode).await.unwrap();
            assert_eq!(facts.kind, "Episode");
            assert!(!facts.media_source_id.is_empty() && facts.size.is_some(), "no file facts for the episode");
            let mut res = jf.transfer(&format!("/Items/{episode}/Download"), 1000).await.unwrap();
            assert_eq!(res.status().as_u16(), 206, "the download didn't answer a range");
            assert_eq!(res.content_length().map(|len| len + 1000), facts.size, "the range didn't start at byte 1000");
            assert!(res.chunk().await.unwrap().is_some_and(|c| !c.is_empty()), "the ranged download sent nothing");
            drop(res);
            let user_data = format!("/UserItems/{episode}/UserData?userId={uid}");
            let before: serde_json::Value = jf.get_json(user_data.clone()).await.unwrap();
            let same = json!({
                "PlaybackPositionTicks": before["PlaybackPositionTicks"],
                "Played": before["Played"],
                "LastPlayedDate": before["LastPlayedDate"],
            });
            jf.post_empty(&user_data, same).await.unwrap();
            let after: serde_json::Value = jf.get_json(user_data).await.unwrap();
            assert_eq!(after["PlaybackPositionTicks"], before["PlaybackPositionTicks"], "writing the resume point back changed it");
            assert_eq!(after["Played"], before["Played"]);
            let refused = crate::playback::download_facts_with(&jf, with_file["SeriesId"].as_str().unwrap_or("x")).await;
            assert!(matches!(refused, Err(Error::Download(_)) | Err(Error::Status(_))), "a show was accepted as one download");

            // Live updates: the socket takes the sign-in header, and hears of a favourite changing.
            // The favourite is put back as it was straight after.
            let access = jf.socket_access().unwrap();
            let mut socket = crate::live::connect(&access).await.expect("the live updates socket refused the sign-in");
            let was_favorite = before["IsFavorite"].as_bool().unwrap_or(false);
            crate::home::set_favorite_with(&jf, episode.clone(), !was_favorite).await.unwrap();
            let heard = crate::live::wait_for_user_data(&mut socket, &uid, &episode, Duration::from_secs(15)).await;
            crate::home::set_favorite_with(&jf, episode.clone(), was_favorite).await.unwrap();
            let change = heard.expect("the server didn't tell the socket about the favourite");
            assert_eq!(serde_json::to_value(&change).unwrap()["favorite"], !was_favorite);

            sign_out_with(&jf).await.unwrap();

            // Removing the server forgets it.
            assert!(!remove_server_with(&jf, &server_id).await.unwrap(), "removing a server signed nobody out");
            assert!(servers_with(&jf).is_empty(), "the removed server is still listed");
        });
    }

    #[test]
    fn addresses() {
        assert_eq!(
            candidates("192.168.1.20:8097").unwrap(),
            vec!["http://192.168.1.20:8097"]
        );
        assert_eq!(
            candidates("http://192.168.1.20:8097/web/#/home").unwrap(),
            vec!["http://192.168.1.20:8097"]
        );
        assert_eq!(
            candidates("jellyfin.home").unwrap(),
            vec!["http://jellyfin.home", "http://jellyfin.home:8096"]
        );
        assert_eq!(
            candidates("https://media.example.com/jellyfin/web/index.html").unwrap(),
            vec!["https://media.example.com/jellyfin"]
        );
        assert!(candidates("").is_err());
        assert!(candidates("ftp://host").is_err());
    }
}
