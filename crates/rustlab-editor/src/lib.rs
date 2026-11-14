use sourceview5::prelude::*;
use std::cell::RefCell;
use std::io;
use std::path::Path;

pub struct Editor {
    document: RefCell<FileDocument>,
    pub buffer: sourceview5::Buffer,
    pub view: sourceview5::View,
}

impl Editor {
    pub fn new(document: FileDocument, text: &str) -> Self {
        let buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
        let languages = sourceview5::LanguageManager::default();
        if let Some(language) = languages.guess_language(Some(document.path()), None) {
            buffer.set_language(Some(&language));
        }
        buffer.set_text(text);
        buffer.set_modified(false);
        buffer.set_enable_undo(true);
        let view = sourceview5::View::builder()
            .buffer(&buffer)
            .monospace(true)
            .show_line_numbers(true)
            .highlight_current_line(true)
            .auto_indent(true)
            .insert_spaces_instead_of_tabs(true)
            .tab_width(4)
            .build();
        attach_pairs(&view);
        Self {
            document: RefCell::new(document),
            buffer,
            view,
        }
    }

    pub fn set_colors(&self, scheme: &str) {
        let manager = sourceview5::StyleSchemeManager::default();
        self.buffer
            .set_style_scheme(manager.scheme(scheme).as_ref());
    }

    pub fn insert_snippet(&self, text: &str, iter: &mut gtk::TextIter) -> bool {
        let Ok(snippet) = sourceview5::Snippet::new_parsed(text) else {
            return false;
        };
        self.view.push_snippet(&snippet, Some(iter));
        true
    }

    pub fn colors(&self) -> Option<gtk::glib::GString> {
        self.buffer.style_scheme().map(|scheme| scheme.id())
    }

    pub fn has_rust_highlighting(&self) -> bool {
        self.view.shows_line_numbers()
            && self.buffer.is_highlight_syntax()
            && self
                .buffer
                .language()
                .is_some_and(|language| language.id() == "rust")
    }

    pub fn path(&self) -> std::path::PathBuf {
        self.document.borrow().path().to_owned()
    }

    pub fn matches(&self, path: &Path) -> bool {
        self.document.borrow().path() == path
    }

    pub fn save(&self) -> io::Result<()> {
        let (start, end) = self.buffer.bounds();
        self.document
            .borrow_mut()
            .save(&self.buffer.text(&start, &end, true))?;
        self.buffer.set_modified(false);
        Ok(())
    }
}

pub use rustlab_core::Document as FileDocument;

fn attach_pairs(view: &sourceview5::View) {
    use gtk::{gdk, glib::Propagation};
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = view.downgrade();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        if modifiers.intersects(
            gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::SUPER_MASK,
        ) {
            return Propagation::Proceed;
        }
        let Some(view) = weak.upgrade() else {
            return Propagation::Proceed;
        };
        if !view.is_editable() {
            return Propagation::Proceed;
        }
        let buffer = view.buffer();
        let handled = if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
            indent_block(&buffer, view.tab_width() as usize)
        } else {
            edit_pair(&buffer, key.to_unicode(), key == gdk::Key::BackSpace)
        };
        if handled {
            Propagation::Stop
        } else {
            Propagation::Proceed
        }
    });
    view.add_controller(controller);
}

fn indent_block(buffer: &gtk::TextBuffer, width: usize) -> bool {
    if buffer.has_selection() {
        return false;
    }
    let mut cursor = buffer.iter_at_offset(buffer.cursor_position());
    let mut start = cursor;
    start.set_line_offset(0);
    let prefix = buffer.text(&start, &cursor, true);
    let Some(open @ ('{' | '(' | '[')) = prefix.trim_end_matches([' ', '\t']).chars().next_back()
    else {
        return false;
    };
    if let Some(source) = buffer.downcast_ref::<sourceview5::Buffer>() {
        source.ensure_highlight(&start, &cursor);
        let mut opening = cursor;
        opening.backward_chars(
            prefix
                .chars()
                .rev()
                .take_while(|c| matches!(c, ' ' | '\t'))
                .count() as i32
                + 1,
        );
        if ["comment", "string"]
            .iter()
            .any(|class| source.iter_has_context_class(&opening, class))
        {
            return false;
        }
    }
    let indent: String = prefix
        .chars()
        .take_while(|c| matches!(c, ' ' | '\t'))
        .collect();
    let inner = format!("\n{indent}{}", " ".repeat(width));
    let split_pair = closing(open) == Some(cursor.char());
    buffer.begin_user_action();
    buffer.insert(&mut cursor, &inner);
    if split_pair {
        let offset = cursor.offset();
        buffer.insert(&mut cursor, &format!("\n{indent}"));
        cursor = buffer.iter_at_offset(offset);
    }
    buffer.place_cursor(&cursor);
    buffer.end_user_action();
    true
}

