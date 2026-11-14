use gtk::{gdk, gio, glib, prelude::*};
use rustlab_editor::Editor;
use rustlab_lsp::Task;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

pub fn attach(editor: &Rc<Editor>, worker: Sender<Task>, uri: String, overlay: &gtk::Overlay) {
    let popup = gtk::Popover::builder()
        .autohide(false)
        .has_arrow(false)
        .build();
    let list = gtk::ListBox::new();
    let scroll = gtk::ScrolledWindow::builder()
        .min_content_width(300)
        .max_content_height(230)
        .propagate_natural_height(true)
        .child(&list)
        .build();
    popup.set_child(Some(&scroll));
    overlay.add_overlay(&popup);
    let state = Rc::new(State {
        editor: Rc::downgrade(editor),
        overlay: overlay.downgrade(),
        popup: popup.downgrade(),
        list: list.downgrade(),
        worker,
        uri,
        revision: Cell::new(0),
        waiting: Cell::new(false),
        warm: Cell::new(0),
        timer: RefCell::new(None),
        items: RefCell::new(Vec::new()),
    });
    let changes = state.clone();
    editor
        .buffer
        .connect_changed(move |_| changes.schedule(false));
    let focus = gtk::EventControllerFocus::new();
    let leaving = state.clone();
    focus.connect_leave(move |_| leaving.invalidate());
    editor.view.add_controller(focus);
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let keyboard = state.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        use glib::Propagation::{Proceed, Stop};
        if key == gdk::Key::space && modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
            keyboard.schedule(true);
            return Stop;
        }
        let Some(list) = keyboard.list.upgrade() else {
            return Proceed;
        };
        if !keyboard
            .popup
            .upgrade()
            .is_some_and(|popup| popup.is_visible())
        {
            return Proceed;
        }
        let index = list.selected_row().map_or(0, |row| row.index());
        match key {
            gdk::Key::Escape => keyboard.invalidate(),
            gdk::Key::Return | gdk::Key::Tab => keyboard.accept(index),
            gdk::Key::Up | gdk::Key::Down => {
                let next = if key == gdk::Key::Up {
                    index - 1
                } else {
                    index + 1
                };
                if let Some(row) = list.row_at_index(next) {
                    list.select_row(Some(&row));
                    let _ = row
                        .activate_action("list.scroll-to-item", Some(&(next as u32).to_variant()));
                }
            }
            _ => {
                keyboard.invalidate();
                return Proceed;
            }
        }
        Stop
    });
    editor.view.add_controller(keys);
    list.connect_row_activated(move |_, row| state.accept(row.index()));
}

struct State {
    editor: std::rc::Weak<Editor>,
    overlay: glib::WeakRef<gtk::Overlay>,
    popup: glib::WeakRef<gtk::Popover>,
    list: glib::WeakRef<gtk::ListBox>,
    worker: Sender<Task>,
    uri: String,
    revision: Cell<u64>,
    waiting: Cell<bool>,
    warm: Cell<u8>,
    timer: RefCell<Option<glib::SourceId>>,
    items: RefCell<Vec<(i32, i32, Value)>>,
}

impl State {
    fn invalidate(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
        if let Some(popup) = self.popup.upgrade() {
            popup.popdown();
        }
    }

    fn schedule(self: &Rc<Self>, manual: bool) {
        self.invalidate();
        let state = self.clone();
        let timer = glib::timeout_add_local_once(
            Duration::from_millis(if manual { 0 } else { 180 }),
            move || {
                state.timer.take();
                state.request(manual);
            },
        );
        self.timer.replace(Some(timer));
    }

