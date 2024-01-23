use core::{
    cell::{Cell, UnsafeCell},
    future::Future,
    pin::Pin,
    ptr::NonNull,
    task::{Context, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;

use crate::linked_list::{LinkedList, LinkedListItem, LinkedListLinks};

unsafe fn waker_clone(context: *const ()) -> RawWaker {
    RawWaker::new(context, &RAW_WAKER_VTABLE)
}

unsafe fn waker_wake(context: *const ()) {
    let task = &*(context as *const TaskCore);
    critical_section::with(|cs| task.insert_back(cs));
}

unsafe fn waker_wake_by_ref(context: *const ()) {
    let task = &*(context as *const TaskCore);
    critical_section::with(|cs| task.insert_back(cs));
}

unsafe fn waker_drop(_context: *const ()) {}

static RAW_WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(waker_clone, waker_wake, waker_wake_by_ref, waker_drop);

trait TaskHandle {
    fn poll_task(&self, cx: &mut Context) -> core::task::Poll<()>;
}

struct TaskCore {
    runtime: NonNull<Runtime>,
    task_handle: Mutex<Cell<Option<NonNull<dyn TaskHandle>>>>,
    links: LinkedListLinks<Self>,
}

impl TaskCore {
    fn run_once(&self) {
        if let Some(mut task_handle) =
            critical_section::with(|cs| self.task_handle.borrow(cs).take())
        {
            let data = self as *const Self as *const ();
            let waker = unsafe { Waker::from_raw(RawWaker::new(data, &RAW_WAKER_VTABLE)) };
            let mut cx = Context::from_waker(&waker);

            if unsafe { task_handle.as_mut() }
                .poll_task(&mut cx)
                .is_pending()
            {
                critical_section::with(|cs| self.task_handle.borrow(cs).set(Some(task_handle)));
            }
        }
    }
}

impl LinkedListItem for TaskCore {
    fn links(&self) -> &LinkedListLinks<Self> {
        &self.links
    }

    fn list(&self) -> &LinkedList<Self> {
        unsafe { &self.runtime.as_ref().tasks }
    }
}

/// A joinable handle for a task.
///
/// The task is aborted if the handle is dropped.
pub struct JoinHandle<'a, T> {
    task_core: &'a TaskCore,
    result: &'a Mutex<Cell<Option<T>>>,
}

impl<'a, T> JoinHandle<'a, T> {
    /// Drive the runtime until the handle's task completes.
    ///
    /// Returns the value returned by the future
    pub fn join(self) -> T {
        while critical_section::with(|cs| {
            let borrow = self.task_core.task_handle.borrow(cs);
            let v = borrow.take();
            let has_some = v.is_some();
            borrow.set(v);
            has_some
        }) {
            unsafe { self.task_core.runtime.as_ref().run_once() };
        }

        critical_section::with(|cs| self.result.borrow(cs).take().expect("No Result"))
    }
}

impl<'a, T> Drop for JoinHandle<'a, T> {
    fn drop(&mut self) {
        critical_section::with(|cs| self.task_core.remove(cs));
    }
}

struct CapturingFuture<F: Future> {
    future: UnsafeCell<F>,
    result: Mutex<Cell<Option<F::Output>>>,
}

impl<F: Future> TaskHandle for CapturingFuture<F> {
    fn poll_task(&self, cx: &mut Context<'_>) -> core::task::Poll<()> {
        unsafe { Pin::new_unchecked(&mut *self.future.get()) }
            .poll(cx)
            .map(|output| critical_section::with(|cs| self.result.borrow(cs).set(Some(output))))
    }
}

/// An asyncronous task
pub struct Task<F: Future> {
    core: Option<TaskCore>,
    future: CapturingFuture<F>,
}

impl<'a, F> Task<F>
where
    F: Future + 'a,
    F::Output: 'a,
{
    /// Create a new task from a future
    pub fn new(future: F) -> Self {
        Self {
            core: None,
            future: CapturingFuture {
                future: UnsafeCell::new(future),
                result: Mutex::new(Cell::new(None)),
            },
        }
    }
}

impl<F: Future> core::ops::Drop for Task<F> {
    fn drop(&mut self) {
        critical_section::with(|cs| {
            if let Some(core) = self.core.take() {
                core.remove(cs);
            }
        })
    }
}

/// The asyncronous runtime.
///
/// Note that it is **not threadsafe** and should thus only be run from a single thread.
#[derive(Default)]
pub struct Runtime {
    tasks: LinkedList<TaskCore>,
}

impl Runtime {
    // Create a new runtime
    pub fn new() -> Self {
        Self::default()
    }

    unsafe fn run_once(&self) {
        if let Some(first_task) = critical_section::with(|cs| self.tasks.pop_first(cs)) {
            first_task.run_once();
        } else {
            #[cfg(all(feature = "cortex_m", not(feature = "wfe")))]
            cortex_m::asm::wfi();
            #[cfg(all(feature = "cortex_m", feature = "wfe"))]
            cortex_m::asm::wfe();
        }
    }

    /// Spawn the task into the given runtime.
    /// Note that the task will not be run until a join handle is joined.
    pub fn spawn<'a, F: Future>(&'a self, task: &'a mut Task<F>) -> JoinHandle<'a, F::Output> {
        if task.core.is_some() {
            panic!("Task already spawned");
        }

        let future = unsafe {
            Mutex::new(Cell::new(Some(core::ptr::NonNull::from(
                core::mem::transmute::<&mut (dyn TaskHandle + 'a), &mut dyn TaskHandle>(
                    &mut task.future,
                ),
            ))))
        };

        let task_core = {
            let task_core = task.core.get_or_insert(TaskCore {
                task_handle: future,
                runtime: NonNull::from(self),
                links: LinkedListLinks::default(),
            });

            critical_section::with(move |cs| task_core.insert_back(cs))
        };

        JoinHandle {
            task_core,
            result: &task.future.result,
        }
    }
}