fn closing(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        _ => None,
    }
}

fn edit_pair(buffer: &gtk::TextBuffer, key: Option<char>, backspace: bool) -> bool {
    let mut cursor = buffer.iter_at_offset(buffer.cursor_position());
    let mut previous = cursor;
    let has_previous = previous.backward_char();
    let selection = buffer.selection_bounds();
    if backspace {
        if selection.is_some() || !has_previous || closing(previous.char()) != Some(cursor.char()) {
            return false;
        }
        cursor.forward_char();
        buffer.delete(&mut previous, &mut cursor);
        return true;
    }
    let Some(key) = key else { return false };
    if key == '"' {
        let mut escapes = 0;
        let mut before = cursor;
        while before.backward_char() && before.char() == '\\' {
            escapes += 1;
        }
        if escapes % 2 != 0 {
            return false;
        }
    }
    if selection.is_none() && matches!(key, ')' | ']' | '}' | '"') && cursor.char() == key {
        cursor.forward_char();
        buffer.place_cursor(&cursor);
        return true;
    }
    let Some(close) = closing(key) else {
        return false;
    };
    buffer.begin_user_action();
    if let Some((mut start, mut end)) = selection {
        let start_offset = start.offset();
        let end_offset = end.offset();
        buffer.insert(&mut end, &close.to_string());
        start = buffer.iter_at_offset(start_offset);
        buffer.insert(&mut start, &key.to_string());
        buffer.select_range(
            &buffer.iter_at_offset(start_offset + 1),
            &buffer.iter_at_offset(end_offset + 1),
        );
    } else {
        buffer.insert(&mut cursor, &format!("{key}{close}"));
        cursor.backward_char();
        buffer.place_cursor(&cursor);
    }
    buffer.end_user_action();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requiere una sesión gráfica"]
    fn inserts_skips_wraps_and_removes_pairs() {
        gtk::init().unwrap();
        let buffer = gtk::TextBuffer::new(None);
        buffer.set_text("    if listo {}");
        buffer.place_cursor(&buffer.iter_at_offset(14));
        assert!(indent_block(&buffer, 4));
        assert_eq!(
            buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
            "    if listo {\n        \n    }"
        );
        assert_eq!(
            buffer
                .iter_at_offset(buffer.cursor_position())
                .line_offset(),
            8
        );
        buffer.set_text("");
        assert!(edit_pair(&buffer, Some('('), false));
        assert_eq!(buffer.cursor_position(), 1);
        assert!(edit_pair(&buffer, Some(')'), false));
        assert_eq!(buffer.cursor_position(), 2);
        buffer.place_cursor(&buffer.iter_at_offset(1));
        assert!(edit_pair(&buffer, None, true));
        assert_eq!(buffer.char_count(), 0);
        let view =
            sourceview5::View::with_buffer(&sourceview5::Buffer::new(None::<&gtk::TextTagTable>));
        let snippet = sourceview5::Snippet::new_parsed("sumar(${1:a}, ${2:b})$0").unwrap();
        view.push_snippet(&snippet, None);
        let snippet_buffer = view.buffer();
        assert_eq!(
            snippet_buffer.text(
                &snippet_buffer.start_iter(),
                &snippet_buffer.end_iter(),
                true
            ),
            "sumar(a, b)"
        );
        let (first, last) = snippet_buffer.selection_bounds().unwrap();
        assert_eq!(snippet_buffer.text(&first, &last, true), "a");
        buffer.set_text("dato");
        buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
        assert!(edit_pair(&buffer, Some('{'), false));
        assert_eq!(
            buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
            "{dato}"
        );
    }
}
