//! Native solver: a stable problem editor, focused workflows and readable results.
//! No mathematical computation runs on the UI thread.
use super::*;
use folio_app::{MathReport, MathRequest, MathStep, VectorCommand, VectorFormula};
use std::{cell::RefCell, collections::HashMap, sync::Arc};
type FormulaCache = HashMap<usize, (Arc<VectorFormula>, Arc<Vec<gpui::Path<Pixels>>>)>;
#[derive(Clone, PartialEq)]
struct GraphKey {
    svg: String,
    palette: super::graph::Palette,
}
struct GraphPreview {
    key: GraphKey,
    image: Option<Arc<RenderImage>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Solution,
    Graph,
    Check,
}

const OPERATIONS: [(&str, &str); 8] = [
    ("auto", "Solve automatically"),
    ("evaluate", "Evaluate"),
    ("simplify", "Simplify"),
    ("factor", "Factor"),
    ("differentiate", "Differentiate"),
    ("integrate", "Integrate"),
    ("assign", "Define variable"),
    ("interpret", "Translate word problem"),
];

pub(super) struct Inputs {
    pub(super) expression: Entity<Field>,
    pub(super) variable: Entity<Field>,
    pub(super) next_line: Entity<Field>,
    pub(super) range: Entity<Field>,
    pub(super) result_latex: Entity<Field>,
    pub(super) latex_open: bool,
    applying_latex: bool,
    return_focus: bool,
    result_source: String,
    result_observed: String,
    formula_cache: RefCell<FormulaCache>,
    graph_preview: Option<GraphPreview>,
    graph_pending: Option<GraphKey>,
    mode: Mode,
    operation: String,
    actions_open: bool,
    settings_open: bool,
    source_open: bool,
    insert_open: bool,
    methods_open: bool,
    pub(super) guided: bool,
    feedback: Option<String>,
    scroll: ScrollHandle,
    guide_anchor: ScrollAnchor,
    revision: u64,
    _subscriptions: Vec<Subscription>,
}
impl Inputs {
    pub(super) fn visible_fields(&self) -> [bool; 4] {
        [
            !self.latex_open,
            self.settings_open && !self.latex_open,
            self.mode == Mode::Check && !self.latex_open,
            self.mode == Mode::Graph && !self.latex_open,
        ]
    }
    pub(super) fn result_dirty(&self, cx: &App) -> bool {
        self.result_latex.read(cx).content != self.result_source
    }
    pub(super) fn can_run(&self, cx: &App) -> bool {
        !self.expression.read(cx).content.trim().is_empty()
            && (self.mode != Mode::Check || !self.next_line.read(cx).content.trim().is_empty())
    }
}

fn operation_name(operation: &str) -> &'static str {
    match operation {
        "evaluate" => "Arithmetic",
        "simplify" => "Simplify",
        "factor" => "Factor",
        "differentiate" => "Derivative",
        "integrate" => "Integral",
        "assign" => "Variable",
        "interpret" => "Word problem",
        _ => "Automatic",
    }
}

// Route prose to the reviewed template interpreter; symbolic names and LaTeX
// still go through the parser. Explicit operations always take precedence.
fn automatic_operation(expression: &str) -> &'static str {
    if expression.contains(":=") {
        return "assign";
    }
    let words = expression
        .split_whitespace()
        .filter(|word| word.len() > 1 && word.chars().all(char::is_alphabetic))
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let percentage = (words.iter().any(|word| word == "of")
        && (expression.contains('%') || words.iter().any(|word| word == "percent")))
        || (words
            .iter()
            .any(|word| matches!(word.as_str(), "increase" | "decrease"))
            && words.iter().any(|word| word == "by"));
    let prose = percentage
        || (words.len() >= 3
            && words.iter().any(|word| {
                matches!(
                    word.as_str(),
                    "is" | "of" | "the" | "plus" | "minus" | "travels"
                )
            })
            && !expression
                .chars()
                .any(|c| matches!(c, '+' | '-' | '*' | '/' | '=' | '^' | '\\')));
    if prose { "interpret" } else { "auto" }
}

