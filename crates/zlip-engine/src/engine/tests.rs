use super::*;
use std::cell::Cell;
use zlip_config::{Action, ActionKind, Automation, Snippet, SnippetFolder, When};
use zlip_store::MasterKey;

struct Fx {
    dir: tempfile::TempDir,
    cfg: ConfigHandle,
    e: Engine,
    t0: Instant,
}

fn fx_with(cfg: Config) -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(
        &dir.path().join("h.db"),
        &dir.path().join("img"),
        &MasterKey::from_bytes([3; 32]),
    )
    .unwrap();
    let handle = ConfigHandle::new(cfg);
    let (e, skipped) = Engine::new(handle.clone(), store, dir.path().join("trust.json")).unwrap();
    assert_eq!(skipped, 0);
    Fx {
        dir,
        cfg: handle,
        e,
        t0: Instant::now(),
    }
}

fn fx() -> Fx {
    fx_with(Config::default())
}

struct Guard {
    seq: Cell<u32>,
    cleared: Cell<u32>,
}
impl Guard {
    fn at(seq: u32) -> Self {
        Self {
            seq: Cell::new(seq),
            cleared: Cell::new(0),
        }
    }
}
impl ClipboardGuard for Guard {
    fn clipboard_sequence(&self) -> u32 {
        self.seq.get()
    }
    fn clear_clipboard(&self) {
        self.cleared.set(self.cleared.get() + 1);
        self.seq.set(self.seq.get() + 1);
    }
}

fn cap(text: &str, ms: i64, t0: Instant) -> CaptureInput {
    CaptureInput {
        payload: Payload::Text(text.into()),
        owner_exe: Some(r"C:\apps\notepad.exe".into()),
        clip_seq: 5,
        now_ms: ms,
        now: t0,
        enqueue: true,
    }
}

fn stored_id(o: &CaptureOutcome) -> ItemId {
    match o {
        CaptureOutcome::Stored { id, .. } | CaptureOutcome::Sensitive { id, .. } => *id,
        other => panic!("expected an id, got {other:?}"),
    }
}

fn q(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        limit: 50,
        ..SearchQuery::default()
    }
}

fn ctx(t0: Instant, clip: Option<&str>) -> PasteContext {
    PasteContext {
        now_local: chrono::NaiveDate::from_ymd_opt(2026, 11, 12)
            .unwrap()
            .and_hms_opt(9, 5, 7)
            .unwrap(),
        clipboard: clip.map(str::to_owned),
        now: t0,
    }
}

fn text_of(p: &PreparedPaste) -> &str {
    match &p.content {
        PasteContent::Text(t) => t,
        other => panic!("expected text, got {other:?}"),
    }
}

fn with_snippet() -> Config {
    Config {
        snippet_folders: vec![SnippetFolder {
            name: "Terminal".into(),
            items: vec![
                Snippet {
                    title: "Docker prune".into(),
                    body: "docker system prune -af".into(),
                },
                Snippet {
                    title: "Greeting".into(),
                    body: "Hi {cursor}, today is {date}".into(),
                },
            ],
        }],
        ..Config::default()
    }
}

#[test]
fn captures_text_newest_first_and_collapses_duplicates() {
    let mut f = fx();
    let a = stored_id(&f.e.ingest(cap("alpha", 1, f.t0)));
    let b = stored_id(&f.e.ingest(cap("beta", 2, f.t0)));
    let ids = |f: &mut Fx| {
        f.e.search(&q(""), f.t0)
            .iter()
            .map(|v| v.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&mut f), vec![b.0, a.0]);
    let again = f.e.ingest(cap("alpha", 3, f.t0));
    assert!(matches!(
        again,
        CaptureOutcome::Stored {
            duplicate: true,
            ..
        }
    ));
    assert_eq!(ids(&mut f), vec![a.0, b.0], "a re-copy moves to the top");
}

