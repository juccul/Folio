//! GPUI's task model on Rune: `cx.spawn` futures run on the UI thread, polled
//! before each frame with the view at hand, and `background_executor()`
//! futures run on their own threads. A finished background task wakes the
//! window so the futures waiting for it continue on the next frame.
use super::*;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex, OnceLock},
    task::{Poll, Wake},
};

type LocalFuture = Pin<Box<dyn Future<Output = ()>>>;

thread_local! {
    static LOCAL: RefCell<Vec<LocalFuture>> = const { RefCell::new(Vec::new()) };
    static SPAWNED: RefCell<Vec<LocalFuture>> = const { RefCell::new(Vec::new()) };
    static VIEW: Cell<*mut NotesView> = const { Cell::new(std::ptr::null_mut()) };
    /// The earliest deadline of a timer still waiting, found while polling.
    static DEADLINE: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
}
static WAKER: OnceLock<rune::Waker> = OnceLock::new();

/// The window's waker, for background work finishing.
pub fn set_waker(waker: rune::Waker) {
    let _ = WAKER.set(waker);
}
fn wake_window() {
    if let Some(waker) = WAKER.get() {
        waker();
    }
}

struct WindowWaker;
impl Wake for WindowWaker {
    fn wake(self: Arc<Self>) {
        wake_window();
    }
}

/// What polling the UI-thread futures came to.
pub struct Polled {
    /// Some future finished (and may have changed the view, or a future
    /// awaiting it can continue on the next poll).
    pub finished: bool,
    /// The earliest timer deadline a waiting future needs a poll at. Futures
    /// awaiting background work need none: finished work wakes the window.
    pub deadline: Option<std::time::Instant>,
}

/// Polls the UI-thread futures once, with `view` reachable through
/// [`WeakView::update`].
pub fn poll(view: &mut NotesView) -> Polled {
    VIEW.with(|v| v.set(view as *mut NotesView));
    DEADLINE.with(|d| d.set(None));
    let waker = std::task::Waker::from(Arc::new(WindowWaker));
    let mut context = std::task::Context::from_waker(&waker);
    let mut futures = LOCAL.with(|l| std::mem::take(&mut *l.borrow_mut()));
    futures.extend(SPAWNED.with(|s| std::mem::take(&mut *s.borrow_mut())));
    let before = futures.len();
    futures.retain_mut(|f| f.as_mut().poll(&mut context).is_pending());
    let finished = futures.len() < before;
    // Futures spawned while polling run from the next poll.
    LOCAL.with(|l| {
        let mut l = l.borrow_mut();
        futures.append(&mut l);
        *l = futures;
    });
    VIEW.with(|v| v.set(std::ptr::null_mut()));
    // Futures spawned during this poll have not run yet.
    let spawned = SPAWNED.with(|s| !s.borrow().is_empty());
    Polled {
        finished: finished || spawned,
        deadline: DEADLINE.with(Cell::get),
    }
}

/// A task's result, for awaiting; dropping or detaching it lets it finish.
pub struct Task<T>(Rc<RefCell<Option<T>>>);
impl<T> Task<T> {
    pub fn detach(self) {}
    pub fn ready(value: T) -> Self {
        Task(Rc::new(RefCell::new(Some(value))))
    }
}
impl<T> Future for Task<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, _: &mut std::task::Context<'_>) -> Poll<T> {
        match self.0.borrow_mut().take() {
            Some(v) => Poll::Ready(v),
            None => Poll::Pending,
        }
    }
}

/// The view, reachable from a spawned future while it is polled.
#[derive(Clone, Copy)]
pub struct WeakView;
impl WeakView {
    pub fn update<R>(
        &self,
        _: &mut AsyncApp,
        f: impl FnOnce(&mut NotesView, &mut Context<NotesView>) -> R,
    ) -> Result<R, ()> {
        let ptr = VIEW.with(Cell::get);
        if ptr.is_null() {
            return Err(());
        }
        // SAFETY: futures are polled only from `poll`, which holds the one
        // `&mut NotesView` for the duration and does not touch it meanwhile.
        let view = unsafe { &mut *ptr };
        let mut cx = Context::fresh();
        let result = f(view, &mut cx);
        view.queue_effects(cx.effects);
        Ok(result)
    }
    pub fn update_in<R>(
        &self,
        cx: &mut AsyncApp,
        f: impl FnOnce(&mut NotesView, &mut Window, &mut Context<NotesView>) -> R,
    ) -> Result<R, ()> {
        self.update(cx, |view, cx| {
            let mut window = Window::render(view);
            let result = f(view, &mut window, cx);
            view.queue_effects(window.effects);
            result
        })
    }
}

