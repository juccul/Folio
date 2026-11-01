//! The editor's "Document actions" popover and its sections.
use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum MoreSection {
    #[default]
    Document,
    Page,
    Math,
}

/// A command listed in the Document actions popover.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DocumentAction {
    Rename,
    Duplicate,
    EditTags,
    MoveToFolder,
    Export,
    NewDocumentOptions,
    ToggleTrash,
    DuplicatePage,
    BookmarkPage,
    MovePage,
    UseAsCover,
    SaveTemplate,
    AddFromTemplate,
    ToggleInfinite,
    PageSize,
    InsertSpace,
    RemoveSpace,
    DeletePage,
    MathSolver,
    InsertEquation,
    IndexHandwriting,
    ClearHandwritingIndex,
}
impl DocumentAction {
    /// Control identifier, also used to decide whether the control is enabled.
    fn id(self) -> &'static str {
        match self {
            Self::Rename => "rename-note",
            Self::Duplicate => "duplicate-note",
            Self::EditTags => "tags",
            Self::MoveToFolder => "move-document-folder",
            Self::Export => "export",
            Self::NewDocumentOptions => "new-document-options",
            Self::ToggleTrash => "trash-note",
            Self::DuplicatePage => "duplicate-page",
            Self::BookmarkPage => "bookmark-page",
            Self::MovePage => "move-page",
            Self::UseAsCover => "cover-page",
            Self::SaveTemplate => "save-page-template",
            Self::AddFromTemplate => "page-templates",
            Self::ToggleInfinite => "infinite",
            Self::PageSize => "page-size",
            Self::InsertSpace => "insert-space",
            Self::RemoveSpace => "remove-space",
            Self::DeletePage => "delete-page",
            Self::MathSolver => "open-math-solver",
            Self::InsertEquation => "insert-equation",
            Self::IndexHandwriting => "index-page-handwriting",
            Self::ClearHandwritingIndex => "clear-page-index",
        }
    }
    fn label(self, trashed: bool) -> &'static str {
        match self {
            Self::Rename => "Rename…",
            Self::Duplicate => "Duplicate",
            Self::EditTags => "Edit tags…",
            Self::MoveToFolder => "Move to folder…",
            Self::Export => "Export",
            Self::NewDocumentOptions => "New document with options…",
            Self::ToggleTrash if trashed => "Restore document",
            Self::ToggleTrash => "Move to trash",
            Self::DuplicatePage => "Duplicate page",
            Self::BookmarkPage => "Name bookmark…",
            Self::MovePage => "Move to another document…",
            Self::UseAsCover => "Use as document cover",
            Self::SaveTemplate => "Save as template…",
            Self::AddFromTemplate => "Add page from template…",
            Self::ToggleInfinite => "Infinite canvas",
            Self::PageSize => "Page size…",
            Self::InsertSpace => "Insert space below",
            Self::RemoveSpace => "Remove space below",
            Self::DeletePage => "Delete page",
            Self::MathSolver => "Math solver",
            Self::InsertEquation => "Insert LaTeX equation…",
            Self::IndexHandwriting => "Index handwriting on this page…",
            Self::ClearHandwritingIndex => "Clear handwriting index",
        }
    }
    fn icon(self, trashed: bool) -> Icon {
        match self {
            Self::Rename => Icon::Pencil,
            Self::Duplicate | Self::DuplicatePage => Icon::Copy,
            Self::EditTags => Icon::Tag,
            Self::MoveToFolder => Icon::Folder,
            Self::Export => Icon::Export,
            Self::NewDocumentOptions => Icon::NewPage,
            Self::ToggleTrash if trashed => Icon::Undo,
            Self::ToggleTrash | Self::DeletePage => Icon::Trash,
            Self::BookmarkPage => Icon::Bookmark,
            Self::MovePage => Icon::MovePage,
            Self::UseAsCover => Icon::Image,
            Self::SaveTemplate | Self::AddFromTemplate => Icon::Template,
            Self::ToggleInfinite => Icon::Infinite,
            Self::PageSize => Icon::Ruler,
            Self::InsertSpace => Icon::SpaceInsert,
            Self::RemoveSpace => Icon::SpaceRemove,
            Self::MathSolver => Icon::Calculator,
            Self::InsertEquation => Icon::Math,
            Self::IndexHandwriting => Icon::TextScan,
            Self::ClearHandwritingIndex => Icon::Eraser,
        }
    }
}

