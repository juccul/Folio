//! The editor's "Document actions" popover and its sections.
use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum MoreSection {
    #[default]
    Document,
    Page,
    Paper,
    Math,
    Layout,
}
impl MoreSection {
    fn title(self) -> &'static str {
        match self {
            Self::Document => "Document",
            Self::Page => "Current page",
            Self::Paper => "Paper and canvas",
            Self::Math => "Math and handwriting",
            Self::Layout => "Page layout",
        }
    }
}

/// A command listed in one section of the Document actions popover.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DocumentAction {
    Rename,
    Duplicate,
    EditTags,
    ToggleTrash,
    DeletePage,
    DuplicatePage,
    BookmarkPage,
    MovePage,
    PageSize,
    ToggleInfinite,
    InsertSpace,
    RemoveSpace,
    InsertEquation,
    MathSolver,
    UseAsCover,
    SaveTemplate,
    AddFromTemplate,
    IndexHandwriting,
    ClearHandwritingIndex,
    NewDocumentOptions,
}
impl DocumentAction {
    /// Menu order; each section shows its actions in this order.
    const ALL: [Self; 20] = [
        Self::Rename,
        Self::Duplicate,
        Self::EditTags,
        Self::ToggleTrash,
        Self::DeletePage,
        Self::DuplicatePage,
        Self::BookmarkPage,
        Self::MovePage,
        Self::PageSize,
        Self::ToggleInfinite,
        Self::InsertSpace,
        Self::RemoveSpace,
        Self::InsertEquation,
        Self::MathSolver,
        Self::UseAsCover,
        Self::SaveTemplate,
        Self::AddFromTemplate,
        Self::IndexHandwriting,
        Self::ClearHandwritingIndex,
        Self::NewDocumentOptions,
    ];
    fn section(self) -> MoreSection {
        match self {
            Self::Rename
            | Self::Duplicate
            | Self::EditTags
            | Self::ToggleTrash
            | Self::NewDocumentOptions => MoreSection::Document,
            Self::DeletePage
            | Self::DuplicatePage
            | Self::BookmarkPage
            | Self::MovePage
            | Self::UseAsCover
            | Self::SaveTemplate
            | Self::AddFromTemplate => MoreSection::Page,
            Self::PageSize | Self::ToggleInfinite => MoreSection::Paper,
            Self::InsertEquation
            | Self::MathSolver
            | Self::IndexHandwriting
            | Self::ClearHandwritingIndex => MoreSection::Math,
            Self::InsertSpace | Self::RemoveSpace => MoreSection::Layout,
        }
    }
    /// Control identifier, also used to decide whether the control is enabled.
    fn id(self) -> &'static str {
        match self {
            Self::Rename => "rename-note",
            Self::Duplicate => "duplicate-note",
            Self::EditTags => "tags",
            Self::ToggleTrash => "trash-note",
            Self::DeletePage => "delete-page",
            Self::DuplicatePage => "duplicate-page",
            Self::BookmarkPage => "bookmark-page",
            Self::MovePage => "move-page",
            Self::PageSize => "page-size",
            Self::ToggleInfinite => "infinite",
            Self::InsertSpace => "insert-space",
            Self::RemoveSpace => "remove-space",
            Self::InsertEquation => "insert-equation",
            Self::MathSolver => "open-math-solver",
            Self::UseAsCover => "cover-page",
            Self::SaveTemplate => "save-page-template",
            Self::AddFromTemplate => "page-templates",
            Self::IndexHandwriting => "index-page-handwriting",
            Self::ClearHandwritingIndex => "clear-page-index",
            Self::NewDocumentOptions => "new-document-options",
        }
    }
    fn label(self, trashed: bool) -> &'static str {
        match self {
            Self::Rename => "Rename document…",
            Self::Duplicate => "Duplicate document",
            Self::EditTags => "Edit tags…",
            Self::ToggleTrash if trashed => "Restore document",
            Self::ToggleTrash => "Move to trash",
            Self::DeletePage => "Delete current page",
            Self::DuplicatePage => "Duplicate current page",
            Self::BookmarkPage => "Name page bookmark…",
            Self::MovePage => "Move page to document…",
            Self::PageSize => "Custom page size…",
            Self::ToggleInfinite => "Toggle infinite canvas",
            Self::InsertSpace => "Insert 80 units below selection",
            Self::RemoveSpace => "Remove 40 units below selection",
            Self::InsertEquation => "Insert LaTeX equation…",
            Self::MathSolver => "Math solver",
            Self::UseAsCover => "Use current page as cover",
            Self::SaveTemplate => "Save page as template…",
            Self::AddFromTemplate => "Add page from template…",
            Self::IndexHandwriting => "Index page handwriting…",
            Self::ClearHandwritingIndex => "Clear page handwriting index",
            Self::NewDocumentOptions => "New document with options…",
        }
    }
}

