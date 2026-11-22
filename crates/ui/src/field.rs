//! Single- and multi-line text fields: Rune's text editor with Folio's
//! field styling. A field is shared between the view and the handlers that
//! read it through [`Entity`].
use super::*;
use rune::{TextStyle, Tx};
use std::{cell::RefCell, rc::Rc};

/// A shared, mutable view object, as a GPUI entity is.
pub struct Entity<T>(pub Rc<RefCell<T>>);
impl<T> Clone for Entity<T> {
    fn clone(&self) -> Self {
        Entity(self.0.clone())
    }
}
impl<T> Entity<T> {
    pub fn new(value: T) -> Self {
        Entity(Rc::new(RefCell::new(value)))
    }
    pub fn read<C>(&self, _: &C) -> std::cell::Ref<'_, T> {
        self.0.borrow()
    }
    pub fn update<C, R>(&self, cx: &mut C, f: impl FnOnce(&mut T, &mut C) -> R) -> R {
        f(&mut self.0.borrow_mut(), cx)
    }
    pub fn entity_id(&self) -> usize {
        Rc::as_ptr(&self.0) as *const () as usize
    }
}
impl<T> PartialEq for Entity<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

pub type FieldCallback = Rc<dyn Fn(&mut NotesView, &mut Window, &mut Context<NotesView>)>;
pub type NavigateCallback = Rc<dyn Fn(&mut NotesView, bool, &mut Window, &mut Context<NotesView>)>;

static NEXT_FIELD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Clone)]
pub struct InlineStyle {
    pub text: std::sync::Arc<folio_document::TextBlock>,
    pub source: std::sync::Arc<folio_document::Object>,
    pub transform: folio_document::Transform,
    pub mask: Bounds<Pixels>,
    pub color: u32,
    pub handles: Vec<folio_document::Point>,
}

pub struct Field {
    pub edit: rune::TextEdit,
    pub focus: FocusHandle,
    pub multiline: bool,
    pub height: Option<f32>,
    pub secret: bool,
    pub theme: Theme,
    pub placeholder: Option<SharedString>,
    /// Tighter padding, for toolbars such as the find bar.
    pub compact: bool,
    pub inline: Option<InlineStyle>,
    pub on_submit: Option<FieldCallback>,
    pub on_change: Option<FieldCallback>,
    pub on_navigate: Option<NavigateCallback>,
    pub on_escape: Option<FieldCallback>,
    key: String,
}
impl Field {
    pub fn new(content: String, multiline: bool) -> Self {
        let id = NEXT_FIELD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let key = format!("field-{id}");
        let mut edit = rune::TextEdit::new(content);
        let end = edit.text.len();
        edit.set_caret(end);
        Self {
            edit,
            focus: FocusHandle::new(key.clone()),
            multiline,
            height: None,
            secret: false,
            theme: Theme::new(&folio_app::Settings::default()),
            placeholder: None,
            compact: false,
            inline: None,
            on_submit: None,
            on_change: None,
            on_navigate: None,
            on_escape: None,
            key,
        }
    }
    pub fn content(&self) -> String {
        self.edit.text.clone()
    }
    pub fn select_all_content(&mut self) {
        let end = self.edit.text.len();
        self.edit.set_selection(0, end);
    }
    pub fn set_content<C>(&mut self, content: String, _: &mut C) {
        self.edit.replace_text(content);
        let end = self.edit.text.len();
        self.edit.set_caret(end);
        self.edit.clear_history();
    }
    pub fn set_accessible_content<C>(&mut self, content: String, _: &mut C) {
        self.edit.replace_text(content);
    }
    pub fn key(&self) -> &str {
        &self.key
    }
}

