//! Programs to start along with QRZero (PstRotatorAz, a rig control server, a band decoder app...).
//! They start detached, without a console window on Windows, and never hold up QRZero's own start.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Where the list is kept.
pub const SETTING_KEY: &str = "startup_apps";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StartupApp {
    /// Stable id (for status); 0 gets one when saved.
    pub id: u64,
    pub enabled: bool,
    /// The program, e.g. `C:\Program Files (x86)\PstRotatorAz\PstRotatorAz.exe`.
    pub path: String,
    /// Arguments, as you'd type them after the program name.
    pub args: String,
    /// Leave it alone when a program with the same file name is already running (Windows only).
    pub skip_if_running: bool,
}

impl Default for StartupApp {
    fn default() -> Self {
        StartupApp { id: 0, enabled: true, path: String::new(), args: String::new(), skip_if_running: true }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AppStatus {
    pub ok: bool,
    pub text: String,
}

#[derive(Default)]
pub struct Startup {
    status: Mutex<BTreeMap<u64, AppStatus>>,
}

/// Gives new (and duplicated) entries ids and tidies the text.
pub fn normalize(mut apps: Vec<StartupApp>) -> Vec<StartupApp> {
    let mut next = apps.iter().map(|a| a.id).max().unwrap_or(0) + 1;
    let mut seen = std::collections::HashSet::new();
    for a in &mut apps {
        a.path = a.path.trim().trim_matches('"').to_string();
        a.args = a.args.trim().to_string();
        if a.id == 0 || !seen.insert(a.id) {
            a.id = next;
            seen.insert(next);
            next += 1;
        }
    }
    apps.retain(|a| !a.path.is_empty());
    apps
}

/// Splits an argument string the usual way: spaces separate, double quotes group.
/// (On Windows the string is passed through as typed instead.)
#[cfg_attr(windows, allow(dead_code))]
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut cur, mut quoted, mut any) = (String::new(), false, false);
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The program's file name, for the process list check.
fn exe_name(path: &str) -> String {
    Path::new(path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Whether a program by this file name is running. Only answered on Windows.
#[cfg(windows)]
pub fn is_running(path: &str) -> Option<bool> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let name = exe_name(path);
    if name.is_empty() {
        return None;
    }
    let out = Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    Some(process_list_has(&text, &name))
}

#[cfg(not(windows))]
pub fn is_running(_path: &str) -> Option<bool> {
    None
}

/// Looks for an image name in `tasklist /FO CSV /NH` output.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn process_list_has(csv: &str, name: &str) -> bool {
    csv.lines().filter_map(|l| l.trim().strip_prefix('"')?.split('"').next()).any(|image| image.eq_ignore_ascii_case(name))
}

/// Starts one program, detached. Returns what to show in Settings.
pub fn launch(app: &StartupApp) -> Result<String, String> {
    let path = app.path.trim().trim_matches('"');
    if path.is_empty() {
        return Err("no program given".into());
    }
    if app.skip_if_running && is_running(path) == Some(true) {
        return Ok(format!("{} was already running", exe_name(path)));
    }
    let mut cmd = Command::new(path);
    if let Some(dir) = Path::new(path).parent().filter(|d| d.is_dir()) {
        // Many Windows programs look for their settings next to the .exe.
        cmd.current_dir(dir);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
        // Windows programs parse their own command line, so pass it as typed.
        if !app.args.trim().is_empty() {
            cmd.raw_arg(app.args.trim());
        }
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::process::CommandExt;
        cmd.args(split_args(&app.args));
        // Its own process group, so a Ctrl+C in QRZero's terminal doesn't reach it.
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| format!("couldn't start {path}: {e}"))?;
    let pid = child.id();
    // Reap it when it exits (no zombies on Linux), and catch an immediate failure.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(format!("started (process {pid})"))
}

impl Startup {
    pub fn new() -> Arc<Self> {
        Arc::new(Startup::default())
    }

    pub fn status(&self) -> BTreeMap<u64, AppStatus> {
        self.status.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn forget_others(&self, apps: &[StartupApp]) {
        self.status.lock().unwrap_or_else(|p| p.into_inner()).retain(|id, _| apps.iter().any(|a| a.id == *id));
    }

    /// Starts one program now and records how it went.
    pub fn launch_one(&self, app: &StartupApp) -> AppStatus {
        let now = chrono::Local::now().format("%H:%M:%S");
        let status = match launch(app) {
            Ok(text) => AppStatus { ok: true, text: format!("{now}: {text}") },
            Err(e) => {
                tracing::warn!("startup app: {e}");
                AppStatus { ok: false, text: format!("{now}: {e}") }
            }
        };
        if app.id != 0 {
            self.status.lock().unwrap_or_else(|p| p.into_inner()).insert(app.id, status.clone());
        }
        status
    }

    /// Starts the enabled programs in the background, one after another.
    pub fn launch_all(self: &Arc<Self>, apps: Vec<StartupApp>) {
        let apps: Vec<_> = apps.into_iter().filter(|a| a.enabled && !a.path.trim().is_empty()).collect();
        if apps.is_empty() {
            return;
        }
        let me = self.clone();
        std::thread::spawn(move || {
            for app in apps {
                me.launch_one(&app);
                // A little space so programs that share a port or COM port start in order.
                std::thread::sleep(Duration::from_millis(300));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args() {
        assert_eq!(split_args(r#"-a "C:\Program Files\x.ini" b"#), ["-a", r"C:\Program Files\x.ini", "b"]);
        assert_eq!(split_args("  "), Vec::<String>::new());
        assert_eq!(split_args(r#"--name="" x"#), ["--name=", "x"]);
    }

    #[test]
    fn process_list() {
        let csv = "\"System Idle Process\",\"0\",\"Services\",\"0\",\"8 K\"\r\n\"PstRotatorAz.exe\",\"4242\",\"Console\",\"1\",\"30,000 K\"\r\n";
        assert!(process_list_has(csv, "pstrotatoraz.EXE"));
        assert!(!process_list_has(csv, "rigctld.exe"));
        assert!(!process_list_has("INFO: No tasks are running which match the specified criteria.", "x.exe"));
    }

    #[test]
    fn ids_and_blank_paths() {
        let apps = normalize(vec![
            StartupApp { id: 3, path: "\"C:\\x\\a.exe\"".into(), ..StartupApp::default() },
            StartupApp { path: "b".into(), ..StartupApp::default() },
            StartupApp { path: "  ".into(), ..StartupApp::default() },
        ]);
        assert_eq!(apps.iter().map(|a| (a.id, a.path.as_str())).collect::<Vec<_>>(), [(3, "C:\\x\\a.exe"), (4, "b")]);
    }

    #[test]
    fn missing_program_reports_an_error() {
        let s = Startup::default();
        let st = s.launch_one(&StartupApp { id: 1, path: "/no/such/program-qrzero-test".into(), skip_if_running: false, ..StartupApp::default() });
        assert!(!st.ok && st.text.contains("couldn't start"), "{}", st.text);
        assert!(!s.status()[&1].ok);
    }

    #[cfg(unix)]
    #[test]
    fn launches_detached_with_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.txt");
        let app = StartupApp {
            id: 1,
            path: "/bin/sh".into(),
            args: format!("-c \"echo started > '{}'\"", out.display()),
            skip_if_running: true,
            ..StartupApp::default()
        };
        let st = Startup::default().launch_one(&app);
        assert!(st.ok, "{}", st.text);
        for _ in 0..100 {
            if std::fs::read_to_string(&out).is_ok_and(|t| t.trim() == "started") {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the program didn't run");
    }
}
