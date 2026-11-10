//! Front-end independent logic shared by the CLI and the GUI.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_PASSWORD_CHARS: usize = 64;

pub fn parse_password(s: &str) -> Result<String, String> {
    let len = s.chars().count();
    if len == 0 {
        Err("password must not be empty".into())
    } else if len > MAX_PASSWORD_CHARS {
        Err(format!(
            "password has {len} characters, maximum is {MAX_PASSWORD_CHARS}"
        ))
    } else {
        Ok(s.to_owned())
    }
}

/// Recursively collects all `.zip` files under `input_dir` (sorted).
/// Unreadable directories/entries are skipped; returns an empty vec if none found.
pub fn find_zip_files(input_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![input_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // file_type() does not follow symlinks, avoiding symlink loops
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
            {
                found.push(path);
            }
        }
    }

    found.sort();
    found
}

/// Extracts `zip_path` into `out_dir` using `password`; returns number of extracted files.
/// All entries are first verified (password + CRC) without writing anything, so a wrong
/// password or corrupt archive leaves `out_dir` untouched.
pub fn extract_zip(zip_path: &Path, password: &str, out_dir: &Path) -> Result<usize, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("cannot open: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("invalid zip: {e}"))?;
    let pw = password.as_bytes();

    // Pass 1: verify
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index_decrypt(i, pw)
            .map_err(|e| format!("entry #{i}: {e}"))?;
        if entry.enclosed_name().is_none() {
            return Err(format!("entry #{i}: unsafe path"));
        }
        std::io::copy(&mut entry, &mut std::io::sink()).map_err(|e| format!("entry #{i}: {e}"))?;
    }

    // Pass 2: write (only now is `out_dir` created, so failures leave nothing behind)
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let mut count = 0;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index_decrypt(i, pw)
            .map_err(|e| format!("entry #{i}: {e}"))?;
        let target = out_dir.join(entry.enclosed_name().ok_or("unsafe path")?);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            let mut out =
                std::fs::File::create(&target).map_err(|e| format!("{}: {e}", target.display()))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("{}: {e}", target.display()))?;
            count += 1;
        }
    }
    Ok(count)
}
#[derive(Debug, Clone)]
pub struct Options {
    pub password: String,
    pub input: PathBuf,
    pub output: PathBuf,
    pub remove_zip: bool,
    /// Extract each zip into a new folder named after the zip (without extension).
    pub folder_per_zip: bool,
}

/// Progress notification emitted by [`run`]. `Display` gives the CLI/GUI log line.
#[derive(Debug, Clone)]
pub enum Event {
    Started {
        input: PathBuf,
        output: PathBuf,
        password_len: usize,
    },
    Found(usize),
    Extracted {
        zip: PathBuf,
        files: usize,
        removed: bool,
    },
    /// Extracted fine, but the zip could not be deleted.
    RemoveFailed {
        zip: PathBuf,
        error: String,
    },
    Failed {
        zip: PathBuf,
        error: String,
    },
    Cancelled,
}