#[test]
fn categories_and_labels_reach_the_view() {
    let mut f = fx();
    f.e.ingest(cap("https://www.github.com/rhshourav/zlip", 1, f.t0));
    f.e.ingest(cap("#C5CAF5", 2, f.t0));
    f.e.ingest(cap(r#"{"a": 1}"#, 3, f.t0));
    let v = f.e.search(&q(""), f.t0);
    assert_eq!(
        (v[0].category, v[0].label.as_deref()),
        (Category::Code, Some("JSON"))
    );
    assert!(
        v[0].preview_html
            .as_deref()
            .is_some_and(|h| h.contains("<span"))
    );
    assert_eq!(
        (v[1].category, v[1].color.as_deref()),
        (Category::Color, Some("#C5CAF5"))
    );
    assert_eq!(
        (v[2].category, v[2].label.as_deref()),
        (Category::Link, Some("github.com"))
    );
    assert_eq!(v[2].source_app.as_deref(), Some("Notepad"));
}

#[test]
fn secrets_never_touch_the_database_or_the_search_index() {
    let mut f = fx();
    let secret = "AKIAIOSFODNN7EXAMPLE";
    let out = f.e.ingest(cap(secret, 1, f.t0));
    assert!(matches!(
        out,
        CaptureOutcome::Sensitive {
            label: "API key",
            ..
        }
    ));
    let id = stored_id(&out);

    let listed = f.e.search(&q(""), f.t0);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].category, Category::Sensitive);
    assert_eq!(listed[0].preview, "API key • 20 characters");
    assert!(listed[0].expires_in_ms.is_some_and(|ms| ms <= 60_000));
    assert!(
        f.e.search(&q("AKIA"), f.t0).is_empty(),
        "content must not be searchable"
    );
    assert_eq!(
        f.e.search(&q("api key"), f.t0).len(),
        1,
        "the masked label is"
    );

    for name in ["h.db", "h.db-wal"] {
        let raw = std::fs::read(f.dir.path().join(name)).unwrap_or_default();
        assert!(
            !raw.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "secret found in {name}"
        );
    }
    assert_eq!(
        f.e.reveal(id, f.t0).as_deref().map(String::as_str),
        Some(secret)
    );
    let d = f.e.detail(id, f.t0).unwrap();
    assert!(!d.html.contains("AKIA") && d.editable_text.is_none());
}

#[test]
fn secrets_expire_wipe_and_leave_every_list() {
    let mut f = fx();
    f.e.ingest(cap("normal text", 1, f.t0));
    f.e.queue_toggle();
    let id = stored_id(&f.e.ingest(cap("AKIAIOSFODNN7EXAMPLE", 2, f.t0)));
    assert_eq!(
        f.e.queue_view(f.t0).len,
        1,
        "a collecting queue accepts secrets by id"
    );
    let deadline = f.e.vault_deadline().unwrap();
    assert!(
        f.e.sweep_vault(deadline - Duration::from_secs(1), &Guard::at(5))
            .is_empty()
    );
    let guard = Guard::at(5);
    assert_eq!(f.e.sweep_vault(deadline, &guard), vec![id]);
    assert_eq!(
        guard.cleared.get(),
        1,
        "clipboard still held the secret, so it was cleared"
    );
    assert_eq!(f.e.queue_view(f.t0).len, 0);
    assert!(f.e.reveal(id, deadline).is_none());
    assert_eq!(
        f.e.search(&q(""), deadline).len(),
        1,
        "only the normal item remains"
    );
}

#[test]
fn privacy_skips_are_decided_before_content_is_read() {
    let mut f = fx();
    let facts = |exe: Option<&'static str>, title: Option<&'static str>| CaptureFacts {
        owner_exe: exe,
        foreground_title: title,
        ..CaptureFacts::default()
    };
    assert_eq!(
        f.e.should_capture(facts(
            Some(r"C:\Program Files\Bitwarden\Bitwarden.exe"),
            None
        )),
        Some(SkipReason::BlacklistedExe)
    );
    assert_eq!(
        f.e.should_capture(facts(None, Some("Vault - 1Password - Chrome"))),
        Some(SkipReason::BlacklistedTitle)
    );
    assert_eq!(
        f.e.should_capture(CaptureFacts {
            exclude_from_monitor: true,
            ..CaptureFacts::default()
        }),
        Some(SkipReason::ExcludedFormat)
    );
    assert_eq!(
        f.e.should_capture(facts(Some(r"C:\Windows\notepad.exe"), Some("Notes"))),
        None
    );
    f.e.set_private_mode(true);
    assert_eq!(
        f.e.should_capture(facts(None, None)),
        Some(SkipReason::PrivateMode)
    );
}

