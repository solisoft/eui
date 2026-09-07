//! Spec 08 §10: what the worker process may do once it is running.
//!
//! The worker holds everything that reads bytes a server chose — the frame
//! decoder, the tree, the layout engine, the text shaper, the PNG decoder,
//! the bytecode VM. Once it has started it needs no file, no socket and no
//! other process: fonts are in the binary, frames and assets arrive on its
//! standard input, draw lists leave on its standard output. So that is all
//! it is allowed. On Linux two mechanisms say so, each enough on its own:
//!
//! - **Landlock** denies every filesystem and TCP access the running kernel
//!   knows how to deny (best effort: an older kernel enforces what it has).
//! - **seccomp** allows the handful of system calls the worker's loop
//!   needs — read, write, memory, clocks, exit — and kills the process on
//!   any other. Not "returns an error": kills. A worker that reaches for
//!   `openat` is not a worker having a bad day, it is a worker running
//!   something that was not in the binary.
//!
//! One thing happens before the door closes: a throwaway driver is built,
//! so that whatever initialises itself lazily — the text engine's font
//! loader starts a thread pool and asks the system how many cores it has —
//! has done so. After that, a thread being created or a file being opened
//! is exactly what the filter is there to stop.
//!
//! The window process is not confined by this module: it owns the display,
//! the GPU driver, TLS and the pin store, and its policy is the platform's.
//!
//! macOS (`sandbox_init`) and Windows (AppContainer) are not done; there the
//! worker still runs in its own process, which contains a crash and a
//! memory blow-up, but a compromised worker is not confined. [`lock_down`]
//! says which of the two it managed, and the window prints it.

/// Confine the calling process to what the worker loop needs. Returns a
/// one-line description of what was enforced.
#[cfg(target_os = "linux")]
pub fn lock_down() -> Result<String, String> {
    let mut report = Vec::new();
    // A worker that seccomp kills must not leave a core dump behind: the
    // dump would be the session — every text on screen — written to disk
    // and handed to a crash reporter, for a death that is policy, not a
    // bug. Not dumpable also means no other process of this user can
    // attach a debugger to the worker.
    rustix::process::set_dumpable_behavior(rustix::process::DumpableBehavior::NotDumpable).map_err(|e| format!("prctl: {e}"))?;
    report.push("not dumpable".to_owned());
    report.push(landlock()?);
    report.push(seccomp()?);
    Ok(report.join("; "))
}

/// Confine the calling process. Nothing to do yet off Linux; the error
/// says so.
#[cfg(not(target_os = "linux"))]
pub fn lock_down() -> Result<String, String> {
    Err(format!("no sandbox on {} yet — the worker runs in its own process, unconfined", std::env::consts::OS))
}

#[cfg(target_os = "linux")]
fn landlock() -> Result<String, String> {
    use landlock::{Access, AccessFs, AccessNet, RestrictSelfAttr, Ruleset, RulesetAttr, RulesetStatus, ABI};
    // Every right the ABI knows, handled with no rule granting any of them:
    // that is a deny-all. Best-effort compatibility means an older kernel
    // enforces the rights it has rather than refusing.
    let abi = ABI::V5;
    let status = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map_err(|e| format!("landlock: {e}"))?
        .handle_access(AccessNet::from_all(abi))
        .map_err(|e| format!("landlock: {e}"))?
        .create()
        .map_err(|e| format!("landlock: {e}"))?
        // The text engine's font loader warmed a thread pool before the
        // door closed; those threads are confined too where the kernel can
        // (ABI 7), and only ever shape text otherwise.
        .all_threads(true)
        .map_err(|e| format!("landlock: {e}"))?
        .restrict_self()
        .map_err(|e| format!("landlock: {e}"))?;
    Ok(match status.ruleset {
        RulesetStatus::FullyEnforced => "landlock: files and sockets denied".to_owned(),
        RulesetStatus::PartiallyEnforced => "landlock: partly enforced (older kernel)".to_owned(),
        RulesetStatus::NotEnforced => "landlock: not available on this kernel".to_owned(),
    })
}

