use crate::{
    entry::{Entry, app_name, classify_text},
    view::{
        ActionReport, FolderView, ItemDetail, ItemView, OsInfo, QueueView, StatusView, UiConfig,
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::NaiveDateTime;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
use zlip_actions::{ActionError, ActionInput, RunOutcome, TrustStore, run_script};
use zlip_config::{Action, Config, ConfigHandle};
use zlip_core::{
    Candidate, Category, ImageData, ItemId, PasteQueue, Payload, PlaceholderContext, QueueMode,
    Searcher, Transform, TransformError, apply_transforms, classify, expand_placeholders,
    image::{decode_png, encode_png, thumbnail},
    placeholder::{PlaceholderError, has_placeholders},
};
use zlip_highlight::{escape_html, render};
use zlip_security::{
    Blacklist, CaptureFacts, ClipboardGuard, SensitiveVault, SkipReason, detector::detect_sensitive,
};
use zlip_store::{NewItem, RetentionPolicy, Store, StoreError, StoredKind};

/// `folder` filter value meaning "history items I pinned".
pub const PINNED_FILTER: &str = "@pinned";
/// Ids at or above this belong to config snippets (they are not in the database).
const VIRTUAL_BASE: u64 = 1 << 62;
const MAX_TOTAL_HISTORY_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EDITABLE_BYTES: usize = 256 * 1024;
const MAX_INLINE_IMAGE_BYTES: usize = 6 * 1024 * 1024;
const THUMB_SIDE: u32 = 160;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[derive(Debug, thiserror::Error)]
pub enum PasteError {
    #[error("that item no longer exists")]
    NotFound,
    #[error("that secret has expired")]
    Expired,
    #[error("{0}")]
    Transform(#[from] TransformError),
    #[error("{0}")]
    Placeholder(#[from] PlaceholderError),
    #[error("could not read the item: {0}")]
    Store(String),
}

/// Never printed in full: the payload may be a secret.
pub struct CaptureInput {
    pub payload: Payload,
    /// Full path of the program that made the copy, when known.
    pub owner_exe: Option<String>,
    /// Clipboard sequence number read while the clipboard was held open.
    pub clip_seq: u32,
    pub now_ms: i64,
    pub now: Instant,
    /// Append to the paste queue when it is collecting.
    pub enqueue: bool,
}

impl std::fmt::Debug for CaptureInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CaptureInput({:?}, enqueue={})",
            self.payload, self.enqueue
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CaptureOutcome {
    Stored {
        id: ItemId,
        category: Category,
        duplicate: bool,
        queued: bool,
    },
    Sensitive {
        id: ItemId,
        label: &'static str,
        queued: bool,
    },
    Ignored(&'static str),
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    pub category: Option<Category>,
    /// A snippet folder name, or [`PINNED_FILTER`].
    pub folder: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PasteOptions {
    pub transforms: Vec<Transform>,
    /// Paste text only (a file list becomes its paths).
    pub plain: bool,
}

/// Holds the previous clipboard text, which may be a secret, so `Debug` hides it.
pub struct PasteContext {
    /// Local wall-clock time, for `{date}` and `{time}`.
    pub now_local: NaiveDateTime,
    /// Text on the clipboard before Zlip overwrote it, for `{clipboard}`.
    pub clipboard: Option<String>,
    pub now: Instant,
}

impl std::fmt::Debug for PasteContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PasteContext(clipboard: {} bytes)",
            self.clipboard.as_ref().map_or(0, String::len)
        )
    }
}

/// What to put on the clipboard. Text is wiped from memory when dropped.
pub enum PasteContent {
    Text(Zeroizing<String>),
    Files(Vec<String>),
    Image(ImageData),
}

impl std::fmt::Debug for PasteContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(t) => write!(f, "Text({} bytes)", t.len()),
            Self::Files(v) => write!(f, "Files({})", v.len()),
            Self::Image(i) => write!(f, "Image({}x{})", i.width, i.height),
        }
    }
}

