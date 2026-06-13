use std::ffi::CString;

use crate::error::SandboxError;
use crate::policy::{Policy, SyscallAction};

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

fn build_argv(program: &str, args: &[String]) -> Vec<CString> {
    let mut argv: Vec<CString> = Vec::new();
    argv.push(CString::new(program).expect("program name contains null byte"));
    for arg in args {
        argv.push(CString::new(arg.as_str()).expect("arg contains null byte"));
    }
    argv
}

fn apply_seccomp(policy: &Policy) -> Result<(), SandboxError> {
    unsafe {
        let ret = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
        if ret != 0 {
            return Err(SandboxError::Spawn("prctl(PR_SET_NO_NEW_PRIVS) failed".into()));
        }
    }

    let allow_syscalls: Vec<&str> = policy
        .syscall_rules
        .iter()
        .filter(|r| r.action == SyscallAction::Allow)
        .map(|r| r.name.as_str())
        .collect();

    // Build a BPF filter:
    //   - Load syscall number (offset 0 in seccomp_data)
    //   - For each allowed syscall, check equality and SECCOMP_RET_ALLOW
    //   - Default: SECCOMP_RET_KILL
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct sock_filter {
        code: u16,
        jt: u8,
        jf: u8,
        k: u32,
    }

    #[repr(C)]
    struct sock_fprog {
        len: u16,
        filter: *const sock_filter,
    }

    let n = allow_syscalls.len();

    // Instructions per allow rule: 1 LD + 2 JMP = 3, plus 1 RET = 4 baseline
    let mut filters = Vec::with_capacity(2 + n * 2);

    // Load syscall number: BPF_LD | BPF_W | BPF_ABS, offset 0
    filters.push(sock_filter {
        code: 0x20,
        jt: 0,
        jf: 0,
        k: 0,
    });

    for (i, name) in allow_syscalls.iter().enumerate() {
        let sysno = match resolve_syscall_number(name) {
            Some(n) => n,
            None => continue,
        };

        let is_last = i == n - 1;

        // JEQ: if A == sysno, jump forward (allow), else continue
        if is_last {
            // Last rule: if match → ALLOW (jt=1), else KILL (jf=0)
            filters.push(sock_filter {
                code: 0x15,
                jt: 1,
                jf: 0,
                k: sysno,
            });
        } else {
            // Non-last: if match → ALLOW (jt=1), else continue checking (jf=0)
            filters.push(sock_filter {
                code: 0x15,
                jt: 1,
                jf: 0,
                k: sysno,
            });
        }

        // RET ALLOW (skip 1 instruction to the ALLOW ret)
        filters.push(sock_filter {
            code: 0x06,
            jt: 0,
            jf: 0,
            k: 0x7fff0000,
        });
    }

    // Default: RET KILL
    filters.push(sock_filter {
        code: 0x06,
        jt: 0,
        jf: 0,
        k: 0x00000000,
    });

    let prog = sock_fprog {
        len: filters.len() as u16,
        filter: filters.as_ptr(),
    };

    unsafe {
        let ret = libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &prog as *const _ as usize,
            0,
            0,
        );
        if ret != 0 {
            return Err(SandboxError::Spawn("prctl(PR_SET_SECCOMP) failed".into()));
        }
    }

    Ok(())
}

