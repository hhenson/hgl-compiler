//! Installs disposable executables for the CLI's unit and integration tests.

use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};

/// Publishes a complete executable fixture at `path`. The writer runs in
/// its own process so a concurrent fork from the test process cannot
/// inherit its file descriptor. Publish only after the writer exits.
pub(crate) fn publish_executable(path: &Path, mut bytes: impl Read, permissions: fs::Permissions) {
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).unwrap();
    let staging = tempfile::tempdir_in(parent).unwrap();
    let staged = staging.path().join("fixture");
    let mut writer = Command::new("/bin/sh")
        .args(["-c", "exec cat > \"$1\"", "fixture-writer"])
        .arg(&staged)
        .stdin(Stdio::piped())
        .spawn()
        .expect("start the executable fixture writer");
    let transferred = {
        let mut input = writer.stdin.take().unwrap();
        io::copy(&mut bytes, &mut input)
    };
    let status = writer
        .wait()
        .expect("wait for the executable fixture writer");
    transferred.expect("stream the executable fixture");
    assert!(status.success(), "the executable fixture writer failed");
    fs::set_permissions(&staged, permissions).unwrap();
    fs::rename(staged, path).unwrap();
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    use super::{Path, Read, fs, io, publish_executable};

    struct ObservedTransfer<'a> {
        contents: &'a [u8],
        destination: &'a Path,
        directory: &'a Path,
    }

    impl Read for ObservedTransfer<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            assert_eq!(fs::read(self.destination).unwrap(), b"previous fixture");
            let owns_staged_file = fs::read_dir("/proc/self/fd")
                .unwrap()
                .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
                .any(|path| path.starts_with(self.directory));
            assert!(
                !owns_staged_file,
                "the test process holds a descriptor to the staged fixture"
            );
            self.contents.read(buffer)
        }
    }

    #[test]
    fn fixture_writer_keeps_the_staging_descriptor_out_of_the_test_process() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().canonicalize().unwrap();
        let destination = directory.join("fixture");
        fs::write(&destination, b"previous fixture").unwrap();
        let contents = b"#!/bin/sh\nexit 19\n";
        let bytes = ObservedTransfer {
            contents,
            destination: &destination,
            directory: &directory,
        };
        publish_executable(&destination, bytes, fs::Permissions::from_mode(0o700));
        assert_eq!(fs::read(&destination).unwrap(), contents);
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            Command::new(&destination).status().unwrap().code(),
            Some(19)
        );
    }
}
