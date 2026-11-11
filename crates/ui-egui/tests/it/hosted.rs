//! A hosted page (`files_locked`) keeps the one document its host gave it: a person can't open,
//! create or close tabs by any route (menus, shortcuts, palette, Home, the tab strip, drops), so a
//! save can only store that document. The host's own open and save still work, and with the lock
//! off New and Open work as before.

use crate::support;

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::commands::COMMANDS;
use pdfcraft_ui_egui::{PdfCraftApp, RecentFile};

/// An `n`-page document with a proper xref table.
fn fixture(n: usize) -> Vec<u8> {
    let mut objs: Vec<String> = vec!["<< /Type /Catalog /Pages 2 0 R >>".into()];
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 200 300] >>", kids.join(" ")));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for i in 0..n {
        objs.push(format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>", 5 + 2 * i));
        let body = format!("BT /F1 24 Tf 20 150 Td (Page {}) Tj ET", i + 1);
        objs.push(format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

/// A scratch folder of this test's own (the tests share one process and run in parallel).
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfcraft-hosted-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A shell with the hosted document open (unless `open` is false), locked when `locked`. A file
/// picker answers with `pick`, and the recent list names it, so every route a person could take
/// has a real file to open.
fn harness(locked: bool, open: bool, pick: &std::path::Path) -> Harness<'static, PdfCraftApp> {
    let pick = pick.to_string_lossy().into_owned();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        if open {
            app.open_bytes("hosted.pdf", None, fixture(3)).unwrap();
        }
        app.files_locked = locked;
        app.pick_override = Some(vec![pick.clone()]);
        app.recent.insert(0, RecentFile { name: "picked.pdf".into(), path: pick, pages: 2, size: 0 });
        app
    });
    h.run_steps(2);
    h
}

/// Frames enough for a picked file to be read, prepared and opened, had anything asked for one.
fn frames(h: &mut Harness<'static, PdfCraftApp>) {
    for _ in 0..20 {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// The hosted document is still the only tab, and in front.
fn still_one_tab(h: &Harness<'static, PdfCraftApp>, after: &str) {
    assert_eq!(h.state().views.len(), 1, "{after} changed the tabs");
    assert_eq!(h.state().active, Some(0), "{after} changed the active tab");
    let (_, id) = h.state().active_ids().unwrap();
    assert_eq!(h.state().session.get(id).unwrap().name, "hosted.pdf", "{after} replaced the hosted document");
}

fn picked_file(name: &str) -> std::path::PathBuf {
    let path = scratch(name).join("picked.pdf");
    std::fs::write(&path, fixture(2)).unwrap();
    path
}

fn shortcut(h: &mut Harness<'static, PdfCraftApp>, key: Key, modifiers: Modifiers) {
    support::input::one_frame(h, [Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }]);
    support::input::one_frame(h, [Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers }]);
}

#[test]
fn locked_dispatch_refuses_tab_changes() {
    let pick = picked_file("dispatch");
    let creates = COMMANDS.iter().map(|s| s.id).filter(|id| id.starts_with("create."));
    for id in ["file.open", "file.open_recent", "file.close", "file.close_all"].into_iter().chain(creates) {
        let mut h = harness(true, true, &pick);
        let ctx = h.ctx.clone();
        h.state_mut().dispatch_command(&ctx, id);
        frames(&mut h);
        still_one_tab(&h, id);
        // The palette and the tool panels run commands through `run_command`.
        h.state_mut().run_command(id);
        frames(&mut h);
        still_one_tab(&h, &format!("{id} from the palette"));
    }
    // Shortcuts reach the same commands without the menu: New, Open, Close and Close all.
    let mut h = harness(true, true, &pick);
    for (key, modifiers) in
        [(Key::N, Modifiers::COMMAND), (Key::O, Modifiers::COMMAND), (Key::W, Modifiers::COMMAND), (Key::W, Modifiers::COMMAND | Modifiers::SHIFT)]
    {
        shortcut(&mut h, key, modifiers);
        frames(&mut h);
        still_one_tab(&h, &format!("{modifiers:?}+{key:?}"));
    }
    let _ = std::fs::remove_dir_all(pick.parent().unwrap());
}

#[test]
fn locked_open_dialog_and_recent_do_nothing() {
    let pick = picked_file("dialog");
    let mut h = harness(true, true, &pick);
    h.state_mut().open_dialog();
    frames(&mut h);
    still_one_tab(&h, "open_dialog");
    h.state_mut().open_recent(&pick.to_string_lossy());
    frames(&mut h);
    still_one_tab(&h, "open_recent");
    // A tool chosen on Home, where no tab is in front, asks for no other file to run on.
    h.state_mut().active = None;
    h.state_mut().run_command("export.text");
    frames(&mut h);
    assert_eq!(h.state().views.len(), 1, "a tool chosen on Home opened another file");
    let _ = std::fs::remove_dir_all(pick.parent().unwrap());
}

#[test]
fn locked_tab_cannot_be_closed() {
    let pick = picked_file("close");
    let mut h = harness(true, true, &pick);
    h.state_mut().request_close_tab(0);
    frames(&mut h);
    still_one_tab(&h, "request_close_tab");
    assert!(h.state().close_request.is_none(), "nothing asks about closing it either");
    let _ = std::fs::remove_dir_all(pick.parent().unwrap());
}