fn resolve_syscall_number(name: &str) -> Option<u32> {
    // x86_64 syscall numbers
    match name {
        "read" => Some(0),
        "write" => Some(1),
        "open" => Some(2),
        "close" => Some(3),
        "stat" => Some(4),
        "fstat" => Some(5),
        "lstat" => Some(6),
        "poll" => Some(7),
        "lseek" => Some(8),
        "mmap" => Some(9),
        "mprotect" => Some(10),
        "munmap" => Some(11),
        "brk" => Some(12),
        "rt_sigaction" => Some(13),
        "rt_sigprocmask" => Some(14),
        "rt_sigreturn" => Some(15),
        "ioctl" => Some(16),
        "pread64" => Some(17),
        "pwrite64" => Some(18),
        "readv" => Some(19),
        "writev" => Some(20),
        "access" => Some(21),
        "pipe" => Some(22),
        "select" => Some(23),
        "sched_yield" => Some(24),
        "mremap" => Some(25),
        "msync" => Some(26),
        "mincore" => Some(27),
        "madvise" => Some(28),
        "shmget" => Some(29),
        "shmat" => Some(30),
        "shmctl" => Some(31),
        "dup" => Some(32),
        "dup2" => Some(33),
        "pause" => Some(34),
        "nanosleep" => Some(35),
        "getitimer" => Some(36),
        "alarm" => Some(37),
        "setitimer" => Some(38),
        "getpid" => Some(39),
        "sendfile" => Some(40),
        "socket" => Some(41),
        "connect" => Some(42),
        "accept" => Some(43),
        "sendto" => Some(44),
        "recvfrom" => Some(45),
        "sendmsg" => Some(46),
        "recvmsg" => Some(47),
        "shutdown" => Some(48),
        "bind" => Some(49),
        "listen" => Some(50),
        "getsockname" => Some(51),
        "getpeername" => Some(52),
        "socketpair" => Some(53),
        "setsockopt" => Some(54),
        "getsockopt" => Some(55),
        "clone" => Some(56),
        "fork" => Some(57),
        "vfork" => Some(58),
        "execve" => Some(59),
        "exit" => Some(60),
        "wait4" => Some(61),
        "kill" => Some(62),
        "uname" => Some(63),
        "semget" => Some(64),
        "semop" => Some(65),
        "semctl" => Some(66),
        "shmdt" => Some(67),
        "msgget" => Some(68),
        "msgsnd" => Some(69),
        "msgrcv" => Some(70),
        "msgctl" => Some(71),
        "fcntl" => Some(72),
        "flock" => Some(73),
        "fsync" => Some(74),
        "fdatasync" => Some(75),
        "truncate" => Some(76),
        "ftruncate" => Some(77),
        "getdents" => Some(78),
        "getcwd" => Some(79),
        "chdir" => Some(80),
        "fchdir" => Some(81),
        "rename" => Some(82),
        "mkdir" => Some(83),
        "rmdir" => Some(84),
        "creat" => Some(85),
        "link" => Some(86),
        "unlink" => Some(87),
        "symlink" => Some(88),
        "readlink" => Some(89),
        "chmod" => Some(90),
        "fchmod" => Some(91),
        "chown" => Some(92),
        "fchown" => Some(93),
        "lchown" => Some(94),
        "umask" => Some(95),
        "gettimeofday" => Some(96),
        "getrlimit" => Some(97),
        "getrusage" => Some(98),
        "sysinfo" => Some(99),
        "times" => Some(100),
        "ptrace" => Some(101),
        "getuid" => Some(102),
        "syslog" => Some(103),
        "getgid" => Some(104),
        "setuid" => Some(105),
        "setgid" => Some(106),
        "geteuid" => Some(107),
        "getegid" => Some(108),
        "setpgid" => Some(109),
        "getppid" => Some(110),
        "getpgrp" => Some(111),
        "setsid" => Some(112),
        "setreuid" => Some(113),
        "setregid" => Some(114),
        "getgroups" => Some(115),
        "setgroups" => Some(116),
        "setresuid" => Some(117),
        "getresuid" => Some(118),
        "setresgid" => Some(119),
        "getresgid" => Some(120),
        "getpgid" => Some(121),
        "setfsuid" => Some(122),
        "setfsgid" => Some(123),
        "getsid" => Some(124),
        "capget" => Some(125),
        "capset" => Some(126),
        "rt_sigpending" => Some(127),
        "rt_sigtimedwait" => Some(128),
        "rt_sigqueueinfo" => Some(129),
        "rt_sigsuspend" => Some(130),
        "sigaltstack" => Some(131),
        "utime" => Some(132),
        "mknod" => Some(133),
        "uselib" => Some(134),
        "personality" => Some(135),
        "ustat" => Some(136),
        "statfs" => Some(137),
        "fstatfs" => Some(138),
        "sysfs" => Some(139),
        "getpriority" => Some(140),
        "setpriority" => Some(141),
        "sched_setparam" => Some(142),
        "sched_getparam" => Some(143),
        "sched_setscheduler" => Some(144),
        "sched_getscheduler" => Some(145),
        "sched_get_priority_max" => Some(146),
        "sched_get_priority_min" => Some(147),
        "sched_rr_get_interval" => Some(148),
        "mlock" => Some(149),
        "munlock" => Some(150),
        "mlockall" => Some(151),
        "munlockall" => Some(152),
        "vhangup" => Some(153),
        "modify_ldt" => Some(154),
        "pivot_root" => Some(155),
        "_sysctl" => Some(156),
        "prctl" => Some(157),
        "arch_prctl" => Some(158),
        "adjtimex" => Some(159),
        "setrlimit" => Some(160),
        "chroot" => Some(161),
        "sync" => Some(162),
        "acct" => Some(163),
        "settimeofday" => Some(164),
        "mount" => Some(165),
        "umount2" => Some(166),
        "swapon" => Some(167),
        "swapoff" => Some(168),
        "reboot" => Some(169),
        "sethostname" => Some(170),
        "setdomainname" => Some(171),
        "iopl" => Some(172),
        "ioperm" => Some(173),
        "create_module" => Some(174),
        "init_module" => Some(175),
        "delete_module" => Some(176),
        "get_kernel_syms" => Some(177),
        "query_module" => Some(178),
        "quotactl" => Some(179),
        "nfsservctl" => Some(180),
        "getpmsg" => Some(181),
        "putpmsg" => Some(182),
        "afs_syscall" => Some(183),
        "tuxcall" => Some(184),
        "security" => Some(185),
        "gettid" => Some(186),
        "readahead" => Some(187),
        "setxattr" => Some(188),
        "lsetxattr" => Some(189),
        "fsetxattr" => Some(190),
        "getxattr" => Some(191),
        "lgetxattr" => Some(192),
        "fgetxattr" => Some(193),
        "listxattr" => Some(194),
        "llistxattr" => Some(195),
        "flistxattr" => Some(196),
        "removexattr" => Some(197),
        "lremovexattr" => Some(198),
        "fremovexattr" => Some(199),
        "tkill" => Some(200),
        "time" => Some(201),
        "futex" => Some(202),
        "sched_setaffinity" => Some(203),
        "sched_getaffinity" => Some(204),
        "set_thread_area" => Some(205),
        "io_setup" => Some(206),
        "io_destroy" => Some(207),
        "io_getevents" => Some(208),
        "io_submit" => Some(209),
        "io_cancel" => Some(210),
        "get_thread_area" => Some(211),
        "lookup_dcookie" => Some(212),
        "epoll_create" => Some(213),
        "epoll_ctl_old" => Some(214),
        "epoll_wait_old" => Some(215),
        "remap_file_pages" => Some(216),
        "getdents64" => Some(217),
        "set_tid_address" => Some(218),
        "restart_syscall" => Some(219),
        "semtimedop" => Some(220),
        "fadvise64" => Some(221),
        "timer_create" => Some(222),
        "timer_settime" => Some(223),
        "timer_gettime" => Some(224),
        "timer_getoverrun" => Some(225),
        "timer_delete" => Some(226),
        "clock_settime" => Some(227),
        "clock_gettime" => Some(228),
        "clock_getres" => Some(229),
        "clock_nanosleep" => Some(230),
        "exit_group" => Some(231),
        "epoll_wait" => Some(232),
        "epoll_ctl" => Some(233),
        "tgkill" => Some(234),
        "utimes" => Some(235),
        "vmsplice" => Some(236),
        "move_pages" => Some(237),
        "waitid" => Some(247),
        "eventfd" => Some(284),
        "memfd_create" => Some(319),
        "seccomp" => Some(317),
        "execveat" => Some(322),
        _ => None,
    }
}

