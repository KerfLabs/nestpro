//! Read-only browsing of Expert's SQL Server database. Queries run through
//! Windows' own .NET SqlClient (in Windows PowerShell), so nothing extra has
//! to be installed; the app only ever sends the SELECTs built here — no
//! free SQL, nothing that writes. Table meanings come from the queries
//! Expert runs (see MECHANICS.md "Database tables", DT1–DT9).

use std::process::{Command, Stdio};

/// Rows shown per table.
pub const BROWSE_ROWS: usize = 200;
/// Seconds to wait to connect: Expert leaves ADO's connection timeout at its
/// default, 15 s (rule CN3).
pub const CONNECT_TIMEOUT_S: u32 = 15;
/// Seconds a query may run: Expert sets the command timeout to 0x258 = 600 s
/// (rule CN3).
pub const QUERY_TIMEOUT_S: u32 = 600;
/// Longest cell text shown in the grid (full text on hover).
pub const CELL_CHARS: usize = 40;

/// Expert's main tables (names from Expert's queries), with what they
/// hold and the column to sort newest first.
pub const LANTEK_TABLES: &[(&str, &str, &str)] = &[
    ("DIS_MMNN_MMOO_00000200", "Jobs (JobRef, JobName, CDate)", "CDate DESC"),
    ("DIS_NEST_NEST_00000100", "Nestings / sheets (NstRef, JobRef, WrkRef machine, MatRef, SThickness, CNC, Quantity)", "RecID DESC"),
    ("DIS_NEST_NEST_00000500", "Parts on each nesting (NstRef, PrdRefDst)", "RecID DESC"),
    ("MMNN_MMOO_00000300", "Parts ordered in each job (DIS_JobRef, PrdRefDst, Quantity, WrkRef)", "RecID DESC"),
    ("PPRR_PPRR_00000100", "Products / parts (PrdRef, DIS_PCLASS, StdCost)", "RecID DESC"),
    ("MMNN_OOPP_00000100", "Work centres / machines (WrkRef)", "RecID DESC"),
    ("DIS_MMTT_MMTT_00000100", "Materials (MatRef)", "RecID DESC"),
];

/// A query result: column names and rows of text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub cols: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Parse the helper's output: tab-separated, first line the column names.
pub fn parse_tsv(text: &str) -> Table {
    let mut lines = text.lines();
    let cols: Vec<String> = lines.next().map(|l| l.split('\t').map(str::to_string).collect()).unwrap_or_default();
    let rows = lines.filter(|l| !l.is_empty()).map(|l| l.split('\t').map(str::to_string).collect()).collect();
    Table { cols, rows }
}

/// Is this a plain table name (letters, digits, underscore)? Only such names
/// are put into a query, in brackets.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The query listing every table with its row count.
pub fn tables_sql() -> String {
    "SELECT t.name AS [Table], SUM(p.rows) AS [Rows] FROM sys.tables t JOIN sys.partitions p ON p.object_id = t.object_id AND p.index_id IN (0, 1) GROUP BY t.name ORDER BY t.name".into()
}

/// The query showing the first rows of a table (newest first for Expert's
/// own tables).
pub fn browse_sql(table: &str) -> Option<String> {
    if !safe_name(table) {
        return None;
    }
    let order = LANTEK_TABLES.iter().find(|t| t.0.eq_ignore_ascii_case(table)).map(|t| format!(" ORDER BY {}", t.2)).unwrap_or_default();
    Some(format!("SELECT TOP {BROWSE_ROWS} * FROM [{table}]{order}"))
}

/// A text value for a query: N'…' with quotes doubled.
pub fn nq(s: &str) -> String {
    format!("N'{}'", s.replace('\'', "''"))
}

/// Jobs, newest first, with how many parts they order and how many
/// nestings they have (columns as in Expert's queries, DT1–DT3: JobRef,
/// JobName, CDate; MMNN_MMOO_00000300.DIS_JobRef / IsOutsourcing = '0';
/// DIS_NEST_NEST_00000100.JobRef).
pub fn jobs_sql() -> String {
    format!(
        "SELECT TOP {JOBS_MAX} T1.JobRef, T1.JobName, T1.CDate, \
         (SELECT COUNT(*) FROM MMNN_MMOO_00000300 M WHERE M.DIS_JobRef = T1.JobRef AND M.IsOutsourcing = '0') AS Parts, \
         (SELECT COUNT(*) FROM DIS_NEST_NEST_00000100 N WHERE N.JobRef = T1.JobRef) AS Nestings, \
         T2.BwsrID AS Folder \
         FROM DIS_MMNN_MMOO_00000200 T1 LEFT OUTER JOIN DIS_MMNN_BWSR_00000200 T2 ON T1.RecID = T2.RecordID AND T2.TblRef = 'DIS_MMNN_MMOO_00000200' \
         ORDER BY T1.CDate DESC"
    )
}

