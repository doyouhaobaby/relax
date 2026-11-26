use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(super) const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "cccccccccccccccccccccccccccccccccccccccc";
const D: &str = "dddddddddddddddddddddddddddddddddddddddd";
struct Temp { home: PathBuf, root: PathBuf }
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let home = std::env::temp_dir().join(format!("makepad-jit-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let root = home.join("cache/loader");
        fs::create_dir_all(&root).unwrap(); Self { home, root }
    }
    fn built(&self, name: &str) -> Active {
        let a = named(&self.root, name);
        fs::create_dir_all(a.binary.parent().unwrap()).unwrap();
        fs::write(&a.binary, b"binary").unwrap(); fs::write(a.dir.join(".built"), format!("{name} 1.98.0 host-triple")).unwrap(); a
    }
}
impl Drop for Temp { fn drop(&mut self) { fs::remove_dir_all(&self.home).unwrap(); } }
#[test]
fn active_selection_and_invalid_names() {
    let t = Temp::new(); let src = t.built("src"); let a = t.built(A);
    assert_eq!(active(&t.root), src);
    for bad in ["../escape", "/escape", "", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "abc", B] {
        atomic_write(&t.root.join("current"), bad).unwrap(); assert_eq!(active(&t.root), src);
    }
    atomic_write(&t.root.join("current"), A).unwrap(); assert_eq!(active(&t.root), a);
    let binary = t.home.join("target/loader").join(A).join(HOST_BINARY);
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    assert_eq!(active(&t.root).binary, a.dir.join("target").join(HOST_BINARY));
    fs::write(&binary, b"new binary").unwrap();
    assert_eq!(active(&t.root), Active { binary, ..a });
    atomic_write(&t.root.join("current"), "src").unwrap(); assert_eq!(active(&t.root), src);
}
#[test]
fn switch_confirmation_and_idempotence() {
    let t = Temp::new(); let lock = Lock::acquire(&t.root).unwrap(); t.built("src"); t.built(A);
    let record = t.root.join(A).join("runner.launch");
    let backup = t.root.join(A).join("runner.launch.previous");
    assert!(begin_code_switch(&lock, "src", "missing record").is_err());
    fs::write(t.root.join("src/runner.launch"), "bootstrap").unwrap();
    fs::write(&backup, "stray backup").unwrap();
    begin_switch(&lock, A, "record a\n").unwrap(); assert_eq!(active(&t.root).name, A); assert_eq!(pointer(&t.root, "previous").as_deref(), Some("src"));
    assert!(!backup.exists());
    assert!(confirms_own_boot(&t.root, &t.root.join(A)));
    assert!(!confirms_own_boot(&t.root, &t.root.join("src")));
    assert_eq!(fs::read_to_string(t.root.join(A).join("runner.launch")).unwrap(), "record a\n", "the new folder holds its pin");
    assert!(pending_is(&t.root, &t.root.join(A))); assert!(begin_switch(&lock, A, "refused\n").is_err());
    assert_eq!(fs::read_to_string(t.root.join(A).join("runner.launch")).unwrap(), "record a\n", "a refused switch leaves the pin");
    confirm_boot(&lock).unwrap(); confirm_boot(&lock).unwrap();
    assert_eq!(rollback(&lock, "unused").unwrap().name, A); assert!(!t.root.join("rolled-back").exists());

    assert!(begin_code_switch(&lock, "src", "record b\n").is_err());
    assert!(begin_code_switch(&lock, A, "record a\n").is_err());
    assert!(!backup.exists());
    begin_code_switch(&lock, A, "record b\n").unwrap();
    assert!(begin_code_switch(&lock, A, "refused\n").is_err());
    for key in ["current", "previous", "pending"] {
        assert_eq!(pointer(&t.root, key).as_deref(), Some(A), "{key}");
    }
    assert_eq!(fs::read_to_string(&backup).unwrap(), "record a\n");
    assert_eq!(fs::read_to_string(&record).unwrap(), "record b\n");
    assert!(!confirms_own_boot(&t.root, &t.root.join(A)), "only the watcher confirms a code switch");
    // A backup that cannot be removed must leave pending in place for recovery.
    fs::remove_file(&backup).unwrap();
    fs::create_dir(&backup).unwrap();
    fs::write(backup.join("child"), "").unwrap();
    assert!(confirm_boot(&lock).is_err());
    assert!(pending_is(&t.root, &t.root.join(A)));
    fs::remove_dir_all(&backup).unwrap();
    fs::write(&backup, "record a\n").unwrap();
    confirm_boot(&lock).unwrap(); confirm_boot(&lock).unwrap();
    assert!(!backup.exists() && !t.root.join("pending").exists());
    assert_eq!(fs::read_to_string(record).unwrap(), "record b\n");
}
#[test]
fn crash_states_roll_back_to_previous_and_twice_is_a_noop() {
    for previous in ["src", A] {
        for current_written in [false, true] {
            let t = Temp::new(); let lock = Lock::acquire(&t.root).unwrap(); t.built("src"); t.built(A); t.built(B);
            atomic_write(&t.root.join("current"), previous).unwrap();
            atomic_write(&t.root.join("previous"), previous).unwrap();
            // A crash before pending is published hasn't switched anything.
            assert_eq!(rollback(&lock, "unused").unwrap().name, previous); assert!(!t.root.join("rolled-back").exists());
            atomic_write(&t.root.join("pending"), B).unwrap();
            if current_written { atomic_write(&t.root.join("current"), B).unwrap(); }
            assert_eq!(rollback(&lock, "did not present").unwrap().name, previous);
            assert_eq!(t.root.join("current").exists(), previous != "src");
            assert_eq!(fs::read_to_string(t.root.join("rolled-back")).unwrap(), "did not present");
            assert_eq!(rollback(&lock, "second").unwrap().name, previous);
            assert_eq!(fs::read_to_string(t.root.join("rolled-back")).unwrap(), "did not present");
        }
    }
    let t = Temp::new(); let lock = Lock::acquire(&t.root).unwrap(); let a = t.built(A);
    let record = a.dir.join("runner.launch");
    let backup = a.dir.join("runner.launch.previous");
    atomic_write(&t.root.join("current"), A).unwrap();
    fs::write(&record, "old").unwrap();
    for restored_before_crash in [false, true] {
        begin_code_switch(&lock, A, "trial").unwrap();
        if restored_before_crash {
            atomic_write(&t.root.join("rolled-back"), "trial failed").unwrap();
            fs::rename(&backup, &record).unwrap();
        }
        rollback(&lock, "trial failed").unwrap();
        rollback(&lock, "second").unwrap();
        assert_eq!(fs::read_to_string(&record).unwrap(), "old");
        assert_eq!(fs::read_to_string(t.root.join("current")).unwrap(), A);
        assert_eq!(fs::read_to_string(t.root.join("rolled-back")).unwrap(), "trial failed");
        assert!(!backup.exists() && !t.root.join("pending").exists());
        dismiss_rollback(&lock).unwrap();
    }
    begin_code_switch(&lock, A, "presented").unwrap();
    fs::remove_file(&backup).unwrap(); // confirm was interrupted before removing pending
    rollback(&lock, "recovery").unwrap();
    assert_eq!(fs::read_to_string(record).unwrap(), "presented");
    assert!(!t.root.join("rolled-back").exists());
    assert!(!t.root.join("pending").exists());
}