#[test]
fn automations_clean_matching_copies_before_storage() {
    let cfg = Config {
        automations: vec![Automation {
            when: When {
                category: Some(Category::Link),
                source_exe: None,
            },
            then: vec![Transform::StripTrackingParams],
        }],
        ..Config::default()
    };
    let mut f = fx_with(cfg);
    let id = stored_id(&f.e.ingest(cap("https://a.com/p?id=7&utm_source=x&fbclid=y", 1, f.t0)));
    assert_eq!(
        f.e.detail(id, f.t0).unwrap().editable_text.as_deref(),
        Some("https://a.com/p?id=7")
    );
}

#[test]
fn limits_ignore_oversized_or_blank_copies() {
    let mut f = fx();
    assert_eq!(
        f.e.ingest(cap("   \n ", 1, f.t0)),
        CaptureOutcome::Ignored("empty")
    );
    let big = "x ".repeat(600 * 1024);
    assert_eq!(
        f.e.ingest(cap(&big, 2, f.t0)),
        CaptureOutcome::Ignored("too large")
    );
}

#[test]
fn the_queue_collects_then_drains_in_order_skipping_what_expired() {
    let mut f = fx();
    f.e.queue_toggle();
    let a = stored_id(&f.e.ingest(cap("first", 1, f.t0)));
    let s = stored_id(&f.e.ingest(cap("AKIAIOSFODNN7EXAMPLE", 2, f.t0)));
    let b = stored_id(&f.e.ingest(cap("third", 3, f.t0)));
    assert_eq!(f.e.queue_view(f.t0).len, 3);
    assert_eq!(f.e.search(&q("third"), f.t0)[0].queue_position, Some(3));
    assert_eq!(f.e.queue_toggle(), QueueMode::Draining);
    assert_eq!(f.e.queue_view(f.t0).next_preview.as_deref(), Some("first"));

    let (a_paste, mid) = (
        f.e.queue_pop(&ctx(f.t0, None)).unwrap().unwrap(),
        f.e.queue_pop(&ctx(f.t0 + Duration::from_secs(61), None)),
    );
    assert_eq!((a_paste.id, text_of(&a_paste)), (a, "first"));
    let _ = (s, b);
    let last = mid.unwrap().unwrap();
    assert_eq!(text_of(&last), "third", "the expired secret was skipped");
    assert!(f.e.queue_pop(&ctx(f.t0, None)).is_none());
    assert_eq!(f.e.queue_mode(), QueueMode::Idle);
}

#[test]
fn overlay_adds_to_the_queue_and_starts_collecting_when_idle() {
    let mut f = fx();
    let a = stored_id(&f.e.ingest(CaptureInput {
        enqueue: false,
        ..cap("a", 1, f.t0)
    }));
    let v = f.e.queue_add(a, f.t0);
    assert_eq!((v.mode, v.len), ("collecting", 1));
}

#[test]
fn snippets_are_found_by_search_and_folder_but_not_in_the_plain_list() {
    let mut f = fx_with(with_snippet());
    assert!(f.e.search(&q(""), f.t0).is_empty());
    assert_eq!(f.e.search(&q("docker"), f.t0).len(), 1);
    let folder = SearchQuery {
        folder: Some("Terminal".into()),
        limit: 50,
        ..SearchQuery::default()
    };
    let v = f.e.search(&folder, f.t0);
    assert_eq!(
        v.iter()
            .map(|x| x.preview.lines().next().unwrap())
            .collect::<Vec<_>>(),
        vec!["Docker prune", "Greeting"]
    );
    assert!(v.iter().all(|x| x.read_only
        && x.category == Category::Snippet
        && x.label.as_deref() == Some("Terminal")));
    assert_eq!(
        f.e.folders()
            .iter()
            .map(|x| (x.name.as_str(), x.count))
            .collect::<Vec<_>>(),
        vec![("Terminal", 2)]
    );
}