/// Most jobs read for the Job Explorer tree.
pub const JOBS_MAX: usize = 5000;

/// The Job Explorer's folders (rule DT8: NodeID, NodeName, ParentID and
/// BwsrType in DIS_MMNN_BWSR_00000100; rule DT9: a job's folder is
/// DIS_MMNN_BWSR_00000200.BwsrID for TblRef 'DIS_MMNN_MMOO_00000200' and the
/// job's RecordID).
pub fn folders_sql() -> String {
    "SELECT NodeID, NodeName, ParentID, BwsrType FROM DIS_MMNN_BWSR_00000100".into()
}

/// One folder of the Job Explorer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Folder {
    pub id: String,
    pub name: String,
    /// Sub-folders (indexes into JobTree::folders) and jobs (row indexes).
    pub children: Vec<usize>,
    pub jobs: Vec<usize>,
}

/// The Job Explorer tree: folders, top folders, and jobs in no folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JobTree {
    pub folders: Vec<Folder>,
    pub roots: Vec<usize>,
    pub loose: Vec<usize>,
}

fn col(t: &Table, name: &str) -> Option<usize> {
    t.cols.iter().position(|c| c.eq_ignore_ascii_case(name))
}

/// Build the tree. Only folders of the tree type that holds jobs are kept
/// (the same table also holds the products' folders); siblings are sorted
/// by name, case-insensitively, as in Expert's Job Explorer.
pub fn job_tree(folders: &Table, jobs: &Table) -> JobTree {
    let (fi, fn_, fp, ft) = (col(folders, "NodeID"), col(folders, "NodeName"), col(folders, "ParentID"), col(folders, "BwsrType"));
    let jf = col(jobs, "Folder");
    let get = |r: &Vec<String>, k: Option<usize>| k.and_then(|k| r.get(k)).cloned().unwrap_or_default();
    let job_folder: Vec<String> = jobs.rows.iter().map(|r| get(r, jf)).collect();
    // Tree types that hold jobs.
    let types: Vec<String> = folders.rows.iter().filter(|r| job_folder.contains(&get(r, fi))).map(|r| get(r, ft)).collect();
    let kept: Vec<&Vec<String>> = folders.rows.iter().filter(|r| types.is_empty() || types.contains(&get(r, ft))).collect();
    let mut tree = JobTree { folders: kept.iter().map(|r| Folder { id: get(r, fi), name: get(r, fn_), ..Default::default() }).collect(), ..Default::default() };
    let idx = |id: &str, t: &JobTree| t.folders.iter().position(|f| f.id == id);
    for (k, r) in kept.iter().enumerate() {
        match idx(&get(r, fp), &tree) {
            Some(p) if p != k => tree.folders[p].children.push(k),
            _ => tree.roots.push(k),
        }
    }
    for (j, f) in job_folder.iter().enumerate() {
        match idx(f, &tree) {
            Some(k) if !f.is_empty() => tree.folders[k].jobs.push(j),
            _ => tree.loose.push(j),
        }
    }
    let names: Vec<String> = tree.folders.iter().map(|f| f.name.to_lowercase()).collect();
    tree.roots.sort_by(|a, b| names[*a].cmp(&names[*b]));
    for f in tree.folders.iter_mut() {
        f.children.sort_by(|a, b| names[*a].cmp(&names[*b]));
    }
    tree
}

/// Most jobs listed for a search, and in one open folder, so long lists stay
/// quick to draw and scroll.
pub const SEARCH_MAX: usize = 300;
pub const FOLDER_MAX: usize = 500;

/// Folders whose name, and jobs whose reference or name, contain the text
/// (ignoring case). Folders come with everything inside them.
pub fn search(tree: &JobTree, jobs: &Table, needle: &str) -> (Vec<usize>, Vec<usize>) {
    let n = needle.to_lowercase();
    let fs = (0..tree.folders.len()).filter(|&k| tree.folders[k].name.to_lowercase().contains(&n)).collect();
    let js = (0..jobs.rows.len()).filter(|&k| jobs.rows[k].iter().take(2).any(|v| v.to_lowercase().contains(&n))).collect();
    (fs, js)
}

/// What can be shown for one job.
pub const JOB_VIEWS: [&str; 3] = ["Parts ordered", "Nestings & CNC programs", "Parts on its nestings"];

