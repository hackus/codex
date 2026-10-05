# Windows background process audit

Background console-subsystem executables must use `background_command` (convert
with `tokio::process::Command::from` for async execution), or a shared launcher
that establishes the same policy before spawning. Redirecting stdio alone does
not prevent Windows from allocating a console when the parent is detached.
Later `creation_flags` calls replace earlier flags; contained background spawns
must retain `CREATE_NO_WINDOW` when adding suspension or falling back.

## Additional paths covered by this change

| Launch path | Policy |
| --- | --- |
| shell-command PowerShell discovery and availability probes | Shared background command |
| git-utils synchronous patch/root/staging commands | Shared background command |
| worktree Git commands | Shared background command |
| cloud-tasks Git remote discovery | Shared background command |
| daemon managed-server capability probe | Shared background command |
| CLI desktop installation query and URL-launch PowerShell helper | Shared background command; requested GUI still opens |
| TUI desktop URL helper and daemon-start request | Shared background command |
| LM Studio noninteractive model download | Shared background command; preserve output handles |
| protocol export Prettier helpers | Shared background command |
| Windows file-search fallback command | Shared background command |
| sandbox CreateProcessAsUserW with ConsoleMode::NoWindow | Honor console policy with explicit or inherited stdio |

## Existing protected routes reviewed

- `utils/pty/child_command.rs` sets `CREATE_NO_WINDOW` at construction. This
  covers pipe execution, MCP stdio servers, and filesystem helper processes.
- `utils/pty/win/job.rs` retains `CREATE_NO_WINDOW` for suspended job assignment
  and both job creation/assignment fallbacks. Async Git and MCP HTTP header
  helpers use this path.
- Hooks, PowerShell AST parsing, Code Mode host startup, environment probes,
  daemon identity/version probes and installers, sandbox setup/provisioning,
  runner logon, and registered runtime removal already suppress console windows.
- Plugin Git/npm operations, auth/credential helpers, exec-server transport,
  taskkill, rollout search, and doctor helpers already use the shared helper.
- Both raw ConPTY launch sites retain the fork's existing fixes. PTY creation
  and lifecycle are separate from ordinary background process construction.

## Intentional direct launches

- Managed daemon/updater roots use `DETACHED_PROCESS` (and appropriate job
  flags). Do not combine this with `CREATE_NO_WINDOW`, which Windows ignores
  when detaching. Their background children need their own console policy.
- External editors, user commands with inherited terminal stdio, CLI update
  interactions, and sandbox debug commands retain their terminal semantics.
- Elevated sandbox setup uses ShellExecuteExW with `SW_HIDE`; elevation consent
  is handled by Windows.
- Unix-only launchers, WSL interoperability helpers compiled on Linux, and test
  fixture launches are separate from the native Windows production paths.

## Verification

The process helper test launches a detached parent and checks `GetConsoleWindow`
in direct std/Tokio children and descendants of CMD and PowerShell. It also
checks stdout/stderr capture and nonzero exit status propagation. PTY tests cover
job creation/assignment failures, pipes with and without containment, and ConPTY
output lifetime. The fork release workflow runs these tests before building.

This audit covers Codex-controlled native Windows launch sites. A program that
explicitly creates its own GUI/console, or launches another detached process,
controls that separate behavior; process creation flags are not a global policy
inherited by every possible descendant.