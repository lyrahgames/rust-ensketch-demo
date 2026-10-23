use futures_channel::oneshot;
use std::{
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle, ThreadId},
};

type Job<S> = Box<dyn FnOnce(&mut S) + Send + 'static>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorError {
    Closed,
    Cancelled,
    Panicked,
    WrongThread,
}

pub struct Task<R>(oneshot::Receiver<std::result::Result<R, ExecutorError>>);
impl<R> Future for Task<R> {
    type Output = std::result::Result<R, ExecutorError>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::pin::Pin::new(&mut self.0)
            .poll(cx)
            .map(|r| r.unwrap_or(Err(ExecutorError::Cancelled)))
    }
}

pub struct Submitter<S> {
    sender: mpsc::Sender<Option<Job<S>>>,
    closed: Arc<AtomicBool>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl<S> Clone for Submitter<S> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            closed: self.closed.clone(),
            wake: self.wake.clone(),
        }
    }
}
impl<S: 'static> Submitter<S> {
    pub fn spawn(
        &self,
        task: impl FnOnce(&mut S) + Send + 'static,
    ) -> std::result::Result<(), ExecutorError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ExecutorError::Closed);
        }
        self.sender
            .send(Some(Box::new(task)))
            .map_err(|_| ExecutorError::Closed)?;
        if let Some(wake) = &self.wake {
            wake();
        }
        Ok(())
    }
    pub fn call<R: Send + 'static>(
        &self,
        task: impl FnOnce(&mut S) -> R + Send + 'static,
    ) -> std::result::Result<Task<R>, ExecutorError> {
        let (sender, receiver) = oneshot::channel();
        self.spawn(move |state| {
            let result =
                catch_unwind(AssertUnwindSafe(|| task(state))).map_err(|_| ExecutorError::Panicked);
            let _ = sender.send(result);
        })?;
        Ok(Task(receiver))
    }
    /// Native futures run on a helper thread; browser futures run on the
    /// browser's logical main executor. Neither borrows state across await.
    /// Submit mutations back through a cloned Submitter.
    pub fn call_async<R, F>(
        &self,
        operation: impl FnOnce() -> F + Send + 'static,
    ) -> std::result::Result<Task<R>, ExecutorError>
    where
        R: Send + 'static,
        F: Future<Output = R> + Send + 'static,
    {
        let (sender, receiver) = oneshot::channel();
        #[cfg(not(target_arch = "wasm32"))]
        self.spawn(move |_| {
            thread::spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| pollster::block_on(operation())))
                    .map_err(|_| ExecutorError::Panicked);
                let _ = sender.send(result);
            });
        })?;
        #[cfg(target_arch = "wasm32")]
        self.spawn(move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                let result = operation().await;
                let _ = sender.send(Ok(result));
            });
        })?;
        Ok(Task(receiver))
    }
}

