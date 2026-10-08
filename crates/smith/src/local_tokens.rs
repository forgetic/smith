//! Durable user-only OAuth records (protocol/hosts.md, section 5.4;
//! domain/host.md, section 7). The shell owns the configured token directory,
//! never logs token bytes, and acknowledges a replacement only after syncing
//! its file and directory. Records are decoded by Skein before use.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use skein_oauth as oauth;

/// The user's private configuration directory for saved OAuth records.
pub struct Tokens {
    root: PathBuf,
    limit: oauth::Limits,
}

impl Tokens {
    pub fn new(root: &Path, limit: oauth::Limits) -> Result<Self, String> {
        match DirBuilder::new().mode(0o700).create(root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("token directory: {error}")),
        }
        let metadata = fs::symlink_metadata(root).map_err(|error| format!("token directory metadata: {error}"))?;
        if !metadata.is_dir() || metadata.mode() & 0o077 != 0 {
            return Err("token directory must be a private directory (mode 0700)".into());
        }
        Ok(Self { root: root.into(), limit })
    }

    pub fn load(&self, account: u32) -> Result<Option<oauth::SavedToken>, String> {
        let path = self.root.join(format!("{account}.json"));
        let file = match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("token record open: {error}")),
        };
        let metadata = file.metadata().map_err(|error| format!("token record metadata: {error}"))?;
        let directory =
            fs::symlink_metadata(&self.root).map_err(|error| format!("token directory metadata: {error}"))?;
        if !metadata.is_file()
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
            || metadata.uid() != directory.uid()
            || metadata.len() > u64::from(self.limit.record_bytes)
        {
            return Err("token record must be a bounded private regular file (mode 0600)".into());
        }
        let mut bytes = Vec::new();
        file.take(u64::from(self.limit.record_bytes).saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| format!("token record read: {error}"))?;
        let record = oauth::decode_record(&bytes, &self.limit).map_err(|_| "invalid token record")?;
        if record.key != account {
            return Err("token record account differs from its filename".into());
        }
        Ok(Some(record))
    }

    pub fn save(&self, account: u32, bytes: &[u8]) -> Result<(), String> {
        let record = oauth::decode_record(bytes, &self.limit).map_err(|_| "invalid token candidate")?;
        if record.key != account {
            return Err("token candidate account differs from its filename".into());
        }
        let temporary = self.root.join(format!(".{account}.pending"));
        // A stale temporary is never opened or truncated; remove its directory
        // entry before exclusive creation, which also refuses a racing symlink.
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("token temporary removal: {error}")),
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)
            .map_err(|error| format!("token temporary open: {error}"))?;
        file.write_all(bytes).map_err(|error| format!("token record write: {error}"))?;
        file.sync_all().map_err(|error| format!("token record sync: {error}"))?;
        fs::rename(&temporary, self.root.join(format!("{account}.json")))
            .map_err(|error| format!("token record rename: {error}"))?;
        File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("token directory sync: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skein_lib::Wall;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn limits() -> oauth::Limits {
        oauth::Limits {
            document_bytes: 4096,
            string_bytes: 1024,
            token_bytes: 512,
            client_bytes: 256,
            detail_bytes: 256,
            record_bytes: 4096,
            depth: 8,
            tokens: 64,
        }
    }

    fn candidate(account: u32, token: &[u8]) -> Box<[u8]> {
        oauth::encode_record(
            &oauth::SavedToken {
                key: account,
                generation: 1,
                access_token: token.into(),
                refresh_token: b"refresh".as_slice().into(),
                metadata: None,
                expires_at: Wall::from_nanos(30_000_000_000),
            },
            &limits(),
        )
        .expect("token encoding")
    }

    #[test]
    fn saved_records_are_private_and_replaced_as_complete_records() {
        let root = std::env::temp_dir().join(format!("smith-private-tokens-{}", std::process::id()));
        let store = Tokens::new(&root, limits()).expect("private directory");
        assert!(store.load(7).expect("missing record").is_none());
        store.save(7, &candidate(7, b"first")).expect("first durable save");
        store.save(7, &candidate(7, b"second")).expect("replacement durable save");
        assert_eq!(fs::metadata(root.join("7.json")).expect("file mode").mode() & 0o777, 0o600);
        assert_eq!(store.load(7).expect("saved record").expect("present").access_token.as_ref(), b"second");
        assert!(store.save(8, &candidate(7, b"wrong-account")).is_err());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn public_records_and_symlinks_are_refused_without_reading_the_target() {
        let root = std::env::temp_dir().join(format!("smith-refused-tokens-{}", std::process::id()));
        let store = Tokens::new(&root, limits()).expect("private directory");
        store.save(7, &candidate(7, b"first")).expect("save");
        fs::set_permissions(root.join("7.json"), fs::Permissions::from_mode(0o644)).expect("make public");
        assert!(store.load(7).is_err());
        symlink(root.join("7.json"), root.join("8.json")).expect("symlink");
        assert!(store.load(8).is_err());
        fs::remove_dir_all(root).expect("cleanup");
    }
}
