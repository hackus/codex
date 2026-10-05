//! Verifies detached background children stay console-free through std and Tokio launches.

use super::background_command;
use pretty_assertions::assert_eq;
use std::os::windows::process::CommandExt;
use std::process::Command;

#[tokio::test]
async fn background_commands_have_no_console_under_detached_parent() {
    const TEST_NAME: &str = "tests::background_commands_have_no_console_under_detached_parent";
    const PHASE: &str = "CODEX_TEST_BACKGROUND_COMMAND_PHASE";
    let executable = std::env::current_exe().expect("test executable");
    match std::env::var(PHASE).as_deref() {
        Ok("probe") => {
            #[link(name = "kernel32")]
            unsafe extern "system" {
                #[link_name = "GetConsoleWindow"]
                fn get_console_window() -> isize;
            }
            assert_eq!(unsafe { get_console_window() }, 0);
            println!("background stdout");
            eprintln!("background stderr");
        }
        Ok("detached") => {
            // Exercise the shell descendants used by PowerShell discovery, batch
            // shims, and background helpers, not just direct executable launches.
            let mut cmd = background_command("cmd.exe");
            cmd.args(["/d", "/s", "/c"])
                .raw_arg(format!(
                    "\"\"{}\" --exact {TEST_NAME} --nocapture\"",
                    executable.display()
                ))
                .env(PHASE, "probe");
            let output = cmd.output().expect("run cmd descendant");
            assert!(output.status.success(), "cmd descendant failed: {output:?}");
            assert!(String::from_utf8_lossy(&output.stdout).contains("background stdout"));
            assert!(String::from_utf8_lossy(&output.stderr).contains("background stderr"));

            let executable_literal = executable.display().to_string().replace('\'', "''");
            let output = background_command("powershell.exe")
                .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
                .arg(format!(
                    "& '{executable_literal}' --exact '{TEST_NAME}' --nocapture; exit $LASTEXITCODE"
                ))
                .env(PHASE, "probe")
                .output()
                .expect("run PowerShell descendant");
            assert!(
                output.status.success(),
                "PowerShell descendant failed: {output:?}"
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("background stdout"));
            assert!(String::from_utf8_lossy(&output.stderr).contains("background stderr"));

            let status = background_command("cmd.exe")
                .args(["/d", "/c", "exit 23"])
                .status()
                .expect("run failing background command");
            assert_eq!(status.code(), Some(23));
            for asynchronous in [false, true] {
                let mut command = background_command(&executable);
                command
                    .args(["--exact", TEST_NAME, "--nocapture"])
                    .env(PHASE, "probe");
                let output = if asynchronous {
                    tokio::process::Command::from(command).output().await
                } else {
                    command.output()
                }
                .expect("run background command");
                assert!(output.status.success(), "probe failed: {output:?}");
                assert!(String::from_utf8_lossy(&output.stdout).contains("background stdout"));
                assert!(String::from_utf8_lossy(&output.stderr).contains("background stderr"));
            }
        }
        _ => {
            let output = Command::new(executable)
                .args(["--exact", TEST_NAME, "--nocapture"])
                .env(PHASE, "detached")
                .creation_flags(/*flags*/ 0x0000_0008) // DETACHED_PROCESS
                .output()
                .expect("spawn detached parent");
            assert!(
                output.status.success(),
                "detached parent failed: {output:?}"
            );
        }
    }
}