impl NotesView {
    fn run_document_action(
        &mut self,
        action: DocumentAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.more_open = false;
        match action {
            DocumentAction::Rename => self.modal(Modal::Rename, window, cx),
            DocumentAction::Duplicate => self.controller.duplicate_note(),
            DocumentAction::EditTags => self.modal(Modal::Tags, window, cx),
            DocumentAction::ToggleTrash => self
                .controller
                .manage_note(self.controller.active, folio_app::NoteAction::Trash),
            DocumentAction::DeletePage => self.controller.delete_page(),
            DocumentAction::DuplicatePage => self.controller.duplicate_page(),
            DocumentAction::BookmarkPage => self.modal(Modal::PageBookmark, window, cx),
            DocumentAction::MovePage => self.modal(Modal::MovePage, window, cx),
            DocumentAction::PageSize => self.modal(Modal::PageSize, window, cx),
            DocumentAction::ToggleInfinite => {
                let p = self.controller.page().properties.clone();
                self.controller.page_size(p.width, p.height, !p.infinite)
            }
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
            DocumentAction::InsertEquation => self.modal(Modal::Equation, window, cx),
            DocumentAction::MathSolver => self.start_math(window, cx),
            DocumentAction::UseAsCover => self.controller.use_page_as_cover(),
            DocumentAction::SaveTemplate => self.modal(Modal::SaveTemplate, window, cx),
            DocumentAction::AddFromTemplate => self.modal(Modal::Templates, window, cx),
            DocumentAction::IndexHandwriting => {
                if let Err(e) = self.controller.index_page_handwriting() {
                    self.controller.error = Some(e);
                }
            }
            DocumentAction::ClearHandwritingIndex => self.controller.clear_handwriting_index(),
            DocumentAction::NewDocumentOptions => self.modal(Modal::NewDocument, window, cx),
        }
    }
    /// Contents of the Document actions popover for the current section.
    pub(crate) fn document_actions_menu(
        &self,
        mut panel: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let section = self.more_section;
        panel = panel.child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(section.title()),
        );
        if section != MoreSection::Document {
            panel = panel.child(
                self.button(
                    "more-back",
                    "‹ Document actions",
                    false,
                    cx,
                    |this, _, _| this.more_section = MoreSection::Document,
                )
                .justify_start(),
            );
        }
        let trashed = self.controller.session().document.metadata.trashed;
        for action in DocumentAction::ALL {
            // Move to trash is listed last, apart from everyday actions.
            if action.section() != section || action == DocumentAction::ToggleTrash {
                continue;
            }
            panel = panel.child(
                self.button(
                    action.id(),
                    action.label(trashed),
                    false,
                    cx,
                    move |this, window, cx| this.run_document_action(action, window, cx),
                )
                .justify_start()
                .text_sm(),
            );
        }
        match section {
            MoreSection::Paper => {
                let mut paper = div().flex().gap_1();
                for (id, label, p) in [
                    ("blank", "Blank", Paper::Blank),
                    ("ruled", "Ruled", Paper::Ruled),
                    ("grid", "Grid", Paper::Grid),
                    ("dots", "Dots", Paper::Dots),
                ] {
                    paper = paper.child(
                        self.button(
                            id,
                            label,
                            self.controller.page().properties.paper == p,
                            cx,
                            move |this, _, _| this.controller.paper(p),
                        )
                        .text_xs()
                        .px_2(),
                    );
                }
                panel = panel.child(paper);
            }
            MoreSection::Document => {
                panel = panel.child(
                    self.control(
                        "export",
                        "Export document ›",
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(icon(Icon::Export, theme.muted))
                            .child("Export document ›")
                            .into_any_element(),
                        false,
                        cx,
                        |this, _, _| {
                            this.dismiss_popovers();
                            this.export_open = true;
                        },
                    )
                    .justify_start(),
                );
                panel = panel.child(
                    self.button(
                        "move-document-folder",
                        "Move to folder…",
                        false,
                        cx,
                        |this, w, cx| {
                            this.more_open = false;
                            this.modal(Modal::MoveDocument(this.controller.active), w, cx);
                        },
                    )
                    .justify_start(),
                );
                for (key, section) in [
                    ("page", MoreSection::Page),
                    ("paper", MoreSection::Paper),
                    ("math", MoreSection::Math),
                    ("layout", MoreSection::Layout),
                ] {
                    panel = panel.child(
                        self.button(
                            format!("more-section-{key}"),
                            format!("{} ›", section.title()),
                            false,
                            cx,
                            move |this, _, _| this.more_section = section,
                        )
                        .justify_start(),
                    );
                }
            }
            MoreSection::Page | MoreSection::Math | MoreSection::Layout => {}
        }
        if section == MoreSection::Document {
            let action = DocumentAction::ToggleTrash;
            panel = panel.child(div().h(px(1.)).mx_2().bg(theme.border)).child(
                self.button(
                    action.id(),
                    action.label(trashed),
                    false,
                    cx,
                    move |this, window, cx| this.run_document_action(action, window, cx),
                )
                .justify_start()
                .text_sm(),
            );
        }
        panel
    }
}