impl Entity<Field> {
    /// Builds the field's element. The editor's text lives in the field;
    /// Rune edits it in place through `on_edit`.
    pub fn render(&self) -> Div {
        let field = self.0.borrow();
        let theme = field.theme;
        // Text edited on the canvas draws as the canvas draws it: at its
        // size, weight and color under the page's zoom.
        let style = match &field.inline {
            Some(inline) => {
                let t = inline.transform;
                let zoom = (t.a * t.a + t.b * t.b).sqrt().max(0.01);
                let size = inline.text.font_size * zoom;
                let mut style =
                    TextStyle::new(size, if inline.text.bold { 700. } else { 400. }).lh(size * 1.4);
                if inline.text.italic {
                    style = style.italic();
                }
                style
            }
            None => TextStyle::new(14., 400.).lh(22.),
        };
        let ink = match &field.inline {
            Some(inline) => rgb(inline.color).color(),
            None => rgb(theme.ink).color(),
        };
        let shared = self.0.clone();
        let mut input = rune::text_input(&field.edit, style)
            .key(field.key.clone())
            .color(ink)
            .caret(ink, rgba((theme.accent << 8) | 0x40).color())
            // SAFETY: the view is single-threaded and Rune calls this accessor
            // only between frames, while no borrow of the field is held.
            .on_edit(move |_: &mut crate::AppState| unsafe { &mut (*shared.as_ptr()).edit })
            .w_full();
        if let Some(placeholder) = &field.placeholder {
            input = input.placeholder(placeholder.clone(), rgb(theme.muted).color());
        }
        if field.secret {
            input = input.secure();
        }
        if !field.multiline {
            input = input.single_line();
        }
        if field.multiline && field.inline.is_none() {
            input = input.min_h(field.height.unwrap_or(110.));
        }
        if let Some(inline) = &field.inline {
            input = input.align(match inline.text.alignment {
                folio_document::Alignment::Left => rune::Align::Start,
                folio_document::Alignment::Center => rune::Align::Center,
                folio_document::Alignment::Right => rune::Align::End,
            });
        }
        let submit = field.on_submit.clone();
        let multiline = field.multiline;
        if let Some(submit) = submit {
            input = input.on_submit(move |s: &mut crate::AppState, tx: &mut Tx| {
                let f = submit.clone();
                dispatch(s, tx, |view, window, cx| f(view, window, cx));
            });
        }
        let change = field.on_change.clone();
        if let Some(change) = change {
            input = input.on_input_change(move |s: &mut crate::AppState, tx: &mut Tx| {
                let f = change.clone();
                dispatch(s, tx, |view, window, cx| f(view, window, cx));
            });
        }
        let navigate = field.on_navigate.clone();
        let escape = field.on_escape.clone();
        input = input.on_input_key(
            move |s: &mut crate::AppState, key: &rune::KeyEvent, tx: &mut Tx| match key.key {
                rune::Key::Up | rune::Key::Down if !multiline => {
                    if let Some(f) = navigate.clone() {
                        let forward = matches!(key.key, rune::Key::Down);
                        dispatch(s, tx, |view, window, cx| f(view, forward, window, cx));
                        return true;
                    }
                    false
                }
                rune::Key::Escape => {
                    if let Some(f) = escape.clone() {
                        dispatch(s, tx, |view, window, cx| f(view, window, cx));
                        return true;
                    }
                    false
                }
                _ => false,
            },
        );
        let compact = field.compact;
        let inline = field.inline.is_some();
        drop(field);
        div()
            .w_full()
            .when(!inline, |s| {
                s.bg(theme.raised())
                    .border_1()
                    .border_color(theme.input)
                    .rounded(px(theme.radius))
            })
            .text_color(rgb(theme.ink))
            .when(!compact && !inline, |s| s.p_3())
            .when(compact, |s| s.px_2().py_1())
            .when(!inline, |s| {
                let ring = rgb(theme.ring).color();
                s.el(move |e| e.focus_within_ring(1., -1., ring))
            })
            .child(input)
    }
}

impl IntoElement for Entity<Field> {
    fn into_any_element(self) -> AnyElement {
        self.render().into_any_element()
    }
}
impl IntoElement for &Entity<Field> {
    fn into_any_element(self) -> AnyElement {
        self.render().into_any_element()
    }
}

/// Field events GPUI views subscribed to.
pub struct Submitted;
pub struct Navigate(pub bool);
pub struct FieldBoundsChanged;
pub trait FieldEvent: Sized + 'static {
    fn install(
        field: &mut Field,
        f: Rc<dyn Fn(&mut NotesView, &Self, &mut Window, &mut Context<NotesView>)>,
    );
}
impl FieldEvent for Submitted {
    fn install(
        field: &mut Field,
        f: Rc<dyn Fn(&mut NotesView, &Self, &mut Window, &mut Context<NotesView>)>,
    ) {
        field.on_submit = Some(Rc::new(move |view, window, cx| {
            f(view, &Submitted, window, cx)
        }));
    }
}
impl FieldEvent for Navigate {
    fn install(
        field: &mut Field,
        f: Rc<dyn Fn(&mut NotesView, &Self, &mut Window, &mut Context<NotesView>)>,
    ) {
        field.on_navigate = Some(Rc::new(move |view, down, window, cx| {
            f(view, &Navigate(down), window, cx)
        }));
    }
}
impl FieldEvent for FieldBoundsChanged {
    fn install(
        _: &mut Field,
        _: Rc<dyn Fn(&mut NotesView, &Self, &mut Window, &mut Context<NotesView>)>,
    ) {
    }
}
impl<T> Context<T> {
    /// A new entity, as GPUI's `cx.new` makes one.
    pub fn new<U>(&mut self, f: impl FnOnce(&mut Context<U>) -> U) -> Entity<U> {
        let mut cx = Context::fresh();
        let value = f(&mut cx);
        self.effects.extend(cx.effects);
        Entity::new(value)
    }
    /// Runs `f` whenever the field's text changes.
    pub fn observe(
        &mut self,
        entity: &Entity<Field>,
        f: impl Fn(&mut NotesView, Entity<Field>, &mut Context<NotesView>) + 'static,
    ) -> Subscription {
        let handle = entity.clone();
        entity.0.borrow_mut().on_change =
            Some(Rc::new(move |view, _, cx| f(view, handle.clone(), cx)));
        Subscription
    }
    pub fn subscribe<E: FieldEvent>(
        &mut self,
        entity: &Entity<Field>,
        f: impl Fn(&mut NotesView, Entity<Field>, &E, &mut Context<NotesView>) + 'static,
    ) -> Subscription {
        let handle = entity.clone();
        E::install(
            &mut entity.0.borrow_mut(),
            Rc::new(move |view, event, _, cx| f(view, handle.clone(), event, cx)),
        );
        Subscription
    }
    pub fn subscribe_in<E: FieldEvent>(
        &mut self,
        entity: &Entity<Field>,
        _: &Window,
        f: impl Fn(&mut NotesView, &Entity<Field>, &E, &mut Window, &mut Context<NotesView>) + 'static,
    ) -> Subscription {
        let handle = entity.clone();
        E::install(
            &mut entity.0.borrow_mut(),
            Rc::new(move |view, event, window, cx| f(view, &handle, event, window, cx)),
        );
        Subscription
    }
}
impl Field {
    /// GPUI's three-argument constructor.
    pub fn with_cx<C>(content: String, multiline: bool, _: &mut C) -> Self {
        Self::new(content, multiline)
    }
}