#[test]
fn locked_drops_are_ignored() {
    let pick = picked_file("drop");
    let mut h = harness(true, true, &pick);
    h.input_mut().dropped_files.push(std::sync::Arc::new(Dropped(pick.clone(), fixture(2))));
    frames(&mut h);
    still_one_tab(&h, "a dropped file");
    let _ = std::fs::remove_dir_all(pick.parent().unwrap());
}

#[test]
fn locked_host_open_still_opens() {
    let pick = picked_file("host-open");
    let mut h = harness(true, false, &pick);
    h.state().startup_inbox.lock().unwrap().push(("hosted.pdf".into(), fixture(3)));
    frames(&mut h);
    still_one_tab(&h, "the host's open");
    assert!(h.state().startup_inbox.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(pick.parent().unwrap());
}

/// The host saves through `execute("file.save")` (`window.pdfcraft.execute`). In the browser the
/// bytes go to `host_save`; natively, as here, they go to the file the save is told to write.
#[test]
fn locked_execute_save_still_runs() {
    let pick = picked_file("save");
    let out = pick.parent().unwrap().join("saved.pdf");
    let mut h = harness(true, true, &pick);
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    assert!(h.state_mut().execute("file.save"), "the host's save runs while locked");
    support::wait_until(&mut h, "the saved file", |_| out.exists());
    assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF"));
    // (A native save names the tab after the file it wrote.)
    assert_eq!(h.state().views.len(), 1);
    assert_eq!(h.state().active, Some(0));
    let _ = std::fs::remove_dir_all(pick.parent().unwrap());
}

fn page_count(path: &std::path::Path) -> usize {
    pdfcraft_render::PageRenderer::new(std::sync::Arc::new(std::fs::read(path).unwrap()), Default::default()).page_count()
}

/// Another tab can still appear on a hosted page (an attachment, a revision, a comment summary:
/// views of the document). Save stores the host's document whichever tab is in front. Tested on
/// the native save path (`save_override`); in the browser the same choice of tab feeds `host_save`.
#[test]
fn locked_save_stores_the_host_tab_when_another_is_active() {
    let pick = picked_file("save-host-tab");
    let out = pick.parent().unwrap().join("saved.pdf");
    let mut h = harness(true, true, &pick);
    // A second tab, as a derived view opens one: five pages, where the host's document has three.
    h.state_mut().open_bytes("summary.pdf", None, fixture(5)).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().views.len(), 2);
    h.state_mut().active = Some(1);
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    assert!(h.state_mut().execute("file.save"));
    support::wait_until(&mut h, "the saved file", |_| out.exists());
    assert_eq!(page_count(&out), 3, "the host's document was saved, not the tab in front");
    // Ctrl+S takes the same path.
    std::fs::remove_file(&out).unwrap();
    shortcut(&mut h, Key::S, Modifiers::COMMAND);
    support::wait_until(&mut h, "the saved file", |_| out.exists());
    assert_eq!(page_count(&out), 3, "Ctrl+S saved the host's document");
    let _ = std::fs::remove_dir_all(pick.parent().unwrap());
}

/// With the host's tab gone, a hosted save stores nothing rather than another tab.
#[test]
fn locked_save_refuses_when_the_host_tab_is_gone() {
    let pick = picked_file("save-host-gone");
    let out = pick.parent().unwrap().join("saved.pdf");
    let mut h = harness(true, true, &pick);
    h.state_mut().open_bytes("summary.pdf", None, fixture(5)).unwrap();
    h.run_steps(2);
    h.state_mut().close_tab(0);
    h.run_steps(2);
    assert_eq!(h.state().views.len(), 1);
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.state_mut().execute("file.save");
    frames(&mut h);
    assert!(!out.exists(), "another tab was stored as the host's document");
    let _ = std::fs::remove_dir_all(pick.parent().unwrap());
}

#[test]
fn unlocked_open_dialog_still_works() {
    let pick = picked_file("unlocked");
    let mut h = harness(false, true, &pick);
    let ctx = h.ctx.clone();
    h.state_mut().dispatch_command(&ctx, "file.open");
    support::wait_until(&mut h, "the picked file", |app| app.views.len() == 2);
    h.state_mut().dispatch_command(&ctx, "create.blank");
    support::wait_until(&mut h, "a new blank PDF", |app| app.views.len() == 3);
    // The picked file has no changes, so it closes without asking.
    h.state_mut().request_close_tab(1);
    support::wait_until(&mut h, "the picked tab closed", |app| app.views.len() == 2);
    let _ = std::fs::remove_dir_all(pick.parent().unwrap());
}

/// The tab strip draws no Open button, and Home neither its Open file card nor the recent list.
#[test]
fn locked_chrome_offers_no_other_file() {
    let pick = picked_file("chrome");
    for locked in [false, true] {
        let mut h = harness(locked, true, &pick);
        h.state_mut().active = None;
        h.run_steps(3);
        let shown = |label: &str| h.query_by_label(label).is_some();
        assert_eq!(shown("Open"), !locked, "the tab strip's Open button (locked: {locked})");
        assert_eq!(shown("Open file"), !locked, "Home's Open file card (locked: {locked})");
        assert_eq!(shown("picked.pdf"), !locked, "Home's recent list (locked: {locked})");
    }
    let _ = std::fs::remove_dir_all(pick.parent().unwrap());
}

#[derive(Debug)]
struct Dropped(std::path::PathBuf, Vec<u8>);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.1.clone())
    }
}