// Interrupt the actual write sequence immediately after its nth durable directory change.
std::thread_local! {
    static WRITES_LEFT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
pub(super) fn write_completed() -> io::Result<()> {
    WRITES_LEFT.with(|left| match left.get() {
        Some(1) => { left.set(None); Err(io::ErrorKind::Interrupted.into()) }
        Some(n) => { left.set(Some(n - 1)); Ok(()) }
        None => Ok(()),
    })
}

// Capture every file in the installation, including any unexpected writes.
fn files(dir: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut snapshot = std::collections::BTreeMap::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            snapshot.extend(files(&path));
        } else if path.file_name().unwrap() == LOCK_FILE || path.file_name().unwrap() == PREPARE_LOCK {
            // Windows can refuse a read on a locked range, even in the holding process.
            assert_eq!(path.metadata().unwrap().len(), 0);
            snapshot.insert(path, Vec::new());
        } else {
            let bytes = fs::read(&path).unwrap();
            snapshot.insert(path, bytes);
        }
    }
    snapshot
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation { Begin, Confirm, Rollback }

#[test]
fn every_write_prefix_settles_without_changing_loaders() {
    for previous in ["src", A] {
        for code_only in [false, true] {
            for op in [Operation::Begin, Operation::Confirm, Operation::Rollback] {
                for cut in 0.. {
                    let t = Temp::new();
                    let root = &t.root;
                    let old = t.built(previous);
                    let trial = if code_only { named(root, previous) } else { t.built(B) };
                    fs::write(old.dir.join("runner.launch"), "confirmed record").unwrap();
                    fs::write(root.join("current"), previous).unwrap();
                    // A stale *different* previous must never be published by the new pending.
                    fs::write(root.join("previous"), B).unwrap();
                    fs::write(trial.dir.join("runner.launch.previous"), "stray backup").unwrap();
                    let lock = Lock::acquire(root).unwrap();
                    let begin = || if code_only {
                        begin_code_switch(&lock, previous, "trial record")
                    } else {
                        begin_switch(&lock, B, "trial record")
                    };
                    if op != Operation::Begin { begin().unwrap(); }

                    let complete = if cut == 0 { false } else {
                        WRITES_LEFT.with(|left| left.set(Some(cut)));
                        let result = match op {
                            Operation::Begin => begin(),
                            Operation::Confirm => confirm_boot(&lock),
                            Operation::Rollback => rollback(&lock, "trial failed").map(|_| ()),
                        };
                        WRITES_LEFT.with(|left| left.set(None));
                        match result {
                            Ok(()) => true,
                            Err(e) if e.kind() == io::ErrorKind::Interrupted => false,
                            Err(e) => panic!("{op:?} after {cut} writes: {e}"),
                        }
                    };
                    drop(lock);
                    let case = format!("{op:?}, code-only={code_only}, previous={previous}, cut={cut}");
                    let pending = root.join("pending").exists();
                    if pending { assert_eq!(pointer(root, "previous").as_deref(), Some(previous), "previous before pending: {case}"); }
                    let selected = confirmed(root);
                    let kept_trial = op == Operation::Confirm &&
                        (!pending || (code_only && !trial.dir.join("runner.launch.previous").exists()));
                    let expected = if kept_trial { &trial } else { &old };
                    assert_eq!(&selected, expected, "{case}");
                    let settled = settle(root).unwrap();
                    assert_eq!(settled, if pending { Settlement::RolledBack } else { Settlement::Idle }, "{case}");
                    assert_eq!(active(root), selected, "starter and recovered loader: {case}");
                    assert!(!root.join("pending").exists(), "{case}");
                    if op != Operation::Begin || pending {
                        assert!(!trial.dir.join("runner.launch.previous").exists(), "settled trial backup: {case}");
                    }
                    let record = fs::read_to_string(selected.dir.join("runner.launch")).unwrap();
                    assert_eq!(record, if kept_trial { "trial record" } else { "confirmed record" }, "{case}");
                    assert_eq!(root.join("rolled-back").exists(), !kept_trial && (pending || op == Operation::Rollback), "rollback note: {case}");
                    let settled_files = files(&t.home);
                    assert_eq!(settle(root).unwrap(), Settlement::Idle, "{case}");
                    assert_eq!(files(&t.home), settled_files, "second settle: {case}");
                    if complete { break; }
                }
            }
        }
    }
}