#[derive(Debug)]
pub struct PreparedPaste {
    pub id: ItemId,
    pub content: PasteContent,
    /// Left-arrow presses to land on `{cursor}` after pasting.
    pub cursor_back: usize,
    pub sensitive: bool,
}

pub struct Engine {
    cfg: ConfigHandle,
    seen: Arc<Config>,
    blacklist: Blacklist,
    store: Store,
    vault: SensitiveVault,
    sensitive_created: HashMap<ItemId, i64>,
    queue: PasteQueue,
    searcher: Searcher,
    /// Newest first.
    entries: Vec<Entry>,
    snippets: Vec<Entry>,
    snippet_bodies: HashMap<ItemId, String>,
    thumbs: HashMap<ItemId, String>,
    private_mode: bool,
    trust: TrustStore,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Engine({} items)", self.entries.len())
    }
}

fn io_err(e: &StoreError) -> PasteError {
    PasteError::Store(e.to_string())
}

impl Engine {
    /// Load the history into the index. Also returns how many stored rows failed
    /// authentication and were skipped.
    pub fn new(
        cfg: ConfigHandle,
        store: Store,
        trust_file: PathBuf,
    ) -> Result<(Self, usize), EngineError> {
        let (rows, skipped) = store.load_index()?;
        let seen = cfg.snapshot();
        let mut engine = Self {
            blacklist: Blacklist::from_config(&seen.security),
            cfg,
            seen,
            store,
            vault: SensitiveVault::new(),
            sensitive_created: HashMap::new(),
            queue: PasteQueue::new(),
            searcher: Searcher::new(),
            entries: rows.into_iter().map(Entry::from_indexed).collect(),
            snippets: Vec::new(),
            snippet_bodies: HashMap::new(),
            thumbs: HashMap::new(),
            private_mode: false,
            trust: TrustStore::load(trust_file),
        };
        engine.rebuild_snippets();
        Ok((engine, skipped))
    }

    // ------------------------------------------------------------ config

    /// The current config; rebuilds everything derived from it when it changed.
    fn cfg(&mut self) -> Arc<Config> {
        let now = self.cfg.snapshot();
        if !Arc::ptr_eq(&now, &self.seen) {
            self.blacklist = Blacklist::from_config(&now.security);
            self.seen = Arc::clone(&now);
            self.rebuild_snippets();
        }
        now
    }

    fn rebuild_snippets(&mut self) {
        let (mut n, mut entries, mut bodies) = (0u64, Vec::new(), HashMap::new());
        for folder in &self.seen.snippet_folders {
            for sn in &folder.items {
                let id = ItemId(VIRTUAL_BASE + n);
                n += 1;
                let mut e = Entry::build(
                    id,
                    Category::Snippet,
                    StoredKind::Text,
                    Some(&sn.body),
                    0,
                    false,
                    Some(folder.name.clone()),
                    sn.body.len() as u64,
                    None,
                    None,
                    None,
                    true,
                );
                // The title is what you read; title and body are both searchable;
                // the body alone is what gets pasted (see `full_text`).
                let first_body_line = sn
                    .body
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or_default()
                    .trim();
                e.preview = if first_body_line.is_empty() {
                    sn.title.clone()
                } else {
                    format!("{}\n{first_body_line}", sn.title)
                };
                e.head = format!("{}\n{}", sn.title, sn.body);
                e.complete = false;
                bodies.insert(id, sn.body.clone());
                entries.push(e);
            }
        }
        self.snippets = entries;
        self.snippet_bodies = bodies;
    }