/// A FIFO closure queue. Main executors are drained at host lifecycle points;
/// worker executors run continuously on one dedicated thread.
pub struct Executor<S> {
    submitter: Submitter<S>,
    receiver: Option<mpsc::Receiver<Option<Job<S>>>>,
    state: Option<S>,
    owner: Arc<Mutex<Option<ThreadId>>>,
    worker: Option<JoinHandle<()>>,
}
impl<S: 'static> Executor<S> {
    pub fn new_main(state: S) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            submitter: Submitter {
                sender,
                closed: Arc::new(AtomicBool::new(false)),
                wake: None,
            },
            receiver: Some(receiver),
            state: Some(state),
            owner: Arc::new(Mutex::new(Some(thread::current().id()))),
            worker: None,
        }
    }
    pub fn with_wakeup(mut self, wake: impl Fn() + Send + Sync + 'static) -> Self {
        self.submitter.wake = Some(Arc::new(wake));
        self
    }
    pub fn submitter(&self) -> Submitter<S> {
        self.submitter.clone()
    }
    pub fn spawn(
        &self,
        task: impl FnOnce(&mut S) + Send + 'static,
    ) -> std::result::Result<(), ExecutorError> {
        self.submitter.spawn(task)
    }
    pub fn call<R: Send + 'static>(
        &self,
        task: impl FnOnce(&mut S) -> R + Send + 'static,
    ) -> std::result::Result<Task<R>, ExecutorError> {
        self.submitter.call(task)
    }
    pub fn call_async<R, F>(
        &self,
        operation: impl FnOnce() -> F + Send + 'static,
    ) -> std::result::Result<Task<R>, ExecutorError>
    where
        R: Send + 'static,
        F: Future<Output = R> + Send + 'static,
    {
        self.submitter.call_async(operation)
    }
    pub fn with_state<R>(
        &mut self,
        f: impl FnOnce(&mut S) -> R,
    ) -> std::result::Result<R, ExecutorError> {
        if self.owner.lock().unwrap().as_ref() != Some(&thread::current().id()) {
            return Err(ExecutorError::WrongThread);
        }
        Ok(f(self.state.as_mut().ok_or(ExecutorError::Closed)?))
    }
    /// Runs a non-Send closure immediately on this executor's owning thread.
    pub fn spawn_local(
        &mut self,
        task: impl FnOnce(&mut S),
    ) -> std::result::Result<(), ExecutorError> {
        self.with_state(task)
    }
    /// Executes queued jobs in FIFO start order. A panicking fire-and-forget job
    /// is contained so later jobs still execute.
    pub fn drain(&mut self) -> std::result::Result<usize, ExecutorError> {
        if self.owner.lock().unwrap().as_ref() != Some(&thread::current().id()) {
            return Err(ExecutorError::WrongThread);
        }
        let mut count = 0;
        let receiver = self.receiver.as_ref().ok_or(ExecutorError::Closed)?;
        while let Ok(Some(job)) = receiver.try_recv() {
            if let Some(state) = self.state.as_mut() {
                let _ = catch_unwind(AssertUnwindSafe(|| job(state)));
            }
            count += 1;
        }
        Ok(count)
    }
    pub fn shutdown(&mut self) {
        self.submitter.closed.store(true, Ordering::Release);
        let _ = self.submitter.sender.send(None);
        self.receiver.take();
        self.state.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
    /// Constructs thread-affine, possibly non-Send state on the worker itself.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_worker_with(factory: impl FnOnce() -> S + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel::<Option<Job<S>>>();
        let owner = Arc::new(Mutex::new(None));
        let owner_thread = owner.clone();
        let worker = thread::spawn(move || {
            *owner_thread.lock().unwrap() = Some(thread::current().id());
            let mut state = factory();
            while let Ok(Some(job)) = receiver.recv() {
                let _ = catch_unwind(AssertUnwindSafe(|| job(&mut state)));
            }
        });
        Self {
            submitter: Submitter {
                sender,
                closed: Arc::new(AtomicBool::new(false)),
                wake: None,
            },
            receiver: None,
            state: None,
            owner,
            worker: Some(worker),
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl<S: Send + 'static> Executor<S> {
    pub fn new_worker(state: S) -> Self {
        Self::new_worker_with(move || state)
    }
}
impl<S> Drop for Executor<S> {
    fn drop(&mut self) {
        self.submitter.closed.store(true, Ordering::Release);
        let _ = self.submitter.sender.send(None);
        self.receiver.take();
        if let Some(worker) = self.worker.take()
            && worker.thread().id() != thread::current().id()
        {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fifo_and_result() {
        let mut e = Executor::new_main(Vec::new());
        e.spawn(|s| s.push(1)).unwrap();
        let t = e
            .call(|s| {
                s.push(2);
                s.clone()
            })
            .unwrap();
        e.drain().unwrap();
        assert_eq!(pollster::block_on(t).unwrap(), vec![1, 2]);
    }
    #[test]
    fn worker_affinity_and_panic() {
        let mut e = Executor::new_worker(0);
        let first = e
            .call(|s| {
                *s += 1;
                thread::current().id()
            })
            .unwrap();
        let second = e
            .call(|s| {
                *s += 1;
                (*s, thread::current().id())
            })
            .unwrap();
        let id = pollster::block_on(first).unwrap();
        assert_eq!(pollster::block_on(second).unwrap(), (2, id));
        let panic = e.call::<()>(|_| panic!("expected")).unwrap();
        assert_eq!(pollster::block_on(panic), Err(ExecutorError::Panicked));
        e.shutdown();
        assert!(matches!(e.spawn(|_| {}), Err(ExecutorError::Closed)));
    }
    #[test]
    fn async_operation_and_shutdown() {
        let mut e = Executor::new_main(());
        let task = e.call_async(|| async { 42 }).unwrap();
        e.drain().unwrap();
        assert_eq!(pollster::block_on(task), Ok(42));
        e.shutdown();
        assert!(matches!(
            e.call_async(|| async { 1 }),
            Err(ExecutorError::Closed)
        ));
    }
    #[test]
    fn async_start_obeys_queue_and_cancellation() {
        let mut e = Executor::new_main(0);
        let observed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let flag = observed.clone();
        e.spawn(move |_| flag.store(1, Ordering::SeqCst)).unwrap();
        let flag = observed.clone();
        let task = e
            .call_async(move || {
                let value = flag.load(Ordering::SeqCst);
                async move { value }
            })
            .unwrap();
        assert_eq!(observed.load(Ordering::SeqCst), 0);
        e.drain().unwrap();
        assert_eq!(pollster::block_on(task), Ok(1));

        let cancelled = e.call_async(|| async { 7 }).unwrap();
        e.shutdown();
        assert_eq!(pollster::block_on(cancelled), Err(ExecutorError::Cancelled));
    }
    #[test]
    fn cross_executor_and_completion() {
        let mut first = Executor::new_worker(());
        let mut second = Executor::new_worker(Vec::<u32>::new());
        let target = second.submitter();
        let completion = first
            .call(move |_| target.spawn(|state| state.push(7)))
            .unwrap();
        assert_eq!(pollster::block_on(completion), Ok(Ok(())));
        let read = second.call(|state| state.clone()).unwrap();
        assert_eq!(pollster::block_on(read), Ok(vec![7]));
        first.shutdown();
        second.shutdown();
    }
    #[test]
    fn local_non_send_closure() {
        let mut executor = Executor::new_main(0);
        let value = std::rc::Rc::new(std::cell::Cell::new(5));
        executor.spawn_local(|state| *state += value.get()).unwrap();
        assert_eq!(executor.with_state(|state| *state).unwrap(), 5);
    }
}
