use processcraft_core::{ids::ElementSelector, model::BpmnDocument};
use processcraft_render::{Appearance, BpmnViewport, HitRegion, TextEngine, Viewport};
use std::sync::{Arc, Condvar, Mutex, mpsc};

pub struct RenderJob {
    pub key: String,
    pub document: Arc<BpmnDocument>,
    pub diagram: ElementSelector,
    pub appearance: Appearance,
    pub view: Viewport,
}
pub struct RenderResult {
    pub key: String,
    pub result: Result<(Vec<u8>, usize, usize, Vec<HitRegion>), String>,
}
pub struct Renderer {
    pending: Arc<(Mutex<Option<RenderJob>>, Condvar)>,
    results: mpsc::Receiver<RenderResult>,
}
impl Renderer {
    pub fn new() -> Self {
        let pending: Arc<(Mutex<Option<RenderJob>>, Condvar)> =
            Arc::new((Mutex::new(None), Condvar::new()));
        let worker = pending.clone();
        let (tx, results) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut fonts = TextEngine::new();
            loop {
                let job = {
                    let (lock, ready) = &*worker;
                    let mut slot = lock.lock().unwrap();
                    while slot.is_none() {
                        slot = ready.wait(slot).unwrap();
                    }
                    slot.take().unwrap()
                };
                let result = BpmnViewport::build(
                    &job.document,
                    &job.diagram,
                    job.appearance,
                    &mut fonts,
                    "Noto Sans",
                    job.view,
                )
                .map(|v| {
                    (
                        v.pixels.data().to_vec(),
                        v.pixels.width() as usize,
                        v.pixels.height() as usize,
                        v.hits,
                    )
                })
                .map_err(|e| e.to_string());
                let _ = tx.send(RenderResult {
                    key: job.key,
                    result,
                });
            }
        });
        Self { pending, results }
    }
    pub fn submit(&self, job: RenderJob) {
        let (lock, ready) = &*self.pending;
        *lock.lock().unwrap() = Some(job);
        ready.notify_one();
    }
    pub fn poll(&self) -> Option<RenderResult> {
        self.results.try_recv().ok()
    }
}

pub fn is_current(result_key: &str, current_key: &str) -> bool {
    result_key == current_key
}

#[cfg(test)]
mod lifecycle {
    #[test]
    fn stale_render_is_discarded() {
        assert!(!super::is_current("session-a:1:light", "session-a:2:light"));
        assert!(!super::is_current("session-a:2:light", "session-b:2:light"));
        assert!(super::is_current("session-b:2:dark", "session-b:2:dark"));
    }
}

fn enqueue_latest<T>(slot: &(Mutex<Option<T>>, Condvar), value: T) {
    let (lock, ready) = slot;
    *lock.lock().unwrap() = Some(value);
    ready.notify_one();
}
fn publish_recovery_result(
    result: Result<(), String>,
    tx: &mpsc::SyncSender<String>,
    wake: impl FnOnce(),
) {
    if let Err(error) = result {
        let _ = tx.try_send(error);
    }
    wake();
}
pub struct RecoveryJob {
    pub project: Arc<processcraft_core::model::ProjectData>,
    pub origin: processcraft_io::recovery::RecoveryOrigin,
    pub generation: u64,
    pub context: egui::Context,
}
pub struct RecoveryWriter {
    pending: Arc<(Mutex<Option<RecoveryJob>>, Condvar)>,
    errors: mpsc::Receiver<String>,
}
impl RecoveryWriter {
    pub fn new() -> Self {
        let pending: Arc<(Mutex<Option<RecoveryJob>>, Condvar)> =
            Arc::new((Mutex::new(None), Condvar::new()));
        let worker = pending.clone();
        let (tx, errors) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            loop {
                let job = {
                    let (lock, ready) = &*worker;
                    let mut slot = lock.lock().unwrap();
                    while slot.is_none() {
                        slot = ready.wait(slot).unwrap();
                    }
                    slot.take().unwrap()
                };
                let result =
                    processcraft_io::recovery::RecoveryStore::new(crate::jobs::recovery_location())
                        .and_then(|store| store.write(&job.project, job.origin, job.generation));
                publish_recovery_result(result.map(|_| ()).map_err(|e| e.to_string()), &tx, || {
                    job.context.request_repaint()
                });
            }
        });
        Self { pending, errors }
    }
    pub fn submit(&self, job: RecoveryJob) {
        enqueue_latest(&self.pending, job);
    }
    pub fn error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }
}