/// The system calls the worker loop is allowed. Reading and writing its
/// pipes, memory for the allocator, clocks for transitions, signals and
/// exits for a clean end, and what glibc and Rust's runtime do at thread
/// start and on a panic. Nothing that names a file, a socket or another
/// process.
#[cfg(target_os = "linux")]
const ALLOWED: &[libc::c_long] = &[
    libc::SYS_read,
    libc::SYS_write,
    libc::SYS_readv,
    libc::SYS_writev,
    libc::SYS_close,
    libc::SYS_fstat,
    libc::SYS_newfstatat,
    libc::SYS_lseek,
    libc::SYS_mmap,
    libc::SYS_munmap,
    libc::SYS_mremap,
    libc::SYS_mprotect,
    libc::SYS_madvise,
    libc::SYS_brk,
    libc::SYS_futex,
    libc::SYS_exit,
    libc::SYS_exit_group,
    libc::SYS_rt_sigreturn,
    libc::SYS_rt_sigprocmask,
    libc::SYS_rt_sigaction,
    libc::SYS_sigaltstack,
    libc::SYS_clock_gettime,
    libc::SYS_clock_getres,
    libc::SYS_clock_nanosleep,
    libc::SYS_nanosleep,
    libc::SYS_sched_yield,
    libc::SYS_getrandom,
    libc::SYS_getpid,
    libc::SYS_gettid,
    libc::SYS_tgkill,
    libc::SYS_membarrier,
    libc::SYS_rseq,
    libc::SYS_set_robust_list,
    libc::SYS_prlimit64,
    libc::SYS_getrlimit,
    // mimalloc (the allocator of a Soli-built host) asks which NUMA node
    // it is on when it maps a segment.
    libc::SYS_getcpu,
];

#[cfg(target_os = "linux")]
fn seccomp() -> Result<String, String> {
    use seccompiler::{apply_filter_all_threads, BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter, SeccompRule, TargetArch};
    /// `PR_SET_VMA`, the one `prctl` an allocator makes.
    const PR_SET_VMA: u64 = 0x53564d41;
    let arch = TargetArch::try_from(std::env::consts::ARCH).map_err(|e| format!("seccomp: {e:?}"))?;
    let mut rules: std::collections::BTreeMap<i64, Vec<SeccompRule>> = ALLOWED.iter().map(|n| (i64::from(*n), Vec::new())).collect();
    // mimalloc (a Soli-built host's allocator) names the memory it maps,
    // `prctl(PR_SET_VMA, PR_SET_VMA_ANON_NAME, …)`, so the mapping shows
    // in /proc; that one prctl and no other. It is the only conditional
    // rule: everything else is allowed whole or not at all.
    let set_vma = SeccompCondition::new(0, SeccompCmpArgLen::Dword, SeccompCmpOp::Eq, PR_SET_VMA).map_err(|e| format!("seccomp: {e}"))?;
    rules.insert(i64::from(libc::SYS_prctl), vec![SeccompRule::new(vec![set_vma]).map_err(|e| format!("seccomp: {e}"))?]);
    // `EUI_SECCOMP_LOG=1` while developing: a stray system call is logged
    // by the kernel (`dmesg`, `type=1326 … syscall=N`) and refused with
    // ENOSYS instead of killing the worker, so the allowlist can be fixed.
    let mismatch = if std::env::var("EUI_SECCOMP_LOG").is_ok_and(|v| v == "1") { SeccompAction::Log } else { SeccompAction::KillProcess };
    let filter = SeccompFilter::new(rules, mismatch, SeccompAction::Allow, arch).map_err(|e| format!("seccomp: {e}"))?;
    let program: BpfProgram = filter.try_into().map_err(|e| format!("seccomp: {e}"))?;
    // Every thread, the warmed font-loader pool included.
    apply_filter_all_threads(&program).map_err(|e| format!("seccomp: {e}"))?;
    Ok(format!("seccomp: {} system calls allowed (and prctl only to name a mapping), any other kills the worker", ALLOWED.len()))
}
