#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use crate::error::SandboxError;
use crate::policy::Policy;

// ── Win32 type aliases ──────────────────────────────────────────
type HANDLE = isize;
type BOOL = i32;
type DWORD = u32;
type LPVOID = *mut std::ffi::c_void;
type LPCVOID = *const std::ffi::c_void;
type LPCWSTR = *const u16;

const INVALID_HANDLE_VALUE: HANDLE = -1;
const FALSE: BOOL = 0;
const TRUE: BOOL = 1;
const CREATE_SUSPENDED: DWORD = 0x00000004;
const STILL_ACTIVE: DWORD = 259;

// ── kernel32 FFI ────────────────────────────────────────────────
#[link(name = "kernel32")]
extern "system" {
    fn CloseHandle(hObject: HANDLE) -> BOOL;
    fn CreateJobObjectW(
        lpJobAttributes: LPVOID,
        lpName: LPCWSTR,
    ) -> HANDLE;
    fn SetInformationJobObject(
        hJob: HANDLE,
        JobObjectInfoClass: i32,
        lpJobObjectInfo: LPCVOID,
        cbJobObjectInfoLength: DWORD,
    ) -> BOOL;
    fn AssignProcessToJobObject(hJob: HANDLE, hProcess: HANDLE) -> BOOL;
    fn CreateProcessW(
        lpApplicationName: LPCWSTR,
        lpCommandLine: *mut u16,
        lpProcessAttributes: LPVOID,
        lpThreadAttributes: LPVOID,
        bInheritHandles: BOOL,
        dwCreationFlags: DWORD,
        lpEnvironment: LPVOID,
        lpCurrentDirectory: LPCWSTR,
        lpStartupInfo: *const STARTUPINFOW,
        lpProcessInformation: *mut PROCESS_INFORMATION,
    ) -> BOOL;
    fn TerminateProcess(hProcess: HANDLE, uExitCode: u32) -> BOOL;
    fn ResumeThread(hThread: HANDLE) -> DWORD;
    fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: *mut DWORD) -> BOOL;
}

// ── Win32 structs ───────────────────────────────────────────────
#[repr(C)]
struct STARTUPINFOW {
    cb: DWORD,
    lpReserved: LPCWSTR,
    lpDesktop: LPCWSTR,
    lpTitle: LPCWSTR,
    dwX: DWORD,
    dwY: DWORD,
    dwXSize: DWORD,
    dwYSize: DWORD,
    dwXCountChars: DWORD,
    dwYCountChars: DWORD,
    dwFillAttribute: DWORD,
    dwFlags: DWORD,
    wShowWindow: u16,
    cbReserved2: u16,
    lpReserved2: LPVOID,
    hStdInput: HANDLE,
    hStdOutput: HANDLE,
    hStdError: HANDLE,
}

#[repr(C)]
struct PROCESS_INFORMATION {
    hProcess: HANDLE,
    hThread: HANDLE,
    dwProcessId: DWORD,
    dwThreadId: DWORD,
}

// ── Job Object constants ────────────────────────────────────────
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: DWORD = 0x00002000;
const JOB_OBJECT_LIMIT_PROCESS_TIME: DWORD = 0x00000002;
const JOB_OBJECT_LIMIT_JOB_MEMORY: DWORD = 0x00000200;

#[repr(C)]
struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
    PerProcessUserTimeLimit: i64,
    PerJobUserTimeLimit: i64,
    LimitFlags: DWORD,
    MinimumWorkingSetSize: usize,
    MaximumWorkingSetSize: usize,
    ActiveProcessLimit: DWORD,
    Affinity: usize,
    ChildProcessRestrictions: DWORD,
    Reserved: [u64; 2],
}

#[repr(C)]
struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
    BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION,
    IoInfo: IO_COUNTERS,
    ProcessMemoryLimit: usize,
    JobMemoryLimit: usize,
    PeakProcessMemoryUsed: usize,
    PeakJobMemoryUsed: usize,
}

#[repr(C)]
struct IO_COUNTERS {
    ReadOperationCount: u64,
    WriteOperationCount: u64,
    OtherOperationCount: u64,
    ReadTransferCount: u64,
    WriteTransferCount: u64,
    OtherTransferCount: u64,
}

// ── ProcessHandle ────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct ProcessHandle {
    process_handle: HANDLE,
    job_handle: HANDLE,
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.process_handle);
            CloseHandle(self.job_handle);
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────

