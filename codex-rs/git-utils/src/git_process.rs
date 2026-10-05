use std::process::Output;
use std::process::Stdio;
use std::time::Duration;

use codex_protocol::shell_environment::scrub_non_inheritable_env_vars;
#[cfg(windows)]
use codex_utils_pty::JobObject;
#[cfg(unix)]
use codex_utils_pty::process_group::kill_process_group;
use tokio::process::Child;
use tokio::process::Command;
use tokio::time::timeout;

struct KillGitProcessTreeOnDrop {
    #[cfg(unix)]
    process_id: u32,
    #[cfg(windows)]
    job: Option<JobObject>,
    #[cfg(unix)]
    armed: bool,
}

#[cfg(unix)]
impl Drop for KillGitProcessTreeOnDrop {
    fn drop(&mut self) {
        if self.armed {
            let _ = kill_process_group(self.process_id);
        }
    }
}

#[cfg(windows)]
fn log_windows_diagnostic(message: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let path = std::env::temp_dir().join("codex-git-diag.txt");

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{message}");
    }
}

#[cfg(windows)]
fn dump_windows_console_state() {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    type Hwnd = *mut c_void;

    const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleWindow() -> Hwnd;
        fn GetStdHandle(nStdHandle: u32) -> Handle;
        fn GetFileType(hFile: Handle) -> u32;
        fn GetConsoleMode(
            hConsoleHandle: Handle,
            lpMode: *mut u32,
        ) -> i32;
        fn GetCurrentProcessId() -> u32;
    }

    unsafe fn describe_handle(name: &str, handle_id: u32) {
        let handle = unsafe { GetStdHandle(handle_id) };

        if handle.is_null() {
            log_windows_diagnostic(&format!(
                "[CODEX-GIT-DIAG] {name}: handle=NULL"
            ));
            return;
        }

        let file_type = unsafe { GetFileType(handle) };

        let mut mode = 0u32;
        let console_mode_result =
            unsafe { GetConsoleMode(handle, &mut mode) };

        log_windows_diagnostic(&format!(
            "[CODEX-GIT-DIAG] {name}: \
             handle={handle:p} \
             file_type={file_type} \
             console_mode_ok={} \
             console_mode=0x{mode:08x}",
            console_mode_result != 0
        ));
    }

    unsafe {
        let pid = GetCurrentProcessId();
        let console_window = GetConsoleWindow();

        log_windows_diagnostic(
            "[CODEX-GIT-DIAG] ================================"
        );

        log_windows_diagnostic(&format!(
            "[CODEX-GIT-DIAG] Codex PID={pid}"
        ));

        log_windows_diagnostic(&format!(
            "[CODEX-GIT-DIAG] GetConsoleWindow={console_window:p}"
        ));

        describe_handle("STDIN ", STD_INPUT_HANDLE);
        describe_handle("STDOUT", STD_OUTPUT_HANDLE);
        describe_handle("STDERR", STD_ERROR_HANDLE);

        log_windows_diagnostic(
            "[CODEX-GIT-DIAG] ================================"
        );
    }
}

fn spawn_git_command(command: &mut Command) -> Option<(Child, KillGitProcessTreeOnDrop)> {
    scrub_non_inheritable_env_vars(command.as_std_mut());

    #[cfg(unix)]
    command.process_group(0);

    command.kill_on_drop(true);

    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    dump_windows_console_state();

    #[cfg(windows)]
    let (child, job) = JobObject::spawn_background(command).ok()?;

    #[cfg(not(windows))]
    let child = command.spawn().ok()?;

    #[cfg(windows)]
    log_windows_diagnostic(&format!(
        "[CODEX-GIT-DIAG] spawned git child pid={:?}",
        child.id()
    ));

    let process_tree = KillGitProcessTreeOnDrop {
        #[cfg(unix)]
        process_id: child.id()?,

        #[cfg(windows)]
        job,

        #[cfg(unix)]
        armed: true,
    };

    Some((child, process_tree))
}

async fn wait_for_git_command_with_timeout_output(
    child: Child,
    process_tree: KillGitProcessTreeOnDrop,
    timeout_duration: Duration,
) -> Option<Output> {
    #[cfg(unix)]
    let mut process_tree = process_tree;

    let result = timeout(timeout_duration, child.wait_with_output()).await;

    match result {
        Ok(Ok(output)) => {
            #[cfg(windows)]
            if let Some(job) = &process_tree.job {
                job.preserve_descendants().ok()?;
            }

            #[cfg(unix)]
            {
                process_tree.armed = false;
            }

            Some(output)
        }
        _ => None,
    }
}

pub(crate) async fn run_git_command_with_timeout_output(
    command: &mut Command,
    timeout_duration: Duration,
) -> Option<Output> {
    let (child, process_tree) = spawn_git_command(command)?;
    wait_for_git_command_with_timeout_output(child, process_tree, timeout_duration).await
}

#[cfg(test)]
#[path = "git_process_tests.rs"]
mod tests;