#[test]
fn snippet_placeholders_expand_and_place_the_cursor() {
    let mut f = fx_with(with_snippet());
    let folder = SearchQuery {
        folder: Some("Terminal".into()),
        limit: 50,
        ..SearchQuery::default()
    };
    let greeting = f.e.search(&folder, f.t0)[1].id;
    let p =
        f.e.prepare_paste(ItemId(greeting), &PasteOptions::default(), &ctx(f.t0, None))
            .unwrap();
    assert_eq!(
        (text_of(&p), p.cursor_back),
        ("Hi , today is 2026-11-12", 21)
    );
    let docker = f.e.search(&folder, f.t0)[0].id;
    assert_eq!(
        text_of(
            &f.e.prepare_paste(ItemId(docker), &PasteOptions::default(), &ctx(f.t0, None))
                .unwrap()
        ),
        "docker system prune -af"
    );
}

#[test]
fn history_text_is_pasted_verbatim_until_it_is_pinned() {
    let mut f = fx();
    let id = stored_id(&f.e.ingest(cap("today is {date}, clip={clipboard}", 1, f.t0)));
    let c = ctx(f.t0, Some("CLIP"));
    assert_eq!(
        text_of(&f.e.prepare_paste(id, &PasteOptions::default(), &c).unwrap()),
        "today is {date}, clip={clipboard}"
    );
    assert_eq!(f.e.toggle_pin(id), Some(true));
    assert_eq!(
        text_of(&f.e.prepare_paste(id, &PasteOptions::default(), &c).unwrap()),
        "today is 2026-11-12, clip=CLIP"
    );
    assert_eq!(f.e.toggle_pin(id), Some(false));
}

#[test]
fn pinned_code_with_only_escaped_braces_is_not_rewritten() {
    let mut f = fx();
    let id = stored_id(&f.e.ingest(cap("{{ .Name }} and {x: 1}", 1, f.t0)));
    f.e.toggle_pin(id);
    assert_eq!(
        text_of(
            &f.e.prepare_paste(id, &PasteOptions::default(), &ctx(f.t0, None))
                .unwrap()
        ),
        "{{ .Name }} and {x: 1}"
    );
}

#[test]
fn transforms_apply_and_errors_are_user_readable() {
    let mut f = fx();
    let id = stored_id(&f.e.ingest(cap("  Hello Big World  ", 1, f.t0)));
    let opts = PasteOptions {
        transforms: vec![Transform::Trim, Transform::SnakeCase],
        plain: false,
    };
    assert_eq!(
        text_of(&f.e.prepare_paste(id, &opts, &ctx(f.t0, None)).unwrap()),
        "hello_big_world"
    );
    let bad = PasteOptions {
        transforms: vec![Transform::Base64Decode],
        plain: false,
    };
    let err = f.e.prepare_paste(id, &bad, &ctx(f.t0, None)).unwrap_err();
    assert_eq!(err.to_string(), "the text is not valid Base64");
    assert!(matches!(
        f.e.prepare_paste(ItemId(999), &PasteOptions::default(), &ctx(f.t0, None)),
        Err(PasteError::NotFound)
    ));
}

#[test]
fn a_transform_drops_the_cursor_offset() {
    let mut f = fx_with(with_snippet());
    let folder = SearchQuery {
        folder: Some("Terminal".into()),
        limit: 50,
        ..SearchQuery::default()
    };
    let greeting = f.e.search(&folder, f.t0)[1].id;
    let opts = PasteOptions {
        transforms: vec![Transform::Upper],
        plain: false,
    };
    let p =
        f.e.prepare_paste(ItemId(greeting), &opts, &ctx(f.t0, None))
            .unwrap();
    assert_eq!(
        (text_of(&p), p.cursor_back),
        ("HI , TODAY IS 2026-11-12", 0)
    );
}

#[test]
fn file_lists_paste_as_files_or_as_plain_paths() {
    let mut f = fx();
    let files = vec![r"C:\a\one.txt".to_owned(), r"C:\a\two.txt".to_owned()];
    let id = stored_id(&f.e.ingest(CaptureInput {
        payload: Payload::Files(files.clone()),
        ..cap("", 1, f.t0)
    }));
    match f
        .e
        .prepare_paste(id, &PasteOptions::default(), &ctx(f.t0, None))
        .unwrap()
        .content
    {
        PasteContent::Files(got) => assert_eq!(got, files),
        other => panic!("{other:?}"),
    }
    let plain = PasteOptions {
        plain: true,
        ..PasteOptions::default()
    };
    assert_eq!(
        text_of(&f.e.prepare_paste(id, &plain, &ctx(f.t0, None)).unwrap()),
        "C:\\a\\one.txt\r\nC:\\a\\two.txt"
    );
    assert_eq!(f.e.detail(id, f.t0).unwrap().files, files);
}