#[test]
fn live_watcher_owns_settlement_and_confirmation() {
    for code_only in [false, true] {
        let t = Temp::new();
        let old = t.built("src");
        fs::write(old.dir.join("runner.launch"), "confirmed record").unwrap();
        let watcher = Lock::acquire(&t.root).unwrap();
        let trial = if code_only {
            begin_code_switch(&watcher, "src", "trial record").unwrap();
            named(&t.root, "src")
        } else {
            let trial = t.built(A);
            begin_switch(&watcher, A, "trial record").unwrap();
            trial
        };
        let before = files(&t.home);
        assert!(matches!(Lock::try_acquire(&t.root).unwrap(), TryLock::Busy));
        assert_eq!(settle(&t.root).unwrap(), Settlement::Live);
        confirm_own_boot(&t.root, &trial.dir).unwrap();
        assert_eq!(files(&t.home), before, "a second start and its first frame leave the live trial alone");
        drop(watcher);
        assert_eq!(settle(&t.root).unwrap(), Settlement::RolledBack);
        assert_eq!(active(&t.root), old);
        assert_eq!(fs::read_to_string(old.dir.join("runner.launch")).unwrap(), "confirmed record");
        assert!(t.root.join("rolled-back").is_file());
    }
}

#[test]
fn recover_command_waits_for_the_watcher_before_rolling_back() {
    use super::tests::{answer, spawn};
    use std::{sync::mpsc, thread, time::Duration};

    let t = Temp::new();
    let fresh = t.home.join("fresh/cache/loader");
    let wait = ["--wait".to_string()];
    assert_eq!(recover_command(&fresh, &[]), 0);
    assert_eq!(recover_command(&fresh, &wait), 0);
    assert!(!fresh.exists());

    let previous = t.built(A);
    t.built(B);
    fs::write(t.root.join("current"), A).unwrap();
    fs::write(previous.dir.join("runner.launch"), "confirmed record").unwrap();
    begin_switch(&Lock::acquire(&t.root).unwrap(), B, "trial record").unwrap();
    let (mut holder, mut lines) = spawn(&t.root, "hold");
    assert_eq!(answer(&mut lines, "holder"), "held");
    let before = files(&t.home);
    assert_eq!(recover_command(&t.root, &[]), 3);
    assert_eq!(files(&t.home), before, "nonblocking recovery left the live trial alone");

    let root = t.root.clone();
    let (started, ready) = mpsc::sync_channel(0);
    let waiter = thread::spawn(move || {
        started.send(()).unwrap();
        recover_command(&root, &wait)
    });
    ready.recv().unwrap();
    thread::sleep(Duration::from_millis(100));
    assert!(!waiter.is_finished(), "--wait returned while the watcher held the lock");
    assert_eq!(files(&t.home), before, "waiting left every file intact");
    drop(holder.stdin.take());
    assert!(holder.wait().unwrap().success());
    assert_eq!(waiter.join().unwrap(), 0);
    assert!(!t.root.join("pending").exists());
    assert_eq!(fs::read_to_string(t.root.join("current")).unwrap(), A);
    assert_eq!(active(&t.root), previous);
    assert_eq!(fs::read_to_string(t.root.join("rolled-back")).unwrap(), "The update did not confirm its first frame.");
}

