use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::ExitStatus;
use tokio::io::unix::AsyncFd;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

const PIDFD_SIGNAL_PROCESS_GROUP: libc::c_uint = 1 << 2;

pub(super) struct Job {
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
    child: Child,
    group: Group,
    registration: Option<String>,
}

impl Job {
    pub async fn spawn(command: &mut Command, registration: Option<&str>) -> io::Result<Self> {
        static WAIT_SUPPORT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if WAIT_SUPPORT.get().is_none() { wait_support()?; let _ = WAIT_SUPPORT.set(()); }
        let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
        if unsafe { libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut action) } < 0 {
            return Err(io::Error::last_os_error());
        }
        if action.sa_sigaction == libc::SIG_IGN || action.sa_flags & libc::SA_NOCLDWAIT != 0 {
            return Err(io::Error::other("Git process ownership requires a waitable child"));
        }
        command.process_group(0);
        let mut child = command.spawn()?;
        let pid = child.id().ok_or_else(|| io::Error::other("Git child has no identity"))? as libc::pid_t;
        #[cfg(test)]
        let failure = capture_hook().lock().unwrap().take();
        #[cfg(test)]
        if let Some(hook) = &failure {
            if let Some(path) = &hook.ready {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                while !path.exists() {
                    if std::time::Instant::now() > deadline { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }
        }
        let captured = Group::capture(pid);
        #[cfg(test)]
        let captured = if failure.is_some() { Err(io::Error::from_raw_os_error(libc::EMFILE)) } else { captured };
        let group = match captured {
            Ok(group) => group,
            Err(error) => {
                unsafe { libc::kill(-pid, libc::SIGKILL); }
                let _ = child.start_kill();
                #[cfg(test)]
                if let Some(hook) = failure {
                    let _ = hook.entered.send(pid as u32);
                    let _ = hook.release.await;
                }
                child.wait().await?;
                return Err(error);
            }
        };
        Ok(Self { stdin: child.stdin.take(), stdout: child.stdout.take(), stderr: child.stderr.take(), child, group, registration: registration.map(str::to_string) })
    }

    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.group.exited().await?;
        if let Some(id) = &self.registration { super::reject_cancellation(id); }
        self.group.stop()?;
        #[cfg(test)]
        {
            let hook = exit_hook().lock().unwrap().take();
            if let Some(hook) = hook {
                let _ = hook.entered.send(());
                let _ = hook.release.await;
            }
        }
        let status = self.child.wait().await;
        self.group.retained = false;
        status
    }

    pub fn start_kill(&mut self) -> io::Result<()> {
        self.group.stop()?;
        self.child.start_kill()
    }

    #[cfg(test)]
    pub(super) fn use_retained_group(&mut self) { self.group.pidfd_group = false; }
}

#[cfg(test)]
pub(in crate::git) struct CaptureHook { pub ready: Option<std::path::PathBuf>, pub entered: tokio::sync::oneshot::Sender<u32>, pub release: tokio::sync::oneshot::Receiver<()> }
#[cfg(test)]
pub(in crate::git) struct ExitHook { pub entered: tokio::sync::oneshot::Sender<()>, pub release: tokio::sync::oneshot::Receiver<()> }
#[cfg(test)]
fn capture_hook() -> &'static std::sync::Mutex<Option<CaptureHook>> { static HOOK: std::sync::Mutex<Option<CaptureHook>> = std::sync::Mutex::new(None); &HOOK }
#[cfg(test)]
fn exit_hook() -> &'static std::sync::Mutex<Option<ExitHook>> { static HOOK: std::sync::Mutex<Option<ExitHook>> = std::sync::Mutex::new(None); &HOOK }
#[cfg(test)]
pub(in crate::git) fn inject_capture_failure(hook: CaptureHook) { *capture_hook().lock().unwrap() = Some(hook); }
#[cfg(test)]
pub(in crate::git) fn pause_before_reap(hook: ExitHook) { *exit_hook().lock().unwrap() = Some(hook); }

fn wait_support() -> io::Result<()> {
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, std::process::id(), 0) };
    if raw < 0 { return Err(io::Error::last_os_error()); }
    let descriptor = unsafe { OwnedFd::from_raw_fd(raw as i32) };
    loop {
        let mut information = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
        let result = unsafe { libc::waitid(libc::P_PIDFD, descriptor.as_raw_fd() as libc::id_t,
            &mut information, libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) };
        let error = io::Error::last_os_error();
        if result < 0 && error.raw_os_error() == Some(libc::ECHILD) { return Ok(()); }
        if result < 0 && error.kind() == io::ErrorKind::Interrupted { continue; }
        return Err(io::Error::new(io::ErrorKind::Unsupported, "Linux Git requires pidfd and waitid support"));
    }
}

