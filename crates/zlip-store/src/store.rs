use crate::crypto::{CryptoError, MasterKey, Sealer};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;
use zlip_core::{Category, ImageData, ItemId, Payload, image::png_dimensions};

const SCHEMA_VERSION: i64 = 1;
const CANARY: &[u8] = b"zlip canary";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("file error: {0}")]
    Io(#[from] io::Error),
    #[error("the history was encrypted with a different key (the stored key was lost or replaced)")]
    WrongKey,
    #[error("a stored item failed authentication and was not loaded")]
    Tampered,
    #[error("stored data is malformed: {0}")]
    Corrupt(&'static str),
    #[error("encryption failed: {0}")]
    Crypto(String),
}

impl From<CryptoError> for StoreError {
    fn from(e: CryptoError) -> Self {
        match e {
            CryptoError::Auth => Self::Tampered,
            other => Self::Crypto(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredKind {
    Text,
    Files,
    Image,
}

impl StoredKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Files => "files",
            Self::Image => "image",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "text" => Some(Self::Text),
            "files" => Some(Self::Files),
            "image" => Some(Self::Image),
            _ => None,
        }
    }
}

/// Everything about an item except its content. Readable without decrypting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredItem {
    pub id: ItemId,
    pub category: Category,
    pub kind: StoredKind,
    /// When it was (last) copied, unix milliseconds. A re-copy moves it to the top.
    pub created_at_ms: i64,
    pub last_used_ms: i64,
    pub use_count: u32,
    pub pinned: bool,
    pub folder: Option<String>,
    pub size_bytes: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// An item plus the text that search needs (images have none).
#[derive(Debug, Clone)]
pub struct IndexedItem {
    pub meta: StoredItem,
    pub source_exe: Option<String>,
    /// Text, or newline-joined paths for a file list.
    pub text: Option<String>,
}

#[derive(Debug)]
pub struct NewItem<'a> {
    pub payload: &'a Payload,
    pub category: Category,
    pub source_exe: Option<&'a str>,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inserted {
    /// The id the item lives under (the existing id when it was a duplicate).
    pub id: ItemId,
    pub duplicate: bool,
}

/// What to keep. Pinned items are always exempt.
#[derive(Debug, Clone, Copy)]
pub struct RetentionPolicy {
    pub max_items: usize,
    pub max_age_secs: Option<u64>,
    pub max_total_bytes: u64,
}

pub struct Store {
    conn: Connection,
    sealer: Sealer,
    images: PathBuf,
    next_id: u64,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Store")
    }
}

fn aad(id: ItemId, field: &str) -> Vec<u8> {
    let mut a = id.0.to_le_bytes().to_vec();
    a.extend_from_slice(field.as_bytes());
    a
}

impl Store {
    /// Open (or create) the history at `db_path`, with images under `images_dir`.
    ///
    /// Returns [`StoreError::WrongKey`] when the file was written with another
    /// key. The caller should quarantine the file and start fresh.
    pub fn open(db_path: &Path, images_dir: &Path, key: &MasterKey) -> Result<Self, StoreError> {
        if let Some(dir) = db_path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::create_dir_all(images_dir)?;
        let conn = Connection::open(db_path)?;
        Self::init(conn, images_dir.to_path_buf(), key)
    }