/// The context a spawned future gets.
pub struct AsyncApp;
impl AsyncApp {
    pub fn background_executor(&self) -> BackgroundExecutor {
        BackgroundExecutor
    }
    pub fn update<R>(&mut self, f: impl FnOnce(&mut Context<NotesView>) -> R) -> Result<R, ()> {
        let mut cx = Context::fresh();
        let result = f(&mut cx);
        let ptr = VIEW.with(Cell::get);
        if !ptr.is_null() {
            // SAFETY: as in `WeakView::update`.
            unsafe { (*ptr).queue_effects(cx.effects) };
        }
        Ok(result)
    }
}

/// Runs futures on threads of their own.
#[derive(Clone, Copy)]
pub struct BackgroundExecutor;
impl BackgroundExecutor {
    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> BackgroundTask<T> {
        let slot = Arc::new(Mutex::new(None));
        let result = slot.clone();
        let spawned = std::thread::Builder::new()
            .name("folio-ui-task".into())
            .spawn(move || {
                let value = futures::executor::block_on(future);
                if let Ok(mut slot) = result.lock() {
                    *slot = Some(value);
                }
                wake_window();
            });
        if spawned.is_err() {
            eprintln!("Folio could not start a background task");
        }
        BackgroundTask(slot)
    }
    /// Completes once `duration` has passed.
    pub fn timer(&self, duration: std::time::Duration) -> Timer {
        Timer(std::time::Instant::now() + duration)
    }
}
pub struct BackgroundTask<T>(Arc<Mutex<Option<T>>>);
impl<T> BackgroundTask<T> {
    pub fn detach(self) {}
}
impl<T> Future for BackgroundTask<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, _: &mut std::task::Context<'_>) -> Poll<T> {
        match self.0.lock().ok().and_then(|mut slot| slot.take()) {
            Some(v) => Poll::Ready(v),
            None => Poll::Pending,
        }
    }
}
pub struct Timer(std::time::Instant);
impl Future for Timer {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut std::task::Context<'_>) -> Poll<()> {
        if std::time::Instant::now() >= self.0 {
            Poll::Ready(())
        } else {
            DEADLINE.with(|d| d.set(Some(d.get().map_or(self.0, |e| e.min(self.0)))));
            Poll::Pending
        }
    }
}

impl<T> Context<T> {
    pub fn background_executor(&self) -> BackgroundExecutor {
        BackgroundExecutor
    }
    /// Runs `f` on the UI thread, polled before each frame.
    pub fn spawn<R: 'static>(
        &mut self,
        f: impl AsyncFnOnce(WeakView, &mut AsyncApp) -> R + 'static,
    ) -> Task<R> {
        let slot = Rc::new(RefCell::new(None));
        let result = slot.clone();
        let future: LocalFuture = Box::pin(async move {
            let mut cx = AsyncApp;
            let value = f(WeakView, &mut cx).await;
            *result.borrow_mut() = Some(value);
        });
        SPAWNED.with(|s| s.borrow_mut().push(future));
        Task(slot)
    }
    pub fn spawn_in<R: 'static>(
        &mut self,
        _: &Window,
        f: impl AsyncFnOnce(WeakView, &mut AsyncApp) -> R + 'static,
    ) -> Task<R> {
        self.spawn(f)
    }
    pub fn drop_image<I>(&mut self, _: I, _: Option<&Window>) {}
    pub fn entity(&self) -> WeakView {
        WeakView
    }
}
impl WeakView {
    pub fn downgrade(self) -> Self {
        self
    }
}
