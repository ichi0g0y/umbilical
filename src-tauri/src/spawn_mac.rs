//! Start a process that macOS sees as responsible for itself. macOS only.
//!
//! macOS checks privacy permissions (TCC) for the *responsible* process. A
//! child normally gets the responsible process of its parent. When that process
//! ends, the child and every process it starts later become responsible for
//! themselves, and Umbilical's permissions no longer reach them. The daemon
//! lives longer than the GUI, so it is started responsible for itself. Then it
//! is the responsible process of every session.

use std::ffi::{CString, c_char, c_int};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

unsafe extern "C" {
    // Private, but in libSystem since macOS 10.14. Chromium and node-pty use it.
    fn responsibility_spawnattrs_setdisclaim(
        attr: *mut libc::posix_spawnattr_t,
        disclaim: c_int,
    ) -> c_int;
}

/// Start `exe args` responsible for itself, in its own process group, with
/// stdio on /dev/null and no other open files.
pub fn spawn_disclaimed(exe: &Path, args: &[&str]) -> io::Result<libc::pid_t> {
    let path = CString::new(exe.as_os_str().as_bytes())?;
    let mut owned = vec![path.clone()];
    for a in args {
        owned.push(CString::new(*a)?);
    }
    let mut argv: Vec<*mut c_char> = owned.iter().map(|s| s.as_ptr().cast_mut()).collect();
    argv.push(ptr::null_mut());

    let mut attr: libc::posix_spawnattr_t = ptr::null_mut();
    let mut actions: libc::posix_spawn_file_actions_t = ptr::null_mut();
    let mut pid: libc::pid_t = 0;
    unsafe {
        check(libc::posix_spawnattr_init(&mut attr))?;
        let result = check(libc::posix_spawn_file_actions_init(&mut actions)).and_then(|()| {
            let flags = libc::POSIX_SPAWN_SETPGROUP | libc::POSIX_SPAWN_CLOEXEC_DEFAULT;
            check(libc::posix_spawnattr_setflags(&mut attr, flags as _))?;
            // Own process group: signals to the GUI's group do not reach it.
            check(libc::posix_spawnattr_setpgroup(&mut attr, 0))?;
            check(responsibility_spawnattrs_setdisclaim(&mut attr, 1))?;
            for (fd, mode) in [
                (0, libc::O_RDONLY),
                (1, libc::O_WRONLY),
                (2, libc::O_WRONLY),
            ] {
                check(libc::posix_spawn_file_actions_addopen(
                    &mut actions,
                    fd,
                    c"/dev/null".as_ptr(),
                    mode,
                    0,
                ))?;
            }
            check(libc::posix_spawn(
                &mut pid,
                path.as_ptr(),
                &actions,
                &attr,
                argv.as_ptr(),
                *libc::_NSGetEnviron(),
            ))
        });
        if !actions.is_null() {
            libc::posix_spawn_file_actions_destroy(&mut actions);
        }
        libc::posix_spawnattr_destroy(&mut attr);
        result?;
    }
    // Reap it when it ends, so no zombie stays while the GUI runs.
    std::thread::spawn(move || unsafe {
        libc::waitpid(pid, ptr::null_mut(), 0);
    });
    Ok(pid)
}

fn check(code: c_int) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    unsafe extern "C" {
        fn responsibility_get_pid_responsible_for_pid(pid: libc::pid_t) -> libc::pid_t;
    }

    fn responsible(pid: libc::pid_t) -> libc::pid_t {
        unsafe { responsibility_get_pid_responsible_for_pid(pid) }
    }

    #[test]
    fn child_and_its_children_use_the_new_process() {
        // `; true` keeps sh from replacing itself with sleep.
        let sh = super::spawn_disclaimed(Path::new("/bin/sh"), &["-c", "/bin/sleep 5; true"])
            .expect("spawn");
        std::thread::sleep(Duration::from_millis(300));
        let out = std::process::Command::new("pgrep")
            .args(["-P", &sh.to_string()])
            .output()
            .expect("pgrep");
        let child: libc::pid_t = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .expect("child pid");

        let sh_resp = responsible(sh);
        let child_resp = responsible(child);
        let sh_group = unsafe { libc::getpgid(sh) };
        unsafe {
            libc::kill(child, libc::SIGKILL);
            libc::kill(sh, libc::SIGKILL);
        }
        assert_eq!(sh_resp, sh, "the spawned process is responsible for itself");
        assert_eq!(child_resp, sh, "its children get it as responsible");
        assert_eq!(sh_group, sh, "own process group");
    }
}
