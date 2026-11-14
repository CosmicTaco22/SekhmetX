//! Background image decoding.
//!
//! Worker threads decode slices near the one on screen, closest first, so
//! scrolling through a large CT stays smooth even with compressed files.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

use crate::frame::{Frame, load_frame};

pub struct Job {
    pub idx: usize,
    pub path: PathBuf,
    pub frame: u32,
    /// Decoded before anything else, wherever it is (the series' middle
    /// image, which sets the series' default window).
    pub urgent: bool,
}

struct Queue {
    generation: u64,
    focus: usize,
    jobs: Vec<Job>,
    quit: bool,
}

type Done = (u64, usize, Result<Arc<Frame>, String>);

pub struct Loader {
    shared: Arc<(Mutex<Queue>, Condvar)>,
    results: Receiver<Done>,
    generation: u64,
    /// Slices queued or being decoded right now.
    pending: HashSet<usize>,
}

impl Loader {
    pub fn new(repaint: impl Fn() + Send + Sync + 'static) -> Self {
        let shared = Arc::new((
            Mutex::new(Queue {
                generation: 0,
                focus: 0,
                jobs: Vec::new(),
                quit: false,
            }),
            Condvar::new(),
        ));
        let (tx, rx) = channel::<Done>();
        let repaint = Arc::new(repaint);
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1))
            .unwrap_or(2)
            .clamp(2, 6);
        for _ in 0..workers {
            let shared = shared.clone();
            let tx: Sender<Done> = tx.clone();
            let repaint = repaint.clone();
            std::thread::spawn(move || worker(shared, tx, repaint));
        }
        Self {
            shared,
            results: rx,
            generation: 0,
            pending: HashSet::new(),
        }
    }

    /// Drops all queued work (call when switching series).
    pub fn reset(&mut self) {
        self.generation += 1;
        let (lock, _) = &*self.shared;
        let mut q = lock.lock().unwrap();
        q.generation = self.generation;
        q.jobs.clear();
        self.pending.clear();
    }

    /// Tells the workers which slice is on screen, and forgets queued slices
    /// that are now too far away.
    pub fn set_focus(&mut self, focus: usize, radius: usize) {
        let (lock, _) = &*self.shared;
        let mut q = lock.lock().unwrap();
        q.focus = focus;
        let pending = &mut self.pending;
        q.jobs.retain(|j| {
            let keep = j.urgent || j.idx.abs_diff(focus) <= radius;
            if !keep {
                pending.remove(&j.idx);
            }
            keep
        });
    }

    pub fn is_pending(&self, idx: usize) -> bool {
        self.pending.contains(&idx)
    }

    pub fn request(&mut self, job: Job) {
        if !self.pending.insert(job.idx) {
            return;
        }
        let (lock, cvar) = &*self.shared;
        lock.lock().unwrap().jobs.push(job);
        cvar.notify_one();
    }

    /// Finished slices for the current series.
    pub fn poll(&mut self) -> Vec<(usize, Result<Arc<Frame>, String>)> {
        let mut out = Vec::new();
        while let Ok((generation, idx, result)) = self.results.try_recv() {
            if generation == self.generation {
                self.pending.remove(&idx);
                out.push((idx, result));
            }
        }
        out
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.shared;
        lock.lock().unwrap().quit = true;
        cvar.notify_all();
    }
}

fn worker(shared: Arc<(Mutex<Queue>, Condvar)>, tx: Sender<Done>, repaint: Arc<dyn Fn() + Send + Sync>) {
    let (lock, cvar) = &*shared;
    loop {
        let (generation, job) = {
            let mut q = lock.lock().unwrap();
            while q.jobs.is_empty() && !q.quit {
                q = cvar.wait(q).unwrap();
            }
            if q.quit {
                return;
            }
            let focus = q.focus;
            let best = q
                .jobs
                .iter()
                .enumerate()
                .min_by_key(|(_, j)| (!j.urgent, j.idx.abs_diff(focus)))
                .map(|(i, _)| i)
                .unwrap();
            (q.generation, q.jobs.swap_remove(best))
        };
        // A decoder bug must not take the worker (or the app) down with it.
        let result = std::panic::catch_unwind(|| load_frame(&job.path, job.frame))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("the image decoder crashed on this file")))
            .map(Arc::new)
            .map_err(|e| format!("{e:#}"));
        if tx.send((generation, job.idx, result)).is_err() {
            return;
        }
        repaint();
    }
}