#[test]
fn images_are_stored_and_pasted_back_intact() {
    let mut f = fx();
    let png = zlip_core::image::encode_png(&zlip_core::image::Rgba8Image {
        width: 4,
        height: 2,
        pixels: vec![200; 32],
    })
    .unwrap();
    let img = ImageData {
        png: png.clone(),
        width: 4,
        height: 2,
    };
    let id = stored_id(&f.e.ingest(CaptureInput {
        payload: Payload::Image(img),
        ..cap("", 1, f.t0)
    }));
    let v = &f.e.search(&q(""), f.t0)[0];
    assert_eq!(
        (v.category, v.width, v.height, v.label.as_deref()),
        (Category::Image, Some(4), Some(2), Some("4 × 2"))
    );
    assert!(
        f.e.thumbnail(id)
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    assert!(f.e.detail(id, f.t0).unwrap().image.is_some());
    match f
        .e
        .prepare_paste(id, &PasteOptions::default(), &ctx(f.t0, None))
        .unwrap()
        .content
    {
        PasteContent::Image(i) => assert_eq!(i.png, png),
        other => panic!("{other:?}"),
    }
}

#[test]
fn search_scopes_by_category_and_pinned_folder() {
    let mut f = fx();
    let note = stored_id(&f.e.ingest(cap("grocery list milk eggs", 1, f.t0)));
    f.e.ingest(cap("https://example.com/milk", 2, f.t0));
    let by = |cat| SearchQuery {
        text: "milk".into(),
        category: Some(cat),
        limit: 50,
        ..SearchQuery::default()
    };
    assert_eq!(f.e.search(&by(Category::Link), f.t0).len(), 1);
    assert_eq!(f.e.search(&by(Category::Text), f.t0).len(), 1);
    assert_eq!(f.e.search(&q("milk"), f.t0).len(), 2);
    f.e.toggle_pin(note);
    let pinned = SearchQuery {
        folder: Some(PINNED_FILTER.into()),
        limit: 50,
        ..SearchQuery::default()
    };
    assert_eq!(
        f.e.search(&pinned, f.t0)
            .iter()
            .map(|v| v.id)
            .collect::<Vec<_>>(),
        vec![note.0]
    );
    assert_eq!(f.e.folders()[0].name, "Pinned");
}

#[test]
fn retention_and_clear_history_spare_pinned_items() {
    let mut f = fx();
    let keep = stored_id(&f.e.ingest(cap("keep me", 1, f.t0)));
    f.e.toggle_pin(keep);
    for n in 0..14 {
        f.e.ingest(cap(&format!("item {n}"), 100 + n, f.t0));
    }
    // The limit is lowered afterwards (settings changed), so the clean-up has work to do.
    let mut lowered = Config::default();
    lowered.retention.max_items = 10;
    f.cfg.set(lowered);
    assert_eq!(
        f.e.enforce_retention(10_000),
        4,
        "14 unpinned minus the 10 allowed; the pinned one is exempt"
    );
    assert_eq!(f.e.search(&q(""), f.t0).len(), 11);
    assert_eq!(f.e.clear_history(), 10);
    let left = f.e.search(&q(""), f.t0);
    assert_eq!(left.iter().map(|v| v.id).collect::<Vec<_>>(), vec![keep.0]);
}

#[test]
fn delete_removes_from_index_queue_and_vault() {
    let mut f = fx();
    f.e.queue_toggle();
    let a = stored_id(&f.e.ingest(cap("a", 1, f.t0)));
    let s = stored_id(&f.e.ingest(cap("AKIAIOSFODNN7EXAMPLE", 2, f.t0)));
    assert!(f.e.delete(a) && f.e.delete(s));
    assert!(!f.e.delete(a));
    assert_eq!(
        (f.e.search(&q(""), f.t0).len(), f.e.queue_view(f.t0).len),
        (0, 0)
    );
    assert!(f.e.reveal(s, f.t0).is_none());
}

#[test]
fn live_config_reload_rebuilds_snippets_and_the_blacklist() {
    let mut f = fx();
    assert!(f.e.search(&q("docker"), f.t0).is_empty());
    let mut next = with_snippet();
    next.security.blacklist_exes = vec!["myvault.exe".into()];
    f.cfg.set(next);
    assert_eq!(f.e.search(&q("docker"), f.t0).len(), 1);
    let blocked = CaptureFacts {
        owner_exe: Some(r"D:\x\MyVault.exe"),
        ..CaptureFacts::default()
    };
    assert_eq!(
        f.e.should_capture(blocked),
        Some(SkipReason::BlacklistedExe)
    );
    let bw = CaptureFacts {
        owner_exe: Some("bitwarden.exe"),
        ..CaptureFacts::default()
    };
    assert_eq!(
        f.e.should_capture(bw),
        None,
        "the user's new list replaces the default one"
    );
}

#[test]
fn secrets_and_images_are_refused_by_actions() {
    let cfg = Config {
        actions: vec![Action {
            name: "lint".into(),
            kind: ActionKind::Script,
            path: "/nowhere/x.sh".into(),
            args: vec![],
            timeout_ms: 1000,
        }],
        ..Config::default()
    };
    let mut f = fx_with(cfg);
    let s = stored_id(&f.e.ingest(cap("AKIAIOSFODNN7EXAMPLE", 1, f.t0)));
    assert_eq!(
        f.e.run_action(s, "lint"),
        ActionReport::Failed {
            message: "Secrets are never sent to scripts.".into()
        }
    );
    let t = stored_id(&f.e.ingest(cap("hello", 2, f.t0)));
    assert!(matches!(
        f.e.run_action(t, "nope"),
        ActionReport::Failed { .. }
    ));
    assert!(
        matches!(f.e.run_action(t, "lint"), ActionReport::Failed { .. }),
        "missing script file is reported, not run"
    );
    assert_eq!(
        f.e.detail(t, f.t0).unwrap().actions,
        vec!["lint".to_owned()]
    );
    assert!(f.e.detail(s, f.t0).unwrap().actions.is_empty());
}

#[test]
fn quick_edit_paste_applies_transforms_to_the_edited_text() {
    let mut f = fx();
    let opts = PasteOptions {
        transforms: vec![Transform::KebabCase],
        plain: false,
    };
    let p = f.e.prepare_text_paste("Hello World".into(), &opts).unwrap();
    assert_eq!(text_of(&p), "hello-world");
    let saved = f.e.save_text("edited note".into(), 5, f.t0);
    assert!(matches!(
        saved,
        CaptureOutcome::Stored { queued: false, .. }
    ));
}

#[test]
fn ui_config_tells_the_ui_whether_corners_are_rounded() {
    let mut f = fx();
    let win11 = f.e.ui_config(OsInfo {
        build: 22631,
        native_backdrop: true,
        transparency: true,
        dark: true,
    });
    let win10 = f.e.ui_config(OsInfo {
        build: 19045,
        ..OsInfo::default()
    });
    assert!(win11.os.rounded && win11.os.native_backdrop);
    assert!(!win10.os.rounded);
    assert_eq!(win11.hotkeys.open_overlay, "Win+V");
}

#[test]
fn debug_output_leaks_nothing() {
    let mut f = fx();
    f.e.ingest(cap("AKIAIOSFODNN7EXAMPLE", 1, f.t0));
    assert!(!format!("{:?}", f.e).contains("AKIA"));
    let p =
        f.e.prepare_text_paste("top secret".into(), &PasteOptions::default())
            .unwrap();
    assert!(!format!("{p:?}").contains("top secret"));
}

#[test]
fn pasting_from_the_list_moves_the_item_to_the_top_and_only_history_items_move() {
    let mut f = fx_with(with_snippet());
    let a = stored_id(&f.e.ingest(cap("alpha", 1, f.t0)));
    let b = stored_id(&f.e.ingest(cap("beta", 2, f.t0)));
    let c = stored_id(&f.e.ingest(cap("gamma", 3, f.t0)));
    let ids = |f: &mut Fx| {
        f.e.search(&q(""), f.t0)
            .iter()
            .map(|v| v.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&mut f), vec![c.0, b.0, a.0]);

    assert!(f.e.bump_to_top(a, 10));
    assert_eq!(ids(&mut f), vec![a.0, c.0, b.0]);
    assert_eq!(f.e.search(&q(""), f.t0)[0].created_at_ms, 10);
    assert!(f.e.bump_to_top(a, 11), "already on top is fine");
    assert_eq!(ids(&mut f), vec![a.0, c.0, b.0]);

    // Not history items: unknown ids, secrets and config snippets stay where they are.
    assert!(!f.e.bump_to_top(ItemId(999), 12));
    let secret = stored_id(&f.e.ingest(cap("AKIAIOSFODNN7EXAMPLE", 13, f.t0)));
    assert!(!f.e.bump_to_top(secret, 14));
    let folder = SearchQuery {
        folder: Some("Terminal".into()),
        limit: 50,
        ..SearchQuery::default()
    };
    let snippet = ItemId(f.e.search(&folder, f.t0)[0].id);
    assert!(!f.e.bump_to_top(snippet, 15));
}

#[test]
fn an_item_you_just_used_outlives_older_ones_when_the_history_is_full() {
    let mut cfg = Config::default();
    cfg.retention.max_items = 10;
    let mut f = fx_with(cfg);
    let first = stored_id(&f.e.ingest(cap("item 0", 100, f.t0)));
    for n in 1..10 {
        f.e.ingest(cap(&format!("item {n}"), 100 + n, f.t0));
    }
    assert!(f.e.bump_to_top(first, 500));
    for n in 10..13 {
        f.e.ingest(cap(&format!("item {n}"), 600 + n, f.t0));
    }
    let left: Vec<String> =
        f.e.search(&q(""), f.t0)
            .into_iter()
            .map(|v| v.preview)
            .collect();
    assert_eq!(left.len(), 10);
    assert!(left.contains(&"item 0".to_owned()), "used recently: kept");
    for gone in ["item 1", "item 2", "item 3"] {
        assert!(!left.contains(&gone.to_owned()), "{gone} was the oldest");
    }
}

#[test]
fn the_history_limit_holds_as_items_arrive_not_only_every_ten_minutes() {
    let mut cfg = Config::default();
    cfg.retention.max_items = 10;
    let mut f = fx_with(cfg);
    let keep = stored_id(&f.e.ingest(cap("keep me", 1, f.t0)));
    f.e.toggle_pin(keep);
    for n in 0..25 {
        f.e.ingest(cap(&format!("item {n}"), 100 + n, f.t0));
    }
    let listed = f.e.search(&q(""), f.t0);
    assert_eq!(listed.len(), 11, "the 10 newest plus the pinned one");
    assert_eq!(listed[0].preview, "item 24");
    assert_eq!(listed[9].preview, "item 15");
    assert_eq!(listed[10].id, keep.0, "pinned items are exempt");

    // Copying something already in the history adds nothing, so nothing is dropped.
    f.e.ingest(cap("item 15", 900, f.t0));
    assert_eq!(f.e.search(&q(""), f.t0).len(), 11);
}

#[test]
fn passwords_follow_the_generator_settings() {
    let mut f = fx();
    let default = f.e.generate_password().unwrap();
    assert_eq!(default.len(), 20, "the default length");

    let mut cfg = Config::default();
    cfg.password_generator.length = 32;
    cfg.password_generator.lowercase = false;
    cfg.password_generator.digits = false;
    cfg.password_generator.symbols = false;
    f.cfg.set(cfg);
    let caps = f.e.generate_password().unwrap();
    assert_eq!(caps.len(), 32);
    assert!(caps.bytes().all(|b| b.is_ascii_uppercase()), "{caps}");
}

#[test]
fn two_generated_passwords_are_different() {
    let mut f = fx();
    assert_ne!(f.e.generate_password(), f.e.generate_password());
}

#[test]
fn the_generator_can_be_turned_off() {
    let mut f = fx();
    let mut cfg = Config::default();
    cfg.password_generator.enabled = false;
    f.cfg.set(cfg);
    let err = f.e.generate_password().unwrap_err();
    assert!(err.contains("turned off"), "{err}");
}

#[test]
fn the_overlay_is_told_what_the_generator_does() {
    let mut f = fx();
    let mut cfg = Config::default();
    cfg.password_generator.length = 40;
    f.cfg.set(cfg);
    let ui = f.e.ui_config(OsInfo::default());
    assert!(ui.password_generator.enabled);
    assert_eq!(ui.password_generator.length, 40);
}
