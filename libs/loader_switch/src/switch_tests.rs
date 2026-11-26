use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
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
        } else if path.file_name().unwrap() == LOCK_FILE {
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
