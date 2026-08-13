//! macOS process isolation using the Seatbelt sandbox framework.
//!
//! Strategy:
//!   1. `fork()` the child (all fallible allocations happen in the parent,
//!      so the child only runs async-signal-safe operations).
//!   2. In the child: apply `setrlimit` resource limits from the policy,
//!      then call `sandbox_init` with a policy-derived Seatbelt profile,
//!      then `execv`.
//!   3. The parent tracks the pid; `kill`/`is_alive` use `kill(2)` and
//!      `waitpid(2)`.
//!
//! Syscall-level filtering is not practical on macOS (no seccomp); the
//! Seatbelt profile covers the same intent via operation classes
//! (`network*`, `file-*`).

use std::ffi::{c_char, c_int, CStr, CString};

use crate::error::SandboxError;
use crate::policy::Policy;

// ── Sandbox framework FFI ────────────────────────────────────────
// sandbox_init(profile, flags, errorbuf) applies a Seatbelt profile to
// the current process. SANDBOX_PROFILE means the string is a raw SBPL
// profile rather than a named/builtin one.
#[link(name = "Sandbox", kind = "framework")]
extern "C" {
    fn sandbox_init(profile: *const c_char, flags: u64, errorbuf: *mut *mut c_char) -> c_int;
    fn sandbox_free_error(errorbuf: *mut c_char);
}

const SANDBOX_PROFILE: u64 = 3;

#[derive(Debug)]
pub(crate) struct ProcessHandle {
    pid: libc::pid_t,
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            libc::waitpid(self.pid, std::ptr::null_mut(), libc::WNOHANG);
        }
    }
}

// ── Policy → Seatbelt profile ────────────────────────────────────
//
// The profile starts from `(allow default)` (a working, unconfined
// process) and denies the operation classes the policy does not permit:
//   - networking: denied with `network*` when `allow_networking` is false
//   - filesystem: write/ioctl operations are denied when
//     `allow_filesystem` is false. Reads stay allowed so the dynamic
//     linker can still load system dylibs in the child.
fn build_profile(policy: &Policy) -> String {
    let mut profile = String::from("(version 1)\n(allow default)\n");
    if !policy.allow_networking {
        profile.push_str("(deny network*)\n");
    }
    if !policy.allow_filesystem {
        profile.push_str("(deny file-write*)\n(deny file-ioctl)\n");
    }
    profile
}

fn build_argv(program: &str, args: &[String]) -> Vec<CString> {
    let mut argv: Vec<CString> = Vec::new();
    argv.push(CString::new(program).expect("program name contains null byte"));
    for arg in args {
        argv.push(CString::new(arg.as_str()).expect("arg contains null byte"));
    }
    argv
}

/// Precomputed resource limits so the forked child performs no allocation.
#[derive(Default)]
struct PrecomputedLimits {
    rlimit_as: Option<libc::rlimit>,
    rlimit_cpu: Option<libc::rlimit>,
}

fn compute_limits(policy: &Policy) -> PrecomputedLimits {
    let mut limits = PrecomputedLimits::default();
    if let Some(bytes) = policy.max_memory {
        limits.rlimit_as = Some(libc::rlimit {
            rlim_cur: bytes,
            rlim_max: bytes,
        });
    }
    if let Some(cpu_ms) = policy.max_cpu_time {
        let seconds = cpu_ms.div_ceil(1000);
        limits.rlimit_cpu = Some(libc::rlimit {
            rlim_cur: seconds,
            rlim_max: seconds,
        });
    }
    limits
}

/// Runs inside the forked child before exec. Only touches pre-built data.
unsafe fn apply_sandbox(limits: &PrecomputedLimits, profile: &CString) -> Result<(), SandboxError> {
    if let Some(rl) = &limits.rlimit_as {
        if libc::setrlimit(libc::RLIMIT_AS, rl) != 0 {
            return Err(SandboxError::Spawn("setrlimit(RLIMIT_AS) failed".into()));
        }
    }

    if let Some(rl) = &limits.rlimit_cpu {
        if libc::setrlimit(libc::RLIMIT_CPU, rl) != 0 {
            return Err(SandboxError::Spawn("setrlimit(RLIMIT_CPU) failed".into()));
        }
    }

    let mut errorbuf: *mut c_char = std::ptr::null_mut();
    let result = sandbox_init(profile.as_ptr(), SANDBOX_PROFILE, &mut errorbuf);
    if result != 0 {
        let detail = if errorbuf.is_null() {
            "unknown error".to_string()
        } else {
            let detail = CStr::from_ptr(errorbuf).to_string_lossy().to_string();
            sandbox_free_error(errorbuf);
            detail
        };
        return Err(SandboxError::Spawn(format!(
            "sandbox_init failed: {detail}"
        )));
    }

    Ok(())
}

pub(crate) fn spawn(
    program: &str,
    args: &[String],
    policy: &Policy,
) -> Result<(u32, ProcessHandle), SandboxError> {
    // All fallible allocations happen here, in the parent.
    let argv = build_argv(program, args);
    let profile = CString::new(build_profile(policy))
        .map_err(|_| SandboxError::Spawn("sandbox profile contains NUL".into()))?;
    let limits = compute_limits(policy);
    let c_program = CString::new(program).map_err(|_| {
        SandboxError::Spawn("program name contains null byte".into())
    })?;

    unsafe {
        let pid = libc::fork();
        if pid == -1 {
            return Err(SandboxError::Spawn("fork failed".into()));
        }

        if pid == 0 {
            // ── Child process ──
            let result = apply_sandbox(&limits, &profile);
            if result.is_ok() {
                let mut c_args: Vec<*const libc::c_char> =
                    argv.iter().map(|a| a.as_ptr()).collect();
                c_args.push(std::ptr::null());
                libc::execv(c_program.as_ptr(), c_args.as_ptr());
            }

            let msg = result
                .err()
                .map(|e| format!("{}\0", e))
                .unwrap_or_else(|| String::from("execv failed\0"));
            let _ = libc::write(
                libc::STDERR_FILENO,
                msg.as_ptr() as *const std::ffi::c_void,
                msg.len(),
            );
            libc::_exit(1);
        }

        Ok((pid as u32, ProcessHandle { pid }))
    }
}

pub(crate) fn kill(handle: &mut ProcessHandle) -> Result<(), SandboxError> {
    unsafe {
        let result = libc::kill(handle.pid, libc::SIGKILL);
        if result != 0 {
            return Err(SandboxError::Kill("kill failed".into()));
        }
        Ok(())
    }
}

pub(crate) fn is_alive(handle: &mut ProcessHandle) -> Result<bool, SandboxError> {
    unsafe {
        let mut status: libc::c_int = 0;
        let result = libc::waitpid(handle.pid, &mut status, libc::WNOHANG);
        if result == -1 {
            return Err(SandboxError::ProcessCheck("waitpid failed".into()));
        }
        if result == 0 {
            return Ok(true);
        }
        Ok(false)
    }
}