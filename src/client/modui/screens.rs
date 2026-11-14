//! A mod's own screen: its title, its rows of widgets and the events the
//! player's clicks make.
//!
//! A mod opens a screen by name (`ui_open` in scripts), then adds widgets
//! one after the other: a text line, a button, a toggle, a stepper, an
//! input field, a scrolling list. The game draws them with the same widgets
//! the settings use (`screen_draw.rs`) and hands what the player does back
//! as [`UiEvent`]s, which reach the mod's `on_ui_click(id)`,
//! `on_ui_change(id, value)` and `on_ui_close(name)`. One screen is open
//! at a time. This file is the model, with every bound; it draws nothing.

use crate::client::modui::limits::*;
use alloc::string::String;
use alloc::vec::Vec;

/// One row of a screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    Text { text: String },
    Button { id: String, label: String },
    Toggle { id: String, label: String, value: bool },
    Stepper { id: String, label: String, value: i32, min: i32, max: i32 },
    Input { id: String, label: String, value: String },
    List { id: String, items: Vec<String>, selected: i32, scroll: i32 },
}

/// Most rows of a list shown at once.
pub const LIST_ROWS: usize = 5;
/// Longest text typed into an input field.
pub const INPUT_MAX: usize = 60;

impl Widget {
    pub fn id(&self) -> Option<&str> {
        match self {
            Widget::Text { .. } => None,
            Widget::Button { id, .. } | Widget::Toggle { id, .. } | Widget::Stepper { id, .. } | Widget::Input { id, .. } | Widget::List { id, .. } => Some(id),
        }
    }

    /// How many rows of the screen it takes.
    pub fn rows(&self) -> usize {
        match self {
            Widget::List { items, .. } => items.len().clamp(1, LIST_ROWS),
            _ => 1,
        }
    }
}

/// What a widget now holds.
#[derive(Clone, Debug, PartialEq)]
pub enum UiValue {
    Bool(bool),
    Int(i64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum UiKind {
    /// A button was pressed.
    Click,
    /// A toggle, stepper, input or list changed.
    Change(UiValue),
    /// The screen closed (by the player or by the mod).
    Closed,
}

/// Something the player did on a mod's screen.
#[derive(Clone, Debug, PartialEq)]
pub struct UiEvent {
    pub mod_name: String,
    /// The screen's name.
    pub screen: String,
    /// The widget's id (empty for `Closed`).
    pub id: String,
    pub kind: UiKind,
}

/// The open screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Open {
    pub mod_name: String,
    pub name: String,
    pub title: String,
    pub widgets: Vec<Widget>,
    /// The first widget shown (the screen scrolls when it is taller than the window).
    pub scroll: i32,
}

#[derive(Debug, Default)]
pub struct Screens {
    pub open: Option<Open>,
    /// A screen was opened and the game has not switched to it yet.
    pub opened_now: bool,
    pub events: Vec<UiEvent>,
}

impl Screens {
    /// Open a screen (empty, with a title) for `mod_name`; one that was open closes.
    pub fn open(&mut self, mod_name: &str, name: &str, title: &str) -> bool {
        let Some(name) = clean_id(name) else { return false };
        if self.open.is_some() {
            self.close();
        }
        self.open = Some(Open { mod_name: String::from(mod_name), name, title: clean_text(title, 40), widgets: Vec::new(), scroll: 0 });
        self.opened_now = true;
        true
    }

    /// Close the open screen (an event tells its mod).
    pub fn close(&mut self) {
        if let Some(o) = self.open.take() {
            self.push_event(UiEvent { mod_name: o.mod_name, screen: o.name, id: String::new(), kind: UiKind::Closed });
        }
        self.opened_now = false;
    }

    /// Close the screen if it is `mod_name`'s (a mod asked for it).
    pub fn close_for(&mut self, mod_name: &str) -> bool {
        if self.open.as_ref().map_or(false, |o| o.mod_name == mod_name) {
            self.close();
            return true;
        }
        false
    }

    /// Forget a mod's screen without a word (the mod was switched off).
    pub fn drop_mod(&mut self, mod_name: &str) {
        if self.open.as_ref().map_or(false, |o| o.mod_name == mod_name) {
            self.open = None;
            self.opened_now = false;
        }
        self.events.retain(|e| e.mod_name != mod_name);
    }