impl Drop for Job {
    fn drop(&mut self) { let _ = self.group.stop(); }
}

struct Group {
    descriptor: AsyncFd<OwnedFd>,
    pid: libc::pid_t,
    pidfd_group: bool,
    retained: bool,
    stopped: bool,
}

impl Group {
    fn capture(pid: libc::pid_t) -> io::Result<Self> {
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        if raw < 0 { return Err(io::Error::last_os_error()); }
        let descriptor = unsafe { OwnedFd::from_raw_fd(raw as i32) };
        let result = unsafe { libc::syscall(libc::SYS_pidfd_send_signal, descriptor.as_raw_fd(), 0,
            std::ptr::null::<libc::siginfo_t>(), PIDFD_SIGNAL_PROCESS_GROUP) };
        let pidfd_group = if result == 0 { true } else {
            match io::Error::last_os_error().raw_os_error() {
                Some(libc::ESRCH) => true,
                Some(libc::EINVAL) => false,
                _ => return Err(io::Error::last_os_error()),
            }
        };
        Ok(Self { descriptor: AsyncFd::new(descriptor)?, pid, pidfd_group, retained: true, stopped: false })
    }

    fn observed_exit(&self) -> io::Result<bool> {
        let mut information = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
        let result = unsafe { libc::waitid(libc::P_PIDFD, self.descriptor.as_raw_fd() as libc::id_t,
            &mut information, libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) };
        if result < 0 { return Err(io::Error::last_os_error()); }
        Ok(unsafe { information.si_pid() } != 0)
    }

    async fn exited(&self) -> io::Result<()> {
        loop {
            match self.observed_exit() {
                Ok(true) => return Ok(()),
                Ok(false) => (),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
            let mut ready = self.descriptor.readable().await?;
            match self.observed_exit() {
                Ok(true) => return Ok(()),
                Ok(false) => ready.clear_ready(),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                Err(error) => return Err(error),
            }
        }
    }

    fn stop(&mut self) -> io::Result<()> {
        if self.stopped { return Ok(()); }
        let result = if self.pidfd_group {
            unsafe { libc::syscall(libc::SYS_pidfd_send_signal, self.descriptor.as_raw_fd(), libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(), PIDFD_SIGNAL_PROCESS_GROUP) }
        } else {
            if !self.retained { return Err(io::Error::other("Git process group lost its retained leader")); }
            self.observed_exit()?;
            unsafe { libc::kill(-self.pid, libc::SIGKILL) as libc::c_long }
        };
        if result < 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(io::Error::last_os_error());
        }
        self.stopped = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::process_tests::Fixture;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn retained_child_fallback_closes_descendant_pipes_before_reaping() {
        let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
        for mode in ["pipes", "running"] {
            let fixture = Fixture::new();
            let mut command = Command::new("python3");
            command.arg(Fixture::helper()).arg(&fixture.0).arg(mode).stdin(Stdio::null())
                .stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
            let mut job = Job::spawn(&mut command, None).await.unwrap();
            job.use_retained_group();
            if mode == "running" {
                let deadline = Instant::now() + Duration::from_secs(2);
                while !fixture.0.join("pids.json").exists() {
                    assert!(Instant::now() < deadline);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                job.start_kill().unwrap();
            }
            let mut stdout = job.stdout.take().unwrap();
            let mut stderr = job.stderr.take().unwrap();
            let status = job.wait().await.unwrap();
            assert_eq!(status.success(), mode == "pipes");
            let mut output = Vec::new();
            let mut error = Vec::new();
            tokio::time::timeout(Duration::from_secs(1), stdout.read_to_end(&mut output)).await.unwrap().unwrap();
            tokio::time::timeout(Duration::from_secs(1), stderr.read_to_end(&mut error)).await.unwrap().unwrap();
            fixture.gone().await;
        }
    }

    #[tokio::test]
    async fn failed_spawn_returns_an_error_without_creating_helpers() {
        let fixture = Fixture::new();
        let mut command = Command::new(fixture.0.join("missing-program"));
        assert!(matches!(Job::spawn(&mut command, None).await, Err(error) if error.kind() == io::ErrorKind::NotFound));
        assert!(!fixture.0.join("pids.json").exists());
    }
}
