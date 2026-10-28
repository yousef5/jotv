//! The helper programs JoTV runs: mpv, ffmpeg, ffprobe and yt-dlp.
//!
//! Windows has no package manager to install them from, so the Windows build
//! ships them in a `bin` folder next to JoTV.exe (see `tauri.windows.conf.json`
//! and `scripts/fetch-windows-tools.ps1`). Everywhere else they come from PATH.

use std::path::PathBuf;

/// Path to run `name` from: the bundled copy when there is one, else the bare
/// name so the system searches PATH.
pub fn program(name: &str) -> PathBuf {
    #[cfg(windows)]
    if let Some(exe) = bundled(name) {
        return exe;
    }
    PathBuf::from(name)
}

#[cfg(windows)]
fn bundled(name: &str) -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.join("bin");
    let exe = dir.join(format!("{name}.exe"));
    exe.is_file().then_some(exe)
}

/// Windows opens a console window for every console program a GUI app starts;
/// this keeps mpv, ffmpeg and yt-dlp from flashing one on screen.
pub trait NoConsole {
    fn no_console(&mut self) -> &mut Self;
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

impl NoConsole for std::process::Command {
    fn no_console(&mut self) -> &mut Self {
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(self, CREATE_NO_WINDOW);
        self
    }
}

impl NoConsole for tokio::process::Command {
    fn no_console(&mut self) -> &mut Self {
        #[cfg(windows)]
        self.creation_flags(CREATE_NO_WINDOW);
        self
    }
}

/// Makes `child` die with JoTV on Windows, like `PR_SET_PDEATHSIG` on Linux:
/// otherwise a crash leaves MPV playing audio with no window to show it in.
/// The process joins a job object that kills everything in it when JoTV's
/// handle to it closes, which Windows does on any exit.
#[cfg(windows)]
pub fn tie_to_app(child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    // HANDLE isn't Send; keep the raw value. Never closed: it must live as long as the app.
    static JOB: OnceLock<Option<isize>> = OnceLock::new();
    let job = JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(None, windows::core::PCWSTR::null()).ok()?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .ok()?;
        Some(job.0 as isize)
    });
    if let Some(job) = job {
        unsafe {
            AssignProcessToJobObject(HANDLE(*job as *mut _), HANDLE(child.as_raw_handle())).ok();
        }
    }
}

#[cfg(not(windows))]
pub fn tie_to_app(_: &std::process::Child) {}