fn apply_namespaces(policy: &Policy) -> Result<(), SandboxError> {
    let mut flags = libc::CLONE_NEWNS;

    if !policy.allow_networking {
        flags |= libc::CLONE_NEWNET;
    }

    unsafe {
        let ret = libc::unshare(flags);
        if ret != 0 {
            return Err(SandboxError::Spawn("unshare failed".into()));
        }
    }

    Ok(())
}

fn child_exec(program: &str, argv: Vec<CString>, policy: &Policy) -> Result<(), SandboxError> {
    apply_namespaces(policy)?;

    if !policy.syscall_rules.is_empty() {
        apply_seccomp(policy)?;
    }

    let c_program = CString::new(program).expect("program name contains null byte");
    let mut c_args: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    c_args.push(std::ptr::null());

    unsafe {
        libc::execvp(c_program.as_ptr(), c_args.as_ptr());
    }

    Err(SandboxError::Spawn("execvp failed".into()))
}

pub(crate) fn spawn(
    program: &str,
    args: &[String],
    policy: &Policy,
) -> Result<(u32, ProcessHandle), SandboxError> {
    let argv = build_argv(program, args);

    unsafe {
        let pid = libc::fork();
        if pid == -1 {
            return Err(SandboxError::Spawn("fork failed".into()));
        }

        if pid == 0 {
            let result = child_exec(program, argv, policy);
            match result {
                Ok(()) => {}
                Err(e) => {
                    let msg = CString::new(format!("{}\0", e))
                        .unwrap_or_else(|_| CString::new("error").unwrap());
                    let _ = libc::write(
                        libc::STDERR_FILENO,
                        msg.as_ptr() as *const std::ffi::c_void,
                        msg.to_bytes().len(),
                    );
                }
            }
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