#[test]
fn prune_keeps_active_and_previous_and_never_prunes_pending() {
    let t = Temp::new();
    let lock = Lock::acquire(&t.root).unwrap();
    let targets = t.home.join("target/loader");
    fs::create_dir_all(&targets).unwrap();
    for name in ["src", A, B, C] {
        let loader = t.built(name);
        fs::rename(loader.dir.join("target"), targets.join(name)).unwrap();
    }
    fs::create_dir(targets.join(D)).unwrap(); // an orphan target with no source
    fs::create_dir(targets.join("unrelated")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(targets.join(A), t.root.join(C).join("target")).unwrap();
    fs::create_dir(t.root.join(".unpack")).unwrap(); fs::create_dir(t.root.join("unrelated")).unwrap();
    atomic_write(&t.root.join("current"), A).unwrap(); begin_switch(&lock, B, "record b\n").unwrap(); prune(&lock, None).unwrap();
    for name in ["src", A, B, C, ".unpack"] { assert!(t.root.join(name).exists()); }
    for name in ["src", A, B, C, D] { assert!(targets.join(name).is_dir(), "pending keeps target {name}"); }
    confirm_boot(&lock).unwrap(); prune(&lock, None).unwrap();
    for name in [A, B, "unrelated"] { assert!(t.root.join(name).exists()); }
    for name in ["src", C, ".unpack"] { assert!(!t.root.join(name).exists()); }
    for name in [A, B] { assert!(targets.join(name).join(HOST_BINARY).is_file(), "kept loader {name} survives"); }
    for name in ["src", C, D] { assert!(!targets.join(name).exists(), "stale target {name} goes"); }
    assert!(targets.join("unrelated").is_dir());
}
#[test]
fn retry_removes_only_an_incomplete_valid_version() {
    let t = Temp::new(); t.built("src"); fs::create_dir(t.root.join(A)).unwrap(); fs::write(t.root.join(A).join("partial"), b"partial").unwrap();
    let targets = t.home.join("target/loader");
    fs::create_dir_all(targets.join(A)).unwrap();
    let prepare = Prepare::try_hold(&t.root).unwrap().unwrap();
    prepare_version(&prepare, A).unwrap(); assert!(!t.root.join(A).join("partial").exists());
    assert!(!targets.join(A).exists(), "a retry clears the incomplete build");
    fs::create_dir(targets.join(B)).unwrap();
    assert!(prepare_version(&prepare, "../escape").is_err()); t.built(B); assert!(prepare_version(&prepare, B).is_err());
    assert!(targets.join(B).is_dir(), "refusing to replace a built loader keeps its target");
}

#[test]
fn another_runner_cannot_prune_a_build_or_offer_until_its_owner_exits() {
    use super::tests::{answer, free_after_exit, spawn};

    let t = Temp::new();
    t.built(A);
    t.built("src");
    fs::write(t.root.join("current"), A).unwrap();
    fs::write(t.root.join("previous"), "src").unwrap();
    let source = t.root.join(B);
    let target = target_root(&t.root).join(B);
    let binary = target.join(HOST_BINARY);

    for end in ["exit", "kill"] {
        t.built(C); // An unrelated obsolete loader detects a partial sweep.
        let (mut child, mut lines) = spawn(&t.root, "prepare");
        assert_eq!(answer(&mut lines, "prepare"), "preparing");
        let switch = Lock::try_hold(&t.root).unwrap().expect("preparation leaves switch.lock free");
        for offered in [false, true] {
            if offered {
                fs::create_dir_all(binary.parent().unwrap()).unwrap();
                fs::write(&binary, b"built binary").unwrap();
                fs::write(source.join(".built"), B).unwrap();
            }
            assert!(Prepare::try_hold(&t.root).unwrap().is_none(), "the child owns prepare.lock");
            let before = files(&t.home);
            prune(&switch, None).unwrap();
            assert_eq!(files(&t.home), before, "prune must skip the whole sweep, offered={offered}");
        }
        if end == "kill" { child.kill().unwrap(); } else { drop(child.stdin.take()); }
        let status = child.wait().unwrap();
        if end == "exit" { assert!(status.success()); }
        free_after_exit(&format!("the preparer's {end}"), || Prepare::try_hold(&t.root).unwrap().is_some());
        prune(&switch, None).unwrap();
        assert!(!source.exists() && !target.exists(), "the abandoned loader can be pruned after {end}");
        assert!(!t.root.join(C).exists(), "the sweep resumes after {end}");
        assert!(t.root.join(PREPARE_LOCK).is_file(), "the lock file survives a sweep");
    }
}
#[cfg(unix)]
#[test]
fn cleanup_does_not_follow_symlinks() {
    let t = Temp::new(); let lock = Lock::acquire(&t.root).unwrap(); let outside = Temp::new(); t.built("src");
    let prepare = |name| prepare_version(&Prepare::try_hold(&t.root).unwrap().unwrap(), name);
    std::os::unix::fs::symlink(&outside.root, t.root.join(A)).unwrap();
    assert!(prepare(A).is_err()); prune(&lock, None).unwrap(); assert!(outside.root.exists()); assert!(t.root.join(A).is_symlink());
    let targets = t.home.join("target/loader");
    fs::create_dir_all(&targets).unwrap();
    std::os::unix::fs::symlink(&outside.root, targets.join(B)).unwrap();
    assert!(prepare(B).is_err()); prune(&lock, None).unwrap(); assert!(targets.join(B).is_symlink());
    fs::rename(&targets, t.home.join("saved-targets")).unwrap();
    std::os::unix::fs::symlink(&outside.root, &targets).unwrap();
    outside.built(C);
    assert!(prune(&lock, None).is_err()); assert!(prepare(C).is_err()); assert!(outside.root.join(C).is_dir());
}