    fn init(conn: Connection, images: PathBuf, key: &MasterKey) -> Result<Self, StoreError> {
        // secure_delete zeroes freed pages, so deleted rows do not linger in the file.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA secure_delete = ON;",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 0 {
            conn.execute_batch(
                "CREATE TABLE items (
                    id           INTEGER PRIMARY KEY,
                    category     TEXT    NOT NULL,
                    kind         TEXT    NOT NULL,
                    created_at   INTEGER NOT NULL,
                    last_used_at INTEGER NOT NULL,
                    use_count    INTEGER NOT NULL DEFAULT 0,
                    pinned       INTEGER NOT NULL DEFAULT 0,
                    folder       TEXT,
                    size_bytes   INTEGER NOT NULL,
                    width        INTEGER,
                    height       INTEGER,
                    fingerprint  BLOB    NOT NULL UNIQUE,
                    payload      BLOB,
                    image_name   TEXT,
                    meta         BLOB
                 );
                 CREATE INDEX items_created ON items(created_at DESC);
                 CREATE TABLE canary (sealed BLOB NOT NULL);",
            )?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        } else if version != SCHEMA_VERSION {
            return Err(StoreError::Corrupt("unknown database version"));
        }
        let sealer = Sealer::new(key);
        let canary_aad = b"canary";
        match conn
            .query_row("SELECT sealed FROM canary LIMIT 1", [], |r| {
                r.get::<_, Vec<u8>>(0)
            })
            .optional()?
        {
            Some(sealed) => {
                let plain = sealer
                    .open(&sealed, canary_aad)
                    .map_err(|_| StoreError::WrongKey)?;
                if plain.as_slice() != CANARY {
                    return Err(StoreError::WrongKey);
                }
            }
            None => {
                conn.execute(
                    "INSERT INTO canary (sealed) VALUES (?1)",
                    [sealer.seal(CANARY, canary_aad)?],
                )?;
            }
        }
        let max: Option<i64> = conn.query_row("SELECT MAX(id) FROM items", [], |r| r.get(0))?;
        let next_id = max.map_or(1, |m| m as u64 + 1);
        Ok(Self {
            conn,
            sealer,
            images,
            next_id,
        })
    }

    /// One id space for history items *and* vault items, so they never collide.
    pub fn allocate_id(&mut self) -> ItemId {
        let id = ItemId(self.next_id);
        self.next_id += 1;
        id
    }

    fn image_path(&self, name: &str) -> PathBuf {
        self.images.join(format!("{name}.zimg"))
    }

    /// Store `item` under `id`, or bump the existing identical item to the top.
    pub fn insert(&mut self, id: ItemId, item: &NewItem<'_>) -> Result<Inserted, StoreError> {
        let (kind, bytes): (StoredKind, Zeroizing<Vec<u8>>) = match item.payload {
            Payload::Text(t) => (StoredKind::Text, Zeroizing::new(t.clone().into_bytes())),
            Payload::Files(f) => (StoredKind::Files, Zeroizing::new(f.join("\n").into_bytes())),
            Payload::Image(i) => (StoredKind::Image, Zeroizing::new(i.png.clone())),
        };
        let fp = self.sealer.fingerprint(kind.as_str(), &bytes);

        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM items WHERE fingerprint = ?1",
                [&fp[..]],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            self.conn.execute(
                "UPDATE items SET created_at = ?1 WHERE id = ?2",
                params![item.now_ms, existing],
            )?;
            return Ok(Inserted {
                id: ItemId(existing as u64),
                duplicate: true,
            });
        }

        let meta = self.sealer.seal(
            item.source_exe.unwrap_or_default().as_bytes(),
            &aad(id, "meta"),
        )?;
        let (payload_blob, image_name, w, h) = if let Payload::Image(img) = item.payload {
            let name: String = fp.iter().map(|b| format!("{b:02x}")).collect();
            let sealed = self.sealer.seal(&img.png, name.as_bytes())?;
            fs::write(self.image_path(&name), sealed)?;
            (None, Some(name), Some(img.width), Some(img.height))
        } else {
            (
                Some(self.sealer.seal(&bytes, &aad(id, "payload"))?),
                None,
                None,
                None,
            )
        };

        self.conn.execute(
            "INSERT INTO items (id, category, kind, created_at, last_used_at, size_bytes, width, height, fingerprint, payload, image_name, meta)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id.0 as i64,
                item.category.as_str(),
                kind.as_str(),
                item.now_ms,
                item.payload.size_bytes() as i64,
                w,
                h,
                &fp[..],
                payload_blob,
                image_name,
                meta
            ],
        )?;
        Ok(Inserted {
            id,
            duplicate: false,
        })
    }

    fn row_to_meta(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredItem> {
        let category: String = r.get("category")?;
        let kind: String = r.get("kind")?;
        Ok(StoredItem {
            id: ItemId(r.get::<_, i64>("id")? as u64),
            category: Category::from_name(&category).unwrap_or(Category::Text),
            kind: StoredKind::parse(&kind).unwrap_or(StoredKind::Text),
            created_at_ms: r.get("created_at")?,
            last_used_ms: r.get("last_used_at")?,
            use_count: r.get::<_, i64>("use_count")? as u32,
            pinned: r.get::<_, i64>("pinned")? != 0,
            folder: r.get("folder")?,
            size_bytes: r.get::<_, i64>("size_bytes")? as u64,
            width: r.get("width")?,
            height: r.get("height")?,
        })
    }

    fn decrypt_meta(
        &self,
        id: ItemId,
        sealed: Option<Vec<u8>>,
    ) -> Result<Option<String>, StoreError> {
        let Some(sealed) = sealed else {
            return Ok(None);
        };
        let plain = self.sealer.open(&sealed, &aad(id, "meta"))?;
        let s = String::from_utf8(plain.to_vec())
            .map_err(|_| StoreError::Corrupt("source name is not UTF-8"))?;
        Ok((!s.is_empty()).then_some(s))
    }

    /// Everything the in-RAM search index needs, newest first. Rows that fail
    /// authentication are skipped (and counted in the second value) rather than
    /// failing the whole load.
    pub fn load_index(&self) -> Result<(Vec<IndexedItem>, usize), StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, category, kind, created_at, last_used_at, use_count, pinned, folder, size_bytes, width, height, payload, meta
             FROM items ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                Self::row_to_meta(r)?,
                r.get::<_, Option<Vec<u8>>>("payload")?,
                r.get::<_, Option<Vec<u8>>>("meta")?,
            ))
        })?;
        let (mut out, mut skipped) = (Vec::new(), 0);
        for row in rows {
            let (meta, payload, sealed_meta) = row?;
            let text = match (meta.kind, payload) {
                (StoredKind::Image, _) | (_, None) => Ok(None),
                (_, Some(p)) => self
                    .sealer
                    .open(&p, &aad(meta.id, "payload"))
                    .map_err(StoreError::from)
                    .and_then(|plain| {
                        String::from_utf8(plain.to_vec())
                            .map(Some)
                            .map_err(|_| StoreError::Corrupt("text is not UTF-8"))
                    }),
            };
            match (text, self.decrypt_meta(meta.id, sealed_meta)) {
                (Ok(text), Ok(source_exe)) => out.push(IndexedItem {
                    meta,
                    source_exe,
                    text,
                }),
                _ => skipped += 1,
            }
        }
        Ok((out, skipped))
    }

    /// Full content of one item (decrypts the image file when needed).
    pub fn get_payload(&self, id: ItemId) -> Result<Option<Payload>, StoreError> {
        let row = self
            .conn
            .query_row(
                "SELECT kind, payload, image_name, width, height FROM items WHERE id = ?1",
                [id.0 as i64],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<Vec<u8>>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<u32>>(3)?,
                        r.get::<_, Option<u32>>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((kind, payload, image_name, w, h)) = row else {
            return Ok(None);
        };
        match StoredKind::parse(&kind) {
            Some(StoredKind::Image) => {
                let name = image_name.ok_or(StoreError::Corrupt("image row has no file"))?;
                let sealed = fs::read(self.image_path(&name))?;
                let png = self.sealer.open(&sealed, name.as_bytes())?;
                let (width, height) = match (w, h) {
                    (Some(w), Some(h)) => (w, h),
                    _ => png_dimensions(&png)
                        .ok_or(StoreError::Corrupt("image has no dimensions"))?,
                };
                Ok(Some(Payload::Image(ImageData {
                    png: png.to_vec(),
                    width,
                    height,
                })))
            }
            Some(k @ (StoredKind::Text | StoredKind::Files)) => {
                let sealed = payload.ok_or(StoreError::Corrupt("row has no payload"))?;
                let plain = self.sealer.open(&sealed, &aad(id, "payload"))?;
                let text = String::from_utf8(plain.to_vec())
                    .map_err(|_| StoreError::Corrupt("text is not UTF-8"))?;
                Ok(Some(if k == StoredKind::Files {
                    Payload::Files(text.split('\n').map(str::to_owned).collect())
                } else {
                    Payload::Text(text)
                }))
            }
            None => Err(StoreError::Corrupt("unknown item kind")),
        }
    }

    pub fn set_pinned(&self, id: ItemId, pinned: bool) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE items SET pinned = ?1 WHERE id = ?2",
            params![i64::from(pinned), id.0 as i64],
        )?;
        Ok(())
    }

    pub fn set_folder(&self, id: ItemId, folder: Option<&str>) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE items SET folder = ?1 WHERE id = ?2",
            params![folder, id.0 as i64],
        )?;
        Ok(())
    }

    /// Record a paste (feeds future ranking).
    pub fn touch(&self, id: ItemId, now_ms: i64) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE items SET use_count = use_count + 1, last_used_at = ?1 WHERE id = ?2",
            params![now_ms, id.0 as i64],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: ItemId) -> Result<bool, StoreError> {
        let name: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT image_name FROM items WHERE id = ?1",
                [id.0 as i64],
                |r| r.get(0),
            )
            .optional()?;
        let Some(name) = name else { return Ok(false) };
        self.conn
            .execute("DELETE FROM items WHERE id = ?1", [id.0 as i64])?;
        if let Some(name) = name {
            let _ = fs::remove_file(self.image_path(&name));
        }
        Ok(true)
    }

    /// Delete every unpinned item. Returns the ids removed.
    pub fn clear_unpinned(&self) -> Result<Vec<ItemId>, StoreError> {
        let ids = self.ids("SELECT id FROM items WHERE pinned = 0 ORDER BY id", [])?;
        for id in &ids {
            self.delete(*id)?;
        }
        Ok(ids)
    }

    fn ids(&self, sql: &str, p: impl rusqlite::Params) -> Result<Vec<ItemId>, StoreError> {
        let mut stmt = self.conn.prepare(sql)?;
        let ids = stmt
            .query_map(p, |r| Ok(ItemId(r.get::<_, i64>(0)? as u64)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Apply age, count and size limits (oldest unpinned first). Returns what was deleted.
    pub fn enforce_retention(
        &self,
        policy: &RetentionPolicy,
        now_ms: i64,
    ) -> Result<Vec<ItemId>, StoreError> {
        let mut doomed: Vec<ItemId> = Vec::new();
        if let Some(age) = policy.max_age_secs {
            let cutoff = now_ms.saturating_sub((age as i64).saturating_mul(1000));
            doomed.extend(self.ids(
                "SELECT id FROM items WHERE pinned = 0 AND created_at < ?1",
                [cutoff],
            )?);
        }
        // Newest-first list of what survives the age rule, for the count and size rules.
        let survivors = {
            let mut stmt = self.conn.prepare(
                "SELECT id, size_bytes FROM items WHERE pinned = 0 ORDER BY created_at DESC",
            )?;
            stmt.query_map([], |r| {
                Ok((
                    ItemId(r.get::<_, i64>(0)? as u64),
                    r.get::<_, i64>(1)? as u64,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|(id, _)| !doomed.contains(id))
            .collect::<Vec<_>>()
        };
        let mut total = 0u64;
        for (i, (id, size)) in survivors.iter().enumerate() {
            total += size;
            if i >= policy.max_items || total > policy.max_total_bytes {
                doomed.push(*id);
            }
        }
        for id in &doomed {
            self.delete(*id)?;
        }
        Ok(doomed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _dir: tempfile::TempDir,
        db: PathBuf,
        images: PathBuf,
    }
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        Fixture {
            db: dir.path().join("h.db"),
            images: dir.path().join("images"),
            _dir: dir,
        }
    }
    fn key(b: u8) -> MasterKey {
        MasterKey::from_bytes([b; 32])
    }
    fn text(s: &str) -> Payload {
        Payload::Text(s.into())
    }
    fn add(st: &mut Store, p: &Payload, cat: Category, now: i64) -> Inserted {
        let id = st.allocate_id();
        st.insert(
            id,
            &NewItem {
                payload: p,
                category: cat,
                source_exe: Some("notepad.exe"),
                now_ms: now,
            },
        )
        .unwrap()
    }

    #[test]
    fn text_survives_a_restart_and_the_file_never_contains_it() {
        let f = fixture();
        let secret = "needle-in-the-haystack-12345";
        {
            let mut st = Store::open(&f.db, &f.images, &key(1)).unwrap();
            add(&mut st, &text(secret), Category::Text, 100);
        }
        let raw = fs::read(&f.db).unwrap();
        assert!(
            !raw.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "plaintext found in the database file"
        );
        let wal = fs::read(f.db.with_extension("db-wal")).unwrap_or_default();
        assert!(
            !wal.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "plaintext found in the WAL"
        );

        let st = Store::open(&f.db, &f.images, &key(1)).unwrap();
        let (items, skipped) = st.load_index().unwrap();
        assert_eq!((items.len(), skipped), (1, 0));
        assert_eq!(items[0].text.as_deref(), Some(secret));
        assert_eq!(items[0].source_exe.as_deref(), Some("notepad.exe"));
    }

    #[test]
    fn a_different_key_is_detected_not_silently_accepted() {
        let f = fixture();
        Store::open(&f.db, &f.images, &key(1)).unwrap();
        assert!(matches!(
            Store::open(&f.db, &f.images, &key(2)),
            Err(StoreError::WrongKey)
        ));
    }

    #[test]
    fn duplicates_collapse_and_move_to_the_top_keeping_pin_state() {
        let f = fixture();
        let mut st = Store::open(&f.db, &f.images, &key(1)).unwrap();
        let first = add(&mut st, &text("same"), Category::Text, 100);
        add(&mut st, &text("other"), Category::Text, 200);
        st.set_pinned(first.id, true).unwrap();
        let again = add(&mut st, &text("same"), Category::Text, 300);
        assert!(again.duplicate);
        assert_eq!(again.id, first.id);
        let (items, _) = st.load_index().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text.as_deref(), Some("same"), "bumped to the top");
        assert!(items[0].meta.pinned);
    }

    #[test]
    fn tampered_rows_are_skipped_and_counted_not_fatal() {
        let f = fixture();
        {
            let mut st = Store::open(&f.db, &f.images, &key(1)).unwrap();
            add(&mut st, &text("good"), Category::Text, 100);
            add(&mut st, &text("victim"), Category::Text, 200);
            let c = &st.conn;
            // swap in another row's ciphertext: AAD binds ciphertext to its own row id
            c.execute(
                "UPDATE items SET payload = (SELECT payload FROM items WHERE id = 1) WHERE id = 2",
                [],
            )
            .unwrap();
        }
        let st = Store::open(&f.db, &f.images, &key(1)).unwrap();
        let (items, skipped) = st.load_index().unwrap();
        assert_eq!((items.len(), skipped), (1, 1));
        assert_eq!(items[0].text.as_deref(), Some("good"));
    }

    #[test]
    fn file_lists_round_trip() {
        let f = fixture();
        let mut st = Store::open(&f.db, &f.images, &key(1)).unwrap();
        let files = Payload::Files(vec!["C:/a b/c.txt".into(), "C:/d.png".into()]);
        let id = add(&mut st, &files, Category::File, 1).id;
        assert_eq!(st.get_payload(id).unwrap(), Some(files));
        assert_eq!(st.get_payload(ItemId(999)).unwrap(), None);
    }

    #[test]
    fn images_are_encrypted_files_deleted_with_their_row() {
        let f = fixture();
        let mut st = Store::open(&f.db, &f.images, &key(1)).unwrap();
        let png = zlip_core::image::encode_png(&zlip_core::image::Rgba8Image {
            width: 2,
            height: 2,
            pixels: vec![9; 16],
        })
        .unwrap();
        let img = Payload::Image(ImageData {
            png: png.clone(),
            width: 2,
            height: 2,
        });
        let id = add(&mut st, &img, Category::Image, 1).id;
        let files: Vec<_> = fs::read_dir(&f.images).unwrap().collect();
        assert_eq!(files.len(), 1);
        let on_disk = fs::read(files[0].as_ref().unwrap().path()).unwrap();
        assert!(
            !on_disk.windows(4).any(|w| w == b"\x89PNG"),
            "image file must be ciphertext"
        );
        assert_eq!(st.get_payload(id).unwrap(), Some(img));
        assert!(st.delete(id).unwrap());
        assert_eq!(fs::read_dir(&f.images).unwrap().count(), 0);
        assert!(!st.delete(id).unwrap());
    }

    #[test]
    fn metadata_updates() {
        let f = fixture();
        let mut st = Store::open(&f.db, &f.images, &key(1)).unwrap();
        let id = add(&mut st, &text("x"), Category::Text, 5).id;
        st.set_pinned(id, true).unwrap();
        st.set_folder(id, Some("Terminal")).unwrap();
        st.touch(id, 50).unwrap();
        st.touch(id, 60).unwrap();
        let m = &st.load_index().unwrap().0[0].meta;
        assert_eq!(
            (m.pinned, m.folder.as_deref(), m.use_count, m.last_used_ms),
            (true, Some("Terminal"), 2, 60)
        );
    }

    #[test]
    fn ids_continue_after_a_restart() {
        let f = fixture();
        let last = {
            let mut st = Store::open(&f.db, &f.images, &key(1)).unwrap();
            add(&mut st, &text("a"), Category::Text, 1);
            add(&mut st, &text("b"), Category::Text, 2).id
        };
        let mut st = Store::open(&f.db, &f.images, &key(1)).unwrap();
        assert!(st.allocate_id() > last);
    }

    fn policy(max_items: usize, age: Option<u64>, bytes: u64) -> RetentionPolicy {
        RetentionPolicy {
            max_items,
            max_age_secs: age,
            max_total_bytes: bytes,
        }
    }

    #[test]
    fn retention_by_age_count_and_size_spares_pinned() {
        let f = fixture();
        let mut st = Store::open(&f.db, &f.images, &key(1)).unwrap();
        let old = add(&mut st, &text("old"), Category::Text, 1_000).id;
        let pinned_old = add(&mut st, &text("pinned"), Category::Text, 1_000).id;
        st.set_pinned(pinned_old, true).unwrap();
        for n in 0..5 {
            add(
                &mut st,
                &text(&format!("recent-{n}")),
                Category::Text,
                10_000_000 + n,
            );
        }
        let now = 10_000_100;
        // age: anything older than 100 s goes (unless pinned)
        let gone = st
            .enforce_retention(&policy(100, Some(100), u64::MAX), now)
            .unwrap();
        assert_eq!(gone, vec![old]);
        // count: keep the newest 3 unpinned
        let gone = st
            .enforce_retention(&policy(3, None, u64::MAX), now)
            .unwrap();
        assert_eq!(gone.len(), 2);
        // size: each remaining item is 8-9 bytes; a 10-byte budget keeps one
        let gone = st.enforce_retention(&policy(100, None, 10), now).unwrap();
        assert_eq!(gone.len(), 2);
        let (left, _) = st.load_index().unwrap();
        assert_eq!(left.len(), 2, "one recent item plus the pinned one");
        assert!(
            left.iter().any(|i| i.meta.id == pinned_old),
            "pinned survives every rule"
        );
    }

    #[test]
    fn clear_unpinned_keeps_pinned() {
        let f = fixture();
        let mut st = Store::open(&f.db, &f.images, &key(1)).unwrap();
        let keep = add(&mut st, &text("keep"), Category::Text, 1).id;
        add(&mut st, &text("drop"), Category::Text, 2);
        st.set_pinned(keep, true).unwrap();
        assert_eq!(st.clear_unpinned().unwrap().len(), 1);
        assert_eq!(st.load_index().unwrap().0.len(), 1);
    }
}
