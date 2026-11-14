//! Background image decoding across one or more open series (for multi-series
//! comparison viewports).
//!
//! Worker threads decode slices near the ones on screen in each visible
//! viewport, closest first, so scrolling through multi-series CT/MRI studies
//! stays smooth even with compressed files.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

use crate::frame::{load_frame, Frame};

/// Identifies a specific slice of a specific series in the loaded index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SliceKey {
    pub scan_gen: u64,
    pub study: usize,
    pub series: usize,
    pub slice: usize,
}

pub struct Job {
    pub key: SliceKey,
    pub path: PathBuf,
    pub frame: u32,
    /// Decoded before anything else (e.g. the series' middle image, which sets
    /// the series' default window, or the slice currently on screen).
    pub urgent: bool,
    /// Distance in slices from a visible viewport's current slice.
    pub distance: usize,
}

struct Queue {
    generation: u64,
    jobs: Vec<Job>,
    quit: bool,
}

type Done = (u64, SliceKey, Result<Arc<Frame>, String>);

pub struct Loader {
    shared: Arc<(Mutex<Queue>, Condvar)>,
    results: Receiver<Done>,
    generation: u64,
    /// Slices queued or being decoded right now.
    pending: HashSet<SliceKey>,
}

impl Loader {
    pub fn new(repaint: impl Fn() + Send + Sync + 'static) -> Self {
        let shared = Arc::new((
            Mutex::new(Queue {
                generation: 0,
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

    /// Drops all queued work (call when rescanning a new folder).
    pub fn reset(&mut self) {
        self.generation += 1;
        let (lock, _) = &*self.shared;
        let mut q = lock.lock().unwrap();
        q.generation = self.generation;
        q.jobs.clear();
        self.pending.clear();
    }

    /// Updates distances of queued jobs from the active `(study, series, focus_slice)`
    /// viewports and drops non-urgent jobs that are now outside `radius` of all
    /// active viewports.
    pub fn update_focuses(&mut self, focuses: &[(usize, usize, usize)], radius: usize) {
        let (lock, _) = &*self.shared;
        let mut q = lock.lock().unwrap();
        let pending = &mut self.pending;
        q.jobs.retain_mut(|j| {
            let min_dist = focuses
                .iter()
                .filter(|&&(st, se, _)| st == j.key.study && se == j.key.series)
                .map(|&(_, _, sl)| j.key.slice.abs_diff(sl))
                .min();
            match min_dist {
                Some(d) if j.urgent || d <= radius => {
                    j.distance = d;
                    true
                }
                _ if j.urgent => true,
                _ => {
                    pending.remove(&j.key);
                    false
                }
            }
        });
    }

    pub fn is_pending(&self, key: SliceKey) -> bool {
        self.pending.contains(&key)
    }

    pub fn request(&mut self, job: Job) {
        if !self.pending.insert(job.key) {
            return;
        }
        let (lock, cvar) = &*self.shared;
        lock.lock().unwrap().jobs.push(job);
        cvar.notify_one();
    }

    /// Finished slices for the current scan generation.
    pub fn poll(&mut self) -> Vec<(SliceKey, Result<Arc<Frame>, String>)> {
        let mut out = Vec::new();
        while let Ok((generation, key, result)) = self.results.try_recv() {
            if generation == self.generation {
                self.pending.remove(&key);
                out.push((key, result));
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

fn worker(
    shared: Arc<(Mutex<Queue>, Condvar)>,
    tx: Sender<Done>,
    repaint: Arc<dyn Fn() + Send + Sync>,
) {
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
            let best = q
                .jobs
                .iter()
                .enumerate()
                .min_by_key(|(_, j)| (!j.urgent, j.distance))
                .map(|(i, _)| i)
                .unwrap();
            (q.generation, q.jobs.swap_remove(best))
        };
        // A decoder bug must not take the worker (or the app) down with it.
        let result = std::panic::catch_unwind(|| load_frame(&job.path, job.frame))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("the image decoder crashed on this file")))
            .map(Arc::new)
            .map_err(|e| format!("{e:#}"));
        if tx.send((generation, job.key, result)).is_err() {
            return;
        }
        repaint();
    }
}