    fn request(self: &Rc<Self>, manual: bool) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        if !editor.view.has_focus() || editor.buffer.has_selection() {
            return;
        }
        let cursor = editor
            .buffer
            .iter_at_offset(editor.buffer.cursor_position());
        let mut previous = cursor;
        if !manual
            && (!previous.backward_char()
                || !matches!(previous.char(), '.' | ':' | '_' | 'a'..='z' | 'A'..='Z' | '0'..='9'))
        {
            return;
        }
        if self.waiting.replace(true) {
            self.schedule(manual);
            return;
        }
        let mut line_start = cursor;
        line_start.set_line_offset(0);
        let line_prefix = editor.buffer.text(&line_start, &cursor, true);
        let text = editor
            .buffer
            .text(&editor.buffer.start_iter(), &editor.buffer.end_iter(), true);
        let (reply, receiver) = mpsc::channel();
        if self
            .worker
            .send(Task::Complete {
                uri: self.uri.clone(),
                text: text.to_string(),
                line: cursor.line() as u32,
                character: line_prefix.encode_utf16().count() as u32,
                reply,
            })
            .is_err()
        {
            self.waiting.set(false);
            return;
        }
        let revision = self.revision.get();
        let offset = cursor.offset();
        let state = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || receiver.recv()).await;
            state.waiting.set(false);
            if let Some(editor) = state.editor.upgrade()
                && state.revision.get() == revision
                && editor.buffer.cursor_position() == offset
                && editor.view.has_focus()
            {
                match result {
                    Ok(Ok(Ok(items))) if items.is_empty() && state.warm.get() < 12 => {
                        state.warm.set(state.warm.get() + 1);
                        state.schedule(false);
                    }
                    Ok(Ok(Ok(items))) => {
                        state.warm.set(12);
                        state.show(&editor, offset, items);
                    }
                    _ if manual => {
                        gtk::AlertDialog::builder()
                            .message("No se pudo obtener el autocompletado de rust-analyzer")
                            .detail("Verificá que rust-analyzer esté instalado y que Cargo pueda cargar el proyecto.")
                            .build()
                            .show(editor.view.root().and_downcast::<gtk::Window>().as_ref());
                    }
                    _ => {}
                }
            }
        });
    }

    fn show(&self, editor: &Editor, offset: i32, items: Vec<Value>) {
        let (Some(list), Some(popup)) = (self.list.upgrade(), self.popup.upgrade()) else {
            return;
        };
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        let mut edits = self.items.borrow_mut();
        edits.clear();
        let cursor = editor.buffer.iter_at_offset(offset);
        let mut items = items;
        items.sort_by(|left, right| {
            let sort_key = |item: &Value| {
                item["sortText"]
                    .as_str()
                    .or_else(|| item["label"].as_str())
                    .unwrap_or_default()
                    .to_owned()
            };
            sort_key(left).cmp(&sort_key(right))
        });
        for item in items.into_iter().take(80) {
            let Some(label) = item["label"].as_str() else {
                continue;
            };
            let edit = &item["textEdit"];
            let range = edit.get("range").or_else(|| edit.get("replace"));
            let (start, end) = if let Some(range) = range {
                let Some(start) = position(editor.buffer.upcast_ref(), &range["start"]) else {
                    continue;
                };
                let Some(end) = position(editor.buffer.upcast_ref(), &range["end"]) else {
                    continue;
                };
                if start > end {
                    continue;
                }
                (start, end)
            } else {
                let mut start = cursor;
                while start.backward_char() {
                    if !(start.char().is_alphanumeric() || start.char() == '_') {
                        start.forward_char();
                        break;
                    }
                }
                (start.offset(), offset)
            };
            let row = gtk::Label::builder()
                .label(label)
                .xalign(0.0)
                .margin_start(8)
                .margin_end(8)
                .margin_top(4)
                .margin_bottom(4)
                .build();
            row.set_tooltip_text(item["detail"].as_str());
            list.append(&row);
            edits.push((start, end, item));
        }
        if edits.is_empty() {
            return;
        }
        list.select_row(list.row_at_index(0).as_ref());
        let rect = editor.view.iter_location(&cursor);
        let (x, y) =
            editor
                .view
                .buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x(), rect.y());
        let Some(overlay) = self.overlay.upgrade() else {
            return;
        };
        let Some(point) = editor
            .view
            .compute_point(&overlay, &gtk::graphene::Point::new(x as f32, y as f32))
        else {
            return;
        };
        let (x, y) = (point.x() as i32, point.y() as i32);
        popup.set_pointing_to(Some(&gdk::Rectangle::new(
            x,
            y,
            rect.width().max(1),
            rect.height(),
        )));
        popup.popup();
    }

    fn accept(self: &Rc<Self>, index: i32) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        let Some((start, end, item)) = self.items.borrow().get(index as usize).cloned() else {
            return;
        };
        self.invalidate();
        if item.get("data").is_none() || item.get("additionalTextEdits").is_some() {
            self.apply(&editor, start, end, &item);
            return;
        }
        let (reply, receive) = mpsc::channel();
        if self
            .worker
            .send(Task::Resolve(item.clone(), reply))
            .is_err()
        {
            return;
        }
        let revision = self.revision.get();
        let cursor = editor.buffer.cursor_position();
        let state = self.clone();
        glib::spawn_future_local(async move {
            let result =
                gio::spawn_blocking(move || receive.recv_timeout(Duration::from_secs(3))).await;
            if let Some(editor) = state.editor.upgrade()
                && state.revision.get() == revision
                && editor.buffer.cursor_position() == cursor
            {
                let resolved = match result {
                    Ok(Ok(Ok(resolved))) => resolved,
                    _ => return,
                };
                state.apply(&editor, start, end, &resolved);
            }
        });
    }

    fn apply(&self, editor: &Editor, mut start: i32, mut end: i32, item: &Value) {
        let buffer = editor.buffer.upcast_ref::<gtk::TextBuffer>();
        let text = item["textEdit"]["newText"]
            .as_str()
            .or_else(|| item["insertText"].as_str())
            .or_else(|| item["label"].as_str())
            .unwrap_or_default();
        let mut extra = Vec::new();
        if let Some(edits) = item["additionalTextEdits"].as_array() {
            for edit in edits {
                let Some(from) = position(buffer, &edit["range"]["start"]) else {
                    return;
                };
                let Some(to) = position(buffer, &edit["range"]["end"]) else {
                    return;
                };
                let Some(text) = edit["newText"].as_str() else {
                    return;
                };
                if from > to || (from < end && to > start) || from == start {
                    return;
                }
                extra.push((from, to, text));
            }
        }
        extra.sort_by_key(|&(from, _, _)| std::cmp::Reverse(from));
        if extra
            .windows(2)
            .any(|pair| pair[1].1 > pair[0].0 || pair[1].0 == pair[0].0)
        {
            return;
        }
        buffer.begin_user_action();
        for (from, to, text) in extra {
            replace(buffer, from, to, text);
            if to <= start {
                let delta = text.chars().count() as i32 - (to - from);
                start += delta;
                end += delta;
            }
        }
        if item["insertTextFormat"] == 2 {
            replace(buffer, start, end, "");
            let mut iter = buffer.iter_at_offset(start);
            if !editor.insert_snippet(text, &mut iter) {
                replace(buffer, start, start, text);
            }
        } else {
            replace(buffer, start, end, text);
            buffer.place_cursor(&buffer.iter_at_offset(start + text.chars().count() as i32));
        }
        buffer.end_user_action();
        self.invalidate();
    }
}

fn replace(buffer: &gtk::TextBuffer, start: i32, end: i32, text: &str) {
    let mut first = buffer.iter_at_offset(start);
    let mut last = buffer.iter_at_offset(end);
    buffer.delete(&mut first, &mut last);
    buffer.insert(&mut first, text);
}

fn position(buffer: &gtk::TextBuffer, value: &Value) -> Option<i32> {
    let line = i32::try_from(value["line"].as_u64()?).ok()?;
    let target = value["character"].as_u64()?;
    let mut iter = buffer.iter_at_line(line)?;
    let mut units = 0;
    while units < target && !iter.ends_line() && !iter.is_end() {
        units += iter.char().len_utf16() as u64;
        iter.forward_char();
    }
    (units == target).then(|| iter.offset())
}