/// One entry of a Document actions section.
enum Entry {
    Action(DocumentAction),
    /// Opens another section.
    Section(MoreSection),
    /// A muted heading over the entries that follow.
    Heading(&'static str),
    Separator,
    PaperPicker,
}

impl NotesView {
    fn run_document_action(
        &mut self,
        action: DocumentAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Toggles stay open so their new state is visible.
        if action != DocumentAction::ToggleInfinite {
            self.more_open = false;
        }
        match action {
            DocumentAction::Rename => self.modal(Modal::Rename, window, cx),
            DocumentAction::Duplicate => self.controller.duplicate_note(),
            DocumentAction::EditTags => self.modal(Modal::Tags, window, cx),
            DocumentAction::MoveToFolder => {
                self.modal(Modal::MoveDocument(self.controller.active), window, cx)
            }
            DocumentAction::Export => {
                self.dismiss_popovers();
                self.export_open = true;
            }
            DocumentAction::NewDocumentOptions => self.modal(Modal::NewDocument, window, cx),
            DocumentAction::ToggleTrash => self
                .controller
                .manage_note(self.controller.active, folio_app::NoteAction::Trash),
            DocumentAction::DuplicatePage => self.controller.duplicate_page(),
            DocumentAction::BookmarkPage => self.modal(Modal::PageBookmark, window, cx),
            DocumentAction::MovePage => self.modal(Modal::MovePage, window, cx),
            DocumentAction::UseAsCover => self.controller.use_page_as_cover(),
            DocumentAction::SaveTemplate => self.modal(Modal::SaveTemplate, window, cx),
            DocumentAction::AddFromTemplate => self.modal(Modal::Templates, window, cx),
            DocumentAction::ToggleInfinite => {
                let p = self.controller.page().properties.clone();
                self.controller.page_size(p.width, p.height, !p.infinite)
            }
            DocumentAction::PageSize => self.modal(Modal::PageSize, window, cx),
            DocumentAction::InsertSpace | DocumentAction::RemoveSpace => {
                let at = self
                    .controller
                    .selection_bounds()
                    .map(|r| r.max.y)
                    .unwrap_or(self.controller.cursor.unwrap_or(DocPoint::new(0., 200.)).y);
                let amount = if action == DocumentAction::InsertSpace {
                    80.
                } else {
                    -40.
                };
                self.controller.insert_space(at, amount, true)
            }
            DocumentAction::DeletePage => self.controller.delete_page(),
            DocumentAction::MathSolver => self.start_math(window, cx),
            DocumentAction::InsertEquation => self.modal(Modal::Equation, window, cx),
            DocumentAction::IndexHandwriting => {
                if let Err(e) = self.controller.index_page_handwriting() {
                    self.controller.error = Some(e);
                }
            }
            DocumentAction::ClearHandwritingIndex => self.controller.clear_handwriting_index(),
        }
    }
    /// Contents of the Document actions popover for the current section.
    pub(crate) fn document_actions_menu(
        &self,
        panel: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        use DocumentAction as A;
        use Entry::*;
        let theme = Theme::new(&self.controller.settings);
        let section = self.more_section;
        let entries: &[Entry] = match section {
            MoreSection::Document => &[
                Action(A::Rename),
                Action(A::Duplicate),
                Action(A::EditTags),
                Action(A::MoveToFolder),
                Action(A::Export),
                Separator,
                Section(MoreSection::Page),
                Section(MoreSection::Math),
                Separator,
                Action(A::NewDocumentOptions),
                Separator,
                Action(A::ToggleTrash),
            ],
            MoreSection::Page => &[
                Action(A::DuplicatePage),
                Action(A::BookmarkPage),
                Action(A::MovePage),
                Action(A::UseAsCover),
                Separator,
                Action(A::SaveTemplate),
                Action(A::AddFromTemplate),
                Separator,
                Heading("Paper"),
                PaperPicker,
                Action(A::ToggleInfinite),
                Action(A::PageSize),
                Separator,
                Heading("Space"),
                Action(A::InsertSpace),
                Action(A::RemoveSpace),
                Separator,
                Action(A::DeletePage),
            ],
            MoreSection::Math => &[
                Action(A::MathSolver),
                Action(A::InsertEquation),
                Separator,
                Heading("Handwriting search"),
                Action(A::IndexHandwriting),
                Action(A::ClearHandwritingIndex),
            ],
        };
        let mut menu = div().flex().flex_col().gap(px(1.));
        if section != MoreSection::Document {
            let session = self.controller.session();
            let title = match section {
                MoreSection::Page => format!(
                    "Page {} of {}",
                    session.page + 1,
                    session.document.pages.len()
                ),
                _ => "Math and handwriting".into(),
            };
            menu = menu
                .child(
                    self.menu_row(
                        "more-back",
                        "‹ Document actions".into(),
                        div().font_weight(FontWeight::SEMIBOLD).child(title),
                        Icon::Back,
                        theme.muted,
                        None,
                        cx,
                        |this, _, _| this.more_section = MoreSection::Document,
                    ),
                )
                .child(separator(theme));
        }
        let trashed = self.controller.session().document.metadata.trashed;
        let properties = &self.controller.page().properties;
        for entry in entries {
            menu = menu.child(match *entry {
                Separator => separator(theme).into_any_element(),
                Heading(text) => heading(theme, text).into_any_element(),
                Section(target) => {
                    let (key, text, kind) = match target {
                        MoreSection::Page => ("page", "Page", Icon::Page),
                        _ => ("math", "Math and handwriting", Icon::Math),
                    };
                    self.menu_row(
                        format!("more-section-{key}"),
                        format!("{text} ›").into(),
                        div().child(text),
                        kind,
                        theme.muted,
                        Some(chevron(theme)),
                        cx,
                        move |this, _, _| this.more_section = target,
                    )
                    .into_any_element()
                }
                PaperPicker => self.paper_picker(cx).into_any_element(),
                Action(action) => {
                    let text = action.label(trashed);
                    let destructive =
                        matches!(action, A::DeletePage) || (action == A::ToggleTrash && !trashed);
                    let label: SharedString = match action {
                        A::Export => "Export document ›".into(),
                        A::ToggleInfinite => format!(
                            "{text}: {}",
                            if properties.infinite { "On" } else { "Off" }
                        )
                        .into(),
                        _ => text.into(),
                    };
                    let trailing = match action {
                        A::Export => Some(chevron(theme)),
                        A::ToggleInfinite => {
                            let progress = self.motion.borrow_mut().value(
                                action.id(),
                                if properties.infinite { 1. } else { 0. },
                                self.controller.settings.reduce_motion,
                                Instant::now(),
                            );
                            Some(appearance::switch_track(theme, progress).into_any_element())
                        }
                        A::PageSize if !properties.infinite => Some(
                            hint(
                                theme,
                                format!("{:.0} × {:.0}", properties.width, properties.height),
                            )
                            .into_any_element(),
                        ),
                        _ => None,
                    };
                    self.menu_row(
                        action.id(),
                        label,
                        div().child(text),
                        action.icon(trashed),
                        if destructive {
                            theme.destructive
                        } else {
                            theme.muted
                        },
                        trailing,
                        cx,
                        move |this, window, cx| this.run_document_action(action, window, cx),
                    )
                    .into_any_element()
                }
            });
        }
        panel.p_1p5().child(menu)
    }
    /// A compact popover row: icon, text and an optional trailing element.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn menu_row(
        &self,
        id: impl Into<SharedString>,
        label: SharedString,
        text: Div,
        kind: Icon,
        icon_color: u32,
        trailing: Option<AnyElement>,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        self.control(
            id,
            label,
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(icon(kind, icon_color).size(rems(1.125)))
                .child(text.flex_1().min_w_0().truncate())
                .children(trailing)
                .into_any_element(),
            false,
            cx,
            action,
        )
        .w_full()
        .justify_start()
        .min_h(rems(2.))
        .px_2()
        .py_1()
    }
    /// Paper patterns as equal tiles, the current one highlighted.
    fn paper_picker(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let current = self.controller.page().properties.paper;
        let mut row = div().flex().gap_1().px_1().pb_1();
        for (id, label, paper, kind) in [
            ("blank", "Blank", Paper::Blank, Icon::Maximize),
            ("ruled", "Ruled", Paper::Ruled, Icon::PaperRuled),
            ("grid", "Grid", Paper::Grid, Icon::PaperGrid),
            ("dots", "Dots", Paper::Dots, Icon::PaperDots),
        ] {
            let active = current == paper;
            row = row.child(
                self.control(
                    id,
                    label,
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(icon(kind, if active { theme.ink } else { theme.muted }))
                        .child(div().text_xs().child(label))
                        .into_any_element(),
                    active,
                    cx,
                    move |this, _, _| this.controller.paper(paper),
                )
                .flex_1()
                .min_w_0()
                .px_1()
                .py_2()
                .when(active, |s| s.border_color(rgb(theme.accent))),
            );
        }
        row
    }
}

/// A muted heading over the popover rows that follow.
pub(super) fn heading(theme: Theme, text: &'static str) -> Div {
    div()
        .px_2()
        .pt_1()
        .pb(px(2.))
        .text_xs()
        .text_color(rgb(theme.muted))
        .child(text)
}
pub(super) fn separator(theme: Theme) -> Div {
    div().h(px(1.)).mx_2().my_1().bg(theme.border)
}
pub(super) fn chevron(theme: Theme) -> AnyElement {
    icon(Icon::Forward, theme.muted)
        .size(rems(1.))
        .into_any_element()
}
pub(super) fn hint(theme: Theme, text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .text_xs()
        .text_color(rgb(theme.muted))
        .child(text.into())
}
