//! Redacted diagnostics: never retains file contents, paths, IDs or parser errors.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    Missing,
    Access,
    PermissionsOrType,
    UnexpectedArtifact,
    LockUnavailable,
    Size,
    HeaderOrSchema,
    Truncated,
    JsonSyntax,
    Fields,
    ProtocolVersion,
    Integrity,
    DuplicateRegistration,
    InvalidState,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub issue: Issue,
    pub location: &'static str,
    pub line: Option<usize>,
    pub recovery: &'static str,
    pub stop_reason: &'static str,
    pub manual_check: &'static str,
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} at {} line {:?}; {}; {}; {}",
            self.issue,
            self.location,
            self.line,
            self.stop_reason,
            self.recovery,
            self.manual_check
        )
    }
}
impl std::error::Error for Diagnostic {}
fn error(issue: Issue, location: &'static str, line: Option<usize>) -> Diagnostic {
    Diagnostic {
        issue,
        location,
        line,
        recovery: "自動復旧不可・履歴の完全性を手動確認するまで再開禁止",
        stop_reason: "保存状態または排他性を確認できない",
        manual_check: "原本保全、権限・所有者、履歴の出所と欠損、全利用プロセスを確認",
    }
}
fn io(e: std::io::Error, location: &'static str) -> Diagnostic {
    error(
        if e.kind() == std::io::ErrorKind::NotFound {
            Issue::Missing
        } else {
            Issue::Access
        },
        location,
        None,
    )
}
fn secure(path: &Path, directory: bool) -> std::result::Result<(), Diagnostic> {
    let loc = if directory { "directory" } else { "journal" };
    let m = fs::symlink_metadata(path).map_err(|e| io(e, loc))?;
    if m.file_type().is_symlink()
        || (directory && !m.is_dir())
        || (!directory && !m.is_file())
        || m.permissions().mode() & 0o077 != 0
    {
        return Err(error(Issue::PermissionsOrType, loc, None));
    }
    Ok(())
}
impl StateStore {
    /// Same fail-closed loader as open(), with structured, redacted errors.
    /// Successful loading does not prove protection against deletion or rollback.
    pub fn open_diagnosed(directory: &Path) -> std::result::Result<Self, Diagnostic> {
        secure(directory, true)?;
        let path = directory.join(FILE_NAME);
        secure(&path, false)?;
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(path)
            .map_err(|e| io(e, "journal"))?;
        file.try_lock()
            .map_err(|_| error(Issue::LockUnavailable, "lock", None))?;
        // No temporary files are part of this journal format. Do not guess which copy is newer.
        for item in fs::read_dir(directory).map_err(|e| io(e, "directory"))? {
            if item.map_err(|e| io(e, "directory"))?.file_name() != FILE_NAME {
                return Err(error(Issue::UnexpectedArtifact, "directory", None));
            }
        }
        if file.metadata().map_err(|e| io(e, "journal"))?.len() > MAX_JOURNAL {
            return Err(error(Issue::Size, "journal", None));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_JOURNAL + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io(e, "journal"))?;
        if bytes.len() as u64 > MAX_JOURNAL {
            return Err(error(Issue::Size, "journal", None));
        }
        if !bytes.starts_with(HEADER) {
            return Err(error(Issue::HeaderOrSchema, "header", Some(1)));
        }
        if !bytes.ends_with(b"\n") {
            return Err(error(
                Issue::Truncated,
                "journal",
                Some(bytes.iter().filter(|b| **b == b'\n').count() + 1),
            ));
        }
        let mut records = BTreeMap::new();
        let mut tail = [0; 32];
        for (index, framed) in bytes[HEADER.len()..]
            .split_inclusive(|b| *b == b'\n')
            .enumerate()
        {
            let line = Some(index + 2);
            let data = framed
                .strip_suffix(b"\n")
                .ok_or_else(|| error(Issue::Truncated, "record", line))?;
            if data.len() > MAX_RECORD {
                return Err(error(Issue::Size, "record", line));
            }
            let entry: Entry = serde_json::from_slice(data).map_err(|e| {
                error(
                    if e.is_eof() {
                        Issue::Truncated
                    } else if e.is_syntax() {
                        Issue::JsonSyntax
                    } else {
                        Issue::Fields
                    },
                    "record",
                    line,
                )
            })?;
            if entry.record.protocol_version != PROTOCOL_VERSION {
                return Err(error(Issue::ProtocolVersion, "record", line));
            }
            if entry.previous != tail
                || digest(&entry.record, tail).map_err(|_| error(Issue::Fields, "record", line))?
                    != entry.checksum
            {
                return Err(error(Issue::Integrity, "record", line));
            }
            let old = records.get(&entry.record.request_id);
            if old.is_some() && entry.record.state == StoredState::Created {
                return Err(error(Issue::DuplicateRegistration, "record", line));
            }
            validate_record(&entry.record, old)
                .map_err(|_| error(Issue::InvalidState, "record", line))?;
            tail = entry.checksum;
            records.insert(entry.record.request_id.clone(), entry.record);
        }
        file.seek(SeekFrom::End(0)).map_err(|e| io(e, "journal"))?;
        Ok(Self {
            file,
            records,
            tail,
            poisoned: false,
            #[cfg(test)]
            fail_state: None,
        })
    }
}