#[cfg(test)]
mod lifecycle_recovery_queue {
    #[test]
    fn recovery_queue_coalesces() {
        use std::sync::{Arc, Barrier, Condvar, Mutex};
        let slot = Arc::new((Mutex::new(None), Condvar::new()));
        let barrier = Arc::new(Barrier::new(2));
        let worker_slot = slot.clone();
        let worker_barrier = barrier.clone();
        let worker = std::thread::spawn(move || {
            worker_barrier.wait();
            worker_slot.0.lock().unwrap().take()
        });
        super::enqueue_latest(&slot, 1);
        super::enqueue_latest(&slot, 2);
        barrier.wait();
        assert_eq!(worker.join().unwrap(), Some(2));
    }
}

#[cfg(test)]
mod recovery_wake_tests {
    #[test]
    fn failure_is_published_before_waking_the_ui() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        super::publish_recovery_result(Err("failure".into()), &tx, || {
            assert_eq!(rx.try_recv().unwrap(), "failure")
        });
    }
}

/// Model validation off the UI thread; only the latest submitted snapshot is checked.
pub struct DiagnosticsWorker {
    pending: Arc<(Mutex<Option<DiagnosticsJob>>, Condvar)>,
    results: mpsc::Receiver<((String, u64), Vec<processcraft_core::Diagnostic>)>,
}
struct DiagnosticsJob {
    key: (String, u64),
    project: Arc<processcraft_core::model::ProjectData>,
    context: egui::Context,
}
impl DiagnosticsWorker {
    pub fn new() -> Self {
        let pending: Arc<(Mutex<Option<DiagnosticsJob>>, Condvar)> =
            Arc::new((Mutex::new(None), Condvar::new()));
        let worker = pending.clone();
        let (tx, results) = mpsc::channel();
        std::thread::spawn(move || {
            loop {
                let job = {
                    let (lock, ready) = &*worker;
                    let mut slot = lock.lock().unwrap();
                    while slot.is_none() {
                        slot = ready.wait(slot).unwrap();
                    }
                    slot.take().unwrap()
                };
                if tx.send((job.key, job.project.diagnostics())).is_err() {
                    return;
                }
                job.context.request_repaint();
            }
        });
        Self { pending, results }
    }
    pub fn submit(
        &self,
        key: (String, u64),
        project: Arc<processcraft_core::model::ProjectData>,
        context: egui::Context,
    ) {
        enqueue_latest(
            &self.pending,
            DiagnosticsJob {
                key,
                project,
                context,
            },
        );
    }
    /// Latest finished result, if any.
    pub fn poll(&self) -> Option<((String, u64), Vec<processcraft_core::Diagnostic>)> {
        self.results.try_iter().last()
    }
}

#[cfg(test)]
mod diagnostics_worker_tests {
    #[test]
    fn worker_returns_the_diagnostics_of_the_snapshot_including_conventions() {
        use processcraft_core::conventions::ConventionsProfile;
        let mut project = crate::controller::blank_project();
        project.conventions = Some(ConventionsProfile::recommended());
        let expected = project.diagnostics();
        assert!(expected.iter().any(|d| d.code.starts_with("convention")));
        let worker = super::DiagnosticsWorker::new();
        worker.submit(
            ("s".into(), 1),
            std::sync::Arc::new(project),
            egui::Context::default(),
        );
        let started = std::time::Instant::now();
        let (key, found) = loop {
            if let Some(result) = worker.poll() {
                break result;
            }
            assert!(started.elapsed().as_secs() < 10, "no worker result");
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(key, ("s".to_owned(), 1));
        assert_eq!(found, expected);
    }
}

/// Unit-test UI frames use a private recovery folder, never the desktop user's recovery store.
pub(crate) fn recovery_location() -> std::path::PathBuf {
    #[cfg(test)]
    {
        static LOCATION: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        LOCATION
            .get_or_init(|| {
                std::env::temp_dir().join(format!(
                    "processcraft-unit-recovery-{}",
                    uuid::Uuid::new_v4()
                ))
            })
            .clone()
    }
    #[cfg(not(test))]
    {
        processcraft_io::recovery::RecoveryStore::default_location()
    }
}
