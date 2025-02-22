use std::{thread, mem, cmp};
use std::sync::{mpsc, Arc, Mutex};
use std::panic::{self, AssertUnwindSafe};
use libc::{cpu_set_t, CPU_SETSIZE, CPU_ISSET, _SC_NPROCESSORS_ONLN};

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

impl ThreadPool {
    pub fn new(size: usize) -> Self {
        assert!(size > 0, "ThreadPool size must be greater than 0");

        let (tx, rx) = mpsc::channel();
        let rx: Arc<Mutex<mpsc::Receiver<Job>>> = Arc::new(Mutex::new(rx));
        let workers: Vec<Worker> = (0..size)
            .map(|id| Worker::new(id, Arc::clone(&rx)))
            .collect();

        tracing::info!(worker_count = size, "ThreadPool created");

        Self {
            workers,
            sender: Some(tx),
        }
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job: Job = Box::new(f);
        self.sender.as_ref().unwrap().send(job).unwrap();
    }
}

impl Default for ThreadPool {
    fn default() -> Self {
        // Use the number of logical CPUs as the default size.
        Self::new(get_num_cpus())
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        drop(self.sender.take());

        self.workers.iter_mut().for_each(|worker| {
            tracing::info!(worker_id = worker.id, "Shutting down worker");

            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        });
    }
}

struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, rx: Arc<Mutex<mpsc::Receiver<Job>>>) -> Self {
        let thread: thread::JoinHandle<()> = thread::spawn(move || {
            let span = tracing::info_span!("Worker", worker_id = id);
            let _guard = span.enter();

            tracing::info!("Worker started");
            Self::worker_loop(rx);
        });

        Self {
            id,
            thread: Some(thread),
        }
    }

    fn worker_loop(rx: Arc<Mutex<mpsc::Receiver<Job>>>) {
        loop {
            let received_job: Result<Job, _> = rx.lock().unwrap().recv();

            match received_job {
                Ok(job) => {
                    tracing::debug!("Received a job. Executing.");

                    if let Err(err) = panic::catch_unwind(AssertUnwindSafe(job)) {
                        let panic_msg: &str = err
                            .downcast_ref()
                            .copied()
                            .or_else(|| err.downcast_ref::<String>().map(|s| &**s))
                            .unwrap_or("Any { .. }");

                        tracing::error!("Job panicked: {:?}", panic_msg);
                    }
                }
                Err(_) => {
                    tracing::debug!("Channel closed. Shutting down worker.");
                    break;
                }
            }
        }
    }
}

fn get_num_cpus() -> usize {
    let mut set: cpu_set_t = unsafe { mem::zeroed() };

    if unsafe { libc::sched_getaffinity(0, mem::size_of::<cpu_set_t>(), &mut set) } == 0 {
        return (0..CPU_SETSIZE as usize)
            .filter(|&i| unsafe { CPU_ISSET(i, &set) })
            .count();
    }
    let count: i64 = unsafe { libc::sysconf(_SC_NPROCESSORS_ONLN) };

    cmp::max(count, 1) as usize
}
