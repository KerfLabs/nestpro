//! Where the original keeps its database, and how to reach it: the SQL Server
//! instance and database named in Expert's own settings (LANTEKIN.CNI section R,
//! "PARAMETROS DE CONEXION CON LA BASE DE DATOS"), the machines it lists
//! (DBConnect.lst) and the active one (LANTEKPR.CFG A000), and the connection
//! string Expert builds from them (DB2, CN1–CN2). See MECHANICS.md
//! "Database connection".

use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::time::Duration;

/// SQL Server Browser (resolves a named instance to its TCP port).
pub const SQL_BROWSER_PORT: u16 = 1434;
/// Default SQL Server TCP port.
pub const SQL_DEFAULT_PORT: u16 = 1433;
/// Seconds to wait for the server in "Test connection".
pub const TEST_TIMEOUT_S: u64 = 3;

/// Expert's database settings, read from its install folder. The password is
/// never read into the app — only whether one is set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LantekDb {
    /// R001: SQL Server, as `host\instance` (or `host`).
    pub server: String,
    /// R002: database (shown in Expert's title bar in brackets).
    pub database: String,
    /// R003: user; empty means Windows sign-in.
    pub user: String,
    /// R004 is set.
    pub has_password: bool,
    /// R005 / R008: the database folder on the server and its share.
    pub server_path: String,
    pub share: String,
    /// DBConnect.lst: machines defined in the database.
    pub machines: Vec<String>,
    /// LANTEKPR.CFG A000: the active machine.
    pub active_machine: String,
    /// The machine seed (LANTEKPR.SEM here): technology config (C000), post
    /// config (C001) and the programs run in order (Z lines: name, time
    /// calculation 0-3, writes the CNC 0/1).
    pub seed_cft: String,
    pub seed_cff: String,
    pub seed_programs: Vec<(String, u8, u8)>,
}

/// The quoted value of a `Kxxx "value" ; comment` line.
fn quoted(line: &str) -> Option<String> {
    let a = line.find('"')?;
    let b = line[a + 1..].find('"')? + a + 1;
    Some(line[a + 1..b].to_string())
}

fn read_latin1(p: &Path) -> Option<String> {
    std::fs::read(p).ok().map(|b| b.iter().map(|&c| c as char).collect())
}