impl NotesView {
    pub(super) fn start_math(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.controller.open_math_solver() {
            Ok(()) => {
                self.math_inputs = None;
                self.sync_math_inputs(window, cx);
            }
            Err(error) => self.controller.error = Some(error),
        }
        if let Some(inputs) = &self.math_inputs
            && inputs.expression.read(cx).content.is_empty()
            && !self.controller.recognition_pending
        {
            inputs.expression.read(cx).focus.focus(window);
        } else {
            self.focus.focus(window);
        }
        cx.notify();
    }
    pub(super) fn sync_math_inputs(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(session) = &self.controller.math_session else {
            self.math_inputs = None;
            return;
        };
        if let Some(inputs) = &mut self.math_inputs {
            let source = session
                .report
                .as_ref()
                .map(|r| r.answer_latex.as_str())
                .unwrap_or("");
            if source != inputs.result_source {
                inputs.result_source = source.into();
                inputs.result_observed = source.into();
                inputs
                    .result_latex
                    .update(cx, |field, cx| field.set_content(source.into(), cx));
            }
            if inputs.applying_latex && !session.pending {
                if session.error.is_none()
                    && session
                        .report
                        .as_ref()
                        .is_some_and(|r| r.status == "edited")
                {
                    inputs.latex_open = false;
                    inputs.return_focus = true;
                    inputs.feedback =
                        Some("Answer updated. Your original problem is unchanged.".into());
                    inputs.scroll.set_offset(point(px(0.), px(0.)));
                }
                inputs.applying_latex = false;
            }
            if session.report.is_none() {
                inputs.latex_open = false;
            }
            if inputs.revision != session.revision {
                // Do not reset the cursor just because a calculation finished.
                if inputs.expression.read(cx).content != session.request.expression {
                    inputs.expression.update(cx, |field, cx| {
                        field.set_content(session.request.expression.clone(), cx)
                    });
                }
                if inputs.next_line.read(cx).content != session.request.next_line {
                    inputs.next_line.update(cx, |field, cx| {
                        field.set_content(session.request.next_line.clone(), cx)
                    });
                }
                inputs.revision = session.revision;
            }
            return;
        }
        let theme = Theme::new(&self.controller.settings);
        let request = session.request.clone();
        let revision = session.revision;
        let result_source = session
            .report
            .as_ref()
            .map(|r| r.answer_latex.clone())
            .unwrap_or_default();
        let mode = match request.operation.as_str() {
            "graph" => Mode::Graph,
            "check" => Mode::Check,
            _ => Mode::Solution,
        };
        let operation = if OPERATIONS.iter().any(|(op, _)| *op == request.operation) {
            request.operation.clone()
        } else {
            "auto".into()
        };
        let make = |content: String, multiline, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let height = (content.lines().count().clamp(1, 4) * 26) as f32;
                let mut field = Field::new(content, multiline, cx);
                field.theme = theme;
                field.height = multiline.then_some(height);
                field
            })
        };
        let expression = make(request.expression, true, cx);
        let variable = make(request.variable, false, cx);
        let next_line = make(request.next_line, true, cx);
        let range = make(format!("{}, {}", request.x_min, request.x_max), false, cx);
        let result_latex = make(result_source.clone(), true, cx);
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.observe(&result_latex, |this, entity, cx| {
            let content = entity.read(cx).content.clone();
            let changed = this.math_inputs.as_mut().is_some_and(|inputs| {
                let changed = inputs.result_observed != content;
                inputs.result_observed = content.clone();
                changed
            });
            let field = entity.read(cx);
            let min_lines = if this.math_inputs.as_ref().is_some_and(|i| i.latex_open) {
                3
            } else {
                1
            };
            let height = (field.content.lines().count().clamp(min_lines, 6) * 26) as f32;
            if field.height != Some(height) {
                entity.update(cx, |field, cx| {
                    field.height = Some(height);
                    cx.notify();
                });
            }
            if changed {
                if this
                    .controller
                    .math_session
                    .as_ref()
                    .is_some_and(|s| s.pending)
                {
                    this.controller.cancel_math_solver();
                }
                if let Some(session) = &mut this.controller.math_session {
                    session.error = None;
                }
                if let Some(inputs) = &mut this.math_inputs {
                    inputs.feedback = None;
                    inputs.applying_latex = false;
                }
            }
            cx.notify();
        }));
        subscriptions.push(
            cx.subscribe(&result_latex, |_, _, _: &FieldBoundsChanged, cx| {
                cx.notify()
            }),
        );
        subscriptions.push(cx.subscribe(&result_latex, |this, _, _: &Submitted, cx| {
            this.apply_math_latex(cx)
        }));
        for (kind, entity) in [&expression, &variable, &next_line, &range]
            .into_iter()
            .enumerate()
        {
            subscriptions.push(cx.observe(entity, move |this, entity, cx| {
                let content = entity.read(cx).content.clone();
                let height = (content.lines().count().clamp(1, 4) * 26) as f32;
                if entity.read(cx).multiline && entity.read(cx).height != Some(height) {
                    entity.update(cx, |field, cx| {
                        field.height = Some(height);
                        cx.notify();
                    });
                }
                let differs =
                    this.controller
                        .math_session
                        .as_ref()
                        .is_some_and(|session| match kind {
                            0 => session.request.expression != content,
                            1 => session.request.variable != content.trim(),
                            2 => session.request.next_line != content,
                            _ => {
                                session.request.operation == "graph"
                                    && parse_range(&content).ok()
                                        != Some((session.request.x_min, session.request.x_max))
                            }
                        });
                if differs {
                    this.controller.cancel_math_solver();
                    if let Some(session) = &mut this.controller.math_session {
                        session.report = None;
                        session.error = None;
                        session.request.method.clear();
                        match kind {
                            0 => session.request.expression = content,
                            1 => session.request.variable = content.trim().into(),
                            2 => session.request.next_line = content,
                            _ => {}
                        }
                    }
                    if let Some(inputs) = &mut this.math_inputs {
                        inputs.feedback = None;
                        inputs.guided = false;
                    }
                }
                cx.notify();
            }));
            subscriptions.push(cx.subscribe_in(
                entity,
                window,
                |this, _, _: &Submitted, window, cx| this.run_primary_math(window, cx),
            ));
            subscriptions
                .push(cx.subscribe(entity, |_, _, _: &FieldBoundsChanged, cx| cx.notify()));
        }
        let scroll = ScrollHandle::new();
        let guide_anchor = ScrollAnchor::for_handle(scroll.clone());
        self.math_inputs = Some(Inputs {
            expression,
            variable,
            next_line,
            range,
            result_latex,
            result_observed: result_source.clone(),
            result_source,
            latex_open: false,
            applying_latex: false,
            return_focus: false,
            formula_cache: RefCell::new(HashMap::new()),
            graph_preview: None,
            graph_pending: None,
            mode,
            operation,
            actions_open: false,
            settings_open: false,
            source_open: false,
            insert_open: false,
            methods_open: false,
            guided: false,
            feedback: None,
            scroll,
            guide_anchor,
            revision,
            _subscriptions: subscriptions,
        });
    }
    fn math_request(&self, operation: &str, cx: &Context<Self>) -> Result<MathRequest, String> {
        let mut request = self
            .controller
            .math_session
            .as_ref()
            .ok_or("Open Solve first")?
            .request
            .clone();
        request.operation = operation.into();
        if let Some(inputs) = &self.math_inputs {
            request.expression = inputs.expression.read(cx).content.clone();
            request.variable = inputs.variable.read(cx).content.trim().to_string();
            request.next_line = inputs.next_line.read(cx).content.clone();
            if operation == "graph" {
                (request.x_min, request.x_max) = parse_range(&inputs.range.read(cx).content)?;
            }
        }
        Ok(request)
    }
    fn run_primary_math(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(inputs) = &self.math_inputs else {
            return;
        };
        if !inputs.can_run(cx)
            || self.controller.recognition_pending
            || self
                .controller
                .math_session
                .as_ref()
                .is_some_and(|s| s.pending)
        {
            return;
        }
        let operation = match inputs.mode {
            Mode::Graph => "graph".into(),
            Mode::Check => "check".into(),
            Mode::Solution if inputs.operation == "auto" => {
                automatic_operation(&inputs.expression.read(cx).content).into()
            }
            Mode::Solution => inputs.operation.clone(),
        };
        self.run_math(&operation, window, cx);
    }
    fn run_math(&mut self, operation: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(inputs) = &mut self.math_inputs {
            inputs.feedback = None;
            inputs.guided = false;
            inputs.actions_open = false;
            inputs.insert_open = false;
            inputs.scroll.set_offset(point(px(0.), px(0.)));
        }
        let result = self
            .math_request(operation, cx)
            .and_then(|request| self.controller.run_math_solver(request));
        if let Err(error) = result
            && let Some(session) = &mut self.controller.math_session
        {
            session.error = Some(error);
        }
        self.focus.focus(window);
        cx.notify();
    }
    fn math_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .math_inputs
            .as_ref()
            .is_some_and(|inputs| inputs.mode == mode)
        {
            return;
        }
        self.controller.cancel_math_solver();
        if let Some(session) = &mut self.controller.math_session {
            session.report = None;
            session.error = None;
            session.request.method.clear();
        }
        if let Some(inputs) = &mut self.math_inputs {
            inputs.mode = mode;
            inputs.actions_open = false;
            inputs.insert_open = false;
            inputs.feedback = None;
            inputs.guided = false;
            inputs.scroll.set_offset(point(px(0.), px(0.)));
            if mode == Mode::Check {
                inputs.next_line.read(cx).focus.focus(window);
            }
        }
        cx.notify();
    }
    fn math_toggle(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        active: bool,
        slot: fn(&mut Inputs) -> &mut bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.button(id, label, active, cx, move |this, _, cx| {
            if let Some(inputs) = &mut this.math_inputs {
                let value = slot(inputs);
                *value = !*value;
                if *value {
                    inputs.scroll.set_offset(point(px(0.), px(0.)));
                }
            }
            cx.notify();
        })
    }
    fn math_step_element(&mut self, step: MathStep, index: String, details: bool) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut row = div()
            .flex()
            .flex_col()
            .gap_2()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_3()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme.accent))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(index.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(step.explanation),
                    ),
            );
        if let Some(vector) = step.vector {
            row = row.child(
                div()
                    .w_full()
                    .py_2()
                    .child(self.native_formula(vector, 40., theme.ink)),
            );
        } else {
            row = row.child(div().text_sm().child(step.after));
        }
        if step.status != "verified" {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(step.status),
            );
        }
        if details {
            if !step.details.is_empty() {
                row = row.child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.muted))
                        .child(step.details),
                );
            }
            for (i, child) in step.children.into_iter().enumerate() {
                row =
                    row.child(self.math_step_element(child, format!("{index}.{}", i + 1), details));
            }
        }
        row
    }
    fn math_settings(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let session = self.controller.math_session.as_ref().unwrap();
        let mut settings = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .bg(rgb(theme.selected))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Calculation settings"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().text_sm().child("Target variable"))
                    .child(
                        div()
                            .flex_1()
                            .child(self.math_inputs.as_ref().unwrap().variable.clone()),
                    ),
            );
        for (label, choices, current, is_domain) in [
            (
                "Numbers",
                [("Real", "real"), ("Complex", "complex")],
                session.request.domain.clone(),
                true,
            ),
            (
                "Angles",
                [("Radians", "radians"), ("Degrees", "degrees")],
                session.request.angle.clone(),
                false,
            ),
        ] {
            let mut row = div().flex().items_center().gap_1().child(
                div()
                    .w(px(72.))
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(label),
            );
            for (label, value) in choices {
                row = row.child(self.button(
                    format!("math-{value}"),
                    label,
                    current == value,
                    cx,
                    move |this, _, cx| {
                        this.controller.cancel_math_solver();
                        if let Some(session) = &mut this.controller.math_session {
                            if is_domain {
                                session.request.domain = value.into();
                            } else {
                                session.request.angle = value.into();
                            }
                            session.report = None;
                            session.error = None;
                        }
                        cx.notify();
                    },
                ));
            }
            settings = settings.child(row);
        }
        if let Ok(variables) = self.controller.math_variables()
            && !variables.is_empty()
        {
            settings = settings.child(div().text_xs().text_color(rgb(theme.muted)).child(format!(
                    "Page variables: {}",
                    variables
                        .into_iter()
                        .map(|(name, value)| format!("{name} = {value}"))
                        .collect::<Vec<_>>()
                        .join(" · ")
                )));
        }
        settings
    }
    fn apply_math_latex(&mut self, cx: &mut Context<Self>) {
        let Some(inputs) = &self.math_inputs else {
            return;
        };
        if !inputs.result_dirty(cx)
            || self
                .controller
                .math_session
                .as_ref()
                .is_some_and(|s| s.pending)
        {
            return;
        }
        let latex = inputs.result_latex.read(cx).content.clone();
        match self.controller.edit_math_result(latex) {
            Ok(()) => {
                if let Some(inputs) = &mut self.math_inputs {
                    inputs.applying_latex = true;
                }
            }
            Err(error) => {
                if let Some(session) = &mut self.controller.math_session {
                    session.error = Some(error);
                }
            }
        }
        cx.notify();
    }
    fn begin_math_latex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(inputs) = &mut self.math_inputs {
            inputs.latex_open = true;
            inputs.applying_latex = false;
            inputs.insert_open = false;
            inputs.actions_open = false;
            inputs.settings_open = false;
            inputs.source_open = false;
            inputs.methods_open = false;
            inputs.feedback = None;
            inputs.scroll.set_offset(point(px(0.), px(0.)));
            inputs.result_latex.update(cx, |field, cx| {
                field.height = Some((field.content.lines().count().clamp(3, 6) * 26) as f32);
                cx.notify();
            });
            inputs.result_latex.read(cx).focus.focus(window);
        }
        cx.notify();
    }
    fn cancel_math_latex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.controller.cancel_math_solver();
        if let Some(inputs) = &mut self.math_inputs {
            inputs.applying_latex = false;
            inputs.latex_open = false;
            let source = inputs.result_source.clone();
            inputs.result_observed = source.clone();
            inputs
                .result_latex
                .update(cx, |field, cx| field.set_content(source, cx));
            inputs.scroll.set_offset(point(px(0.), px(0.)));
        }
        if let Some(session) = &mut self.controller.math_session {
            session.error = None;
        }
        self.focus.focus(window);
        cx.notify();
    }
    fn navigate_math_guide(&mut self, direction: i8, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.controller.math_session {
            let count = session.report.as_ref().map_or(0, |r| r.steps.len());
            session.revealed = guide_position(session.revealed, count, direction);
            session.hint = false;
        }
        if let Some(inputs) = &mut self.math_inputs {
            inputs.guide_anchor.scroll_to(window, cx);
            window.on_next_frame(|window, _| window.refresh());
        }
        cx.notify();
    }
    fn use_math_example(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(inputs) = &mut self.math_inputs else {
            return;
        };
        let variable = inputs.variable.read(cx).content.clone();
        let (expression, next) = match inputs.mode {
            Mode::Graph => (format!("y={variable}^2"), String::new()),
            Mode::Check => (format!("2{variable}+3=11"), format!("2{variable}=8")),
            Mode::Solution => (format!("2{variable}+3=11"), String::new()),
        };
        inputs.operation = "auto".into();
        // Update the request before notifying field observers so the example's
        // calculation cannot be cancelled by its own field changes.
        if let Some(session) = &mut self.controller.math_session {
            session.request.expression = expression.clone();
            session.request.next_line = next.clone();
            session.request.method.clear();
            if inputs.mode == Mode::Graph {
                session.request.x_min = -10.;
                session.request.x_max = 10.;
                inputs
                    .range
                    .update(cx, |field, cx| field.set_content("-10, 10".into(), cx));
            }
        }
        inputs
            .expression
            .update(cx, |field, cx| field.set_content(expression, cx));
        inputs
            .next_line
            .update(cx, |field, cx| field.set_content(next, cx));
        self.run_primary_math(window, cx);
    }
    fn native_formula(
        &self,
        formula: Arc<VectorFormula>,
        height: f32,
        color: u32,
    ) -> impl IntoElement {
        let mut cache = self
            .math_inputs
            .as_ref()
            .unwrap()
            .formula_cache
            .borrow_mut();
        if cache.len() >= 128 {
            cache.clear();
        }
        let key = Arc::as_ptr(&formula) as usize;
        let paths = cache
            .entry(key)
            .or_insert_with(|| {
                let paths = formula
                    .paths
                    .iter()
                    .filter_map(|outline| {
                        let rule = if outline.even_odd {
                            FillRule::EvenOdd
                        } else {
                            FillRule::NonZero
                        };
                        let mut path = PathBuilder::fill().with_style(PathStyle::Fill(
                            FillOptions::default()
                                .with_fill_rule(rule)
                                .with_tolerance(0.01),
                        ));
                        let p = |[x, y]: [f32; 2]| point(px(x), px(y));
                        for command in &outline.commands {
                            match *command {
                                VectorCommand::Move(a) => path.move_to(p(a)),
                                VectorCommand::Line(a) => path.line_to(p(a)),
                                VectorCommand::Quad(a, b) => path.curve_to(p(b), p(a)),
                                VectorCommand::Cubic(a, b, c) => {
                                    path.cubic_bezier_to(p(c), p(a), p(b))
                                }
                                VectorCommand::Close => path.close(),
                            }
                        }
                        path.build().ok()
                    })
                    .collect();
                (formula.clone(), Arc::new(paths))
            })
            .1
            .clone();
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let scale = (f32::from(bounds.size.width) / formula.width)
                    .min(f32::from(bounds.size.height) / formula.height);
                let x = f32::from(bounds.origin.x)
                    + (f32::from(bounds.size.width) - formula.width * scale) / 2.;
                let y = f32::from(bounds.origin.y)
                    + (f32::from(bounds.size.height) - formula.height * scale) / 2.;
                for path in paths.iter() {
                    window.paint_path(
                        path.clone().transformed([scale, 0., 0., scale, x, y]),
                        rgb(color),
                    );
                }
            },
        )
        .w_full()
        .h(px(height))
        .flex_shrink_0()
    }
    fn graph_preview(&mut self, svg: &str, cx: &mut Context<Self>) -> Div {
        let palette = Theme::new(&self.controller.settings)
            .graph(self.controller.page().properties.pdf.is_some());
        let key = GraphKey {
            svg: svg.into(),
            palette,
        };
        let inputs = self.math_inputs.as_mut().unwrap();
        let cached = inputs
            .graph_preview
            .as_ref()
            .filter(|preview| preview.key == key);
        let mut element = div().w_full().rounded_md().bg(rgb(palette.paper)).p_2();
        if let Some(cached) = cached {
            return if let Some(image) = &cached.image {
                element.child(
                    img(image.clone())
                        .w_full()
                        .h(px(210.))
                        .max_h(px(210.))
                        .min_h_0()
                        .object_fit(ObjectFit::Contain),
                )
            } else {
                element
                    .text_color(rgb(palette.labels))
                    .child("Graph preview unavailable")
            };
        }
        element = element.h(px(226.));
        if inputs.graph_pending.is_none() {
            inputs.graph_pending = Some(key.clone());
            let task_key = key.clone();
            let task = cx.background_executor().spawn(async move {
                folio_export::raster_svg(&task_key.palette.svg(&task_key.svg), 2.)
                    .ok()
                    .map(|pixmap| {
                        super::graph::image(pixmap.width(), pixmap.height(), pixmap.take())
                    })
            });
            cx.spawn(async move |view, cx| {
                let image = task.await;
                let _ = view.update(cx, |view, cx| {
                    if let Some(inputs) = &mut view.math_inputs
                        && inputs.graph_pending.as_ref() == Some(&key)
                    {
                        inputs.graph_pending = None;
                        inputs.graph_preview = Some(GraphPreview { key, image });
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        element
    }
    fn math_report_element(&mut self, report: &MathReport, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let session = self.controller.math_session.as_ref().unwrap();
        let (revealed, details, hint) = (session.revealed, session.show_details, session.hint);
        let inputs = self.math_inputs.as_ref().unwrap();
        let (guided, methods_open, mode) = (inputs.guided, inputs.methods_open, inputs.mode);
        let mut result = div().flex().flex_col().gap_3().child(
            div().font_weight(FontWeight::SEMIBOLD).child(
                if report.interpretation.is_some() {
                    "Review the equation"
                } else if mode == Mode::Check {
                    "Step feedback"
                } else {
                    report.title.as_str()
                }
                .to_string(),
            ),
        );
        if inputs.latex_open {
            let source = inputs.result_latex.clone();
            let dirty = inputs.result_dirty(cx);
            let pending = session.pending;
            let error = session.error.clone();
            let mut editor = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.muted))
                        .child("Edit the formula source, then apply your changes."),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .child("LaTeX source"),
                )
                .child(source);
            if let Some(error) = error {
                editor = editor.child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.destructive))
                        .child(error),
                );
            }
            editor = editor.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(if pending {
                        "Updating the answer…"
                    } else if dirty {
                        "Current answer · apply to update"
                    } else {
                        "Current answer"
                    }),
            );
            if let Some(vector) = &report.vector {
                editor = editor.child(div().w_full().py_3().child(self.native_formula(
                    vector.clone(),
                    52.,
                    theme.ink,
                )));
            }
            return editor.child(div().text_xs().text_color(rgb(theme.muted)).child("Edited answers are saved as static formulas. Solve the problem again to verify the answer."));
        }
        if let Some(vector) = &report.vector {
            result = result.child(div().w_full().py_2().child(self.native_formula(
                vector.clone(),
                42.,
                theme.ink,
            )));
        } else if let Some(graph) = &report.graph {
            result = result.child(self.graph_preview(&graph.svg, cx));
        } else {
            result = result.child(div().text_sm().child(report.answer.clone()));
        }
        if report.vector.is_some() && report.interpretation.is_none() {
            result = result.child(
                self.button(
                    "math-edit-latex",
                    "Edit LaTeX",
                    false,
                    cx,
                    |this, window, cx| this.begin_math_latex(window, cx),
                )
                .justify_start()
                .py_1(),
            );
        }
        if !report.approximate.is_empty() {
            result = result.child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.muted))
                    .child(format!("≈ {}", report.approximate)),
            );
        }
        if !report.message.is_empty() {
            result = result.child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.muted))
                    .child(report.message.clone()),
            );
        }
        if !report.restrictions.is_empty() {
            result = result.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(format!("Valid when: {}", report.restrictions.join(", "))),
            );
        }
        if let Some(interpretation) = report.interpretation.clone() {
            return result
                .child(
                    div()
                        .text_sm()
                        .child("Check that this matches your problem, then solve it."),
                )
                .child(
                    self.button(
                        "math-use-interpretation",
                        "Solve this equation",
                        true,
                        cx,
                        move |this, window, cx| {
                            if let Some(inputs) = &this.math_inputs {
                                inputs.expression.update(cx, |field, cx| {
                                    field.set_content(interpretation.clone(), cx)
                                });
                            }
                            if let Some(session) = &mut this.controller.math_session {
                                session.request.expression = interpretation.clone();
                                session.request.method.clear();
                            }
                            if let Some(inputs) = &mut this.math_inputs {
                                inputs.operation = "auto".into();
                            }
                            // Observers see the same expression, so cannot cancel this new request.
                            this.run_math("auto", window, cx);
                        },
                    )
                    .bg(rgb(theme.accent))
                    .text_color(rgb(theme.primary_foreground)),
                );
        }
        if report.steps.is_empty() || mode != Mode::Solution {
            return result;
        }
        let count = report.steps.len();
        let mut heading = div().flex().items_center().justify_between().gap_2().child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(format!("Steps · {count}")),
        );
        heading = heading.child(self.button(
            "math-guide",
            if guided { "Show all steps" } else { "Guide me" },
            guided,
            cx,
            |this, window, cx| {
                if let Some(inputs) = &mut this.math_inputs {
                    inputs.guided = !inputs.guided;
                    if let Some(session) = &mut this.controller.math_session {
                        session.revealed = if inputs.guided { 1 } else { usize::MAX };
                        session.hint = false;
                    }
                    if inputs.guided {
                        inputs.guide_anchor.scroll_to(window, cx);
                        window.on_next_frame(|window, _| window.refresh());
                    } else {
                        inputs.scroll.set_offset(point(px(0.), px(0.)));
                    }
                }
                cx.notify();
            },
        ));
        result = result.child(
            div()
                .id("math-steps-heading")
                .mt_2()
                .anchor_scroll(Some(
                    self.math_inputs.as_ref().unwrap().guide_anchor.clone(),
                ))
                .child(heading),
        );
        if report.methods.len() > 1 {
            result = result.child(self.math_toggle(
                "math-methods",
                "Change method",
                methods_open,
                |i| &mut i.methods_open,
                cx,
            ));
            if methods_open {
                let current = self
                    .controller
                    .math_session
                    .as_ref()
                    .unwrap()
                    .request
                    .method
                    .clone();
                let mut methods = div().flex().flex_wrap().gap_1();
                for method in report.methods.clone() {
                    methods = methods.child(self.button(
                        format!("math-method-{method}"),
                        method.replace('_', " "),
                        current == method,
                        cx,
                        move |this, window, cx| {
                            if let Some(session) = &mut this.controller.math_session {
                                session.request.method = method.clone();
                            }
                            let operation = this
                                .controller
                                .math_session
                                .as_ref()
                                .unwrap()
                                .request
                                .operation
                                .clone();
                            this.run_math(&operation, window, cx);
                        },
                    ));
                }
                result = result.child(methods);
            }
        }
        if guided {
            result = result.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(format!("Step {} of {count}", revealed.min(count))),
            );
            if hint && let Some(next) = report.steps.get(revealed) {
                result = result.child(
                    div()
                        .p_3()
                        .rounded_md()
                        .bg(rgb(theme.selected))
                        .text_sm()
                        .child(format!("Next step hint: {}", next.explanation)),
                );
            }
            if let Some(step) = report.steps.get(revealed.saturating_sub(1)).cloned() {
                result = result.child(self.math_step_element(step, revealed.to_string(), details));
            }
            if revealed >= count {
                result = result.child(div().text_sm().text_color(rgb(theme.muted)).child(
                    "You’ve reached the answer. Show all steps to review the complete solution.",
                ));
            }
        } else {
            for (index, step) in report.steps.clone().into_iter().enumerate() {
                result =
                    result.child(self.math_step_element(step, (index + 1).to_string(), details));
            }
        }
        if revealed > 0 {
            result = result.child(self.button(
                "math-details",
                if details {
                    "Hide explanations"
                } else {
                    "Explain steps"
                },
                details,
                cx,
                |this, _, cx| {
                    if let Some(session) = &mut this.controller.math_session {
                        session.show_details = !session.show_details;
                    }
                    cx.notify();
                },
            ));
        }
        result
    }
    fn insert_math(&mut self, worked: bool, live: bool, cx: &mut Context<Self>) {
        let operation = self
            .controller
            .math_session
            .as_ref()
            .map(|s| s.request.operation.as_str())
            .unwrap_or("auto");
        let result = self.math_request(operation, cx).and_then(|request| {
            let session = self
                .controller
                .math_session
                .as_ref()
                .ok_or("No math result")?;
            if request.expression != session.request.expression
                || request.x_min != session.request.x_min
                || request.x_max != session.request.x_max
            {
                return Err("The input changed; solve again before inserting".into());
            }
            self.controller.insert_math_result(worked, live)
        });
        match result {
            Ok(()) => {
                if let Some(inputs) = &mut self.math_inputs {
                    inputs.insert_open = false;
                    inputs.feedback = Some(
                        if worked {
                            "Solution and steps added to the page."
                        } else if live {
                            "Live result added. It updates with your writing or variables."
                        } else {
                            "Result added to the page."
                        }
                        .into(),
                    );
                }
            }
            Err(error) => {
                if let Some(session) = &mut self.controller.math_session {
                    session.error = Some(error);
                }
            }
        }
        cx.notify();
    }
    fn math_result_actions(&self, report: &MathReport, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let inputs = self.math_inputs.as_ref().unwrap();
        let insert_open = inputs.insert_open;
        let can_insert = report.rendered_svg.is_some()
            && !matches!(report.status.as_str(), "incorrect" | "unknown" | "review");
        let is_check = self
            .controller
            .math_session
            .as_ref()
            .unwrap()
            .request
            .operation
            == "check";
        let mut footer = div()
            .flex_shrink_0()
            .p_3()
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap_2();
        if inputs.latex_open {
            return footer
                .child(
                    self.button(
                        "math-apply-latex",
                        if self
                            .controller
                            .math_session
                            .as_ref()
                            .is_some_and(|s| s.pending)
                        {
                            "Applying…"
                        } else {
                            "Apply changes"
                        },
                        true,
                        cx,
                        |this, _, cx| this.apply_math_latex(cx),
                    )
                    .w_full()
                    .bg(rgb(theme.accent))
                    .text_color(rgb(theme.primary_foreground)),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            self.button(
                                "math-copy-latex",
                                "Copy LaTeX",
                                false,
                                cx,
                                |this, _, cx| {
                                    if let Some(inputs) = &this.math_inputs {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            inputs.result_latex.read(cx).content.clone(),
                                        ));
                                    }
                                    if let Some(inputs) = &mut this.math_inputs {
                                        inputs.feedback = Some("Copied to clipboard.".into());
                                    }
                                    cx.notify();
                                },
                            )
                            .flex_1(),
                        )
                        .child(
                            self.button(
                                "math-revert-latex",
                                "Cancel editing",
                                false,
                                cx,
                                |this, window, cx| this.cancel_math_latex(window, cx),
                            )
                            .flex_1(),
                        ),
                )
                .when(inputs.feedback.is_some(), |footer| {
                    footer.child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme.muted))
                            .child(inputs.feedback.clone().unwrap_or_default()),
                    )
                });
        }
        if inputs.guided && !report.steps.is_empty() {
            footer = footer.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        self.button(
                            "math-previous",
                            "Previous step",
                            false,
                            cx,
                            |this, window, cx| this.navigate_math_guide(-1, window, cx),
                        )
                        .flex_1()
                        .px_2(),
                    )
                    .child(
                        self.button("math-next", "Next step", true, cx, |this, window, cx| {
                            this.navigate_math_guide(1, window, cx)
                        })
                        .flex_1()
                        .px_2(),
                    )
                    .child(
                        self.button(
                            "math-hint",
                            "Hint",
                            self.controller
                                .math_session
                                .as_ref()
                                .is_some_and(|s| s.hint),
                            cx,
                            |this, _, cx| {
                                if let Some(session) = &mut this.controller.math_session {
                                    session.hint = !session.hint;
                                }
                                cx.notify();
                            },
                        )
                        .px_2(),
                    ),
            );
        }
        if can_insert && insert_open && report.status != "edited" {
            let mut options = div()
                .flex()
                .flex_col()
                .gap_1()
                .bg(rgb(theme.selected))
                .rounded_md()
                .p_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .px_2()
                        .py_1()
                        .child("Add to your note"),
                );
            if !report.steps.is_empty() {
                options = options.child(
                    self.button(
                        "insert-math-worked",
                        "Answer and steps",
                        false,
                        cx,
                        |this, _, cx| this.insert_math(true, false, cx),
                    )
                    .justify_start(),
                );
            }
            if !is_check && report.assignment.is_none() {
                options = options
                    .child(
                        self.button(
                            "insert-math-live",
                            if report.graph.is_some() {
                                "Live graph"
                            } else {
                                "Live answer"
                            },
                            false,
                            cx,
                            |this, _, cx| this.insert_math(false, true, cx),
                        )
                        .justify_start(),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme.muted))
                            .px_2()
                            .child("Live results update with your writing or page variables."),
                    );
            }
            footer = footer.child(options);
        }
        let mut actions = div().flex().gap_2().items_center().child(
            self.button(
                "copy-math-result",
                if report.answer_latex.is_empty() {
                    "Copy result"
                } else {
                    "Copy LaTeX"
                },
                false,
                cx,
                |this, _, cx| {
                    if let Some(report) = this
                        .controller
                        .math_session
                        .as_ref()
                        .and_then(|s| s.report.as_ref())
                    {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            if report.answer_latex.is_empty() {
                                report.answer.clone()
                            } else {
                                report.answer_latex.clone()
                            },
                        ));
                        if let Some(inputs) = &mut this.math_inputs {
                            inputs.feedback = Some("Copied to clipboard.".into());
                        }
                    }
                    cx.notify();
                },
            )
            .flex_1(),
        );
        if can_insert {
            actions = actions
                .child(
                    self.button(
                        "insert-math-result",
                        if report.graph.is_some() {
                            "Add graph"
                        } else if report.assignment.is_some() {
                            "Add variable"
                        } else {
                            "Add answer"
                        },
                        true,
                        cx,
                        |this, _, cx| this.insert_math(false, false, cx),
                    )
                    .flex_1()
                    .bg(rgb(theme.accent))
                    .text_color(rgb(theme.primary_foreground)),
                )
                .when(
                    report.status != "edited"
                        && (!report.steps.is_empty() || (!is_check && report.assignment.is_none())),
                    |actions| {
                        actions.child(
                            self.control(
                                "math-insert-options",
                                "Add options",
                                icon(Icon::Down, theme.ink).into_any_element(),
                                insert_open,
                                cx,
                                |this, _, cx| {
                                    if let Some(inputs) = &mut this.math_inputs {
                                        inputs.insert_open = !inputs.insert_open;
                                    }
                                    cx.notify();
                                },
                            )
                            .px_2(),
                        )
                    },
                );
        }
        footer = footer.child(actions);
        if let Some(feedback) = &inputs.feedback {
            footer = footer.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(feedback.clone()),
            );
        }
        footer
    }
    pub(super) fn math_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.sync_math_inputs(window, cx);
        if self
            .math_inputs
            .as_mut()
            .is_some_and(|inputs| std::mem::take(&mut inputs.return_focus))
        {
            self.focus.focus(window);
        }
        let theme = Theme::new(&self.controller.settings);
        let viewport_width = f32::from(window.viewport_size().width);
        let width = (viewport_width * 0.34)
            .clamp(320., 420.)
            .max(320. * self.controller.settings.ui_scale)
            .min(viewport_width * 0.55);
        let mut panel = div()
            .id("math-solver-panel")
            .occlude()
            .w(px(width))
            .flex_shrink_0()
            .min_h_0()
            .border_l_1()
            .border_color(theme.border)
            .bg(rgb(theme.surface))
            .flex()
            .flex_col();
        let Some(inputs) = &self.math_inputs else {
            return panel;
        };
        let scroll = inputs.scroll.clone();
        let (expression, next_line, range) = (
            inputs.expression.clone(),
            inputs.next_line.clone(),
            inputs.range.clone(),
        );
        let (mode, operation, actions_open, settings_open, source_open) = (
            inputs.mode,
            inputs.operation.clone(),
            inputs.actions_open,
            inputs.settings_open,
            inputs.source_open,
        );
        let empty = expression.read(cx).content.trim().is_empty();
        let editing = inputs.latex_open;
        let session = self.controller.math_session.as_ref().unwrap();
        let (pending, report, error) = (
            session.pending,
            session.report.clone(),
            session.error.clone(),
        );
        let reading = self.controller.recognition_pending;
        let can_run = !pending && !reading && inputs.can_run(cx);
        let header = div()
            .flex_shrink_0()
            .px_4()
            .py_2()
            .flex()
            .items_center()
            .justify_between()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(if editing {
                "Edit answer"
            } else {
                "Math solver"
            }))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .when(!editing, |controls| {
                        controls.child(
                            self.math_toggle(
                                "math-settings",
                                "Settings",
                                settings_open,
                                |i| &mut i.settings_open,
                                cx,
                            )
                            .py_1(),
                        )
                    })
                    .child(
                        self.button(
                            "close-math-solver",
                            "Close",
                            false,
                            cx,
                            |this, window, cx| {
                                this.controller.close_math_solver();
                                this.math_inputs = None;
                                this.focus.focus(window);
                                cx.notify();
                            },
                        )
                        .py_1(),
                    ),
            );
        panel = panel.child(header);
        if editing && let Some(report) = &report {
            let editor = self.math_report_element(report, cx);
            return panel
                .child(
                    div()
                        .id("math-solver-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&scroll)
                        .child(div().p_4().child(editor)),
                )
                .child(self.math_result_actions(report, cx));
        }
        let mut tabs = div().flex().px_4().gap_1().pb_2();
        for (id, label, value) in [
            ("math-solution-tab", "Solution", Mode::Solution),
            ("math-graph-tab", "Graph", Mode::Graph),
            ("math-check-tab", "Check work", Mode::Check),
        ] {
            tabs = tabs.child(
                self.button(id, label, mode == value, cx, move |this, window, cx| {
                    this.math_mode(value, window, cx)
                })
                .flex_1()
                .py_1(),
            );
        }
        panel = panel.child(tabs);
        let problem_label = match mode {
            Mode::Graph => "Function",
            Mode::Check => "Original problem",
            Mode::Solution => "Problem",
        };
        let mut editor = div()
            .flex_shrink_0()
            .px_4()
            .pb_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .child(problem_label),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                self.button(
                                    "math-use-selection",
                                    "Use selection",
                                    false,
                                    cx,
                                    |this, _, cx| {
                                        if let Err(error) = this
                                            .controller
                                            .read_math_selection(RecognitionKind::Math, false)
                                            && let Some(session) = &mut this.controller.math_session
                                        {
                                            session.error = Some(error);
                                        }
                                        cx.notify();
                                    },
                                )
                                .py_1(),
                            )
                            .child(
                                self.control(
                                    "math-source",
                                    "Read options",
                                    icon(Icon::Down, theme.ink).into_any_element(),
                                    source_open,
                                    cx,
                                    |this, _, cx| {
                                        if let Some(inputs) = &mut this.math_inputs {
                                            inputs.source_open = !inputs.source_open;
                                        }
                                        cx.notify();
                                    },
                                )
                                .py_1()
                                .px_2(),
                            ),
                    ),
            )
            .child(div().w_full().flex_shrink_0().child(expression));
        if source_open {
            let mut sources = div()
                .flex()
                .flex_wrap()
                .gap_1()
                .child(self.button(
                    "math-read-selection",
                    "Read selected math",
                    false,
                    cx,
                    |this, _, cx| {
                        if let Err(error) = this
                            .controller
                            .read_math_selection(RecognitionKind::Math, false)
                            && let Some(session) = &mut this.controller.math_session
                        {
                            session.error = Some(error);
                        }
                        cx.notify();
                    },
                ))
                .child(self.button(
                    "math-read-text",
                    "Read selected text",
                    false,
                    cx,
                    |this, _, cx| {
                        if let Err(error) = this
                            .controller
                            .read_math_selection(RecognitionKind::Text, false)
                            && let Some(session) = &mut this.controller.math_session
                        {
                            session.error = Some(error);
                        }
                        cx.notify();
                    },
                ));
            if self.controller.page().properties.pdf.is_some() {
                sources = sources.child(self.button(
                    "math-pdf-region",
                    "Read PDF region",
                    false,
                    cx,
                    |this, window, cx| this.modal(Modal::MathPdfRegion, window, cx),
                ));
            }
            editor = editor.child(sources);
        }
        match mode {
            Mode::Solution => {}
            Mode::Graph => {
                editor = editor.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(div().text_xs().child("x range"))
                        .child(div().flex_1().child(range)),
                );
            }
            Mode::Check => {
                editor = editor
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Your next step"),
                            )
                            .child(
                                self.button(
                                    "math-read-next",
                                    "Use handwritten step",
                                    false,
                                    cx,
                                    |this, _, cx| {
                                        if let Err(error) = this
                                            .controller
                                            .read_math_selection(RecognitionKind::Math, true)
                                            && let Some(session) = &mut this.controller.math_session
                                        {
                                            session.error = Some(error);
                                        }
                                        cx.notify();
                                    },
                                )
                                .py_1(),
                            ),
                    )
                    .child(div().w_full().flex_shrink_0().child(next_line));
            }
        }
        let primary_label = match mode {
            Mode::Graph => "Plot graph",
            Mode::Check => "Check next step",
            Mode::Solution => match operation.as_str() {
                "auto" => "Solve problem",
                "evaluate" => "Calculate",
                "simplify" => "Simplify expression",
                "factor" => "Factor expression",
                "differentiate" => "Find derivative",
                "integrate" => "Find integral",
                "assign" => "Preview variable",
                "interpret" => "Translate problem",
                _ => "Solve problem",
            },
        };
        if pending || reading {
            editor = editor.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(if reading {
                        self.controller.recognition_status.clone()
                    } else {
                        "Solving…".to_string()
                    }))
                    .child(
                        self.button("cancel-math", "Cancel", false, cx, |this, _, cx| {
                            this.controller.cancel_math_solver();
                            cx.notify();
                        }),
                    ),
            );
        } else {
            let primary = self
                .button(
                    "math-primary",
                    primary_label,
                    true,
                    cx,
                    |this, window, cx| this.run_primary_math(window, cx),
                )
                .font_weight(FontWeight::SEMIBOLD)
                .bg(rgb(theme.accent))
                .text_color(rgb(theme.primary_foreground))
                .opacity(if can_run { 1. } else { 0.4 });
            if mode == Mode::Solution {
                editor = editor.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(
                            self.control(
                                "math-more-actions",
                                format!("Operation: {}", operation_name(&operation)),
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_1()
                                    .child(operation_name(&operation))
                                    .child(icon(Icon::Down, theme.ink))
                                    .into_any_element(),
                                actions_open,
                                cx,
                                |this, _, cx| {
                                    if let Some(inputs) = &mut this.math_inputs {
                                        inputs.actions_open = !inputs.actions_open;
                                        inputs.settings_open = false;
                                        inputs.scroll.set_offset(point(px(0.), px(0.)));
                                    }
                                    cx.notify();
                                },
                            )
                            .flex_1()
                            .min_w_0()
                            .px_2(),
                        )
                        .child(primary.flex_1().min_w_0().px_2()),
                );
            } else {
                editor = editor.child(primary.w_full());
            }
        }
        panel = panel.child(editor);
        let mut body = div()
            .w_full()
            .flex_shrink_0()
            .p_4()
            .flex()
            .flex_col()
            .gap_4();
        if actions_open && mode == Mode::Solution {
            let mut choices = div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_md()
                .bg(rgb(theme.selected))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .px_2()
                        .py_1()
                        .child("Choose an operation"),
                );
            for (value, label) in OPERATIONS {
                choices = choices.child(
                    self.button(
                        format!("math-action-{value}"),
                        label,
                        operation == value,
                        cx,
                        move |this, _, cx| {
                            this.controller.cancel_math_solver();
                            if let Some(session) = &mut this.controller.math_session {
                                session.report = None;
                                session.error = None;
                                session.request.method.clear();
                            }
                            if let Some(inputs) = &mut this.math_inputs {
                                inputs.operation = value.into();
                                inputs.actions_open = false;
                                inputs.feedback = None;
                            }
                            cx.notify();
                        },
                    )
                    .justify_start(),
                );
            }
            body = body.child(choices);
        }
        if settings_open {
            body = body.child(self.math_settings(cx));
        }
        if let Some(error) = error {
            body = body.child(
                div()
                    .rounded_md()
                    .p_3()
                    .bg(rgb(theme.selected))
                    .text_sm()
                    .text_color(rgb(theme.destructive))
                    .child(error),
            );
        }
        if let Some(report) = &report {
            body = body.child(self.math_report_element(report, cx));
        } else if !pending && !reading && !actions_open && !settings_open {
            let (title, help, example) = match mode {
                Mode::Solution => (
                    "Start with a problem",
                    "Type an expression or use a selection from your page.",
                    "For example: 2x + 3 = 11",
                ),
                Mode::Graph => (
                    "See the function",
                    "Enter a function of your target variable. Adjust the range above.",
                    "For example: y = x^2",
                ),
                Mode::Check => (
                    "Check your next step",
                    "Enter the original problem and your next line. We’ll check whether they are equivalent.",
                    "For example: 2x + 3 = 11 → 2x = 8",
                ),
            };
            if empty {
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .py_3()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
                        .child(div().text_sm().text_color(rgb(theme.muted)).child(help))
                        .child(div().text_xs().text_color(rgb(theme.muted)).child(example))
                        .child(
                            self.button(
                                "math-example",
                                "Try example",
                                false,
                                cx,
                                |this, window, cx| this.use_math_example(window, cx),
                            )
                            .justify_start(),
                        ),
                );
            }
            if !empty {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child("Ctrl+Enter to run. Your original writing stays on the page."),
                );
            }
        }
        panel = panel.child(
            div()
                .id("math-solver-scroll")
                .flex_1()
                .min_h_0()
                .border_t_1()
                .border_color(theme.border)
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .child(body),
        );
        if let Some(report) = &report
            && report.interpretation.is_none()
        {
            panel = panel.child(self.math_result_actions(report, cx));
        }
        panel
    }
}