/// The query for one of JOB_VIEWS for a job.
pub fn job_sql(view: usize, job: &str) -> String {
    let j = nq(job);
    match view {
        0 => format!("SELECT TOP 500 * FROM MMNN_MMOO_00000300 WHERE DIS_JobRef = {j} AND IsOutsourcing = '0' ORDER BY IOrder"),
        1 => format!("SELECT TOP 500 * FROM DIS_NEST_NEST_00000100 WHERE JobRef = {j} ORDER BY NOrder"),
        _ => format!("SELECT TOP 500 T2.* FROM DIS_NEST_NEST_00000500 T2 JOIN DIS_NEST_NEST_00000100 T1 ON T1.NstRef = T2.NstRef WHERE T1.JobRef = {j} ORDER BY T2.NstRef"),
    }
}

/// What the table holds, for Expert's own tables.
pub fn describe(table: &str) -> Option<&'static str> {
    LANTEK_TABLES.iter().find(|t| t.0.eq_ignore_ascii_case(table)).map(|t| t.1)
}

/// The helper script: connects with SqlClient from the values in the
/// environment (so a password never appears on a command line), runs one
/// query and prints tab-separated text.
const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
try {
  $b = New-Object System.Data.SqlClient.SqlConnectionStringBuilder
  $b['Data Source'] = $env:NESTPRO_SERVER
  $b['Initial Catalog'] = $env:NESTPRO_DB
  $b['Connect Timeout'] = [int]$env:NESTPRO_CT
  $b['Application Name'] = 'nestpro (read-only)'
  if ($env:NESTPRO_USER) {
    $pw = $env:NESTPRO_PW
    if (-not $pw -and $env:NESTPRO_PWENC) {
      $ss = ConvertTo-SecureString $env:NESTPRO_PWENC
      $pw = [Runtime.InteropServices.Marshal]::PtrToStringBSTR([Runtime.InteropServices.Marshal]::SecureStringToBSTR($ss))
    }
    $b['User ID'] = $env:NESTPRO_USER; $b['Password'] = $pw
  } else { $b['Integrated Security'] = $true }
  $c = New-Object System.Data.SqlClient.SqlConnection $b.ConnectionString
  $c.Open()
  $cmd = $c.CreateCommand(); $cmd.CommandText = $env:NESTPRO_SQL; $cmd.CommandTimeout = [int]$env:NESTPRO_QT
  $r = $cmd.ExecuteReader()
  $sb = New-Object System.Text.StringBuilder
  $n = @(); for ($i = 0; $i -lt $r.FieldCount; $i++) { $n += $r.GetName($i) }
  [void]$sb.AppendLine(($n -join "`t"))
  while ($r.Read()) {
    $v = @()
    for ($i = 0; $i -lt $r.FieldCount; $i++) {
      $x = $r.GetValue($i)
      if ($x -is [DBNull]) { $x = '' } elseif ($x -is [byte[]]) { $x = "(binary $($x.Length) bytes)" }
      $v += (([string]$x) -replace "[`t`r`n]", ' ')
    }
    [void]$sb.AppendLine(($v -join "`t"))
  }
  $c.Close()
  [Console]::Out.Write($sb.ToString())
} catch {
  [Console]::Error.Write($_.Exception.Message)
  exit 1
}
"#;

/// Run a query on a saved connection. A typed password wins over the
/// remembered one, which is only decrypted inside the helper process.
pub fn run_conn(c: &crate::settings::Connection, password: &str, sql: &str) -> Result<Table, String> {
    if !cfg!(windows) {
        return Err("The Lantek database link runs through Windows PowerShell's SqlClient, so it works on Windows only.".into());
    }
    // The script goes in as -EncodedCommand (UTF-16LE, base64) so it runs as
    // one block; values travel in the environment.
    let utf16: Vec<u8> = SCRIPT.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &base64(&utf16)])
        .env("NESTPRO_SERVER", &c.server)
        .env("NESTPRO_DB", &c.database)
        .env("NESTPRO_USER", if c.windows { "" } else { c.user.as_str() })
        .env("NESTPRO_PW", password)
        .env("NESTPRO_PWENC", &c.password_enc)
        .env("NESTPRO_SQL", sql)
        .env("NESTPRO_CT", CONNECT_TIMEOUT_S.to_string())
        .env("NESTPRO_QT", QUERY_TIMEOUT_S.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start PowerShell: {e}"))?;
    drop(child.stdin.take());
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(parse_tsv(&String::from_utf8_lossy(&out.stdout)))
    } else {
        Err(clean_error(&String::from_utf8_lossy(&out.stderr)))
    }
}

/// SqlClient's message without PowerShell's progress records (CLIXML).
pub fn clean_error(stderr: &str) -> String {
    let mut e = stderr.to_string();
    for marker in ["#< CLIXML", "<Objs "] {
        if let Some(k) = e.find(marker) {
            e.truncate(k);
        }
    }
    let e = e.split_whitespace().collect::<Vec<_>>().join(" ");
    if e.is_empty() {
        "the query failed".into()
    } else {
        e
    }
}