    fn entry(&self, id: ItemId) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .or_else(|| self.snippets.iter().find(|e| e.id == id))
    }

    fn is_secret(&self, id: ItemId) -> bool {
        self.sensitive_created.contains_key(&id)
    }

    fn full_text(&self, e: &Entry) -> Result<String, PasteError> {
        if e.read_only {
            return self
                .snippet_bodies
                .get(&e.id)
                .cloned()
                .ok_or(PasteError::NotFound);
        }
        if e.complete {
            return Ok(e.head.clone());
        }
        match self.store.get_payload(e.id).map_err(|x| io_err(&x))? {
            Some(Payload::Text(t)) => Ok(t),
            Some(Payload::Files(f)) => Ok(f.join("\n")),
            _ => Err(PasteError::NotFound),
        }
    }

    // ----------------------------------------------------------- capture

    /// Whether `text` would be treated as a secret under the current settings.
    pub fn looks_sensitive(&mut self, text: &str) -> bool {
        let cfg = self.cfg();
        detect_sensitive(text, &cfg.security).is_some()
    }

    /// Decide *before reading any content* whether this copy may be captured.
    pub fn should_capture(&mut self, mut facts: CaptureFacts<'_>) -> Option<SkipReason> {
        self.cfg();
        facts.private_mode = self.private_mode;
        self.blacklist.decide(&facts)
    }

    /// Store, classify and index one clipboard capture.
    pub fn ingest(&mut self, input: CaptureInput) -> CaptureOutcome {
        let cfg = self.cfg();
        let CaptureInput {
            payload,
            owner_exe,
            clip_seq,
            now_ms,
            now,
            enqueue,
        } = input;

        match &payload {
            Payload::Text(t) if t.trim().is_empty() => return CaptureOutcome::Ignored("empty"),
            Payload::Text(t) if t.len() > cfg.retention.max_text_kb as usize * 1024 => {
                return CaptureOutcome::Ignored("too large");
            }
            Payload::Image(i)
                if i.png.len() > cfg.retention.max_image_mb as usize * 1024 * 1024 =>
            {
                return CaptureOutcome::Ignored("too large");
            }
            Payload::Files(f) if f.is_empty() => return CaptureOutcome::Ignored("empty"),
            _ => {}
        }

        // Secrets never reach the database, the index, automations or logs.
        let payload = match payload {
            Payload::Text(text) => {
                if let Some(kind) = detect_sensitive(&text, &cfg.security) {
                    let id = self.store.allocate_id();
                    let ttl = Duration::from_secs(u64::from(cfg.security.sensitive_ttl_secs));
                    self.vault.insert(id, text, kind, clip_seq, ttl, now);
                    self.sensitive_created.insert(id, now_ms);
                    let queued = enqueue && self.queue.push(id);
                    return CaptureOutcome::Sensitive {
                        id,
                        label: kind.label(),
                        queued,
                    };
                }
                Payload::Text(text)
            }
            other => other,
        };

        let (payload, category, language) =
            self.apply_automations(&cfg, payload, owner_exe.as_deref());

        let id = self.store.allocate_id();
        let stored = match self.store.insert(
            id,
            &NewItem {
                payload: &payload,
                category,
                source_exe: owner_exe.as_deref(),
                now_ms,
            },
        ) {
            Ok(s) => s,
            Err(e) => return CaptureOutcome::Failed(e.to_string()),
        };
        if stored.duplicate {
            if let Some(pos) = self.entries.iter().position(|e| e.id == stored.id) {
                let mut e = self.entries.remove(pos);
                e.created_ms = now_ms;
                self.entries.insert(0, e);
            }
        } else {
            self.entries.insert(
                0,
                Entry::from_capture(stored.id, &payload, category, language, owner_exe, now_ms),
            );
        }
        let queued = enqueue && self.queue.push(stored.id);
        CaptureOutcome::Stored {
            id: stored.id,
            category,
            duplicate: stored.duplicate,
            queued,
        }
    }

    fn apply_automations(
        &self,
        cfg: &Config,
        payload: Payload,
        exe: Option<&str>,
    ) -> (Payload, Category, Option<zlip_highlight::Language>) {
        let cls = classify(&payload);
        let Payload::Text(text) = payload else {
            return (payload, cls.category, cls.language);
        };
        let mut current = text.clone();
        for rule in cfg
            .automations
            .iter()
            .filter(|a| a.matches(cls.category, exe))
        {
            match apply_transforms(&rule.then, &current) {
                Ok(next) => current = next,
                Err(e) => tracing::debug!("automation skipped: {e}"),
            }
        }
        if current == text {
            return (Payload::Text(text), cls.category, cls.language);
        }
        let (category, language) = classify_text(&current);
        (Payload::Text(current), category, language)
    }

    // ------------------------------------------------------------- views

    /// Fuzzy search. Secrets take part only through their masked label, never their content.
    pub fn search(&mut self, q: &SearchQuery, now: Instant) -> Vec<ItemView> {
        self.cfg();
        let limit = q.limit.clamp(1, 200);
        let has_query = !q.text.trim().is_empty();
        let folder = q.folder.as_deref();

        let masked: Vec<(ItemId, String, i64)> = self
            .vault
            .entries(now)
            .into_iter()
            .map(|v| {
                (
                    v.id,
                    v.masked,
                    self.sensitive_created.get(&v.id).copied().unwrap_or(0),
                )
            })
            .collect();

        let mut cands: Vec<Candidate<'_>> =
            Vec::with_capacity(self.entries.len() + self.snippets.len() + masked.len());
        for e in &self.entries {
            let in_folder = match folder {
                None => true,
                Some(PINNED_FILTER) => e.pinned,
                Some(name) => e.folder.as_deref() == Some(name),
            };
            if in_folder {
                cands.push(Candidate {
                    id: e.id,
                    text: &e.head,
                    category: e.category,
                    recency: e.created_ms,
                });
            }
        }
        // Snippets live in their folders; they join the main list only when searching.
        if folder.is_some_and(|f| f != PINNED_FILTER) || (folder.is_none() && has_query) {
            for (i, s) in self.snippets.iter().enumerate() {
                if folder.is_none_or(|f| s.folder.as_deref() == Some(f)) {
                    cands.push(Candidate {
                        id: s.id,
                        text: &s.head,
                        category: s.category,
                        recency: -(i as i64) - 1,
                    });
                }
            }
        }
        if folder.is_none() {
            for (id, text, created) in &masked {
                cands.push(Candidate {
                    id: *id,
                    text,
                    category: Category::Sensitive,
                    recency: *created,
                });
            }
        }

        let ids = self.searcher.search(&q.text, cands, q.category, limit);
        let positions: HashMap<ItemId, usize> = self
            .queue
            .snapshot()
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, i + 1))
            .collect();
        let vault_entries = self.vault.entries(now);

        ids.into_iter()
            .filter_map(|id| {
                if let Some(v) = vault_entries.iter().find(|v| v.id == id) {
                    return Some(ItemView {
                        id: id.0,
                        category: Category::Sensitive,
                        label: Some(v.kind.label().to_owned()),
                        preview: v.masked.clone(),
                        preview_html: None,
                        color: None,
                        created_at_ms: self.sensitive_created.get(&id).copied().unwrap_or(0),
                        pinned: false,
                        folder: None,
                        size_bytes: 0,
                        width: None,
                        height: None,
                        source_app: None,
                        expires_in_ms: Some(
                            u64::try_from(v.expires_in.as_millis()).unwrap_or(u64::MAX),
                        ),
                        queue_position: positions.get(&id).copied(),
                        read_only: false,
                    });
                }
                self.entry(id).map(|e| e.view(positions.get(&id).copied()))
            })
            .collect()
    }

    /// Folder chips: pinned history items and the folders defined in the config.
    pub fn folders(&mut self) -> Vec<FolderView> {
        let cfg = self.cfg();
        let mut out = Vec::new();
        let pinned = self.entries.iter().filter(|e| e.pinned).count();
        if pinned > 0 {
            out.push(FolderView {
                name: "Pinned".into(),
                count: pinned,
                kind: "pinned",
                filter: PINNED_FILTER.into(),
            });
        }
        for f in &cfg.snippet_folders {
            out.push(FolderView {
                name: f.name.clone(),
                count: f.items.len(),
                kind: "snippets",
                filter: f.name.clone(),
            });
        }
        out
    }

    /// Everything the expanded card shows.
    pub fn detail(&mut self, id: ItemId, now: Instant) -> Option<ItemDetail> {
        let cfg = self.cfg();
        if self.is_secret(id) {
            let v = self.vault.entries(now).into_iter().find(|v| v.id == id)?;
            return Some(ItemDetail {
                id: id.0,
                html: escape_html(&v.masked),
                ..ItemDetail::default()
            });
        }
        let e = self.entry(id)?.clone();
        let mut d = ItemDetail {
            id: id.0,
            size_bytes: e.size_bytes,
            source_app: e.source_exe.as_deref().map(app_name),
            ..ItemDetail::default()
        };
        match e.kind {
            StoredKind::Image => {
                if let Ok(Some(Payload::Image(img))) = self.store.get_payload(id)
                    && img.png.len() <= MAX_INLINE_IMAGE_BYTES
                {
                    d.image = Some(format!(
                        "data:image/png;base64,{}",
                        STANDARD.encode(&img.png)
                    ));
                }
                return Some(d);
            }
            StoredKind::Files => {
                d.files = self
                    .full_text(&e)
                    .ok()?
                    .lines()
                    .map(str::to_owned)
                    .collect();
                return Some(d);
            }
            StoredKind::Text => {}
        }
        let text = self.full_text(&e).ok()?;
        d.char_count = text.chars().count();
        d.line_count = text.lines().count().max(1);
        if e.category == Category::Code && !e.read_only {
            let r = render(&text);
            d.language = r.language.map(|l| l.label().to_owned());
            d.reformatted = r.reformatted;
            d.truncated = r.truncated;
            d.html = r.html;
        } else {
            let (shown, truncated) = if text.len() > 64 * 1024 {
                let mut end = 64 * 1024;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                (&text[..end], true)
            } else {
                (text.as_str(), false)
            };
            d.html = escape_html(shown);
            d.truncated = truncated;
        }
        if !e.read_only && text.len() <= MAX_EDITABLE_BYTES {
            d.editable_text = Some(text);
        } else if e.read_only {
            d.editable_text = Some(self.full_text(&e).ok()?);
        }
        d.actions = cfg.actions.iter().map(|a| a.name.clone()).collect();
        Some(d)
    }

    /// A small PNG data URL for an image card, generated once and cached.
    pub fn thumbnail(&mut self, id: ItemId) -> Option<String> {
        if let Some(t) = self.thumbs.get(&id) {
            return Some(t.clone());
        }
        let Ok(Some(Payload::Image(img))) = self.store.get_payload(id) else {
            return None;
        };
        let small = thumbnail(&decode_png(&img.png).ok()?, THUMB_SIDE);
        let url = format!(
            "data:image/png;base64,{}",
            STANDARD.encode(encode_png(&small).ok()?)
        );
        self.thumbs.insert(id, url.clone());
        Some(url)
    }

    /// The text of a secret, for an explicit "reveal" or paste.
    #[must_use]
    pub fn reveal(&self, id: ItemId, now: Instant) -> Option<Zeroizing<String>> {
        self.vault.reveal(id, now)
    }

    #[must_use]
    pub fn status(&self, now: Instant) -> StatusView {
        StatusView {
            private_mode: self.private_mode,
            queue: self.queue_view(now),
        }
    }

    pub fn ui_config(&mut self, os: OsInfo) -> UiConfig {
        UiConfig::new(&self.cfg(), os)
    }

    // ------------------------------------------------------------- paste

    /// Resolve an item to what should land on the clipboard.
    ///
    /// Order for text: placeholders (pinned items and config snippets only),
    /// then the chosen transforms. The `{cursor}` offset is dropped when a
    /// transform runs, because it can change the text's length.
    pub fn prepare_paste(
        &mut self,
        id: ItemId,
        opts: &PasteOptions,
        ctx: &PasteContext,
    ) -> Result<PreparedPaste, PasteError> {
        self.cfg();
        if self.is_secret(id) {
            let secret = self.vault.reveal(id, ctx.now).ok_or(PasteError::Expired)?;
            let text = if opts.transforms.is_empty() {
                secret
            } else {
                Zeroizing::new(apply_transforms(&opts.transforms, &secret)?)
            };
            return Ok(PreparedPaste {
                id,
                content: PasteContent::Text(text),
                cursor_back: 0,
                sensitive: true,
            });
        }
        let entry = self.entry(id).ok_or(PasteError::NotFound)?.clone();
        let (content, cursor_back) = match entry.kind {
            StoredKind::Text => {
                let text = self.full_text(&entry)?;
                let expand = (entry.read_only || entry.pinned || entry.folder.is_some())
                    && has_placeholders(&text);
                let (text, cursor) = if expand {
                    let ex = expand_placeholders(
                        &text,
                        &PlaceholderContext {
                            now: ctx.now_local,
                            clipboard: ctx.clipboard.as_deref(),
                        },
                    )?;
                    (ex.text, ex.cursor_back)
                } else {
                    (text, 0)
                };
                let text = apply_transforms(&opts.transforms, &text)?;
                let cursor = if opts.transforms.is_empty() {
                    cursor
                } else {
                    0
                };
                (PasteContent::Text(Zeroizing::new(text)), cursor)
            }
            StoredKind::Files => {
                let list: Vec<String> =
                    self.full_text(&entry)?.lines().map(str::to_owned).collect();
                if opts.plain || !opts.transforms.is_empty() {
                    let text = apply_transforms(&opts.transforms, &list.join("\r\n"))?;
                    (PasteContent::Text(Zeroizing::new(text)), 0)
                } else {
                    (PasteContent::Files(list), 0)
                }
            }
            StoredKind::Image => match self.store.get_payload(id).map_err(|e| io_err(&e))? {
                Some(Payload::Image(img)) => (PasteContent::Image(img), 0),
                _ => return Err(PasteError::NotFound),
            },
        };
        Ok(PreparedPaste {
            id,
            content,
            cursor_back,
            sensitive: false,
        })
    }

    /// Paste text the user edited in the quick editor.
    pub fn prepare_text_paste(
        &mut self,
        text: String,
        opts: &PasteOptions,
    ) -> Result<PreparedPaste, PasteError> {
        let text = apply_transforms(&opts.transforms, &text)?;
        Ok(PreparedPaste {
            id: ItemId(0),
            content: PasteContent::Text(Zeroizing::new(text)),
            cursor_back: 0,
            sensitive: false,
        })
    }

    /// Record a paste (feeds future ranking). Only stored items are tracked.
    pub fn mark_used(&mut self, id: ItemId, now_ms: i64) {
        if !self.is_secret(id) && id.0 < VIRTUAL_BASE && id.0 != 0 {
            let _ = self.store.touch(id, now_ms);
        }
    }

    // ------------------------------------------------------------ edits

    /// Toggle the pin. Config snippets and secrets cannot be pinned.
    pub fn toggle_pin(&mut self, id: ItemId) -> Option<bool> {
        let e = self.entries.iter_mut().find(|e| e.id == id)?;
        let pinned = !e.pinned;
        self.store.set_pinned(id, pinned).ok()?;
        e.pinned = pinned;
        Some(pinned)
    }

    pub fn delete(&mut self, id: ItemId) -> bool {
        self.queue.remove(id);
        if self.is_secret(id) {
            self.sensitive_created.remove(&id);
            return self.vault.remove(id);
        }
        if self.entries.iter().all(|e| e.id != id) || self.store.delete(id).is_err() {
            return false;
        }
        self.entries.retain(|e| e.id != id);
        self.thumbs.remove(&id);
        true
    }

    /// Save text from the quick editor as a new history item.
    pub fn save_text(&mut self, text: String, now_ms: i64, now: Instant) -> CaptureOutcome {
        self.ingest(CaptureInput {
            payload: Payload::Text(text),
            owner_exe: Some("Zlip".into()),
            clip_seq: 0,
            now_ms,
            now,
            enqueue: false,
        })
    }

    /// Delete everything except pinned items. Returns how many were removed.
    pub fn clear_history(&mut self) -> usize {
        let Ok(ids) = self.store.clear_unpinned() else {
            return 0;
        };
        for id in &ids {
            self.queue.remove(*id);
            self.thumbs.remove(id);
        }
        self.entries.retain(|e| e.pinned);
        ids.len()
    }

    // ------------------------------------------------------------- queue

    pub fn queue_toggle(&mut self) -> QueueMode {
        self.queue.toggle_collect()
    }

    /// Add an item from the overlay; starts collecting if the queue was idle.
    pub fn queue_add(&mut self, id: ItemId, now: Instant) -> QueueView {
        if self.queue.mode() == QueueMode::Idle {
            self.queue.toggle_collect();
        }
        if self.entry(id).is_some() || self.is_secret(id) {
            self.queue.push(id);
        }
        self.queue_view(now)
    }

    /// The next item for a queue paste, skipping items that vanished or expired.
    pub fn queue_pop(&mut self, ctx: &PasteContext) -> Option<Result<PreparedPaste, PasteError>> {
        loop {
            let id = self.queue.pop()?;
            match self.prepare_paste(id, &PasteOptions::default(), ctx) {
                Err(PasteError::Expired | PasteError::NotFound) => {}
                other => return Some(other),
            }
        }
    }

    pub fn queue_clear(&mut self) {
        self.queue.clear();
    }

    #[must_use]
    pub fn queue_mode(&self) -> QueueMode {
        self.queue.mode()
    }

    #[must_use]
    pub fn queue_view(&self, now: Instant) -> QueueView {
        let mode = match self.queue.mode() {
            QueueMode::Idle => "idle",
            QueueMode::Collecting => "collecting",
            QueueMode::Draining => "draining",
        };
        let next_preview = self.queue.peek().and_then(|id| {
            if self.is_secret(id) {
                self.vault
                    .entries(now)
                    .into_iter()
                    .find(|v| v.id == id)
                    .map(|v| v.masked)
            } else {
                self.entry(id)
                    .map(|e| e.preview.lines().next().unwrap_or_default().to_owned())
            }
        });
        QueueView {
            mode,
            len: self.queue.len(),
            next_preview,
        }
    }

    // ----------------------------------------------------- time-driven work

    /// Expire due secrets. The system clipboard is cleared only if it still
    /// holds what was captured (see `SensitiveVault::sweep`).
    pub fn sweep_vault(&mut self, now: Instant, guard: &impl ClipboardGuard) -> Vec<ItemId> {
        let gone = self.vault.sweep(now, guard);
        for id in &gone {
            self.sensitive_created.remove(id);
            self.queue.remove(*id);
        }
        gone
    }

    /// Wipe all secrets immediately.
    pub fn wipe_vault(&mut self, guard: &impl ClipboardGuard) -> usize {
        let gone = self.vault.wipe_all(guard);
        for id in &gone {
            self.sensitive_created.remove(id);
            self.queue.remove(*id);
        }
        gone.len()
    }

    #[must_use]
    pub fn vault_deadline(&self) -> Option<Instant> {
        self.vault.next_deadline()
    }

    /// Apply the retention limits. Returns how many items were removed.
    pub fn enforce_retention(&mut self, now_ms: i64) -> usize {
        let cfg = self.cfg();
        let policy = RetentionPolicy {
            max_items: cfg.retention.max_items as usize,
            max_age_secs: cfg.retention.max_age_secs(),
            max_total_bytes: MAX_TOTAL_HISTORY_BYTES,
        };
        let Ok(ids) = self.store.enforce_retention(&policy, now_ms) else {
            return 0;
        };
        if !ids.is_empty() {
            let doomed: std::collections::HashSet<ItemId> = ids.iter().copied().collect();
            self.entries.retain(|e| !doomed.contains(&e.id));
            for id in &ids {
                self.queue.remove(*id);
                self.thumbs.remove(id);
            }
        }
        ids.len()
    }

    // -------------------------------------------------------- private mode

    pub fn set_private_mode(&mut self, on: bool) {
        self.private_mode = on;
    }

    #[must_use]
    pub fn private_mode(&self) -> bool {
        self.private_mode
    }

    // ------------------------------------------------------------ actions

    /// Gather everything a script run needs while holding the engine, so the
    /// (possibly long) run itself can happen *outside* the lock.
    /// Secrets and images never qualify.
    pub fn begin_action(&mut self, id: ItemId, name: &str) -> Result<ActionJob, ActionReport> {
        let fail = |message: String| Err(ActionReport::Failed { message });
        let cfg = self.cfg();
        let Some(action) = cfg.actions.iter().find(|a| a.name == name) else {
            return fail(format!("There is no action called “{name}”."));
        };
        if self.is_secret(id) {
            return fail("Secrets are never sent to scripts.".into());
        }
        let text = match self.entry(id).cloned() {
            Some(e) if e.kind != StoredKind::Image => match self.full_text(&e) {
                Ok(t) => t,
                Err(e) => return fail(e.to_string()),
            },
            Some(_) => return fail("Images cannot be sent to scripts.".into()),
            None => return fail(PasteError::NotFound.to_string()),
        };
        Ok(ActionJob {
            action: action.clone(),
            text,
            trust: self.trust.clone(),
        })
    }

    /// Prepare and run in one step (convenient in tests; the app uses
    /// [`Engine::begin_action`] so it can release the lock first).
    pub fn run_action(&mut self, id: ItemId, name: &str) -> ActionReport {
        match self.begin_action(id, name) {
            Ok(job) => job.run(),
            Err(report) => report,
        }
    }

    /// Approve the script behind `name`, but only if it is still the exact file
    /// whose hash the user was shown.
    pub fn trust_action(&mut self, name: &str, shown_sha256: &str) -> Result<(), String> {
        let cfg = self.cfg();
        let action = cfg
            .actions
            .iter()
            .find(|a| a.name == name)
            .ok_or("There is no such action.")?;
        let now = zlip_actions::sha256_file(std::path::Path::new(&action.path))
            .map_err(|e| e.to_string())?;
        if !now.eq_ignore_ascii_case(shown_sha256) {
            return Err(
                "The script changed after it was shown to you. Run the action again to review it."
                    .into(),
            );
        }
        self.trust
            .trust(name, &action.path, &now)
            .map_err(|e| e.to_string())
    }
}

/// A prepared script run. Owns copies of everything it needs, so it can run
/// without any lock held.
pub struct ActionJob {
    action: Action,
    text: String,
    trust: TrustStore,
}

impl std::fmt::Debug for ActionJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ActionJob({})", self.action.name)
    }
}

impl ActionJob {
    /// Run the script (blocks until it finishes or times out).
    #[must_use]
    pub fn run(self) -> ActionReport {
        let name = self.action.name.clone();
        match run_script(
            &self.action,
            ActionInput::from_non_sensitive(&self.text),
            &self.trust,
        ) {
            Ok(RunOutcome::Finished(r)) => ActionReport::Done {
                exit_code: r.exit_code,
                stdout: r.stdout,
                stderr: r.stderr,
                timed_out: r.timed_out,
                truncated: r.truncated,
            },
            Ok(RunOutcome::NeedsTrust { sha256 }) => ActionReport::NeedsTrust {
                action: name,
                path: self.action.path.clone(),
                sha256,
            },
            Err(ActionError::BatchFile(p)) => ActionReport::Failed {
                message: format!(
                    "Batch files are not supported ({p}). Use a .ps1 script or an .exe."
                ),
            },
            Err(e) => ActionReport::Failed {
                message: e.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests;