fn guide_position(revealed: usize, count: usize, direction: i8) -> usize {
    if count == 0 {
        return 0;
    }
    let current = revealed.clamp(1, count);
    if direction < 0 {
        current.saturating_sub(1).max(1)
    } else {
        current.saturating_add(1).min(count)
    }
}

fn parse_range(content: &str) -> Result<(f64, f64), String> {
    let values = content
        .split(',')
        .map(str::trim)
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Use two numbers for the x range, for example -10, 10.")?;
    if values.len() != 2
        || !values[0].is_finite()
        || !values[1].is_finite()
        || values[0] >= values[1]
    {
        return Err("The x range needs two finite numbers, with the smaller one first.".into());
    }
    Ok((values[0], values[1]))
}

#[cfg(test)]
mod tests {
    use super::{automatic_operation, guide_position};

    #[test]
    fn guide_navigation_starts_at_one_and_never_leaves_the_solution() {
        assert_eq!(guide_position(0, 4, -1), 1);
        assert_eq!(guide_position(1, 4, -1), 1);
        assert_eq!(guide_position(1, 4, 1), 2);
        assert_eq!(guide_position(2, 4, -1), 1);
        assert_eq!(guide_position(4, 4, 1), 4);
        assert_eq!(guide_position(usize::MAX, 4, -1), 3);
        assert_eq!(guide_position(1, 1, 1), 1);
        assert_eq!(guide_position(usize::MAX, 0, -1), 0);
    }

    #[test]
    fn automatic_solving_keeps_symbolic_names_and_reviews_supported_prose() {
        for (problem, expected) in [
            ("alpha + beta + gamma", "auto"),
            ("percent + 1", "auto"),
            (r"\sin(x) + \cos(x)", "auto"),
            ("20 percent of 50", "interpret"),
            ("20% of 50", "interpret"),
            ("increase 100 by 10%", "interpret"),
            ("twice a number plus 3 is 11", "interpret"),
            ("a := 5", "assign"),
        ] {
            assert_eq!(automatic_operation(problem), expected, "{problem}");
        }
    }
}