/// Plain advice for the errors people meet first.
pub fn advice(err: &str) -> Option<&'static str> {
    let e = err.to_lowercase();
    if e.contains("invalid object name") {
        Some("This database has no Lantek job tables — use All tables to look around it.")
    } else if e.contains("login failed") {
        Some("The server answered but refused this sign-in. Check the login and password in Settings → Connections (or switch to your Windows account if it has access).")
    } else if e.contains("network-related") || e.contains("server was not found") || e.contains("timeout") {
        Some("The server couldn't be reached — check the server name in Database connection and that this PC is on the shop network.")
    } else {
        None
    }
}

/// Standard base64.
pub fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                s.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// Cut a cell's text for the grid.
pub fn cell(s: &str) -> String {
    if s.chars().count() > CELL_CHARS {
        format!("{}…", s.chars().take(CELL_CHARS).collect::<String>())
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod dbq_tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn reports_the_servers_error_when_it_cannot_connect() {
        // Nothing listens on local port 1: SqlClient's own error comes back.
        let c = crate::settings::Connection { server: "tcp:127.0.0.1,1".into(), database: "x".into(), windows: true, ..Default::default() };
        let e = run_conn(&c, "", "SELECT 1").unwrap_err();
        assert!(!e.is_empty() && !e.contains("PowerShell"), "{e}");
    }

    #[test]
    fn builds_only_safe_selects_and_reads_results() {
        assert_eq!(browse_sql("DIS_MMNN_MMOO_00000200").unwrap(), "SELECT TOP 200 * FROM [DIS_MMNN_MMOO_00000200] ORDER BY CDate DESC");
        assert_eq!(browse_sql("SomeTable").unwrap(), "SELECT TOP 200 * FROM [SomeTable]");
        assert!(browse_sql("x]; DROP TABLE y").is_none());
        assert!(browse_sql("").is_none());
        assert!(tables_sql().starts_with("SELECT "));
        let t = parse_tsv("JobRef\tJobName\r\nJ1\tFrames\r\nJ2\t\r\n");
        assert_eq!(t.cols, vec!["JobRef", "JobName"]);
        assert_eq!(t.rows, vec![vec!["J1".to_string(), "Frames".into()], vec!["J2".to_string(), "".into()]]);
        assert_eq!(describe("dis_nest_nest_00000100").map(|d| d.starts_with("Nestings")), Some(true));
        assert_eq!(cell(&"x".repeat(50)).chars().count(), CELL_CHARS + 1);
        // Job values are quoted safely.
        assert_eq!(nq("O'Brien"), "N'O''Brien'");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(clean_error("Login failed for user 'lantek'.#< CLIXML\r\n<Objs Version=..."), "Login failed for user 'lantek'.");
        assert!(advice("Login failed for user 'lantek'.").is_some());
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert!(job_sql(1, "J-1").ends_with("WHERE JobRef = N'J-1' ORDER BY NOrder"));
        assert!(jobs_sql().contains("LEFT OUTER JOIN DIS_MMNN_BWSR_00000200 T2 ON T1.RecID = T2.RecordID AND T2.TblRef = 'DIS_MMNN_MMOO_00000200'"));
        // The Job Explorer tree: jobs' folder type kept, names sorted ignoring case.
        let folders = parse_tsv("NodeID\tNodeName\tParentID\tBwsrType\n1\tWESTON\t0\t1\n2\tandrew\t0\t1\n3\tPITS\t2\t1\n9\tSheet metal\t0\t2\n");
        let jobs = parse_tsv("JobRef\tJobName\tCDate\tParts\tNestings\tFolder\nJ1\tA\t\t1\t1\t3\nJ2\tB\t\t0\t0\t\n");
        let t = job_tree(&folders, &jobs);
        assert_eq!(t.roots.iter().map(|&k| t.folders[k].name.as_str()).collect::<Vec<_>>(), vec!["andrew", "WESTON"]);
        let a = t.roots[0];
        assert_eq!(t.folders[t.folders[a].children[0]].name, "PITS");
        assert_eq!(t.folders[t.folders[a].children[0]].jobs, vec![0]);
        assert_eq!(t.loose, vec![1]);
        // Search finds folders by name and jobs by reference or name.
        let (fs, js) = search(&t, &jobs, "pit");
        assert_eq!(fs.iter().map(|&k| t.folders[k].name.as_str()).collect::<Vec<_>>(), vec!["PITS"]);
        assert!(js.is_empty());
        assert_eq!(search(&t, &jobs, "j2").1, vec![1]);
    }
}