fn to_utf16(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn build_command_line(program: &str, args: &[String]) -> Vec<u16> {
    let mut cmd = format!("\"{}\"", program);
    for arg in args {
        cmd.push(' ');
        if arg.contains(' ') {
            cmd.push_str(&format!("\"{}\"", arg));
        } else {
            cmd.push_str(arg);
        }
    }
    cmd.encode_utf16().chain(std::iter::once(0)).collect()
}

// ── Public API ──────────────────────────────────────────────────

pub(crate) fn spawn(
    program: &str,
    args: &[String],
    policy: &Policy,
) -> Result<(u32, ProcessHandle), SandboxError> {
    unsafe {
        let job_handle = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        if job_handle == INVALID_HANDLE_VALUE {
            return Err(SandboxError::Spawn("failed to create job object".into()));
        }

        if let Err(e) = set_job_limits(job_handle, policy) {
            CloseHandle(job_handle);
            return Err(e);
        }

        let mut cmd_line = build_command_line(program, args);

        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as DWORD,
            lpReserved: std::ptr::null(),
            lpDesktop: std::ptr::null(),
            lpTitle: std::ptr::null(),
            dwX: 0,
            dwY: 0,
            dwXSize: 0,
            dwYSize: 0,
            dwXCountChars: 0,
            dwYCountChars: 0,
            dwFillAttribute: 0,
            dwFlags: 0,
            wShowWindow: 0,
            cbReserved2: 0,
            lpReserved2: std::ptr::null_mut(),
            hStdInput: 0,
            hStdOutput: 0,
            hStdError: 0,
        };
        let mut pi = PROCESS_INFORMATION {
            hProcess: 0,
            hThread: 0,
            dwProcessId: 0,
            dwThreadId: 0,
        };

        let result = CreateProcessW(
            std::ptr::null(),
            cmd_line.as_mut_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            FALSE,
            CREATE_SUSPENDED,
            std::ptr::null_mut(),
            std::ptr::null(),
            &si,
            &mut pi,
        );

        if result == FALSE {
            CloseHandle(job_handle);
            return Err(SandboxError::Spawn("failed to create process".into()));
        }

        let assign_result = AssignProcessToJobObject(job_handle, pi.hProcess);
        if assign_result == FALSE {
            TerminateProcess(pi.hProcess, 1);
            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
            CloseHandle(job_handle);
            return Err(SandboxError::Spawn(
                "failed to assign process to job object".into(),
            ));
        }

        ResumeThread(pi.hThread);

        let pid = pi.dwProcessId;
        CloseHandle(pi.hThread);

        Ok((
            pid,
            ProcessHandle {
                process_handle: pi.hProcess,
                job_handle,
            },
        ))
    }
}

pub(crate) fn kill(handle: &mut ProcessHandle) -> Result<(), SandboxError> {
    unsafe {
        let result = TerminateProcess(handle.process_handle, 1);
        if result == FALSE {
            return Err(SandboxError::Kill("failed to terminate process".into()));
        }
        Ok(())
    }
}

pub(crate) fn is_alive(handle: &mut ProcessHandle) -> Result<bool, SandboxError> {
    unsafe {
        let mut exit_code: DWORD = 0;
        let result = GetExitCodeProcess(handle.process_handle, &mut exit_code);
        if result == FALSE {
            return Err(SandboxError::ProcessCheck(
                "failed to get process exit code".into(),
            ));
        }
        Ok(exit_code == STILL_ACTIVE)
    }
}

unsafe fn set_job_limits(job_handle: HANDLE, policy: &Policy) -> Result<(), SandboxError> {
    let mut limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

    let mut basic_limits = JOBOBJECT_BASIC_LIMIT_INFORMATION {
        PerProcessUserTimeLimit: 0,
        PerJobUserTimeLimit: 0,
        LimitFlags: 0,
        MinimumWorkingSetSize: 0,
        MaximumWorkingSetSize: 0,
        ActiveProcessLimit: 0,
        Affinity: 0,
        ChildProcessRestrictions: 0,
        Reserved: [0; 2],
    };

    if let Some(cpu_ms) = policy.max_cpu_time {
        limit_flags |= JOB_OBJECT_LIMIT_PROCESS_TIME;
        basic_limits.PerProcessUserTimeLimit = cpu_ms as i64 * 10_000;
    }

    if policy.max_memory.is_some() {
        limit_flags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
    }

    basic_limits.LimitFlags = limit_flags;

    let extended_limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        BasicLimitInformation: basic_limits,
        IoInfo: IO_COUNTERS {
            ReadOperationCount: 0,
            WriteOperationCount: 0,
            OtherOperationCount: 0,
            ReadTransferCount: 0,
            WriteTransferCount: 0,
            OtherTransferCount: 0,
        },
        ProcessMemoryLimit: 0,
        JobMemoryLimit: policy.max_memory.unwrap_or(0) as usize,
        PeakProcessMemoryUsed: 0,
        PeakJobMemoryUsed: 0,
    };

    let result = SetInformationJobObject(
        job_handle,
        JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
        &extended_limits as *const _ as LPCVOID,
        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as DWORD,
    );

    if result == FALSE {
        return Err(SandboxError::Spawn(
            "failed to set job object limits".into(),
        ));
    }

    Ok(())
}