    pub fn push_event(&mut self, ev: UiEvent) {
        if self.events.len() < MAX_UI_EVENTS {
            self.events.push(ev);
        }
    }

    /// Add a widget to `mod_name`'s open screen. `false`: no such screen, too
    /// many widgets, or an id already used on it.
    pub fn add(&mut self, mod_name: &str, w: Widget) -> bool {
        let Some(o) = self.open.as_mut().filter(|o| o.mod_name == mod_name) else { return false };
        if o.widgets.len() >= MAX_WIDGETS {
            return false;
        }
        if let Some(id) = w.id() {
            if o.widgets.iter().any(|x| x.id() == Some(id)) {
                return false;
            }
        }
        o.widgets.push(w);
        true
    }

    pub fn text(&mut self, mod_name: &str, text: &str) -> bool {
        self.add(mod_name, Widget::Text { text: clean_text(text, MAX_TEXT) })
    }

    pub fn button(&mut self, mod_name: &str, id: &str, label: &str) -> bool {
        match clean_id(id) {
            Some(id) => self.add(mod_name, Widget::Button { id, label: clean_text(label, MAX_LABEL) }),
            None => false,
        }
    }

    pub fn toggle(&mut self, mod_name: &str, id: &str, label: &str, value: bool) -> bool {
        match clean_id(id) {
            Some(id) => self.add(mod_name, Widget::Toggle { id, label: clean_text(label, MAX_LABEL), value }),
            None => false,
        }
    }

    pub fn stepper(&mut self, mod_name: &str, id: &str, label: &str, value: i64, min: i64, max: i64) -> bool {
        let (lo, hi) = (min.clamp(-1_000_000, 1_000_000) as i32, max.clamp(-1_000_000, 1_000_000) as i32);
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        match clean_id(id) {
            Some(id) => self.add(mod_name, Widget::Stepper { id, label: clean_text(label, MAX_LABEL), value: (value.clamp(-1_000_000, 1_000_000) as i32).clamp(lo, hi), min: lo, max: hi }),
            None => false,
        }
    }

    pub fn input(&mut self, mod_name: &str, id: &str, label: &str, value: &str) -> bool {
        match clean_id(id) {
            Some(id) => self.add(mod_name, Widget::Input { id, label: clean_text(label, 20), value: value.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).take(INPUT_MAX).collect() }),
            None => false,
        }
    }

    /// A list; `items` are separated by newlines.
    pub fn list(&mut self, mod_name: &str, id: &str, items: &str, selected: i64) -> bool {
        let items: Vec<String> = items.lines().take(MAX_LIST_ITEMS).map(|l| clean_text(l, 60)).collect();
        let sel = if selected >= 0 && (selected as usize) < items.len() { selected as i32 } else { -1 };
        match clean_id(id) {
            Some(id) => self.add(mod_name, Widget::List { id, items, selected: sel, scroll: 0 }),
            None => false,
        }
    }

    fn find(&mut self, mod_name: &str, id: &str) -> Option<&mut Widget> {
        let o = self.open.as_mut().filter(|o| o.mod_name == mod_name)?;
        o.widgets.iter_mut().find(|w| w.id() == Some(id))
    }

