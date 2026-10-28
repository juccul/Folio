//! Rendering and editing LaTeX equation objects.
use super::*;

impl Controller {
    pub fn cancel_equation_render(&mut self) {
        self.equation_generation += 1;
        self.equation_pending = false;
        self.equation_result = None;
        self.equation_live_wait = None;
    }
    fn begin_equation_render(&mut self) {
        self.cancel_equation_render();
        self.equation_pending = true;
    }
    fn submit_equation(&mut self, job: Job) {
        match self.workers.submit(job) {
            Ok(()) => self.busy += 1,
            Err(error) => {
                self.equation_pending = false;
                self.equation_result = Some(Err(error));
            }
        }
    }
    pub fn edit_equation(&mut self, id: Id, latex: String) {
        if self.read_only() {
            self.equation_result = Some(Err("Restore this document before editing".into()));
            return;
        }
        self.begin_equation_render();
        if self
            .page()
            .objects
            .get(&id)
            .is_some_and(|o| matches!(o.as_ref(), Object::Equation(e) if e.math_link.is_some()))
        {
            self.equation_live_wait = Some((id, latex.clone()));
            if let Err(error) = self.update_math_expression(id, latex) {
                self.equation_pending = false;
                self.equation_result = Some(Err(error));
            }
            return;
        }
        if let Some(object) = self.page().objects.get(&id).cloned()
            && let Object::Equation(e) = object.as_ref()
        {
            self.submit_equation(Job::Equation {
                generation: self.equation_generation,
                note: self.active,
                page: self.page().id,
                existing: Some(object.clone()),
                latex,
                rect: e.rect,
            });
        } else {
            self.equation_pending = false;
            self.equation_result = Some(Err("This equation is no longer available".into()));
        }
    }
    pub fn insert_equation(&mut self, latex: String) {
        if self.read_only() {
            self.equation_result = Some(Err("Restore this document before editing".into()));
            return;
        }
        self.begin_equation_render();
        let position = self.cursor.unwrap_or(Point::new(100., 100.));
        self.submit_equation(Job::Equation {
            generation: self.equation_generation,
            existing: None,
            note: self.active,
            page: self.page().id,
            latex,
            rect: Rect::new(position.x, position.y, 240., 70.),
        });
    }
    /// Commit a rendered equation unless the target changed while rendering.
    pub(crate) fn equation_rendered(
        &mut self,
        generation: u64,
        note: Id,
        page: Id,
        before: Option<Arc<Object>>,
        after: Object,
    ) {
        if generation != self.equation_generation {
            return;
        }
        self.equation_pending = false;
        self.equation_result = Some(Err("The destination page is no longer available".into()));
        let Some(p) = self.sessions.get(&note).and_then(|s| s.document.page(page)) else {
            return;
        };
        let id = after.id();
        if p.objects.get(&id).cloned() != before {
            self.equation_result =
                Some(Err("Equation changed while rendering; edit it again".into()));
            return;
        }
        let index = p
            .order
            .iter()
            .position(|v| *v == id)
            .unwrap_or(p.order.len());
        self.commit_to(
            note,
            "Edit equation",
            vec![Change::Object {
                page,
                id,
                before,
                after: Some(Arc::new(after)),
                index,
            }],
        );
        self.equation_result = Some(Ok(()));
    }
}