/// Read Expert's settings from its install folder.
pub fn read(dir: &Path) -> Option<LantekDb> {
    let cni = read_latin1(&dir.join("LANTEKIN.CNI"))?;
    let mut db = LantekDb::default();
    for line in cni.lines() {
        let key = line.split_whitespace().next().unwrap_or("");
        match key {
            "R001" => db.server = quoted(line).unwrap_or_default(),
            "R002" => db.database = quoted(line).unwrap_or_default(),
            "R003" => db.user = quoted(line).unwrap_or_default(),
            "R004" => db.has_password = quoted(line).is_some_and(|s| !s.is_empty()),
            "R005" => db.server_path = quoted(line).unwrap_or_default(),
            "R008" => db.share = quoted(line).unwrap_or_default(),
            _ => {}
        }
    }
    if let Some(lst) = read_latin1(&dir.join("DBConnect.lst")) {
        db.machines = machines_for(&lst, &db.server, &db.database);
    }
    if let Some(sem) = read_latin1(&dir.join("LANTEKPR.SEM")) {
        for line in sem.lines() {
            let key = line.split_whitespace().next().unwrap_or("");
            match key {
                "C000" => db.seed_cft = quoted(line).unwrap_or_default(),
                "C001" => db.seed_cff = quoted(line).unwrap_or_default(),
                k if k.starts_with('Z') && k.len() == 4 => {
                    if let Some(name) = quoted(line) {
                        let after = line[line.rfind('"').unwrap_or(0) + 1..].split(';').next().unwrap_or("");
                        let nums: Vec<u8> = after.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                        db.seed_programs.push((name, nums.first().copied().unwrap_or(0), nums.get(1).copied().unwrap_or(0)));
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(cfg) = read_latin1(&dir.join("LANTEKPR.CFG")) {
        db.active_machine = cfg.lines().find(|l| l.trim_start().starts_with("A000")).and_then(quoted).unwrap_or_default();
    }
    Some(db)
}

/// The machines DBConnect.lst lists for one database. As Expert reads it
/// (CN4): an `A` line names a
/// database as "server,database" (compared ignoring case), the `B` lines
/// after it are its machines, and a `C` line ends the list. A file with no
/// `A` line lists every `B` line.
pub fn machines_for(lst: &str, server: &str, database: &str) -> Vec<String> {
    let key = format!("{server},{database}");
    let kind = |l: &str| l.trim_start().chars().next().unwrap_or(' ').to_ascii_uppercase();
    if !lst.lines().any(|l| kind(l) == 'A') {
        return lst.lines().filter(|l| kind(l) == 'B').filter_map(quoted).filter(|s| !s.is_empty()).collect();
    }
    let mut on = false;
    let mut out = vec![];
    for l in lst.lines() {
        match kind(l) {
            'A' => on = quoted(l).is_some_and(|a| a.eq_ignore_ascii_case(&key)),
            'B' if on => out.extend(quoted(l).filter(|s| !s.is_empty())),
            'C' if on => break,
            _ => {}
        }
    }
    out
}

impl LantekDb {
    /// The connection string Expert builds (DB2, CN1): Windows
    /// sign-in through OLE DB when there's no user, otherwise the ODBC "SQL
    /// Server" driver with user and password. The password is always masked here.
    pub fn connection_string(&self) -> String {
        let s = if self.user.is_empty() {
            format!("Provider=sqloledb;Data Source={};Initial Catalog={};Persist Security Info=False;Integrated Security=SSPI", self.server, self.database)
        } else {
            format!("DRIVER=SQL SERVER;SERVER={};DATABASE={};User ID={};Password={}", self.server, self.database, self.user, if self.has_password { "••••••" } else { "" })
        };
        // CN2: doubled quotes become single.
        s.replace("''", "'")
    }

    /// (host, instance) from `host\instance`.
    pub fn host_instance(&self) -> (String, Option<String>) {
        match self.server.split_once('\\') {
            Some((h, i)) => (h.to_string(), Some(i.to_string())),
            None => {
                let h = self.server.split(',').next().unwrap_or("").to_string();
                (h, None)
            }
        }
    }
}

/// Ask the SQL Server Browser on `host` which TCP port `instance` listens on
/// (SSRP CLNT_UCAST_INST: byte 0x04 + instance name; the reply lists `tcp;port`).
pub fn browser_port(host: &str, instance: &str) -> Result<u16, String> {
    let addr = (host, SQL_BROWSER_PORT).to_socket_addrs().map_err(|e| format!("can't find {host}: {e}"))?.next().ok_or(format!("can't find {host}"))?;
    let sock = UdpSocket::bind(if addr.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" }).map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(Duration::from_secs(TEST_TIMEOUT_S))).ok();
    let mut msg = vec![0x04u8];
    msg.extend_from_slice(instance.as_bytes());
    sock.send_to(&msg, addr).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 4096];
    let (n, _) = sock.recv_from(&mut buf).map_err(|_| format!("no answer from the SQL Server Browser on {host} (UDP {SQL_BROWSER_PORT})"))?;
    parse_browser_reply(&buf[..n]).ok_or_else(|| format!("instance {instance} has no TCP port listed"))
}

/// The TCP port in a SQL Server Browser reply (`...;tcp;1234;...`).
pub fn parse_browser_reply(reply: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(reply.get(3..).unwrap_or(&[]));
    let parts: Vec<&str> = text.split(';').collect();
    parts.windows(2).find(|w| w[0].eq_ignore_ascii_case("tcp")).and_then(|w| w[1].trim().parse().ok())
}

/// Test whether the database server can be reached from this PC: find the
/// instance's port, then open a TCP connection to it. (Signing in and reading
/// tables is not done here.)
pub fn test_reach(db: &LantekDb) -> String {
    let (host, inst) = db.host_instance();
    if host.is_empty() {
        return "No server is set.".into();
    }
    let port = match &inst {
        Some(i) => match browser_port(&host, i) {
            Ok(p) => p,
            Err(e) => return format!("✗ {e}."),
        },
        None => SQL_DEFAULT_PORT,
    };
    let addrs: Vec<SocketAddr> = match (host.as_str(), port).to_socket_addrs() {
        Ok(a) => a.collect(),
        Err(e) => return format!("✗ can't find {host}: {e}."),
    };
    for a in &addrs {
        if TcpStream::connect_timeout(a, Duration::from_secs(TEST_TIMEOUT_S)).is_ok() {
            return format!("✓ {} answers on TCP {port}{}.", host, inst.map_or(String::new(), |i| format!(" (instance {i})")));
        }
    }
    format!("✗ {host} doesn't accept connections on TCP {port}.")
}

#[cfg(test)]
mod db_tests {
    use super::*;

    #[test]
    fn reads_settings_and_builds_expert_connection_strings() {
        let dir = std::env::temp_dir().join("nestpro_db_test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("LANTEKIN.CNI"), "R001 \"srv\\Lantek\"   ; x\nR002 \"SHOP\" ; db\nR003 \"\" ; Username.\nR004 \"\" ; Password.\nR008 \"share$\" ;\n").unwrap();
        std::fs::write(dir.join("DBConnect.lst"), "A000 \"OTHER,DB\" ;\nB000 \"Punch\" ;\nC000 \"\" ;\nA000 \"SRV\\LANTEK,shop\" ;\nB000 \"LC5 A\" ;\nB000 \"LC5 B\" ;\nC000 \"\" ;\n").unwrap();
        std::fs::write(dir.join("LANTEKPR.CFG"), "#\nA000 \"LC5 B\"\n").unwrap();
        std::fs::write(dir.join("LANTEKPR.SEM"), "C000 \"X.CFT\" ; t\nC001 \"X.CFF\" ; p\nZ001 \"CALCTIME.EXE\" 3 0    ; Time\nZ002 \"POSTPROC.EXE\" 3 1    ; Post\n").unwrap();
        let db = read(&dir).unwrap();
        assert_eq!(db.server, "srv\\Lantek");
        assert_eq!(db.database, "SHOP");
        assert_eq!(db.machines, vec!["LC5 A", "LC5 B"]);
        assert_eq!(db.active_machine, "LC5 B");
        assert_eq!((db.seed_cft.as_str(), db.seed_cff.as_str()), ("X.CFT", "X.CFF"));
        assert_eq!(db.seed_programs, vec![("CALCTIME.EXE".to_string(), 3, 0), ("POSTPROC.EXE".to_string(), 3, 1)]);
        assert_eq!(db.host_instance(), ("srv".to_string(), Some("Lantek".to_string())));
        // No user: Windows sign-in through OLE DB, like Expert.
        assert!(db.connection_string().starts_with("Provider=sqloledb;Data Source=srv\\Lantek;Initial Catalog=SHOP;"));
        assert!(db.connection_string().ends_with("Integrated Security=SSPI"));
        // With a user: the ODBC SQL Server driver; the password is never shown.
        let mut u = db.clone();
        u.user = "lantek".into();
        u.has_password = true;
        assert!(u.connection_string().starts_with("DRIVER=SQL SERVER;SERVER=srv\\Lantek;DATABASE=SHOP;User ID=lantek;Password="));
        assert!(!u.connection_string().contains("secret"));
    }

    #[test]
    fn reads_the_instance_port_from_a_browser_reply() {
        let mut r = vec![0x05u8, 0x40, 0x00];
        r.extend_from_slice(b"ServerName;RCI;InstanceName;LANTEK;IsClustered;No;Version;15.0;tcp;49733;;");
        assert_eq!(parse_browser_reply(&r), Some(49733));
        assert_eq!(parse_browser_reply(&[5, 0, 0]), None);
    }
}
