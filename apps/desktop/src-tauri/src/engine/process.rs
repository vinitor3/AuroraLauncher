use std::ffi::OsStr;
use std::process::Command;

/// Start a child without allocating a console window on Windows. Explicitly
/// configured stdout/stderr pipes and files still work, as do graphical JVM
/// windows. Other platforms retain the standard `Command` behavior.
pub(super) fn background_command(program: impl AsRef<OsStr>) -> Command {
    let command = Command::new(program);
    #[cfg(target_os = "windows")]
    let command = {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = command;
        command.creation_flags(CREATE_NO_WINDOW);
        command
    };
    command
}

#[cfg(test)]
mod tests {
    use super::background_command;

    #[test]
    fn background_process_keeps_pipes_without_console() {
        const CHILD_MARKER: &str = "AURORA_BACKGROUND_PROCESS_TEST_CHILD";
        if std::env::var_os(CHILD_MARKER).is_some() {
            #[cfg(target_os = "windows")]
            {
                #[link(name = "kernel32")]
                extern "system" {
                    fn GetConsoleWindow() -> *mut std::ffi::c_void;
                }
                // SAFETY: GetConsoleWindow takes no arguments and only returns
                // the current process's console handle; no memory is accessed.
                assert!(unsafe { GetConsoleWindow() }.is_null());
            }
            println!("aurora-background-stdout");
            eprintln!("aurora-background-stderr");
            return;
        }

        let output = background_command(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "engine::process::tests::background_process_keeps_pipes_without_console",
                "--nocapture",
            ])
            .env(CHILD_MARKER, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("aurora-background-stdout"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("aurora-background-stderr"));
    }
}