impl Event {
    pub fn is_error(&self) -> bool {
        matches!(self, Event::RemoveFailed { .. } | Event::Failed { .. })
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Started {
                input,
                output,
                password_len,
            } => write!(
                f,
                "input:  {}\noutput: {}\npassword length: {password_len} chars",
                input.display(),
                output.display()
            ),
            Event::Found(n) => write!(f, "found {n} zip file(s)"),
            Event::Extracted {
                zip,
                files,
                removed: false,
            } => {
                write!(f, "ok: {} ({files} file(s))", zip.display())
            }
            Event::Extracted {
                zip,
                files,
                removed: true,
            } => {
                write!(f, "ok: {} ({files} file(s)), zip removed", zip.display())
            }
            Event::RemoveFailed { zip, error } => write!(
                f,
                "warning: {} extracted, but could not remove zip: {error}",
                zip.display()
            ),
            Event::Failed { zip, error } => {
                write!(f, "error: {}: {error} (zip kept)", zip.display())
            }
            Event::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Extracts every zip under `opts.input`, reporting progress through `report`.
/// Checks `cancel` between archives. Per-archive failures are reported as events,
/// only invalid directories make this return `Err`.
pub fn run(
    opts: &Options,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(Event),
) -> Result<(), String> {
    for (label, dir) in [("input", &opts.input), ("output", &opts.output)] {
        if !dir.is_dir() {
            return Err(format!("{label} directory not found: {}", dir.display()));
        }
    }
    parse_password(&opts.password)?;

    report(Event::Started {
        input: opts.input.clone(),
        output: opts.output.clone(),
        password_len: opts.password.chars().count(),
    });

    let zips = find_zip_files(&opts.input);
    report(Event::Found(zips.len()));
    for zip in zips {
        if cancel.load(Ordering::Relaxed) {
            report(Event::Cancelled);
            break;
        }
        let target = match (opts.folder_per_zip, zip.file_stem()) {
            (true, Some(stem)) => opts.output.join(stem),
            _ => opts.output.clone(),
        };
        match extract_zip(&zip, &opts.password, &target) {
            Ok(files) if !opts.remove_zip => report(Event::Extracted {
                zip,
                files,
                removed: false,
            }),
            Ok(files) => match std::fs::remove_file(&zip) {
                Ok(()) => report(Event::Extracted {
                    zip,
                    files,
                    removed: true,
                }),
                Err(e) => report(Event::RemoveFailed {
                    zip,
                    error: e.to_string(),
                }),
            },
            Err(error) => report(Event::Failed { zip, error }),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn empty_dir_returns_empty_vec() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(find_zip_files(tmp.path()).is_empty());
    }

    #[test]
    fn missing_dir_returns_empty_vec() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(find_zip_files(&tmp.path().join("nope")).is_empty());
    }

    #[test]
    fn finds_zips_recursively_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(&r.join("b.zip"));
        touch(&r.join("a.zip"));
        touch(&r.join("sub/c.zip"));
        touch(&r.join("sub/deep/d.zip"));

        let found = find_zip_files(r);
        assert_eq!(
            found,
            vec![
                r.join("a.zip"),
                r.join("b.zip"),
                r.join("sub/c.zip"),
                r.join("sub/deep/d.zip"),
            ]
        );
    }

    #[test]
    fn ignores_non_zip_files() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(&r.join("a.txt"));
        touch(&r.join("b.jpg"));
        touch(&r.join("zip")); // no extension
        touch(&r.join("a.zip.bak"));
        touch(&r.join("ok.zip"));

        assert_eq!(find_zip_files(r), vec![r.join("ok.zip")]);
    }

    #[test]
    fn extension_is_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(&r.join("a.ZIP"));
        touch(&r.join("b.Zip"));

        assert_eq!(find_zip_files(r), vec![r.join("a.ZIP"), r.join("b.Zip")]);
    }

    #[test]
    fn ignores_directories_named_like_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::create_dir(r.join("fake.zip")).unwrap();
        touch(&r.join("fake.zip/inner.zip"));

        // the directory itself is not returned, its contents are
        assert_eq!(find_zip_files(r), vec![r.join("fake.zip/inner.zip")]);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlink_loops() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(&r.join("sub/a.zip"));
        std::os::unix::fs::symlink(r, r.join("sub/loop")).unwrap();

        assert_eq!(find_zip_files(r), vec![r.join("sub/a.zip")]);
    }

    fn make_zip(path: &Path, password: &str) {
        use std::io::Write;
        let file = fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .with_aes_encryption(zip::AesMode::Aes256, password);
        w.start_file("dir/hello.txt", opts.clone()).unwrap();
        w.write_all(b"hello").unwrap();
        w.finish().unwrap();
    }

    #[test]
    fn extract_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let (z, out) = (tmp.path().join("a.zip"), tmp.path().join("out"));
        fs::create_dir(&out).unwrap();
        make_zip(&z, "1234");

        assert_eq!(extract_zip(&z, "1234", &out), Ok(1));
        assert_eq!(fs::read(out.join("dir/hello.txt")).unwrap(), b"hello");
    }

    #[test]
    fn extract_creates_missing_out_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let (z, out) = (tmp.path().join("a.zip"), tmp.path().join("new"));
        make_zip(&z, "1234");

        assert_eq!(extract_zip(&z, "1234", &out), Ok(1));
        assert!(out.join("dir/hello.txt").exists());
    }

    #[test]
    fn wrong_password_does_not_create_out_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let (z, out) = (tmp.path().join("a.zip"), tmp.path().join("new"));
        make_zip(&z, "1234");

        assert!(extract_zip(&z, "nope", &out).is_err());
        assert!(!out.exists());
    }

    #[test]
    fn extract_wrong_password_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let (z, out) = (tmp.path().join("a.zip"), tmp.path().join("out"));
        fs::create_dir(&out).unwrap();
        make_zip(&z, "1234");

        assert!(extract_zip(&z, "nope", &out).is_err());
        assert!(fs::read_dir(&out).unwrap().next().is_none());
        assert!(z.exists());
    }
}