    /// Change a widget's text: a text line (by position, see [`Screens::set_line`]), a button's or toggle's
    /// label, an input's contents, or a list's items (newline separated).
    pub fn set_text(&mut self, mod_name: &str, id: &str, text: &str) -> bool {
        match self.find(mod_name, id) {
            Some(Widget::Button { label, .. }) | Some(Widget::Toggle { label, .. }) | Some(Widget::Stepper { label, .. }) => {
                *label = clean_text(text, MAX_LABEL);
                true
            }
            Some(Widget::Input { value, .. }) => {
                *value = text.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).take(INPUT_MAX).collect();
                true
            }
            Some(Widget::List { items, selected, scroll, .. }) => {
                *items = text.lines().take(MAX_LIST_ITEMS).map(|l| clean_text(l, 60)).collect();
                if *selected >= items.len() as i32 {
                    *selected = -1;
                }
                *scroll = 0;
                true
            }
            _ => false,
        }
    }

    /// Change the text of the `n`th widget if it is a text line.
    pub fn set_line(&mut self, mod_name: &str, n: usize, text: &str) -> bool {
        match self.open.as_mut().filter(|o| o.mod_name == mod_name).and_then(|o| o.widgets.get_mut(n)) {
            Some(Widget::Text { text: t }) => {
                *t = clean_text(text, MAX_TEXT);
                true
            }
            _ => false,
        }
    }

    /// Change a toggle (nonzero is on), a stepper (kept in its range) or a
    /// list's selected entry (out of range: none).
    pub fn set_value(&mut self, mod_name: &str, id: &str, n: i64) -> bool {
        match self.find(mod_name, id) {
            Some(Widget::Toggle { value, .. }) => {
                *value = n != 0;
                true
            }
            Some(Widget::Stepper { value, min, max, .. }) => {
                *value = (n.clamp(-1_000_000, 1_000_000) as i32).clamp(*min, *max);
                true
            }
            Some(Widget::List { items, selected, .. }) => {
                *selected = if n >= 0 && (n as usize) < items.len() { n as i32 } else { -1 };
                true
            }
            _ => false,
        }
    }

    /// What an input holds, or a list's selected entry's text.
    pub fn get_text(&mut self, mod_name: &str, id: &str) -> Option<String> {
        match self.find(mod_name, id)? {
            Widget::Input { value, .. } => Some(value.clone()),
            Widget::List { items, selected, .. } => items.get((*selected).max(0) as usize).filter(|_| *selected >= 0).cloned(),
            Widget::Button { label, .. } | Widget::Toggle { label, .. } | Widget::Stepper { label, .. } => Some(label.clone()),
            _ => None,
        }
    }

    /// A toggle's, stepper's or list's value (a list with nothing chosen is -1).
    pub fn get_value(&mut self, mod_name: &str, id: &str) -> Option<i64> {
        match self.find(mod_name, id)? {
            Widget::Toggle { value, .. } => Some(*value as i64),
            Widget::Stepper { value, .. } => Some(*value as i64),
            Widget::List { selected, .. } => Some(*selected as i64),
            _ => None,
        }
    }

    /// Take the waiting events.
    pub fn take_events(&mut self) -> Vec<UiEvent> {
        core::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> Screens {
        let mut s = Screens::default();
        assert!(s.open("m", "stats", "my stats"));
        s
    }

    #[test]
    fn a_screen_collects_widgets_for_its_mod_only() {
        let mut s = open();
        assert_eq!(s.open.as_ref().unwrap().title, "MY STATS");
        assert!(s.opened_now);
        assert!(s.text("m", "hello"));
        assert!(s.button("m", "go", "go on"));
        assert!(s.toggle("m", "t", "on?", true));
        assert!(s.stepper("m", "n", "count", 5, 0, 10));
        assert!(s.input("m", "name", "name", "abc"));
        assert!(s.list("m", "l", "a\nb\nc", 1));
        // Another mod cannot add to it.
        assert!(!s.button("other", "x", "x"));
        // A used id is refused, a bad id too.
        assert!(!s.button("m", "go", "again"));
        assert!(!s.button("m", "bad id", "x"));
        assert_eq!(s.open.as_ref().unwrap().widgets.len(), 6);
        // Nothing open: nothing added.
        let mut none = Screens::default();
        assert!(!none.text("m", "x"));
        assert!(!s.open("m", "bad name", "x"));
        assert_eq!(s.open.as_ref().unwrap().name, "stats");
    }

    #[test]
    fn widgets_are_capped_and_text_is_cleaned() {
        let mut s = open();
        for i in 0..MAX_WIDGETS {
            assert!(s.text("m", &format!("line {}", i)));
        }
        assert!(!s.text("m", "one too many"));
        let mut s = open();
        s.text("m", &"x".repeat(1000));
        match &s.open.as_ref().unwrap().widgets[0] {
            Widget::Text { text } => assert_eq!(text.len(), MAX_TEXT),
            w => panic!("{:?}", w),
        }
        let many: String = (0..1000).map(|i| format!("item {}\n", i)).collect();
        s.list("m", "l", &many, 0);
        match &s.open.as_ref().unwrap().widgets[1] {
            Widget::List { items, .. } => assert_eq!(items.len(), MAX_LIST_ITEMS),
            w => panic!("{:?}", w),
        }
        s.input("m", "i", "in", &"y".repeat(500));
        match &s.open.as_ref().unwrap().widgets[2] {
            Widget::Input { value, .. } => assert_eq!(value.len(), INPUT_MAX),
            w => panic!("{:?}", w),
        }
    }

    #[test]
    fn steppers_and_lists_keep_their_values_in_range() {
        let mut s = open();
        s.stepper("m", "n", "n", 50, 0, 10);
        assert_eq!(s.get_value("m", "n"), Some(10));
        s.stepper("m", "r", "r", 3, 9, 1);
        assert_eq!(s.get_value("m", "r"), Some(3), "a reversed range is turned round");
        assert!(s.set_value("m", "n", -4));
        assert_eq!(s.get_value("m", "n"), Some(0));
        s.list("m", "l", "a\nb", 7);
        assert_eq!(s.get_value("m", "l"), Some(-1));
        assert!(s.set_value("m", "l", 1));
        assert_eq!(s.get_text("m", "l").as_deref(), Some("B"));
        assert!(s.set_value("m", "l", 2));
        assert_eq!(s.get_text("m", "l"), None);
        s.toggle("m", "t", "t", false);
        assert!(s.set_value("m", "t", 5));
        assert_eq!(s.get_value("m", "t"), Some(1));
        assert!(!s.set_value("m", "nope", 1));
        assert!(!s.set_value("other", "t", 1));
    }

    #[test]
    fn texts_can_be_changed_after_the_fact() {
        let mut s = open();
        s.text("m", "first");
        s.input("m", "i", "in", "");
        s.button("m", "b", "old");
        s.list("m", "l", "a", 0);
        assert!(s.set_line("m", 0, "second"));
        assert!(!s.set_line("m", 1, "not a text line"));
        assert!(!s.set_line("m", 99, "x"));
        assert!(s.set_text("m", "i", "typed"));
        assert_eq!(s.get_text("m", "i").as_deref(), Some("typed"));
        assert!(s.set_text("m", "b", "new"));
        assert_eq!(s.get_text("m", "b").as_deref(), Some("NEW"));
        assert!(s.set_text("m", "l", "x\ny\nz"));
        assert_eq!(s.get_value("m", "l"), Some(0));
        match &s.open.as_ref().unwrap().widgets[0] {
            Widget::Text { text } => assert_eq!(text, "SECOND"),
            w => panic!("{:?}", w),
        }
    }

    #[test]
    fn closing_tells_the_mod_and_a_new_screen_closes_the_old() {
        let mut s = open();
        assert!(s.open("m", "second", "two"));
        let ev = s.take_events();
        assert_eq!(ev.len(), 1);
        assert_eq!((ev[0].screen.as_str(), &ev[0].kind), ("stats", &UiKind::Closed));
        assert!(!s.close_for("other"));
        assert!(s.close_for("m"));
        assert!(s.open.is_none());
        assert_eq!(s.take_events()[0].screen, "second");
        // Closing nothing says nothing.
        s.close();
        assert!(s.take_events().is_empty());
    }

    #[test]
    fn a_mod_switched_off_leaves_no_screen_or_events() {
        let mut s = open();
        s.push_event(UiEvent { mod_name: String::from("m"), screen: String::from("stats"), id: String::from("go"), kind: UiKind::Click });
        s.push_event(UiEvent { mod_name: String::from("n"), screen: String::from("x"), id: String::from("go"), kind: UiKind::Click });
        s.drop_mod("m");
        assert!(s.open.is_none());
        assert_eq!(s.events.len(), 1);
        assert_eq!(s.events[0].mod_name, "n");
    }

    #[test]
    fn the_event_queue_is_bounded() {
        let mut s = Screens::default();
        for _ in 0..1000 {
            s.push_event(UiEvent { mod_name: String::from("m"), screen: String::from("s"), id: String::from("b"), kind: UiKind::Click });
        }
        assert_eq!(s.events.len(), MAX_UI_EVENTS);
        assert_eq!(s.take_events().len(), MAX_UI_EVENTS);
        assert!(s.events.is_empty());
    }

    #[test]
    fn rows_count_lists_by_their_shown_rows() {
        assert_eq!(Widget::Text { text: String::new() }.rows(), 1);
        assert_eq!(Widget::List { id: String::new(), items: Vec::new(), selected: -1, scroll: 0 }.rows(), 1);
        assert_eq!(Widget::List { id: String::new(), items: alloc::vec![String::new(); 3], selected: -1, scroll: 0 }.rows(), 3);
        assert_eq!(Widget::List { id: String::new(), items: alloc::vec![String::new(); 30], selected: -1, scroll: 0 }.rows(), LIST_ROWS);
    }
}
